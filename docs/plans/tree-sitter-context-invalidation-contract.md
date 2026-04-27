---
title: "tree-sitter-context invalidate Contract"
type: contract
status: active
date: 2026-04-28
---

# tree-sitter-context invalidate Contract

## Overview

This document specifies the wire format, CLI interface, and output schema for the `tree-sitter-context invalidate` command, which enables agents to detect which semantic chunks changed after file edits.

## CLI Interface

### Command

```bash
tree-sitter-context invalidate <new-path> --old <old-path> [--format sexpr|json]
```

### Arguments

| Argument | Required | Description |
|----------|----------|-------------|
| `<new-path>` | Yes | Path to the new (current) file |
| `--old <old-path>` | Yes | Path to the old (previous) file |
| `--format <format>` | No | Output format: `sexpr` (default) or `json` |
| `--quiet` | No | Suppress output (for validation only) |

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | File not found |
| 3 | No language grammar |
| 4 | Parse error |
| 5 | Invalid format |

### Error Prefixes

All errors written to stderr use typed prefixes:

- `file_not_found:` - File does not exist or is unreadable
- `no_language:` - No grammar available for the file type
- `parse_error:` - Failed to parse one of the files
- `invalid_format:` - Unsupported output format
- `error:` - Generic error (fallback)

## S-expression Output Format

### Schema

```lisp
(invalidation
  (schema_version "<version>")
  (affected
    ((stable_id "<id>")
     (kind "<kind>")
     (name "<name>")                ; optional
     (path "<path>")
     (byte_range <start> <end>)
     (estimated_tokens <n>)
     (confidence "exact|high|medium|low")
     (reason "content_changed|changed_range_overlap|...")
     (match_strategy "stable_id|...")
     (changed_ranges                ; optional, if applicable
       ((start <byte>) (end <byte>))
       ...)))
  (added (...))                    ; same structure as affected
  (removed (...))                  ; same structure as affected
  (unchanged (...))                ; same structure as affected
  (changed_ranges
    ((start <byte>) (end <byte>))
    ...)
  (meta
    (schema_version "<version>")
    (source_path "<path>")         ; optional
    (total_chunks <n>)
    (total_estimated_tokens <n>)))
```

### Field Descriptions

#### Chunk Fields

- `stable_id` - Stable identifier (e.g., `named:abc123...` or `unnamed:def456...`)
- `kind` - Syntax kind (e.g., `function_item`, `struct_item`)
- `name` - Optional name of the chunk (for named chunks)
- `path` - Relative path to the source file
- `byte_range` - Tuple of (start_byte, end_byte) in the new file
- `estimated_tokens` - Estimated token count for the chunk
- `confidence` - Confidence level: `exact`, `high`, `medium`, `low`
- `reason` - Why the chunk was classified this way:
  - `content_changed` - Content differs despite stable identity match
  - `changed_range_overlap` - Chunk overlaps a changed range
  - `added_chunk` - New chunk not in old file
  - `removed_chunk` - Chunk from old file no longer present
  - `no_change_detected` - Chunk unchanged
  - `degraded_matching` - Matched with degraded confidence
- `match_strategy` - How the chunk was matched:
  - `stable_id` - Matched by stable identity
  - `content_comparison` - Matched by content hash
  - `textual_range_overlap` - Matched by overlapping byte range
  - `edit_range_overlap` - Matched by edit range (edit-stream mode)
  - `unmatched` - No match found (for added/removed)
- `changed_ranges` - List of changed byte ranges overlapping this chunk

### Determinism Guarantees

1. **Byte stability** - Same inputs produce identical byte output
2. **Ordering** - Chunks within each bucket sorted by stable_id
3. **No timestamps** - Output does not contain timestamps
4. **No absolute paths** - Paths are relative to repo root

## JSON Output Format

When `--format json` is specified, the output is the JSON serialization of the `InvalidationOutput` schema:

```json
{
  "records": [...],
  "affected": [...],
  "added": [...],
  "removed": [...],
  "unchanged": [...],
  "changed_ranges": [...],
  "diagnostics": [...],
  "meta": {
    "schema_version": "0.1.0",
    "source_path": "src/lib.rs",
    "total_chunks": 42,
    "total_estimated_tokens": 1500
  }
}
```

## Examples

### Body-only Change

A function body change produces one affected chunk:

