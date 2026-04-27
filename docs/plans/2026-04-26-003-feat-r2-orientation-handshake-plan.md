---
title: "feat: R2 v2 thin orientation handshake"
type: feat
status: active
date: 2026-04-26
origin: docs/brainstorms/2026-04-26-r2-orientation-handshake-requirements.md
---

# feat: R2 v2 thin orientation handshake

## Overview

R0（context firewall）与 R1（graph substrate）已分别完成，但两者尚未握手：bundle 二进制不读 `.tree-sitter-context-mcp/HEAD`、Provenance 中 `graph_snapshot_id` 与 `orientation_freshness` 永远是字符串 `"unknown"`、pi-mono 无任何机制消费 orientation。

本 plan 用 **4 个原子单元** 把握手协议落到最小可信形态：(U1) bundle 复用已有 `GraphStore::read_head` 把真实 snapshot id 与 typed freshness 写入 Provenance，并把 R0 测试与 plan 文档中所有 `"unknown"` 字面量硬断言一次扫齐；(U2) 新增 `tree-sitter-context orientation get` 子命令，输出 byte-stable 的 thin orientation block（deterministic stats + reserved postprocess fields）；(U3) 落 `scripts/orientation-handshake-harness.mjs` 与 fixture，把"端到端三条断言"做成 CI gate；(U4) 性能复测，确保 R12 门限仍 PASS、不需要 daemon 介入。

postprocess（Louvain / PageRank / centrality）、pi-mono 上游产品集成、R3 工具集、Auto-compact 替换 **均不在本 plan 范围**。

---

## Problem Frame

R2 是 R0 与 R1 之间唯一缺失的握手协议（see origin: `docs/brainstorms/2026-04-26-r2-orientation-handshake-requirements.md`）。三处现实证据：

- `crates/context/src/protocol.rs:41-52` — `Provenance::default()` 把两个字段硬编码 `"unknown"`。
- `crates/context/src/sexpr.rs:407-408` 与 `crates/context/tests/sexpr_contract.rs:52-53,149-150` — R0 sexpr contract test 依赖这个字面量。
- `crates/cli/src/bin/tree-sitter-context.rs` — bundle 二进制完全不读 HEAD；HEAD 写入仅在 R1 的 `crates/cli/src/context_graph.rs:203,430` 侧。

`GraphStore::read_head` (`crates/context/src/graph/store.rs:151`) 已经实现，并返回 typed `Result<GraphSnapshotId, GraphError>`，其中 `GraphError` 已含 `MissingSnapshot` / `CorruptedSnapshot` / `SchemaMismatch` / `WriteFailure` 四个变体（`crates/context/src/graph/snapshot.rs:159`+）。R2 实现侧的"共享 HEAD helper"不需要新写，只需要让 bundle 与新 orientation get 都通过它读 HEAD 并把 typed error 映射到合适的退出语义。

pi-mono 侧明确为**验收 harness，非产品交付**：harness 落本 repo 的 `scripts/`，不进 pi-mono submodule 上游 code path。

---

## Requirements Trace

- R1. **R2.1** `tree-sitter-context orientation get [--budget N] [--format sexpr|json]` 子命令；不改 `bundle` / `graph` 参数语义。
- R2. **R2.2** orientation 输出含 `graph_snapshot_id` / `schema_version` / `stats` / `top_referenced` / `entry_points` / 三个 reserved postprocess fields (`god_nodes` / `communities` / `architecture_summary` 全部值 `postprocess_unavailable`)。
- R3. **R2.3** orientation 输出 byte-stable：相同 snapshot+budget+format → 完全相同字节；canonical 排序 (repo-relative path, symbol_path, stable_id)；无时间戳 / 绝对路径 / 浮点。
- R4. **R2.4** budget 不足 → 按 stats > top_referenced > entry_points 优先级截断 + 显式 `(budget_truncated true reason ...)` 节点；estimated_tokens 不得 cap 在 budget。
- R5. **R2.5** bundle 读 HEAD，把当前 snapshot id 写入 `Provenance.graph_snapshot_id`；HEAD 缺失写 `"no_graph"`；不再写 `"unknown"`。
- R6. **R2.6** bundle 接受可选 `--orientation-snapshot-id <id>`；提供且与 HEAD 相等 → `fresh`，不等 → `stale`，未提供或 HEAD 缺失 → `unknown`。`{fresh, stale, unknown}` 三态 enum 不 widening。
- R7. **R2.7** bundle 与 orientation get 共用同一个 HEAD reader 路径（即 `GraphStore::read_head`）；任何时刻两者读到 HEAD 一致或都报同种 typed error。
- R8. **R2.8** bundle 输出 schema 不新增必填字段、不改 v1 字段语义；只允许把 `"unknown"` 占位升级为真实值或新 sentinel `"no_graph"`。
- R9. **R2.9** graph 未 build 时 orientation get → 退出码非 0 + stderr typed `no_graph`；bundle 仍执行 R0 v1 单文件路径，仅 Provenance 标 `no_graph` / `unknown`。
- R10. **R2.10** HEAD 损坏 / schema 不匹配 → 两者都返回 typed `graph_corrupt` / `schema_mismatch` 退出码，HEAD 文件不被覆写。
- R11. **R2.11** 仓内提供 `scripts/orientation-handshake-harness.mjs`；只用 Node ≥18 内建模块；不依赖 pi-mono 任何包；不修改 pi-mono submodule 任何文件；退出码作 CI gate。
- R12. **R2.12** harness 至少覆盖三条断言：(a) orientation get JSON schema 含全字段且 graph_snapshot_id 真值；(b) bundle (with id) → fresh；(c) update + 旧 id → stale。
- R13. **R2.13** 所有 `"unknown"` 字面量硬断言改写为枚举/格式断言；R0 plan / contract 文档示例同步更新。
- R14. **R2.14** R2 不替换 pi-mono 工具面 / Auto-compact / 不实现 R3 / 不引入 daemon / postprocess / 不自动注入 pi-mono prompt。
- R15. **R2.15** orientation 输出在 R3 升级时只允许新增字段；reserved postprocess 字段一旦换为真值，键名与位置保持。
- R16. **Performance**（origin Success Criteria 隐含 R12 门限延续）：R2 引入的 cold orientation get + bundle freshness 在 fixture repo 上仍满足 R0/R1 已建立的 subprocess p95 latency 预算（< 100ms cold path）。

