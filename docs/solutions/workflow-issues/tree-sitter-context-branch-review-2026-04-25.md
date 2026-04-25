---
title: "tree-sitter-context experimental branch review checkpoint"
date: 2026-04-25
category: workflow-issues
module: tree-sitter-context
problem_type: workflow_issue
component: development_workflow
severity: medium
applies_when:
  - Reviewing an experimental RFC/prototype branch before deciding what to fix, defer, or document
  - Promoting tree-sitter-context from prototype to public CLI or Rust API contract
  - Checking whether agent-facing context output is explainable, budgeted, and incrementally updatable
tags:
  - tree-sitter-context
  - branch-review
  - rfc
  - experimental-prototype
  - verification
  - invalidation
  - cli-contract
  - agent-context
---

# tree-sitter-context branch review checkpoint

This is a compound-engineering review checkpoint for the experimental `rfc-tree-sitter-context` branch. It preserves the review evidence and risk assessment; the canonical RFC remains [docs/plans/tree-sitter-context-rfc-2026-04-24.md](../../plans/tree-sitter-context-rfc-2026-04-24.md), and the follow-up queue lives in [docs/plans/tree-sitter-context-follow-up-plan-2026-04-25.md](../../plans/tree-sitter-context-follow-up-plan-2026-04-25.md).

The original draft lived at `docs/rfc-tree-sitter-context-branch-review.md`. It was moved here because the file is an internal branch review artifact, not public mdBook documentation.

## Branch Snapshot

- Date reviewed: 2026-04-25
- Branch: `rfc-tree-sitter-context`
- Base: `15154504de1e82cba8ddc956097cdbc405ff8ded` (`master` at review time)
- Reviewed HEAD: `e1d8b64080bfcfb1d29d134897d9f66e80c35b48`
- Scope: current branch diff plus this review artifact
- Branch shape at review time: 12 commits, 23 tracked files changed, about 3.8k added lines

## Intent

The branch moves `tree-sitter-context` from RFC to an experimental Rust crate and CLI prototype. It adds `crates/context/`, exposes `tree-sitter context` / `tree-sitter ctx`, and explores semantic chunks, stable chunk identity, old/new snapshot invalidation, symbols output, budget bundles, and a smoke benchmark.

The product thesis is still invalidation-first: a useful agent context primitive should tell an agent which semantic chunks changed, why they were classified that way, what confidence to assign, and which context was omitted.

## What Landed

- New `tree-sitter-context` workspace crate under `crates/context/`.
- Canonical JSON output types for context, invalidation, chunks, symbols, diagnostics, and confidence.
- AST chunking by tree-sitter syntax boundaries such as functions, methods, structs, enums, traits, impls, modules, and macros.
- Stable identity prototype for named and unnamed chunks.
- Snapshot invalidation that combines chunk matching with source comparison and changed-range metadata.
- Symbol extraction through `tree-sitter-tags`.
- Budgeted bundling API.
- Smoke benchmark example with Rust fixtures.
- Main CLI integration with `context` and `ctx` commands.
- RFC and deferred follow-up documentation in `docs/plans/tree-sitter-context-rfc-2026-04-24.md` and `docs/plans/tree-sitter-context-follow-up-plan-2026-04-25.md`.

## Verification Recorded

| Command | Result | Notes |
| --- | --- | --- |
| `git diff --check 15154504de1e82cba8ddc956097cdbc405ff8ded..HEAD` | Passed | Immutable base avoids drift when `master` moves. |
| `cargo test -p tree-sitter-context` | Passed in this review run | 24 unit tests passed. |
| `cargo check -p tree-sitter-cli` | Passed in this review run | Type-checks the CLI path only; it is not executable CLI coverage. |
| `cargo run -p tree-sitter-context --example smoke_benchmark` | Passed in the original review run | Produced benchmark markdown. |
| `cargo clippy -p tree-sitter-context --all-targets` | Passed with warning in the original review run | New crate lacks `package.readme` metadata. |
| `cargo clippy -p tree-sitter-context --all-targets -- -D warnings` | Failed in the original review run | `clippy::cargo-common-metadata` promoted the missing readme warning to an error. |

The smoke benchmark showed millisecond-level invalidation latency, but JSON output was still larger than raw source in the sample run. The prototype proves structure and invalidation flow; it does not yet prove token-efficiency.

## Primary Review Findings

### P1 -- Fix Before Promotion

| # | File | Issue | Why it matters |
| --- | --- | --- | --- |
| 1 | `crates/context/src/invalidation.rs` | Snapshot invalidation uses `Tree::changed_ranges` on independently parsed old/new trees. | Without an edited old tree or separate text diff, changed ranges can be incomplete or misleading. Snapshot mode should compute textual ranges, use proper incremental parsing, or clearly mark the result as synthesized/degraded. |
| 2 | `crates/context/src/identity.rs` | `StableId` uses `DefaultHasher` and does not disambiguate repeated names. | Public chunk identity must be stable across Rust versions/platforms and must not collapse repeated methods or same-name declarations. `match_chunks` currently stores chunks in a `HashMap`, so duplicate stable IDs overwrite earlier chunks. |
| 3 | `crates/context/src/chunk.rs` | `estimated_tokens` is capped at `max_tokens`. | Bundling depends on true token estimates. Capping the measurement makes oversized chunks look budget-safe and can violate the budget contract. |

