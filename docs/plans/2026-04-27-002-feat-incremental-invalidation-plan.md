---
title: "feat: Add incremental invalidation CLI and pi-mono tool"
type: feat
status: completed
date: 2026-04-27
origin: docs/ideation/2026-04-26-tree-sitter-pi-integration-ideation.md
---

# feat: Add incremental invalidation CLI and pi-mono tool

## Overview

This plan adds `tree-sitter-context invalidate` CLI command and the pi-mono `get_invalidated_chunks` extension tool. It exposes the existing `crates/context/src/invalidation.rs` infrastructure through the dedicated context binary, enabling pi-mono agents to detect which semantic chunks changed after file edits. Instead of re-reading entire files after edits, agents can query for specifically affected chunks and refresh only stale context, reducing token waste.

The work follows R0-R3 established patterns: S-expression canonical output as default, JSON as optional escape hatch, byte-stable serialization, typed error results, and extension tool registration in pi-mono. It does not introduce stateful caching or daemon mode—invalidation remains a stateless comparison between two file states.

---

## Problem Frame

After completing R0-R3, pi-mono can retrieve semantic chunks via `get_context_bundle` with stable identities. However, when the agent edits a file, it currently has no mechanism to determine which previously-read chunks are now stale. The agent must either:

1. Re-read the entire file (wasteful, defeats the purpose of semantic chunking)
2. Continue with potentially stale context (risky, may lead to decisions based on outdated code)
3. Re-read all previously accessed chunks conservatively (better but still imprecise)

The Rust side already has sophisticated invalidation logic in `crates/context/src/invalidation.rs:20-240` with `invalidate_snapshot()` and `invalidate_edits()` functions that classify chunks as `affected`, `added`, `removed`, or `unchanged` using stable identity matching and tree-sitter's `changed_ranges`. This infrastructure is only exposed through the legacy `tree-sitter context --old` command in the main CLI, which outputs JSON and is not integrated with the pi-mono extension tool architecture.

This plan bridges that gap by:
1. Adding canonical S-expression serialization for `InvalidationOutput`
2. Exposing snapshot-based invalidation through the `tree-sitter-context` dedicated binary
3. Adding a pi-mono extension tool that wraps the CLI for ergonomic agent usage

---

## Requirements Trace

- **R1.** Add `tree-sitter-context invalidate <new-path> --old <old-path> [--format sexpr|json]` CLI command.
- **R2.** Implement canonical S-expression serializer for `InvalidationOutput` matching R0-R3 canonical form conventions.
- **R3.** Support both full snapshot comparison (file-based) and JSON output format parity with existing main CLI.
- **R4.** Add pi-mono `get_invalidated_chunks` extension tool with parameters: `path`, `old_content_base64`, optional `previous_stable_ids` filter.
- **R5.** pi-mono tool returns structured result with `affected`, `added`, `removed`, `unchanged` chunk classifications.
- **R6.** Maintain R0-R3 S-expression canonical guarantees: deterministic ordering, no timestamps, no absolute paths in output.
- **R7.** Support optional `previous_stable_ids` filter in pi-mono tool to limit results to chunks the agent previously read.
- **R8.** Preserve existing `get_context_bundle` behavior—no breaking changes to R0-R3 contracts.
- **R9.** Add CLI integration tests following existing `context_bundle_test.rs` patterns.
- **R10.** Document the invalidation contract and wire format for future R4+ graph-aware invalidation work.

**Origin actors:** A1 pi-mono agent runtime, A2 `tree-sitter-context` CLI, A3 agent operator, A4 future graph invalidation implementer.

**Origin flows:** F1 agent reads chunks before edit, F2 agent edits file, F3 agent queries invalidated chunks, F4 agent selectively refreshes stale chunks.

**Origin acceptance examples:** AE1 agent detects body-only function change as single affected chunk, AE2 agent detects signature change as removed+added chunks, AE3 whitespace-only change returns no affected chunks, AE4 unknown file returns typed error, AE5 pi-mono filter limits results to requested stable_ids.

---

## Scope Boundaries

