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
        model_version: None,
        queue_depth: None,
        kv_pages_used: None,
        kv_pages_capacity: None,
        time_to_first_token_ms: None,
        decode_token_latencies_ms: Vec::new(),
        token_trace_fingerprint: None,
    }
}

fn telemetry_observation(
    id: &str,
    latency_ms: f64,
    token_ids: &[i64],
    queue_depth: usize,
    kv_pages_used: usize,
    ttft_ms: f64,
    decode_token_latencies_ms: &[f64],
) -> BackendObservation {
    BackendObservation {
        request_id: id.into(),
        latency_ms,
        ok: Some(true),
        output_fingerprint: None,
        output_token_ids: token_ids.to_vec(),
        output_values: None,
        error: None,
        model_version: Some("decoder-7b@2026-06-24".into()),
        queue_depth: Some(queue_depth),
        kv_pages_used: Some(kv_pages_used),
        kv_pages_capacity: Some(20),
        time_to_first_token_ms: Some(ttft_ms),
        decode_token_latencies_ms: decode_token_latencies_ms.to_vec(),
        token_trace_fingerprint: None,
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
        model_version: None,
        queue_depth: None,
        kv_pages_used: None,
        kv_pages_capacity: None,
        time_to_first_token_ms: None,
        decode_token_latencies_ms: Vec::new(),
        token_trace_fingerprint: None,
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
fn carries_operational_telemetry_into_release_report() {
    let input = BackendMirrorInput {
        thresholds: GateThresholds::default(),
        baseline: backend_set(
            "vllm",
            vec![
                telemetry_observation("prompt-a", 18.0, &[101, 1402, 13], 2, 8, 6.0, &[4.0, 4.5]),
                telemetry_observation("prompt-b", 21.0, &[205, 778, 990], 3, 9, 7.0, &[5.0, 5.5]),
                telemetry_observation("prompt-c", 24.0, &[42, 42, 7], 4, 10, 8.0, &[6.0, 6.5]),
            ],
        ),
        candidate: backend_set(
            "sglang",
            vec![
                telemetry_observation("prompt-a", 17.5, &[101, 1402, 13], 4, 10, 5.5, &[3.8, 4.0]),
                telemetry_observation("prompt-b", 20.6, &[205, 778, 990], 5, 11, 6.5, &[4.7, 5.0]),
                telemetry_observation("prompt-c", 23.5, &[42, 42, 7], 6, 12, 7.5, &[5.8, 6.0]),
            ],
        ),
    };

    let report = evaluate_release(&mirror_to_gate_input(&input).expect("valid mirror input"));

    assert_eq!(report.decision, GateDecision::Promote);
    assert_eq!(report.max_candidate_queue_depth, Some(6));
    assert_eq!(report.max_candidate_memory_pressure_pct, Some(60.0));
    assert_eq!(report.token_trace_pairs, 3);
    assert_eq!(report.token_trace_mismatch_rate, 0.0);
    assert_eq!(report.candidate_p95_ttft_ms, Some(7.5));
    assert_eq!(report.candidate_decode_token_p95_ms, Some(6.0));
}

#[test]
fn holds_when_candidate_telemetry_exceeds_policy() {
    let input = BackendMirrorInput {
        thresholds: GateThresholds {
            max_ttft_regression_pct: 10.0,
            max_decode_token_p95_regression_pct: 10.0,
            max_candidate_memory_pressure_pct: 90.0,
            ..GateThresholds::default()
        },
        baseline: backend_set(
            "vllm",
            vec![
                telemetry_observation("prompt-a", 18.0, &[101], 2, 8, 6.0, &[4.0]),
                telemetry_observation("prompt-b", 21.0, &[205], 3, 9, 7.0, &[5.0]),
                telemetry_observation("prompt-c", 24.0, &[42], 4, 10, 8.0, &[6.0]),
            ],
        ),
        candidate: backend_set(
            "sglang",
            vec![
                telemetry_observation("prompt-a", 18.0, &[101], 7, 19, 10.0, &[8.0]),
                telemetry_observation("prompt-b", 21.0, &[205], 8, 19, 11.0, &[9.0]),
                telemetry_observation("prompt-c", 24.0, &[42], 9, 19, 12.0, &[10.0]),
            ],
        ),
    };

    let report = evaluate_release(&mirror_to_gate_input(&input).expect("valid mirror input"));

    assert_eq!(report.decision, GateDecision::Hold);
    assert!(report.reasons.iter().any(|reason| reason.contains("TTFT")));
    assert!(
        report
            .reasons
            .iter()
            .any(|reason| reason.contains("decode-token"))
    );
    assert!(
        report
            .reasons
            .iter()
            .any(|reason| reason.contains("memory pressure"))
    );
    assert!(
        report.triage.iter().any(
            |item| item.signal == "time_to_first_token" && item.owner_hint == "prefill_runtime"
        )
    );
    assert!(
        report.triage.iter().any(
            |item| item.signal == "decode_token_latency" && item.owner_hint == "decode_runtime"
        )
    );
    assert!(
        report.triage.iter().any(
            |item| item.signal == "kv_memory_pressure" && item.owner_hint == "runtime_capacity"
        )
    );
}

#[test]
fn numeric_mirror_outputs_use_model_backend_tolerance() {
    let input = BackendMirrorInput {
        thresholds: GateThresholds {
            min_matched_requests: 3,
            max_output_mismatch_rate: 0.0,
            max_error_rate_increase: 0.01,
            max_p95_latency_regression_pct: 10.0,
            max_ttft_regression_pct: 10.0,
            max_decode_token_p95_regression_pct: 10.0,
            max_candidate_memory_pressure_pct: 95.0,
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
                model_version: None,
                queue_depth: None,
                kv_pages_used: None,
                kv_pages_capacity: None,
                time_to_first_token_ms: None,
                decode_token_latencies_ms: Vec::new(),
                token_trace_fingerprint: None,
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
