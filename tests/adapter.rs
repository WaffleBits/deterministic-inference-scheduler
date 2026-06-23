use rust_inference_runtime::{
    BackendMirrorInput, BackendObservation, BackendObservationSet, GateDecision, GateThresholds,
    NumericTolerance, evaluate_release, mirror_to_gate_input,
};

fn backend_set(backend: &str, observations: Vec<BackendObservation>) -> BackendObservationSet {
    BackendObservationSet {
        backend: backend.into(),
        model: "decoder-7b".into(),
        accelerator: Some("h100".into()),
        observations,
    }
}

fn token_observation(id: &str, latency_ms: f64, token_ids: &[i64]) -> BackendObservation {
    BackendObservation {
        request_id: id.into(),
        latency_ms,
        ok: Some(true),
        output_fingerprint: None,
        output_token_ids: token_ids.to_vec(),
        output_values: None,
        error: None,
    }
}

fn numeric_observation(id: &str, latency_ms: f64, values: &[f64]) -> BackendObservation {
    BackendObservation {
        request_id: id.into(),
        latency_ms,
        ok: Some(true),
        output_fingerprint: None,
        output_token_ids: Vec::new(),
        output_values: Some(values.to_vec()),
        error: None,
    }
}

#[test]
fn converts_vllm_and_sglang_mirrors_into_release_gate_input() {
    let input = BackendMirrorInput {
        thresholds: GateThresholds::default(),
        baseline: backend_set(
            "vllm",
            vec![
                token_observation("prompt-a", 18.0, &[101, 1402, 13]),
                token_observation("prompt-b", 21.0, &[205, 778, 990]),
                token_observation("prompt-c", 24.0, &[42, 42, 7]),
            ],
        ),
        candidate: backend_set(
            "sglang",
            vec![
                token_observation("prompt-a", 17.5, &[101, 1402, 13]),
                token_observation("prompt-b", 20.6, &[205, 778, 990]),
                token_observation("prompt-c", 23.5, &[42, 42, 7]),
            ],
        ),
    };

    let gate_input = mirror_to_gate_input(&input).expect("valid mirror input");
    assert_eq!(gate_input.baseline[0].backend.as_deref(), Some("vllm"));
    assert_eq!(gate_input.candidate[0].backend.as_deref(), Some("sglang"));
    assert_eq!(
        gate_input.baseline[0].output_fingerprint,
        gate_input.candidate[0].output_fingerprint
    );

    let report = evaluate_release(&gate_input);
    assert_eq!(report.decision, GateDecision::Promote);
    assert_eq!(report.segments.len(), 1);
    assert_eq!(report.segments[0].baseline_backend, "vllm");
    assert_eq!(report.segments[0].candidate_backend, "sglang");
}

#[test]
fn numeric_mirror_outputs_use_model_backend_tolerance() {
    let input = BackendMirrorInput {
        thresholds: GateThresholds {
            min_matched_requests: 3,
            max_output_mismatch_rate: 0.0,
            max_error_rate_increase: 0.01,
            max_p95_latency_regression_pct: 10.0,
            max_numeric_drift_rate: 0.0,
            numeric_tolerances: vec![NumericTolerance {
                model: Some("decoder-7b".into()),
                candidate_backend: Some("sglang".into()),
                candidate_accelerator: Some("h100".into()),
                max_abs_error: 0.002,
                max_rel_error: 0.01,
            }],
        },
        baseline: backend_set(
            "vllm",
            vec![
                numeric_observation("prompt-a", 18.0, &[0.125, 0.5, 1.25]),
                numeric_observation("prompt-b", 21.0, &[0.25, 0.75, 1.5]),
                numeric_observation("prompt-c", 24.0, &[0.375, 1.0, 1.75]),
            ],
        ),
        candidate: backend_set(
            "sglang",
            vec![
                numeric_observation("prompt-a", 18.1, &[0.126, 0.501, 1.251]),
                numeric_observation("prompt-b", 20.9, &[0.251, 0.751, 1.501]),
                numeric_observation("prompt-c", 23.8, &[0.376, 1.001, 1.751]),
            ],
        ),
    };

    let report = evaluate_release(&mirror_to_gate_input(&input).expect("valid mirror input"));

    assert_eq!(report.decision, GateDecision::Promote);
    assert_eq!(report.numeric_pairs, 3);
    assert_eq!(report.tolerated_numeric_outputs, 3);
    assert_eq!(report.output_mismatch_rate, 0.0);
}

#[test]
fn rejects_successful_observation_without_output_material() {
    let input = BackendMirrorInput {
        thresholds: GateThresholds::default(),
        baseline: backend_set(
            "vllm",
            vec![BackendObservation {
                request_id: "prompt-a".into(),
                latency_ms: 18.0,
                ok: Some(true),
                output_fingerprint: None,
                output_token_ids: Vec::new(),
                output_values: None,
                error: None,
            }],
        ),
        candidate: backend_set("sglang", vec![token_observation("prompt-a", 17.5, &[1])]),
    };

    let error = mirror_to_gate_input(&input).expect_err("missing output must fail");
    assert!(
        error
            .to_string()
            .contains("no output fingerprint, token ids, or numeric values")
    );
}
