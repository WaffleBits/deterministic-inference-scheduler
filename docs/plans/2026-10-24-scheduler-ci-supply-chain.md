# Plan: enforce supply-chain checks in scheduler CI

Date checked: 2026-10-24
Repository: `WaffleBits/deterministic-inference-scheduler`
Base: `978a7855140c7a42ce923c993525c98c4cbe4869`
Branch: `security/scheduler-supply-chain-20261024`

## Current role sample

Checked on 2026-10-24 against official Greenhouse/Ashby feeds. Listings below were present as active/listed in their official feeds. Greenhouse dates are the feed's `updated_at`; Ashby exposes `publishedAt`. Pay is copied from the listing; equity is not valued.

| Role | Official listing; date | Compensation as posted | Location/work mode | Repeated requirements relevant here |
|---|---|---|---|---|
| Anthropic — Performance Engineer, Inference Systems | [Greenhouse](https://job-boards.greenhouse.io/anthropic/jobs/5224564008); first published 2026-05-20, feed updated 2026-08-21 | Annual Salary: `$350,000 — $850,000 USD` | San Francisco, New York City, or Seattle; Anthropic's listing says staff are expected in an office at least 25% of the time | Inference systems correctness, throughput/latency/reliability, cross-layer investigation, and repeatable performance evaluation. |
| Anduril — Senior Software Engineer, Distributed Systems (Rust) | [Greenhouse](https://boards.greenhouse.io/andurilindustries/jobs/5201924007?gh_jid=5201924007); first published 2026-08-03, feed updated 2026-10-23 | US Salary Range: `$191,000 — $254,000 USD` (base salary; equity is described separately) | Seattle, Washington; remote eligibility is not exposed | Rust/systems programming, distributed networking, deterministic simulation, property-based testing, and observability. |
| Render — Software Engineer, Infrastructure (all levels) | [Ashby](https://jobs.ashbyhq.com/render/d52dc923-1641-4386-b8ba-5c15e1d7028d); published 2025-10-13; `isListed: true` in the feed checked 2026-10-24 | `$170K – $290K • Offers Equity` | Remote: United States (also Canada) | Cloud infrastructure security/reliability, Kubernetes or similar orchestration, IaC, observability, load testing, and chaos engineering. |

This sample is a market signal, not a claim of role fit or production-scale ownership. Across the sample, correctness, repeatable testing, and secure/reliable infrastructure recur.

## Live public evidence inventory

GitHub API access confirmed the account is `WaffleBits`. The public profile repository [WaffleBits/WaffleBits](https://github.com/WaffleBits/WaffleBits) is an unarchived Astro/TypeScript site. Its README and pinned portfolio link to the current scheduler and other relevant work. The newest relevant code repository is `deterministic-inference-scheduler` (Rust; last pushed 2026-10-23); other strong public evidence includes `triton-inference-benchmark`, `secure-gpu-inference-gateway`, and `triton-kernel-lab`.

The target repository already has `Cargo.lock`, deterministic scheduling/replay, promote/hold/rollback fixtures, and 128 bounded generated property cases. Its public CI at `.github/workflows/ci.yml` runs rustfmt, Clippy, tests, and CLI fixtures. It currently references `actions/checkout@v4` and `dtolnay/rust-toolchain@stable`, and does not audit the locked Rust dependency graph. The previous boundary-fix work reports a local RustSec database fetch blocked by host disk capacity; an OSV fallback is not a RustSec audit and is not represented as one.

## Evidence map and one selected gap

**Already demonstrated:** scheduler logic and bounded synthetic invariant tests; a working Rust CI job with lint, tests, and CLI fixtures.

**Present but buried:** the dependency graph is committed in `Cargo.lock`, but CI does not run a security advisory audit against it.

**Missing:** CI has no automated locked-dependency advisory gate, and its external Actions references are mutable tags.

**Exactly one selected improvement:** harden this existing scheduler CI supply-chain boundary: pin its external Actions to reviewed full commit SHAs, run a version-pinned `cargo-audit` against `Cargo.lock`, and add a small stdlib policy test that prevents either safeguard from silently disappearing. This is one coherent secure-CI improvement; it does not claim high-volume CI, production operation, or production scheduler traces. Extend the scheduler repository rather than create another project. Do not change profile/resume claims.

## Acceptance plan

1. Add policy tests first and confirm the current workflow fails because its action refs float and it has no locked dependency audit.
2. Pin both workflow Actions to the commit SHAs resolved from their official refs. Install `cargo-audit` at an exact crate version with its packaged lockfile, then audit the repository's `Cargo.lock` in CI.
3. Run the policy tests and the canonical Rust checks in the repository's `rust:1-bookworm` environment: `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-targets`, release replay and promote/rollback/numeric-tolerance CLI fixtures, and `cargo audit --file Cargo.lock`. Assert CLI outputs and run `git diff --check`.
4. Push a feature branch, open a PR, wait for the repository's CI to pass, merge only when green, then verify the merged workflow and post-merge CI run from GitHub. Keep generated `target/` output untracked.

## Implementation and local verification

- Added `tests/test_ci_supply_chain.py` before changing CI. Against the original workflow, its repository-policy test failed on both mutable action refs and the absent audit install/command. After pinning and adding the audit steps, all four policy tests pass, including the actual repository workflow plus negative fixtures for floating refs and a missing/unpinned audit.
- `.github/workflows/ci.yml` now pins `actions/checkout` to the reviewed v6 commit `d23441a48e516b6c34aea4fa41551a30e30af803` and `dtolnay/rust-toolchain` to the verified stable-action commit `b725eda525362e7cf34af9a7243cac1e3f01a7ec`. CI installs `cargo-audit` 0.22.2 with the crate's lockfile and audits the project's `Cargo.lock`.
- `actionlint` and `git diff --check` pass.
- Canonical project checks ran in the cached `rust:1-bookworm` image (Rust/Cargo 1.99.1): `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-targets` passed. The integration suites reported 7 adapter, 5 release, and 7 scheduler tests passing (19 total, including 128 generated cases).
- Release CLI fixtures ran successfully. Parsed outputs confirmed replay completed 4/4 requests and returned all KV pages; the safe gate returned `promote`, the bad gate returned `rollback`, and the numeric-tolerance gate returned `promote`.
- **Local audit limitation:** I started the exact `cargo install --locked cargo-audit --version 0.22.2` path in the Rust container, but compilation was still in progress after 7m35s and the host had about 891 MB free. I stopped only that temporary build to avoid exhausting this nearly full host. No local `cargo audit` success is claimed. The pushed PR's GitHub CI must pass the installed tool and audit command before merge; otherwise the workflow will be corrected or the change left unmerged.
- No profile/resume claims or production-scale claims changed. Rust build outputs are outside the repository in `/tmp`.

## Pull request and publication results

_To be completed after GitHub Actions and merge/read-back verification._
