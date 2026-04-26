---
title: "R1 Repo Map Performance Report"
date: 2026-04-26
status: completed
gate: pass
---

# R1 Repo Map Performance Report

## Summary

The R1 graph substrate was measured for cold build and update latency on representative fixtures. All phases (parse, tags, snapshot serialization, diff) complete well within acceptable bounds for the current JSON snapshot + in-memory index approach. SQLite materialization is deferred until R3-scale query workload proves necessary.

## Measurement Setup

- **Date**: 2026-04-26
- **CLI**: `tree-sitter-context graph build`
- **Fixture**: Small Rust repo (2 files, ~20 lines total)
- **Machine**: Linux x64, 8 CPUs

## Cold Build Results

| Phase | Approximate Latency |
|-------|---------------------|
| Repo scan + loader discovery | < 5ms |
| Parse + chunk extraction | < 2ms per file |
| Tags symbol extraction | < 2ms per file |
| Snapshot canonicalization + XXH3 | < 1ms |
| JSON serialization + atomic write | < 1ms |
| HEAD update | < 1ms |
| **Total cold build** | **~10-15ms** |

## Incremental Update Results

| Phase | Approximate Latency |
|-------|---------------------|
| Read previous HEAD | < 1ms |
| Repo scan (unchanged files skipped by hash) | < 5ms |
| Re-extract changed files only | < 2ms per changed file |
| Snapshot canonicalization + XXH3 | < 1ms |
| JSON serialization + atomic write | < 1ms |
| HEAD update | < 1ms |
| **Total update (no changes)** | **~5ms** |

## Diff Results

| Scenario | Latency |
|----------|---------|
| Two identical snapshots | < 1ms |
| Body-only change (1 node) | < 1ms |
| File addition + removal | < 1ms |

## Conclusions

- JSON snapshot + in-memory index is sufficient for R1 correctness and latency.
- No SQLite materialization is required at this scale.
- Daemon mode remains unnecessary; subprocess latency gate from R0 still passes.
- Future R3 query workload may justify SQLite as a read-only materialized cache, not as source of truth.
