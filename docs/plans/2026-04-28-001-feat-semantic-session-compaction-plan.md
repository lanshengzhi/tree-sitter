---
title: "feat: Semantic Session Compaction CLI and pi-mono extension tool"
type: feat
status: active
date: 2026-04-28
origin: docs/brainstorms/2026-04-28-semantic-session-compaction-requirements.md
---

# feat: Semantic Session Compaction CLI and pi-mono extension tool

## Overview

Add `tree-sitter-context compact` CLI command and a pi-mono `get_semantic_compact` extension tool that compress session context by keeping full content for changed chunks and extracting only signatures for unchanged chunks. This replaces the LLM-based compaction with deterministic AST-aware compression, reducing token waste while preserving code structure.

The work builds on the recently completed invalidation infrastructure (U1-U5 in `feat/incremental-invalidation`) and follows the same patterns: S-expression canonical output, CLI integration tests, and pi-mono bridge/tool registration.

---

## Problem Frame

pi-mono's current session compaction uses LLM summarization, which has three concrete problems:
1. **Information loss**: LLM summaries drop function signatures and type information.
2. **Token waste**: Re-reads entire files even when only small parts changed.
3. **Compaction cost**: Every compaction requires an LLM API call.

tree-sitter-context now has stable chunk identities and incremental invalidation. Semantic Session Compaction leverages this to classify chunks as affected/unchanged and compress unchanged named chunks to signatures using existing tags/symbols infrastructure.

(see origin: docs/brainstorms/2026-04-28-semantic-session-compaction-requirements.md)

---

## Requirements Trace

- R1. Add `tree-sitter-context compact` CLI command with `--old` and `--budget` flags.
- R2. Multi-file batch processing.
- R3. Invalidation-based classification (affected/added/removed/unchanged).
- R4. Full content for affected/added/removed chunks.
- R5. Signature-only output for unchanged named chunks.
- R6. Canonical S-expression output.
- R7. Token budget enforcement with `--budget`.
- R8. Omission metadata when budget exceeded.
- R9. Typed `budget_exceeded` error when unavoidable.
- R10. pi-mono `get_semantic_compact` extension tool.
- R11. Bridge function for multi-file CLI spawning and result assembly.
- R12. Structured result with `preserved`, `signatures_only`, `omitted`, and token stats.
- R13. CLI integration tests for core scenarios.
- R14. Byte-stability tests.

**Origin actors:** A1 pi-mono agent, A2 agent operator, A3 tree-sitter-context CLI, A4 extension tool bridge
**Origin flows:** F1 explicit semantic compaction, F2 token budget enforcement
**Origin acceptance examples:** AE1 body-only change, AE2 unchanged function, AE3 unchanged struct, AE4 budget enforcement, AE5 error paths

---

## Scope Boundaries

- **Deferred for later**
  - Cross-file reference retention: requires R1 graph cross-file resolution.
  - Auto-compaction replacement: v1 is explicit extension tool only.
  - LLM-generated summaries for affected chunks: deterministic signature extraction first.
  - pi-mono E2E harness tests: CLI and unit tests in scope; full pi-mono end-to-end deferred.
  - Multi-language parity beyond Rust: start with Rust fixtures.

- **Outside this product's identity**
  - Replacing pi-mono core auto-compact logic: tree-sitter-context is a primitive, not a session manager.
  - MCP server or daemon mode: out of scope per RFC.
  - Cross-file impact analysis: this is graph navigation, not compaction.

- **Deferred to Follow-Up Work**
  - pi-mono integration test harness: separate PR after CLI stabilizes.
  - Extension tool loading in pi-mono's default toolset: requires pi-mono maintainer approval.

---

## Context & Research

### Relevant Code and Patterns