**Origin actors:** A1 R2 orientation builder, A2 R2 bundle freshness wiring, A3 R1 graph store + HEAD（只读复用）, A4 pi-mono harness consumer, A5 operator/CI, A6 R3 future implementer.

**Origin flows:** F1 cold orientation get, F2 bundle with freshness, F3 HEAD-missing graceful, F4 harness verification, F5 R0 / R1 backward compatibility.

**Origin acceptance examples:** AE1 orientation sexpr 字段, AE2 byte stability, AE3 budget truncation, AE4 fresh end-to-end, AE5 no_graph 双面, AE6 stale, AE7 graph_corrupt, AE8 harness exit code, AE9 harness 不污染 pi-mono, AE10 fresh 严谨性, AE11 stale 严谨性, AE12 sexpr.rs 硬断言改写, AE13 R0 v1 contract test 不破, AE14 R3 thin→rich 升级不 break R2 协议。

---

## Scope Boundaries

- 不替换 pi-mono `read/write/edit/bash/grep/find/ls`；不引入 R3 工具（`safe_edit` / `find-callers` / `get_ranked_architecture` 等）。
- 不替换 pi-mono Auto-compact / 五层 compaction pipeline；不实现 graph-aware compaction runtime。
- 不在 pi-mono submodule 产品代码加 orientation 注入、freshness 检查或新工具；harness 仅 stay 在本 repo 测试路径。
- 不实现 god_nodes / communities / architecture_summary 真值计算；R2 中以 `postprocess_unavailable` 占位。
- 不引入 daemon / stdio JSON-RPC / MCP server / N-API / WASM bridge。
- 不实现 Two-Corrections Rule、`should_reorient`、exploration overlay、blast-radius graded invalidation。
- 不改 R0 v1 bundle 必填参数集合或必有输出字段集合；不 widening `orientation_freshness` 三态 enum。
- 不改 R1 graph build / update / diff / status / verify / clean 已有 CLI 参数与 snapshot manifest；R2 只读不写 graph store。

### Deferred to Follow-Up Work

- Postprocess 计算（Louvain / PageRank / community / god_nodes）— 单独 R3 thin→rich plan，复用 R2 reserved 字段位置。
- pi-mono 上游产品集成 — 由 pi-mono 维护方按 R2 锁定的 contract 自行落地（见 origin Success Criteria）。
- R3 agent-facing query primitives；Auto-compact 替换；S-expression 化整套 pi-mono tool result。
- daemon 决策：只有当 U4 报告显示 R12 门限被击穿时才重启评估。

---

## Context & Research

### Relevant Code and Patterns

- `crates/context/src/graph/store.rs` — `GraphStore::open(repo_root)` + `read_head() -> Result<GraphSnapshotId, GraphError>` 已实现；R2 直接复用，不新写 HEAD reader。
- `crates/context/src/graph/snapshot.rs:130` — `GraphSnapshot { schema_version, snapshot_id, files, edges, diagnostics, meta }`，含 `canonicalize_snapshot()`；orientation 计算从 GraphSnapshot 派生。
- `crates/context/src/graph/snapshot.rs:159` — `enum GraphError { MissingSnapshot, CorruptedSnapshot, SchemaMismatch, WriteFailure }`；R2 在 CLI 侧映射到 `no_graph` / `graph_corrupt` / `schema_mismatch` 退出语义。
- `crates/context/src/graph/xref.rs:20` — `resolve_xref(snapshot)` 与 `node_signature(...)`；orientation 的 top_referenced 排序键复用同一 cross-file inbound edge 计算。
- `crates/context/src/protocol.rs:41-52` — Provenance 结构 + `Default::default` 硬编码 `"unknown"`；U1 的关键修改点。
- `crates/context/src/sexpr.rs:264-280` — Provenance sexpr 序列化路径；U1 不改格式只改值来源。
- `crates/context/src/sexpr.rs:407-408` — sexpr 内联单元测试硬断言 `"unknown"`，U1 改写。
- `crates/context/tests/sexpr_contract.rs:52-53,149-150` — 对外 contract test 硬断言 `"unknown"`，U1 改写。
- `crates/context/tests/generated_types_contract.rs:58-59` — 子串断言（断言字段名出现），无需改动；U1 验证仍通过即可。
- `crates/cli/src/bin/tree-sitter-context.rs:107-...` — `BundleArgs` clap 结构；`Commands::Bundle` / `Commands::Graph` 两顶层；U1 在 BundleArgs 加 `--orientation-snapshot-id`，U2 加 `Commands::Orientation(OrientationArgs)`。
- `crates/cli/src/bin/tree-sitter-context.rs:291,306,342,381,400` — bundle 内 `Provenance::new("strategy", confidence)` 多处构造；U1 加 builder method 让这些调用顺手附加 (snapshot_id, freshness)。
- `crates/cli/src/context_graph.rs:217-238` — `graph_status` / `graph_verify` 已经走 `store.read_head().ok()` 模式；U1 / U2 沿用此 pattern 而不是再发明 HEAD 读路径。
- `crates/cli/src/context_graph.rs:524: render_json` — JSON 渲染辅助；U2 orientation `--format json` 路径直接复用。
- `crates/context/src/chunk.rs:195: estimate_tokens(byte_len)` — 现有 token 估算函数；U2 budget 必须复用同一 tokenizer，不能各算各的。
- `crates/cli/src/tests/context_bundle_test.rs` — bundle CLI integration test pattern，U1 扩展 `--orientation-snapshot-id` 用例时 mirror 该结构。
- `crates/context/tests/graph_snapshot_contract.rs` — graph snapshot contract test pattern；U2 orientation contract test 沿用。
- `crates/cli/src/tests/fixtures/` 与 `crates/cli/src/tests/helpers/fixtures.rs` — 现有 fixture 落点；U3 harness fixture 沿用。

### Institutional Learnings

