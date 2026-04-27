---
title: "R3 PageRank God Nodes Postprocess Requirements"
type: requirements
status: draft
date: 2026-04-26
origin:
  - "docs/ideation/2026-04-26-tree-sitter-pi-integration-ideation.md#4-first-class-repo-map-via-symbol-graph"
  - "docs/ideation/2026-04-26-tree-sitter-repo-navigation-ideation.md#r3-query-primitive-set-four-categories-s-expression-output-every-call-budgeted"
  - "docs/brainstorms/2026-04-26-r2-orientation-handshake-requirements.md"
dependencies:
  - "docs/plans/2026-04-26-001-feat-r0-context-firewall-plan.md"
  - "docs/plans/2026-04-26-002-feat-r1-repo-map-plan.md"
  - "docs/plans/2026-04-26-003-feat-r2-orientation-handshake-plan.md"
  - "docs/plans/tree-sitter-context-cli-v1-contract.md"
  - "docs/plans/tree-sitter-context-graph-r1-contract.md"
  - "docs/plans/r0-orientation-compaction-v2-contract.md"
  - "docs/plans/sexpr-canonical-form-v1.md"
  - "docs/plans/r2-orientation-handshake-performance-report-2026-04-26.md"
---

# R3 PageRank God Nodes Postprocess 需求

## Problem Frame

R2 已经把 orientation handshake 接通：bundle 读 HEAD、`orientation get` emits byte-stable thin block、harness 在 fixture 上 fresh / stale / no_graph / corrupt 四态可断言。但 orientation 里的 **navigation 信号仍是 raw fan-in**：

- `crates/context/src/protocol.rs` 与 `sexpr.rs`（R2 定义）在 orientation 里发 `top_referenced` —— 按 `cross-file inbound reference 边数 desc` 取 top-N。
- `crates/context/src/protocol.rs`（R2 reserved）发 `(god_nodes postprocess_unavailable)` / `(communities postprocess_unavailable)` / `(architecture_summary postprocess_unavailable)` 三个 typed gap 占位。
- pi-mono 端目前看到的 orientation 只有 raw 计数信号；没有"加权重要性"信号。

外部已验证的失败模式：**Aider hub-dominance bug**（GH#2405）—— agent 用 raw fan-in 当 navigation 信号时，外围 utility（`str_to_cstring`、`format_error`、日志宏一类）会因为被 50 个不重要模块调用而登顶；真正的架构枢纽（`Parser::parse`、`Tree::edit`、`Query::new`）被淹没。R2 的 `top_referenced` 字段，按设计就是裸 inbound 计数，**精确复制**这个失败模式。

R3 用最薄的一刀填掉**一个** R2 reserved 字段：把 `(god_nodes postprocess_unavailable)` 升级为 PageRank deterministic 变体计算出的 `(god_nodes (computation_status computed) ((rank 1) ...) ...)`。**不**触碰 R1 graph build/update CLI、**不**改 R2 orientation 字段排版、**不**升级 communities / architecture_summary（继续 `postprocess_unavailable`）、**不**实现 R3 agent-facing query primitive surface（`find-callers` / `safe_edit` / `should_reorient` 等）、**不**进入 pi-mono 上游产品 code path。

设计姿态延续 R0/R1/R2：合同分离 + 最小贯通 + reserved 占位为后续阶段留钩子。`top_referenced` 与 `god_nodes` 的语义边界由 R3 显式锁死 —— 前者是 **透明度信号**（who is called the most, raw count），后者是 **navigation 信号**（who is called by whom that matters, weighted propagation）。两个列表合同上并存、位置不动、值差异由算法本质决定。

---

## Actors

- A1. **R3 postprocess builder** — 新 `tree-sitter-context graph postprocess` 子命令；读 HEAD + 读 R1 snapshot → 跑 PageRank deterministic 变体 → 写 `.tree-sitter-context-mcp/postprocess/<snapshot_id>.json`。
- A2. **R2 orientation builder（升级路径）** — `orientation get` 在原有 thin stats / top_referenced / entry_points 之外，**在 R2 锁定的 `god_nodes` 字段位置**读 postprocess 产物（若存在且 schema 匹配）→ emit `(god_nodes (computation_status computed) ((rank N) ...) ...)`；不存在或不匹配 → emit `(god_nodes postprocess_unavailable)`（沿用 R2 wire）。
- A3. **R1 graph store + HEAD** — 已存在；R3 只读，不写、不改 build/update CLI。
- A4. **pi-mono harness consumer（升级路径）** — R2 已有的 `scripts/orientation-handshake-harness.mjs` 增加 postprocess 链路断言；不进 pi-mono submodule 上游。
- A5. **operator / CI** — 在 fixture repo 上跑端到端：`graph build` → `graph postprocess` → `orientation get`（god_nodes computed）→ `bundle (with id)` → 断言；`graph update` 后用旧 id → orientation get 读不到新 snapshot 的 postprocess → 断言 `postprocess_unavailable`。
- A6. **R3.1 / R4 future implementer** — 在本文档锁的 postprocess 接缝上实现 communities (Louvain / label propagation) 与 architecture_summary（轻量结构化总结）；扩展 `(computation_status ...)` 协议到 `stale` / `degraded` 等额外态。

