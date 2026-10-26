# Architecture

The repository separates scheduling behavior from release policy.

## Scheduler

`Scheduler` owns:

- a priority queue ordered by effective priority and stable submission sequence, with configurable waiting-time aging;
- a conservative paged-KV reservation model;
- a bounded prefill budget;
- a round-robin decode queue; and
- a per-tick trace of admission, decode, completion, and memory state.

Requests reserve enough KV pages for the prompt and declared maximum output.
This policy trades utilization for a simple invariant: an admitted request
cannot fail later because its declared decode length exhausts the runtime's KV
budget. Requests that do not fit remain queued while smaller requests may
proceed, avoiding strict head-of-line blocking. At each admission tick, queued
requests gain one effective-priority point for each configured aging interval
(default: 32 ticks), capped at the maximum `u8` priority. Stable submission
sequence breaks ties, so older work is preferred after priorities converge.
The controlled replay fixture demonstrates this policy with synthetic arrivals;
it does not establish a production latency bound.

The scheduler is deliberately accelerator-agnostic. A production runtime would
connect each trace step to backend-specific kernels, streams, memory managers,
and distributed coordination.

## Replay

Replay inputs contain scheduler configuration and timestamped requests. The
runner preserves input order for equal arrival times and emits a stable FNV-1a
fingerprint over the serialized trace. Replaying the same input should produce
the same admission, decode, completion, and memory sequence.

The replay report also summarizes workload pressure and capacity use: maximum
queued requests, maximum active requests, peak KV pages, peak KV pressure
percentage, ticks with queued requests, ticks at active capacity, total prompt
and decode tokens, total reserved KV pages, declared prefill/decode/KV capacity,
and utilization ratios over the full run. Those fields make stress fixtures
reviewable without requiring a reader to manually inspect every tick.

## Release Gate

The release evaluator joins baseline and candidate observations by request ID.
It computes:

- request coverage;
- exact output-fingerprint mismatch rate;
- model/backend-scoped numeric drift when fingerprints differ;
- error-rate increase;
- successful-request p95 latency; and
- candidate p95 regression;
- TTFT p95 and decode-token p95 regression;
- candidate queue depth and KV memory pressure;
- candidate route/scheduler provenance and streaming trace coverage; and
- segment summaries by model, model version, baseline backend, candidate
  backend, and accelerator.

Correctness, numeric drift, or reliability regressions produce `rollback`.
Latency, token-path, memory-pressure, missing routing provenance, missing
streaming trace coverage, or incomplete evidence produce `hold`. A complete
candidate within policy produces `promote`.
Hold and rollback reports include structured triage items so CI or rollout
tooling can route the failed signal to a likely owner without parsing prose.

This is a local validation component, not a deployment controller. Production
integration would obtain observations from mirrored traffic, canary
populations, telemetry, and an audited rollout system.

## Backend Mirror Adapter

The adapter sits before the release gate. It normalizes backend-specific
mirrored observations into `GateInput` without changing the gate policy. This
keeps ingestion concerns separate from rollout decisions.

The adapter currently accepts compact vLLM/SGLang-style request summaries and
streaming request traces: request ID, latency, health, model, backend,
accelerator, output token IDs, streaming token events, optional explicit
fingerprints, optional numeric vectors, model version, route ID, replica ID,
scheduler policy, queue depth, KV page usage, TTFT, decode-token latencies, and
optional token-trace fingerprints. If an engine does not provide an output
fingerprint, the adapter computes a stable FNV-1a fingerprint from token IDs,
streaming token events, or numeric values. Successful observations without
output material are rejected so a candidate cannot be promoted from
latency-only evidence.