- `docs/solutions/workflow-issues/tree-sitter-context-branch-review-2026-04-25.md` — "every primitive must surface reason / strategy / confidence / omissions"；R2 的 typed sentinels (`no_graph` / `graph_corrupt`) 是这一规则的延续。
- `docs/plans/r0-context-firewall-performance-report-2026-04-26.md` — R12 cold path subprocess p95 < 100ms 已 PASS；U4 复测必须在同 fixture / 同机型基线下复现这一门限。
- `docs/plans/r1-repo-map-performance-report-2026-04-26.md` — R1 build / update 各 phase 的 latency baseline；U4 报告需在此基础上叠加 orientation get + bundle freshness 两段新成本。
- `docs/plans/tree-sitter-context-rfc-2026-04-24.md` — 优先复用 loader / store / sexpr 等现有原语，不重写。
- `docs/brainstorms/2026-04-26-r0-context-firewall-requirements.md` line 96 — `orientation_freshness ∈ {fresh|stale|unknown}` enum 已锁；R2 的 R6 必须坚守这条 enum 不 widening。

### External References

- 不引入新外部参考。R0 + R1 + 现有 store / tags / sexpr 模式已足够支撑 R2 实现。

---

## Key Technical Decisions

- **U1 与 R0 硬断言扫尾合并为单原子 commit。** 理由：bundle 一改成"返回真值" → 现有断言立刻失败；split 会留 broken middle state。原子合并保证每个 commit 的 `cargo test` 都绿。
- **`graph_snapshot_id` 用 sentinel `"no_graph"`，不复用 `"unknown"`；`orientation_freshness` 仍只用 R0 锁定的三态 enum。** 理由：用同一 magic string 同时表达"没 graph"和"调用方没传 id"会让 caller 无法分支；R2 用 `graph_snapshot_id` 字段承载"有/没 graph"，`orientation_freshness` 字段承载"我能不能判断"，两层语义干净分离。
- **HEAD 读路径复用 `GraphStore::read_head`**（已存在于 `crates/context/src/graph/store.rs:151`），不新建 helper module。理由：R2.7 共享要求已被 R1 落实；新建 module 仅是符号复制，徒增维护面。
- **GraphError 变体不扩展。** R2 在 CLI 边界把已有 `MissingSnapshot` / `CorruptedSnapshot` / `SchemaMismatch` 三态映射到对外 `no_graph` / `graph_corrupt` / `schema_mismatch` 退出语义；context crate 内部错误集合不动。
- **orientation 默认 sexpr / `--format json` 是 escape hatch。** sexpr 是 R0 锁定的 prompt-cache-friendly canonical；harness 用 json 仅为断言便利。
- **budget 截断顺序固定为 stats > top_referenced > entry_points。** R2 不引入 `--include` 白名单。理由：可配置截断会让 byte-stability 检查矩阵爆炸；先固定顺序，下一阶段需要再开。
- **Token 估算复用 `estimate_tokens(byte_len)`。** R2 不引入新 tokenizer。理由：避免 cache key 漂移（R0 estimated_tokens 已用此函数）。
- **harness 单文件 ESM，无 npm 依赖。** 路径定为 `scripts/orientation-handshake-harness.mjs`。理由：用户明确"动了就是动了，但只在测试路径里，随时可以拔掉"；零 npm 依赖最大化可拔出性。
- **harness fixture 落 `crates/cli/src/tests/fixtures/orientation_handshake/`。** 沿用既有 fixture 目录约定；harness 用 `child_process` 在 tempdir 中复制此 fixture 后操作，保证 CI 可重入。
- **CI gate 集成**：在现有 `.github/workflows/` 中已存在的 Rust test workflow 末尾追加一个独立 step 跑 harness（需要 Node ≥18 runner）；不创建新 workflow 文件。如果当前 runner 没装 Node，单独 setup-node。

---

## Open Questions

### Resolved During Planning

- HEAD 读 helper 模块落点 → 复用 `crates/context/src/graph/store.rs::GraphStore::read_head`。
- HEAD 读 typed error 命名 → 沿用 `GraphError::MissingSnapshot` / `CorruptedSnapshot` / `SchemaMismatch`。
- "unknown" 字面量影响范围 → 已在 Context & Research 中列全集。
- Budget 优先级是否可配 → 不可配，固定顺序。
- harness 路径 + CI 集成方式 → `scripts/orientation-handshake-harness.mjs`，piggyback 到现有 Rust test workflow。
- R3 字段保留策略 → 三个 reserved postprocess 字段键名 + sexpr 位置保持（R2.15 由 U2 contract test 守住）；R3 可继续 additive 增加新字段。

### Deferred to Implementation

- **`Provenance::new` 的 builder 形态** — 是 `Provenance::with_graph_state(snapshot_id, freshness)` 链式 method 还是新 constructor `Provenance::new_with_head(strategy, confidence, head_info)`：U1 实现时选择对 5 个调用点改动最小的版本。
- **`OrientationBlock` Rust struct 字段细节** — `top_referenced` / `entry_points` 元素结构（建议含 `symbol_path` / `path` / `stable_id` / `score`，但具体字段名 + 是否 flatten 由 U2 实现时根据 sexpr canonical 美感决定）。
- **`postprocess_unavailable` 在 sexpr / json 中的精确形态** — 裸符号 `(god_nodes postprocess_unavailable)` vs 带 reason 的子结构 `(god_nodes (status postprocess_unavailable) (reason ...))`。U2 实现时与现有 `unsupported` / `unknown_cross_file` 风格对齐。
- **CI runner Node 版本** — 现有 workflow 是否已 setup-node：U3 实现时 grep `.github/workflows/`；如果没有则新增 setup-node step。
- **harness fixture 内容** — 用 R1 报告中的"2 文件 ~20 行 Rust"足以覆盖 happy path，是否再加一个 cross-file 引用的小例子让 top_referenced 有非 trivial 排序：U3 实现时根据 fixture 跑出的 orientation 实际形态决定。

---

## High-Level Technical Design

> *以下示意 R2 端到端调用链与 orientation block 形态，仅作 review 时方向性参考，不是实现规范。实施 agent 把它当 context，不要逐字复刻。*

### 调用链 sequence