### P2 -- Contract And Agent-Use Gaps

| # | File | Issue | Why it matters |
| --- | --- | --- | --- |
| 1 | `crates/cli/src/context.rs` | `--budget` is accepted but not applied. | Users get the same style of output with or without a budget, which breaks the user-visible CLI contract. |
| 2 | `crates/cli/src/context.rs` | `--quiet` is accepted but still writes the main JSON output. | Script users cannot rely on quiet mode to suppress stdout. |
| 3 | `crates/cli/src/main.rs` / `crates/cli/src/context.rs` | `--grammar-path` is accepted by `ContextCmd` but effectively ignored. | The command implies grammar path control, but language lookup still relies on configured filename mappings. |
| 4 | `crates/context/src/schema.rs` / `crates/context/src/invalidation.rs` | Invalidation output is not explainable enough for agents. | Buckets such as `affected`, `added`, and `removed` lack per-chunk reason, match strategy, old/new relationship, and invalidation-specific confidence. |
| 5 | `crates/context/src/schema.rs` / `crates/cli/src/context.rs` | Diagnostics are free-form, and loader failures happen outside JSON output. | Agents need stable diagnostic codes and suggested fixes to branch on recoverable failures. |
| 6 | `crates/context/src/invalidation.rs` | Invalidation drops chunking diagnostics from old/new chunk passes. | Fallback-to-whole-file or max-chunk-limit diagnostics are lost, so agents may treat degraded invalidation as precise. |
| 7 | `crates/context/src/chunk.rs` | Parse errors can still produce `Exact` chunks without diagnostics. | Invalid source should downgrade confidence and expose parser error ranges. |
| 8 | `crates/context/src/identity.rs` | Invalidation result ordering is nondeterministic. | Iterating randomized `HashMap`s leaks nondeterminism into public JSON and tests. |
| 9 | `crates/context/src/bundle.rs` | `BundleOutput` is not yet a stable public contract. | It lacks versioned metadata, source/target metadata, relevance reasons, and a clear definition of whether the budget applies to code tokens or full serialized output. |
| 10 | `crates/context/Cargo.toml` | New crate lacks `package.readme` metadata. | `clippy -D warnings` fails under the workspace lint setup. |
| 11 | `crates/cli/src/context.rs` and symbols path | New CLI behavior has no executable integration tests. | `cargo check` only type-checks; there is no coverage for `context`/`ctx`, `--old`, `--symbols`, `--budget`, `--quiet`, stdout shape, loader/config behavior, or grammar-path behavior. |

### P3 -- Low-Risk Cleanup

| # | File | Issue | Why it matters |
| --- | --- | --- | --- |
| 1 | `crates/context/examples/smoke_benchmark.rs` | Header comment says `--bin smoke_benchmark`; the target is an example. | The copyable command should be `cargo run -p tree-sitter-context --example smoke_benchmark`. |
| 2 | `crates/context/src/schema.rs` | Schema tests are substring checks, not real snapshots. | The core value is a machine-readable contract; fixed pretty-JSON snapshots should cover context, invalidation, bundle, diagnostics, and symbol records. |
| 3 | `crates/context/src/symbols.rs` | `SymbolOptions.max_docs_len` is ignored. | Long docs can exceed the intended symbol payload bound. |
| 4 | `crates/context/src/invalidation.rs` | Edit-stream confidence downgrade skips removed chunks. | All edit-stream classifications should share degraded confidence or expose confidence at the classification level. |
| 5 | `docs/plans/tree-sitter-context-rfc-2026-04-24.md` | The RFC draft had a local restore-point path before frontmatter. | Durable repository docs should not include machine-local absolute paths, and frontmatter should start at the top of the file. |

## Testing Gaps To Close

- Add CLI integration tests that execute `context` and `ctx` against fixture grammars.
- Assert JSON output for normal chunking, `--old` invalidation, `--symbols`, `--budget`, `--quiet`, loader/config behavior, and grammar-path behavior.
- Add stable-ID collision tests with repeated method names in different `impl` blocks.
- Add token-estimate and bundle regression tests proving oversized chunks are omitted rather than miscounted.
- Add invalidation tests for independent snapshot parsing, rename/add/remove cases, deterministic JSON ordering, and parse-error confidence downgrade.
- Add symbol extraction tests for definitions, references, docs, malformed tags, `max_symbols`, and `max_docs_len`.
- Replace schema substring tests with fixed JSON snapshots for all public output shapes.

## Related Documentation

- [docs/plans/tree-sitter-context-rfc-2026-04-24.md](../../plans/tree-sitter-context-rfc-2026-04-24.md) -- canonical RFC and current implementation state.
- [docs/plans/tree-sitter-context-follow-up-plan-2026-04-25.md](../../plans/tree-sitter-context-follow-up-plan-2026-04-25.md) -- deferred follow-up queue, including the remediation items from this checkpoint.
