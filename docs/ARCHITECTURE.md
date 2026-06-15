# Architecture

The repository separates scheduling behavior from release policy.

## Scheduler

`Scheduler` owns:

- a priority queue ordered by priority and stable arrival sequence;
- a conservative paged-KV reservation model;
- a bounded prefill budget;
- a round-robin decode queue; and
- a per-tick trace of admission, decode, completion, and memory state.

Requests reserve enough KV pages for the prompt and declared maximum output.
This policy trades utilization for a simple invariant: an admitted request
cannot fail later because its declared decode length exhausts the runtime's KV
budget. Requests that do not fit remain queued while smaller requests may
proceed, avoiding strict head-of-line blocking.

The scheduler is deliberately accelerator-agnostic. A production runtime would
connect each trace step to backend-specific kernels, streams, memory managers,
and distributed coordination.

## Replay

Replay inputs contain scheduler configuration and timestamped requests. The
runner preserves input order for equal arrival times and emits a stable FNV-1a
fingerprint over the serialized trace. Replaying the same input should produce
the same admission, decode, completion, and memory sequence.

## Release Gate

The release evaluator joins baseline and candidate observations by request ID.
It computes:

- request coverage;
- exact output-fingerprint mismatch rate;
- error-rate increase;
- successful-request p95 latency; and
- candidate p95 regression.

Correctness or reliability regressions produce `rollback`. Latency regressions
or incomplete evidence produce `hold`. A complete candidate within policy
produces `promote`.

This is a local validation component, not a deployment controller. Production
integration would obtain observations from mirrored traffic, canary
populations, telemetry, and an audited rollout system.