```mermaid
sequenceDiagram
    participant H as harness.mjs
    participant CLI as tree-sitter-context CLI
    participant Store as GraphStore
    participant Disk as .tree-sitter-context-mcp/

    H->>CLI: graph build
    CLI->>Store: write snapshot + update HEAD
    Store->>Disk: write HEAD = X

    H->>CLI: orientation get --format json
    CLI->>Store: open + read_head() -> X
    CLI->>Store: read_snapshot(X)
    CLI->>CLI: build OrientationBlock (thin stats + reserved postprocess)
    CLI-->>H: { graph_snapshot_id: X, stats, top_referenced, entry_points, god_nodes: "postprocess_unavailable", ... }

    H->>CLI: bundle <path> --stable-id <id> --orientation-snapshot-id X
    CLI->>Store: read_head() -> X
    CLI->>CLI: Provenance.graph_snapshot_id = X; freshness = X==X ? fresh : stale
    CLI-->>H: bundle result with provenance(graph_snapshot_id=X, orientation_freshness=fresh)

    Note over H,CLI: harness 修改 fixture + graph update -> HEAD = Y
    H->>CLI: bundle <path> --stable-id <id> --orientation-snapshot-id X
    CLI->>Store: read_head() -> Y
    CLI->>CLI: Provenance.graph_snapshot_id = Y; freshness = X != Y -> stale
    CLI-->>H: bundle result with provenance(graph_snapshot_id=Y, orientation_freshness=stale)
```

### Orientation block sexpr 形态（directional sketch）

```text
(orientation
  (schema_version "1")
  (graph_snapshot_id "<XXH3 hex>")
  (stats
    (file_count 42)
    (symbol_count 318)
    (language_count 3)
    (edge_count 720))
  (top_referenced
    ((symbol_path "rust::crate::module::Foo") (path "src/foo.rs") (stable_id "...") (inbound_refs 17))
    ...)
  (entry_points
    ((symbol_path "rust::crate::main") (path "src/main.rs") (stable_id "..."))
    ...)
  (god_nodes postprocess_unavailable)
  (communities postprocess_unavailable)
  (architecture_summary postprocess_unavailable))
```

JSON 形态镜像同结构（snake_case keys，arrays 用 [] 而非 cons cells）。

### 退出语义矩阵

| 场景 | bundle 退出码 | bundle.graph_snapshot_id | bundle.orientation_freshness | orientation get 退出码 | orientation get stderr |
|------|----|----|----|----|----|
| graph 已 build, 传 id 等于 HEAD | 0 | `<XXH3>` | `fresh` | 0 | — |
| graph 已 build, 传 id 不等于 HEAD | 0 | `<XXH3>` | `stale` | 0 | — |
| graph 已 build, 不传 id | 0 | `<XXH3>` | `unknown` | 0 | — |
| graph 未 build | 0 | `"no_graph"` | `unknown` | ≠0 | typed `no_graph` |
| HEAD 损坏 | ≠0 | (typed err) | (typed err) | ≠0 | typed `graph_corrupt` |
| schema_version 不匹配 | ≠0 | (typed err) | (typed err) | ≠0 | typed `schema_mismatch` |

---

## Implementation Units

- U1. **HEAD-shared bundle wiring + R0 hard-assertion sweep**

**Goal:** bundle 二进制读 HEAD，把真实 `graph_snapshot_id` 与 typed `orientation_freshness` 写入 Provenance；同 commit 把所有 `"unknown"` 字面量硬断言与 R0 plan 文档示例改成枚举/格式断言，保证每个 commit 测试都绿。

**Requirements:** R5, R6, R7, R8, R13。覆盖 origin F2 / F3 / F5 / AE4 / AE5 / AE6 / AE12 / AE13。

**Dependencies:** None（R1 已落地的 GraphStore + GraphError）。

**Files:**
- Modify: `crates/cli/src/bin/tree-sitter-context.rs` — `BundleArgs` 加 `--orientation-snapshot-id <ID>`；`run_bundle` 调 `GraphStore::open(repo_root).read_head()`；map `GraphError::MissingSnapshot` → `("no_graph", "unknown")`；`CorruptedSnapshot` / `SchemaMismatch` → 退出码非 0 + typed stderr。把所有 5 处 `Provenance::new(...)` 改成新 builder 形态。
- Modify: `crates/context/src/protocol.rs` — Provenance 加 `with_graph_state(snapshot_id: impl Into<String>, freshness: impl Into<String>) -> Self` builder；`Default` 保持 `"unknown"`（仅 fallback 路径用）。
- Modify: `crates/context/src/sexpr.rs` — 第 407-408 行内联测试改 enum/format 断言；序列化路径不变。
- Modify: `crates/context/tests/sexpr_contract.rs` — 第 52-53、149-150 行硬断言改 `assert!(matches!(parsed.orientation_freshness.as_str(), "fresh"|"stale"|"unknown"))` + `assert_ne!(parsed.graph_snapshot_id, "unknown")`。新增 fresh / stale / no_graph / unspecified 四态各一组断言。
- Modify: `docs/plans/sexpr-canonical-form-v1.md` — 第 100-101 行示例从 `"unknown"` 改成 `"<XXH3>"` / `"no_graph"` 与 `fresh|stale|unknown`。
- Modify: `docs/plans/r0-orientation-compaction-v2-contract.md` — 第 120-121 行 v1 limitation 注释改为"R2 完成；现状 …"。
- Modify: `docs/plans/tree-sitter-context-cli-v1-contract.md` — 第 142-143 行 out-of-scope 条目移到"R2 已完成"小节。
- Modify: `docs/plans/2026-04-26-001-feat-r0-context-firewall-plan.md` — 第 204、292、305、443、554 行示例同步更新；保留历史叙述（"R0 当时"），但新增"R2 之后"小节说明 R0 锁定的契约现已被 R2 实化。
- Test: `crates/cli/src/tests/context_bundle_test.rs` — 新增四个 case 覆盖 `--orientation-snapshot-id` × {provided==HEAD, provided!=HEAD, unprovided, no graph}。

