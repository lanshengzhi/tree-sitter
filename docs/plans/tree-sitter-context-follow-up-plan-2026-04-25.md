---
title: "tree-sitter-context follow-up plan"
date: 2026-04-25
status: open
source_docs:
  - docs/plans/tree-sitter-context-rfc-2026-04-24.md
  - docs/solutions/workflow-issues/tree-sitter-context-branch-review-2026-04-25.md
---

# tree-sitter-context follow-up plan

## Deferred from `tree-sitter-context` Branch Review

See [tree-sitter-context branch review checkpoint](../solutions/workflow-issues/tree-sitter-context-branch-review-2026-04-25.md) for the full review evidence.

### Identity and invalidation correctness

- What: Replace `DefaultHasher`-based chunk identity with an explicitly stable algorithm and include enough disambiguation to distinguish repeated names.
- Why: Stable IDs are part of the public invalidation contract; duplicate IDs currently overwrite earlier chunks during matching.
- Context: Include normalized path, syntax kind, name, parent identity, disambiguator, and/or source anchor. Handle duplicate ID buckets by degrading confidence or emitting diagnostics instead of silently overwriting.

- What: Rework snapshot invalidation so changed ranges are reliable for independently supplied old/new files.
- Why: `Tree::changed_ranges` is meaningful for edited trees, but snapshot mode currently parses old and new independently and can produce incomplete/misleading range evidence.
- Context: Either compute textual changed byte ranges, edit the old tree before incremental reparse, or mark snapshot ranges as synthesized/degraded with explicit reason metadata.

- What: Preserve true `estimated_tokens` and let bundle selection omit oversized chunks.
- Why: Capping token estimates at `max_tokens` makes large chunks appear budget-safe and undermines the budget contract.
- Context: Add regression tests for large syntax nodes and bundle omission behavior.

### CLI and schema contract

- What: Decide and implement the user-visible behavior for `--budget`, `--quiet`, and `--grammar-path`.
- Why: These flags are accepted by the CLI but are either ignored or only partially wired.
- Context: If behavior is not ready, hide the flags until the contract is implemented.

- What: Add executable CLI integration tests for `tree-sitter context` and `tree-sitter ctx`.
- Why: `cargo check -p tree-sitter-cli` only type-checks the command path.
- Context: Cover normal JSON output, `--old`, `--symbols`, `--budget`, `--quiet`, stdout shape, loader/config behavior, and grammar-path behavior.

- What: Make invalidation and diagnostics agent-actionable.
- Why: Agents need per-chunk invalidation reasons, match strategy, confidence, stable diagnostic codes, and suggested fixes for recoverable failures.
- Context: Consider `InvalidationRecord`-style output and diagnostic fields such as `code`, `problem`, `cause`, `fix`, `docs_url`, `degraded_mode`, and `exit_code`.

- What: Replace schema substring tests with fixed JSON snapshots.
- Why: Public output shapes need contract tests that catch field, enum, and structure drift.
- Context: Cover `ContextOutput`, `InvalidationOutput`, `BundleOutput`, diagnostics, and symbol records.

### DX and package cleanup

- What: Add package readme metadata for `tree-sitter-context`.
- Why: `cargo clippy -p tree-sitter-context --all-targets -- -D warnings` fails on `clippy::cargo-common-metadata`.
- Context: Add `crates/context/README.md` and `readme = "README.md"`, or explicitly suppress the lint.

- What: Correct the smoke benchmark copyable command.
- Why: The target is an example, not a bin target.
- Context: Use `cargo run -p tree-sitter-context --example smoke_benchmark`.

- What: Add symbol extraction tests and enforce symbol option limits.
- Why: `symbols.rs` currently documents integration-test coverage but has no executable tests, and `SymbolOptions.max_docs_len` is ignored.
- Context: Cover definitions, references, docs, malformed tag diagnostics, `max_symbols`, and `max_docs_len`.

## Deferred from `/autoplan` Review

### Adapter validation track

- What: Add one thin real-agent adapter or integration spike after the first useful invalidation CLI exists.
- Why: CEO and Eng reviews both found that adoption proof is the main risk; a clean Rust primitive is not enough if no agent workflow uses it.
- Pros: Validates schema usability, output size, and integration friction before hardening the API.
- Cons: Pulls scope toward product work and may distract from primitive correctness if started too early.
- Context: Do this only after canonical JSON snapshots and `diff` / `invalidate` smoke benchmarks exist. Candidate adapters include a local CLI wrapper for an existing agent workflow or a minimal MCP proof that consumes the JSON output without becoming the product.
- Depends on / blocked by: Stable JSON schema, invalidation CLI, smoke benchmark.

### Multi-language expansion

- What: Add Python and TypeScript context fixtures after Rust v1 passes benchmark gates.
- Why: Agent adoption happens in mixed repositories; Rust-only success may not generalize.
- Pros: Catches grammar/query variance and schema assumptions early.
- Cons: Adds fixture and query complexity before the Rust proof is complete.
- Context: Keep Rust as the first benchmark language. Expand only after chunk identity, diagnostics, and invalidation semantics are stable enough to compare across languages.
- Depends on / blocked by: Rust v1 benchmark passing.

### Release and distribution pipeline

- What: Define how `tree-sitter-context` is built, packaged, and installed if it graduates from prototype.
- Why: A CLI/library without distribution cannot be adopted outside local development.
- Pros: Makes DX real and gives users a predictable install path.
- Cons: Premature release work can create maintenance burden before value is proven.
- Context: Current v1 validation can use local `cargo` workflows. Publishing, binary releases, or CLI integration should wait until benchmark and adapter evidence are positive.
- Depends on / blocked by: Final placement decision and benchmark proof.

### Hosted playground

- What: Consider a zero-install playground or recorded demo for `tree-sitter-context`.
- Why: DX review found that the magical moment currently requires local setup; a playground could reduce evaluation friction later.
- Pros: Faster evaluation and easier sharing of benchmark/invalidation examples.
- Cons: Product surface, hosting, and maintenance are too much before primitive proof.
- Context: Do not build this in v1. Revisit only after invalidation demo, JSON schema, and smoke benchmark are stable.
- Depends on / blocked by: Stable invalidation output and benchmark proof.

### Context-specific community channel

- What: Decide whether `tree-sitter-context` needs a dedicated discussion/support channel.
- Why: If external adapters appear, integration questions may not fit existing tree-sitter parser-development channels.
- Pros: Gives adopters a place to ask schema, benchmark, and adapter questions.
- Cons: Premature community surface creates support burden before adoption exists.
- Context: Not needed during prototype. Revisit after at least one external adapter or serious user.
- Depends on / blocked by: External adoption signal.
