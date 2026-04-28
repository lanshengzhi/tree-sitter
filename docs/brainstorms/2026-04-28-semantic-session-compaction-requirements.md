---
date: 2026-04-28
topic: semantic-session-compaction
---

# Semantic Session Compaction

## Problem Frame

pi-mono's current session compaction uses LLM summarization to compress conversation history when sessions exceed token budgets. This approach has three concrete problems:

1. **Information loss**: LLM summaries drop function signatures, type information, and implementation details that later turns need to reason correctly about code.
2. **Token waste on unchanged code**: When agents re-read files after compaction, they receive entire files even though most functions haven't changed since the last read.
3. **Compaction latency/cost**: Every compaction requires an LLM API call, adding latency and cost that pure AST operations could avoid.

tree-sitter-context now has the infrastructure to solve this: stable chunk identities, incremental invalidation (detecting affected vs unchanged chunks), and symbol extraction via tags queries. Semantic Session Compaction leverages this infrastructure to replace text-truncating compaction with structure-preserving compression: keep full content for changed chunks, compress unchanged chunks to signatures.

---

## Actors

- A1. **pi-mono agent**: The coding agent running in a session, which reads files, edits code, and may explicitly request compaction.
- A2. **Agent operator**: The human user or orchestrator managing the session, who may trigger compaction manually or observe its effects.
- A3. **tree-sitter-context CLI**: The Rust CLI that performs AST analysis, invalidation, and signature extraction.
- A4. **Extension tool bridge**: The pi-mono TypeScript bridge that wraps CLI calls and formats results for the agent.

---

## Key Flows

- F1. **Explicit semantic compaction**
  - **Trigger:** Agent operator or agent itself decides to compact session context (e.g., `/semantic-compact` command, or automatic trigger when token budget is near exhaustion).
  - **Actors:** A1, A2, A3, A4
  - **Steps:**
    1. Agent identifies files referenced in current session.
    2. pi-mono extension tool (`get_semantic_compact`) receives file paths and old content/snapshot reference.
    3. Bridge spawns `tree-sitter-context compact` CLI for each file pair (old vs current).
    4. CLI parses both files, chunks both, runs `invalidate_snapshot` to classify chunks.
    5. For affected/added/removed chunks: emit full chunk content.
    6. For unchanged chunks: extract signature via tags/symbols, discard body.
    7. CLI returns compacted bundle with token stats.
    8. Bridge parses S-expression, returns structured result to agent.
    9. Agent replaces old file reads in session context with compacted representation.
  - **Outcome:** Session context is reduced in tokens while preserving all changed code and signatures of unchanged code.
  - **Covered by:** R1, R2, R3, R4, R5, R6

- F2. **Token budget enforcement**
  - **Trigger:** Compaction result exceeds the requested token budget.
  - **Actors:** A1, A3
  - **Steps:**
    1. CLI computes `compacted_tokens` after classification and signature extraction.
    2. If `compacted_tokens > --budget`, CLI discards lowest-priority chunks (unchanged signatures first, then least-recently-referenced affected chunks).
    3. CLI reports what was omitted and why.
    4. If still over budget, return error with `budget_exceeded` reason.
  - **Outcome:** Compaction either fits within budget or fails explicitly with diagnostics.
  - **Covered by:** R7, R8

---

## Requirements

**CLI command**
- R1. Add `tree-sitter-context compact <paths...> --old <snapshot-path-or-base64> [--budget <tokens>]` CLI command.
- R2. CLI accepts multiple file paths and processes each independently.
- R3. CLI parses old and new versions of each file, generates chunks, runs `invalidate_snapshot` classification.
- R4. For affected/added/removed chunks: emit full chunk content including body.
- R5. For unchanged named chunks (function, struct, trait, enum, etc.): extract signature via existing tags/symbols infrastructure and emit `signature_only` form.
- R6. Output is canonical S-expression following R0-R3 conventions: deterministic ordering, no timestamps, no absolute paths, byte-stable.

**Token budget**
- R7. Support `--budget <tokens>` parameter; default to no budget limit.
- R8. If compacted output exceeds budget, discard chunks in priority order: unchanged signatures first, then affected chunks by least-recently-referenced, with explicit `omitted` metadata.
- R9. If output still exceeds budget after discarding, return typed error with `budget_exceeded` reason and `required_tokens` field.

**pi-mono extension**
- R10. Add `get_semantic_compact` extension tool in pi-mono with parameters: `paths: string[]`, `old_contents_base64: Record<string, string>` (path -> base64 old content), optional `budget?: number`.
- R11. Bridge function spawns CLI for each file, parses S-expression output, assembles multi-file result.
- R12. Tool returns structured result with `preserved`, `signatures_only`, `omitted` arrays and `original_tokens` / `compacted_tokens` stats.