**Approach:**
- 让 `run_bundle` 在构造 Provenance 之前先尝试 `GraphStore::open(repo_root)` + `.read_head()`：成功 → 真值；`MissingSnapshot` → sentinel `"no_graph"`；`CorruptedSnapshot` / `SchemaMismatch` → 立刻 `eprintln!` typed message + 非 0 退出码（R10 严格要求："HEAD 文件不被覆写" 由 read 路径自然满足，因为 R2 不写 HEAD）。
- repo root 解析：bundle 已有 `<path>` 入参；从该 path 沿父目录寻找含 `.tree-sitter-context-mcp/` 的目录作为 repo root（与 `graph_status` 同 helper；如 `graph_status` 当前在 `context_graph.rs` 中私有，可新增 `pub fn resolve_repo_root` 暴露给两个 binary path）。
- 把 5 处 `Provenance::new(...)` 替换成 `Provenance::new(...).with_graph_state(snapshot_id, freshness)`；`Default::default()` 保持作为 fallback（少数 not-applicable 测试路径仍需要它）。
- 文档同步只改示例文本与"现状"小节；不改 R0 / R1 的设计决策叙述。

**Patterns to follow:**
- `crates/cli/src/context_graph.rs:graph_status` — 已是"open store + read_head + typed diagnostic"的范式，复制其错误处理姿态。
- `crates/cli/src/tests/context_bundle_test.rs` 现有 happy-path test 的 fixture setup 模式。

**Test scenarios:**
- *Happy path — Covers AE4.* 在 fixture repo 跑 `graph build` 后 `bundle <path> --stable-id <id> --orientation-snapshot-id <X>`（X 等于 HEAD）→ stdout sexpr 含 `(graph_snapshot_id "<X>")` 与 `(orientation_freshness "fresh")`。
- *Edge case — Covers AE6.* `graph build` → `graph update`（fixture 已改） → `bundle ... --orientation-snapshot-id <X_old>` → `graph_snapshot_id` 是 HEAD 当前真值 (≠X_old)，`orientation_freshness == "stale"`。
- *Edge case.* `bundle` 不传 `--orientation-snapshot-id` → `graph_snapshot_id` 是 HEAD 当前真值，`orientation_freshness == "unknown"`，退出 0。
- *Error path — Covers AE5.* 在没 graph 的 fresh repo 调 `bundle ...` → `graph_snapshot_id == "no_graph"`，`orientation_freshness == "unknown"`，退出 0。
- *Error path — Covers AE7.* 手工写一个非 hex 的 HEAD 文件 → 调 `bundle ...` → 非 0 退出 + stderr 含 typed `graph_corrupt`；HEAD 文件内容未被覆写。
- *Contract regression — Covers AE12, AE13.* 全量跑 `cargo test -p tree-sitter-context` 与 `cargo test -p tree-sitter-cli` → 全部通过；`grep -rn '"unknown"' crates/context/src crates/context/tests` 不应找到任何未在白名单内的硬断言。
- *Contract regression — Covers AE13.* `bundle --budget N --max-tokens N --format sexpr --tier sig --stable-id <id> <path>` 全部 R0 v1 参数组合的现有 contract test 全绿。

**Verification:**
- 新增 4 条 bundle CLI integration test 全部 PASS。
- `cargo test --workspace` 全绿。
- `grep -rn '"unknown"' crates/context/ docs/plans/sexpr-canonical-form-v1.md docs/plans/r0-orientation-compaction-v2-contract.md docs/plans/tree-sitter-context-cli-v1-contract.md` 不再含与 graph_snapshot_id / orientation_freshness 相关的字面量硬断言或硬编码示例。

---

- U2. **`tree-sitter-context orientation get` 子命令（thin）**

**Goal:** 新增顶层子命令 `orientation get`，在 R1 graph 之上派生 byte-stable thin orientation block；输出含 deterministic stats、top_referenced、entry_points 与三个 reserved postprocess 字段（值固定 `postprocess_unavailable`）；支持 `--budget` 与 `--format {sexpr,json}`，sexpr 默认。

**Requirements:** R1, R2, R3, R4, R7, R9, R10, R15。覆盖 origin F1 / F3 / AE1 / AE2 / AE3 / AE7 / AE14。

**Dependencies:** U1（U1 暴露的 `resolve_repo_root` helper / Provenance freshness 路径已稳）。

**Files:**
- Create: `crates/context/src/orientation.rs` — `pub struct OrientationBlock { schema_version, graph_snapshot_id, stats, top_referenced, entry_points, god_nodes, communities, architecture_summary }`；`pub fn build_orientation(snapshot: &GraphSnapshot, budget: Option<usize>) -> OrientationBlock`；canonical 排序逻辑；budget 截断 + `(budget_truncated true reason ...)` 节；reserved 字段统一返回 `OrientationField::PostprocessUnavailable`。
- Modify: `crates/context/src/lib.rs` — `pub mod orientation;` re-export `OrientationBlock` 与 `build_orientation`。
- Modify: `crates/context/src/sexpr.rs` — 新增 `pub fn orientation_to_sexpr(block: &OrientationBlock) -> String`；与 Provenance sexpr 同 canonical 风格。
- Create / Modify: `crates/cli/src/context_graph.rs` — 新增 `OrientationGetOptions { repo_root, budget, format }` / `OrientationGetResult { block, format, diagnostics }` / `pub fn orientation_get(opts) -> Result<...>`；GraphError 映射沿用 U1 helper。
- Modify: `crates/cli/src/bin/tree-sitter-context.rs` — 新增 `Commands::Orientation(OrientationArgs)`；`OrientationArgs` 子命令枚举只有一个 `Get(OrientationGetArgs)`；run_orientation 入口；非 0 退出码处理 typed errors。
- Test: `crates/context/tests/orientation_block_contract.rs` (new) — OrientationBlock 字段断言、byte-stability 断言、budget 截断断言、reserved postprocess 字段断言、R3 升级形态模拟（手工填 god_nodes 后跑加载测试）。
- Test: `crates/cli/src/tests/orientation_get_test.rs` (new) — CLI 集成：`orientation get --format json`、`--format sexpr`、`--budget 100`、no_graph 错误退出码、graph_corrupt 错误退出码。

