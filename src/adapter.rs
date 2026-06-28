use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::release::{GateInput, GateThresholds, Observation, OperationalTelemetry};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackendMirrorInput {
    #[serde(default)]
    pub thresholds: GateThresholds,
    pub baseline: BackendObservationSet,
    pub candidate: BackendObservationSet,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackendObservationSet {
    pub backend: String,
    pub model: String,
    #[serde(default)]
    pub accelerator: Option<String>,
    pub observations: Vec<BackendObservation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackendObservation {
    pub request_id: String,
    pub latency_ms: f64,
    #[serde(default)]
    pub ok: Option<bool>,
    #[serde(default)]
    pub output_fingerprint: Option<String>,
    #[serde(default)]
    pub output_token_ids: Vec<i64>,
    #[serde(default)]
    pub output_values: Option<Vec<f64>>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub model_version: Option<String>,
    #[serde(default)]
    pub route_id: Option<String>,
    #[serde(default)]
    pub replica_id: Option<String>,
    #[serde(default)]
    pub scheduler_policy: Option<String>,
    #[serde(default)]
    pub streaming_token_events: Vec<StreamingTokenEvent>,
    #[serde(default)]
    pub queue_depth: Option<usize>,
    #[serde(default)]
    pub kv_pages_used: Option<usize>,
    #[serde(default)]
    pub kv_pages_capacity: Option<usize>,
    #[serde(default)]
    pub time_to_first_token_ms: Option<f64>,
    #[serde(default)]
    pub decode_token_latencies_ms: Vec<f64>,
    #[serde(default)]
    pub token_trace_fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamingTokenEvent {
    pub sequence: usize,
    pub token_id: i64,
    pub elapsed_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterError {
    message: String,
}

impl AdapterError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for AdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for AdapterError {}

pub fn mirror_to_gate_input(input: &BackendMirrorInput) -> Result<GateInput, AdapterError> {
    Ok(GateInput {
        thresholds: input.thresholds.clone(),
        baseline: normalize_set(&input.baseline)?,
        candidate: normalize_set(&input.candidate)?,
    })
}

fn normalize_set(set: &BackendObservationSet) -> Result<Vec<Observation>, AdapterError> {
    if set.backend.trim().is_empty() {
        return Err(AdapterError::new("backend must not be empty"));
    }
    if set.model.trim().is_empty() {
        return Err(AdapterError::new("model must not be empty"));
    }

    set.observations
        .iter()
        .map(|observation| normalize_observation(set, observation))
        .collect()
}

fn normalize_observation(
    set: &BackendObservationSet,
    observation: &BackendObservation,
) -> Result<Observation, AdapterError> {
    if observation.request_id.trim().is_empty() {
        return Err(AdapterError::new("request_id must not be empty"));
    }
    if !observation.latency_ms.is_finite() || observation.latency_ms < 0.0 {
        return Err(AdapterError::new(format!(
            "request {} has invalid latency_ms",
            observation.request_id
        )));
    }
    validate_operational_telemetry(observation)?;

    let ok = observation.ok.unwrap_or_else(|| {
        observation
            .error
            .as_ref()
            .is_none_or(|error| error.trim().is_empty())
    });
    let output_fingerprint = output_fingerprint(observation, ok)?;

    Ok(Observation {
        request_id: observation.request_id.clone(),
        output_fingerprint,
        latency_ms: observation.latency_ms,
        ok,
        model: Some(set.model.clone()),
        backend: Some(set.backend.clone()),
        accelerator: set.accelerator.clone(),
        output_values: observation.output_values.clone(),
        telemetry: operational_telemetry(observation),
    })
}

fn validate_operational_telemetry(observation: &BackendObservation) -> Result<(), AdapterError> {
    if let Some(ttft) = observation.time_to_first_token_ms
        && (!ttft.is_finite() || ttft < 0.0)
    {
        return Err(AdapterError::new(format!(
            "request {} has invalid time_to_first_token_ms",
            observation.request_id
        )));
    }

    for latency in &observation.decode_token_latencies_ms {
        if !latency.is_finite() || *latency < 0.0 {
            return Err(AdapterError::new(format!(
                "request {} has invalid decode_token_latencies_ms",
                observation.request_id
            )));
        }
    }
    validate_streaming_token_events(observation)?;

    match (observation.kv_pages_used, observation.kv_pages_capacity) {
        (Some(_), Some(0)) => Err(AdapterError::new(format!(
            "request {} has kv_pages_capacity of zero",
            observation.request_id
        ))),
        (Some(used), Some(capacity)) if used > capacity => Err(AdapterError::new(format!(
            "request {} has kv_pages_used greater than kv_pages_capacity",
            observation.request_id
        ))),
        _ => Ok(()),
    }
}

fn validate_streaming_token_events(observation: &BackendObservation) -> Result<(), AdapterError> {
    let mut last_sequence = None;
    let mut last_elapsed_ms = None;
    for event in &observation.streaming_token_events {
        if !event.elapsed_ms.is_finite() || event.elapsed_ms < 0.0 {
            return Err(AdapterError::new(format!(
                "request {} has invalid streaming_token_events elapsed_ms",
                observation.request_id
            )));
        }
        if let Some(last_sequence) = last_sequence
            && event.sequence <= last_sequence
        {
            return Err(AdapterError::new(format!(
                "request {} has non-increasing streaming token sequence",
                observation.request_id
            )));
        }
        if let Some(last_elapsed_ms) = last_elapsed_ms
            && event.elapsed_ms < last_elapsed_ms
        {
            return Err(AdapterError::new(format!(
                "request {} has decreasing streaming token elapsed_ms",
                observation.request_id
            )));
        }
        last_sequence = Some(event.sequence);
        last_elapsed_ms = Some(event.elapsed_ms);
    }
    Ok(())
}

fn operational_telemetry(observation: &BackendObservation) -> OperationalTelemetry {
    OperationalTelemetry {
        model_version: observation.model_version.clone(),
        route_id: normalized_label(&observation.route_id),
        replica_id: normalized_label(&observation.replica_id),
        scheduler_policy: normalized_label(&observation.scheduler_policy),
        streamed_token_count: observation.streaming_token_events.len(),
        queue_depth: observation.queue_depth,
        kv_pages_used: observation.kv_pages_used,
        kv_pages_capacity: observation.kv_pages_capacity,
        time_to_first_token_ms: observation
            .time_to_first_token_ms
            .or_else(|| first_stream_token_ms(observation)),
        decode_token_latencies_ms: if observation.decode_token_latencies_ms.is_empty() {
            decode_latencies_from_stream(observation)
        } else {
            observation.decode_token_latencies_ms.clone()
        },
        token_trace_fingerprint: token_trace_fingerprint(observation),
    }
}

fn token_trace_fingerprint(observation: &BackendObservation) -> Option<String> {
    if let Some(fingerprint) = observation.token_trace_fingerprint.as_ref()
        && !fingerprint.trim().is_empty()
    {
        return Some(fingerprint.clone());
    }

    let token_ids = observed_token_ids(observation);
    if token_ids.is_empty() {
        None
    } else {
        Some(format!(
            "token-trace-fnv64:{:016x}",
            hash_i64_values(&token_ids)
        ))
    }
}

fn output_fingerprint(observation: &BackendObservation, ok: bool) -> Result<String, AdapterError> {
    if let Some(fingerprint) = observation.output_fingerprint.as_ref()
        && !fingerprint.trim().is_empty()
    {
        return Ok(fingerprint.clone());
    }

    let token_ids = observed_token_ids(observation);
    if !token_ids.is_empty() {
        return Ok(format!("tokens-fnv64:{:016x}", hash_i64_values(&token_ids)));
    }

    if let Some(values) = observation.output_values.as_ref()
        && !values.is_empty()
    {
        return Ok(format!("values-fnv64:{:016x}", hash_f64_values(values)));
    }

    if !ok {
        return Ok("error".into());
    }

    Err(AdapterError::new(format!(
        "request {} is successful but has no output fingerprint, token ids, or numeric values",
        observation.request_id
    )))
}

fn observed_token_ids(observation: &BackendObservation) -> Vec<i64> {
    if !observation.output_token_ids.is_empty() {
        observation.output_token_ids.clone()
    } else {
        observation
            .streaming_token_events
            .iter()
            .map(|event| event.token_id)
            .collect()
    }
}

fn first_stream_token_ms(observation: &BackendObservation) -> Option<f64> {
    observation
        .streaming_token_events
        .first()
        .map(|event| event.elapsed_ms)
}

fn decode_latencies_from_stream(observation: &BackendObservation) -> Vec<f64> {
    observation
        .streaming_token_events
        .windows(2)
        .map(|events| round6(events[1].elapsed_ms - events[0].elapsed_ms))
        .collect()
}

fn normalized_label(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn round6(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn hash_i64_values(values: &[i64]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    feed_usize(&mut hash, values.len());
    for value in values {
        feed_bytes(&mut hash, &value.to_le_bytes());
    }
    hash
}

fn hash_f64_values(values: &[f64]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    feed_usize(&mut hash, values.len());
    for value in values {
        feed_bytes(&mut hash, &value.to_bits().to_le_bytes());
    }
    hash
}

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn feed_usize(hash: &mut u64, value: usize) {
    feed_bytes(hash, &value.to_le_bytes());
}

fn feed_bytes(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(FNV_PRIME);
    }
}
