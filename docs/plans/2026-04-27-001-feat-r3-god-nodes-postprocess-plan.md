---
title: R3 PageRank God Nodes Postprocess Plan
type: feat
status: completed
date: 2026-04-27
origin: docs/brainstorms/2026-04-26-r3-god-nodes-requirements.md
---

# R3 PageRank God Nodes Postprocess Plan

## Overview

R3 upgrades the `god_nodes` reserved field in orientation output from a `(god_nodes postprocess_unavailable)` placeholder to a computed PageRank-based navigation signal: `(god_nodes (computation_status computed) ((rank 1) (stable_id "...") (path "...")) ...)`.

This is a single-field thin→rich upgrade. It does not touch R1 graph build/update CLI, does not upgrade `communities` or `architecture_summary`, does not implement agent-facing query primitives, and does not enter pi-mono upstream code paths.

The implementation adds a new `tree-sitter-context graph postprocess` CLI subcommand that reads the current HEAD snapshot, computes a deterministic PageRank variant over cross-file edges, and writes a JSON artifact. `orientation get` then reads this artifact (if present and schema-compatible) and emits the computed god_nodes block in the R2-locked field position.

---

## Problem Frame

R2's orientation handshake emits `top_referenced` — a raw cross-file inbound reference count. This replicates the Aider hub-dominance bug: peripheral utilities (`str_to_cstring`, `format_error`, logging macros) get called by many unimportant modules and dominate the top-N list, while true architectural hubs (`Parser::parse`, `Tree::edit`, `Query::new`) are buried.

R3 solves this by computing PageRank on the cross-file reference graph. PageRank propagates importance recursively: a node is important if it is called by important nodes. This surfaces architectural hubs over peripheral utilities.

(see origin: docs/brainstorms/2026-04-26-r3-god-nodes-requirements.md)

---

## Requirements Trace

- **R3.1** — Add `graph postprocess` CLI subcommand; no new flags beyond `--quiet` and `[REPO_ROOT]`; do not modify existing CLI semantics.
- **R3.2** — Postprocess artifact at `.tree-sitter-context-mcp/postprocess/<snapshot_id>.json` with schema: `snapshot_id`, `schema_version` (compile-time constant `"r3-god-nodes-2026-04-26"`), `computed_at` (non-contract metadata), `god_nodes` array of `{rank, stable_id, path}` sorted by rank ascending.
- **R3.3** — Deterministic PageRank: 30 iterations, 1/N uniform initialization, damping 0.85, uniform edge weights, no randomness, no wallclock early-stop, deterministic iteration order.
- **R3.4** — Orientation get emits computed god_nodes when artifact present+valid; falls back to `(god_nodes postprocess_unavailable)` when missing/corrupt/schema-mismatch. Field position locked: after `entry_points`, before `communities`.
- **R3.5** — Top-N fixed at 20; rank 1..K (K ≤ 20); tie-break by `(path, stable_id)` lexicographic when float difference < 1e-12.
- **R3.6** — `god_nodes` array and `schema_version` are contract (byte-stable); `computed_at` is non-contract.
- **R3.7** — Shared read helper in `crates/context` returning typed enum: `Present(GodNodes)` / `Missing` / `Corrupt(reason)` / `SchemaMismatch(version)` / `SnapshotMismatch`.
- **R3.8** — Graceful fallback: any read error → `postprocess_unavailable` + stderr typed warning, exit code unchanged.
- **R3.9** — Typed errors for `graph postprocess`: `no_graph`, `graph_corrupt`, `snapshot_unreadable`, `postprocess_write_failed`. Write failures must not truncate existing artifacts.
- **R3.10** — `top_referenced` semantics unchanged; R0/R2 contract tests pass unmodified.
- **R3.11** — Document semantic separation: `top_referenced` = transparency signal (raw count), `god_nodes` = navigation signal (weighted propagation).
- **R3.12** — Do not upgrade communities/architecture_summary; do not add tuning CLI; do not auto-vacuum.
- **R3.13** — Document vacuum policy as deferred to R3.1; allow manual operator cleanup.
- **R3.14** — Harness assertions (d)(e)(f): computed god_nodes present; bundle freshness after postprocess; stale fallback after update without re-postprocess.
- **R3.15** — Wire protocol only allows adding new fields or new enum members within `(computation_status ...)`; renaming/deleting/moving existing fields is not allowed.

