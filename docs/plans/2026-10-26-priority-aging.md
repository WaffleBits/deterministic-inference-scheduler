# Plan: bound priority starvation in the inference scheduler

Date checked: 2026-10-26
Repository: `WaffleBits/deterministic-inference-scheduler`
Base commit: `7dbb7fcb3096c42d85371f5d024e46b15fd38cb9`
Branch: `feat/scheduler-priority-aging-20261026`

## Current role sample

Checked 2026-10-26 against official Greenhouse job feeds. All three listings were present in their companies' active feeds when checked. Greenhouse exposes `updated_at`, not an original publication date; those values are labeled as feed updates. Compensation and locations are copied from the listings. Remote eligibility is not inferred.

| Role | Official listing; feed updated | Compensation as listed | Location / work mode | Relevant recurring requirements |
|---|---|---|---|---|
| Anthropic — Performance Engineer, Inference Systems | [Greenhouse](https://job-boards.greenhouse.io/anthropic/jobs/5224564008); 2026-08-21 12:50:15 -04:00 | Annual Salary: `$350,000 — $850,000 USD` | San Francisco, New York City, or Seattle; the listing says staff are expected in an office at least 25% of the time | Profiling and root-cause investigation; latency/throughput optimization; correctness and regression detection; inference reliability, tail latency, and observability. |
| Point72 — GenAI Security Engineer | [Greenhouse](https://boards.greenhouse.io/point72/jobs/8399360002?gh_jid=8399360002); 2026-10-22 12:15:32 -04:00 | Annual base salary `$225,000-$300,000 (USD)`, excluding discretionary bonus and benefits | New York, NY; no remote classification exposed in the feed | Production-ready GenAI security controls; infrastructure/application pipelines; control testing, privacy-by-design, threat modeling, and audit-ready evidence. |
| Robinhood — Staff Software Engineer, AI Security | [Greenhouse](https://boards.greenhouse.io/robinhood/jobs/8214383?t=gh_src=&gh_jid=8214383); 2026-10-06 12:11:22 -04:00 | Zone 1 `$230,000 — $270,000 USD`; Zone 2 `$203,000 — $238,000 USD`; Zone 3 `$180,000 — $211,000 USD`, plus bonus/equity eligibility | Bellevue, WA or Menlo Park, CA; posting specifies at least three office days per week | Secure AI infrastructure, identity/access controls, cloud/distributed systems, reliability automation, and reusable security capabilities. |

The sample is a market signal, not a claim of role fit or production ownership. Across these listings, testable correctness, reliable serving, and controls that can be explained from evidence recur. The Anthropic listing specifically emphasizes latency/throughput and tail-latency diagnosis.

## Live public evidence inventory

The authenticated GitHub API confirmed the `WaffleBits` account has 21 public repositories. Its public profile describes AI infrastructure, GPU performance, secure distributed systems, and mission software. The unarchived Astro profile repository is [WaffleBits/WaffleBits](https://github.com/WaffleBits/WaffleBits); its README and portfolio data link the following relevant public work:

- [deterministic-inference-scheduler](https://github.com/WaffleBits/deterministic-inference-scheduler), Rust: deterministic continuous-batching and paged-KV scheduling, replayable traces, release gates, and 128 bounded generated invariant cases. At the checked base, its admission queue orders strictly by configured priority and stable submission sequence.
- [triton-inference-benchmark](https://github.com/WaffleBits/triton-inference-benchmark), Python: authenticated serving benchmarks, repeatable local fixtures, privacy-safe reports, and reliability gates.
- [secure-gpu-inference-gateway](https://github.com/WaffleBits/secure-gpu-inference-gateway), Python: mock-backed inference access controls, budgets, audit logging, and observability.
- [triton-kernel-lab](https://github.com/WaffleBits/triton-kernel-lab), Triton/Python: correctness oracles and measured hardware benchmarks.

The scheduler's own README, source, tests, Cargo lockfile, and CI workflow were read from the live public repository and local checkout at `7dbb7fc`. Its public CI runs formatting, Clippy, tests, dependency audit, and replay/release-gate CLI fixtures. The worktree was clean and matched `origin/main`; no unrelated local changes were present.

## Evidence map and one selected gap

**Already demonstrated:** stable priority-aware admission, bounded prefill work, round-robin decode, deterministic replay, KV-capacity checks, release gates, and generated tests for replay/accounting and capacity invariants on synthetic workloads.

**Present but buried:** priority and stable-order behavior are implemented in the scheduler, but there is no fixture or policy for a low-priority request waiting behind an ongoing stream of newly arriving higher-priority work.

**Missing:** the queue uses a static priority key. Under a sustained higher-priority arrival stream, an older lower-priority request can remain queued until that stream ends. Existing properties prove accounting/capacity, not bounded queue starvation.

**Exactly one selected improvement:** add configurable, deterministic priority aging to the existing Rust scheduler. A waiting request's effective priority increases by one per configured aging interval, saturating at the maximum priority; existing submission order remains the tie-breaker. Add a controlled synthetic CLI fixture and tests proving an older low-priority request is admitted while higher-priority arrivals continue. This targets queue fairness and tail behavior relevant to the sampled inference-performance role. It will not claim production traces, production starvation bounds under real traffic, GPU execution, or measured performance gains. No new repository or profile/resume claims are needed.

## Acceptance plan

1. Add the starvation regression test and synthetic replay fixture first; run it against the current code and record the expected failing assertion.
2. Add `priority_aging_interval_ticks` to scheduler configuration with a nonzero validation rule and a serde default so existing JSON fixtures remain readable. Recompute effective queue priorities at each admission tick from request age, cap the age boost safely, and preserve the stable sequence tie-breaker.
3. Assert behavior both through the library test and the real `runtime-lab replay` CLI output. Add the new configuration semantics and synthetic-workload boundary to the README/architecture documentation.
4. Run the canonical CI checks in the cached `rust:1-bookworm` container: `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-targets`, `cargo audit --file Cargo.lock`, and release replay/release-gate CLI fixtures. Run the new aging fixture through the CLI, assert its output, and run `git diff --check`.
5. Push a feature branch without force, open a PR, require scheduler CI to pass, merge only when green, then read back the merged source and successful post-merge CI. This Rust repository has no Pages deployment. Do not modify the portfolio or resume.

## Implementation and local verification

- The new scheduler regression failed before changing the implementation: the low-priority request was not admitted before the continuing higher-priority arrival stream ended. After implementation, the test passes at tick 4 while higher-priority requests continue arriving through tick 12.
- `SchedulerConfig` now has `priority_aging_interval_ticks`, defaulting to 32 for existing JSON inputs and rejecting zero. At each admission tick, queued requests are re-keyed using saturating priority age; original sequence remains the stable tie-breaker. The replay report schema is now version 4 because the serialized config includes the new field.
- Added a 14-request synthetic replay fixture, a library test for the policy, a legacy-input default test, a zero-interval validation test, and a CLI-output validator. README/architecture docs describe the policy and explicitly bound the evidence to synthetic fixtures.
- In the cached `rust:1-bookworm` container (Rust/Cargo 1.99.1), `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-targets` passed. Tests reported 7 adapter, 5 release, and 10 scheduler tests passing; the existing 128 generated replay/property cases also passed.
- The real `runtime-lab replay` CLI wrote and parsed the new priority-aging artifact: the low-priority request was admitted at tick 4 before the last higher-priority arrival at tick 12, and all 14 requests completed. The existing replay CLI fixture emitted schema version 4; release-gate fixtures returned `promote`, `rollback`, and `promote` for safe, bad, and numeric-tolerance inputs respectively.
- `python3 tests/test_ci_supply_chain.py` passed all 4 policy tests; `git diff --check` passed. No generated build output is tracked.
- **Local dependency-audit limitation:** `cargo audit --file Cargo.lock` was not run locally. The host filesystem is full (479 MB available at check time), and installing the pinned `cargo-audit` crate would risk exhausting it. The PR's GitHub CI audit must pass before merge; no local audit success is claimed.

## Pull request and post-merge verification

Pending commit, remote CI, review, merge, and post-merge CI read-back.