**Approach:**
- `OrientationBlock` 字段排序：`stats` 子结构按字段定义顺序输出；`top_referenced` 排序键 `(-inbound_refs, repo_relative_path, symbol_path, stable_id)`，取 `top-N`（默认 N=20，受 budget 截断影响）；`entry_points` 选取来自 tags 中 `definition.public` / `class.public` / `function.public` 等 capture 的 symbol，且本文件无入边（即非内部递归调用），按 `(repo_relative_path, symbol_path, stable_id)` 排序。
- top_referenced 的 inbound_refs 计算：遍历 snapshot.edges，过滤 `EdgeKind::Reference` 且 `confidence == confirmed` 且跨文件，按 target node handle 聚合计数。
- budget 截断：先编码完整 block 估 tokens，超过 budget 则按 stats > top_referenced > entry_points 顺序削减；削减后再编码一次，若仍超，递减 N 直至 stats-only；最终 emit `(budget_truncated true reason ... omitted [...])`。tokenizer 直接调 `crate::chunk::estimate_tokens`。
- `--format sexpr` 与 `--format json` 共享 OrientationBlock；只在序列化层分流。json 路径复用 `crates/cli/src/context_graph.rs:render_json`。
- typed error 映射：复用 U1 思路 — `GraphError::MissingSnapshot` → 退出码 2 + stderr `no_graph: run \`tree-sitter-context graph build\` first`；`CorruptedSnapshot` → 退出码 3 + stderr `graph_corrupt: ...`；`SchemaMismatch` → 退出码 4 + stderr `schema_mismatch: expected=..., found=...`。
- byte-stability 实测：在 contract test 中跑两遍 `build_orientation(...)` + `orientation_to_sexpr(...)` → `assert_eq!(bytes_a, bytes_b)`。

**Patterns to follow:**
- `crates/context/src/graph/snapshot.rs:canonicalize_snapshot` — 同款 canonical 排序 + serde-stable 字段顺序的实现姿态。
- `crates/context/src/sexpr.rs` 既有 Provenance / Bundle sexpr writer — escape_string、字段顺序、原子节点拼接风格。
- `crates/context/tests/graph_snapshot_contract.rs` — contract test 风格（fixture 输入 → snapshot id 断言 → field 断言）。

**Test scenarios:**
- *Happy path — Covers AE1.* fixture build → `orientation get --format sexpr` → 含 `(graph_snapshot_id "<hex>")`、`(stats ...)`、`(top_referenced ...)`、`(entry_points ...)`、`(god_nodes postprocess_unavailable)`、`(communities postprocess_unavailable)`、`(architecture_summary postprocess_unavailable)`。
- *Happy path — Covers AE1.* `--format json` → 输出能被 `serde_json::from_str::<Value>` parse 且含相同字段集合（snake_case keys）。
- *Edge case — Covers AE2.* 同 fixture / 同 budget / 同 format 跑两次 → 字节完全相等；改 fixture + graph update 后 graph_snapshot_id 与 stats 改变，但 reserved postprocess 字段保持 `postprocess_unavailable`。
- *Edge case — Covers AE3.* `--budget 100`（远小于自然输出）→ stats 保留、top_referenced 与 entry_points 至少削减、输出含 `(budget_truncated true reason "budget_exhausted" omitted (...))`；estimated_tokens 不被 cap 在 100。
- *Error path — Covers AE5.* fresh repo 无 graph build → `orientation get` 退出码非 0；stderr 含 `no_graph` 字面量；stdout 为空或无误导内容。
- *Error path — Covers AE7.* HEAD 文件被破坏成非 hex → `orientation get` 退出码非 0；stderr 含 `graph_corrupt`；HEAD 文件未被覆写。
- *Edge case — Covers AE14.* contract test 手工构造 `OrientationBlock` 把 god_nodes 替换成真值数组 → R2 当下 contract test 仅"god_nodes 必须为字符串 postprocess_unavailable"那条 fail，其余 schema 断言全绿，证明 R3 升级路径不破。
- *Integration.* CLI `orientation get --budget 2000` 与 `bundle --orientation-snapshot-id <id>` 跑同一 repo → 两者读到相同 HEAD（写一个 stress test：在两次 CLI 调用之间不变 HEAD → 抓出 graph_snapshot_id 必相等）。

**Verification:**
- `cargo test -p tree-sitter-context --test orientation_block_contract` 全绿。
- `cargo test -p tree-sitter-cli --test orientation_get_test` 全绿。
- 手动 `tree-sitter-context orientation get --format sexpr | wc -c` 在同一 fixture 上跑两次输出相同字节数；`cmp` 全字节一致。

---

- U3. **End-to-end harness 脚本 + fixture**

**Goal:** 落 `scripts/orientation-handshake-harness.mjs` 与 fixture，把"graph build → orientation get → bundle (fresh) → graph update → bundle (stale)"串成单脚本退出码作 CI gate；harness 完全不依赖 pi-mono 任何包，不修改 pi-mono submodule。

**Requirements:** R11, R12, R14。覆盖 origin F4 / AE8 / AE9 / AE10 / AE11。

**Dependencies:** U1, U2（harness 验证 U1 freshness 行为 + U2 orientation get 输出）。

**Files:**
- Create: `scripts/orientation-handshake-harness.mjs` — Node ≥18，仅 import `node:assert/strict`、`node:child_process`、`node:fs/promises`、`node:path`、`node:os`；流程：在 OS tempdir 创建 fixture 副本 → spawn `tree-sitter-context graph build` → spawn `tree-sitter-context orientation get --format json --budget 2000` → 解析 stdout 拿 `graph_snapshot_id` → spawn `tree-sitter-context bundle <fixture-path> --stable-id <fixture-stable-id> --orientation-snapshot-id <X> --format json` → 断言 `orientation_freshness === "fresh"` 与 `graph_snapshot_id === X`；改 fixture 一文件 → spawn `graph update` → spawn `bundle ... --orientation-snapshot-id <X>` → 断言 `orientation_freshness === "stale"` 且 `graph_snapshot_id !== X`；任一断言失败立刻非 0 退出 + stderr 指向具体行。
- Create: `crates/cli/src/tests/fixtures/orientation_handshake/` — 至少 2 个 .rs 文件：`a.rs` 定义 pub fn `target` 并由 `b.rs` 调用，让 graph 中存在 cross-file inbound reference，使 top_referenced 非 trivial。harness 修改 fixture 时改 `target` 函数体（非签名）来产生 stale 而不变 stable_id。
- Modify: `.github/workflows/<existing-ci>.yml` — 在 Rust test job 后追加 step：`actions/setup-node@v4` (if 缺) + `node scripts/orientation-handshake-harness.mjs`；step 名 `orientation-handshake-harness`。
- Create / Modify: 如果 `crates/cli/src/tests/helpers/fixtures.rs` 提供 fixture path resolver，让 harness 通过 `cargo run --bin tree-sitter-context -- ...` 而非裸 binary path 调用，免去 binary 位置查找。