**Origin actors:** A1 (R3 postprocess builder), A2 (R2 orientation builder upgrade path), A3 (R1 graph store + HEAD), A4 (pi-mono harness consumer), A5 (operator/CI), A6 (R3.1/R4 future implementer)

**Origin flows:** F1 (cold postprocess), F2 (warm orientation get with computed god_nodes), F3 (postprocess unavailable/fallback), F4 (stale postprocess after graph update), F5 (idempotent postprocess/determinism gate), F6 (R0/R1/R2 backward compatibility)

**Origin acceptance examples:** AE1–AE14 (see origin document)

---

## Scope Boundaries

- **Deferred for later (product sequencing):**
  - `communities` upgrade (Louvain/label propagation) — R3.1
  - `architecture_summary` upgrade — R4
  - PageRank edge-type weighting (call > import > ref)
  - `score_percentile` (0-100) integer field
  - Agent-facing query primitive surface (`find-callers`, `safe_edit`, `should_reorient`)
  - pi-mono upstream product integration
  - Daemon decision (only if postprocess breaches performance gate)

- **Outside this product's identity:**
  - Replacing pi-mono `read/write/edit/bash/grep/find/ls`
  - Replacing pi-mono Auto-compact
  - MCP server / N-API / WASM bridge
  - Two-Corrections Rule, exploration overlay, blast-radius invalidation

- **Deferred to Follow-Up Work:**
  - Vacuum policy implementation — deferred to R3.1
  - Contract doc updates for `r0-orientation-compaction-v2-contract.md` (incremental patch)

---

## Context & Research

### Relevant Code and Patterns

- **CLI command dispatch**: `crates/cli/src/bin/tree-sitter-context.rs` — clap derive macros with nested `GraphCommands` enum; add `Postprocess(PostprocessArgs)` variant following `Build`/`Update` pattern.
- **Graph store**: `crates/context/src/graph/store.rs` — atomic writes via temp file + `fsync` + `rename`; `read_head()` returns `GraphSnapshotId`; `read_snapshot()` returns `GraphSnapshot`.
- **Orientation block builder**: `crates/context/src/orientation.rs` — `build_orientation()` sets `god_nodes: OrientationField::PostprocessUnavailable`; `build_top_referenced()` filters cross-file edges (`source.path != target.path`).
- **sexpr emission**: `crates/context/src/sexpr.rs` lines 405-413 — hardcodes `(god_nodes postprocess_unavailable)`; must handle both old and new forms at the locked position.
- **Error handling**: `crates/cli/src/bin/tree-sitter-context.rs` lines 278-298 — typed string prefixes (`no_graph:`, `graph_corrupt:`, `schema_mismatch:`) with specific exit codes.
- **Harness**: `scripts/orientation-handshake-harness.mjs` — Node.js E2E using only built-in modules; spawns cargo-run binary.
- **GraphError enum**: `crates/context/src/graph/snapshot.rs` lines 159-180 — already has `PostprocessUnavailable` variant.

### Institutional Learnings

- **Determinism**: `DefaultHasher` and `HashMap` iteration leak nondeterminism. Fix: explicit digests (XXH3/blake3) + sorted traversal order. R3 must avoid `HashMap` iteration in PageRank computation.
- **Contract testing**: Schema tests must be snapshot assertions, not substring checks. R3 contract test must assert `assert_eq!(run1.god_nodes, run2.god_nodes)` byte-for-byte.
- **Atomic writes**: GraphStore uses temp+fsync+rename. Postprocess artifact must follow identical pattern.
- **Namespace isolation**: `graph *` subcommands must not change `bundle` flags or sexpr output. Additive-only field changes enforced.

### External References

- **petgraph PageRank**: Evaluated and rejected. Serial `page_rank()` is deterministic but uses non-standard O(n·V²·E) algorithm. Hand-rolling standard power-iteration is ~50 lines and avoids the dependency.
- **PageRank standard formulation**: Power iteration with O(n·(|V|+|E|)) per iteration; dangling nodes redistributed uniformly.

---

## Key Technical Decisions

