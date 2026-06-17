# Release Validation

The gate is designed around three different operational responses.

## Promote

Use `promote` when:

- baseline and candidate request coverage is complete;
- output fingerprints stay within the configured mismatch budget;
- configured numeric tolerances cover any expected model/backend output drift;
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

## Production Extension Points

A real rollout system should add:

- statistically meaningful sample sizes and confidence intervals;
- prompt-class and region segmentation;
- SLO burn-rate and saturation signals;
- canary population controls and audited rollback execution; and
- provenance linking every decision to build, model, and configuration IDs.

The checked fixtures are synthetic and exist to make the policy executable in
CI. They are not claims about production traffic or fleet scale.