**Approach:**
- harness 用 `cargo run --quiet -p tree-sitter-cli --bin tree-sitter-context --` 作为 CLI 调用前缀，避免 build 路径硬编码。
- fixture 复制到 tempdir，避免污染源 fixture 目录（CI 上 source readonly 也可工作）。
- 三条断言的实现严格匹配 R12 / AE10 / AE11：fresh 断言前不调 update、不改 fixture；stale 断言必须显式 update + 改 fixture，否则断言主动 fail（用 `assert.notStrictEqual(snapshotIdAfterUpdate, X)` 守住）。
- stderr 上的失败信息按 `[harness] step=<X> reason=<Y> expected=<E> actual=<A>` 格式输出，方便 CI log 定位。
- harness 不引入 npm 依赖、不写 `package.json`、不创建 `node_modules`；ESM 单文件。

**Patterns to follow:**
- 不存在 Node harness 先例；遵循"单文件、零依赖、内建模块"的最小化原则（CLAUDE.md 也强调测试路径可拔出）。

**Test scenarios:**
- *Happy path — Covers AE8, AE10.* 在干净 tempdir 跑 harness → 三条断言全 PASS → 退出码 0。
- *Negative — Covers AE10.* 故意在 fresh 断言前手动修改 fixture（mock harness 行为）→ stale 应该被检测出 → harness 在 fresh 断言处主动 fail；这条用 unit-style 子测试或在 harness 内开 `--self-test` flag 验证。
- *Negative — Covers AE11.* 跳过 graph update 直接断言 stale → harness 在 stale 断言处主动 fail（id 与旧 X 仍相等）。
- *Static — Covers AE9.* `grep -E "from\s+['\"]|require\(['\"]" scripts/orientation-handshake-harness.mjs` 仅匹配 `node:` 前缀模块；脚本不出现 pi-mono / @pi 等字符串。
- *Static — Covers AE9.* `git diff --stat pi-mono/` 在 PR 范围内为空（CI 可加 grep 步骤断言）。
- *Integration.* CI workflow run 在 PR 上出现 `orientation-handshake-harness` 步骤且 PASS。

**Verification:**
- 本地 `node scripts/orientation-handshake-harness.mjs` 退出 0。
- CI 在 PR 上 `orientation-handshake-harness` step PASS。
- 故意打破 U1 / U2（commit 一个回退 freshness 计算的 bad change 在 dry-run branch）应当让 harness fail，证明 gate 真起作用；该验证不需要进 git，仅在 review 阶段口头/screenshot 演示。

---

- U4. **R12 性能门复测 + R2 报告**

**Goal:** 在 R0 / R1 已建立的 fixture 与机型下，复测加 U1 + U2 + U3 之后 cold-path latency；产出 `docs/plans/r2-orientation-handshake-performance-report-2026-04-26.md` 报告，明确 R12 subprocess p95 < 100ms 是否仍 PASS；如未 PASS，列出 daemon 决策再评估理由。

**Requirements:** R16（origin Success Criteria 引申）。

**Dependencies:** U1, U2, U3。

**Files:**
- Create: `docs/plans/r2-orientation-handshake-performance-report-2026-04-26.md` — 沿用 R0 / R1 性能报告 frontmatter (`gate: pass | fail`)；含 measurement setup（fixture / 机型 / cargo profile）、cold orientation get latency table、bundle freshness 增量 latency table、R12 gate 结论、daemon 决策结论。
- Modify (if needed): `crates/cli/src/tests/perf/` 或 `scripts/` — 如果现有 R0 / R1 perf 测量脚本有可复用 harness，扩展一条 orientation get 与 bundle 含 freshness 的 measurement；如果是手工 timing，则在报告中描述 measurement 步骤而不增脚本。

**Approach:**
- 测量 fixture：使用 `crates/cli/src/tests/fixtures/orientation_handshake/` 同款 fixture（与 R1 报告"2 文件 ~20 行"同量级，便于横向对照）。
- 测量条目：
  1. `orientation get --format sexpr --budget 2000`（cold path: open store + read head + read snapshot + canonicalize + emit）。
  2. `bundle <path> --stable-id <id> --orientation-snapshot-id <X>`（U1 后）vs 不传 id（基线）。
  3. `bundle ... --orientation-snapshot-id <X>` 在 graph 不存在时 (`no_graph` 路径)。
- gate：每项 cold path 单次 subprocess wall time p95 < 100ms（沿用 R0 R12 阈值）。
- 报告结构：Summary（PASS / FAIL 一句话）→ Setup → Cold orientation get table → Bundle freshness delta table → Gate 结论 → Daemon 决策（PASS 时维持 R0 / R1 既有"不引入 daemon"的决议；FAIL 时列出后续 plan，但不在本 plan 中实施）。
- 如果 FAIL，标记报告 `gate: fail` 并在 Outstanding 中列出"daemon 决策必须在下一阶段优先重启"，**不**在本 plan 中尝试修复（这是 R2 边界外）。

**Patterns to follow:**
- `docs/plans/r0-context-firewall-performance-report-2026-04-26.md` — frontmatter + 表格风格。
- `docs/plans/r1-repo-map-performance-report-2026-04-26.md` — phase-by-phase latency table 风格。

**Test scenarios:**
- *Verification scenario.* 同 fixture / 同机型 / 同 cargo release profile 下连续测 5 次取中位数与 p95；写入报告。
- *Verification scenario.* 比对 R1 报告中 "Total cold build ~10-15ms" 与 R2 cold orientation get 数据，写明增量来源（snapshot deserialize 加 emit）。
- *Test expectation: none for this unit's code paths — 报告本身是产出物，不是新功能；本单元不引入新生产代码。*

**Verification:**
- 报告文件存在并通过 markdown lint。
- 报告 frontmatter `gate: pass`（理想情况）；如 `gate: fail`，PR description 明确标注并触发后续 daemon 评估单独 plan。
- 报告引用的 latency 数据可在本机重现（CI 不强制跑此 perf）。

---

## System-Wide Impact