---

## Key Flows

- F1. **Cold postprocess**
  - **Trigger:** operator / CI / harness 在 R1-build 完的 repo 上调 `tree-sitter-context graph postprocess`。
  - **Actors:** A5 → A1 → A3
  - **Steps:** 读 HEAD → 取 snapshot_id → 加载 R1 snapshot 的节点与 cross-file edges → 跑 PageRank deterministic 变体（30 iter / 1/N init / 0.85 damping / uniform edge weights）→ 取 top-20，按 `(-rank, path, stable_id)` 排序 → 写 `.tree-sitter-context-mcp/postprocess/<snapshot_id>.json`。
  - **Outcome:** 同 snapshot 重复执行 → `god_nodes` array 字节相等。
  - **Covered by:** R3.1, R3.2, R3.3, R3.5, R3.6

- F2. **Warm orientation get with computed god_nodes**
  - **Trigger:** harness 或未来 pi-mono session start 调 `orientation get`，且当前 HEAD 对应的 postprocess 产物存在。
  - **Actors:** A4 → A2 → A3
  - **Steps:** 读 HEAD → 读 R1 snapshot → 检查 `postprocess/<snapshot_id>.json` 存在且 `schema_version` 匹配 → 取出 `god_nodes` array → 在 R2 锁定字段位置 emit `(god_nodes (computation_status computed) ((rank 1) (stable_id ...) (path ...)) ...)`。
  - **Outcome:** orientation 字节流仍按 R2 排版顺序，但 `god_nodes` 不再是 `postprocess_unavailable` 占位。
  - **Covered by:** R3.4, R3.7, R3.8

- F3. **Postprocess unavailable / fallback**
  - **Trigger:** 当前 HEAD 没有对应 postprocess 产物（首次 build 后未跑 postprocess、或 update 后还没 re-postprocess、或 schema_version 不匹配、或文件损坏）。
  - **Actors:** A4 → A2
  - **Steps:** orientation get 检测 postprocess 缺失 / 不匹配 → emit `(god_nodes postprocess_unavailable)`，wire 与 R2 完全相同。其他字段（top_referenced / entry_points / stats）不受影响。
  - **Outcome:** R2 wire 合同不破；caller 看到的 god_nodes 形态等同 R2。
  - **Covered by:** R3.4, R3.9, R3.12

- F4. **Stale postprocess after graph update**
  - **Trigger:** 执行顺序为 `graph build` → `graph postprocess` → 修改 fixture → `graph update` → 立刻 `orientation get`。
  - **Actors:** A5 → A1 → A3 → A2
  - **Steps:** `graph update` 改 HEAD → 新 snapshot_id 没有 postprocess 产物 → orientation get 落回 F3 路径，emit `(god_nodes postprocess_unavailable)`。**旧** `postprocess/<old_id>.json` 不被自动删除，但也不被读取（按 snapshot_id 绑定，自然失效）。
  - **Outcome:** 旧产物残留是已知技术债，由 R3.13 vacuum policy 显式记账；orientation 行为正确。
  - **Covered by:** R3.9, R3.12, R3.13

- F5. **Idempotent postprocess (determinism gate)**
  - **Trigger:** 同 fixture 同 snapshot_id 跑 `graph postprocess` 两次。
  - **Actors:** A5 → A1
  - **Steps:** 第一次写 `postprocess/<id>.json`；第二次写同路径 → `god_nodes` array 字节完全相等。artifact 顶层包装的 `computed_at` / 其它非合同元数据可不同（artifact ≠ canonical surface，仅 `god_nodes` array 是合同）。
  - **Outcome:** PageRank 实现可被 contract test 强制 deterministic。
  - **Covered by:** R3.3, R3.6, R3.7

- F6. **R0 / R1 / R2 backward compatibility**
  - **Trigger:** 老调用方按 R0 v1 / R2 v1 调 bundle 与 orientation get，**不调** `graph postprocess`。
  - **Actors:** A4 → A2
  - **Steps:** `graph postprocess` 是新增独立子命令；不调它 → orientation 走 F3 fallback → `god_nodes` 仍为 `postprocess_unavailable`，与 R2 wire 完全相同；bundle 不读 postprocess 产物，Provenance 字段不变。
  - **Outcome:** R0 / R2 contract test 不破；新合同对老调用方零侵入。
  - **Covered by:** R3.10, R3.12

---

## Requirements Trace

