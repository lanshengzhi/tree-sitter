---
title: "tree-sitter-context hardening implementation plan"
type: fix
status: active
date: 2026-04-25
origin: docs/plans/tree-sitter-context-follow-up-plan-2026-04-25.md
---

# tree-sitter-context hardening implementation plan

## Overview

This plan turns the remaining `tree-sitter-context` follow-up work into an implementation sequence. The goal is to harden the experimental context crate and CLI enough that its JSON output can be trusted by agent integrations: stable identity, explainable invalidation, honest budget behavior, structured diagnostics, and executable coverage for public CLI flags.

This is a hardening plan, not a product expansion. It keeps the existing prototype shape and fixes the contract gaps surfaced by the RFC and branch review.

---

## Problem Frame

The current branch proves that tree-sitter can emit chunks, symbols, invalidation buckets, and budget bundles. The review checkpoint found that the prototype is still unsafe to promote because several public-facing behaviors can silently mislead agents:

- chunk identity can collide or drift across platforms,
- snapshot invalidation can imply precision it does not have,
- budgeted output undercounts large chunks,
- CLI flags are accepted without honored behavior,
- diagnostics are not stable or actionable enough for automation,
- tests type-check the CLI path but do not execute its behavior.

The implementation should make these failure modes explicit, tested, and conservative.

---

## Requirements Trace

- R1. Stable chunk identity must use an explicitly stable algorithm and must not silently overwrite repeated-name chunks.
- R2. Invalidation output must explain why each chunk was classified and expose degraded confidence when evidence is incomplete.
- R3. Token estimates must report the true estimate; budget enforcement must omit oversized chunks instead of masking them.
- R4. Public CLI flags `--budget`, `--quiet`, and `--grammar-path` must either work as documented or be removed/hidden.
- R5. Diagnostics must be structured enough for agents to branch on stable codes and suggested fixes.
- R6. Public JSON output shapes must be covered by fixed contract snapshots, not substring checks.
- R7. New behavior must have executable tests covering crate logic and CLI integration.
- R8. DX cleanup must remove known paper cuts: package readme metadata, benchmark command accuracy, symbol option enforcement.

---

## Scope Boundaries

- Do not add an MCP server, persistent index, vector search, or cross-file resolver.
- Do not make the Rust API stable beyond the experimental schema/versioning contract.
- Do not publish or package releases in this plan.
- Do not expand to Python or TypeScript fixtures until the Rust v1 hardening gates pass.
- Do not build an adapter spike until invalidation output, schema snapshots, and CLI behavior are trustworthy.

### Deferred to Follow-Up Work

- Adapter validation track remains deferred to `docs/plans/tree-sitter-context-follow-up-plan-2026-04-25.md`.
- Multi-language fixtures remain deferred until after Rust hardening and benchmark proof.
- Release/distribution work remains deferred until this prototype has a stable enough contract.

---

## Context & Research

### Relevant Code and Patterns

- `crates/context/src/schema.rs` defines public JSON records and current substring-based schema tests.
- `crates/context/src/chunk.rs` estimates tokens, emits chunks, and currently caps `estimated_tokens`.
- `crates/context/src/identity.rs` computes `StableId` with `DefaultHasher` and matches chunks through `HashMap`.
- `crates/context/src/invalidation.rs` classifies snapshot and edit-stream invalidation.
- `crates/context/src/bundle.rs` contains the budgeted bundle API and omitted chunk model.
- `crates/context/src/symbols.rs` wraps `tree-sitter-tags` and currently leaves symbol tests as an integration TODO.
- `crates/cli/src/context.rs` is the CLI execution path for `tree-sitter context`.
- `crates/cli/src/main.rs` defines `ContextCmd` and its flags.
- `crates/cli/src/tests/` contains the existing CLI test structure and fixture helpers.
- `crates/cli/src/tests/helpers/fixtures.rs` shows how test grammars and loader fixtures are built.
- Workspace dependencies already include `serde_json`, `schemars`, `similar`, and `tree-sitter-tags`.

### Institutional Learnings

- `docs/solutions/workflow-issues/tree-sitter-context-branch-review-2026-04-25.md` is the review checkpoint for the current branch. Its key pattern is to keep planning, follow-up work, and review findings in separate durable artifacts.

### External References

- None used. Local code and review artifacts provide enough guidance; this work is mostly contract hardening inside the current Rust workspace.

