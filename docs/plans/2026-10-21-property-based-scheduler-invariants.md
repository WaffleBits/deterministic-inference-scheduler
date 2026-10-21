# Plan: property-test scheduler replay invariants

Date: 2026-10-21
Repository: `WaffleBits/deterministic-inference-scheduler`
Branch: `test/property-based-scheduler-invariants-20261021`

## Current official role sample

Checked 2026-10-21 against active listings in official company Greenhouse feeds. The feeds expose `updated_at`, not original posting dates; each role below was returned by its company's live jobs API during this check. Compensation and location terms are copied from the listings; equity is not assigned an estimated value.

| Role | Canonical listing | Feed updated | Compensation as listed | Location / work mode | Relevant published requirements |
|---|---|---|---|---|---|
| Anthropic, Performance Engineer, Inference Engine | https://job-boards.greenhouse.io/anthropic/jobs/5418323008 | 2026-09-09 19:09:53 -04:00 | `Annual Salary: $350,000 — $850,000 USD` | San Francisco, CA or New York City, NY; company policy expects at least 25% in-office, with possible role-specific exceptions | Rust/systems programming, throughput/reliability/latency, profile before optimizing, and reproducibility through determinism, replay, and property-based tests. |
| Robinhood, Senior Staff Software Engineer, Developer Infrastructure | https://boards.greenhouse.io/robinhood/jobs/8189867?t=gh_src=&gh_jid=8189867 | 2026-10-11 03:14:59 -04:00 | `$264,000 — $310,000 USD` base pay (plus bonus, equity, and benefits) | Menlo Park, CA or New York, NY; posting requires office attendance at least three days per week | CI/CD and test infrastructure, distributed systems, Kubernetes/cloud, and safe pre-merge feedback. Its thousands-of-build scale, Bazel ownership, and staff-level scope are not established by this project. |
| Gemini, Staff Platform Engineer | https://boards.greenhouse.io/embed/job_app?for=gemini&token=8186546&gh_jid=8186546 | 2026-10-02 14:40:35 -04:00 | `$168,000 - $240,000` base for New York State; bonus/equity excluded | New York, NY; Miami, FL; or Remote (USA) | Secure platform defaults, service validation, reliability, and engineering-facing platform workflows; the listing requests 8+ years and production ownership. |

Across this sample, reliability and validation recur. The strongest direct signal for this repository is Anthropic's explicit requirement for Rust systems programming and deterministic, replayable, property-tested systems. The other roles reinforce tested and reliable platform behavior, not a claim that this project operates at their production scale.

## Public evidence inventory

The authenticated GitHub API confirmed account `WaffleBits` has 21 public repositories. The public profile repo is an Astro/TypeScript site. Relevant public projects include:

- [`deterministic-inference-scheduler`](https://github.com/WaffleBits/deterministic-inference-scheduler): Rust continuous-batching and paged-KV scheduler with deterministic replay, trace summaries, and synthetic promote/hold/rollback fixtures. Its existing tests cover a few hand-authored cases; the source tree contains no property-based testing dependency or generated scheduler-invariant tests.
- [`triton-inference-benchmark`](https://github.com/WaffleBits/triton-inference-benchmark): Python serving benchmark with authenticated agents, TLS, restart reconciliation, privacy-safe reports, and reliability fixtures.
- [`secure-gpu-inference-gateway`](https://github.com/WaffleBits/secure-gpu-inference-gateway): inference control-plane demo with security and observability checks; its capacity and resilience artifacts are explicitly synthetic.
- [`triton-kernel-lab`](https://github.com/WaffleBits/triton-kernel-lab): Triton kernels with FP32 correctness oracles and measured hardware artifacts.

The scheduler is the existing, narrowest fit for the Rust determinism/replay signal. Its CI already runs formatting, Clippy with warnings denied, all-target tests, a release replay, and release-gate fixtures. No new repository or user-facing profile claim is needed.

## Evidence map

**Already demonstrated:** scheduler admission, KV reservation, round-robin decode, deterministic replay, summary accounting, and release decisions have ordinary example-based tests. CI exercises replay and promote/rollback fixtures. All scheduler inputs and outcomes are synthetic.

**Present but buried:** trace-level accounting and scheduling bounds can be checked through the public replay API; tests currently cover only a small number of fixed traces.

**Missing:** generated valid workloads do not currently test cross-cutting invariants across varied configuration/request combinations. This does not establish live-serving correctness, GPU behavior, real workload representativeness, performance improvement, or production reliability.

## Exactly one selected improvement

Add bounded property-based tests for deterministic scheduler replay invariants in the existing Rust project. Generate valid small synthetic configurations and request sets, assert exact replay determinism and trace/accounting/capacity/completion invariants, and rely on shrinking for counterexamples. Keep implementation behavior and public report schema unchanged. Document the test boundary and run it through the existing CI commands.

## Acceptance plan

1. Add a Rust property-test dependency and bounded generated tests to `tests/scheduler.rs` before changing runtime code. The property should assert that two runs of the same generated input match, each request is admitted/completed once and decoded for its declared output count, trace indexes and counters reconcile, configured active/KV limits are respected, aggregate totals agree, and the terminal KV reservation is zero.
2. Run the new tests against the unchanged scheduler first. If they expose a defect, add the smallest fix and a focused regression; do not weaken the properties.
3. Add a concise README note describing generated invariants and the synthetic boundary. Do not change the portfolio profile, resume, runtime API, release schema, or performance claims.
4. Run the repository's full CI commands in a disposable Rust container because this Oracle Linux host has no Rust toolchain: `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-targets`, the release replay fixture, and all release-gate fixtures. Run `cargo audit` as an additional dependency security check if the tool is available; report any availability limitation explicitly.
5. Review the scoped diff, run `git diff --check`, commit, push without force, open a PR, and require the GitHub CI workflow to pass before merge. Read back the merged commit and passing CI result.

## Implementation and local verification

- Added `proptest` with only its `std` feature, a bounded generated replay invariant test in `tests/scheduler.rs`, and a README section describing what it checks and what it does not establish. No scheduler runtime logic, public API, report schema, profile, or resume was changed.
- Generated test inputs are valid small synthetic workloads. Across 128 property cases per test run, assertions check identical reports on repeat replay; one admission and completion per request; exact declared decode counts; tick ordering and counters; arrived-request state reconciliation; active-request/KV limits; summary totals; and final KV release. Proptest retains a shrunk failure seed next to the test.
- Baseline `cargo test --all-targets` passed before adding the generated test (16 existing tests). The property test then passed against the unchanged scheduler; no runtime fix was needed.
- Reproduced CI in `rust:1-bookworm` on the Oracle Linux 7/aarch64 host because Rust is not installed on the host. Container toolchain: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.
- `cargo fmt --all --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test --all-targets`: passed; 17 tests total, including the generated property test.
- Release replay CLI fixture passed and emitted a synthetic report with 4/4 requests completed in 11 ticks, peak 12 KV pages, and 0 final KV pages.
- Release-gate CLI fixtures passed with outcomes `promote` (safe), `rollback` (bad candidate), and `promote` (numeric tolerance).
- Additional `cargo-audit 0.22.2` scan of the final lockfile passed using the already-fetched RustSec advisory database: JSON reported 0 vulnerability findings and no warnings.

## Scope boundary

Property tests broaden deterministic fixture coverage only. They are not benchmark results, production traces, workload-fidelity evidence, GPU behavior, performance results, or proof of scale. No profile positioning or résumé wording was changed.
