# Brainstorm Handoff: AST-Aware Read Tool

**Date:** 2026-04-28  
**Status:** Implemented — All Phase 1 features merged to `feat/ast-aware-read-tool`  
**Requirements doc:** [`docs/brainstorms/2026-04-28-ast-aware-read-tool-requirements.md`](docs/brainstorms/2026-04-28-ast-aware-read-tool-requirements.md)  
**Implementation plan:** [`docs/plans/2026-04-28-002-feat-ast-aware-read-tool-plan.md`](docs/plans/2026-04-28-002-feat-ast-aware-read-tool-plan.md)

---

## TL;DR

Replaced "read entire files" with "read semantic units." Three new agent-facing tools (`read_ast_outline`, `read_symbol`, `read_ast_delta`) wrap existing `tree-sitter-context` CLI capabilities, with `read_raw` retained for fallback. Agents can now navigate code by structure, read only what they need, and refresh only what changed after edits.

---

## Key Decisions

1. **Four-layer read architecture:** `read_ast_outline` (structure) → `read_symbol` (semantic unit) → `read_ast_delta` (incremental refresh) → `read_raw` (fallback). The default mental model shifts from "file-based reading" to "semantic-unit-based reading."

2. **Re-wrap rather than refactor:** Phase 1 wraps existing `get_context_bundle` and `get_invalidated_chunks` capabilities in agent-friendly interfaces, rather than refactoring tree-sitter-context internals. This minimizes risk and allows rapid validation. The only new CLI command needed is `tree-sitter-context outline`.

3. **Stable IDs exclude body hash:** `stable_id` is a stable semantic identifier (e.g., `rust:fn:process_payment`). Body hash and signature hash are separate, mutable fields. This allows tracking "the same function changed" across edits.

4. **get_semantic_compact is not part of the read layer:** It is a separate context-management tool for compressing already-read context. It operates on a different concern (session memory compaction vs. file reading).

5. **Two-tier snapshot storage:** Session memory holds `file_path → snapshot_id` mappings. `tree-sitter-context` disk cache holds the actual AST snapshots and hash indexes. Agents pass `snapshot_id`; the CLI handles complex state.

---

## Problem Solved

pi-mono coding agents waste tokens by reading entire files when they only need specific functions or classes. After editing, agents lack structured incremental state — they cannot distinguish "this function's body changed" from "this function's signature changed" from "this function is unchanged." The result is repeated full-file reads, inflated context windows, and slower task completion.

**Success criteria:** Agents stop re-reading entire files after edits. Token usage for file reading decreases measurably on tasks involving files >200 lines.

---

## Requirements Overview (R1–R13)

| ID | Requirement | Wraps existing? |
|----|------------|-----------------|
| R1 | `read_ast_outline` returns structured file outline with stable IDs, signatures, ranges | **New CLI command** `tree-sitter-context outline` |
| R2 | `read_symbol` accepts stable ID, returns source + metadata | `get_context_bundle` |
| R3 | `read_ast_delta` accepts file + `since_snapshot_id`, returns changed symbols | `get_invalidated_chunks` |
| R4 | Retain existing `read` as `read_raw` fallback | — |
| R5 | Distinguish `body_changed` vs `signature_changed` | Enhanced `get_invalidated_chunks` output |
| R6 | Two-tier snapshot storage (session memory + disk cache) | New infrastructure |
| R7 | Delta computation by CLI, not agent | — |
| R8–R10 | Unified schema, stable IDs, explicit baseline | Schema contract |
| R11–R13 | CLI integration details | Implementation |

---

## Scope Boundaries

**In Phase 1 (MVP):**
- `read_ast_outline`, `read_symbol`, `read_ast_delta`, `read_raw`
- One new CLI command (`outline`)
- Unified schema for common fields
- Per-session memory + basic disk cache

**Deferred (Phase 2+):**
- Callers/references analysis
- Cross-file dependency graph
- Test file discovery
- Multi-session snapshot persistence
- Tree-sitter-context internal refactoring

**Out of scope:**
- Replacing `read_raw` entirely
- Mandatory AST-aware reads
- Full IDE symbol index or LSP server

---

## Dependencies

- `tree-sitter-context` CLI: `bundle`, `invalidate` commands exist
- `tree-sitter-context` can generate deterministic symbol outlines
- pi-mono extension tool architecture supports new tools
- Agents can understand structured output

---

## Outstanding Questions (Resolved in Planning)

All deferred questions have been resolved in the implementation plan:

- **Outline command implementation:** Uses `chunks_for_tree` with extended `ChunkRecord` (signature/body hashes). Output is canonical S-expression.
- **Disk cache format:** JSON files at `.tree-sitter-context-mcp/cache/`, keyed by `snapshot_id`, max 1000 entries with count-based eviction.
- **Body vs signature change algorithm:** Compares `signature_hash` and `body_hash` on matched chunks. Heuristic: body children identified by field name `body` or kind `block`/`statement_block`/`*_body`.
- **`read_symbol` scope:** Returns only target symbol's source + metadata. No dependency inclusion in Phase 1.

See [`docs/plans/2026-04-28-002-feat-ast-aware-read-tool-plan.md`](docs/plans/2026-04-28-002-feat-ast-aware-read-tool-plan.md) for full details.

---

## Relationship to Existing Work

This brainstorm follows the completed **Semantic Session Compaction** feature (`feat/incremental-invalidation` branch). Key existing capabilities to leverage:

- `crates/context/src/chunk.rs` — `chunks_for_tree()` generates semantic chunks with stable IDs
- `crates/context/src/invalidation.rs` — `invalidate_snapshot()` classifies chunk changes
- `crates/context/src/bundle.rs` — `bundle_chunks()` for token-budgeted context extraction
- `crates/cli/src/context_bundle.rs` — `get_context_bundle` extension tool
- `crates/cli/src/context_invalidate.rs` — `get_invalidated_chunks` extension tool
- `../pi-mono/packages/coding-agent/src/core/tree-sitter-context/` — Bridge and tool registration patterns

---

## Implementation Summary

All 6 implementation units from the plan have been completed:

| Unit | Status | Commit |
|------|--------|--------|
| U1 — Extend `ChunkRecord` with `signature_hash` / `body_hash` | ✅ Done | `9fec6151` |
| U2 — Snapshot disk cache | ✅ Done | `440fcd4a` |
| U3 — `outline` CLI command | ✅ Done | `795b42d5` |
| U4 — Enhanced invalidation with change classification | ✅ Done | `dfb883ac` |
| U5 — Tool integration & schema contract | ✅ Done | `3f804793` |
| U6 — Integration tests | ✅ Done | (part of above) |

## Recommended Next Step

→ Review and merge `feat/ast-aware-read-tool` to `rfc-tree-sitter-context`, or proceed to **Phase 2+** features (callers/references analysis, cross-file dependency graph, multi-session snapshot persistence).