| ID | Requirement | Rationale | Acceptance |
| --- | --- | --- | --- |
| R3.1 | 新增 `tree-sitter-context graph postprocess` 子命令；不接受 `--top-n` 等参数（R3 锁定 N=20）；接受 `[REPO_ROOT]` 位置参数与 `--quiet` 选项（与 R1 `graph build` 一致）；不得改 `graph build` / `graph update` / `graph status` / `graph verify` / `graph clean` / `bundle` / `orientation get` 任一既有 CLI 参数语义。 | postprocess 是独立阶段；R1 / R2 CLI 的延迟基准与 R12 性能门已 PASS，不主动击穿。 | AE1 |
| R3.2 | postprocess 产物路径：`.tree-sitter-context-mcp/postprocess/<snapshot_id>.json`（与 R1 graph store 同根目录、独立子目录）。文件 schema 顶层字段：`snapshot_id`（string，匹配当前 HEAD 的 XXH3 hex）、`schema_version`（string，固定字面量 `"r3-god-nodes-2026-04-26"`，编译期常量）、`computed_at`（unix 时间戳，**仅元数据**）、`god_nodes`（array of objects）。`god_nodes` 数组元素：`{rank: integer, stable_id: string, path: string}`，按 `rank` 升序排列。 | 按 snapshot_id 隔离产物 → 自然 freshness；schema_version 锁住 R3 wire；`computed_at` 仅人类可读、不入合同流。 | AE1, AE2, AE3 |
| R3.3 | PageRank 实现必须 deterministic-equivalent：固定迭代次数 30、初始分布 1/N（N = 节点数）、damping factor 0.85、uniform edge weights（call / import / ref / 任何 R1 既有 cross-file edge 等权处理，不区分类型）；不得使用浮点随机初始化、不得使用基于 wallclock 的早停、不得依赖哈希表迭代顺序。同 snapshot_id 跑两次 → `god_nodes` array 字节相等。 | byte-stable 是 R0 锁定的合同；deterministic-equivalent 让 contract test 可强制；R3 thin 不引入 edge-type 加权 tuning surface（留 R3.1 / R4 视真实信号需要再扩）。 | AE2, AE4 |
| R3.4 | orientation get 在 `god_nodes` 字段上的 wire 形态：postprocess 产物存在且 `schema_version` 匹配 → emit `(god_nodes (computation_status computed) ((rank 1) (stable_id "...") (path "...")) ((rank 2) ...) ...)`；产物不存在 / 损坏 / schema 不匹配 → emit `(god_nodes postprocess_unavailable)`，与 R2 wire 完全相同。`god_nodes` 字段在 orientation block 内的**位置**继续保持 R2 锁定排版（在 `entry_points` 之后，`communities` 之前），不得移动。 | R2.15 红线：reserved 字段位置不可移动；`(computation_status ...)` 子节点是新增内容、不是字段，符合"只允许新增"；postprocess_unavailable 裸符号保 R2 wire 兼容。 | AE3, AE5, AE10 |
| R3.5 | `god_nodes` top-N 默认且固定为 20；CLI 不暴露 `--god-nodes-top-n`；底层算法计算所有节点 PageRank，但 orientation / artifact 只保留前 20。`rank` 取整数 1..20。Tie-breaking：当 raw PageRank 浮点分数相等或差距小于 1e-12 时，按 `(path, stable_id)` 字典序破并；最终 rank 仍是连续整数 1..K（K ≤ 20）。 | 与 R2 `top_referenced` 默认对齐，方便 agent 横向比较；CLI 不暴露 N 减少 tuning surface；tie-break 锁死 byte-stable。 | AE2, AE6 |
| R3.6 | postprocess 产物里的 `god_nodes` array 字节相等是合同；`schema_version` 也是合同（编译期常量）；`snapshot_id` 也是合同（来自 HEAD）。`computed_at` **不是**合同（同一 snapshot 两次跑可不同）。orientation get 读 artifact 时**只**取 `god_nodes` array 写入 OrientationBlock，`computed_at` 与 `schema_version` 不进入 orientation canonical 流。 | 显式声明 artifact 分两层：合同核心（god_nodes）+ 元数据包装（computed_at）；orientation 是单一 source of truth for canonical surface。 | AE2, AE4, AE5 |
| R3.7 | postprocess 产物读取 helper 必须落在 `crates/context` 共享层；`graph postprocess`（写）与 `orientation get`（读）共用同一个产物路径与 schema 解析；helper 返回 typed enum：`Present(GodNodes)` / `Missing` / `Corrupt(reason)` / `SchemaMismatch(version)`。两边任何时刻读同一 snapshot_id 的产物 → 一致或都报同一种错误。 | 沿用 R2.7 helper discipline（HEAD 读 helper 共用），避免两个 binary 各 parse 一份带来漂移。 | AE5, AE7 |
| R3.8 | orientation get 读 postprocess 产物的语义遵循 R3.7 helper 输出：`Present` → emit computed god_nodes；`Missing` / `Corrupt` / `SchemaMismatch` → emit `(god_nodes postprocess_unavailable)` + stderr typed warning（`postprocess_corrupt` / `postprocess_schema_mismatch`），但**退出码不变**（orientation get 仍按现有契约退出）。 | god_nodes 不可用不能让 orientation 整体失败 —— 它是 reserved 字段的 graceful 退化路径；典型 R0 姿态。 | AE5, AE7, AE10 |
| R3.9 | `graph postprocess` 自身的退出码 / typed error：HEAD 缺失 → 退出非 0、stderr typed `no_graph`；HEAD corrupt → typed `graph_corrupt`；R1 snapshot 不可加载 → typed `snapshot_unreadable`；写产物时 IO 失败 → typed `postprocess_write_failed`。这些错误**不**覆写或截断已有产物。 | 沿用 R0 / R1 typed error 姿态；写失败不破坏已有产物（如旧 snapshot 的产物仍可被 orientation 读到，按其 snapshot_id 自然 stale）。 | AE7, AE8 |
| R3.10 | bundle 与 orientation get 在 R0 v1 / R2 v1 contract 的字段集合与字段语义**不变**；R3 仅在 `god_nodes` 字段值上把 `postprocess_unavailable` 升级为含 `(computation_status computed) ((rank N) ...) ...` 的列表。`top_referenced` 字段语义、排序键 `(-inbound_refs, path, stable_id)`、wire 形态全部保持 R2 v1 不动。 | R2.15 红线 + 合同分离条款：`top_referenced` 与 `god_nodes` 是两个独立信号，前者不因后者上线被隐藏 / 截断 / 标 deprecated。 | AE9, AE10 |
| R3.11 | `top_referenced`（裸 inbound count，透明度信号）与 `god_nodes`（PageRank 加权重要性，navigation 信号）的语义分离条款写入 R3 plan 与 r0-orientation-compaction-v2-contract（增量补丁，不重写 R2 既有内容）。两个列表的同时存在是设计决定：前者回答 "谁被调用得最多"，后者回答 "谁被最重要的节点调用"。R3 必须在 contract 文档里显式说明 agent 应将 `god_nodes` 视为首要 navigation 信号、`top_referenced` 视为辅助透明度信号。 | 文档约束 + `(computation_status computed)` 结构提示是组合姿态：纯文档对 agent 弱约束、纯结构对 agent 隐晦；两者叠加才让"该看 god_nodes" 既可读又可执行。 | AE9, AE11 |
| R3.12 | R3 不得：升级 `communities` / `architecture_summary` 字段（继续 `postprocess_unavailable`）；改 R1 graph build / update / status / verify / clean / diff CLI；改 R2 bundle / orientation get 既有 CLI 参数；引入 daemon / MCP server / N-API / WASM bridge；进入 pi-mono submodule 上游 code path；新增 `--top-n` / `--algorithm` / `--damping` 等 PageRank tuning CLI；自动 vacuum 旧 `postprocess/<old_id>.json` 文件；将 PageRank 引入 R1 build/update 路径。 | thin now / rich next 一贯姿态；任何 tuning CLI 都是合同扩张；vacuum 是 R3.1 议题（见 R3.13）。 | AE10, AE12 |
| R3.13 | R3 文档化但**不实现** vacuum policy：`postprocess/` 目录可能积累 stale `<old_id>.json` 文件（每次 graph update 后一份）。R3 plan 里记 `Outstanding for R3.1`：vacuum 触发时机（手动 `graph clean` 子命令扩展？后台周期？）、保留策略（最近 N 个 / 最近 T 时间）、与 R3.7 read helper 的交互。R3 阶段允许目录无界增长，由 operator 手动清理。 | YAGNI：vacuum 没有 carrying cost 直到 disk 真的爆；过早实现需要先决定保留策略，那是另一个产品决定。 | AE12 |
| R3.14 | harness 增加三条断言：(d) `graph build` → `graph postprocess` → `orientation get`（默认 sexpr）→ 必须包含 `(god_nodes (computation_status computed) ((rank 1) ...) ...)` 至少一个 rank 项；(e) 紧接其后 `bundle ... --orientation-snapshot-id <X>` → `orientation_freshness == "fresh"` 且 `graph_snapshot_id == X`（与 R2 既有断言叠加）；(f) 修改 fixture + `graph update` 后**不**调 `graph postprocess`，立刻 `orientation get` → 必须落回 `(god_nodes postprocess_unavailable)`。R2 既有 (a)(b)(c) 三条断言不动。 | 三条覆盖 cold computed 主路径、fresh 协议未破、stale postprocess 反向断言；harness 仍是 Node 内建模块、不依赖 pi-mono submodule。 | AE11, AE13 |
| R3.15 | R3 wire 协议向 R3.1 / R4 升级时**只允许新增字段或新增 `(computation_status ...)` 内的枚举成员（如 `stale` / `degraded`）**；不允许重命名 / 删除 / 移动现存字段，不允许改 `(rank N)` / `(stable_id ...)` / `(path ...)` / `(computation_status computed)` 任一既有形态。R3.1 给 communities / architecture_summary 升级时必须直接复用 `(computation_status ...)` 协议。 | R2.15 同形精神；R3 是 god_nodes thin→rich 第一锤，下游 R3.1 / R4 必须在已锁形态上扩展。 | AE14 |