- `crates/context/src/chunk.rs` — `chunks_for_tree()` generates semantic chunks with `stable_id`, `kind`, `name`, `byte_range`.
- `crates/context/src/invalidation.rs` — `invalidate_snapshot()` classifies chunks into `Affected`, `Added`, `Removed`, `Unchanged` using stable identity matching and textual diff.
- `crates/context/src/symbols.rs` — `symbols_for_tree()` extracts definitions via `tree_sitter_tags::TagsContext::generate_tags()`, returning `name`, `syntax_type`, `byte_range`, `docs`.
- `crates/context/src/sexpr.rs` — `invalidation_to_sexpr()`, `bundle_to_sexpr()` demonstrate canonical S-expression serialization: deterministic ordering, 2-space indentation, string escaping, no timestamps.
- `crates/context/src/schema.rs` — `ChunkRecord`, `InvalidationOutput`, `InvalidationRecord`, `ContextOutput` types with schema versioning.
- `crates/context/src/bundle.rs` — `bundle_chunks()` demonstrates budget enforcement: sorts by priority, explicitly omits with `OmissionReason::OverBudget`.
- `crates/cli/src/bin/tree-sitter-context.rs` — CLI dispatch using `clap` derive macros (`#[derive(Subcommand)]`, `#[derive(Args)]`).
- `crates/cli/src/context_invalidate.rs` — `run_invalidate()` pattern: build loader, parse both files, call invalidation, serialize output.
- `crates/cli/src/tests/context_invalidate_test.rs` — Integration test pattern: `Command::new(&bin).arg("invalidate").args([...]).output()`.
- `pi-mono/packages/coding-agent/src/core/tree-sitter-context/bridge.ts` — `getInvalidatedChunks()` pattern: validate input, write temp file, spawn CLI, parse S-expression.
- `pi-mono/packages/coding-agent/src/core/tree-sitter-context/tool.ts` — `defineTool()` registration pattern for `get_context_bundle` and `get_invalidated_chunks`.

### Institutional Learnings

- **Honest budget accounting**: Never cap token estimates. Oversized chunks must be explicitly omitted with `OmittedContext` metadata (docs/solutions/workflow-issues/tree-sitter-context-branch-review-2026-04-25.md).
- **CLI flags must be honored**: If a flag is visible in `--help`, it must have tested behavior. Broken CLI contracts are worse than missing features.
- **Explainable confidence**: Invalidation output needs per-classification records (`reason`, `match_strategy`, `confidence`) so agents can explain why a chunk was affected.
- **Schema snapshots are the integration contract**: JSON schema must be versioned with fixed snapshot tests, not substring checks.

### External References

- Tree-sitter tags queries documentation for `@definition` capture semantics.

---

## Key Technical Decisions

- **Extension tool first, not auto-compaction replacement**: Aligns with R0-R3 constraint that pi-mono core compaction must not be replaced yet.
- **Rust CLI owns full compression logic**: Signature extraction uses existing `tree-sitter-tags` infrastructure; pushing to pi-mono would require AST parsing in TypeScript.
- **S-expression primary output**: Matches R0-R3 canonical form conventions.
- **Multi-file batch in v1**: Agents typically work with multiple files; single-file-only would be too narrow.
- **Anonymous chunks kept full**: Unchanged `impl` blocks and anonymous chunks have no meaningful signature; they are kept as full content.
- **Signature extraction via tags `@definition` spans**: For unchanged named chunks, extract the declaration line(s) from the tags query match rather than inventing a new extraction logic.

---

## Open Questions

### Resolved During Planning

- [Affects R5] Unchanged anonymous chunks (e.g., `impl` blocks) are kept as full content, not compressed. Only named chunks are compressed to signatures.

### Deferred to Implementation

- [Affects R5][Technical] Exact algorithm for extracting "signature" from a chunk: first N lines, tags query `@definition` span, or AST node-specific logic? Deferred — implementer should evaluate which is most reliable across Rust fixtures.
- [Affects R8][Technical] Priority ordering for budget discarding: recency-based, topological, or simple file-order? Deferred — start with file-order + chunk-order for simplicity.
- [Affects R1][Needs research] Does the CLI accept base64-encoded old content inline, or must pi-mono write temp files? Deferred — evaluate against existing `get_invalidated_chunks` pattern during implementation.

---

## Implementation Units

- U1. **CompactOutput schema and S-expression serialization**

**Goal:** Define `CompactOutput` schema types and canonical S-expression serializer.

**Requirements:** R6, R12, R14

**Dependencies:** None

