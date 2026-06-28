# Release Validation

The gate is designed around three different operational responses.

## Promote

Use `promote` when:

- baseline and candidate request coverage is complete;
- output fingerprints stay within the configured mismatch budget;
- configured numeric tolerances cover any expected model/backend output drift;
- the candidate error-rate increase stays within policy; and
- candidate p95 latency, TTFT p95, decode-token p95, and KV memory pressure
  stay within the configured regression budgets.
- required candidate route/scheduler provenance and streaming token traces are
  complete when those checks are enabled.

## Hold

Use `hold` when the evidence is incomplete or performance needs investigation.
Examples include missing shadow requests, too few matched samples, no successful
output pairs, or a p95 latency regression without a correctness failure.
The same response is used for excessive TTFT regression, decode-token p95
regression, or candidate KV memory pressure because those are operational
signals that need investigation before rollout.
If route/scheduler provenance or streaming token traces are required but
missing, the gate also returns `hold`; the candidate may still be correct, but
the rollout evidence is not complete enough to trust the serving path.

## Rollback

Use `rollback` when the candidate changes successful outputs beyond policy or
increases the request error rate beyond policy. These checks intentionally take
precedence over latency.

## Numeric Tolerance

Some accelerator backends legitimately produce small numeric differences even
when the generated output is acceptable. `numeric_tolerances` allow those
comparisons to be explicit instead of hidden behind broad fingerprint waivers.
Each tolerance can be scoped by model, candidate backend, and candidate
accelerator.

When fingerprints differ, the gate promotes only if both observations provide
numeric output vectors of equal length and every value stays within either the
absolute or relative tolerance. Failed numeric comparisons count as output
mismatches and as numeric drift.

The report includes:

- `numeric_pairs` and `tolerated_numeric_outputs`;
- `numeric_drift_rate`;
- maximum absolute and relative numeric error; and
- segment summaries by model, baseline backend, candidate backend, and
  accelerator.

## Operational Telemetry

Mirrored observations can include rollout context and token-path telemetry:

- model version;
- route ID, replica ID, and scheduler policy;
- queue depth;
- KV pages used and available;
- time to first token;
- per-token decode latencies; and
- streaming token events and token-trace fingerprints.

The gate reports aggregate and per-segment TTFT p95, decode-token p95, maximum
candidate queue depth, maximum candidate memory pressure, candidate
route/scheduler provenance coverage, candidate streaming-trace coverage,
candidate route count, scheduler policies, and token-trace mismatch rate.
Correct outputs with excessive latency, memory pressure, missing provenance, or
missing streaming traces produce `hold`, not `rollback`, because the evidence
points to operational risk rather than a correctness failure.

## Triage Output

Every hold or rollback reason is mirrored into a structured `triage` item.
Each item records:

- the failed signal;
- the recommended response;
- an owner hint; and
- the next investigation action.

This keeps the human-readable `reasons` field intact while making deploy-gate
output easier to route through CI, incident tooling, or rollout dashboards.

## Backend Mirror Adapter

The `mirror` command normalizes request observations from backend-specific
serving traces into the release gate input format. It is intended for mirrored
baseline/candidate comparisons such as vLLM versus SGLang, or a current
runtime versus a candidate runtime behind shadow traffic.

Each observation records request ID, latency, health, model, backend,
accelerator, output material, and optional operational telemetry. Engines may
provide their own `output_fingerprint`; otherwise the adapter hashes output
token IDs, streaming token events, or numeric output vectors with a stable
FNV-1a fingerprint. Streaming token events also let the adapter derive TTFT and
decode-token gaps from elapsed timestamps. Successful observations without
output material are rejected because the release gate cannot audit correctness
from latency alone.

## Production Extension Points

A real rollout system should add:

- statistically meaningful sample sizes and confidence intervals;
- prompt-class and region segmentation;
- SLO burn-rate and saturation signals;
- canary population controls and audited rollback execution; and
- provenance linking every decision to build, model, route, scheduler, and
  configuration IDs.

The checked fixtures are synthetic and exist to make the policy executable in
CI. They are not claims about production traffic or fleet scale.
