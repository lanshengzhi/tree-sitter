---
title: AST-Aware Read Tool
type: feat
status: active
date: 2026-04-28
origin: docs/brainstorms/2026-04-28-ast-aware-read-tool-requirements.md
---

# AST-Aware Read Tool

## Overview

Add three new agent-facing read tools (`read_ast_outline`, `read_symbol`, `read_ast_delta`) that wrap existing `tree-sitter-context` CLI capabilities. These tools let agents navigate code by semantic unit instead of reading entire files, and refresh only changed symbols after edits. The existing `read` tool remains available as `read_raw` for fallback.

---

## Problem Frame

pi-mono coding agents waste tokens by reading entire files when they only need specific functions or classes. After editing, agents lack structured incremental state — they cannot distinguish "this function's body changed" from "this function's signature changed" from "this function is unchanged." The result is repeated full-file reads, inflated context windows, and slower task completion.

This plan implements Phase 1 of the AST-aware read layer: a four-layer architecture (`read_ast_outline` → `read_symbol` → `read_ast_delta` → `read_raw`) with two-tier snapshot storage.

(see origin: docs/brainstorms/2026-04-28-ast-aware-read-tool-requirements.md)

---

## Requirements Trace

- **R1.** `read_ast_outline` returns structured file outline with stable IDs, signatures, ranges
- **R2.** `read_symbol` accepts stable ID, returns source + metadata
- **R3.** `read_ast_delta` accepts file + `since_snapshot_id`, returns changed symbols
- **R4.** Retain existing `read` as `read_raw` fallback
- **R5.** Distinguish `body_changed` vs `signature_changed`
- **R6.** Two-tier snapshot storage (session memory + disk cache)
- **R7.** Delta computation by CLI, not agent
- **R8–R10.** Unified schema, stable IDs, explicit baseline
- **R11–R13.** CLI integration details

**Origin actors:** A1 (pi-mono coding agent), A2 (agent operator), A3 (tree-sitter-context CLI), A4 (extension tool bridge)
**Origin flows:** F1 (first encounter with large file), F2 (post-edit incremental refresh), F3 (fallback to raw text)
**Origin acceptance examples:** AE1 (covers R1, R2), AE2 (covers R3, R5), AE3 (covers R4)

---

## Scope Boundaries

- **Deferred for later (Phase 2+):**
  - Callers/references analysis
  - Cross-file dependency graph
  - Test file discovery
  - Multi-session snapshot persistence
  - Tree-sitter-context internal refactoring into outline/symbol/delta commands

- **Outside this product's identity:**
  - Replacing `read_raw` entirely
  - Mandatory AST-aware reads
  - Full IDE symbol index or LSP server