- Only snapshot-based invalidation (`invalidate_snapshot`) in v1; edit-stream invalidation (`invalidate_edits`) is deferred.
- No stateful caching or daemon mode; each invalidation call is independent.
- No automatic agent refresh logic; the tool only reports staleness, agent decides what to re-read.
- No integration with graph store or cross-file dependencies; invalidation is single-file only.
- No changes to existing `bundle`, `graph`, or `orientation` commands.
- No changes to main `tree-sitter` CLI (`context --old` remains as legacy JSON path).

### Deferred to Follow-Up Work

- Edit-stream invalidation accepting byte-range edits: separate tool addition after v1 proves snapshot approach.
- Stateful chunk caching in pi-mono with automatic background invalidation: requires daemon evaluation per R12 gate.
- Cross-file impact analysis: requires R4+ graph reachability queries.
- Integration with compaction/orientation: future v2 work when agent context window management is redesigned.

---

## Context & Research

### Relevant Code and Patterns

- `crates/context/src/invalidation.rs:20-240` — Core invalidation logic with `invalidate_snapshot()` and `invalidate_edits()` functions. Already classifies chunks using stable identity matching and tree-sitter's changed ranges.
- `crates/context/src/sexpr.rs` — Existing S-expression serializers for `BundleResult` and `OrientationBlock`. Uses deterministic ordering, flat form for atomic children, multi-line for nested structures.
- `crates/context/src/schema.rs` — `InvalidationOutput`, `InvalidationRecord`, `InvalidationStatus`, `InvalidationReason`, `MatchStrategy` types. These need S-expression serialization.
- `crates/cli/src/bin/tree-sitter-context.rs` — CLI dispatch for `bundle`, `graph`, and `orientation` commands. Uses `clap` derive macros with nested subcommand enums.
- `crates/cli/src/tests/context_bundle_test.rs` — CLI integration test pattern: spawn binary via `Command::new()`, assert on exit codes and stderr patterns.
- `pi-mono/packages/coding-agent/src/core/tree-sitter-context/tool.ts` — Existing `get_context_bundle` tool registration pattern using `defineTool()` from `@mariozechner/pi-coding-agent`.
- `pi-mono/packages/coding-agent/src/core/tree-sitter-context/bridge.ts` — Bridge pattern: validates inputs, spawns CLI, parses/canonicalizes stdout, returns typed results.
- `pi-mono/packages/coding-agent/src/core/tree-sitter-context/sexpr.ts` — S-expression parser for canonical v1 subset. Consumption-only on TypeScript side.

### Institutional Learnings

- `docs/plans/2026-04-26-001-feat-r0-context-firewall-plan.md` — Established S-expression canonical form conventions: two-space indentation, deterministic ordering, string escaping subset, no comments, stable negative-result forms.
- `docs/plans/tree-sitter-context-cli-v1-contract.md` — CLI contract rules: stdout for success, stderr + non-zero exit for errors, typed error prefixes, byte-level golden gates for determinism.
- `docs/solutions/workflow-issues/tree-sitter-context-branch-review-2026-04-25.md` — "Every primitive must surface reason / strategy / confidence / omissions." Invalidation output already has these fields.
- `crates/context/src/graph/snapshot.rs` — Atomic write pattern (temp file + fsync + rename) for durability; invalidation is read-only but follows same error handling discipline.

### External References

- Tree-sitter `changed_ranges` API documentation for understanding the foundation of invalidation detection.
- Similar implementation in Aider repo-map (Python) for invalidation after edits—validates that this approach works in production agents.

---

## Key Technical Decisions

- **Expose through dedicated binary, not main CLI:** Rationale: `tree-sitter-context` is the R0-R3 integration point for pi-mono. Keeping all context-aware tools in one binary maintains consistency.
- **S-expression default, JSON optional:** Rationale: Matches `bundle` and `orientation` conventions; S-expressions are prompt-cache-friendly for future LLM-facing outputs.
- **File-based snapshot comparison, not edit-stream:** Rationale: Simpler CLI interface, agent can provide old content via temp file. Edit-stream requires byte-range calculations that couple poorly with pi-mono's string-based edit tool.
- **Optional `previous_stable_ids` filter in pi-mono layer, not CLI:** Rationale: CLI remains general-purpose; filtering is an agent convenience that belongs in the extension tool layer.
- **No edit-stream invalidation in v1:** Rationale: Agent can achieve same result by saving old content before edit. Edit-stream adds complexity (byte position calculation) without clear benefit for the stateless CLI model.