**Files:**
- Modify: `crates/context/src/schema.rs`
- Modify: `crates/context/src/sexpr.rs`
- Test: `crates/context/tests/compact_contract.rs`

**Approach:**
- Add `CompactOutput`, `CompactFileResult`, `CompactChunkRecord`, `CompactOmittedRecord` types to `schema.rs` with `#[derive(Serialize, Deserialize, JsonSchema)]`.
- `CompactChunkRecord` has two variants: `Preserved` (full chunk) and `SignatureOnly` (signature string + metadata).
- Add `compact_to_sexpr(output: &CompactOutput) -> String` in `sexpr.rs` following existing indentation, sorting by `stable_id`, and escaping conventions.
- Include `original_tokens`, `compacted_tokens`, `omitted_count` in top-level metadata.

**Execution note:** Start with contract test asserting `parse(emit(x)) == emit(x)` byte-stability for a representative `CompactOutput`.

**Patterns to follow:**
- `crates/context/src/sexpr.rs` lines 264-280 (Provenance serialization)
- `crates/context/src/sexpr.rs` lines 386-413 (Orientation block serialization)
- `crates/context/tests/sexpr_contract.rs` (round-trip byte-stability pattern)

**Test scenarios:**
- Happy path: serialize complex `CompactOutput` with preserved + signatures_only + omitted → parse and re-emit produces identical bytes.
- Edge case: empty compaction (no files) → empty output with zero tokens, not error.
- Edge case: all chunks preserved (all affected) → no signatures_only or omitted sections.
- Edge case: all chunks signatures_only (all unchanged) → no preserved section.

**Verification:**
- `cargo test -p tree-sitter-context` passes including new contract tests.
- Manual: `echo '(compaction ...)' | cargo run --bin tree-sitter-context -- compact --format sexpr` produces identical bytes on second run.

---

- U2. **Core compaction logic**

**Goal:** Implement semantic compaction function that runs invalidation and extracts signatures for unchanged named chunks.

**Requirements:** R3, R4, R5, R7, R8, R9

**Dependencies:** U1

**Files:**
- Create: `crates/context/src/compact.rs`
- Modify: `crates/context/src/lib.rs` (module export)
- Test: `crates/context/src/compact.rs` (inline `#[cfg(test)]`)

**Approach:**
- Add `compact_files(paths: &[PathBuf], old_contents: &HashMap<PathBuf, Vec<u0038>>, budget: Option<usize>) -> Result<CompactOutput>`.
- For each file:
  1. Parse old and new source using `tree_sitter::Parser`.
  2. Generate chunks for both via `chunks_for_tree()`.
  3. Run `invalidate_snapshot()` to classify.
  4. For affected/added/removed: emit full `ChunkRecord` as `Preserved`.
  5. For unchanged named chunks: extract signature.
     - Query tags/symbols for `@definition` captures overlapping the chunk.
     - Extract the declaration span (first line or `@definition` byte range).
     - Emit as `SignatureOnly` with `signature` field.
  6. For unchanged anonymous chunks: emit as `Preserved` (full content).
  7. Sum `estimated_tokens` for original vs compacted.
- If `budget` is set and `compacted_tokens > budget`, discard lowest-priority chunks: signatures_only first, then least-recently-referenced preserved, with `CompactOmittedRecord` metadata.
- If still over budget after discarding all signatures_only, return typed error.

**Technical design:**
> *This illustrates the intended approach and is directional guidance for review, not implementation specification. The implementing agent should treat it as context, not code to reproduce.*
>
> Signature extraction strategy:
> 1. Run `symbols_for_tree()` on the new tree.
> 2. For each unchanged chunk with `name.is_some()`, find symbol(s) whose `byte_range` is contained within the chunk's `byte_range`.
> 3. Use the symbol's `byte_range` to extract the declaration text from source bytes.
> 4. If no symbol found, fall back to chunk's first line (simple heuristic).
> 5. For struct/enum declarations, the entire declaration may be the signature; for functions, only the signature line (up to `{`).

