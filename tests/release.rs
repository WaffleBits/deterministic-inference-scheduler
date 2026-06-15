use rust_inference_runtime::{
    GateDecision, GateInput, GateThresholds, Observation, evaluate_release,
};

fn observation(id: &str, fingerprint: &str, latency_ms: f64, ok: bool) -> Observation {
    Observation {
        request_id: id.into(),
        output_fingerprint: fingerprint.into(),
        latency_ms,
        ok,
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
