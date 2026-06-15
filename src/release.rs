use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    pub request_id: String,
    pub output_fingerprint: String,
    pub latency_ms: f64,
    pub ok: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GateThresholds {
    pub min_matched_requests: usize,
    pub max_output_mismatch_rate: f64,
    pub max_error_rate_increase: f64,
    pub max_p95_latency_regression_pct: f64,
}

impl Default for GateThresholds {
    fn default() -> Self {
        Self {
            min_matched_requests: 3,
            max_output_mismatch_rate: 0.0,
            max_error_rate_increase: 0.01,
            max_p95_latency_regression_pct: 10.0,
        }
    }
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GateReport {
    pub schema_version: u32,
    pub decision: GateDecision,
    pub matched_requests: usize,
    pub baseline_requests: usize,
    pub candidate_requests: usize,
    pub coverage_rate: f64,
    pub output_mismatch_rate: f64,
    pub baseline_error_rate: f64,
    pub candidate_error_rate: f64,
    pub error_rate_increase: f64,
    pub baseline_p95_latency_ms: Option<f64>,
    pub candidate_p95_latency_ms: Option<f64>,
    pub p95_latency_regression_pct: Option<f64>,
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
    let mismatches = comparable_outputs
        .iter()
        .filter(|(baseline, candidate)| baseline.output_fingerprint != candidate.output_fingerprint)
        .count();
    let output_mismatch_rate = ratio(mismatches, comparable_outputs.len());

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
    let p95_latency_regression_pct = match (baseline_p95_latency_ms, candidate_p95_latency_ms) {
        (Some(baseline), Some(candidate)) if baseline > 0.0 => {
            Some(round6(((candidate - baseline) / baseline) * 100.0))
        }
        _ => None,
    };

    let mut reasons = Vec::new();
    let insufficient_coverage = matched_requests < input.thresholds.min_matched_requests
        || matched_requests != baseline_requests
        || matched_requests != candidate_requests;
    if insufficient_coverage {
        reasons.push(format!(
            "matched {matched_requests} requests; need at least {} with complete coverage",
            input.thresholds.min_matched_requests
        ));
    }
    if comparable_outputs.is_empty() {
        reasons.push("no successful request pairs were available for output comparison".into());
    }

    let correctness_failed = output_mismatch_rate > input.thresholds.max_output_mismatch_rate;
    if correctness_failed {
        reasons.push(format!(
            "output mismatch rate {:.4} exceeded {:.4}",
            output_mismatch_rate, input.thresholds.max_output_mismatch_rate
        ));
    }

    let reliability_failed = error_rate_increase > input.thresholds.max_error_rate_increase;
    if reliability_failed {
        reasons.push(format!(
            "error-rate increase {:.4} exceeded {:.4}",
            error_rate_increase, input.thresholds.max_error_rate_increase
        ));
    }

    let latency_failed = p95_latency_regression_pct
        .is_some_and(|regression| regression > input.thresholds.max_p95_latency_regression_pct);
    if latency_failed {
        reasons.push(format!(
            "p95 latency regression {:.2}% exceeded {:.2}%",
            p95_latency_regression_pct.unwrap_or_default(),
            input.thresholds.max_p95_latency_regression_pct
        ));
    }

    let decision = if correctness_failed || reliability_failed {
        GateDecision::Rollback
    } else if insufficient_coverage || comparable_outputs.is_empty() || latency_failed {
        GateDecision::Hold
    } else {
        reasons.push("candidate stayed within correctness, reliability, and latency policy".into());
        GateDecision::Promote
    };

    GateReport {
        schema_version: 1,
        decision,
        matched_requests,
        baseline_requests,
        candidate_requests,
        coverage_rate,
        output_mismatch_rate,
        baseline_error_rate,
        candidate_error_rate,
        error_rate_increase,
        baseline_p95_latency_ms,
        candidate_p95_latency_ms,
        p95_latency_regression_pct,
        reasons,
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