```lisp
(invalidation
  (schema_version "0.1.0")
  (affected
    ((stable_id "named:foo_abc123")
     (kind "function_item")
     (name "foo")
     (path "src/lib.rs")
     (byte_range 0 45)
     (estimated_tokens 12)
     (confidence "exact")
     (reason "content_changed")
     (match_strategy "stable_id")))
  (added)
  (removed)
  (unchanged)
  (changed_ranges ((start 23) (end 42)))
  (meta
    (schema_version "0.1.0")
    (source_path "src/lib.rs")
    (total_chunks 1)
    (total_estimated_tokens 12)))
```

### Signature Change

A function signature change produces removed + added chunks (stable_id changes):

```lisp
(invalidation
  (schema_version "0.1.0")
  (affected)
  (added
    ((stable_id "named:foo_new456")
     (kind "function_item")
     (name "foo")
     (path "src/lib.rs")
     (byte_range 0 50)
     (estimated_tokens 14)
     (confidence "exact")
     (reason "added_chunk")
     (match_strategy "unmatched")))
  (removed
    ((stable_id "named:foo_old123")
     (kind "function_item")
     (name "foo")
     (path "src/lib.rs")
     (byte_range 0 45)
     (estimated_tokens 12)
     (confidence "exact")
     (reason "removed_chunk")
     (match_strategy "unmatched")))
  (unchanged)
  (changed_ranges ((start 0) (end 50)))
  (meta
    (schema_version "0.1.0")
    (source_path "src/lib.rs")
    (total_chunks 1)
    (total_estimated_tokens 14)))
```

### Whitespace-only Change

Reformatting produces all chunks in unchanged, no affected:

```lisp
(invalidation
  (schema_version "0.1.0")
  (affected)
  (added)
  (removed)
  (unchanged
    ((stable_id "named:foo_abc123")
     (kind "function_item")
     (name "foo")
     (path "src/lib.rs")
     (byte_range 0 45)
     (estimated_tokens 12)
     (confidence "exact")
     (reason "no_change_detected")
     (match_strategy "stable_id")))
  (changed_ranges)
  (meta
    (schema_version "0.1.0")
    (source_path "src/lib.rs")
    (total_chunks 1)
    (total_estimated_tokens 12)))
```

## pi-mono Extension Tool

### Tool: get_invalidated_chunks

```typescript
interface GetInvalidatedChunksInput {
  path: string;                    // Path to source file
  old_content_base64: string;      // Base64-encoded old content
  previous_stable_ids?: string[];  // Optional filter
}

interface GetInvalidatedChunksResult {
  success: boolean;
  affected?: ChunkInfo[];
  added?: ChunkInfo[];
  removed?: ChunkInfo[];
  unchanged?: ChunkInfo[];
  error?: string;
  errorKind?: "invalid-input" | "rust-output-invalid" | "process-error" | "timeout";
}

interface ChunkInfo {
  stable_id: string;
  kind: string;
  name?: string;
  path: string;
  byte_range: [number, number];
  estimated_tokens: number;
  confidence: string;
  reason: string;
  match_strategy: string;
}
```

### Usage Flow

1. Agent reads chunks via `get_context_bundle` before edit
2. Agent edits file via standard tools
3. Agent calls `get_invalidated_chunks` with:
   - `path`: path to edited file
   - `old_content_base64`: base64-encoded content before edit
   - `previous_stable_ids`: array of stable_ids from step 1 (optional filter)
4. Tool returns classification (affected/added/removed/unchanged)
5. Agent selectively re-reads affected chunks via `get_context_bundle`

### Error Handling

- Invalid base64: returns `invalid-input` before spawning CLI
- Missing file: returns `process-error` with exit code 2
- No language grammar: returns `process-error` with exit code 3
- Parse failure: returns `process-error` with exit code 4
- Malformed CLI output: returns `rust-output-invalid`

## Version History

- **v0.1.0** (2026-04-28) - Initial implementation
  - Snapshot-based invalidation
  - S-expression and JSON output
  - pi-mono extension tool with base64 content encoding
  - `previous_stable_ids` filtering

## References

- `crates/context/src/invalidation.rs` - Core invalidation logic
- `crates/context/src/sexpr.rs` - S-expression serialization
- `crates/context/src/schema.rs` - `InvalidationOutput` schema
- `docs/plans/tree-sitter-context-cli-v1-contract.md` - Parent CLI contract
- `pi-mono/packages/coding-agent/src/core/tree-sitter-context/` - Extension tool implementation