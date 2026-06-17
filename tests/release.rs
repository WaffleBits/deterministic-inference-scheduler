use rust_inference_runtime::{
    GateDecision, GateInput, GateThresholds, NumericTolerance, Observation, evaluate_release,
};

fn observation(id: &str, fingerprint: &str, latency_ms: f64, ok: bool) -> Observation {
    Observation {
        request_id: id.into(),
        output_fingerprint: fingerprint.into(),
        latency_ms,
        ok,
        model: None,
        backend: None,
        accelerator: None,
        output_values: None,
    }
}

fn numeric_observation(
    id: &str,
    fingerprint: &str,
    backend: &str,
    values: &[f64],
    latency_ms: f64,
) -> Observation {
    Observation {
        request_id: id.into(),
        output_fingerprint: fingerprint.into(),
        latency_ms,
        ok: true,
        model: Some("decoder-7b".into()),
        backend: Some(backend.into()),
        accelerator: Some("h100".into()),
        output_values: Some(values.to_vec()),
    }
}

fn baseline() -> Vec<Observation> {
    vec![
        observation("a", "aa", 10.0, true),
        observation("b", "bb", 12.0, true),
        observation("c", "cc", 14.0, true),
        observation("d", "dd", 16.0, true),
    ]
}

#[test]
fn promotes_candidate_within_policy() {
    let report = evaluate_release(&GateInput {
        thresholds: GateThresholds::default(),
        baseline: baseline(),
        candidate: vec![
            observation("a", "aa", 10.5, true),
            observation("b", "bb", 12.5, true),
            observation("c", "cc", 14.5, true),
            observation("d", "dd", 16.5, true),
        ],
    });

    assert_eq!(report.decision, GateDecision::Promote);
}

#[test]
fn rolls_back_output_mismatch() {
    let report = evaluate_release(&GateInput {
        thresholds: GateThresholds::default(),
        baseline: baseline(),
        candidate: vec![
            observation("a", "aa", 10.5, true),
            observation("b", "changed", 12.5, true),
            observation("c", "cc", 14.5, true),
            observation("d", "dd", 16.5, true),
        ],
    });

    assert_eq!(report.decision, GateDecision::Rollback);
    assert_eq!(report.output_mismatch_rate, 0.25);
}

#[test]
fn holds_latency_regression_for_investigation() {
    let report = evaluate_release(&GateInput {
        thresholds: GateThresholds::default(),
        baseline: baseline(),
        candidate: vec![
            observation("a", "aa", 20.0, true),
            observation("b", "bb", 22.0, true),
            observation("c", "cc", 24.0, true),
            observation("d", "dd", 26.0, true),
        ],
    });

    assert_eq!(report.decision, GateDecision::Hold);
    assert!(report.p95_latency_regression_pct.unwrap() > 10.0);
}

#[test]
fn promotes_candidate_with_numeric_outputs_inside_model_tolerance() {
    let report = evaluate_release(&GateInput {
        thresholds: GateThresholds {
            min_matched_requests: 3,
            max_output_mismatch_rate: 0.0,
            max_error_rate_increase: 0.01,
            max_p95_latency_regression_pct: 10.0,
            max_numeric_drift_rate: 0.0,
            numeric_tolerances: vec![NumericTolerance {
                model: Some("decoder-7b".into()),
                candidate_backend: Some("candidate".into()),
                candidate_accelerator: Some("h100".into()),
                max_abs_error: 0.002,
                max_rel_error: 0.01,
            }],
        },
        baseline: vec![
            numeric_observation("a", "baseline-a", "baseline", &[1.0, 2.0], 10.0),
            numeric_observation("b", "baseline-b", "baseline", &[2.0, 4.0], 12.0),
            numeric_observation("c", "baseline-c", "baseline", &[3.0, 6.0], 14.0),
        ],
        candidate: vec![
            numeric_observation("a", "candidate-a", "candidate", &[1.001, 2.001], 10.5),
            numeric_observation("b", "candidate-b", "candidate", &[2.001, 4.001], 12.5),
            numeric_observation("c", "candidate-c", "candidate", &[3.001, 6.001], 14.5),
        ],
    });

    assert_eq!(report.decision, GateDecision::Promote);
    assert_eq!(report.output_mismatch_rate, 0.0);
    assert_eq!(report.numeric_pairs, 3);
    assert_eq!(report.tolerated_numeric_outputs, 3);
    assert_eq!(report.segments.len(), 1);
    assert_eq!(report.segments[0].model, "decoder-7b");
    assert_eq!(report.segments[0].candidate_backend, "candidate");
}

#[test]
fn rolls_back_numeric_outputs_outside_model_tolerance() {
    let report = evaluate_release(&GateInput {
        thresholds: GateThresholds {
            min_matched_requests: 3,
            max_output_mismatch_rate: 0.0,
            max_error_rate_increase: 0.01,
            max_p95_latency_regression_pct: 10.0,
            max_numeric_drift_rate: 0.0,
            numeric_tolerances: vec![NumericTolerance {
                model: Some("decoder-7b".into()),
                candidate_backend: Some("candidate".into()),
                candidate_accelerator: Some("h100".into()),
                max_abs_error: 0.002,
                max_rel_error: 0.01,
            }],
        },
        baseline: vec![
            numeric_observation("a", "baseline-a", "baseline", &[1.0, 2.0], 10.0),
            numeric_observation("b", "baseline-b", "baseline", &[2.0, 4.0], 12.0),
            numeric_observation("c", "baseline-c", "baseline", &[3.0, 6.0], 14.0),
        ],
        candidate: vec![
            numeric_observation("a", "candidate-a", "candidate", &[1.001, 2.001], 10.5),
            numeric_observation("b", "candidate-b", "candidate", &[2.5, 4.001], 12.5),
            numeric_observation("c", "candidate-c", "candidate", &[3.001, 6.001], 14.5),
        ],
    });

    assert_eq!(report.decision, GateDecision::Rollback);
    assert_eq!(report.output_mismatch_rate, 0.333333);
    assert_eq!(report.numeric_drift_rate, 0.333333);
    assert!(
        report
            .reasons
            .iter()
            .any(|reason| reason.contains("numeric drift rate"))
    );
}