---

## Acceptance Examples

- AE1. **Covers R3.1, R3.2.** fixture repo `graph build` 后调 `tree-sitter-context graph postprocess` → 退出码 0；`.tree-sitter-context-mcp/postprocess/<HEAD-snapshot-id>.json` 存在；JSON.parse 后 `schema_version == "r3-god-nodes-2026-04-26"`、`snapshot_id == HEAD-snapshot-id`、`god_nodes.length <= 20`、每个元素含 `rank` / `stable_id` / `path` 三键且 `rank` 为连续整数 1..K。
- AE2. **Covers R3.2, R3.3, R3.5.** 同 fixture / 同 snapshot 跑 `graph postprocess` 两次 → 两份 `<snapshot_id>.json` 的 `god_nodes` array 字节完全相等（`computed_at` 可不等）。
- AE3. **Covers R3.4, R3.6.** AE1 之后 `orientation get --format sexpr` → 输出在 R2 锁定 `god_nodes` 字段位置出现 `(god_nodes (computation_status computed) ((rank 1) (stable_id "...") (path "...")) ...)`，且该字段紧跟在 `(entry_points ...)` 之后、`(communities postprocess_unavailable)` 之前。
- AE4. **Covers R3.3.** contract test：`let run1 = run_postprocess(snapshot); let run2 = run_postprocess(snapshot); assert_eq!(run1.god_nodes, run2.god_nodes);` 必须 PASS。
- AE5. **Covers R3.4, R3.6, R3.8.** 删除 `<snapshot_id>.json` 后再 `orientation get` → 输出在 god_nodes 位置 emit `(god_nodes postprocess_unavailable)`，与 R2 wire 完全相同；退出码 0。
- AE6. **Covers R3.5.** fixture 中构造两节点 PageRank 浮点分数差 < 1e-12 的情况 → `god_nodes` array 中两节点按 `(path, stable_id)` 字典序排列，`rank` 仍为连续整数。
- AE7. **Covers R3.7, R3.8, R3.9.** 手工把 `<snapshot_id>.json` 内容改为非法 JSON → `orientation get` 在 god_nodes 位置 emit `(god_nodes postprocess_unavailable)` + stderr typed `postprocess_corrupt`，退出码 0；`graph postprocess` 重跑 → 覆盖原文件、退出码 0。
- AE8. **Covers R3.9.** 没 graph 的 repo 调 `graph postprocess` → 退出非 0、stderr typed `no_graph`；不创建 `postprocess/` 目录。
- AE9. **Covers R3.10, R3.11.** AE3 同次 orientation 输出中 `(top_referenced ...)` 字段值与 R2 v1 输出完全一致（同 fixture / 同 snapshot 对照 R2 sexpr fixture 字节相等）；两个列表中可能有同名符号（如某个 utility 同时出现在 top_referenced 和 god_nodes 之外，或某个核心同时上榜）—— 不去重、不交叉过滤。
- AE10. **Covers R3.4, R3.10, R3.12.** R0 v1 / R2 v1 contract test（`--budget`、`--max-tokens`、`--format`、`--tier`、`--stable-id`、退出码、字段集合、`{fresh|stale|unknown}` enum、`graph_snapshot_id` 真实 XXH3 / `no_graph` sentinel）完整跑通，无任何调整。
- AE11. **Covers R3.11, R3.14.** harness assertion (d)：`orientation get` 输出包含 `(god_nodes (computation_status computed) ` 字符串；assertion (e)：紧接其后 bundle 输出 `orientation_freshness == "fresh"`；assertion (f)：fixture 改动 + `graph update` 后不跑 postprocess → orientation 输出 god_nodes 落回 `postprocess_unavailable`。
- AE12. **Covers R3.12, R3.13.** 多次 `graph build` + `graph postprocess` + `graph update` + `graph postprocess` 后 `postprocess/` 目录会积累多个 `<id>.json` 文件 → R3 阶段断言不要求自动清理，operator 可手动 `rm`；orientation get 行为不受残留影响（按当前 HEAD snapshot_id 寻产物）。
- AE13. **Covers R3.14.** harness 源文件仍只 `import` Node 内建模块；不依赖 pi-mono 任一包；不修改 pi-mono submodule 任何文件。
- AE14. **Covers R3.15.** 模拟 R3.1 把 `communities` 由 `postprocess_unavailable` 换为 `(communities (computation_status computed) ...)` → 用 R3 完成时的 contract test 加载新输出 → 仅在"communities 必须为字符串 postprocess_unavailable"那条断言失败，god_nodes / top_referenced / entry_points / stats 等其它断言全部 PASS。

