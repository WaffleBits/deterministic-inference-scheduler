# Release Validation

The gate is designed around three different operational responses.

## Promote

Use `promote` when:

- baseline and candidate request coverage is complete;
- output fingerprints stay within the configured mismatch budget;
- the candidate error-rate increase stays within policy; and
- candidate p95 latency stays within the configured regression budget.

## Hold

Use `hold` when the evidence is incomplete or performance needs investigation.
Examples include missing shadow requests, too few matched samples, no successful
output pairs, or a p95 latency regression without a correctness failure.

## Rollback

Use `rollback` when the candidate changes successful outputs beyond policy or
increases the request error rate beyond policy. These checks intentionally take
precedence over latency.

## Production Extension Points

A real rollout system should add:

- statistically meaningful sample sizes and confidence intervals;
- model-aware numeric tolerance rather than exact fingerprints where required;
- segmented checks by model, hardware, prompt class, and region;
- SLO burn-rate and saturation signals;
- canary population controls and audited rollback execution; and
- provenance linking every decision to build, model, and configuration IDs.

The checked fixtures are synthetic and exist to make the policy executable in
CI. They are not claims about production traffic or fleet scale.

