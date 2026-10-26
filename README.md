# Deterministic Inference Scheduler

[![CI](https://github.com/WaffleBits/deterministic-inference-scheduler/actions/workflows/ci.yml/badge.svg)](https://github.com/WaffleBits/deterministic-inference-scheduler/actions/workflows/ci.yml)

A deterministic inference runtime core in Rust. It models continuous-batching admission and paged KV-cache scheduling, records replayable traces, and runs promote/hold/rollback release gates over baseline-versus-candidate runs.

Everything runs on synthetic fixtures. No GPU, no model weights, no accelerator required. The runtime schedules and accounts for work; it does not execute model kernels.

## What it does

- Priority-aware admission with stable ordering, configurable priority aging, and bounded prefill work.
- Conservative paged-KV reservations that keep admitted requests within capacity, including integer-boundary inputs.
- Round-robin decode scheduling so active requests make progress.
- Deterministic replay with a stable trace fingerprint and pressure summaries.
- Release gates with `promote`, `hold`, and `rollback` outcomes.
- Backend mirror normalization for vLLM/SGLang-style observations.
- Structured triage on hold and rollback, naming the failed signal and next action.

## Quick start

```bash
cargo test --all-targets

cargo run --release -- replay \
  --input fixtures/workload.json \
  --output artifacts/workload-replay.json

cargo run --release -- gate \
  --input fixtures/release_gate_safe.json \
  --output artifacts/release-gate-promote.json

cargo run --release -- gate \
  --input fixtures/release_gate_bad.json \
  --output artifacts/release-gate-rollback.json
```

The safe fixture produces `promote`. The candidate with an output mismatch and an added error produces `rollback`. More fixtures cover numeric tolerance, backend mirroring, and streaming token events; see the Quick start commands in `docs/RELEASE_VALIDATION.md`.

## Runtime model

Each request declares prompt length, maximum output length, priority, and arrival tick. Admission reserves paged KV capacity for the declared context, applies a per-tick prefill budget, and avoids strict head-of-line blocking when a large request cannot fit. Active requests decode round-robin with a configurable batch width.

Queued requests gain one effective-priority point for every `priority_aging_interval_ticks` of waiting (default: 32 ticks), capped at the priority maximum. Original submission order breaks ties. The synthetic priority-aging fixture verifies that an older low-priority request is admitted while higher-priority work is still arriving; it is a policy test, not a production latency measurement.

Every tick records admitted and decoded request IDs, admitted prefill tokens, decoded token count, queued and active counts, and used KV pages. The replay report rolls these up into a trace fingerprint, peak KV usage, queue and active pressure, token totals, utilization ratios, and completion count.

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

Running the workload fixture writes `artifacts/workload-replay.json`. The top of that report is what a reviewer looks at first: the aggregate outcome of scheduling four requests, before the per-tick detail.

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

All four requests complete in 11 ticks, KV usage peaks at 12 of 20 pages, every reserved page is returned at the end, and the run reproduces the same fingerprint each time.

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

See [Release Validation](docs/RELEASE_VALIDATION.md) and [Architecture](docs/ARCHITECTURE.md) for extension points and invariants.

## Design boundaries

This is a focused runtime and validation artifact, not a claim of production scale. It does not run model kernels, coordinate multiple hosts, or manage real traffic. The interfaces stay small enough to review and extend toward accelerator backends, distributed coordination, and shadow traffic.