**Patterns to follow:**
- `crates/context/src/invalidation.rs` lines 20-240 (`invalidate_snapshot` logic)
- `crates/context/src/bundle.rs` lines 50-120 (budget enforcement and omission tracking)
- `crates/context/src/symbols.rs` (symbol extraction and `@definition` semantics)

**Test scenarios:**
- Happy path: body-only change → function in `preserved`, other functions in `signatures_only`.
- Happy path: signature change → old function in `removed`, new in `added`, none in `affected`.
- Happy path: whitespace-only change → all chunks in `signatures_only`, `preserved` empty.
- Edge case: struct unchanged → struct declaration in `signatures_only` (full declaration is signature).
- Edge case: anonymous `impl` block unchanged → kept in `preserved` (not compressed).
- Edge case: budget forces omission of signatures_only → `omitted` list contains reason "budget".
- Error path: budget too small for even affected chunks → returns `budget_exceeded` error with `required_tokens`.
- Error path: invalid UTF-8 in source → handled during parse, not compaction.

**Verification:**
- `cargo test -p tree-sitter-context` passes including new inline tests.
- Test fixtures cover body change, signature change, whitespace-only, and struct scenarios.

---

- U3. **CLI compact command**

**Goal:** Add `tree-sitter-context compact` command to the dedicated binary.

**Requirements:** R1, R2, R6

**Dependencies:** U2

**Files:**
- Create: `crates/cli/src/context_compact.rs`
- Modify: `crates/cli/src/bin/tree-sitter-context.rs`
- Modify: `crates/cli/src/lib.rs` (module export if needed)

**Approach:**
- Add `Compact(CompactArgs)` variant to `Commands` enum with fields: `paths: Vec<PathBuf>`, `old: PathBuf` (or base64 content), `budget: Option<usize>`, `format: OutputFormat`.
- Implement `run_compact(args: CompactArgs) -> Result<()>` in new `context_compact.rs` module.
- Load language configuration via `build_loader()` (reuse pattern from `context_invalidate.rs`).
- Parse both old and new files for each path.
- Call `compact_files()` from U2.
- Serialize output: if format is Sexpr, use U1 serializer; if Json, use serde.
- Write to stdout on success; write typed errors to stderr with non-zero exit codes.

**Patterns to follow:**
- `crates/cli/src/bin/tree-sitter-context.rs` lines 107+ (BundleArgs pattern)
- `crates/cli/src/context_invalidate.rs` (loader setup, file parsing, error handling)
- `crates/cli/src/context_graph.rs` lines 217-238 (typed error prefixes)

**Test scenarios:**
- Happy path: compact between two real Rust files with changes → exit 0, stdout contains sexpr with preserved and signatures_only chunks.
- Happy path: `--format json` → valid JSON output matching sexpr structure.
- Edge case: identical files → all chunks in signatures_only, preserved empty.
- Edge case: `--budget 100` with large file → omits chunks or returns budget_exceeded.
- Error path: missing `--old` flag → exit non-zero, stderr contains usage help.
- Error path: unreadable old file → exit non-zero, stderr contains `file_not_found`.
- Error path: parse error → exit non-zero, stderr contains `parse_error`.

**Verification:**
- `cargo run --bin tree-sitter-context -- compact --help` shows correct usage.
- Manual test: `tree-sitter-context compact new.rs --old old.rs` produces expected output.

---

- U4. **CLI integration tests**

**Goal:** Add comprehensive CLI integration tests for the compact command.

**Requirements:** R13, R14

**Dependencies:** U3

**Files:**
- Create: `crates/cli/src/tests/context_compact_test.rs`
- Create: `crates/cli/src/tests/fixtures/old_compact.rs`
- Create: `crates/cli/src/tests/fixtures/new_compact.rs`

**Approach:**
- Create test fixtures with various change types: body change, signature change, added function, removed function, unchanged struct, unchanged impl block.
- Follow pattern from `context_invalidate_test.rs`: use `Command::new(tree_sitter_context_bin())`, spawn with args, assert on `output.status.success()` and stderr content.
- Test byte-stability: run same command twice, compare stdout bytes.

**Patterns to follow:**
- `crates/cli/src/tests/context_invalidate_test.rs` (entire file structure)
- `crates/cli/src/tests/context_bundle_test.rs` (fixture directory pattern)