- **Hand-roll PageRank, do not add petgraph dependency.** Rationale: petgraph's serial `page_rank()` uses a non-standard algorithm with O(n·V²·E) complexity. A hand-rolled power iteration is ~50 lines, O(n·(|V|+|E|)), fully deterministic, and avoids a new dependency.
- **Cross-file edges only.** Rationale: Follows `build_top_referenced()` pattern. Intra-file edges would reintroduce local hub-dominance within files, defeating the purpose.
- **Dangling nodes: uniform redistribution to all nodes.** Rationale: Standard PageRank handling. Prevents rank sink at leaf nodes.
- **Empty graph: emit empty `god_nodes` array.** Rationale: Graceful degradation. Orientation get emits `(god_nodes (computation_status computed))` with zero entries. No special error path needed.
- **Atomic writes via temp+rename.** Rationale: Prevents concurrent orientation get from reading half-written JSON. Follows established GraphStore pattern.
- **Add `SnapshotMismatch` as distinct error variant.** Rationale: Prevents serving wrong-graph data if artifact file is manually copied or HEAD changes between read operations.
- **No budget truncation of `god_nodes`.** Rationale: Preserves R2 truncation order (entry_points → top_referenced). `god_nodes` is a core navigation signal and should not be dropped. Add diagnostic if budget exhausted with god_nodes present.

---

## Open Questions

### Resolved During Planning

- **PageRank implementation:** Hand-roll deterministic power iteration; do not use petgraph.
- **Edge scope:** Cross-file edges only (`source.path != target.path`).
- **Dangling node strategy:** Uniform redistribution to all nodes.
- **Empty graph handling:** Emit empty `god_nodes` array, not an error.
- **Artifact write atomicity:** Required via temp+rename, following GraphStore.
- **`SnapshotMismatch`:** Added as distinct variant in read helper enum.
- **Budget truncation:** `god_nodes` is untruncatable; preserve R2 priority order.

### Deferred to Implementation

- **Exact tie-break epsilon:** Requirements specify 1e-12, but implementation may need adjustment based on float behavior across architectures. Contract test will catch drift.
- **Self-loop and duplicate edge handling:** Standard PageRank treats self-loops as normal edges and duplicate edges as multi-edges. Implementation will follow this unless contract tests reveal issues.
- **Performance gate evaluation:** Cold `graph postprocess` latency on tree-sitter repo must be measured. If it breaches R12 gate (~100ms), daemon decision is triggered per R3 deferred work.

---

## Implementation Units

- U1. **Postprocess Types, Schema, and Read/Write Helpers**

**Goal:** Establish the shared layer for postprocess artifact I/O and schema types.

**Requirements:** R3.2, R3.6, R3.7, R3.9

**Dependencies:** None

**Files:**
- Create: `crates/context/src/graph/postprocess.rs`
- Modify: `crates/context/src/graph/mod.rs` (re-export)
- Modify: `crates/context/src/lib.rs` (re-export if needed)
- Test: `crates/context/src/graph/postprocess.rs` (inline `#[cfg(test)]`)

**Approach:**
- Define `PostprocessArtifact` struct with `snapshot_id: String`, `schema_version: String` (constant `"r3-god-nodes-2026-04-26"`), `computed_at: u64`, `god_nodes: Vec<GodNode>`.
- Define `GodNode { rank: usize, stable_id: String, path: String }`.
- Define typed read result enum: `PostprocessStatus { Present(Vec<GodNode>), Missing, Corrupt(String), SchemaMismatch(String), SnapshotMismatch }`.
- Implement `write_postprocess_artifact(store_root, snapshot_id, god_nodes) -> Result<(), GraphError>` using atomic temp+rename.
- Implement `read_postprocess_artifact(store_root, snapshot_id) -> PostprocessStatus` with validation: JSON parse → schema_version match → snapshot_id match → god_nodes array shape validation (continuous ranks 1..K, each element has required keys).

**Patterns to follow:**
- `crates/context/src/graph/store.rs` — atomic write pattern (temp file + fsync + rename)
- `crates/context/src/graph/snapshot.rs` — `GraphError` typed error pattern

**Test scenarios:**
- **Happy path:** Write artifact → read back → `Present` with matching god_nodes array.
- **Edge case:** Empty god_nodes array (N=0 nodes) → write succeeds → read returns `Present` with empty vec.
- **Error path:** Missing artifact → `Missing`.
- **Error path:** Corrupt JSON → `Corrupt` with reason.
- **Error path:** Valid JSON but wrong schema_version → `SchemaMismatch`.
- **Error path:** Valid JSON but snapshot_id mismatch → `SnapshotMismatch`.
- **Error path:** Non-continuous ranks → `Corrupt`.
- **Integration:** Concurrent read during write sees either old or new artifact, never half-written file.

