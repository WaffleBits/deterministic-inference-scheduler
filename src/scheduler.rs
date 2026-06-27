use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque};
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

const MAX_REPLAY_TICKS: u64 = 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulerConfig {
    pub max_active_requests: usize,
    pub decode_batch_size: usize,
    pub max_prefill_tokens_per_tick: u32,
    pub kv_page_tokens: u32,
    pub total_kv_pages: u32,
}

impl SchedulerConfig {
    fn validate(&self) -> Result<(), RuntimeError> {
        if self.max_active_requests == 0 {
            return Err(RuntimeError::InvalidConfig(
                "max_active_requests must be greater than zero".into(),
            ));
        }
        if self.decode_batch_size == 0 {
            return Err(RuntimeError::InvalidConfig(
                "decode_batch_size must be greater than zero".into(),
            ));
        }
        if self.max_prefill_tokens_per_tick == 0 {
            return Err(RuntimeError::InvalidConfig(
                "max_prefill_tokens_per_tick must be greater than zero".into(),
            ));
        }
        if self.kv_page_tokens == 0 || self.total_kv_pages == 0 {
            return Err(RuntimeError::InvalidConfig(
                "KV page size and capacity must be greater than zero".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestSpec {
    pub id: String,
    pub prompt_tokens: u32,
    pub max_output_tokens: u32,
    #[serde(default)]
    pub priority: u8,
    #[serde(default)]
    pub arrival_tick: u64,
}

impl RequestSpec {
    fn validate(&self, config: &SchedulerConfig) -> Result<(), RuntimeError> {
        if self.id.trim().is_empty() {
            return Err(RuntimeError::InvalidRequest(
                "request id must not be empty".into(),
            ));
        }
        if self.prompt_tokens == 0 || self.max_output_tokens == 0 {
            return Err(RuntimeError::InvalidRequest(format!(
                "{} must have positive prompt and output token counts",
                self.id
            )));
        }
        if self.reserved_pages(config.kv_page_tokens) > config.total_kv_pages {
            return Err(RuntimeError::InvalidRequest(format!(
                "{} requires more KV pages than the runtime owns",
                self.id
            )));
        }
        Ok(())
    }

    fn reserved_pages(&self, page_tokens: u32) -> u32 {
        self.prompt_tokens
            .saturating_add(self.max_output_tokens)
            .div_ceil(page_tokens)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct QueuedRequest {
    spec: RequestSpec,
    sequence: u64,
}

impl Ord for QueuedRequest {
    fn cmp(&self, other: &Self) -> Ordering {
        self.spec
            .priority
            .cmp(&other.spec.priority)
            .then_with(|| other.sequence.cmp(&self.sequence))
            .then_with(|| other.spec.id.cmp(&self.spec.id))
    }
}

impl PartialOrd for QueuedRequest {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActiveRequest {
    spec: RequestSpec,
    generated_tokens: u32,
    reserved_pages: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TickTrace {
    pub tick: u64,
    pub admitted: Vec<String>,
    pub admitted_prefill_tokens: u32,
    pub decoded: Vec<String>,
    pub decoded_tokens: u32,
    pub completed: Vec<String>,
    pub queued_requests: usize,
    pub active_requests: usize,
    pub used_kv_pages: u32,
}

#[derive(Debug)]
pub struct Scheduler {
    config: SchedulerConfig,
    queue: BinaryHeap<QueuedRequest>,
    active: BTreeMap<String, ActiveRequest>,
    decode_order: VecDeque<String>,
    known_ids: BTreeSet<String>,
    completed_ids: BTreeSet<String>,
    used_kv_pages: u32,
    sequence: u64,
    tick: u64,
}

impl Scheduler {
    pub fn new(config: SchedulerConfig) -> Result<Self, RuntimeError> {
        config.validate()?;
        Ok(Self {
            config,
            queue: BinaryHeap::new(),
            active: BTreeMap::new(),
            decode_order: VecDeque::new(),
            known_ids: BTreeSet::new(),
            completed_ids: BTreeSet::new(),
            used_kv_pages: 0,
            sequence: 0,
            tick: 0,
        })
    }

    pub fn current_tick(&self) -> u64 {
        self.tick
    }

    pub fn submit(&mut self, spec: RequestSpec) -> Result<(), RuntimeError> {
        spec.validate(&self.config)?;
        if !self.known_ids.insert(spec.id.clone()) {
            return Err(RuntimeError::DuplicateRequest(spec.id));
        }
        self.queue.push(QueuedRequest {
            spec,
            sequence: self.sequence,
        });
        self.sequence += 1;
        Ok(())
    }

    pub fn step(&mut self) -> TickTrace {
        let mut trace = TickTrace {
            tick: self.tick,
            admitted: Vec::new(),
            admitted_prefill_tokens: 0,
            decoded: Vec::new(),
            decoded_tokens: 0,
            completed: Vec::new(),
            queued_requests: 0,
            active_requests: 0,
            used_kv_pages: 0,
        };

        trace.admitted_prefill_tokens = self.admit_requests(&mut trace.admitted);
        trace.decoded_tokens = self.decode_requests(&mut trace.decoded, &mut trace.completed);

        trace.queued_requests = self.queue.len();
        trace.active_requests = self.active.len();
        trace.used_kv_pages = self.used_kv_pages;
        self.tick += 1;
        trace
    }

    pub fn is_idle(&self) -> bool {
        self.queue.is_empty() && self.active.is_empty()
    }

    pub fn completed_count(&self) -> usize {
        self.completed_ids.len()
    }

    fn admit_requests(&mut self, admitted: &mut Vec<String>) -> u32 {
        let mut remaining_prefill = self.config.max_prefill_tokens_per_tick;
        let mut deferred = Vec::new();
        let candidates = self.queue.len();
        let mut admitted_prefill_tokens = 0;

        for _ in 0..candidates {
            if self.active.len() >= self.config.max_active_requests {
                break;
            }

            let Some(candidate) = self.queue.pop() else {
                break;
            };
            let pages = candidate.spec.reserved_pages(self.config.kv_page_tokens);
            let pages_fit = self.used_kv_pages.saturating_add(pages) <= self.config.total_kv_pages;
            let prefill_fits = candidate.spec.prompt_tokens <= remaining_prefill
                || (admitted.is_empty()
                    && remaining_prefill == self.config.max_prefill_tokens_per_tick);

            if pages_fit && prefill_fits {
                remaining_prefill = remaining_prefill.saturating_sub(candidate.spec.prompt_tokens);
                admitted_prefill_tokens += candidate.spec.prompt_tokens;
                self.used_kv_pages += pages;
                let id = candidate.spec.id.clone();
                self.active.insert(
                    id.clone(),
                    ActiveRequest {
                        spec: candidate.spec,
                        generated_tokens: 0,
                        reserved_pages: pages,
                    },
                );
                self.decode_order.push_back(id.clone());
                admitted.push(id);
            } else {
                deferred.push(candidate);
            }
        }

        self.queue.extend(deferred);
        admitted_prefill_tokens
    }

    fn decode_requests(&mut self, decoded: &mut Vec<String>, completed: &mut Vec<String>) -> u32 {
        let batch_size = self.config.decode_batch_size.min(self.decode_order.len());

        for _ in 0..batch_size {
            let Some(id) = self.decode_order.pop_front() else {
                break;
            };
            let (is_complete, reserved_pages) = {
                let request = self
                    .active
                    .get_mut(&id)
                    .expect("decode order must reference an active request");
                request.generated_tokens += 1;
                (
                    request.generated_tokens >= request.spec.max_output_tokens,
                    request.reserved_pages,
                )
            };

            decoded.push(id.clone());
            if is_complete {
                self.active.remove(&id);
                self.used_kv_pages -= reserved_pages;
                self.completed_ids.insert(id.clone());
                completed.push(id);
            } else {
                self.decode_order.push_back(id);
            }
        }

        decoded.len() as u32
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayInput {
    pub config: SchedulerConfig,
    pub requests: Vec<RequestSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplayReport {
    pub schema_version: u32,
    pub config: SchedulerConfig,
    pub request_count: usize,
    pub completed_requests: usize,
    pub total_prompt_tokens: u64,
    pub total_decode_tokens: u64,
    pub total_reserved_kv_pages: u64,
    pub total_ticks: u64,
    pub max_queued_requests: usize,
    pub max_active_requests: usize,
    pub peak_kv_pages: u32,
    pub peak_kv_pressure_pct: f64,
    pub ticks_with_queue_pressure: u64,
    pub ticks_at_active_capacity: u64,
    pub declared_prefill_capacity_tokens: u64,
    pub declared_decode_capacity_tokens: u64,
    pub kv_page_capacity_ticks: u64,
    pub queue_pressure_ratio: f64,
    pub active_capacity_ratio: f64,
    pub prefill_capacity_utilization: f64,
    pub decode_capacity_utilization: f64,
    pub kv_page_occupancy_ratio: f64,
    pub final_kv_pages: u32,
    pub trace_fingerprint: String,
    pub ticks: Vec<TickTrace>,
}

pub fn run_replay(input: ReplayInput) -> Result<ReplayReport, RuntimeError> {
    let mut scheduler = Scheduler::new(input.config.clone())?;
    let request_count = input.requests.len();
    let mut pending: Vec<(usize, RequestSpec)> = input.requests.into_iter().enumerate().collect();
    let total_prompt_tokens = pending
        .iter()
        .map(|(_, request)| u64::from(request.prompt_tokens))
        .sum();
    let total_decode_tokens = pending
        .iter()
        .map(|(_, request)| u64::from(request.max_output_tokens))
        .sum();
    let total_reserved_kv_pages = pending
        .iter()
        .map(|(_, request)| u64::from(request.reserved_pages(input.config.kv_page_tokens)))
        .sum();
    pending.sort_by(|(left_index, left), (right_index, right)| {
        left.arrival_tick
            .cmp(&right.arrival_tick)
            .then_with(|| left_index.cmp(right_index))
    });

    let mut next_request = 0;
    let mut ticks = Vec::new();
    while next_request < pending.len() || !scheduler.is_idle() {
        if scheduler.current_tick() >= MAX_REPLAY_TICKS {
            return Err(RuntimeError::ReplayLimit(MAX_REPLAY_TICKS));
        }

        while next_request < pending.len()
            && pending[next_request].1.arrival_tick <= scheduler.current_tick()
        {
            scheduler.submit(pending[next_request].1.clone())?;
            next_request += 1;
        }
        ticks.push(scheduler.step());
    }

    let peak_kv_pages = ticks
        .iter()
        .map(|tick| tick.used_kv_pages)
        .max()
        .unwrap_or(0);
    let max_queued_requests = ticks
        .iter()
        .map(|tick| tick.queued_requests)
        .max()
        .unwrap_or(0);
    let max_active_requests = ticks
        .iter()
        .map(|tick| tick.active_requests)
        .max()
        .unwrap_or(0);
    let ticks_with_queue_pressure =
        ticks.iter().filter(|tick| tick.queued_requests > 0).count() as u64;
    let max_active_capacity = input.config.max_active_requests;
    let total_kv_pages = input.config.total_kv_pages;
    let ticks_at_active_capacity = ticks
        .iter()
        .filter(|tick| tick.active_requests >= max_active_capacity)
        .count() as u64;
    let total_ticks = scheduler.current_tick();
    let declared_prefill_capacity_tokens =
        total_ticks.saturating_mul(u64::from(input.config.max_prefill_tokens_per_tick));
    let declared_decode_capacity_tokens =
        total_ticks.saturating_mul(input.config.decode_batch_size as u64);
    let kv_page_capacity_ticks = total_ticks.saturating_mul(u64::from(total_kv_pages));
    let occupied_kv_page_ticks = ticks.iter().map(|tick| u64::from(tick.used_kv_pages)).sum();
    let trace_bytes = serde_json::to_vec(&ticks)
        .map_err(|error| RuntimeError::Serialization(error.to_string()))?;

    Ok(ReplayReport {
        schema_version: 3,
        config: input.config,
        request_count,
        completed_requests: scheduler.completed_count(),
        total_prompt_tokens,
        total_decode_tokens,
        total_reserved_kv_pages,
        total_ticks,
        max_queued_requests,
        max_active_requests,
        peak_kv_pages,
        peak_kv_pressure_pct: pressure_pct(peak_kv_pages, total_kv_pages),
        ticks_with_queue_pressure,
        ticks_at_active_capacity,
        declared_prefill_capacity_tokens,
        declared_decode_capacity_tokens,
        kv_page_capacity_ticks,
        queue_pressure_ratio: ratio(ticks_with_queue_pressure, total_ticks),
        active_capacity_ratio: ratio(ticks_at_active_capacity, total_ticks),
        prefill_capacity_utilization: ratio(total_prompt_tokens, declared_prefill_capacity_tokens),
        decode_capacity_utilization: ratio(total_decode_tokens, declared_decode_capacity_tokens),
        kv_page_occupancy_ratio: ratio(occupied_kv_page_ticks, kv_page_capacity_ticks),
        final_kv_pages: 0,
        trace_fingerprint: fnv1a64_hex(&trace_bytes),
        ticks,
    })
}

fn fnv1a64_hex(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn pressure_pct(used: u32, capacity: u32) -> f64 {
    if capacity == 0 {
        0.0
    } else {
        round6((f64::from(used) / f64::from(capacity)) * 100.0)
    }
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        round6(numerator as f64 / denominator as f64)
    }
}

fn round6(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    InvalidConfig(String),
    InvalidRequest(String),
    DuplicateRequest(String),
    ReplayLimit(u64),
    Serialization(String),
}

impl Display for RuntimeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(message) => {
                write!(formatter, "invalid scheduler config: {message}")
            }
            Self::InvalidRequest(message) => write!(formatter, "invalid request: {message}"),
            Self::DuplicateRequest(id) => write!(formatter, "duplicate request id: {id}"),
            Self::ReplayLimit(limit) => write!(formatter, "replay exceeded {limit} ticks"),
            Self::Serialization(message) => write!(formatter, "serialization failed: {message}"),
        }
    }
}

impl Error for RuntimeError {}