---

## Open Questions

### Resolved During Planning

- **Edit-stream vs snapshot invalidation:** Snapshot comparison is sufficient; edit-stream deferred to follow-up.
- **CLI flag design:** `--old <path>` required flag with `<new-path>` as positional argument; mirrors `tree-sitter context --old` but in dedicated binary.
- **Filter by previous stable_ids:** Implemented in pi-mono tool layer, not CLI; keeps CLI general and matches agent use case.
- **Output format:** S-expression default with `--format json` escape hatch; matches R0-R3 pattern.

### Deferred to Implementation

- **Exact S-expression schema for InvalidationRecord:** Implementer should align with existing `sexpr.rs` conventions; add inline tests for byte-stability.
- **Whether to include changed_ranges in output:** Yes for completeness, but mark as non-contract metadata.
- **Error code mapping:** Use same typed error prefixes as bundle/orientation: `no_graph`, `file_not_found`, `parse_error`, etc.

---

## Output Structure

```text
crates/context/src/
  sexpr.rs                    # Add invalidation_serializer module
  
crates/cli/src/
  bin/tree-sitter-context.rs  # Add Invalidate command variant
  context_invalidate.rs       # New: invalidate command implementation
  tests/
    context_invalidate_test.rs # New: CLI integration tests

pi-mono/packages/coding-agent/src/core/tree-sitter-context/
  tool.ts                     # Add get_invalidated_chunks tool
  bridge.ts                   # Add getInvalidatedChunks bridge function
  types.ts                    # Add InvalidationResult types

docs/plans/
  tree-sitter-context-invalidation-contract.md  # New: wire format spec
```

---

## High-Level Technical Design

> *This illustrates the intended approach and is directional guidance for review, not implementation specification. The implementing agent should treat it as context, not code to reproduce.*

### CLI Invocation Flow

```
pi-mono agent → get_invalidated_chunks tool
    ↓ (writes old_content to temp file)
bridge.ts → spawn tree-sitter-context invalidate <current-path> --old <temp-path>
    ↓
tree-sitter-context.rs → dispatch to run_invalidate()
    ↓
context_invalidate.rs → parse both files, chunk both, run invalidate_snapshot()
    ↓
sexpr.rs → serialize InvalidationOutput to canonical S-expression
    ↓
bridge.ts → parse S-expression, filter by previous_stable_ids if provided
    ↓
agent receives: { affected: [...], added: [...], removed: [...], unchanged: [...] }
```

### S-expression Output Sketch

```lisp
(invalidation
  (schema_version "1")
  (affected
    ((stable_id "named:abc123...")
     (kind "function")
     (name "foo")
     (path "src/lib.rs")
     (confidence "High")
     (reason "ContentChanged")
     (strategy "StableId")))
  (added (...))
  (removed (...))
  (unchanged (...))
  (changed_ranges ((start 100) (end 200)))
  (meta
    ((total_chunks 42)
     (source_path "src/lib.rs"))))
```

---

## Implementation Units

- U1. **Invalidation S-expression serializer**

**Goal:** Add canonical S-expression serialization for `InvalidationOutput` and related types in `crates/context/src/sexpr.rs`.

**Requirements:** R2, R6

**Dependencies:** None

**Files:**
- Modify: `crates/context/src/sexpr.rs`
- Test: `crates/context/src/sexpr.rs` (inline `#[cfg(test)]` for byte-stability)

**Approach:**
- Add `invalidation_to_sexpr(output: &InvalidationOutput) -> String` function following existing `bundle_to_sexpr` and `orientation_to_sexpr` patterns.
- Serialize all `InvalidationRecord` fields: status, chunk (stable_id, kind, name, path, confidence), reason, match_strategy, changed_ranges.
- Use deterministic ordering: affected, added, removed, unchanged, changed_ranges, meta.
- Sort records within each bucket by stable_id for byte stability.
- Emit flat form `(affected (...))` with nested record lists.