---

## Key Technical Decisions

- Stable IDs should use a documented digest, not `DefaultHasher`. Prefer adding a workspace dependency on `blake3` and hashing a length-delimited identity payload. If dependency review rejects that addition, use a small explicitly documented deterministic hasher and keep collision diagnostics.
- Chunk matching should preserve traversal order and bucket duplicate IDs. Duplicates should never be collapsed by `HashMap` overwrite; they should be classified with degraded confidence and diagnostics unless disambiguation can resolve them.
- Snapshot invalidation should not present independently parsed `Tree::changed_ranges` as exact evidence. Use a textual byte-range diff for old/new snapshots and reserve tree edit ranges for edit-stream mode.
- Invalidation should move toward per-classification records while keeping compatibility manageable. If replacing the current bucket arrays is too disruptive, add detail records in parallel and keep bucket arrays as derived compatibility fields during the prototype.
- `--budget` should produce a versioned budget-aware JSON payload with included and omitted chunks. If `ContextOutput` remains the top-level payload, omitted context must still be represented explicitly; silent filtering is not acceptable.
- CLI recoverable/degraded cases should produce structured JSON diagnostics where possible. Hard setup failures can still return non-zero, but their error text should map to documented diagnostic codes in tests.
- Contract tests should assert stable structures and required fields, not prose. Avoid brittle full-file output where formatting is not part of the contract.

---

## Open Questions

### Resolved During Planning

- Should this plan fix everything in one PR? No. The units are ordered for incremental commits and review rounds, but they can live in one branch if each unit is tested before moving on.
- Should external research run before implementation? No. The critical decisions are local contract and test decisions, and the repo already has enough examples for Rust crate and CLI testing.

### Deferred to Implementation

- Exact top-level JSON shape for budgeted CLI output: choose the smallest compatible shape while implementing U3/U5, but it must include omitted chunks and metadata.
- Exact diagnostic code names: define them while implementing U1, then snapshot them.
- Whether to keep compatibility bucket arrays in `InvalidationOutput`: decide after adding `InvalidationRecord` shape and updating tests.

---

## High-Level Technical Design

> This illustrates the intended approach and is directional guidance for review, not implementation specification.

```text
source + tree
  -> chunk pass
       -> ChunkRecord { stable_id, range, true estimated_tokens, confidence }
       -> diagnostics { code, message, fix, source }
  -> optional symbols pass
       -> SymbolRecord { docs truncated by max_docs_len }
  -> invalidation pass
       snapshot mode: textual byte ranges + identity matching
       edit mode: edited tree changed_ranges + identity matching
       -> InvalidationRecord { status, reason, match_strategy, confidence }
  -> optional budget pass
       -> included chunks + omitted chunks + budget metadata
  -> CLI JSON output
       -> quiet suppresses main output
       -> recoverable degradation appears as structured diagnostics
```

---

## Implementation Units

- U1. **Schema, diagnostics, and contract snapshots**

**Goal:** Establish the public output contract before changing matching and CLI behavior.

**Requirements:** R2, R5, R6, R7

**Dependencies:** None

**Files:**
- Modify: `crates/context/src/schema.rs`
- Modify: `crates/context/src/lib.rs`
- Test: `crates/context/src/schema.rs`
- Test: `crates/context/tests/` if snapshot fixtures are clearer outside unit tests

**Approach:**
- Add stable diagnostic fields such as `code`, `message`, optional `cause`, optional `fix`, optional `context`, and optional source/category metadata.
- Add invalidation classification detail types if they fit cleanly in schema-first work.
- Replace substring assertions with fixed JSON snapshot assertions for `ContextOutput`, `InvalidationOutput`, bundle output, diagnostics, and symbol records.
- Keep snapshots compact and deterministic.

**Execution note:** Contract-test first. Add snapshots before implementation units start changing behavior.

**Patterns to follow:**
- Existing serde/schemars derives in `crates/context/src/schema.rs`.
- Existing unit-test style in `crates/context/src/schema.rs`.

**Test scenarios:**
- Happy path: `ContextOutput` serializes with schema version, source path, chunks, diagnostics, and totals.
- Happy path: `InvalidationOutput` serializes stable classification fields.
- Edge case: optional diagnostic details omit cleanly when absent.
- Regression: snapshot fails if field names, enum variants, or required output structure drift.

**Verification:**
- `cargo test -p tree-sitter-context schema`

