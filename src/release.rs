use std::collections::BTreeMap;

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
    #[serde(default)]
    pub max_numeric_drift_rate: f64,
    #[serde(default)]
    pub numeric_tolerances: Vec<NumericTolerance>,
}

impl Default for GateThresholds {
    fn default() -> Self {
        Self {
            min_matched_requests: 3,
            max_output_mismatch_rate: 0.0,
            max_error_rate_increase: 0.01,
            max_p95_latency_regression_pct: 10.0,
            max_numeric_drift_rate: 0.0,
            numeric_tolerances: Vec::new(),
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
pub struct SegmentReport {
    pub model: String,
    pub baseline_backend: String,
    pub candidate_backend: String,
    pub accelerator: String,
    pub matched_requests: usize,
    pub output_mismatch_rate: f64,
    pub baseline_error_rate: f64,
    pub candidate_error_rate: f64,
    pub baseline_p95_latency_ms: Option<f64>,
    pub candidate_p95_latency_ms: Option<f64>,
    pub p95_latency_regression_pct: Option<f64>,
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
    pub segments: Vec<SegmentReport>,
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

    let numeric_correctness_failed =
        numeric_drift_rate > input.thresholds.max_numeric_drift_rate && numeric_pairs > 0;
    if numeric_correctness_failed {
        reasons.push(format!(
            "numeric drift rate {:.4} exceeded {:.4}",
            numeric_drift_rate, input.thresholds.max_numeric_drift_rate
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

    let decision = if correctness_failed || numeric_correctness_failed || reliability_failed {
        GateDecision::Rollback
    } else if insufficient_coverage || comparable_outputs.is_empty() || latency_failed {
        GateDecision::Hold
    } else {
        reasons.push("candidate stayed within correctness, reliability, and latency policy".into());
        GateDecision::Promote
    };

    GateReport {
        schema_version: 2,
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
        segments: segment_reports(&matched, &matched_comparisons),
        reasons,
    }
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
        if baseline.ok && candidate.ok {
            entry.output_pairs += 1;
            if comparison.is_some_and(|comparison| !comparison.matches) {
                entry.output_mismatches += 1;
            }
        }
        if baseline.ok {
            entry.baseline_latencies.push(baseline.latency_ms);
        }
        if candidate.ok {
            entry.candidate_latencies.push(candidate.latency_ms);
        }
    }

    segments
        .into_iter()
        .map(|(key, accumulator)| {
            let baseline_p95_latency_ms = percentile95(accumulator.baseline_latencies.into_iter());
            let candidate_p95_latency_ms =
                percentile95(accumulator.candidate_latencies.into_iter());
            SegmentReport {
                model: key.model,
                baseline_backend: key.baseline_backend,
                candidate_backend: key.candidate_backend,
                accelerator: key.accelerator,
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
    }
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