**Execution note:** Start with inline unit tests asserting `parse(emit(x)) == emit(x)` byte-stability.

**Patterns to follow:**
- `crates/context/src/sexpr.rs` lines 264-280 (Provenance serialization)
- `crates/context/src/sexpr.rs` lines 386-413 (Orientation block serialization)
- Existing escape_string and indentation helpers.

**Test scenarios:**
- Happy path: serialize complex InvalidationOutput with all four buckets populated → parse and re-emit produces identical bytes.
- Edge case: empty file (no chunks) → produces empty buckets, not error.
- Edge case: whitespace-only change → affected bucket empty, all chunks in unchanged.
- Edge case: function signature change → old function in removed, new in added, none in affected (since stable_id changes).
- Edge case: body-only change → function in affected with reason ContentChanged.
- Error path: invalid UTF-8 in source → handled during file read, not serialization.

**Verification:**
- `cargo test -p tree-sitter-context` passes including new inline tests.
- Manual verification: `echo '(invalidation ...)' | cargo run --bin tree-sitter-context -- invalidate --format sexpr` produces identical bytes on second run.

---

- U2. **CLI invalidate command**

**Goal:** Add `tree-sitter-context invalidate` command to dedicated binary, wiring to existing `invalidate_snapshot()` logic.

**Requirements:** R1, R3

**Dependencies:** U1

**Files:**
- Create: `crates/cli/src/context_invalidate.rs`
- Modify: `crates/cli/src/bin/tree-sitter-context.rs`
- Modify: `crates/cli/src/lib.rs` (if needed for module export)

**Approach:**
- Add `Invalidate(InvalidateArgs)` variant to `Commands` enum with fields: `new_path: PathBuf`, `old_path: PathBuf`, `format: OutputFormat`.
- Implement `run_invalidate(args: InvalidateArgs) -> Result<()>` in new `context_invalidate.rs` module.
- Load language configuration via `tree-sitter-loader` (reuse existing loader setup pattern from `context.rs`).
- Parse both old and new files using tree-sitter Parser.
- Call `invalidate_snapshot(old_tree, new_tree, old_source, new_source, path)` from `crates/context/src/invalidation.rs`.
- Serialize output: if format is Sexpr, use U1 serializer; if Json, use existing serde serialization.
- Write to stdout on success; write typed errors to stderr with non-zero exit codes.

**Execution note:** Test CLI manually with fixture files before writing integration tests.

**Patterns to follow:**
- `crates/cli/src/bin/tree-sitter-context.rs` lines 107+ (BundleArgs pattern)
- `crates/cli/src/context.rs` lines 20-80 (loader setup and file parsing)
- `crates/cli/src/context_graph.rs` lines 217-238 (error handling with typed prefixes)

**Test scenarios:**
- Happy path: invalidate between two real Rust files with changes → exit 0, stdout contains sexpr with affected chunks.
- Happy path: `--format json` → valid JSON output matching sexpr structure.
- Edge case: identical files → all chunks in unchanged, affected/added/removed empty.
- Error path: missing `--old` flag → exit non-zero, stderr contains usage help.
- Error path: unreadable old file → exit non-zero, stderr contains `file_not_found` or `permission_denied`.
- Error path: parse error in either file → exit non-zero, stderr contains `parse_error`.

**Verification:**
- `cargo run --bin tree-sitter-context -- invalidate --help` shows correct usage.
- Manual test: `tree-sitter-context invalidate new.rs --old old.rs` produces expected output.

---

- U3. **pi-mono extension tool and bridge**

**Goal:** Add `get_invalidated_chunks` extension tool to pi-mono that wraps the CLI and provides ergonomic API for agents.

**Requirements:** R4, R5, R7

**Dependencies:** U2

**Files:**
- Modify: `pi-mono/packages/coding-agent/src/core/tree-sitter-context/tool.ts`
- Modify: `pi-mono/packages/coding-agent/src/core/tree-sitter-context/bridge.ts`
- Create: `pi-mono/packages/coding-agent/src/core/tree-sitter-context/types.ts` (if new types needed)

