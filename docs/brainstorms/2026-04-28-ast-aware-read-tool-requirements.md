---
date: 2026-04-28
topic: ast-aware-read-tool
---

# AST-Aware Read Tool

## Problem Frame

pi-mono coding agents waste tokens by reading entire files when they only need specific functions or classes. After editing, agents lack structured incremental state — they cannot distinguish "this function's body changed" from "this function's signature changed" from "this function is unchanged." The result is repeated full-file reads, inflated context windows, and slower task completion.

The solution is not to replace the existing `read` tool, but to add structured, AST-aware read primitives that let agents read by semantic unit instead of by file. Agents get a navigable code map first, then read only the symbols they need, and after edits, refresh only what changed.

---

## Actors

- A1. **pi-mono coding agent**: The LLM agent executing coding tasks. It calls read tools, makes edits, and decides what to read next.
- A2. **Agent operator**: The human user or orchestrator observing or directing the session.
- A3. **tree-sitter-context CLI**: The Rust CLI that parses files, extracts chunks/symbols, and computes deltas.
- A4. **Extension tool bridge**: The pi-mono TypeScript layer that wraps CLI calls and manages snapshot state.

---

## Key Flows

- F1. **First encounter with a large file**
  - **Trigger:** Agent needs to understand a file it has not yet read in this session.
  - **Actors:** A1, A3, A4
  - **Steps:**
    1. Agent calls `read_ast_outline(file_path)`.
    2. Bridge spawns `tree-sitter-context outline` CLI.
    3. CLI returns symbol outline with stable IDs, signatures, ranges, and snapshot ID.
    4. Agent selects a stable ID of interest.
    5. Agent calls `read_symbol(stable_id)`.
    6. Bridge spawns `tree-sitter-context bundle` CLI (or existing `get_context_bundle`).
    7. Agent receives source of that symbol only.
  - **Outcome:** Agent understands file structure and reads targeted symbols, not the full file.
  - **Covered by:** R1, R2, R8

- F2. **Post-edit incremental refresh**
  - **Trigger:** Agent has edited a file and wants to confirm what changed.
  - **Actors:** A1, A3, A4
  - **Steps:**
    1. Agent calls `read_ast_delta(file_path, since_snapshot_id)`.
    2. Bridge retrieves the cached snapshot for that `snapshot_id` from tree-sitter-context disk cache.
    3. Bridge spawns `tree-sitter-context invalidate` CLI (or existing `get_invalidated_chunks`).
    4. CLI compares old snapshot to current file and returns changed symbols.
    5. Agent sees `body_changed` vs `signature_changed` vs `unchanged`.
    6. Agent selectively re-reads only changed symbols via `read_symbol`.
  - **Outcome:** Agent refreshes only stale context, avoiding full-file re-reads.
  - **Covered by:** R3, R5, R6, R7

- F3. **Fallback to raw text**
  - **Trigger:** Agent needs comments, string literals, non-code files, or AST-unparseable content.
  - **Actors:** A1
  - **Steps:**
    1. Agent calls existing `read(path, offset, limit)`.
    2. Agent receives raw text as before.
  - **Outcome:** Agent gets exact text when structured reading is inappropriate.
  - **Covered by:** R4

---

## Requirements

**Agent-facing read tools**

- R1. Add `read_ast_outline` tool that returns a structured file outline: symbols (functions, classes, methods) with stable IDs, kinds, names, line ranges, signatures, and hierarchical children. Output format must be agent-friendly, not a raw AST dump.
- R2. Add `read_symbol` tool that accepts a stable ID and returns the corresponding semantic unit's source code, plus metadata (signature hash, body hash, range). Internally wraps existing `get_context_bundle` CLI capability.
- R3. Add `read_ast_delta` tool that accepts a file path and a `since_snapshot_id`, and returns which symbols changed since that snapshot. Change types must include at minimum: `body_changed`, `signature_changed`, `added`, `removed`, `unchanged`.
- R4. Retain the existing `read` tool unchanged as `read_raw` for fallback scenarios: non-code files, comments, string literals, syntax errors, and precise line-level patching.

**Delta and state management**

- R5. `read_ast_delta` must distinguish `body_changed` (signature stable, body changed) from `signature_changed` (interface changed, callers may be affected).
- R6. Snapshot state uses a two-tier storage model:
  - **Session memory:** The bridge stores a lightweight mapping of `file_path → snapshot_id` for the current session.
  - **Disk cache:** `tree-sitter-context` maintains a local cache of AST snapshots and hash indexes, keyed by `snapshot_id`. The agent does not manage complex state.
- R7. Delta computation is performed by `tree-sitter-context` CLI, not by the agent. The agent passes `snapshot_id`; the CLI retrieves the cached snapshot, compares it to the current file, and returns the delta.

**Schema and contract**

- R8. All three new tools (`read_ast_outline`, `read_symbol`, `read_ast_delta`) share a unified schema for common fields: `stable_id`, `kind`, `name`, `range`, `signature`, `snapshot_id`.
- R9. `stable_id` must be stable across body changes. Body hash and signature hash are separate fields, not embedded in the `stable_id`.
- R10. `read_ast_delta` requires an explicit `since_snapshot_id` parameter. If omitted, it may fall back to the latest cached snapshot but must report the baseline source in the response.

