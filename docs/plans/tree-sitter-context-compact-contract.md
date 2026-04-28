---
title: "tree-sitter-context compact Contract"
type: contract
status: active
date: 2026-04-28
---

# tree-sitter-context compact Contract

## Overview

This document specifies the wire format, CLI interface, and output schema for the `tree-sitter-context compact` command, which enables agents to compress session context by keeping full content for changed chunks and extracting signatures for unchanged named chunks.

## CLI Interface

### Command

```bash
tree-sitter-context compact <paths...> --old <old-dir> [--format sexpr|json] [--budget <tokens>]
```

### Arguments

| Argument | Required | Description |
|----------|----------|-------------|
| `<paths...>` | Yes | Paths to new (current) files |
| `--old <old-dir>` | Yes | Path to directory containing old file snapshots |
| `--format <format>` | No | Output format: `sexpr` (default) or `json` |
| `--budget <tokens>` | No | Optional token budget for compacted output |
| `--quiet` | No | Suppress output |

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | File not found |
| 3 | No language grammar |
| 4 | Parse error |
| 5 | Invalid format |
| 6 | Budget exceeded |

### Error Prefixes

All errors written to stderr use typed prefixes:

- `file_not_found:` - File does not exist or is unreadable
- `no_language:` - No grammar available for the file type
- `parse_error:` - Failed to parse one of the files
- `invalid_format:` - Unsupported output format
- `budget_exceeded:` - Compacted output exceeds budget even after omitting chunks
- `compaction_error:` - General compaction failure
- `error:` - Generic error (fallback)

## S-expression Output Format

### Schema

```lisp
(compaction
  (schema_version "<version>")
  (files
    (file
      (path "<path>")
      (preserved
        ((stable_id "<id>")
         (kind "<kind>")
         (name "<name>")           ; optional
         (byte_range <start> <end>)
         (estimated_tokens <n>)
         (confidence "exact|high|medium|low"))
        ...)
      (signatures_only
        ((stable_id "<id>")
         (kind "<kind>")
         (name "<name>")           ; optional
         (signature "<signature-text>")
         (estimated_tokens <n>)
         (confidence "exact|high|medium|low"))
        ...)
      (omitted
        ((stable_id "<id>")
         (kind "<kind>")
         (name "<name>")           ; optional
         (reason "<reason>")
         (estimated_tokens <n>))
        ...)
      (original_tokens <n>)
      (compacted_tokens <n>))
    ...)
  (omitted
    ((stable_id "<id>")
     (kind "<kind>")
     (name "<name>")               ; optional
     (reason "<reason>")
     (estimated_tokens <n>))
    ...)
  (original_tokens <n>)
  (compacted_tokens <n>)
  (meta
    (schema_version "<version>")
    (source_path "<path>")         ; optional
    (total_chunks <n>)
    (total_estimated_tokens <n>)))
```

### Sections

#### `files`

One `file` block per input path. Each file contains:

- `preserved`: Full chunk records for affected, added, removed, or anonymous unchanged chunks
- `signatures_only`: Signature-only records for unchanged named chunks
- `omitted`: Chunks omitted due to budget constraints (file-level)
- `original_tokens`: Sum of estimated tokens for all chunks in this file
- `compacted_tokens`: Sum of estimated tokens for preserved + signatures_only chunks

#### `omitted` (top-level)

Chunks omitted from the overall output due to global budget constraints.

#### `original_tokens` / `compacted_tokens`

- `original_tokens`: Total tokens before compaction (sum of all chunks)
- `compacted_tokens`: Total tokens after compaction (preserved + signatures_only)

### Sorting

Within each list (`preserved`, `signatures_only`, `omitted`), records are sorted by `stable_id` in lexicographic order.

### String Escaping

String values use the same escaping rules as the R0-R3 canonical S-expression format:
- `"` → `\"`
- `\` → `\\`
- `\n` → `\n`
- `\t` → `\t`
- Control characters → `\u{fffd}`

## JSON Output Format

When `--format json` is used, the output is a JSON object matching the `CompactOutput` schema:

```json
{
  "files": [
    {
      "path": "src/lib.rs",
      "preserved": [
        {
          "id": { "path": "...", "kind": "...", "name": "...", "anchor_byte": 0 },
          "stable_id": "named:abc",
          "kind": "function_item",
          "name": "foo",
          "byte_range": { "start": 0, "end": 23 },
          "estimated_tokens": 6,
          "confidence": "exact"
        }
      ],
      "signatures_only": [
        {
          "status": "signature_only",
          "id": { ... },
          "stable_id": "named:def",
          "kind": "function_item",
          "name": "bar",
          "byte_range": { "start": 25, "end": 50 },
          "estimated_tokens": 4,
          "confidence": "exact",
          "signature": "fn bar(x: i32) -> String"
        }
      ],
      "omitted": [],
      "original_tokens": 10,
      "compacted_tokens": 10
    }
  ],
  "original_tokens": 10,
  "compacted_tokens": 10,
  "omitted": [],
  "diagnostics": [],
  "meta": {
    "schema_version": "0.1.0",
    "total_chunks": 2,
    "total_estimated_tokens": 10
  }
}
```

## Examples

### Body-only change

```lisp
(compaction
  (schema_version "0.1.0")
  (files
    (file
      (path "src/lib.rs")
      (preserved
        ((stable_id "named:foo")
         (kind "function_item")
         (name "foo")
         (byte_range 0 30)
         (estimated_tokens 6)
         (confidence "exact")))
      (signatures_only
        ((stable_id "named:bar")
         (kind "function_item")
         (name "bar")
         (signature "fn bar(x: i32) -> String")
         (estimated_tokens 4)
         (confidence "exact")))
      (omitted)
      (original_tokens 10)
      (compacted_tokens 10)))
  (omitted)
  (original_tokens 10)
  (compacted_tokens 10)
  (meta
    (schema_version "0.1.0")
    (total_chunks 2)
    (total_estimated_tokens 10)))
```

### Budget enforcement

```lisp
(compaction
  (schema_version "0.1.0")
  (files
    (file
      (path "src/lib.rs")
      (preserved
        ((stable_id "named:foo")
         (kind "function_item")
         (name "foo")
         (byte_range 0 30)
         (estimated_tokens 6)
         (confidence "exact")))
      (signatures_only)
      (omitted
        ((stable_id "named:bar")
         (kind "function_item")
         (name "bar")
         (reason "budget")
         (estimated_tokens 4)))
      (original_tokens 10)
      (compacted_tokens 6)))
  (omitted)
  (original_tokens 10)
  (compacted_tokens 6)
  (meta
    (schema_version "0.1.0")
    (total_chunks 2)
    (total_estimated_tokens 10)))
```

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2026-04-28 | Initial compact command contract |