**Verification:**
- Unit tests pass: write→read roundtrip, all error variants, atomicity.

---

- U2. **Deterministic PageRank Core Algorithm**

**Goal:** Implement deterministic PageRank power iteration over cross-file edges.

**Requirements:** R3.3, R3.5, R3.6

**Dependencies:** U1

**Files:**
- Create: `crates/context/src/pagerank.rs`
- Modify: `crates/context/src/lib.rs` (re-export)
- Test: `crates/context/src/pagerank.rs` (inline `#[cfg(test)]`)

**Approach:**
- Build adjacency list from `GraphSnapshot.edges`, filtering to cross-file edges only (`source.path != target.path`).
- Map each unique `(path, stable_id)` to a dense integer index 0..N-1. Sort nodes by `(path, stable_id)` lexicographic to ensure deterministic index assignment.
- Build outbound degree vector and adjacency list in deterministic order.
- Power iteration: `r_new[v] = (1-damping)/N + damping * sum(r_old[w] / out_degree[w])` for all w with edge w→v.
- Dangling nodes (out_degree == 0): redistribute their rank uniformly to all nodes (standard PageRank).
- Run exactly 30 iterations. No convergence check. No randomness.
- After 30 iterations, sort nodes by `(-score, path, stable_id)`. Take top-20. Assign ranks 1..K (K ≤ 20).
- Tie-break: if `|score_i - score_j| < 1e-12`, sort by `(path, stable_id)` lexicographic.
- Return `Vec<GodNode>` sorted by rank ascending.

**Execution note:** Start with a failing contract test: `assert_eq!(run1, run2)` on two runs with identical snapshot. This gates the implementation.

**Technical design:** *(Directional guidance, not implementation specification)*
```
fn compute_god_nodes(snapshot: &GraphSnapshot) -> Vec<GodNode> {
    // 1. Filter cross-file edges
    // 2. Build sorted node index map: BTreeMap<(path, stable_id), usize>
    // 3. Build adjacency list: Vec<Vec<usize>> (inbound edges for each node)
    // 4. Build out_degree: Vec<usize>
    // 5. Initialize ranks: vec![1.0/N; N]
    // 6. For iter in 0..30:
    //    - dangling_sum = sum of ranks for nodes with out_degree == 0
    //    - For each node v in 0..N (deterministic order):
    //      new_rank[v] = (1-damping)/N + damping * dangling_sum/N
    //      + damping * sum_{w->v} (old_rank[w] / out_degree[w])
    //    - ranks = new_ranks
    // 7. Sort by (-score, path, stable_id), take top 20
    // 8. Assign ranks 1..K, return
}
```

**Patterns to follow:**
- `crates/context/src/orientation.rs` `build_top_referenced()` — cross-file edge filtering pattern
- `crates/context/src/graph/snapshot.rs` — `GraphNodeHandle` structure

**Test scenarios:**
- **Happy path:** Star graph (one central node called by all others) → central node rank 1.
- **Happy path:** Chain graph A→B→C→D → B and C have higher rank than A and D.
- **Edge case:** Empty graph (0 nodes) → empty god_nodes array.
- **Edge case:** Single node, no edges → rank 1.
- **Edge case:** Two nodes with equal scores (symmetric graph) → tie-broken by `(path, stable_id)`.
- **Edge case:** Dangling node (no outbound edges) → rank redistributed uniformly.
- **Edge case:** Self-loop → counts as outbound+inbound edge normally.
- **Edge case:** Duplicate edges (same source→target) → treated as multi-edges (each contributes separately to out_degree and inbound sum).
- **Determinism:** Same snapshot run twice → byte-equal god_nodes arrays.
- **Integration:** Run on tree-sitter repo snapshot → top-10 contains `Parser::parse` / `Tree::edit` / `Query::new` (dogfood check, not CI gate).

**Verification:**
- Contract test `assert_eq!(run1, run2)` passes.
- Unit tests for all edge cases pass.

---

- U3. **`graph postprocess` CLI Subcommand**