**Test scenarios:**
- Covers AE1: body-only change → exactly one preserved chunk, rest signatures_only.
- Covers AE2: signature change → one removed, one added, none preserved.
- Covers AE3: whitespace-only reformat → all chunks in signatures_only, preserved empty.
- Covers AE4: budget enforcement with `--budget` → omitted list non-empty or budget_exceeded error.
- Error path: missing `--old` flag → non-zero exit, usage shown.
- Regression: `--format sexpr` and `--format json` produce equivalent semantic content.
- Regression: byte-stability for repeated runs on same fixtures.

**Verification:**
- All tests pass: `cargo test -p tree-sitter-cli --test context_compact_test`.
- Test fixtures are committed in `crates/cli/src/tests/fixtures/`.

---

- U5. **pi-mono extension tool and bridge**

**Goal:** Add `get_semantic_compact` extension tool to pi-mono that wraps the CLI.

**Requirements:** R10, R11, R12

**Dependencies:** U3

**Files:**
- Modify: `pi-mono/packages/coding-agent/src/core/tree-sitter-context/tool.ts`
- Modify: `pi-mono/packages/coding-agent/src/core/tree-sitter-context/bridge.ts`
- Create: `pi-mono/packages/coding-agent/src/core/tree-sitter-context/types.ts` (if new types needed)

**Approach:**
- Define tool parameters: `paths: string[]`, `old_contents_base64: Record<string, string>` (path → base64 old content), optional `budget?: number`.
- In `bridge.ts`, implement `getSemanticCompact(input, cwd, cliPath)`:
  1. Validate `paths` and `old_contents_base64` keys match.
  2. For each file, decode base64 old content and write to temp file.
  3. Spawn CLI: `tree-sitter-context compact <path> --old <temp-path> [--budget <n>]`.
  4. Parse stdout S-expression using existing `parseSExpr` from `sexpr.ts`.
  5. Assemble multi-file result into single structured object.
  6. Return typed result with `preserved`, `signatures_only`, `omitted`, `original_tokens`, `compacted_tokens`.
- In `tool.ts`, register tool using `defineTool()` following `get_context_bundle` pattern.
- Handle errors: CLI non-zero exit → return typed error result; parse errors → return `invalid-output` error.

**Execution note:** Use base64 encoding to safely pass binary content across JSON-RPC boundary; follow the same temp file pattern as `getInvalidatedChunks`.

**Patterns to follow:**
- `pi-mono/packages/coding-agent/src/core/tree-sitter-context/tool.ts` lines 11-84 (get_context_bundle tool)
- `pi-mono/packages/coding-agent/src/core/tree-sitter-context/bridge.ts` lines 33-109 (getContextBundle bridge)
- `pi-mono/packages/coding-agent/src/core/tree-sitter-context/bridge.ts` lines 110-180 (getInvalidatedChunks pattern)

**Test scenarios:**
- Happy path: tool call with valid paths and old_content → returns structured result with correct classifications.
- Happy path: with `budget` parameter → CLI receives `--budget` flag, result respects limit.
- Edge case: empty `paths` array → returns empty result (not error).
- Error path: invalid base64 → returns validation error before spawning CLI.
- Error path: CLI process error → returns process-error with stderr content.
- Error path: malformed CLI stdout → returns invalid-output error.

**Verification:**
- Loading the extension makes `get_semantic_compact` available to pi-mono.
- Tool can be invoked through pi-mono's tool execution path.
- `npm run check` passes in pi-mono after changes.

---

- U6. **End-to-end validation and documentation**

**Goal:** Validate full flow, document the compact contract, update compatibility tests.

**Requirements:** R10, R12, R14

**Dependencies:** U4, U5

**Files:**
- Create: `docs/plans/tree-sitter-context-compact-contract.md`
- Modify: `pi-mono/packages/coding-agent/test/tree-sitter-context-compat.test.ts`
- Modify: `docs/plans/tree-sitter-context-cli-v1-contract.md` (add compact section)