- **Interaction graph:** R2 加深 `crates/cli/src/bin/tree-sitter-context.rs` 与 `crates/context/src/graph/store.rs` 的耦合（bundle 路径新增 store open）；其他 binary（`tree-sitter` 主 CLI、`tree-sitter-context graph` 各子命令）不受影响。
- **Error propagation:** GraphError 在 CLI 边界统一映射到 typed exit code（0 / 2 / 3 / 4）+ stderr key（`no_graph` / `graph_corrupt` / `schema_mismatch`）；harness 与未来 pi-mono 都靠这个映射分支。R0 已有的"Provenance.confidence + reason + omissions"姿态不变。
- **State lifecycle risks:** R2 不写 graph store 也不写 HEAD；唯一 lifecycle 风险是 reader-writer race（orientation get 读到一半被 graph update 替换 HEAD）。本 plan 不实现 reader-side advisory lock（成本不对称），但 risk 表中显式记录；harness 在串行执行下不会触发，CI 安全。
- **API surface parity:** Provenance struct 字段集合不变；只升级 default 值与 builder 入口。`bundle` 加 `--orientation-snapshot-id` 是可选 flag，对既有调用方完全无破坏。orientation 是新顶层子命令，与 graph / bundle 平行，不冲突。
- **Integration coverage:** harness 提供端到端真实进程链路覆盖；Rust 单元/集成测试只能证 in-process 行为。两者互补不重叠。
- **Unchanged invariants:**
  - R0 v1 bundle CLI 参数集合（`--budget`、`--max-tokens`、`--format {sexpr,json}`、`--tier`、`--stable-id`、positional path）行为完全不变；只新增可选 `--orientation-snapshot-id`。
  - R1 graph build / update / status / verify / diff / clean CLI 参数与输出 schema 完全不变。
  - S-expression canonical form（escape rules、节点排序）、tokenizer (`estimate_tokens`)、stable_id 计算 全部复用。
  - pi-mono submodule 内任何文件未被本 plan 修改。

---

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| HEAD reader-writer race（orientation get 与并发 graph update 之间）| typed `graph_corrupt` 是底线；harness 串行执行；reader-side advisory lock 推后到 follow-up plan，前提是真实 race 在 monorepo 出现。 |
| Byte-stability 因 floating-point / 系统时间渗入而漂移 | OrientationBlock 字段类型限定为整数 / String / Vec；contract test 跑两遍字节比对作回归 gate；不允许浮点排序键。 |
| `"unknown"` 字面量散落范围未知 | U1 实现前先 grep 全集（已在 Context 列出），文档与代码 sweep 列入同一 commit；CI 可加 lint 防回归（`grep -n '"unknown"' crates/context/src/protocol.rs` 不应出现 graph_snapshot_id 默认）。 |
| Tokenizer 漂移导致 prompt-cache key 失效 | U2 显式复用 `crate::chunk::estimate_tokens`，code review 守住；不引入新 tokenizer 是 Key Decision。 |
| harness 在 CI runner 上 Node 缺失 | CI workflow 在 step 前 `actions/setup-node@v4` if needed；本地开发 README 补一行 Node ≥18 要求。 |
| R12 性能门 FAIL（增量 cold path 把单次 subprocess 推过 100ms） | U4 报告 `gate: fail` + 在 PR description 显式标记；不在本 plan 修复，触发独立 daemon 评估 plan；R1 已经留好这个升级路径（origin Risks 已述）。 |
| Pi-mono 维护方读了 contract 但实际集成发现字段缺失 | U2 OrientationBlock 字段集合在 contract test 中冻结；未来字段只能 additive；R3 plan 必须沿用同一 OrientationBlock struct，不可重发结构。 |
| 文档 / 代码示例未同步 | U1 把所有 doc 示例 (sexpr-canonical-form-v1.md / r0-orientation-compaction-v2-contract.md / cli-v1-contract.md / R0 plan) 与代码同 commit 修改；CI 可在 review 阶段加文档 grep gate。 |

---

## Documentation / Operational Notes

- README / CONTRIBUTING 不需要改（命令面新增是内部 contract 性质，pi-mono 维护方阅读 origin 文档即可）。
- `docs/plans/tree-sitter-context-cli-v1-contract.md` 中"R2 已完成 / R0 锁定字段已实化"小节由 U1 添加。
- `docs/plans/r0-orientation-compaction-v2-contract.md` 第 120-121 的"v1 limitation"由 U1 改写为"R2 完成现状"。
- U4 性能报告作为本 plan 的最终交付物之一；下一阶段 plan（R3 thin→rich）必须 link 到本报告以确认 daemon 决策状态。
- CLAUDE.md remote safety（"Never push to upstream"）适用：U3 的 harness fixture 与 scripts 都留 origin fork；任何文档 / 示例提及 pi-mono 相关路径时 reiterate 此规则。

---

## Sources & References

- **Origin document:** `docs/brainstorms/2026-04-26-r2-orientation-handshake-requirements.md`
- Related code: `crates/context/src/graph/store.rs` (HEAD reader), `crates/context/src/graph/snapshot.rs` (GraphError), `crates/context/src/protocol.rs:41-52` (Provenance), `crates/cli/src/bin/tree-sitter-context.rs` (bundle binary), `crates/cli/src/context_graph.rs` (graph CLI helpers).
- Related plans: `docs/plans/2026-04-26-001-feat-r0-context-firewall-plan.md` (R0), `docs/plans/2026-04-26-002-feat-r1-repo-map-plan.md` (R1), `docs/plans/r0-orientation-compaction-v2-contract.md` (orientation compaction v2 锁定项), `docs/plans/tree-sitter-context-cli-v1-contract.md` (R0 v1 CLI 锁定).
- Related contract tests: `crates/context/tests/sexpr_contract.rs`, `crates/context/tests/generated_types_contract.rs`, `crates/context/tests/graph_snapshot_contract.rs`, `crates/cli/src/tests/context_bundle_test.rs`.
- Related performance baselines: `docs/plans/r0-context-firewall-performance-report-2026-04-26.md`, `docs/plans/r1-repo-map-performance-report-2026-04-26.md`.
- Institutional learnings: `docs/solutions/workflow-issues/tree-sitter-context-branch-review-2026-04-25.md`.