---

## Success Criteria

**Operator outcome**

- 在任意 R1-build 完的 fixture repo 上，`graph build → graph postprocess → orientation get` 三条命令拿到含真实 PageRank 排序的 god_nodes block；同 snapshot 重复 postprocess 字节相等。
- harness 端到端 (a)–(f) 六条断言（R2 三条 + R3 三条）作 CI gate，覆盖 fresh / stale / no_graph / corrupt / postprocess-computed / postprocess-stale 六个关键路径。

**Downstream handoff quality**

- R3.1 / R4 实施者只需把 `communities` / `architecture_summary` 字段值从 `postprocess_unavailable` 升级，并给各自加 `(computation_status computed) ...` 即可；不需要新发协议、不改 freshness enum、不改 god_nodes 形态。
- pi-mono 真正集成时把 harness 里 build → postprocess → orientation get 的调用直接搬进 session start 即可；产品 code path 与 prompt 注入位置由 pi-mono 自己决定。
- Auto-compact 替换、agent-facing query primitive surface、safe_edit、should_reorient 都是后续独立阶段；本阶段不预设它们，但 god_nodes 这一信号已经稳定可读。

**Aider hub-dominance bug 修复验证**

- 在 tree-sitter 自身仓上跑 R3 → `god_nodes` top-10 中应至少出现 `Parser::parse` / `Tree::edit` / `Query::new` 中的若干个；外围 utility（FFI 字符串转换、错误格式化、日志宏）不应在 top-5 出现。**这条不是契约**（不写进 acceptance），但作为 R3 真正解决问题的产品体感校验记入 plan 阶段的 dogfood 检查项。