**Goal:** Wire the new CLI command to read HEAD, compute PageRank, and write artifact.

**Requirements:** R3.1, R3.2, R3.9

**Dependencies:** U1, U2

**Files:**
- Modify: `crates/cli/src/bin/tree-sitter-context.rs` (add `Postprocess` to `GraphCommands`, dispatch arm)
- Modify: `crates/cli/src/context_graph.rs` (add `graph_postprocess()` function)
- Test: `crates/cli/src/tests/context_graph_test.rs`

**Approach:**
- Add `Postprocess(GraphPostprocessArgs)` to `GraphCommands` enum with `--repo-root` and `--quiet` flags (following `GraphBuildArgs` pattern).
- Implement `graph_postprocess(opts) -> Result<GraphPostprocessResult>` in `context_graph.rs`:
  1. Open `GraphStore` at repo root.
  2. Read HEAD → get `snapshot_id`.
  3. Read snapshot via `store.read_snapshot(&head_id)`.
  4. Call `compute_god_nodes(&snapshot)` from U2.
  5. Call `write_postprocess_artifact(store.root(), snapshot_id, god_nodes)` from U1.
  6. Return result with status, snapshot_id, node_count, god_nodes_count.
- Error mapping to typed strings:
  - `MissingSnapshot` → `no_graph:` (exit 2)
  - `CorruptedSnapshot` → `graph_corrupt:` (exit 3)
  - `SchemaMismatch` → `schema_mismatch:` (exit 4)
  - `WriteFailure` / IO errors → `postprocess_write_failed:` (exit 5)
- Write failures must not truncate existing artifacts (atomic rename guarantees this).

**Patterns to follow:**
- `crates/cli/src/context_graph.rs` `build_graph()` — store open + snapshot read pattern
- `crates/cli/src/bin/tree-sitter-context.rs` `run_graph()` — command dispatch pattern

**Test scenarios:**
- **Happy path:** `graph build` → `graph postprocess` → exit 0, artifact exists, schema_version correct.
- **Happy path:** Idempotent: run postprocess twice → god_nodes array byte-equal.
- **Error path:** No graph → exit 2, stderr contains `no_graph`.
- **Error path:** Corrupt HEAD → exit 3, stderr contains `graph_corrupt`.
- **Error path:** Read-only directory → exit 5, stderr contains `postprocess_write_failed`, old artifact preserved.
- **Integration:** Postprocess on tree-sitter repo completes without panic.

**Verification:**
- Integration tests pass: happy path, idempotence, all error codes.

---

- U4. **Orientation Get Integration**

**Goal:** Extend `orientation get` to read postprocess artifact and emit computed god_nodes in the locked field position.

**Requirements:** R3.4, R3.7, R3.8, R3.10, R3.15

**Dependencies:** U1

**Files:**
- Modify: `crates/context/src/orientation.rs` (extend `OrientationField` enum, update `build_orientation()`)
- Modify: `crates/context/src/sexpr.rs` (handle both old and new god_nodes forms)
- Modify: `crates/cli/src/context_graph.rs` (update `orientation_get()` to call read helper)
- Test: `crates/cli/src/tests/orientation_get_test.rs`
- Test: `crates/context/src/sexpr.rs` (inline tests for new sexpr form)

**Approach:**
- Extend `OrientationField` enum:
  ```rust
  pub enum OrientationField {
      PostprocessUnavailable,
      Computed {
          status: String, // "computed"
          nodes: Vec<GodNode>,
      },
  }
  ```
- Update `build_orientation()` to accept optional `Option<Vec<GodNode>>` from postprocess read helper. If `Some(nodes)`, set `god_nodes` to `Computed`; else `PostprocessUnavailable`.
- In `crates/cli/src/context_graph.rs` `orientation_get()`:
  1. After reading snapshot, call `read_postprocess_artifact(store.root(), &head_id.0)`.
  2. Pass result to `build_orientation()`.
  3. On `Missing` / `Corrupt` / `SchemaMismatch` / `SnapshotMismatch`: emit typed warning to stderr but do not change exit code.
- Update `sexpr.rs` `orientation_to_sexpr_inner()`:
  - At locked position (after entry_points, before communities):
  - If `PostprocessUnavailable`: emit `(god_nodes postprocess_unavailable)`
  - If `Computed { status, nodes }`: emit `(god_nodes (computation_status computed) ((rank 1) (stable_id "...") (path "...")) ...)`
