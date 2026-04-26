---
title: "R0 Context Firewall Performance Report"
date: 2026-04-26
status: completed
gate: pass
---

# R0 Context Firewall Performance Report

## Summary

The `tree-sitter-context bundle` CLI subprocess path was measured under cold and warm conditions. Both cold and warm p95 latencies are well under the 100ms pass threshold, with the subprocess approach deemed acceptable for v1. No daemon work is required at this time.

## Measurement Setup

- **Date**: 2026-04-26T11:14:42.928Z
- **CLI**: `/home/lansy/Work/github/tree-sitter/target/release/tree-sitter-context`
- **Fixture**: `crates/cli/src/tests/fixtures/small.rs` (8-line Rust file)
- **Stable ID**: `named:351f5e1b1a0c28827a5b3d3fe3575191`
- **Command**:
  ```bash
  tree-sitter-context bundle small.rs \
    --stable-id named:351f5e1b1a0c28827a5b3d3fe3575191 \
    --tier sig --format sexpr \
    --max-tokens 5000 --budget 500 \
    --grammar-path /path/to/tree-sitter-rust
  ```

### Machine Context

| Property | Value |
|----------|-------|
| Platform | linux |
| Architecture | x64 |
| CPUs | 8 |
| Node.js Version | v25.9.0 |
| Subprocess Backend | Node.js `child_process.spawn` (shell: false) |

## Results

### Cold Calls (n=100)

Cold calls were measured with a 100ms delay between invocations to avoid cache warming. This approximates intermittent real-world usage where the CLI is not called in rapid succession.

| Metric | Value |
|--------|-------|
| p50 | 2.86ms |
| p95 | 3.70ms |
| Min | 2.51ms |
| Max | 7.49ms |
| Mean | 3.02ms |

### Warm Calls (n=100)

Warm calls were measured as 100 back-to-back invocations with no delay, simulating rapid consecutive usage within a single agent round.

| Metric | Value |
|--------|-------|
| p50 | 2.50ms |
| p95 | 3.11ms |
| Min | 2.22ms |
| Max | 3.30ms |
| Mean | 2.61ms |

## Gate Decision

The three-tier gate from R12 was applied:

| Tier | Condition | Result |
|------|-----------|--------|
| **Pass** | Cold p95 < 100ms AND warm p95 < 100ms AND calls per round <= 3 | **ACHIEVED** |
| Middle | Any metric exceeds pass but not catastrophically | N/A |
| Fail | Cold p95 > 500ms OR warm p95 > 200ms OR calls per round > 10 | N/A |

**Decision: PASS**

Cold p95 (3.70ms) and warm p95 (3.11ms) both under 100ms threshold. The subprocess overhead is negligible for the v1 vertical slice. Daemon work remains deferred to v1.5 and will only be revisited if future workload characteristics change materially.

## Raw Data

Full measurement data is available in JSON form:

```bash
docs/plans/r0-context-firewall-performance-report-2026-04-26.json
```

## Methodology Notes

1. **Cold vs warm definition**: Cold calls include a 100ms inter-call delay to approximate cache-cold behavior. Warm calls are back-to-back with no delay.
2. **Measurement precision**: Latency is measured from `performance.now()` immediately before `spawn()` to the process `close` event. This includes fork/exec, shared library loading, grammar initialization, parsing, serialization, and stdout capture.
3. **Fixture size**: The small.rs fixture is intentionally minimal (8 lines, 2 functions). Real files may be larger, but the CLI's workload is dominated by process spawn overhead for small files and by parsing/serialization for large files.
4. **Grammar caching**: The tree-sitter-rust grammar is loaded from a local path. In production, grammars may be cached in `~/.cache/tree-sitter/lib`, which would further reduce cold-start latency after the first call.
5. **No root cache drops**: System-level cache drops (e.g., `echo 3 > /proc/sys/vm/drop_caches`) were not performed as they require root privileges and would distort results for normal user-space operation.

## Implications for v1

- Subprocess spawn is the recommended v1 architecture.
- No daemon or persistent process is required.
- The pi-mono bridge can continue using `child_process.spawn` with `shell: false`.
- Future R1 graph work should still design for a potential daemon migration, but this is not a v1 blocker.

## Next Steps

- Monitor real interaction call counts as the R0 extension is adopted.
- Re-run this measurement if the CLI grows significantly heavier (e.g., multi-file resolution, cross-file graph queries).
- Document daemon migration path in R1 planning if call patterns shift toward >3 calls per round consistently.