---

## Scope Boundaries

- 不升级 `communities`（Louvain / label propagation）— R3.1 议题。
- 不升级 `architecture_summary`（轻量结构化总结 / NLP 摘要）— R4 议题。
- 不实现 R3 agent-facing query primitive surface（`find-callers` / `shortest_path` / `get_context_bundle` / `safe_edit` / `should_reorient` / `query_semantic_symbols` / `assert_callgraph` / `missing_symbols`）—— 那是 query API 层，本阶段只填一个 reserved 字段。
- 不替换 pi-mono `read/write/edit/bash/grep/find/ls`；不替换 pi-mono Auto-compact；不在 pi-mono 上游 code path 加任何代码。
- 不引入 daemon / stdio JSON-RPC / MCP server / N-API / WASM bridge；不让 `orientation get` 计算 postprocess（lazy 路径已在 brainstorm 中拒绝）。
- 不实现 Two-Corrections Rule、`should_reorient` meta primitive、exploration overlay、blast-radius graded invalidation、edit-aware degradation —— R5 议题。
- 不改 R0 v1 / R2 v1 任何字段集合、字段语义、enum 取值、CLI 参数；不 widening `orientation_freshness` 三态 enum；不改 R1 graph store schema。
- 不引入 PageRank tuning CLI（`--top-n` / `--damping` / `--iterations` / `--algorithm`）；R3 thin 锁死 N=20、damping 0.85、30 iter、uniform edge weights。
- 不实现自动 vacuum：`postprocess/<old_id>.json` 残留作为已知技术债记入 R3.1 议题。

### Deferred to Follow-Up

- communities 字段升级（R3.1）；architecture_summary 字段升级（R4）；两者均复用本文档锁定的 `(computation_status ...)` 协议。
- PageRank edge-type 加权（call > import > ref）—— 待在真实 codebase 上观察 uniform 权 god_nodes 是否足够区分 hub-dominance 失败模式后再评估。
- `score_percentile` (0-100) 整数字段 —— 若 agent 反馈"只有 rank 不够、需要差距感"再加；R3 thin 不发。
- vacuum policy（手动 `graph clean --postprocess` / 后台周期 / 保留最近 N 个）；R3.1 议题。
- agent-facing query primitive surface（R3 原 ideation 中的"Query primitive set"）—— 单独阶段，不与本文档绑定。
- pi-mono 上游产品集成（session start 注入、bundle 调用接 freshness、Auto-compact 替换）—— 由 pi-mono 维护方按 R0/R2/R3 锁定 contract 自行落地。
- daemon 决策：仅当 `graph postprocess` 在真实 workload 下击穿性能门时再评估。

---

## Key Decisions

