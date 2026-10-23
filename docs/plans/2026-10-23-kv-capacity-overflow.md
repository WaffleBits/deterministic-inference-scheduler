# Plan: widen scheduler KV accounting at integer boundaries

Date checked: 2026-10-23
Repository: `WaffleBits/deterministic-inference-scheduler`
Branch: `fix/scheduler-kv-overflow-20261023`

## Current role sample

Checked 2026-10-23 against official Greenhouse feeds. The dates below are the feed's `updated_at` values, not original publication dates. Each listing was present in its official feed when checked. Salary/location details are reproduced as posted; remote eligibility is not inferred.

| Role | Official listing | Feed updated | Compensation as listed | Location / work mode | Recurring requirements relevant here |
|---|---|---|---|---|---|
| Anthropic — Performance Engineer, Inference Systems | https://job-boards.greenhouse.io/anthropic/jobs/5224564008 | 2026-08-21 12:50:15 -04:00 | Annual salary `$350,000 — $850,000 USD` | San Francisco, New York City, or Seattle; the listing says staff are expected in an office at least 25% of the time | Inference systems, throughput/latency/reliability/correctness, correctness evaluation, telemetry, and regression detection. |
| Anduril — Senior Software Engineer, Distributed Systems (Rust) | https://boards.greenhouse.io/andurilindustries/jobs/5201924007?gh_jid=5201924007 | 2026-10-21 15:41:24 -04:00 | US salary range `$191,000 — $254,000 USD` (base salary; equity is separately described) | Seattle, Washington; remote eligibility not stated | Rust/systems programming, distributed networking, deterministic simulation, and preferred property-based testing. |
| Gemini — Staff Platform Engineer | https://job-boards.greenhouse.io/gemini/jobs/8186546 | 2026-10-02 14:40:35 -04:00 | New York State base salary `$168,000 - $240,000`, excluding discretionary bonus and equity | New York, Miami, or Remote (USA); US hub/hybrid expectations vary by role | Production infrastructure and distributed systems, high availability/recovery, safe delivery, observability, and SLO/error-budget decisions. |

The common evidence signal is disciplined correctness and predictable behavior at system boundaries. This sample is a market signal, not a claim of candidate fit or production ownership.

## Live public evidence inventory

Authenticated GitHub API access as `WaffleBits` confirmed 21 public repositories. The public, unarchived Astro profile is https://github.com/WaffleBits/WaffleBits and links the Rust scheduler. The most relevant public artifacts are:

- [`deterministic-inference-scheduler`](https://github.com/WaffleBits/deterministic-inference-scheduler): Rust continuous-batching and paged-KV scheduling, deterministic replay, and release-gate fixtures. Its current public CI runs formatting, Clippy, tests, and replay/gate CLI fixtures.
- [`triton-inference-benchmark`](https://github.com/WaffleBits/triton-inference-benchmark): Python inference-serving benchmark with authenticated agents, failure accounting, and privacy-safe reports.
- [`secure-gpu-inference-gateway`](https://github.com/WaffleBits/secure-gpu-inference-gateway): mock-backed inference gateway with policy, audit, and operational evidence.

The scheduler's 128 generated property cases check replay/accounting, trace consistency, and active/KV limits on bounded synthetic inputs. The code computed per-request reserved pages with saturating `u32` prompt-plus-output addition and tested aggregate capacity with saturating `u32` addition. The bounded generator did not reach arithmetic boundaries.

## Evidence map and selected gap

**Already demonstrated:** deterministic replay and admission/accounting invariants over bounded synthetic inputs, plus passing public CI at `d53fc0ec5a380f9ae14ccfe8d36ff8dc4b66c50a` (run [39598902096](https://github.com/WaffleBits/deterministic-inference-scheduler/actions/runs/39598902096)).

**Present but buried:** no boundary-specific correctness evidence was exposed in the tests or README.

**Missing:** exact request-token sums near `u32::MAX` could be undercounted by saturating addition; aggregate reservations could pass a saturating capacity comparison and overflow the stored page count. Those cases were not in the existing bounded property generator.

**Exactly one selected improvement:** widen prompt-plus-output arithmetic before page rounding and use checked addition for the aggregate KV-capacity decision, with regression tests for both failure modes. Keep the scheduler as the implementation home; do not create a duplicate repository or change profile/resume positioning for this narrow correctness fix. This does not establish production traces, GPU execution, performance gains, or high-scale service operation.

## Acceptance plan

1. Add regression tests first and demonstrate they fail against the current implementation.
2. Compute page requirements using widened `u64` token arithmetic; reject a single request whose exact reservation exceeds configured capacity; use checked aggregate addition before admission. Update the generated-test oracle to use widened arithmetic as well.
3. Document the checked boundary behavior without claiming production-scale guarantees.
4. Run the repository checks in `rust:1-bookworm`: `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-targets`, release replay and promote/rollback/numeric-tolerance CLI fixtures, `cargo audit` if available, and `git diff --check`.
5. Commit and push the feature branch, open a PR, require its CI to pass, merge only after green CI, then verify the merged source and CI read-back. No Pages deployment is involved in this Rust repository change.

## Implementation and local verification

- Before fixing runtime code, both new boundary tests failed against the prior implementation: the aggregate-capacity case panicked on `u32` overflow, and the over-range single-request case was incorrectly accepted. The pre-existing seven-test scheduler suite otherwise passed.
- Page reservations now widen prompt-plus-output arithmetic to `u64` before ceiling division. Validation rejects a request whose exact page count exceeds configured capacity. Admission uses `checked_add` before accepting an aggregate reservation. The 128-case generator/oracle now also uses widened arithmetic.
- Added the two boundary regressions and documented the behavior in the README. The public profile and resume were not changed; this is a narrow implementation correction, not a new claim of production operation.
- In `rust:1-bookworm`, `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-targets` passed. Tests reported 7 scheduler, 7 adapter, and 5 release-gate tests passing, including the 128 generated scheduler cases.
- The release CLI replay fixture completed 4/4 requests and returned all KV pages. CLI gate fixtures emitted `promote` for safe input, `rollback` for bad output, and `promote` for numeric-tolerance input with 4 tolerated numeric outputs. Parsed output artifacts were asserted.
- An attempted `cargo audit` installed successfully, but fetching the RustSec advisory database failed with `No space left on device` on this host. As a read-only fallback, an exact-version OSV query checked all 27 registry packages in `Cargo.lock` and returned zero advisories. This fallback is not reported as a successful `cargo audit`; a full RustSec audit remains blocked by host disk capacity.
- `git diff --check` passed. The container used a temporary target directory for the successful full check run.
