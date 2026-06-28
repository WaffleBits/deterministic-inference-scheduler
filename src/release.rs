use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    pub request_id: String,
    pub output_fingerprint: String,
    pub latency_ms: f64,
    pub ok: bool,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    pub accelerator: Option<String>,
    #[serde(default)]
    pub output_values: Option<Vec<f64>>,
    #[serde(default)]
    pub telemetry: OperationalTelemetry,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct OperationalTelemetry {
    #[serde(default)]
    pub model_version: Option<String>,
    #[serde(default)]
    pub route_id: Option<String>,
    #[serde(default)]
    pub replica_id: Option<String>,
    #[serde(default)]
    pub scheduler_policy: Option<String>,
    #[serde(default)]
    pub streamed_token_count: usize,
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

impl OperationalTelemetry {
    fn memory_pressure_pct(&self) -> Option<f64> {
        match (self.kv_pages_used, self.kv_pages_capacity) {
            (Some(used), Some(capacity)) if capacity > 0 => {
                Some(round6((used as f64 / capacity as f64) * 100.0))
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NumericTolerance {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub candidate_backend: Option<String>,
    #[serde(default)]
    pub candidate_accelerator: Option<String>,
    pub max_abs_error: f64,
    pub max_rel_error: f64,
}

impl NumericTolerance {
    fn matches(&self, baseline: &Observation, candidate: &Observation) -> bool {
        if !same_optional_value(&baseline.model, &candidate.model) {
            return false;
        }

        let model = candidate.model.as_ref().or(baseline.model.as_ref());
        if !matches_filter(&self.model, model) {
            return false;
        }
        if !matches_filter(&self.candidate_backend, candidate.backend.as_ref()) {
            return false;
        }
        if !matches_filter(&self.candidate_accelerator, candidate.accelerator.as_ref()) {
            return false;
        }

        self.max_abs_error >= 0.0 && self.max_rel_error >= 0.0
    }

    fn specificity(&self) -> usize {
        usize::from(self.model.is_some())
            + usize::from(self.candidate_backend.is_some())
            + usize::from(self.candidate_accelerator.is_some())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GateThresholds {
    pub min_matched_requests: usize,
    pub max_output_mismatch_rate: f64,
    pub max_error_rate_increase: f64,
    pub max_p95_latency_regression_pct: f64,
    #[serde(default = "default_max_ttft_regression_pct")]
    pub max_ttft_regression_pct: f64,
    #[serde(default = "default_max_decode_token_p95_regression_pct")]
    pub max_decode_token_p95_regression_pct: f64,
    #[serde(default = "default_max_candidate_memory_pressure_pct")]
    pub max_candidate_memory_pressure_pct: f64,
    #[serde(default)]
    pub max_numeric_drift_rate: f64,
    #[serde(default)]
    pub numeric_tolerances: Vec<NumericTolerance>,
    #[serde(default)]
    pub require_candidate_route_provenance: bool,
    #[serde(default)]
    pub require_streaming_token_trace: bool,
}

impl Default for GateThresholds {
    fn default() -> Self {
        Self {
            min_matched_requests: 3,
            max_output_mismatch_rate: 0.0,
            max_error_rate_increase: 0.01,
            max_p95_latency_regression_pct: 10.0,
            max_ttft_regression_pct: default_max_ttft_regression_pct(),
            max_decode_token_p95_regression_pct: default_max_decode_token_p95_regression_pct(),
            max_candidate_memory_pressure_pct: default_max_candidate_memory_pressure_pct(),
            max_numeric_drift_rate: 0.0,
            numeric_tolerances: Vec::new(),
            require_candidate_route_provenance: false,
            require_streaming_token_trace: false,
        }
    }
}

fn default_max_ttft_regression_pct() -> f64 {
    10.0
}

fn default_max_decode_token_p95_regression_pct() -> f64 {
    10.0
}

fn default_max_candidate_memory_pressure_pct() -> f64 {
    95.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GateInput {
    #[serde(default)]
    pub thresholds: GateThresholds,
    pub baseline: Vec<Observation>,
    pub candidate: Vec<Observation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateDecision {
    Promote,
    Hold,
    Rollback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriageItem {
    pub signal: String,
    pub response: GateDecision,
    pub owner_hint: String,
    pub action: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SegmentReport {
    pub model: String,
    pub baseline_backend: String,
    pub candidate_backend: String,
    pub accelerator: String,
    pub baseline_model_version: String,
    pub candidate_model_version: String,
    pub matched_requests: usize,
    pub output_mismatch_rate: f64,
    pub baseline_error_rate: f64,
    pub candidate_error_rate: f64,
    pub baseline_p95_latency_ms: Option<f64>,
    pub candidate_p95_latency_ms: Option<f64>,
    pub p95_latency_regression_pct: Option<f64>,
    pub baseline_p95_ttft_ms: Option<f64>,
    pub candidate_p95_ttft_ms: Option<f64>,
    pub ttft_regression_pct: Option<f64>,
    pub baseline_decode_token_p95_ms: Option<f64>,
    pub candidate_decode_token_p95_ms: Option<f64>,
    pub decode_token_p95_regression_pct: Option<f64>,
    pub max_candidate_queue_depth: Option<usize>,
    pub max_candidate_memory_pressure_pct: Option<f64>,
    pub candidate_routing_provenance_rate: f64,
    pub candidate_streaming_trace_rate: f64,
    pub candidate_route_count: usize,
    pub candidate_scheduler_policies: Vec<String>,
    pub token_trace_pairs: usize,
    pub token_trace_mismatch_rate: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GateReport {
    pub schema_version: u32,
    pub decision: GateDecision,
    pub matched_requests: usize,
    pub baseline_requests: usize,
    pub candidate_requests: usize,
    pub coverage_rate: f64,
    pub output_mismatch_rate: f64,
    pub numeric_pairs: usize,
    pub tolerated_numeric_outputs: usize,
    pub numeric_drift_rate: f64,
    pub max_numeric_abs_error: Option<f64>,
    pub max_numeric_rel_error: Option<f64>,
    pub baseline_error_rate: f64,
    pub candidate_error_rate: f64,
    pub error_rate_increase: f64,
    pub baseline_p95_latency_ms: Option<f64>,
    pub candidate_p95_latency_ms: Option<f64>,
    pub p95_latency_regression_pct: Option<f64>,
    pub baseline_p95_ttft_ms: Option<f64>,
    pub candidate_p95_ttft_ms: Option<f64>,
    pub ttft_regression_pct: Option<f64>,
    pub baseline_decode_token_p95_ms: Option<f64>,
    pub candidate_decode_token_p95_ms: Option<f64>,
    pub decode_token_p95_regression_pct: Option<f64>,
    pub max_candidate_queue_depth: Option<usize>,
    pub max_candidate_memory_pressure_pct: Option<f64>,
    pub candidate_routing_provenance_rate: f64,
    pub candidate_streaming_trace_rate: f64,
    pub candidate_route_count: usize,
    pub candidate_scheduler_policies: Vec<String>,
    pub token_trace_pairs: usize,
    pub token_trace_mismatch_rate: f64,
    pub segments: Vec<SegmentReport>,
    pub triage: Vec<TriageItem>,
    pub reasons: Vec<String>,
}

pub fn evaluate_release(input: &GateInput) -> GateReport {
    let baseline_by_id: BTreeMap<&str, &Observation> = input
        .baseline
        .iter()
        .map(|observation| (observation.request_id.as_str(), observation))
        .collect();
    let candidate_by_id: BTreeMap<&str, &Observation> = input
        .candidate
        .iter()
        .map(|observation| (observation.request_id.as_str(), observation))
        .collect();

    let matched: Vec<(&Observation, &Observation)> = baseline_by_id
        .iter()
        .filter_map(|(request_id, baseline)| {
            candidate_by_id
                .get(request_id)
                .map(|candidate| (*baseline, *candidate))
        })
        .collect();

    let matched_requests = matched.len();
    let baseline_requests = input.baseline.len();
    let candidate_requests = input.candidate.len();
    let coverage_rate = ratio(matched_requests, baseline_requests.max(candidate_requests));

    let baseline_errors = matched.iter().filter(|(baseline, _)| !baseline.ok).count();
    let candidate_errors = matched
        .iter()
        .filter(|(_, candidate)| !candidate.ok)
        .count();
    let baseline_error_rate = ratio(baseline_errors, matched_requests);
    let candidate_error_rate = ratio(candidate_errors, matched_requests);
    let error_rate_increase = round6(candidate_error_rate - baseline_error_rate);

    let comparable_outputs: Vec<(&Observation, &Observation)> = matched
        .iter()
        .copied()
        .filter(|(baseline, candidate)| baseline.ok && candidate.ok)
        .collect();
    let matched_comparisons: Vec<Option<OutputComparison>> = matched
        .iter()
        .map(|(baseline, candidate)| {
            if baseline.ok && candidate.ok {
                Some(compare_outputs(baseline, candidate, &input.thresholds))
            } else {
                None
            }
        })
        .collect();
    let comparisons: Vec<OutputComparison> =
        matched_comparisons.iter().flatten().copied().collect();
    let mismatches = comparisons
        .iter()
        .filter(|comparison| !comparison.matches)
        .count();
    let output_mismatch_rate = ratio(mismatches, comparable_outputs.len());
    let numeric_pairs = comparisons
        .iter()
        .filter(|comparison| comparison.used_numeric)
        .count();
    let tolerated_numeric_outputs = comparisons
        .iter()
        .filter(|comparison| comparison.tolerated_numeric)
        .count();
    let numeric_drifts = comparisons
        .iter()
        .filter(|comparison| comparison.numeric_drift)
        .count();
    let numeric_drift_rate = ratio(numeric_drifts, numeric_pairs);
    let max_numeric_abs_error = max_comparison_value(comparisons.iter().map(|comparison| {
        if comparison.used_numeric {
            Some(comparison.max_abs_error)
        } else {
            None
        }
    }));
    let max_numeric_rel_error = max_comparison_value(comparisons.iter().map(|comparison| {
        if comparison.used_numeric {
            Some(comparison.max_rel_error)
        } else {
            None
        }
    }));

    let baseline_p95_latency_ms = percentile95(
        matched
            .iter()
            .filter(|(baseline, _)| baseline.ok)
            .map(|(baseline, _)| baseline.latency_ms),
    );
    let candidate_p95_latency_ms = percentile95(
        matched
            .iter()
            .filter(|(_, candidate)| candidate.ok)
            .map(|(_, candidate)| candidate.latency_ms),
    );
    let p95_latency_regression_pct =
        latency_regression_pct(baseline_p95_latency_ms, candidate_p95_latency_ms);
    let baseline_p95_ttft_ms = percentile95(
        matched
            .iter()
            .filter(|(baseline, _)| baseline.ok)
            .filter_map(|(baseline, _)| baseline.telemetry.time_to_first_token_ms),
    );
    let candidate_p95_ttft_ms = percentile95(
        matched
            .iter()
            .filter(|(_, candidate)| candidate.ok)
            .filter_map(|(_, candidate)| candidate.telemetry.time_to_first_token_ms),
    );
    let ttft_regression_pct = latency_regression_pct(baseline_p95_ttft_ms, candidate_p95_ttft_ms);
    let baseline_decode_token_p95_ms =
        percentile95(matched.iter().filter(|(baseline, _)| baseline.ok).flat_map(
            |(baseline, _)| baseline.telemetry.decode_token_latencies_ms.iter().copied(),
        ));
    let candidate_decode_token_p95_ms = percentile95(
        matched
            .iter()
            .filter(|(_, candidate)| candidate.ok)
            .flat_map(|(_, candidate)| {
                candidate
                    .telemetry
                    .decode_token_latencies_ms
                    .iter()
                    .copied()
            }),
    );
    let decode_token_p95_regression_pct =
        latency_regression_pct(baseline_decode_token_p95_ms, candidate_decode_token_p95_ms);
    let max_candidate_queue_depth = matched
        .iter()
        .filter_map(|(_, candidate)| candidate.telemetry.queue_depth)
        .max();
    let max_candidate_memory_pressure_pct = max_comparison_value(
        matched
            .iter()
            .filter_map(|(_, candidate)| candidate.telemetry.memory_pressure_pct())
            .map(Some),
    );
    let successful_candidate_requests =
        matched.iter().filter(|(_, candidate)| candidate.ok).count();
    let candidate_routing_provenance_count = matched
        .iter()
        .filter(|(_, candidate)| candidate.ok && has_candidate_route_provenance(candidate))
        .count();
    let candidate_streaming_trace_count = matched
        .iter()
        .filter(|(_, candidate)| candidate.ok && has_streaming_token_trace(candidate))
        .count();
    let candidate_routing_provenance_rate = ratio(
        candidate_routing_provenance_count,
        successful_candidate_requests,
    );
    let candidate_streaming_trace_rate = ratio(
        candidate_streaming_trace_count,
        successful_candidate_requests,
    );
    let candidate_route_count = unique_telemetry_values(&matched, |candidate| {
        candidate.telemetry.route_id.as_deref()
    })
    .len();
    let candidate_scheduler_policies = unique_telemetry_values(&matched, |candidate| {
        candidate.telemetry.scheduler_policy.as_deref()
    });
    let token_trace_pairs = matched
        .iter()
        .filter(|(baseline, candidate)| {
            baseline.telemetry.token_trace_fingerprint.is_some()
                && candidate.telemetry.token_trace_fingerprint.is_some()
        })
        .count();
    let token_trace_mismatches = matched
        .iter()
        .filter(|(baseline, candidate)| {
            match (
                &baseline.telemetry.token_trace_fingerprint,
                &candidate.telemetry.token_trace_fingerprint,
            ) {
                (Some(baseline_trace), Some(candidate_trace)) => baseline_trace != candidate_trace,
                _ => false,
            }
        })
        .count();
    let token_trace_mismatch_rate = ratio(token_trace_mismatches, token_trace_pairs);

    let mut reasons = Vec::new();
    let mut triage = Vec::new();
    let insufficient_coverage = matched_requests < input.thresholds.min_matched_requests
        || matched_requests != baseline_requests
        || matched_requests != candidate_requests;
    if insufficient_coverage {
        add_triage(
            &mut reasons,
            &mut triage,
            "evidence_coverage",
            GateDecision::Hold,
            "release_validation",
            "rerun mirrored traffic with complete baseline and candidate request coverage",
            format!(
                "matched {matched_requests} requests; need at least {} with complete coverage",
                input.thresholds.min_matched_requests
            ),
        );
    }
    if comparable_outputs.is_empty() {
        add_triage(
            &mut reasons,
            &mut triage,
            "output_comparison_coverage",
            GateDecision::Hold,
            "release_validation",
            "collect successful mirrored request pairs with auditable output material",
            "no successful request pairs were available for output comparison".into(),
        );
    }

    let correctness_failed = output_mismatch_rate > input.thresholds.max_output_mismatch_rate;
    if correctness_failed {
        add_triage(
            &mut reasons,
            &mut triage,
            "output_correctness",
            GateDecision::Rollback,
            "model_runtime_correctness",
            "block the candidate and compare output fingerprints or numeric tolerance scope",
            format!(
                "output mismatch rate {:.4} exceeded {:.4}",
                output_mismatch_rate, input.thresholds.max_output_mismatch_rate
            ),
        );
    }

    let numeric_correctness_failed =
        numeric_drift_rate > input.thresholds.max_numeric_drift_rate && numeric_pairs > 0;
    if numeric_correctness_failed {
        add_triage(
            &mut reasons,
            &mut triage,
            "numeric_correctness",
            GateDecision::Rollback,
            "accelerator_backend_correctness",
            "block the candidate and review model, backend, and accelerator tolerance policy",
            format!(
                "numeric drift rate {:.4} exceeded {:.4}",
                numeric_drift_rate, input.thresholds.max_numeric_drift_rate
            ),
        );
    }

    let reliability_failed = error_rate_increase > input.thresholds.max_error_rate_increase;
    if reliability_failed {
        add_triage(
            &mut reasons,
            &mut triage,
            "serving_reliability",
            GateDecision::Rollback,
            "serving_runtime",
            "block the candidate and inspect candidate errors before rerunning the gate",
            format!(
                "error-rate increase {:.4} exceeded {:.4}",
                error_rate_increase, input.thresholds.max_error_rate_increase
            ),
        );
    }

    let latency_failed = p95_latency_regression_pct
        .is_some_and(|regression| regression > input.thresholds.max_p95_latency_regression_pct);
    if latency_failed {
        add_triage(
            &mut reasons,
            &mut triage,
            "request_latency",
            GateDecision::Hold,
            "runtime_performance",
            "profile scheduler and backend request path before rollout",
            format!(
                "p95 latency regression {:.2}% exceeded {:.2}%",
                p95_latency_regression_pct.unwrap_or_default(),
                input.thresholds.max_p95_latency_regression_pct
            ),
        );
    }
    let ttft_failed = ttft_regression_pct
        .is_some_and(|regression| regression > input.thresholds.max_ttft_regression_pct);
    if ttft_failed {
        add_triage(
            &mut reasons,
            &mut triage,
            "time_to_first_token",
            GateDecision::Hold,
            "prefill_runtime",
            "inspect prefill queueing and first-token path before rollout",
            format!(
                "TTFT p95 regression {:.2}% exceeded {:.2}%",
                ttft_regression_pct.unwrap_or_default(),
                input.thresholds.max_ttft_regression_pct
            ),
        );
    }
    let decode_token_latency_failed = decode_token_p95_regression_pct.is_some_and(|regression| {
        regression > input.thresholds.max_decode_token_p95_regression_pct
    });
    if decode_token_latency_failed {
        add_triage(
            &mut reasons,
            &mut triage,
            "decode_token_latency",
            GateDecision::Hold,
            "decode_runtime",
            "inspect decode loop, batching policy, and backend saturation before rollout",
            format!(
                "decode-token p95 regression {:.2}% exceeded {:.2}%",
                decode_token_p95_regression_pct.unwrap_or_default(),
                input.thresholds.max_decode_token_p95_regression_pct
            ),
        );
    }
    let memory_pressure_failed = max_candidate_memory_pressure_pct
        .is_some_and(|pressure| pressure > input.thresholds.max_candidate_memory_pressure_pct);
    if memory_pressure_failed {
        add_triage(
            &mut reasons,
            &mut triage,
            "kv_memory_pressure",
            GateDecision::Hold,
            "runtime_capacity",
            "inspect KV reservation, queue pressure, and capacity policy before rollout",
            format!(
                "candidate memory pressure {:.2}% exceeded {:.2}%",
                max_candidate_memory_pressure_pct.unwrap_or_default(),
                input.thresholds.max_candidate_memory_pressure_pct
            ),
        );
    }
    let route_provenance_failed = input.thresholds.require_candidate_route_provenance
        && candidate_routing_provenance_rate < 1.0;
    if route_provenance_failed {
        add_triage(
            &mut reasons,
            &mut triage,
            "routing_provenance",
            GateDecision::Hold,
            "serving_runtime",
            "collect route and scheduler provenance for every successful candidate request",
            format!(
                "candidate routing provenance rate {:.4}; expected complete route and scheduler coverage",
                candidate_routing_provenance_rate
            ),
        );
    }
    let streaming_trace_failed =
        input.thresholds.require_streaming_token_trace && candidate_streaming_trace_rate < 1.0;
    if streaming_trace_failed {
        add_triage(
            &mut reasons,
            &mut triage,
            "streaming_token_trace",
            GateDecision::Hold,
            "decode_runtime",
            "collect streaming token events before trusting decode-path rollout evidence",
            format!(
                "candidate streaming-token trace rate {:.4}; expected complete streaming trace coverage",
                candidate_streaming_trace_rate
            ),
        );
    }

    let decision = if correctness_failed || numeric_correctness_failed || reliability_failed {
        GateDecision::Rollback
    } else if insufficient_coverage
        || comparable_outputs.is_empty()
        || latency_failed
        || ttft_failed
        || decode_token_latency_failed
        || memory_pressure_failed
        || route_provenance_failed
        || streaming_trace_failed
    {
        GateDecision::Hold
    } else {
        reasons.push(
            "candidate stayed within correctness, reliability, latency, and telemetry policy"
                .into(),
        );
        GateDecision::Promote
    };

    GateReport {
        schema_version: 4,
        decision,
        matched_requests,
        baseline_requests,
        candidate_requests,
        coverage_rate,
        output_mismatch_rate,
        numeric_pairs,
        tolerated_numeric_outputs,
        numeric_drift_rate,
        max_numeric_abs_error,
        max_numeric_rel_error,
        baseline_error_rate,
        candidate_error_rate,
        error_rate_increase,
        baseline_p95_latency_ms,
        candidate_p95_latency_ms,
        p95_latency_regression_pct,
        baseline_p95_ttft_ms,
        candidate_p95_ttft_ms,
        ttft_regression_pct,
        baseline_decode_token_p95_ms,
        candidate_decode_token_p95_ms,
        decode_token_p95_regression_pct,
        max_candidate_queue_depth,
        max_candidate_memory_pressure_pct,
        candidate_routing_provenance_rate,
        candidate_streaming_trace_rate,
        candidate_route_count,
        candidate_scheduler_policies,
        token_trace_pairs,
        token_trace_mismatch_rate,
        segments: segment_reports(&matched, &matched_comparisons),
        triage,
        reasons,
    }
}

fn add_triage(
    reasons: &mut Vec<String>,
    triage: &mut Vec<TriageItem>,
    signal: &str,
    response: GateDecision,
    owner_hint: &str,
    action: &str,
    reason: String,
) {
    reasons.push(reason.clone());
    triage.push(TriageItem {
        signal: signal.into(),
        response,
        owner_hint: owner_hint.into(),
        action: action.into(),
        reason,
    });
}

#[derive(Debug, Clone, Copy)]
struct OutputComparison {
    matches: bool,
    used_numeric: bool,
    tolerated_numeric: bool,
    numeric_drift: bool,
    max_abs_error: f64,
    max_rel_error: f64,
}

fn compare_outputs(
    baseline: &Observation,
    candidate: &Observation,
    thresholds: &GateThresholds,
) -> OutputComparison {
    if baseline.output_fingerprint == candidate.output_fingerprint {
        return OutputComparison {
            matches: true,
            used_numeric: false,
            tolerated_numeric: false,
            numeric_drift: false,
            max_abs_error: 0.0,
            max_rel_error: 0.0,
        };
    }

    let Some(tolerance) = best_numeric_tolerance(baseline, candidate, thresholds) else {
        return output_mismatch();
    };
    let Some(baseline_values) = baseline.output_values.as_ref() else {
        return output_mismatch();
    };
    let Some(candidate_values) = candidate.output_values.as_ref() else {
        return output_mismatch();
    };
    if baseline_values.len() != candidate_values.len() || baseline_values.is_empty() {
        return numeric_drift();
    }

    let mut max_abs_error = 0.0_f64;
    let mut max_rel_error = 0.0_f64;
    let mut within_tolerance = true;
    for (baseline_value, candidate_value) in baseline_values.iter().zip(candidate_values) {
        let abs_error = (candidate_value - baseline_value).abs();
        let rel_error = if *baseline_value == 0.0 {
            if abs_error == 0.0 { 0.0 } else { f64::INFINITY }
        } else {
            abs_error / baseline_value.abs()
        };

        max_abs_error = max_abs_error.max(abs_error);
        max_rel_error = max_rel_error.max(rel_error);
        if !baseline_value.is_finite()
            || !candidate_value.is_finite()
            || (abs_error > tolerance.max_abs_error && rel_error > tolerance.max_rel_error)
        {
            within_tolerance = false;
        }
    }

    OutputComparison {
        matches: within_tolerance,
        used_numeric: true,
        tolerated_numeric: within_tolerance,
        numeric_drift: !within_tolerance,
        max_abs_error: round6(max_abs_error),
        max_rel_error: round6(max_rel_error),
    }
}

fn output_mismatch() -> OutputComparison {
    OutputComparison {
        matches: false,
        used_numeric: false,
        tolerated_numeric: false,
        numeric_drift: false,
        max_abs_error: 0.0,
        max_rel_error: 0.0,
    }
}

fn numeric_drift() -> OutputComparison {
    OutputComparison {
        matches: false,
        used_numeric: true,
        tolerated_numeric: false,
        numeric_drift: true,
        max_abs_error: f64::INFINITY,
        max_rel_error: f64::INFINITY,
    }
}

fn best_numeric_tolerance<'a>(
    baseline: &Observation,
    candidate: &Observation,
    thresholds: &'a GateThresholds,
) -> Option<&'a NumericTolerance> {
    thresholds
        .numeric_tolerances
        .iter()
        .filter(|tolerance| tolerance.matches(baseline, candidate))
        .max_by_key(|tolerance| tolerance.specificity())
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SegmentKey {
    model: String,
    baseline_backend: String,
    candidate_backend: String,
    accelerator: String,
    baseline_model_version: String,
    candidate_model_version: String,
}

#[derive(Debug, Default)]
struct SegmentAccumulator {
    matched_requests: usize,
    baseline_errors: usize,
    candidate_errors: usize,
    output_pairs: usize,
    output_mismatches: usize,
    baseline_latencies: Vec<f64>,
    candidate_latencies: Vec<f64>,
    baseline_ttft_latencies: Vec<f64>,
    candidate_ttft_latencies: Vec<f64>,
    baseline_decode_token_latencies: Vec<f64>,
    candidate_decode_token_latencies: Vec<f64>,
    candidate_queue_depths: Vec<usize>,
    candidate_memory_pressure_pct: Vec<f64>,
    candidate_successes: usize,
    candidate_routing_provenance_count: usize,
    candidate_streaming_trace_count: usize,
    candidate_routes: BTreeSet<String>,
    candidate_scheduler_policies: BTreeSet<String>,
    token_trace_pairs: usize,
    token_trace_mismatches: usize,
}

fn segment_reports(
    matched: &[(&Observation, &Observation)],
    comparisons: &[Option<OutputComparison>],
) -> Vec<SegmentReport> {
    let mut segments: BTreeMap<SegmentKey, SegmentAccumulator> = BTreeMap::new();
    for ((baseline, candidate), comparison) in matched.iter().zip(comparisons) {
        let entry = segments
            .entry(segment_key(baseline, candidate))
            .or_default();
        entry.matched_requests += 1;
        if !baseline.ok {
            entry.baseline_errors += 1;
        }
        if !candidate.ok {
            entry.candidate_errors += 1;
        }
        if candidate.ok {
            entry.candidate_successes += 1;
            if has_candidate_route_provenance(candidate) {
                entry.candidate_routing_provenance_count += 1;
            }
            if has_streaming_token_trace(candidate) {
                entry.candidate_streaming_trace_count += 1;
            }
            if let Some(route_id) = candidate.telemetry.route_id.as_ref() {
                entry.candidate_routes.insert(route_id.clone());
            }
            if let Some(policy) = candidate.telemetry.scheduler_policy.as_ref() {
                entry.candidate_scheduler_policies.insert(policy.clone());
            }
        }
        if baseline.ok && candidate.ok {
            entry.output_pairs += 1;
            if comparison.is_some_and(|comparison| !comparison.matches) {
                entry.output_mismatches += 1;
            }
        }
        if baseline.ok {
            entry.baseline_latencies.push(baseline.latency_ms);
            if let Some(ttft) = baseline.telemetry.time_to_first_token_ms {
                entry.baseline_ttft_latencies.push(ttft);
            }
            entry
                .baseline_decode_token_latencies
                .extend(baseline.telemetry.decode_token_latencies_ms.iter().copied());
        }
        if candidate.ok {
            entry.candidate_latencies.push(candidate.latency_ms);
            if let Some(ttft) = candidate.telemetry.time_to_first_token_ms {
                entry.candidate_ttft_latencies.push(ttft);
            }
            entry.candidate_decode_token_latencies.extend(
                candidate
                    .telemetry
                    .decode_token_latencies_ms
                    .iter()
                    .copied(),
            );
        }
        if let Some(queue_depth) = candidate.telemetry.queue_depth {
            entry.candidate_queue_depths.push(queue_depth);
        }
        if let Some(memory_pressure_pct) = candidate.telemetry.memory_pressure_pct() {
            entry
                .candidate_memory_pressure_pct
                .push(memory_pressure_pct);
        }
        if let (Some(baseline_trace), Some(candidate_trace)) = (
            &baseline.telemetry.token_trace_fingerprint,
            &candidate.telemetry.token_trace_fingerprint,
        ) {
            entry.token_trace_pairs += 1;
            if baseline_trace != candidate_trace {
                entry.token_trace_mismatches += 1;
            }
        }
    }

    segments
        .into_iter()
        .map(|(key, accumulator)| {
            let baseline_p95_latency_ms = percentile95(accumulator.baseline_latencies.into_iter());
            let candidate_p95_latency_ms =
                percentile95(accumulator.candidate_latencies.into_iter());
            let baseline_p95_ttft_ms =
                percentile95(accumulator.baseline_ttft_latencies.into_iter());
            let candidate_p95_ttft_ms =
                percentile95(accumulator.candidate_ttft_latencies.into_iter());
            let baseline_decode_token_p95_ms =
                percentile95(accumulator.baseline_decode_token_latencies.into_iter());
            let candidate_decode_token_p95_ms =
                percentile95(accumulator.candidate_decode_token_latencies.into_iter());
            SegmentReport {
                model: key.model,
                baseline_backend: key.baseline_backend,
                candidate_backend: key.candidate_backend,
                accelerator: key.accelerator,
                baseline_model_version: key.baseline_model_version,
                candidate_model_version: key.candidate_model_version,
                matched_requests: accumulator.matched_requests,
                output_mismatch_rate: ratio(
                    accumulator.output_mismatches,
                    accumulator.output_pairs,
                ),
                baseline_error_rate: ratio(
                    accumulator.baseline_errors,
                    accumulator.matched_requests,
                ),
                candidate_error_rate: ratio(
                    accumulator.candidate_errors,
                    accumulator.matched_requests,
                ),
                baseline_p95_latency_ms,
                candidate_p95_latency_ms,
                p95_latency_regression_pct: latency_regression_pct(
                    baseline_p95_latency_ms,
                    candidate_p95_latency_ms,
                ),
                baseline_p95_ttft_ms,
                candidate_p95_ttft_ms,
                ttft_regression_pct: latency_regression_pct(
                    baseline_p95_ttft_ms,
                    candidate_p95_ttft_ms,
                ),
                baseline_decode_token_p95_ms,
                candidate_decode_token_p95_ms,
                decode_token_p95_regression_pct: latency_regression_pct(
                    baseline_decode_token_p95_ms,
                    candidate_decode_token_p95_ms,
                ),
                max_candidate_queue_depth: accumulator.candidate_queue_depths.into_iter().max(),
                max_candidate_memory_pressure_pct: max_comparison_value(
                    accumulator
                        .candidate_memory_pressure_pct
                        .into_iter()
                        .map(Some),
                ),
                candidate_routing_provenance_rate: ratio(
                    accumulator.candidate_routing_provenance_count,
                    accumulator.candidate_successes,
                ),
                candidate_streaming_trace_rate: ratio(
                    accumulator.candidate_streaming_trace_count,
                    accumulator.candidate_successes,
                ),
                candidate_route_count: accumulator.candidate_routes.len(),
                candidate_scheduler_policies: accumulator
                    .candidate_scheduler_policies
                    .into_iter()
                    .collect(),
                token_trace_pairs: accumulator.token_trace_pairs,
                token_trace_mismatch_rate: ratio(
                    accumulator.token_trace_mismatches,
                    accumulator.token_trace_pairs,
                ),
            }
        })
        .collect()
}

fn segment_key(baseline: &Observation, candidate: &Observation) -> SegmentKey {
    SegmentKey {
        model: if same_optional_value(&baseline.model, &candidate.model) {
            candidate
                .model
                .as_ref()
                .or(baseline.model.as_ref())
                .cloned()
                .unwrap_or_else(|| "unspecified".into())
        } else {
            format!(
                "{} -> {}",
                optional_label(&baseline.model),
                optional_label(&candidate.model)
            )
        },
        baseline_backend: optional_label(&baseline.backend),
        candidate_backend: optional_label(&candidate.backend),
        accelerator: candidate
            .accelerator
            .as_ref()
            .or(baseline.accelerator.as_ref())
            .cloned()
            .unwrap_or_else(|| "unspecified".into()),
        baseline_model_version: optional_label(&baseline.telemetry.model_version),
        candidate_model_version: optional_label(&candidate.telemetry.model_version),
    }
}

fn has_candidate_route_provenance(candidate: &Observation) -> bool {
    has_value(&candidate.telemetry.route_id) && has_value(&candidate.telemetry.scheduler_policy)
}

fn has_streaming_token_trace(candidate: &Observation) -> bool {
    candidate.telemetry.streamed_token_count > 0
        && candidate.telemetry.token_trace_fingerprint.is_some()
}

fn unique_telemetry_values(
    matched: &[(&Observation, &Observation)],
    selector: impl Fn(&Observation) -> Option<&str>,
) -> Vec<String> {
    matched
        .iter()
        .filter_map(|(_, candidate)| selector(candidate))
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn has_value(value: &Option<String>) -> bool {
    value.as_ref().is_some_and(|value| !value.trim().is_empty())
}

fn optional_label(value: &Option<String>) -> String {
    value.clone().unwrap_or_else(|| "unspecified".into())
}

fn same_optional_value(left: &Option<String>, right: &Option<String>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => left == right,
        _ => true,
    }
}

fn matches_filter(filter: &Option<String>, value: Option<&String>) -> bool {
    match filter {
        Some(filter) => value.is_some_and(|value| value == filter),
        None => true,
    }
}

fn max_comparison_value(values: impl Iterator<Item = Option<f64>>) -> Option<f64> {
    values
        .flatten()
        .filter(|value| value.is_finite())
        .max_by(f64::total_cmp)
        .map(round6)
}

fn latency_regression_pct(baseline: Option<f64>, candidate: Option<f64>) -> Option<f64> {
    match (baseline, candidate) {
        (Some(baseline), Some(candidate)) if baseline > 0.0 => {
            Some(round6(((candidate - baseline) / baseline) * 100.0))
        }
        _ => None,
    }
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        round6(numerator as f64 / denominator as f64)
    }
}

fn round6(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn percentile95(values: impl Iterator<Item = f64>) -> Option<f64> {
    let mut values: Vec<f64> = values.filter(|value| value.is_finite()).collect();
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let rank = ((values.len() as f64) * 0.95).ceil() as usize;
    Some(values[rank.saturating_sub(1)])
}