**Testing**
- R13. Add CLI integration tests: body-only change (function stays in preserved), signature change (old removed + new added, no affected), whitespace-only change (all unchanged, all signatures-only), missing file error.
- R14. Add byte-stability tests: same input produces identical S-expression bytes across runs.

---

## Acceptance Examples

- AE1. **Covers R1, R3, R4.** Given `old.rs` with function `foo` and `new.rs` where `foo` body changed but signature unchanged, when `compact old.rs new.rs` runs, output contains `foo` in `preserved` with full body.
- AE2. **Covers R3, R5.** Given `old.rs` with function `bar` and `new.rs` where `bar` is unchanged, output contains `bar` in `signatures_only` with `(signature "fn bar(x: i32) -> String")` and no body.
- AE3. **Covers R3, R5.** Given `old.rs` with `struct Baz { a: i32, b: String }` and `new.rs` where `Baz` is unchanged, output contains `Baz` in `signatures_only` with full struct definition (since the entire struct declaration is its signature).
- AE4. **Covers R7, R8, R9.** Given 10 files with 500 unchanged functions each and `--budget 1000`, CLI discards signatures-only chunks beyond budget and reports `omitted` count; if even preserving all affected chunks exceeds budget, returns `budget_exceeded` error.
- AE5. **Covers R13.** Given missing `--old` flag or unreadable old file, CLI exits non-zero with typed error prefix (`file_not_found`, `parse_error`).

---

## Success Criteria

- Human outcome: Agent sessions stay within token budget without losing access to changed code or function signatures; compaction is deterministic and fast (<100ms for typical files).
- Downstream handoff: Planning can proceed without inventing product behavior, scope, or success criteria; all requirements have observable behaviors; acceptance examples disambiguate edge cases.

---

## Scope Boundaries

- **Deferred for later**
  - Cross-file reference retention (e.g., keeping referenced type signatures from other files): requires R1 graph cross-file resolution, too complex for v1.
  - Auto-compaction replacement: v1 is an explicit extension tool only; replacing pi-mono's automatic compaction trigger is a later phase after the tool proves value.
  - LLM-generated summaries for affected chunks: pure signature extraction is deterministic and testable; summaries can be added later.
  - pi-mono integration/E2E tests: CLI and unit tests are in scope; full pi-mono end-to-end harness is U5-level and deferred.
  - Multi-language parity beyond Rust: start with Rust fixtures; other languages follow when tags queries are validated.

- **Outside this product's identity**
  - Replacing pi-mono's core auto-compact logic: tree-sitter-context is a low-level primitive, not a session manager.
  - MCP server or daemon mode: out of scope per RFC.
  - Cross-file impact analysis: this is Direction B (graph navigation), not compaction.

---

## Key Decisions

- **Extension tool first, not auto-compaction replacement**: R0-R3 plans explicitly prohibit replacing pi-mono core compaction. The extension tool validates semantic compression correctness before any core integration.
- **Rust CLI owns compression logic**: Signature extraction uses existing `tree-sitter-tags` infrastructure; pushing this to pi-mono would require AST parsing in TypeScript, which violates the architecture boundary.
- **S-expression default output**: Matches R0-R3 canonical form conventions; pi-mono bridge parses and assembles.
- **Multi-file batch in v1**: Agents typically work with multiple files per session; single-file-only would be too narrow to prove value.

---

## Dependencies / Assumptions

- `tree-sitter-tags` can reliably extract function/struct signatures for Rust fixtures (unverified: needs validation during planning).
- `crates/context/src/graph/snapshot.rs` snapshot format contains sufficient old chunk metadata for invalidation (unverified: needs validation during planning).
- pi-mono extension tool loading pattern (`defineTool`) supports the new tool without core changes.
- Session context includes enough file-operation history for the agent to know which files to compact.

---

## Outstanding Questions

### Resolved During Brainstorm

- [Affects R5][User decision] Unchanged anonymous chunks (e.g., `impl` blocks) are kept as full content, not compressed. Rationale: anonymous chunks have no meaningful "signature" to extract; their value is in their contents. Only named chunks (functions, structs, traits, enums) are compressed to signatures.

### Deferred to Planning

- [Affects R5][Technical] Exact algorithm for extracting "signature" from a chunk: first N lines, tags query `@definition` span, or AST node-specific logic?
- [Affects R8][Technical] Priority ordering for budget discarding: recency-based, topological, or simple file-order?
- [Affects R1][Needs research] Does the CLI accept base64-encoded old content inline, or must pi-mono write temp files (following the `get_invalidated_chunks` pattern)?

---

## Next Steps

-> /ce-plan for structured implementation planning.