- **Deferred to Follow-Up Work:**
  - `get_semantic_compact` integration with read layer (remains a separate context-management tool per origin decision #4)
  - Cross-file symbol resolution in `read_symbol`

---

## Context & Research

### Relevant Code and Patterns

- `crates/context/src/chunk.rs` — `chunks_for_tree()` generates semantic chunks with stable IDs
- `crates/context/src/invalidation.rs` — `invalidate_snapshot()` classifies chunk changes as Affected/Added/Removed/Unchanged
- `crates/context/src/bundle.rs` — `bundle_chunks()` for token-budgeted context extraction
- `crates/context/src/schema.rs` — Core schema: `ChunkRecord`, `InvalidationOutput`, `InvalidationRecord`
- `crates/context/src/sexpr.rs` — Canonical S-expression serialization for CLI output
- `crates/cli/src/bin/tree-sitter-context.rs` — CLI entry point with subcommand dispatch
- `crates/cli/src/context_invalidate.rs` — `invalidate` CLI implementation
- `pi-mono/packages/coding-agent/src/core/tree-sitter-context/bridge.ts` — Bridge pattern: spawn CLI, parse S-expr, return typed results
- `pi-mono/packages/coding-agent/src/core/tree-sitter-context/tool.ts` — Tool registration via `defineTool`

### Institutional Learnings

- The R0–R2 plans established the `tree-sitter-context` CLI as a dedicated binary with S-expression canonical output
- The bridge pattern writes temp files for old content, spawns CLI, parses S-expr, cleans up temps
- Existing tools (`get_context_bundle`, `get_invalidated_chunks`) follow a consistent error-handling pattern with `success`, `error`, `errorKind` fields
- Snapshot storage already exists for graph operations in `.tree-sitter-context-mcp/`

### External References

- None required — local patterns are well-established and directly applicable

---

## Key Technical Decisions

1. **Signature/body hash heuristic:** For a chunk node, "body" children are identified by field name `body` or kind suffix `_body` or kind `block`/`statement_block`. `signature_hash` = hash of node text excluding body children; `body_hash` = hash of body children text (or same as signature if no body exists). This is language-agnostic and sufficient for Phase 1.

2. **Snapshot cache location:** Disk cache stored at `.tree-sitter-context-mcp/cache/` (discovered via same upward walk as graph store). Format: JSON file per snapshot, filename = `<snapshot_id>.json`, containing file path, timestamp, and chunk records.

3. **Snapshot ID generation:** UUID v4 with `snap_` prefix (e.g., `snap_a1b2c3d4...`). Generated by the `outline` command and returned to the agent.

4. **Delta command strategy:** Enhance existing `invalidate` CLI to accept `--since-snapshot-id` as an alternative to `--old`. When `--since-snapshot-id` is provided, the CLI retrieves the cached snapshot, runs invalidation, and classifies `Affected` chunks into `body_changed` or `signature_changed` based on hash comparison. Existing `--old` behavior remains unchanged for backward compatibility.

5. **Output format for outline:** S-expression (canonical form, consistent with existing CLI commands). The bridge parses S-expr and returns structured JSON to the agent. This preserves byte-stable CLI output while keeping agent consumption friendly.

6. **`read_symbol` scope:** Phase 1 returns only the target symbol's source and metadata. No dependency inclusion. The tool wraps `get_context_bundle` and extracts the single matching cell.

---

## Open Questions

### Resolved During Planning

- **Cache eviction policy?** Simple count-based eviction (max 1000 snapshots) for Phase 1. LRU can be added in Phase 2.
- **How does the bridge read source text for `read_symbol`?** The bridge reads the file directly using the byte range from the bundle result. This avoids modifying the CLI.
- **Should `read_ast_delta` output include unchanged symbols?** Yes — agents need the full picture to decide what to re-read. The output includes all symbols with their change type.

### Deferred to Implementation

- Exact serialization format for outline S-expression (follow existing canonical patterns)
- Whether to add a new `DeltaOutput` type or extend `InvalidationOutput` with change types
- Exact error codes and exit codes for the enhanced `invalidate` command

---

## Output Structure

No new directory hierarchy created. All files fit within existing structures.

---

## Implementation Units

- U1. **Extend ChunkRecord with signature and body hashes**

**Goal:** Add `signature_hash` and `body_hash` fields to `ChunkRecord` so that invalidation can distinguish signature changes from body changes.

**Requirements:** R5, R8, R9

**Dependencies:** None

**Files:**
- Modify: `crates/context/src/schema.rs`
- Modify: `crates/context/src/chunk.rs`
- Test: `crates/context/src/chunk.rs` (existing test module)

**Approach:**
- Add `signature_hash: String` and `body_hash: String` to `ChunkRecord`
- In `Chunker::push_chunk`, after computing `stable_id`, compute:
  - `signature_hash`: hash of node text excluding body children
  - `body_hash`: hash of body children text (or copy of signature_hash if no body)
- Use the same 128-bit FNV-1a hasher from `identity.rs` for consistency
- Define `is_body_child(node)` heuristic: field name == "body" || kind ends with "_body" || kind == "block" || kind == "statement_block"

**Patterns to follow:**
- `identity.rs` `StableDigest` for hashing
- Existing `ChunkRecord` field ordering and serialization attributes

**Test scenarios:**
- Happy path: function with body produces different signature and body hashes
- Edge case: struct without body has signature_hash == body_hash
- Edge case: nested functions (body inside body) — only top-level body excluded from signature
- Integration: round-trip serialization preserves hash fields

**Verification:**
- `ChunkRecord` JSON serialization includes new fields
- `chunks_for_tree` output has distinct signature/body hashes for functions

---

- U2. **Add snapshot disk cache**

**Goal:** Provide persistent storage for AST snapshots so the CLI can retrieve old snapshots by ID during delta computation.

**Requirements:** R6, R7

**Dependencies:** U1

**Files:**
- Create: `crates/context/src/snapshot_cache.rs`
- Modify: `crates/context/src/lib.rs` (export new module)
- Test: `crates/context/src/snapshot_cache.rs` (inline test module)

**Approach:**
- `SnapshotCache` struct with methods:
  - `open(repo_root: &Path) -> Result<Self>` — opens cache at `.tree-sitter-context-mcp/cache/`
  - `save(snapshot_id: &str, path: &Path, chunks: &[ChunkRecord]) -> Result<()>` — serializes to JSON
  - `load(snapshot_id: &str) -> Result<Option<CachedSnapshot>>` — deserializes from JSON
  - `evict_if_needed() -> Result<()>` — removes oldest snapshots if count > 1000
- `CachedSnapshot` struct: `{ file_path: PathBuf, created_at: u64, chunks: Vec<ChunkRecord> }`
- Use `serde_json` for serialization (already a dependency)
- Atomic writes: write to temp file, then rename

**Patterns to follow:**
- `graph/store.rs` for directory creation and atomic write patterns
- Existing schema types with `serde` derives

**Test scenarios:**
- Happy path: save and load a snapshot, chunks match
- Edge case: load nonexistent snapshot returns None
- Edge case: eviction removes oldest files when limit exceeded
- Error path: corrupted JSON file returns error gracefully

**Verification:**
- Cache directory created if missing
- Saved snapshot readable after round-trip
- Eviction maintains max snapshot count

---

- U3. **Add `outline` CLI command**

**Goal:** Implement `tree-sitter-context outline <PATH>` that returns a structured symbol outline and saves the snapshot to disk cache.

**Requirements:** R1, R6, R8, R11

**Dependencies:** U1, U2

**Files:**
- Create: `crates/cli/src/context_outline.rs`
- Modify: `crates/cli/src/bin/tree-sitter-context.rs` (add Outline command)
- Test: `crates/cli/src/tests/context_outline_test.rs`

**Approach:**
- `OutlineArgs` struct with `path`, `format` (default "sexpr"), `quiet` flags
- `run_outline(opts: &OutlineOptions) -> Result<()>`:
  1. Parse file with tree-sitter
  2. Call `chunks_for_tree` to get chunks with hashes
  3. Generate `snapshot_id` (UUID v4 with `snap_` prefix)
  4. Save snapshot to cache via `SnapshotCache`
  5. Serialize outline to S-expression or JSON
- Outline output includes: schema_version, snapshot_id, symbols (stable_id, kind, name, byte_range, signature_hash, body_hash), meta
- Add `Outline` variant to `Commands` enum

**Technical design:**
> Directional guidance: The outline S-expression follows the canonical form pattern:
> ```
> (outline
>   (schema_version "0.2.0")
>   (snapshot_id "snap_...")
>   (symbols
>     (symbol (stable_id "named:...") (kind "function_item") (name "foo")
>             (byte_range 0 100) (signature_hash "...") (body_hash "..."))
>     ...)
>   (meta (source_path "src/lib.rs") (total_symbols 20)))
> ```

**Patterns to follow:**
- `context_bundle.rs` for CLI command structure
- `sexpr.rs` for canonical serialization patterns
- `context_invalidate_test.rs` for test structure

**Test scenarios:**
- Covers AE1. Happy path: outline returns symbols with stable IDs and snapshot_id
- Edge case: empty file returns empty symbols list with diagnostic
- Edge case: file with syntax errors returns Low confidence and diagnostic
- Error path: missing file returns non-zero exit
- Error path: unsupported format returns non-zero exit
- Integration: snapshot saved to cache is loadable

**Verification:**
- `tree-sitter-context outline src/lib.rs --format sexpr` returns valid S-expression
- Output contains snapshot_id and symbol list
- Cache contains retrievable snapshot after command completes

---

- U4. **Enhance invalidation with change classification**

**Goal:** Enable the `invalidate` CLI to retrieve snapshots by ID and classify changes as `body_changed` or `signature_changed`.

**Requirements:** R3, R5, R7, R10, R13

**Dependencies:** U1, U2

**Files:**
- Modify: `crates/context/src/invalidation.rs`
- Modify: `crates/context/src/schema.rs`
- Modify: `crates/cli/src/context_invalidate.rs`
- Modify: `crates/cli/src/bin/tree-sitter-context.rs`
- Modify: `crates/context/src/sexpr.rs` (update invalidation serialization)
- Test: `crates/cli/src/tests/context_invalidate_test.rs`
- Test: `crates/context/src/invalidation.rs` (existing test module)

**Approach:**
1. **Schema changes:**
   - Add `ChangeType` enum: `BodyChanged`, `SignatureChanged`, `BothChanged`, `Added`, `Removed`, `Unchanged`
   - Add `change_type: ChangeType` field to `InvalidationRecord`
   - Add `change_type` to S-expression serialization

2. **Invalidation logic enhancement:**
   - In `invalidate_snapshot`, when `MatchResult::Unchanged` has content differences:
     - Compare `old.signature_hash` vs `new.signature_hash`
     - Compare `old.body_hash` vs `new.body_hash`
     - Set `change_type` accordingly:
       - signature differs, body same → `SignatureChanged`
       - signature same, body differs → `BodyChanged`
       - both differ → `BothChanged`
   - For `Added` → `Added`, `Removed` → `Removed`, truly unchanged → `Unchanged`

3. **CLI enhancement:**
   - Add `--since-snapshot-id <ID>` flag to `InvalidateArgs` (alternative to `--old`)
   - When `--since-snapshot-id` is provided:
     - Load old snapshot from cache
     - Use cached file path and chunks
     - Parse current file
     - Run invalidation with loaded chunks vs fresh chunks
   - When `--old` is provided: existing behavior (unchanged)
   - Output format includes `change_type` field

**Patterns to follow:**
- Existing `InvalidateArgs` and `run_invalidate` structure
- Existing `InvalidationOutput` serialization in `sexpr.rs`

**Test scenarios:**
- Covers AE2. Happy path: body-only change produces `BodyChanged`
- Covers AE2. Happy path: signature change produces `SignatureChanged`
- Happy path: unchanged file produces all `Unchanged`
- Happy path: added/removed functions produce `Added`/`Removed`
- Edge case: both signature and body changed produces `BothChanged`
- Error path: invalid snapshot_id returns error
- Error path: missing `--old` and missing `--since-snapshot-id` returns error
- Integration: `--since-snapshot-id` retrieves correct snapshot from cache

**Verification:**
- `tree-sitter-context invalidate src/lib.rs --since-snapshot-id snap_xxx` returns change types
- Existing `--old` behavior unchanged (backward compatible)
- `change_type` appears in S-expression output

---

- U5. **Update bridge with new read tools and session memory**

**Goal:** Add TypeScript bridge functions for `read_ast_outline`, `read_symbol`, `read_ast_delta` with per-session snapshot memory.

**Requirements:** R1, R2, R3, R6, R8, R10

**Dependencies:** U3, U4

**Files:**
- Modify: `pi-mono/packages/coding-agent/src/core/tree-sitter-context/bridge.ts`
- Test: `pi-mono/packages/coding-agent/test/tree-sitter-context/bridge.test.ts` (create if missing)

**Approach:**
1. **Session memory:**
   - Add `sessionMemory: Map<string, string>` at module level (file_path → snapshot_id)
   - Functions: `setSnapshotId(path, id)`, `getSnapshotId(path)`, `clearSessionMemory()`

2. **`readAstOutline`:**
   - Input: `{ path: string }`
   - Spawns: `tree-sitter-context outline <path> --format sexpr`
   - Parses S-expression into structured outline
   - Stores returned `snapshot_id` in session memory
   - Returns: `{ success, outline?, snapshot_id?, error? }`

3. **`readSymbol`:**
   - Input: `{ path: string, stable_id: string }`
   - Spawns: `tree-sitter-context bundle <path> --stable-id <id> ...`
   - Parses bundle result
   - If successful, reads file source for the cell's byte_range
   - Returns: `{ success, source?, metadata?, error? }`
   - Metadata includes: stable_id, kind, name, byte_range, signature_hash, body_hash

4. **`readAstDelta`:**
   - Input: `{ path: string, since_snapshot_id?: string }`
   - If `since_snapshot_id` omitted, uses session memory lookup
   - Spawns: `tree-sitter-context invalidate <path> --since-snapshot-id <id> --format sexpr`
   - Parses invalidation output
   - Returns: `{ success, changes?, snapshot_id?, error? }`
   - `changes` array includes: stable_id, change_type, kind, name
   - Updates session memory with new snapshot_id if returned

**Patterns to follow:**
- Existing `getContextBundle`, `getInvalidatedChunks` bridge functions
- Existing input validation (path traversal rejection, required fields)
- Existing error handling pattern (`success`, `error`, `errorKind`)
- Existing S-expression parsing via `parseSExpr`

**Test scenarios:**
- Happy path: `readAstOutline` returns outline and stores snapshot_id
- Happy path: `readSymbol` returns source for valid stable_id
- Happy path: `readAstDelta` returns changes using explicit snapshot_id
- Happy path: `readAstDelta` falls back to session memory snapshot_id
- Edge case: `readSymbol` with invalid stable_id returns not_found error
- Edge case: `readAstDelta` with expired snapshot_id returns error
- Error path: CLI exit code non-zero returns process-error

**Verification:**
- Bridge functions spawn correct CLI arguments
- Session memory persists snapshot_ids across calls
- Error responses follow existing `errorKind` taxonomy

---

- U6. **Register new tools in pi-mono**

**Goal:** Expose `read_ast_outline`, `read_symbol`, `read_ast_delta` as agent-facing extension tools.

**Requirements:** R1, R2, R3, R8

**Dependencies:** U5

**Files:**
- Modify: `pi-mono/packages/coding-agent/src/core/tree-sitter-context/tool.ts`
- Test: `pi-mono/packages/coding-agent/test/tree-sitter-context/tool.test.ts` (create if missing)

**Approach:**
- Define three new tools using `defineTool`:

1. **`read_ast_outline`**:
   - Parameters: `path` (required string)
   - Description: "Get a structured AST outline of a source file with stable IDs for each symbol. Use this before reading specific functions or classes."
   - Execute: calls `readAstOutline`, returns JSON outline

2. **`read_symbol`**:
   - Parameters: `path` (required), `stable_id` (required)
   - Description: "Read the source code of a specific symbol by its stable ID. Use this after `read_ast_outline` to read only the symbols you need."
   - Execute: calls `readSymbol`, returns source + metadata

3. **`read_ast_delta`**:
   - Parameters: `path` (required), `since_snapshot_id` (optional string)
   - Description: "Detect which symbols changed since a previous snapshot. Use this after editing a file to refresh only stale context."
   - Execute: calls `readAstDelta`, returns change list

- Update `registerTreeSitterContextTools` to register all three new tools alongside existing ones
- Unified schema: all three tools use common fields (stable_id, kind, name, range, signature_hash, body_hash, snapshot_id)

**Patterns to follow:**
- Existing `getContextBundleTool`, `getInvalidatedChunksTool` structure
- Existing parameter validation and type definitions using `@mariozechner/pi-ai` `Type.Object`
- Existing execute pattern: call bridge function, handle errors, return content array

**Test scenarios:**
- Test expectation: none — tool registration is declarative scaffolding with no runtime logic of its own. Verification is via integration tests in U5 and manual agent testing.

**Verification:**
- All three tools appear in agent tool registry
- Tool parameters match unified schema
- `registerTreeSitterContextTools` registers 6 tools total (3 existing + 3 new)

---

## System-Wide Impact

- **Interaction graph:** New `read_ast_outline` and `read_ast_delta` tools interact with disk cache (`.tree-sitter-context-mcp/cache/`). Cache writes happen on every outline call; cache reads happen on delta calls.
- **Error propagation:** Cache errors (missing snapshot, corrupted file) propagate as `process-error` or `rust-output-invalid` via the bridge. Agents should fall back to `read_raw` on any AST tool failure.
- **State lifecycle risks:** Session memory is per-session only (no persistence). If an agent restarts, snapshot_ids are lost and must be re-obtained via `read_ast_outline`. Disk cache entries may accumulate up to 1000 snapshots before eviction.
- **API surface parity:** The existing `get_context_bundle`, `get_invalidated_chunks`, `get_semantic_compact` tools remain unchanged and functional.
- **Integration coverage:** The full flow (outline → symbol → edit → delta → symbol) must be tested end-to-end in a manual or integration test scenario.
- **Unchanged invariants:**
  - `tree-sitter-context bundle` command behavior unchanged
  - `tree-sitter-context invalidate --old <path>` behavior unchanged
  - `get_context_bundle` and `get_invalidated_chunks` bridge functions unchanged
  - S-expression canonical form for bundle and compact outputs unchanged

---

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Signature/body heuristic fails for some languages | Add diagnostic when no body child detected; for Phase 1, `signature_hash == body_hash` is acceptable fallback |
| Disk cache corruption | Atomic writes (temp file + rename); load errors return typed error to agent |
| Snapshot cache grows unbounded | Count-based eviction (1000 max) in Phase 1; LRU in Phase 2 |
| Bridge session memory lost on restart | By design — agents re-call `read_ast_outline` to get new snapshot_id |
| Existing tool breakage | All existing CLI flags and behaviors preserved; new flags are additive only |

---

## Documentation / Operational Notes

- Update `docs/plans/tree-sitter-context-cli-v1-contract.md` with `outline` command specification
- Update pi-mono coding agent documentation to mention new read tools
- No rollout concerns — new tools are opt-in; existing `read_raw` remains default

---

## Sources & References

- **Origin document:** [docs/brainstorms/2026-04-28-ast-aware-read-tool-requirements.md](docs/brainstorms/2026-04-28-ast-aware-read-tool-requirements.md)
- Related code: `crates/context/src/chunk.rs`, `crates/context/src/invalidation.rs`, `crates/cli/src/bin/tree-sitter-context.rs`
- Related contracts: `docs/plans/tree-sitter-context-cli-v1-contract.md`
- Bridge code: `pi-mono/packages/coding-agent/src/core/tree-sitter-context/bridge.ts`