**Approach:**
- Define tool parameters: `path: string`, `old_content_base64: string`, `previous_stable_ids?: string[]`.
- In bridge.ts, implement `getInvalidatedChunks(input, cwd, cliPath)`:
  1. Decode base64 old_content to bytes.
  2. Write to temp file in OS temp directory.
  3. Spawn CLI: `tree-sitter-context invalidate <path> --old <temp-path>`.
  4. Parse stdout S-expression using existing `parseSExpr` from `sexpr.ts`.
  5. If `previous_stable_ids` provided, filter each bucket to only include those stable_ids.
  6. Return typed result with arrays of chunk objects.
- In tool.ts, register tool using `defineTool()` following `get_context_bundle` pattern.
- Handle errors: CLI non-zero exit → return typed error result; parse errors → return `invalid-output` error.

**Execution note:** Use base64 encoding to safely pass binary content across JSON-RPC boundary; pi-mono extension tools receive JSON-serializable parameters.

**Patterns to follow:**
- `pi-mono/packages/coding-agent/src/core/tree-sitter-context/tool.ts` lines 11-84 (get_context_bundle tool)
- `pi-mono/packages/coding-agent/src/core/tree-sitter-context/bridge.ts` lines 33-109 (getContextBundle bridge)
- `pi-mono/packages/coding-agent/src/core/exec.ts` (execCommand for subprocess spawning)

**Test scenarios:**
- Happy path: tool call with valid path and old_content → returns structured result with correct classifications.
- Happy path: with previous_stable_ids filter → only requested IDs appear in result.
- Edge case: empty previous_stable_ids array → returns empty buckets (not error).
- Error path: invalid base64 → returns validation error before spawning CLI.
- Error path: CLI process error → returns process-error with stderr content.
- Error path: malformed CLI stdout → returns invalid-output error.

**Verification:**
- Loading the extension makes `get_invalidated_chunks` available to pi-mono.
- Tool can be invoked through pi-mono's tool execution path.

---

- U4. **CLI integration tests**

**Goal:** Add comprehensive CLI integration tests for invalidate command following existing test patterns.

**Requirements:** R9

**Dependencies:** U2

**Files:**
- Create: `crates/cli/src/tests/context_invalidate_test.rs`

**Approach:**
- Create test fixtures: `old_func.rs` and `new_func.rs` with various change types (body change, signature change, added function, removed function).
- Follow pattern from `context_bundle_test.rs`: use `Command::new(tree_sitter_context_bin())`, spawn with args, assert on `output.status.success()` and stderr content.
- Test both success and error paths.
- Test byte-stability: run same command twice, compare stdout bytes.

**Execution note:** Run tests with `cargo test -p tree-sitter-cli --test context_invalidate_test`.

**Patterns to follow:**
- `crates/cli/src/tests/context_bundle_test.rs` (entire file structure)
- `crates/cli/src/tests/fixtures/` directory for test fixtures.

**Test scenarios:**
- Covers AE1: body-only change → exactly one affected chunk, stable_id unchanged.
- Covers AE2: signature change → one removed, one added, stable_ids differ.
- Covers AE3: whitespace-only reformat → all chunks in unchanged, affected empty.
- Covers AE4: missing new file → non-zero exit, stderr contains error.
- Error path: missing `--old` flag → non-zero exit, usage shown.
- Regression: `--format sexpr` and `--format json` produce equivalent semantic content.
- Regression: byte-stability for repeated runs on same fixtures.

**Verification:**
- All tests pass: `cargo test -p tree-sitter-cli --test context_invalidate_test`.
- Test fixtures are committed in `crates/cli/src/tests/fixtures/`.

---

- U5. **End-to-end validation and documentation**

**Goal:** Validate full flow from pi-mono through CLI and back, document the contract for future implementers.

**Requirements:** R10

**Dependencies:** U3, U4

**Files:**
- Create: `docs/plans/tree-sitter-context-invalidation-contract.md`
- Create: `scripts/invalidation-e2e-harness.mjs` (optional, if E2E harness valuable)
- Modify: `docs/plans/tree-sitter-context-cli-v1-contract.md` (add invalidate section)