**Approach:**
- Write contract document defining S-expression schema for compact output, error codes, and CLI flag semantics.
- Include examples for all three output sections (`preserved`, `signatures_only`, `omitted`).
- Update CLI contract doc to reference compact command.
- Add compact output round-trip test to pi-mono compat tests.
- Optionally create Node.js harness script that validates the full flow.

**Patterns to follow:**
- `docs/plans/tree-sitter-context-cli-v1-contract.md` (structure and tone)
- `docs/plans/tree-sitter-context-invalidation-contract.md` (contract doc depth)
- `pi-mono/packages/coding-agent/test/tree-sitter-context-compat.test.ts` (round-trip test pattern)

**Test scenarios:**
- E2E: pi-mono extension loads, tool is callable, returns expected shape for real Rust file edit.
- Contract: all S-expression examples in contract doc are valid per schema.
- Integration: updating contract doc does not break existing R0-R3 acceptance tests.
- Compat: pi-mono S-expression parser can parse and re-emit Rust CLI compact output.

**Verification:**
- Contract document is complete enough for external implementers to consume.
- pi-mono compat tests pass.
- R0-R3 existing tests still pass after all changes.

---

## System-Wide Impact

- **Interaction graph:** New `compact` command sits beside existing `bundle`, `graph`, `orientation`, `invalidate` in `tree-sitter-context` binary. pi-mono extension tool calls CLI via subprocess, same pattern as `get_context_bundle` and `get_invalidated_chunks`.
- **Error propagation:** CLI errors propagate as non-zero exit + stderr. pi-mono bridge converts to typed error results with `errorKind` field.
- **State lifecycle risks:** No state persisted between calls; temp files for old content are written and cleaned up by OS.
- **API surface parity:** Output format follows R0-R3 S-expression conventions; field names match schema types.
- **Integration coverage:** Unit tests prove compaction logic; CLI tests prove command wiring; pi-mono integration proves end-to-end flow.
- **Unchanged invariants:** Existing `bundle`, `graph`, `orientation`, `invalidate` commands unchanged; existing `get_context_bundle` and `get_invalidated_chunks` tools unchanged; main `tree-sitter` CLI `context --old` unchanged.

---

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Tags queries do not reliably extract signatures for all languages | Start with Rust fixtures; report missing signatures as degraded confidence rather than failing. |
| S-expression schema mismatch between Rust serializer and pi-mono parser | Use existing pi-mono `parseSExpr` which handles canonical v1 subset; add golden tests for byte-stability. |
| Base64 encoding/decoding overhead for large files | Only old content is base64-encoded; new content read from disk. Large files may use `--old` path instead of base64 in future enhancement. |
| Temp file cleanup failures | Use OS temp directory; files auto-cleaned by OS on reboot. Not a correctness issue. |
| Budget enforcement discards too aggressively | Default to no budget; make budget opt-in. Include `omitted` metadata so agents know what was dropped. |
| pi-mono type checker rejects bridge/tool changes | Run `npm run check` after changes; follow existing TypeScript patterns (no `any`, no inline imports). |

---

## Documentation / Operational Notes

- Create new `docs/plans/tree-sitter-context-compact-contract.md` with S-expression schema examples.
- Update `docs/plans/tree-sitter-context-cli-v1-contract.md` to include `compact` command specification.
- No README changes needed until feature stabilizes (following R0-R3 pattern).
- For pi-mono: extension tool is opt-in, not loaded by default (same as `get_context_bundle`).

---

## Sources & References

- **Origin document:** [docs/brainstorms/2026-04-28-semantic-session-compaction-requirements.md](docs/brainstorms/2026-04-28-semantic-session-compaction-requirements.md)
- Related code: `crates/context/src/invalidation.rs`, `crates/context/src/symbols.rs`, `crates/context/src/sexpr.rs`, `crates/cli/src/context_invalidate.rs`
- Related plans: `docs/plans/2026-04-27-002-feat-incremental-invalidation-plan.md`, `docs/plans/tree-sitter-context-cli-v1-contract.md`
- Existing patterns: `pi-mono/packages/coding-agent/src/core/tree-sitter-context/tool.ts`, `pi-mono/packages/coding-agent/src/core/tree-sitter-context/bridge.ts`
