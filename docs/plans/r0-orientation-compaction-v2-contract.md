---
title: "v2 Orientation and Graph-Aware Compaction Contract"
type: contract
status: active
date: 2026-04-26
origin: docs/plans/2026-04-26-001-feat-r0-context-firewall-plan.md
---

# v2 Orientation and Graph-Aware Compaction Contract

## Scope

This document locks the schema and lifecycle contracts for v2 orientation and graph-aware compaction without implementing runtime behavior in v1. It governs:

- Orientation block schema and freshness semantics
- Graph-aware compaction contract and failure behavior
- Future R1/R2 implementation obligations

## Orientation Block Schema

### Structure

The orientation block is a structured context preamble injected into the system prompt:

```text
(orientation
  (graph_snapshot_id "<snapshot-id>")
  ( freshness
    (timestamp "<iso8601>")
    (stale_after_ms 3600000))
  (prefix_freezing
    (frozen_until_line 42)
    (frozen_reason "user_declared_boundary")))
```

### Fields

- `graph_snapshot_id`: Deterministic snapshot identifier from the graph builder
- `freshness.timestamp`: ISO 8601 timestamp of last graph update
- `freshness.stale_after_ms`: Milliseconds after which orientation is considered stale
- `prefix_freezing.frozen_until_line`: Line number prefix that must not be modified
- `prefix_freezing.frozen_reason`: Why the prefix is frozen

### Lifecycle

1. **Session Start**: Orientation is computed from the current graph snapshot
2. **Freshness Check**: Before each turn, verify if `stale_after_ms` has elapsed
3. **Reorientation Triggers**:
   - Severe: User explicitly declares a boundary
   - Severe: Graph snapshot ID changes unexpectedly
   - Ordinary: `stale_after_ms` elapsed since last update
   - Ordinary: Significant graph mutations detected

### Severe vs Ordinary Staleness

- **Severe**: Invalidates the prefix freezing contract. The LLM must not modify frozen lines.
- **Ordinary**: Recommends refresh but does not invalidate prefix freezing.

## Graph-Aware Compaction Contract

### Interface

```typescript
compact(messages: AgentMessage[], options: {
  strategy: "graph-aware" | "llm-summary",
  fallback: "error" | "llm-summary"
}): Promise<CompactionResult>
```

### Behavior

1. **Primary Strategy**: `graph-aware`
   - Uses the code graph to identify redundant context
   - Preserves semantic relationships across messages
   - Returns structured `CompactionResult.details`

2. **Failure Behavior**:
   - If graph is unavailable: MUST NOT silently fall back to LLM summary
   - If `fallback === "error"`: Return typed error with `graph-unavailable` reason
   - If `fallback === "llm-summary"`: Explicitly log the降级 and proceed

3. **Prohibition**: Graph failure must never silently call LLM summarization without explicit operator consent.

### CompactionResult.details Schema

```text
(compaction_result
  (strategy "graph-aware")
  (original_tokens 15000)
  (compacted_tokens 8000)
  (preserved_nodes
    (node (stable_id "named:abc") (reason "entry_point"))
    (node (stable_id "named:def") (reason "recently_modified")))
  (omitted_nodes
    (node (stable_id "named:ghi") (reason "unreachable")))
  (graph_errors
    (error (kind "missing_symbol") (symbol "Foo") (reason "not_in_graph"))))
```

### Typed Graph Errors

- `missing_symbol`: Referenced symbol not found in graph
- `stale_reference`: Graph reference points to outdated node
- `ambiguous_resolution`: Multiple graph nodes match single reference
- `graph_corrupted`: Graph structure is internally inconsistent

## R1 Obligations

The following are deferred to R1 graph implementation:

1. **Deterministic snapshot ID**: `graph_snapshot_id` must be a content-addressable hash
2. **Snapshot diff API**: Must expose added/removed/changed nodes between snapshots
3. **HEAD tracking**: `.tree-sitter-context-mcp/HEAD` stores the active snapshot reference
4. **Compaction queries**: Graph must support "entry points", "recently modified", "unreachable" queries
5. **Typed graph errors**: Error schema above must be implemented

## v1 → R2 Transition

After R2 implementation:
- `graph_snapshot_id` is the current HEAD snapshot ID (`"<XXH3 hex>"`) or `"no_graph"` when no graph exists.
- `orientation_freshness` is `fresh`, `stale`, or `unknown` (enum locked in R0, never widened).
- `compact()` still uses LLM summarization (R3 scope).
- The schema above is now enforced in code.

## Future Acceptance Examples

- **AE11**: Orientation freshness is checked and reported
- **AE12**: Severe reorientation triggers are detected and logged
- **AE13**: CompactionResult.details contains typed graph errors on failure

## Versioning

This contract is versioned independently of the v1 S-expression contract.
Future revisions must maintain backward compatibility for:
- Orientation block schema (additive only)
- CompactionResult.details shape (additive only)
- Error type definitions (additive only)