**Approach:**
- Write contract document defining S-expression schema for invalidation output, error codes, and CLI flag semantics.
- Include examples for all four bucket types (affected, added, removed, unchanged).
- Update CLI contract doc to reference invalidate command.
- Optionally create Node.js harness script (like R2 orientation handshake) that:
  1. Creates temp files with old/new content
  2. Calls CLI directly
  3. Parses S-expression
  4. Validates schema

**Execution note:** Document-first approach: write contract before finalizing implementation details.

**Patterns to follow:**
- `docs/plans/tree-sitter-context-cli-v1-contract.md` (structure and tone)
- `docs/plans/r0-orientation-compaction-v2-contract.md` (contract doc depth)
- `scripts/orientation-handshake-harness.mjs` (harness pattern if applicable)

**Test scenarios:**
- E2E: pi-mono extension loads, tool is callable, returns expected shape for real Rust file edit.
- Contract: all S-expression examples in contract doc are valid per schema.
- Integration: updating contract doc does not break existing R0-R3 acceptance tests.

**Verification:**
- Contract document is complete enough for external implementers to consume.
- Optional harness runs successfully in CI (if implemented).
- R0-R3 existing tests still pass after all changes.

---

## System-Wide Impact

- **Interaction graph:** New `invalidate` command sits beside existing `bundle`, `graph`, `orientation` in `tree-sitter-context` binary. pi-mono extension tool calls CLI via subprocess, same pattern as `get_context_bundle`.
- **Error propagation:** CLI errors (missing file, parse error) propagate as non-zero exit + stderr. pi-mono bridge converts to typed error results with `errorKind` field.
- **State lifecycle risks:** No state persisted between calls; temp files for old content are written and should be cleaned up by OS (use std::env::temp_dir()).
- **API surface parity:** Output format follows R0-R3 S-expression conventions; field names match `InvalidationRecord` schema.
- **Integration coverage:** Unit tests prove serialization correctness; CLI tests prove command wiring; pi-mono integration proves end-to-end flow.
- **Unchanged invariants:** Existing `bundle`, `graph`, `orientation` commands unchanged; existing `get_context_bundle` tool unchanged; main `tree-sitter` CLI `context --old` unchanged (legacy path).

---

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| S-expression schema mismatch between Rust serializer and pi-mono parser | Use existing pi-mono `parseSExpr` which handles canonical v1 subset; add golden tests for byte-stability. |
| Base64 encoding/decoding overhead for large files | Mitigation: only old content is base64-encoded; new content read from disk. Large files may use `--old` path instead of base64 in future enhancement. |
| Temp file cleanup failures | Use OS temp directory; files auto-cleaned by OS on reboot. Not a correctness issue. |
| Breaking change to InvalidationRecord schema in future | Lock schema version in output; contract doc defines stability guarantees. |
| pi-mono extension tool discovery/loading issues | Follow exact same registration pattern as `get_context_bundle` which is proven. |

---

## Documentation / Operational Notes

- Update `docs/plans/tree-sitter-context-cli-v1-contract.md` to include `invalidate` command specification.
- Create new `docs/plans/tree-sitter-context-invalidation-contract.md` with S-expression schema examples.
- No README changes needed until feature stabilizes (following R0-R3 pattern).
- For pi-mono: extension tool is opt-in, not loaded by default (same as `get_context_bundle`).

---

## Sources & References

- **Origin document:** [docs/ideation/2026-04-26-tree-sitter-pi-integration-ideation.md](docs/ideation/2026-04-26-tree-sitter-pi-integration-ideation.md)
- Related code: `crates/context/src/invalidation.rs`, `crates/context/src/sexpr.rs`, `crates/cli/src/bin/tree-sitter-context.rs`
- Related plans: `docs/plans/2026-04-26-001-feat-r0-context-firewall-plan.md`, `docs/plans/tree-sitter-context-cli-v1-contract.md`
- Existing patterns: `pi-mono/packages/coding-agent/src/core/tree-sitter-context/tool.ts`, `pi-mono/packages/coding-agent/src/core/tree-sitter-context/bridge.ts`