- Ensure JSON serialization also handles new enum variant correctly.

**Patterns to follow:**
- `crates/context/src/orientation.rs` — existing `build_orientation()` signature
- `crates/context/src/sexpr.rs` lines 386-413 — locked field position pattern
- `crates/cli/src/context_graph.rs` lines 548-563 — error mapping pattern

**Test scenarios:**
- **Happy path:** Artifact present → sexpr contains `(god_nodes (computation_status computed) ((rank 1) ...))`.
- **Happy path:** Artifact present → JSON serialization parseable and contains computed nodes.
- **Edge case:** Missing artifact → sexpr contains `(god_nodes postprocess_unavailable)`, exit 0.
- **Edge case:** Corrupt artifact → sexpr contains `(god_nodes postprocess_unavailable)`, stderr contains `postprocess_corrupt`, exit 0.
- **Edge case:** Schema mismatch → sexpr contains `(god_nodes postprocess_unavailable)`, stderr contains `postprocess_schema_mismatch`, exit 0.
- **Edge case:** Empty god_nodes array → sexpr contains `(god_nodes (computation_status computed))` with no child lists.
- **Integration:** R2 contract tests pass unmodified (backward compatibility).

**Verification:**
- Orientation get tests pass for all states.
- R2 contract tests (existing) still pass.

---

- U5. **sexpr Fixture and Contract Test Updates**

**Goal:** Audit and update existing fixture assertions to account for the new god_nodes form.

**Requirements:** R3.10, R3.12, R3.15

**Dependencies:** U4

**Files:**
- Modify: `crates/context/src/sexpr.rs` tests (add new fixture for computed god_nodes)
- Modify: `crates/cli/src/tests/` — grep for any tests asserting on god_nodes sexpr
- Modify: `docs/plans/r0-orientation-compaction-v2-contract.md` — add R3 reference examples
- Test: Any modified test files

**Approach:**
- Grep codebase for `postprocess_unavailable` to find all assertion points.
- Classify each:
  - **R2 behavior unchanged:** Tests that run without postprocess artifact (no_graph, fallback). These should still assert `postprocess_unavailable`.
  - **R3 new fixtures:** Tests that run with postprocess artifact. These need new assertions for computed form.
- Add new sexpr fixture test in `sexpr.rs` that constructs an `OrientationBlock` with `OrientationField::Computed` and asserts canonical byte output.
- Update contract docs with R3 reference examples showing both `postprocess_unavailable` (fallback) and `(computation_status computed)` (computed) forms.
- Do not modify R0/R2 plans' existing examples — they remain valid for "no postprocess" state.

**Patterns to follow:**
- `crates/context/src/sexpr.rs` `deterministic_bundle_serialization()` — existing snapshot test pattern
- `docs/plans/sexpr-canonical-form-v1.md` — canonical form specification

**Test scenarios:**
- **Happy path:** sexpr fixture with computed god_nodes → byte-stable output matches expected.
- **Happy path:** sexpr fixture with postprocess_unavailable → byte-stable output unchanged from R2.
- **Integration:** All existing sexpr tests pass without modification.

**Verification:**
- Grep confirms no unclassified `postprocess_unavailable` assertions remain.
- All sexpr tests pass.

---

- U6. **Harness Extension with R3 Assertions**

**Goal:** Add three new end-to-end assertions to the orientation handshake harness.

**Requirements:** R3.14

**Dependencies:** U3, U4

**Files:**
- Modify: `scripts/orientation-handshake-harness.mjs`
- Test: Run harness manually / in CI

**Approach:**
- After existing assertion (c) (bundle stale after update), add:
  - **(d)** `graph postprocess` → `orientation get --format sexpr` → assert stdout contains `(god_nodes (computation_status computed)` substring with at least one `((rank 1) ...)` entry.
  - **(e)** Immediately after (d), `bundle ... --orientation-snapshot-id <current_snapshot_id>` → assert `orientation_freshness == "fresh"` and `graph_snapshot_id == <current_snapshot_id>`.
  - **(f)** Modify fixture again → `graph update` (do NOT run postprocess) → `orientation get --format sexpr` → assert stdout contains `(god_nodes postprocess_unavailable)`.