- **R3 是单字段 thin→rich，不是 query API 层。** 用户已确定最小贯通 = 只升级 `god_nodes` 一个 reserved 字段。多升级一个就要多一份 algorithm 决定、一份 byte-stable 验证、一份 contract test 修订。
- **算法是 PageRank deterministic 变体，不是 weighted degree centrality。** weighted degree 仍是局部计数（只是边带权），无法解 Aider hub-dominance bug —— 外围 utility 被 50 个不重要模块调用仍可登顶；PageRank 的"重要调用者权重传播"才是 canonical fix。
- **Score 编码：整数 rank only，丢 raw float。** R2.3 byte-stable 禁止浮点；rank 完全足够 agent 区分 top-N；`score_percentile` 留 R3.1 视需要再加。
- **Postprocess 是独立子命令，不内置在 `graph build/update`。** R1 / R2 性能门已 PASS，不主动击穿；orientation get 保持只读语义。
- **Artifact 分两层：`god_nodes` array 是合同（byte-stable），wrapper 元数据（`computed_at`）不是合同。** orientation get 只读 god_nodes array，元数据不进 canonical 流。
- **`top_referenced` 与 `god_nodes` 语义分离 + 位置不动。** R2.15 锁死 `god_nodes` 在 R2 排版中的位置；`top_referenced` 不因 god_nodes 上线被隐藏 / 截断 / 标 deprecated。两者并存是设计决定，前者透明度信号、后者 navigation 信号。
- **`(computation_status computed)` 子节点是结构 priming。** 由于 R2.15 禁止重排，无法用顺序提示 agent 优先看 god_nodes；改用结构 —— `god_nodes` computed 时携带 status 子节点，`top_referenced` 不带，agent 视野里两个 top-N 列表的非对称结构成为信号。R3.1 / R4 把 communities / architecture_summary 升级时复用同一份 `(computation_status ...)` 协议，protocol 一次扩、四次复用。
- **不引入 PageRank tuning CLI。** `--top-n` / `--damping` / `--iterations` 都是合同扩张；R3 锁死 N=20 / 0.85 / 30 iter，等真实信号弱时再 R3.1 / R4 视情况扩。

---

## Risks & Dependencies

- **PageRank deterministic 实现 risk:** 浮点累加顺序、迭代收敛精度、节点遍历顺序都可能引入不确定性。R3 阶段必须显式锁：(a) 节点遍历按 `(path, stable_id)` 字典序；(b) 浮点累加按相同顺序；(c) 30 iter 不依赖收敛阈值；(d) 不使用并发计算或并发计算时 final reduce 按确定性顺序。建议 plan 阶段写 contract test 强制 `assert_eq!(run1, run2)` 防回归。
- **Edge enumeration honesty:** R1 snapshot 的 cross-file edges 类型分布在不同语言 tags 下不对称（Rust call edge 与 Python ref edge 语义不完全一致）。R3 thin 选择 uniform 权重避开这个分歧，但需要 plan 阶段确认 R1 实际 emit 哪些 edge kind 以及如何在 PageRank 输入中规范化（去重、自环处理、不可达节点处理）。
- **god_nodes wire 形态对 sexpr 解析器的兼容性 risk:** R0 sexpr canonical form 中 `(god_nodes postprocess_unavailable)` 是 `(symbol symbol)` 形态；R3 升级后是 `(symbol (subsymbol ...) (list ...) (list ...) ...)` 形态。下游 sexpr parser 必须接受两种形态，建议 plan 阶段 grep 全集所有 sexpr 解析点并加 fixture。
- **R3 wire 升级与 R0 plan 文档示例的同步:** R0 / R2 plan 文档中 god_nodes 示例输出全部是 `(god_nodes postprocess_unavailable)`；R3 上线后这些示例对应"未跑 postprocess" 状态仍然成立，**但** R3 plan 必须提供"已跑 postprocess" 状态的对照示例，避免将来阅读 R0/R2 doc 的人误以为 god_nodes 永远是 postprocess_unavailable。
- **`postprocess/<old_id>.json` 残留 disk 占用 risk:** 大型 monorepo 频繁 `graph update` 会让目录积累；R3 不实现 vacuum。Plan 阶段需评估单文件大小（top-20 god_nodes JSON 估 < 5KB）与频率，给出 operator 手动清理指引。
- **Tie-break 在小图上的 risk:** fixture repo（节点 < 20）会出现"全部上榜"情况，rank 1..K 时 K 可能小于 20。harness 断言必须接受 K < 20 的输出。
- **PageRank 在断连子图 / 自环 / 入度 0 节点上的行为 risk:** 经典 PageRank 在悬挂节点（无出边）需要 dangling-node redistribution；R3 实现必须显式选择策略（建议：把悬挂节点的概率均匀重分配到所有节点）并在 contract test 覆盖。
- **R3 plan 文档代价:** 现存 R0 / R2 sexpr fixture / contract test 中包含 `(god_nodes postprocess_unavailable)` 的所有断言点（grep 全集）必须在 R3 plan 阶段显式分类：哪些"postprocess 未跑" 仍走 R2 路径不动、哪些需要新增 R3 fixture。建议 plan 阶段 grep `postprocess_unavailable` 全集后明确分类。
- **Pi-mono fork 安全:** harness 留本 repo，`graph postprocess` 也只在本 repo 内 emit；任何文档若提到 pi-mono 集成必须强调 "fork only, never push to upstream"。