---

- U2. **Stable identity and deterministic chunk matching**

**Goal:** Make chunk identity stable and make duplicate IDs observable instead of silently lossy.

**Requirements:** R1, R2, R6, R7

**Dependencies:** U1

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/context/Cargo.toml`
- Modify: `crates/context/src/identity.rs`
- Modify: `crates/context/src/chunk.rs`
- Test: `crates/context/src/identity.rs`
- Test: `crates/context/benches/fixtures/medium.rs` if a richer duplicate fixture is needed

**Approach:**
- Replace `DefaultHasher` with an explicitly stable digest over a length-delimited identity payload.
- Include enough identity input to distinguish repeated names: normalized path, node kind, name path or parent identity, syntactic depth, and a disambiguating source anchor/content component.
- Change `match_chunks` from single-value maps to duplicate-aware buckets.
- Preserve deterministic output order by traversal order or explicit sorting, not randomized `HashMap` iteration.
- Emit diagnostics or degraded confidence for unresolved duplicate identity buckets.

**Execution note:** Characterization first. Add failing duplicate-name and ordering tests before replacing matching internals.

**Patterns to follow:**
- Existing `StableId::compute` call sites in `crates/context/src/chunk.rs`.
- Existing identity unit tests in `crates/context/src/identity.rs`.

**Test scenarios:**
- Happy path: same named function keeps the same stable ID across repeated parses.
- Edge case: same method name in different `impl` blocks does not silently overwrite either chunk.
- Edge case: reordered chunks produce deterministic output order.
- Regression: unnamed chunk identity changes when content changes.
- Regression: duplicate bucket classification includes diagnostic/degraded confidence when disambiguation is insufficient.

**Verification:**
- `cargo test -p tree-sitter-context identity`

---

- U3. **Chunk correctness and budget accounting**

**Goal:** Make token estimates truthful and budget output explicit.

**Requirements:** R3, R6, R7

**Dependencies:** U1

**Files:**
- Modify: `crates/context/src/chunk.rs`
- Modify: `crates/context/src/bundle.rs`
- Modify: `crates/context/src/schema.rs`
- Test: `crates/context/src/chunk.rs`
- Test: `crates/context/src/bundle.rs`

**Approach:**
- Stop capping `ChunkRecord.estimated_tokens`.
- Represent max-token pressure through bundle omissions or diagnostics, not by mutating the estimate.
- Add versioned metadata to bundle output if it remains a public output shape.
- Define whether budget means chunk-estimated budget or full serialized JSON budget; for this prototype, prefer chunk-estimated budget plus explicit metadata that serialized size is not included.
- Add parse-error diagnostics and confidence downgrade when chunking a tree with parser errors or missing nodes.

**Execution note:** Test-first for token estimate truncation and parse-error confidence.

**Patterns to follow:**
- Existing `estimate_tokens` tests in `crates/context/src/chunk.rs`.
- Existing `bundle_chunks` tests in `crates/context/src/bundle.rs`.

**Test scenarios:**
- Happy path: normal chunk records true estimated token count.
- Edge case: large syntax node with small `max_tokens` still records true estimate.
- Edge case: oversized chunk is omitted from bundle with `OverBudget`.
- Error path: parse-error tree emits diagnostic and does not mark affected output as fully exact.
- Regression: bundle totals equal included and omitted estimated tokens.

**Verification:**
- `cargo test -p tree-sitter-context chunk bundle`

---

- U4. **Explainable invalidation semantics**

**Goal:** Make snapshot and edit-stream invalidation conservative, explainable, and deterministic.

**Requirements:** R2, R5, R6, R7

**Dependencies:** U1, U2, U3

**Files:**
- Modify: `crates/context/src/invalidation.rs`
- Modify: `crates/context/src/schema.rs`
- Test: `crates/context/src/invalidation.rs`
- Test: `crates/context/benches/fixtures/medium.rs`
- Test: `crates/context/benches/fixtures/medium_edited.rs`

**Approach:**
- Split classification evidence by mode:
  - snapshot mode uses textual byte ranges plus identity/content comparison,
  - edit-stream mode uses edited-tree changed ranges plus identity/content comparison.
- Add per-classification reason and match strategy.
- Propagate old/new chunking diagnostics into invalidation output.
- Downgrade confidence for all edit-stream classifications, including removed chunks, unless exact evidence is available.
- Ensure affected/added/removed/unchanged output order is stable.

**Execution note:** Characterization first for current snapshot behavior. Add tests that show the current misleading case before changing the algorithm.

**Patterns to follow:**
- Existing invalidation tests in `crates/context/src/invalidation.rs`.
- Existing tree-sitter edit helpers in `crates/cli/src/tests/helpers/edits.rs` if useful.

**Test scenarios:**
- Happy path: body-only change marks the function affected with a reason.
- Happy path: added function appears as added with textual range evidence.
- Happy path: removed function appears as removed with degraded or explicit removed evidence.
- Edge case: whitespace-only change remains unchanged or low-impact according to documented policy.
- Edge case: independent old/new parse does not report empty changed evidence when content changed.
- Regression: output order is stable across repeated runs.
- Regression: old/new chunk diagnostics are present in invalidation diagnostics.

**Verification:**
- `cargo test -p tree-sitter-context invalidation`

---

- U5. **CLI contract for context command**

**Goal:** Make the user-visible CLI flags truthful and covered by executable tests.

**Requirements:** R4, R5, R7

**Dependencies:** U1, U3, U4

**Files:**
- Modify: `crates/cli/src/main.rs`
- Modify: `crates/cli/src/context.rs`
- Modify: `crates/cli/src/tree_sitter_cli.rs`
- Test: `crates/cli/src/tests/context_test.rs`
- Test: `crates/cli/src/tests/helpers/fixtures.rs` if fixture setup needs a helper

**Approach:**
- Implement `--quiet` so it suppresses main JSON output while preserving errors.
- Implement `--budget` using the budget-aware output shape from U3.
- Make `--grammar-path` materially affect language discovery for `context`, or remove/hide the flag until it can be supported.
- Return structured diagnostics for recoverable degraded cases where possible.
- Add CLI tests using existing test grammar/loader fixture patterns.

**Execution note:** Integration tests should drive this unit. Avoid relying on `cargo check` as evidence of CLI behavior.

**Patterns to follow:**
- CLI command dispatch in `crates/cli/src/main.rs`.
- Loader fixture helpers in `crates/cli/src/tests/helpers/fixtures.rs`.
- Existing CLI tests under `crates/cli/src/tests/`.

**Test scenarios:**
- Happy path: `tree-sitter context <rust fixture>` writes valid JSON.
- Happy path: `tree-sitter ctx <rust fixture>` uses the alias.
- Happy path: `--old` emits invalidation JSON with classifications.
- Happy path: `--symbols` includes symbol records when tags config exists.
- Happy path: `--budget 10` includes omitted context metadata.
- Edge case: `--quiet` produces no main JSON output.
- Error path: missing language produces a stable diagnostic or expected non-zero error.
- Integration: `--grammar-path` changes language discovery or is absent from help if unsupported.

**Verification:**
- `cargo test -p tree-sitter-cli context`
- `cargo check -p tree-sitter-cli`

---

- U6. **Symbols, package metadata, and DX cleanup**

**Goal:** Close the low-risk cleanup items that block warning-free checks and copy-paste DX.

**Requirements:** R7, R8

**Dependencies:** U1

**Files:**
- Modify: `crates/context/src/symbols.rs`
- Modify: `crates/context/examples/smoke_benchmark.rs`
- Modify: `crates/context/Cargo.toml`
- Create: `crates/context/README.md`
- Test: `crates/context/src/symbols.rs`
- Test: `crates/context/examples/smoke_benchmark.rs` if example assertions are added elsewhere

**Approach:**
- Enforce `SymbolOptions.max_docs_len` with deterministic truncation behavior.
- Keep `max_symbols` behavior covered.
- Add symbol tests using a compiled grammar/tags config pattern from existing CLI tag tests where possible.
- Correct the smoke benchmark header command to use `--example`.
- Add `package.readme` metadata and a focused crate README.

**Execution note:** This unit can be done after U1 or between larger units if a low-risk break is useful, but do not let it distract from P1/P2 correctness work.

**Patterns to follow:**
- `crates/cli/src/tests/tags_test.rs` for tags query examples.
- Workspace package metadata style in existing crates.

**Test scenarios:**
- Happy path: symbol extraction returns definitions/references/docs.
- Edge case: docs longer than `max_docs_len` are truncated deterministically.
- Edge case: `max_symbols` stops extraction and emits diagnostic.
- Error path: malformed tags are skipped with diagnostic.
- Regression: `cargo clippy -p tree-sitter-context --all-targets -- -D warnings` no longer fails on missing readme metadata.

**Verification:**
- `cargo test -p tree-sitter-context symbols`
- `cargo clippy -p tree-sitter-context --all-targets -- -D warnings`
- `cargo run -p tree-sitter-context --example smoke_benchmark`

---

## System-Wide Impact

- **Interaction graph:** Core schema changes affect `chunk`, `identity`, `invalidation`, `bundle`, `symbols`, and the CLI JSON output path.
- **Error propagation:** Recoverable degraded states should travel as structured diagnostics; unrecoverable CLI setup failures may remain non-zero errors but should be tested.
- **State lifecycle risks:** Stable IDs and invalidation records become cache-facing data. Silent collision or nondeterministic ordering is the main lifecycle risk.
- **API surface parity:** Rust library output and CLI JSON should expose equivalent diagnostics and confidence semantics.
- **Integration coverage:** CLI integration tests are required because unit tests cannot prove loader/config/flag behavior.
- **Unchanged invariants:** No MCP server, persistent index, cross-file resolution, or release packaging is introduced by this plan.

---

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Schema changes cascade across many modules | Start with U1 contract snapshots and update each unit against those snapshots. |
| Stable identity design overfits Rust fixtures | Include repeated-name and nested fixtures, and keep duplicate diagnostics when disambiguation is uncertain. |
| Snapshot text diff is less precise than edit-stream changed ranges | Mark snapshot evidence and confidence explicitly; do not call it exact when it is not. |
| CLI budget output shape becomes hard to integrate | Keep the output versioned and include omitted context; document whether budget covers estimated chunk tokens or full JSON bytes. |
| CLI tests are expensive or flaky because of grammar compilation | Reuse existing fixture helpers and keep context CLI fixtures small. |
| `blake3` dependency is rejected | Fall back to an explicitly documented deterministic hasher and keep collision diagnostics; do not return to `DefaultHasher`. |

---

## Documentation / Operational Notes

- Update `docs/plans/tree-sitter-context-follow-up-plan-2026-04-25.md` after implementation only if scope changes; progress itself should be derived from git.
- Add crate README content as part of U6, not before the CLI/schema behavior is finalized.
- After completing U1-U4, run `compound-engineering:ce-code-review` before starting CLI/DX cleanup so contract mistakes are caught early.
- After completing U5-U6, run `compound-engineering:ce-code-review` again before PR creation.

---

## Success Metrics

- `cargo test -p tree-sitter-context` passes with new contract, identity, chunk, bundle, invalidation, and symbol coverage.
- `cargo test -p tree-sitter-cli context` or equivalent CLI integration tests pass.
- `cargo check -p tree-sitter-cli` passes.
- `cargo clippy -p tree-sitter-context --all-targets -- -D warnings` passes.
- `tree-sitter context` documented flags either work or are no longer exposed.
- Invalidation JSON includes enough reason/confidence data for an agent to explain affected/added/removed/unchanged classifications.

---

## Phased Delivery

### Phase 1

- U1 Schema, diagnostics, and snapshots.
- U2 Stable identity and deterministic matching.
- U3 Chunk correctness and budget accounting.

### Phase 2

- U4 Explainable invalidation semantics.
- Run targeted tests and `compound-engineering:ce-code-review`.

### Phase 3

- U5 CLI contract and integration tests.
- U6 Symbols, metadata, and DX cleanup.
- Run full verification and `compound-engineering:ce-code-review`.

---

## Sources & References

- Origin document: [docs/plans/tree-sitter-context-follow-up-plan-2026-04-25.md](tree-sitter-context-follow-up-plan-2026-04-25.md)
- RFC: [docs/plans/tree-sitter-context-rfc-2026-04-24.md](tree-sitter-context-rfc-2026-04-24.md)
- Review checkpoint: [docs/solutions/workflow-issues/tree-sitter-context-branch-review-2026-04-25.md](../solutions/workflow-issues/tree-sitter-context-branch-review-2026-04-25.md)
- Related code: `crates/context/src/schema.rs`
- Related code: `crates/context/src/identity.rs`
- Related code: `crates/context/src/invalidation.rs`
- Related code: `crates/cli/src/context.rs`
- Related tests: `crates/cli/src/tests/`
