# Runtime model

Each request declares prompt length, maximum output length, priority, and arrival tick. Admission reserves paged KV capacity for the declared context, applies a per-tick prefill budget, and avoids strict head-of-line blocking when a large request cannot fit. Active requests decode round-robin with a configurable batch width.

Queued requests gain one effective-priority point for every `priority_aging_interval_ticks` of waiting (default: 32 ticks), capped at the priority maximum. Original submission order breaks ties. The synthetic priority-aging fixture verifies that an older low-priority request is admitted while higher-priority work is still arriving; it is a policy test, not a production latency measurement.

Every tick records admitted and decoded request IDs, admitted prefill tokens, decoded token count, queued and active counts, and used KV pages. The replay report rolls these up into a trace fingerprint, peak KV usage, queue and active pressure, token totals, utilization ratios, and completion count.

## Features

- Priority-aware admission with stable ordering, configurable priority aging, and bounded prefill work.
- Conservative paged-KV reservations that keep admitted requests within capacity, including integer-boundary inputs.
- Round-robin decode scheduling so active requests make progress.
- Deterministic replay with a stable trace fingerprint and pressure summaries.
- Release gates with `promote`, `hold`, and `rollback` outcomes.
- Backend mirror normalization for vLLM/SGLang-style observations.
- Structured triage on hold and rollback, naming the failed signal and next action.

## Property-tested invariants

`cargo test --all-targets` also runs 128 bounded property cases over generated,
valid synthetic scheduler configurations and request sets. The test checks
identical replay reports, per-request admission/completion/decode accounting,
trace counters, queue/active state reconciliation, active-request and KV-page
limits, and final KV release. Proptest shrinks counterexamples and persists any
failure seed beside the integration test for replay.

Prompt and output token counts are widened before KV-page rounding, and the
capacity check uses checked addition. Regression tests cover requests near the
`u32` limits so arithmetic overflow cannot turn a capacity violation into an
admission.

These generated fixtures broaden coverage of the scheduler model; they are not
production traces, a workload-fidelity claim, GPU execution, performance
measurements, or proof of all possible scheduler states.

## Replay summary

Running the workload fixture writes `artifacts/workload-replay.json`. The top of that report is the aggregate outcome of scheduling four requests, before the per-tick detail.

```json
{
  "request_count": 4,
  "completed_requests": 4,
  "total_prompt_tokens": 224,
  "total_decode_tokens": 18,
  "total_reserved_kv_pages": 18,
  "total_ticks": 11,
  "peak_kv_pages": 12,
  "peak_kv_pressure_pct": 60.0,
  "decode_capacity_utilization": 0.818182,
  "final_kv_pages": 0,
  "trace_fingerprint": "b454ea97ea75ee90"
}
```

## Release policy

The gate joins baseline and candidate observations by request ID. Outputs are checked by exact fingerprint or by a numeric tolerance scoped to model, backend, and accelerator. Reports include segment summaries so backend-specific regressions stay visible.

| Signal | Response |
|---|---|
| Output mismatch above policy | `rollback` |
| Numeric drift above policy | `rollback` |
| Error-rate increase above policy | `rollback` |
| p95 latency regression above policy | `hold` |
| TTFT, decode-token p95, or memory-pressure regression | `hold` |
| Missing required route, scheduler, or streaming evidence | `hold` |
| Insufficient matched traffic | `hold` |
| Complete evidence within policy | `promote` |