---

## Outstanding Questions

### Resolve Before Planning

（无。算法、接缝、产物形态、字段位置、合同分离、status 协议、scope 边界都已锁定。）

### Deferred to Planning

- [Affects R3.2, R3.7][Technical] postprocess 产物路径与 helper 落点：建议 `crates/context::postprocess` 新模块或扩展 `crates/context::store`；与 R1 既有 graph store 路径协议（`.tree-sitter-context-mcp/`）的目录组织。
- [Affects R3.3][Needs research] PageRank 实现选型：`petgraph` crate 自带 PageRank 但默认 float 收敛阈值；评估是否够用，否则 hand-roll deterministic 版本。
- [Affects R3.3][Technical] dangling-node / 自环 / 不可达节点 PageRank 处理策略；与 contract test fixture 一起锁。
- [Affects R3.4, R3.7][Technical] sexpr 解析器升级：R0 既有 sexpr parser 是否能直接处理 `(god_nodes (computation_status computed) ((rank N) ...) ...)` 嵌套结构，还是需要扩。grep `crates/context/src/sexpr.rs` 既有 god_nodes 解析点。
- [Affects R3.7, R3.8][Technical] postprocess read helper 的 typed error 命名（`Missing` / `Corrupt` / `SchemaMismatch` / `SnapshotMismatch`），与 R2 HEAD 读 helper 既有 typed error 命名对齐。
- [Affects R3.9][Technical] `graph postprocess` 写产物的原子性：先写临时文件再 rename 覆盖，避免并发读到半写文件；与 R1 graph build 的写入策略对齐。
- [Affects R3.10, R3.11][Technical] grep `postprocess_unavailable` / `top_referenced` 全集；R0 plan / cli-v1-contract / orientation-compaction-v2-contract / R2 plan 中需要同步更新或新增 R3 对照示例的位置。
- [Affects R3.13][Technical] vacuum policy 设计的初步评估：是否在 R3.1 之前需要至少一个 manual `graph clean --postprocess` 子命令；对 plan 阶段 disk 估算依赖。
- [Affects R3.14][Technical] harness 三条新增断言的 fixture 准备：是否复用 R2 fixture，还是需要新增节点数 ≥ 20 的 fixture 让 god_nodes 体现完整 top-N。

---

## Next Steps

`-> /ce-plan` 把 R3 拆成实施计划。建议路径：

1. 先做 postprocess read helper 与 typed error（`crates/context` 共享层）+ orientation get 在 god_nodes 字段上的双形态 emit（`postprocess_unavailable` vs `(computation_status computed) ...`）；这一步**不需要** PageRank 实现，可用空 god_nodes array fixture 先验证 wire。
2. 实现 PageRank deterministic 变体（30 iter / 1/N init / 0.85 damping / uniform edge weights / dangling redistribute），在 `crates/context::postprocess` 模块；contract test `assert_eq!(run1.god_nodes, run2.god_nodes)` 强制。
3. 实现 `tree-sitter-context graph postprocess` CLI 子命令；写 `.tree-sitter-context-mcp/postprocess/<snapshot_id>.json`；产物原子写入。
4. orientation get 接通 postprocess read helper：present → emit computed god_nodes；missing/corrupt/schema_mismatch → emit `postprocess_unavailable` + stderr typed warning。
5. 改 R0 / R2 sexpr fixture：grep `postprocess_unavailable` 全集 → 分类为 "R2 行为不变" 与 "R3 新增对照示例"；不重写 R0/R2 既有断言。
6. 扩展 `scripts/orientation-handshake-harness.mjs` 增加三条断言（cold computed / fresh after postprocess / stale postprocess after update）。
7. 性能复测：cold `graph postprocess` 在 R12 性能门以内（按 R1/R2 报告基线扩展 R3 评估），否则触发 daemon 决策。
8. **Dogfood 检查**：在 tree-sitter 自身仓跑 R3 → 人工对比 god_nodes top-10 是否包含 `Parser::parse` / `Tree::edit` / `Query::new`，外围 FFI / 错误 utility 是否被压下。不写进 acceptance test，但作为 plan 阶段 PR 描述里的产品体感校验记录。

communities (R3.1) / architecture_summary (R4) / agent-facing query primitive surface / pi-mono 上游集成 / Auto-compact 替换都不在本计划。
