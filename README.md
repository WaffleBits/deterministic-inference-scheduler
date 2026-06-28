# Rust Inference Runtime

[![CI](https://github.com/WaffleBits/rust-inference-runtime/actions/workflows/ci.yml/badge.svg)](https://github.com/WaffleBits/rust-inference-runtime/actions/workflows/ci.yml)

A deterministic, accelerator-agnostic inference runtime core written in Rust.
The repository focuses on continuous-batching policy, paged KV-cache admission,
replayable scheduling traces, and canary/shadow release decisions.

## What It Demonstrates

- Performance-sensitive Rust with explicit ownership of queue and memory state.
- Priority-aware admission with stable ordering and bounded prefill work.
- Conservative paged-KV reservations that prevent admitted requests from
  exceeding declared capacity.
- Round-robin decode scheduling so active requests make measurable progress.
- Deterministic workload replay with a machine-readable trace fingerprint,
  queue-pressure summary, active-capacity summary, KV-pressure summary, and
  replay-level capacity envelope.
- Baseline/candidate release validation with `promote`, `hold`, and `rollback`
  outcomes.
- Backend mirror normalization for vLLM/SGLang-style serving observations
  before the release gate runs.
- Streaming token-event normalization with route and scheduler provenance for
  mirrored serving traces.
- Exact output checks, model-aware numeric tolerances for backend drift,
  per-segment release summaries, error-rate deltas, p95 latency regression
  policy, TTFT and decode-token p95 checks, KV memory-pressure reporting,
  model-version transitions, token-trace fingerprints, structured triage
  owner hints, tests, and CI.

## Quick Start

```bash
cargo test --all-targets

cargo run --release -- replay \
  --input fixtures/workload.json \
  --output artifacts/workload-replay.json

cargo run --release -- replay \
  --input fixtures/workload_pressure.json \
  --output artifacts/workload-pressure-replay.json

cargo run --release -- gate \
  --input fixtures/release_gate_safe.json \
  --output artifacts/release-gate-promote.json

cargo run --release -- gate \
  --input fixtures/release_gate_bad.json \
  --output artifacts/release-gate-rollback.json

cargo run --release -- gate \
  --input fixtures/release_gate_numeric_tolerance.json \
  --output artifacts/release-gate-numeric-tolerance.json

cargo run --release -- mirror-gate \
  --input fixtures/backend_mirror_vllm_sglang.json \
  --output artifacts/backend-mirror-report.json

cargo run --release -- mirror-gate \
  --input fixtures/backend_mirror_streaming_vllm_sglang.json \
  --output artifacts/backend-mirror-streaming-report.json
```

The safe fixture produces `promote`. The candidate with an output mismatch and
an added error produces `rollback`.
The numeric-tolerance fixture produces `promote` while reporting four tolerated
numeric comparisons across a baseline-runtime to candidate-runtime segment.
The backend-mirror fixture converts vLLM/SGLang-style request observations into
the same release gate and produces `promote` with a vLLM to SGLang segment,
model-version transition metadata, queue depth, KV memory pressure, TTFT, and
decode-token p95 telemetry.
The streaming mirror fixture uses per-token stream events instead of compact
token arrays and requires complete candidate route and scheduler provenance. It
produces `promote` with `candidate_routing_provenance_rate: 1.0`,
`candidate_streaming_trace_rate: 1.0`, two candidate routes, and
`continuous-batching` scheduler evidence.

The checked workload fixture completes four requests in 11 scheduler ticks,
accounts for 224 prompt tokens, 18 decode tokens, and 18 reserved KV pages,
peaks at 12 of 20 KV pages, records three queued-pressure ticks, records three
active-capacity ticks, reports 0.818182 decode-capacity utilization, returns
all pages on completion, and emits trace fingerprint `b454ea97ea75ee90`.

The pressure fixture completes eight mixed-priority requests in 27 scheduler
ticks, records a maximum queue depth of five, reaches all three active slots,
accounts for 432 prompt tokens, 48 decode tokens, and 35 reserved KV pages,
peaks at 13 of 15 KV pages, reports 86.666667% peak KV pressure, records
0.888889 decode-capacity utilization and 0.595062 KV-page occupancy, and
returns all pages on completion.

## Runtime Model

Each request declares prompt length, maximum output length, priority, and
arrival tick. Admission reserves paged KV capacity for the declared maximum
context, applies a per-tick prefill budget, and avoids strict head-of-line
blocking when a large request cannot fit. Active requests decode in round-robin
order with a configurable batch width.

Every tick records:

- admitted request IDs;
- admitted prefill tokens;
- decoded and completed request IDs;
- decoded token count;
- queued and active counts; and
- used KV pages.

The replay report includes a stable trace fingerprint, peak KV pages, peak KV
pressure percentage, maximum queued and active request counts, queue-pressure
ticks, active-capacity ticks, total prompt and decode tokens, total reserved KV
pages, declared prefill/decode/KV capacity, utilization ratios, total ticks,
and completion count.

## Backend Mirror Adapter

`runtime-lab mirror` converts backend-specific mirrored observations into a
gate input. `runtime-lab mirror-gate` performs the conversion and immediately
evaluates the release policy.

The adapter accepts per-request latency, health, model, backend, accelerator,
output token IDs, explicit output fingerprints, optional numeric output
vectors, and optional streaming token events. Successful observations must
carry output material so correctness checks remain auditable. Token IDs,
streaming token events, and numeric vectors are converted into stable FNV-1a
fingerprints when an engine-specific fingerprint is not supplied. Observations
may also carry model version, route ID, replica ID, scheduler policy, queue
depth, KV page usage, TTFT, decode-token latencies, and token-trace
fingerprints. When per-token stream events are provided, the adapter derives
TTFT and decode-token gaps from their elapsed timestamps. Those fields let the
gate surface rollout context and hold a candidate when latency, memory
pressure, routing provenance, or streaming trace coverage crosses policy even
if output correctness is intact.

## Release Policy

The gate joins mirrored baseline and candidate observations by request ID.
Outputs can be validated either by exact fingerprint or by a configured
numeric tolerance scoped to model, candidate backend, and accelerator. Reports
include aggregate metrics plus segment summaries so hardware/backend-specific
regressions remain visible. Hold and rollback reports also include structured
triage items that name the failed signal, the recommended response, an owner
hint, and the next investigation action.

| Signal | Response |
|---|---|
| Output mismatch above policy | `rollback` |
| Numeric drift above model/backend policy | `rollback` |
| Error-rate increase above policy | `rollback` |
| p95 latency regression above policy | `hold` |
| TTFT, decode-token p95, or memory-pressure regression above policy | `hold` |
| Missing required candidate route/scheduler or streaming-token evidence | `hold` |
| Missing or insufficient matched traffic | `hold` |
| Complete evidence within policy | `promote` |

See [Release Validation](docs/RELEASE_VALIDATION.md) for extension points such
as model-aware numeric tolerances, segmented SLO checks, and audited rollout
integration.

## Design Boundaries

This repository is a focused runtime and validation artifact, not a claim of
production fleet scale. It does not execute model kernels, coordinate multiple
hosts, or manage real deployment traffic. The interfaces are intentionally
small enough to review and extend toward accelerator backends, distributed
coordination, shadow traffic, and Kubernetes job scheduling.

See [Architecture](docs/ARCHITECTURE.md) for invariants and tradeoffs.