**Integration**

- R11. `read_ast_outline` requires a new `tree-sitter-context outline` CLI command that extracts symbol outline from a file. This is the only new CLI command needed in Phase 1.
- R12. `read_symbol` wraps the existing `tree-sitter-context bundle` CLI (or `get_context_bundle` extension tool), adapting its output format to the unified schema.
- R13. `read_ast_delta` wraps the existing `tree-sitter-context invalidate` CLI (or `get_invalidated_chunks` extension tool), adapting its output to include `body_changed` vs `signature_changed` classification.

---

## Acceptance Examples

- AE1. **Covers R1, R2.** Given a 500-line Rust file with 20 functions, when the agent calls `read_ast_outline`, it receives an outline listing all 20 functions with stable IDs and line ranges. The agent then calls `read_symbol("rust:fn:process_payment@a1b2")` and receives only the 45 lines of that function, not the full 500 lines.

- AE2. **Covers R3, R5.** Given the agent has previously read a file with `snapshot_id="snap_abc123"`, when the agent edits two functions (one body-only, one signature change) and calls `read_ast_delta(file, since="snap_abc123")`, it receives:
  - `body_changed` for the first function (signature hash unchanged, body hash changed)
  - `signature_changed` for the second function (signature hash changed)
  - `unchanged` for all other functions. The agent then re-reads only the two changed functions.

- AE3. **Covers R4.** Given a YAML configuration file or a file with syntax errors, when the agent calls `read_ast_outline`, it receives an error indicating AST parsing failed. The agent then falls back to `read_raw` and receives the raw text successfully.

---

## Success Criteria

- **Human outcome:** Agents stop re-reading entire files after edits. Token usage for file reading decreases measurably on tasks involving files >200 lines. Agents make fewer "just to be safe" full-file reads.
- **Downstream handoff:** Planning can proceed without inventing product behavior, scope boundaries, or success criteria. The relationship to existing tools (`get_context_bundle`, `get_invalidated_chunks`) is explicit. Schema contracts are defined enough for implementation.

---

## Scope Boundaries

- **Deferred for later (Phase 2+):**
  - Callers/references analysis ("who calls this function")
  - Cross-file dependency graph navigation
  - Test file discovery and reading
  - Semantic compaction integration (`get_semantic_compact` remains a separate context-management tool, not part of the read layer)
  - Multi-session snapshot persistence (Phase 1 uses per-session memory + disk cache)
  - Tree-sitter-context internal refactoring into outline/symbol/delta commands

- **Outside this product's identity:**
  - Replacing pi-mono's core read tool entirely. `read_raw` remains the fallback.
  - Making AST-aware reads mandatory. Agents choose between structured and raw reads based on context.
  - Building a full IDE-like symbol index or LSP server.

---

## Key Decisions

- **Re-wrap rather than refactor:** Phase 1 wraps existing `get_context_bundle` and `get_invalidated_chunks` capabilities in agent-friendly interfaces, rather than refactoring tree-sitter-context internals. This minimizes risk and allows rapid validation.
- **Four-layer read architecture:** `read_ast_outline` (structure) → `read_symbol` (semantic unit) → `read_ast_delta` (incremental refresh) → `read_raw` (fallback). This replaces "file-based reading" with "semantic-unit-based reading" as the default mental model.
- **Stable IDs exclude body hash:** `stable_id` is a stable semantic identifier (e.g., `rust:fn:process_payment`). Body hash and signature hash are separate, mutable fields. This allows tracking "the same function changed" across edits.
- **get_semantic_compact is not part of the read layer:** It is a context-management tool for compressing already-read context. It operates on a different concern (session memory compaction vs. file reading).

---

## Dependencies / Assumptions

- `tree-sitter-context` CLI already supports `bundle` (by stable ID) and `invalidate` (snapshot diff) commands.
- `tree-sitter-context` can generate deterministic symbol outlines via `chunks_for_tree` or `symbols_for_tree`.
- pi-mono extension tool architecture (`defineTool`, bridge pattern) can accommodate new tools without core changes.
- Agents can understand and act on structured output (symbol lists, change types) rather than raw text.

---

## Outstanding Questions

### Resolve Before Planning

None.

### Deferred to Planning

- [Affects R11][Technical] Exact implementation of `tree-sitter-context outline` command: does it use `chunks_for_tree`, `symbols_for_tree`, or a combination? What is the output schema contract?
- [Affects R6][Technical] Disk cache format and location: what directory structure, serialization format, and eviction policy for the tree-sitter-context snapshot cache?
- [Affects R5][Technical] Algorithm for distinguishing `body_changed` from `signature_changed`: does it compare chunk-level hashes, tag-query spans, or tree-sitter node types?
- [Affects R2][Technical] `read_symbol` scope control: should it support `include_dependencies` parameter in Phase 1, or return only the target symbol's source?

---

## Next Steps

-> /ce-plan for structured implementation planning.