- Keep harness using only Node built-in modules. No pi-mono dependencies.
- Ensure harness still cleans up temp directory in `finally` block.

**Patterns to follow:**
- Existing harness pattern: `runCli(tempDir, "graph", "build", ...)` then assert on status/stdout/stderr.
- Existing harness pattern: regex match on sexpr for field extraction.

**Test scenarios:**
- **Happy path:** Full harness run (a)-(f) all pass.
- **Integration:** Harness runs in CI without external dependencies.

**Verification:**
- `node scripts/orientation-handshake-harness.mjs` exits 0 with "all assertions passed".

---

## System-Wide Impact

- **Interaction graph:** `orientation get` now reads from `postprocess/` subdirectory in addition to graph store. No callbacks or observers affected.
- **Error propagation:** Postprocess read failures are silently swallowed by orientation get (graceful fallback). `graph postprocess` errors propagate to CLI with typed exit codes.
- **State lifecycle risks:** Old postprocess artifacts remain after `graph update`. They are naturally stale (bound to old snapshot_id) and ignored. No cleanup in R3.
- **API surface parity:** `bundle` command does not read postprocess artifacts. `orientation_freshness` and `graph_snapshot_id` semantics unchanged.
- **Integration coverage:** E2E harness covers the full build→postprocess→orientation→bundle→update cycle.
- **Unchanged invariants:**
  - `top_referenced` sorting, semantics, and wire form unchanged.
  - `communities` and `architecture_summary` remain `postprocess_unavailable`.
  - R0 v1 / R2 v1 field set, enum values, CLI parameters unchanged.
  - `orientation_freshness` three-state enum (`fresh`/`stale`/`unknown`) unchanged.

---

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| PageRank float non-determinism across architectures | Fixed iteration order, no HashMap iteration, no concurrency. Contract test catches drift. |
| petgraph hand-roll introduces bugs | Standard power-iteration formula, well-understood. Comprehensive unit tests for edge cases. |
| sexpr parser incompatibility with nested god_nodes | R3.15 locks form; pi-mono parser must be updated when integrating. R3 plan documents both forms. |
| Old postprocess artifacts accumulate | Documented as deferred vacuum policy. Single artifact < 5KB. Operator can manual `rm`. |
| Concurrent graph update + postprocess race | Postprocess reads snapshot into memory before computing; atomic write prevents corrupt reads. Race on HEAD read vs update is acceptable (artifact bound to snapshot_id, not HEAD). |
| Budget truncation drops entry_points before god_nodes | Intentional: god_nodes is core navigation signal. Diagnostic emitted if budget exhausted. |

---

## Documentation / Operational Notes

- Update `docs/plans/r0-orientation-compaction-v2-contract.md` with R3 reference examples showing computed god_nodes form.
- Add operator note: "After `graph update`, re-run `graph postprocess` to refresh god_nodes. Old artifacts are not auto-cleaned."
- Performance: Cold `graph postprocess` on tree-sitter repo should be measured. If >100ms, document as trigger for daemon evaluation in R3.1.

---

## Sources & References

- **Origin document:** [docs/brainstorms/2026-04-26-r3-god-nodes-requirements.md](docs/brainstorms/2026-04-26-r3-god-nodes-requirements.md)
- **Related code:**
  - `crates/cli/src/bin/tree-sitter-context.rs` — CLI dispatch
  - `crates/cli/src/context_graph.rs` — graph CLI impl
  - `crates/context/src/graph/store.rs` — atomic writes
  - `crates/context/src/graph/snapshot.rs` — snapshot types, GraphError
  - `crates/context/src/orientation.rs` — orientation block builder
  - `crates/context/src/sexpr.rs` — sexpr serializer
  - `scripts/orientation-handshake-harness.mjs` — E2E harness
- **Related plans:**
  - `docs/plans/2026-04-26-001-feat-r0-context-firewall-plan.md`
  - `docs/plans/2026-04-26-002-feat-r1-repo-map-plan.md`
  - `docs/plans/2026-04-26-003-feat-r2-orientation-handshake-plan.md`
  - `docs/plans/tree-sitter-context-cli-v1-contract.md`
  - `docs/plans/r0-orientation-compaction-v2-contract.md`
  - `docs/plans/sexpr-canonical-form-v1.md`
- **External reference:** Aider hub-dominance bug (GH#2405) — warrant for PageRank over raw fan-in
