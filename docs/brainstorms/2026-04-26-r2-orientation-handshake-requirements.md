---
title: "R2 v2 Thin Orientation Handshake Requirements"
type: requirements
status: draft
date: 2026-04-26
origin:
  - "docs/ideation/2026-04-26-tree-sitter-pi-integration-ideation.md#1-ast-aware-read-tool-with-incremental-invalidation"
  - "docs/ideation/2026-04-26-tree-sitter-repo-navigation-ideation.md#r0-agent-interface-contract--context-firewall"
  - "docs/brainstorms/2026-04-26-r0-context-firewall-requirements.md"
  - "docs/brainstorms/2026-04-26-r1-repo-map-requirements.md"
dependencies:
  - "docs/plans/2026-04-26-001-feat-r0-context-firewall-plan.md"
  - "docs/plans/2026-04-26-002-feat-r1-repo-map-plan.md"
  - "docs/plans/tree-sitter-context-cli-v1-contract.md"
  - "docs/plans/tree-sitter-context-graph-r1-contract.md"
  - "docs/plans/r0-orientation-compaction-v2-contract.md"
  - "docs/plans/sexpr-canonical-form-v1.md"
  - "docs/plans/r1-repo-map-performance-report-2026-04-26.md"
---

# R2 v2 Thin Orientation Handshake 需求

## Problem Frame

R0 与 R1 已分别完成，但两者**没有握手**。三处现实证据：

- `crates/context/src/protocol.rs:41-52`：`Provenance` 的 `graph_snapshot_id` 与 `orientation_freshness` 默认硬编码 `"unknown"`。
- `crates/context/src/sexpr.rs:407-408`：R0 sexpr contract test 硬断言两个字段等于 `"unknown"`。
- `crates/cli/src/bin/tree-sitter-context.rs`（bundle 二进制）：完全不读 `.tree-sitter-context-mcp/HEAD`；HEAD 写入路径仅在 `crates/cli/src/context_graph.rs:203,430`（R1）侧存在，bundle 与 graph 之间没有读路径。
- pi-mono 端没有任何机制消费 orientation block。

R2 用最薄的一刀贯通这三处：让 bundle 读 HEAD、新增 `tree-sitter-context orientation get` 子命令、用本 repo 内 harness 模拟 pi-mono 调用。**不**替换 pi-mono 工具面、**不**替换 Auto-compact、**不**实现 R3 query primitives、**不**把 postprocess (Louvain / PageRank / centrality) 引进来。orientation block 这一阶段只装 deterministic stats（snapshot id、文件/符号/语言计数、按裸 cross-file inbound reference 边数排的 top-N、tags 派生的 entry points）；postprocess 依赖的字段 (`god_nodes` / `communities` / `architecture_summary`) 全部以 `postprocess_unavailable` 占位，作为下一阶段（R3 thin → rich）的钩子。

pi-mono 侧定义为**验收 harness，非产品交付**：本 repo 内一个最小 Node 脚本调 CLI、parse、断言关键字段；不进 pi-mono submodule 上游 code path，从而保留两侧的回退安全。

---

## Actors

- A1. **R2 orientation builder** — 新 `tree-sitter-context orientation get [--budget N] [--format sexpr|json]` 子命令；读 HEAD、加载 R1 snapshot、生成确定性 orientation、按 budget 截断、按 R0 canonical 风格序列化。
- A2. **R2 bundle freshness wiring** — 修改 bundle 二进制：读 HEAD 把真实 `graph_snapshot_id` 写入 Provenance；接收可选 `--orientation-snapshot-id <id>`，与 HEAD 当前 id 比对，给 `orientation_freshness` 赋 `fresh` / `stale` / `unknown`。
- A3. **R1 graph store + HEAD** — 已存在；R2 只读，不写。
- A4. **pi-mono harness consumer** — 本 repo 内 Node 脚本（`scripts/` 或 `docs/verification/`），模拟 pi-mono session start；不进 pi-mono 上游。
- A5. **operator / CI** — 在 fixture repo 上跑端到端：build → orientation get → bundle (with id) → 断言；fixture 改动 + update + bundle (旧 id) → 断言 stale。
- A6. **R3 future implementer** — 在 R2 锁的 orientation contract / freshness 协议之上把 reserved 字段从 `postprocess_unavailable` 升级为真实值。

---

## Key Flows

- F1. **Cold orientation get**
  - **Trigger:** harness 或未来 pi-mono session start 调 `tree-sitter-context orientation get --budget 2000`。
  - **Actors:** A4 → A1 → A3
  - **Steps:** 解析 repo root → 读 HEAD → 加载 snapshot → 计算 thin stats → 把 god_nodes / communities / architecture_summary 标 `postprocess_unavailable` → canonical 排序 → 按 budget 截断 → emit (默认 sexpr)。
  - **Outcome:** byte-stable orientation block，含真实 `graph_snapshot_id`、deterministic stats 与 reserved postprocess fields。
  - **Covered by:** R2.1, R2.2, R2.3, R2.4, R2.7

- F2. **Bundle with freshness**
  - **Trigger:** 拿到 orientation 之后调 bundle，传 `--orientation-snapshot-id`。
  - **Actors:** A4 → A2 → A3
  - **Steps:** bundle 读 HEAD 当前 id → 写 `Provenance.graph_snapshot_id` → 与 caller's id 比对 → 写 `orientation_freshness`。
  - **Outcome:** Provenance 字段不再 "unknown"；caller 知道手中 orientation 是 fresh / stale。
  - **Covered by:** R2.5, R2.6, R2.8

- F3. **HEAD-missing graceful**
  - **Trigger:** graph 未 build 的 repo。
  - **Actors:** A1/A2 → A3
  - **Steps:** orientation get 探测 HEAD 缺失 → 退出码非 0、stderr typed `no_graph`；bundle 仍执行单文件 path+stable_id 路径，`graph_snapshot_id = "no_graph"`，`orientation_freshness = "unknown"`（沿用 R0 enum 中 unknown 的"无法判定"语义）。
  - **Outcome:** 调用方能从 `graph_snapshot_id` 区分"没 graph"与"有 graph 但旧"。
  - **Covered by:** R2.9, R2.10

- F4. **Harness verification**
  - **Trigger:** CI / operator 在 fixture 上跑 harness。
  - **Actors:** A5 → A4 → A1/A2
  - **Steps:** `graph build` → `orientation get --format json` → JSON.parse + schema 断言 → `bundle ... --orientation-snapshot-id X` → 断言 `graph_snapshot_id != "unknown"` 且 `orientation_freshness == "fresh"`；改动 fixture + `graph update` → 用旧 X 调 bundle → 断言 `orientation_freshness == "stale"`。
  - **Outcome:** 端到端契约可作 CI gate。
  - **Covered by:** R2.11, R2.12

- F5. **R0 / R1 backward compatibility**
  - **Trigger:** 老调用方继续按 R0 v1 合同调 bundle，不传 `--orientation-snapshot-id`。
  - **Actors:** A4 → A2
  - **Steps:** bundle 仍写 HEAD 真实 id 到 `graph_snapshot_id`；`orientation_freshness = "unknown"`（caller 没传 id，不可判定）。
  - **Outcome:** R0 v1 调用形态不破；`{fresh|stale|unknown}` 三态 enum 不被 R2 widening。
  - **Covered by:** R2.13, R2.14

---

## Requirements Trace

| ID | Requirement | Rationale | Acceptance |
| --- | --- | --- | --- |
| R2.1 | 新增 `tree-sitter-context orientation get` 子命令；接受 `--budget <TOKENS>`、`--format {sexpr,json}`，default `sexpr` 与 R0 bundle 一致；不得改 `bundle`、`graph` 已有 CLI 参数语义。 | sexpr 是 R0 锁定的 prompt-cache-friendly 默认；harness 用 json 调试。 | AE1, AE2 |
| R2.2 | orientation 输出至少包含：`graph_snapshot_id`、`schema_version`、`stats`（file_count / symbol_count / language_count / edge_count）、`top_referenced`（按 cross-file inbound reference 边数 desc 取 top-N）、`entry_points`（tags 中标 public 且本文件无定义入边的符号）、`god_nodes` / `communities` / `architecture_summary` 三个 reserved postprocess 字段。所有 reserved 字段值固定为 `postprocess_unavailable`。 | thin/rich 分阶段；reserved 字段是 R3 钩子，不能伪造空数组。 | AE2, AE3 |
| R2.3 | orientation 输出必须 byte-stable：相同 graph_snapshot_id + 相同 budget + 相同 format → 字节完全相同。排序键基于 (repo-relative path, symbol_path, stable_id)；不引入时间戳、随机 map 顺序、绝对路径、floating-point。 | Anthropic prompt-cache 命中前提；与 R1.7 同 discipline。 | AE2 |
| R2.4 | budget 不足时按声明优先级截断（先 stats，再 top_referenced，再 entry_points），并显式输出 `(budget_truncated true reason "budget_exhausted" omitted [...])`，不静默丢弃。estimated_tokens 不得 cap 在 budget。 | budget 诚实是 R0 锁定的契约（branch review 学习项）。 | AE3 |
| R2.5 | bundle 必须读 `.tree-sitter-context-mcp/HEAD`，把当前 snapshot id 写入 `Provenance.graph_snapshot_id`；HEAD 不存在时写 `"no_graph"`，不再写 `"unknown"`。 | R0 reserved 字段必须落到真实数据源；用专门 sentinel 区分"没 graph"与 R0 enum 中的 unknown。 | AE4, AE5 |
| R2.6 | bundle 接受可选 `--orientation-snapshot-id <id>`：提供且与 HEAD 相等 → `fresh`；提供且不等 → `stale`；未提供或 HEAD 缺失 → `unknown`。**`orientation_freshness` 取值集合冻结为 R0 已锁的 `{fresh, stale, unknown}` 三态，不得 widening。** | R0 brainstorm 已锁定 enum；下游 enum 解析不能在 R2 后 break。 | AE4, AE5, AE6 |
| R2.7 | orientation get 与 bundle 必须共用同一个 HEAD 读 helper（落在 `crates/context` 共享层）；helper 返回 typed enum：`Present(snapshot_id)` / `Missing` / `Corrupt(reason)` / `SchemaMismatch(version)`。两边任何时刻读到的 HEAD 一致或都报同一种错误。 | freshness 端到端正确性的前提是两边看到同一现实；不能两个 binary 各 parse 一次出现漂移。 | AE4, AE7 |
| R2.8 | bundle 输出 schema 不得新增**必填**字段、不得改 v1 字段语义；仅允许把现有字段值从字符串字面量 `"unknown"` 升级为：(a) `graph_snapshot_id` 的真实 XXH3 或 `"no_graph"` sentinel；(b) `orientation_freshness` 的 `fresh` / `stale` / `unknown`。 | R0 v1 contract test 必须继续通过；新参数 `--orientation-snapshot-id` 是可选，省略时退化为现有行为。 | AE6, AE13 |
| R2.9 | graph 未 build 的 repo 上跑 orientation get → 退出码非 0、stderr 输出 typed `no_graph`（含建议 `tree-sitter-context graph build`），不写半成品 stdout；同 repo bundle 仍可执行（向下兼容 R0 v1 单文件路径），仅 Provenance 标 `no_graph` / `unknown`。 | typed error 是 R0 锁定姿态；bundle 不应因 graph 缺失而拒绝服务。 | AE5 |
| R2.10 | HEAD 文件存在但损坏 / schema 不匹配 → orientation get 与 bundle 都返回 typed `graph_corrupt` / `schema_mismatch` 错误码，HEAD 文件不被两者覆写或截断。 | R1 已锁的 graph 健壮性姿态。 | AE7 |
| R2.11 | 仓内提供 harness 脚本，位置 `scripts/orientation-handshake-harness.mjs`；用 Node ≥18 内建模块（`child_process` / `fs` / `path` / `node:assert`），**不依赖 pi-mono 任一包，不修改 pi-mono submodule 任何文件**；脚本退出码作为 CI gate。 | 用户明确：pi-mono 侧只做 harness 验证；保留两侧产品代码回退安全。 | AE8, AE9 |
| R2.12 | harness 至少覆盖三条断言：(a) `orientation get --format json` 输出 schema 含 R2.2 全字段且 graph_snapshot_id 非空且 ≠ `"unknown"` 且 ≠ `"no_graph"`；(b) bundle (with orientation id) 返回 `orientation_freshness == "fresh"`；(c) 修改 fixture + graph update 后用旧 orientation id 调 bundle → 返回 `orientation_freshness == "stale"`。 | 三条对应"接通"、"freshness 正向"、"freshness 反向"，缺一不算端到端。 | AE8, AE10, AE11 |
| R2.13 | `crates/context/src/sexpr.rs:407-408` 等所有对 `"unknown"` 字符串字面量的硬断言必须在 R2 内被改写：`graph_snapshot_id` 改成"非空 + 非 unknown 字面量 + 匹配 XXH3 或 no_graph 格式"；`orientation_freshness` 改成枚举成员断言。所有 R0 plan / contract 文档中示例输出含 "unknown" 的位置必须同步更新。 | 引入真实值后两套语义不能共存；示例和实际输出必须一致。 | AE12 |
| R2.14 | R2 不得：替换 pi-mono `read/write/edit/bash/grep/find/ls`；替换 pi-mono Auto-compact；实现 R3 query primitives；启动 daemon / MCP server；把 postprocess (Louvain / PageRank) 引入 R1 或 R2；把 orientation block 自动注入 pi-mono 产品 system prompt；新增 `--orientation-snapshot-id` 之外的 bundle 必填参数。 | 用户明确：最小可信 = 只接线；其他都不做。 | AE13 |
| R2.15 | orientation 输出在 R3 thin→rich 升级时**只允许新增字段、不允许重命名/删除/移动现有字段**：当前 reserved-postprocess 字段一旦换为真实值，键名与位置必须保持。 | R2 是握手协议；下游绝对不能因 R3 升级而 break。 | AE14 |

---

## Acceptance Examples

- AE1. **Covers R2.1, R2.2.** fixture repo 跑 `graph build` 后 `orientation get --budget 2000 --format sexpr` 返回非空 sexpr，含 `(graph_snapshot_id "<XXH3 hex>")`、`(stats ...)`、`(top_referenced ...)`、`(entry_points ...)`、`(god_nodes postprocess_unavailable)`。
- AE2. **Covers R2.2, R2.3.** 同 fixture / 同 snapshot / 同 budget / 同 format 跑两次 orientation get → 字节完全相同；改一个文件再 graph update 后，graph_snapshot_id 与 stats 改变，但其他字段保持 canonical 排序。
- AE3. **Covers R2.2, R2.4.** budget 设为 100 tokens（远小于自然输出），orientation 按声明优先级保留 stats、丢弃 top_referenced/entry_points 后部，并显式带 `(budget_truncated true reason "budget_exhausted" omitted [...])`。
- AE4. **Covers R2.5, R2.6, R2.7.** graph build 后 orientation get → 拿到 id X；立刻 `bundle <path> --stable-id <id> --orientation-snapshot-id X` → `graph_snapshot_id = X` 且 `orientation_freshness = fresh`。
- AE5. **Covers R2.5, R2.6, R2.9.** 没 graph 的 repo 调 bundle（不传 orientation id）→ `graph_snapshot_id = "no_graph"`、`orientation_freshness = "unknown"`、退出码 0；同 repo 调 orientation get → 退出码非 0、stderr typed `no_graph`。
- AE6. **Covers R2.6, R2.8.** graph build 后改 fixture 再 graph update → 用旧 X 调 bundle → `orientation_freshness = stale`，`graph_snapshot_id` = HEAD 当前真实值（≠X）。
- AE7. **Covers R2.7, R2.10.** 手工破坏 `.tree-sitter-context-mcp/HEAD`（写不可解析字符串）→ 调 bundle 与 orientation get 都返回 typed `graph_corrupt`；HEAD 文件未被两者覆写或截断。
- AE8. **Covers R2.11, R2.12.** harness 在 fixture 完整跑完三条断言并退出 0；任一断言失败 → 退出非 0 + stderr 指向具体失败行。
- AE9. **Covers R2.11, R2.14.** harness 源文件 `import` / `require` 仅引用 Node 内建模块；不依赖 pi-mono 任一包；不修改 pi-mono submodule 任何文件。
- AE10. **Covers R2.12.** harness 第二条断言（fresh）必须在 graph build 与 bundle 调用之间不调 graph update 且 fixture 文件未变；任何中间变化让此断言变成 stale，验证 harness 严谨性。
- AE11. **Covers R2.12.** harness 第三条断言（stale）必须显式调 graph update 且对比"修改前的 orientation id"；忘记修改 fixture 或忘记 graph update → 断言主动失败而不是默认 pass。
- AE12. **Covers R2.13.** `crates/context/src/sexpr.rs` 中硬编码 `"unknown"` 断言被改写为枚举成员断言（如 `assert!(matches!(parsed.orientation_freshness.as_str(), "fresh"|"stale"|"unknown"))` + `assert!(parsed.graph_snapshot_id != "unknown")`）；R0 plan 文档中"在 v1 均为 unknown"等示例改为"R2 后为真实 id 或 no_graph"。
- AE13. **Covers R2.8, R2.14.** R0 v1 contract test（`--budget`、`--max-tokens`、`--format`、`--tier`、`--stable-id`、退出码、字段集合）完整跑通；pi-mono 现有工具集与 Auto-compact 行为完全未变；harness 不进 pi-mono code path。
- AE14. **Covers R2.15.** 模拟 R3 把 god_nodes 由 `postprocess_unavailable` 换为真实数组 → 用 R2 完成时的 contract test 加载新输出 → 测试只在"god_nodes 必须为字符串 postprocess_unavailable"那条断言失败，其他全部通过。

---

## Success Criteria

**Operator outcome**

- 在任意已 R1-build 过的 fixture repo 上，一条命令拿到 byte-stable orientation block；一条 bundle 调用拿到带真实 graph_snapshot_id / typed freshness 的 Provenance。
- harness 作为 CI gate 能稳定捕到 fresh / stale / no_graph / corrupt 四种关键路径。

**Downstream handoff quality**

- R3 实施者只需把 reserved postprocess 字段从 `postprocess_unavailable` 换成真实值；不需要新发协议、不改 freshness enum。
- pi-mono 真正集成时把 harness 里那段调用直接搬进 session start 即可；产品 code path 与 prompt 注入位置由 pi-mono 自己决定，contract 已稳定。
- Auto-compact 替换、tool surface 替换是后续独立阶段；本阶段不预设它们的实现路径，但 reserved 字段已为它们留好位置。

---

## Scope Boundaries

- 不替换 pi-mono `read/write/edit/bash/grep/find/ls`；不引入 R3 工具（`safe_edit` / `find-callers` / `get_ranked_architecture` / `shortest_path` / `impact-analysis` / `query_semantic_symbols` 等）。
- 不替换 pi-mono Auto-compact / 五层 compaction pipeline；不实现 graph-aware compaction runtime；不调 LLM summary fallback。
- 不在 pi-mono submodule 产品代码里加 orientation 注入、freshness 检查或新工具；仅本 repo 内 harness 脚本是允许的"pi-mono 侧"代码。
- 不实现 god_nodes (PageRank / centrality)、communities (Louvain)、architecture_summary 计算；R2 中以 `postprocess_unavailable` 占位。
- 不引入 daemon / stdio JSON-RPC / MCP server / N-API / WASM bridge。
- 不实现 Two-Corrections Rule、`should_reorient` meta primitive、exploration overlay、blast-radius graded invalidation。
- 不改 R0 v1 bundle 必填参数集合或必有输出字段集合；不 widening `orientation_freshness` 三态 enum。
- 不改 R1 graph build / update / diff / status / verify / clean 的 CLI 参数与 snapshot manifest 字段；R2 只读不写 graph store。

### Deferred to Follow-Up

- Postprocess 计算（Louvain / PageRank / community / god_nodes / architecture summary）—— R3 thin→rich 单独一阶段，复用 R2 reserved 字段位置。
- pi-mono 上游产品集成（session start 注入、bundle 调用接 freshness、回退策略）—— 由 pi-mono 维护方按 R2 锁定的 contract 自行落地。
- R3 agent-facing query primitives；Auto-compact 替换；S-expression 化整套 pi-mono tool result；exploration overlay；blast-radius graded invalidation；Two-Corrections automation。
- daemon 决策：仅当 cold orientation get + bundle freshness 在真实 workload 下击穿 R12 性能门时再评估。

---

## Key Decisions

- **R2 是握手协议，不是工具集。** 用户已确定最小可信 = 只接线。多接一个工具就要多一份契约、一份回退债。
- **orientation 默认 sexpr，json 是 escape hatch。** R0 已锁 sexpr 为 prompt-cache-friendly canonical 形态；harness 用 json 仅为调试便利。
- **`graph_snapshot_id` 与 `orientation_freshness` 的 unknown 语义被精确分裂。** 旧的"两者都 unknown"会让 caller 无法分支：是没 graph？还是没传 id？R2 解法 —— `graph_snapshot_id` 退役 `"unknown"`，改为真实 XXH3 或 `"no_graph"` sentinel；`orientation_freshness` 保留 R0 三态 enum 不动，`unknown` 沿用为"无法判定"伞值（caller 用 `graph_snapshot_id` 区分原因）。
- **postprocess 不进 R2。** 用户明确 thin now / rich next；R1.11 已锁 `postprocess_unavailable` 作为下游可识别的 typed gap。
- **harness 留本 repo，不进 pi-mono submodule。** 用户原话："动了就是动了，但只在测试路径里，随时可以拔掉"。
- **bundle 与 orientation get 共用 HEAD 读 helper。** freshness 端到端正确性的前提是两边看到同一现实。helper 落在 `crates/context` 共享层而非各 binary 自查。

---

## Risks & Dependencies

- **HEAD 一致性 race:** orientation get 与 bundle 之间，graph update 可能并发发生；R2 freshness 协议依赖 caller 自留 id 与下次比对，但若两个进程交错，存在 "orientation 还没读完就被 update 覆盖 HEAD" 的窗口。`graph_corrupt` typed error 是底线；细粒度 race 防护（reader-side advisory lock）建议在 planning 阶段评估。
- **byte-stable 编码 risk:** 任何 floating-point 进入 orientation 都会破坏 cache 命中；R2 必须强制"top_referenced 排序键里只允许整数或 byte string"；建议 planning 阶段写 lint 或 contract test 防回归。
- **R0 contract test / 文档 改写代价:** 现存对 `"unknown"` 字面量的硬断言不止 sexpr.rs:407-408；planning 阶段必须 grep 全集（包括 R0 plan / cli-v1-contract / r0-orientation-compaction-v2-contract）一并改写。
- **Tags entry-point heuristic risk:** "public 且本文件无定义入边" 在不同语言 tags scheme 下不对称（Rust 的 `pub` vs Python 下划线约定）；R2 应只用 tags 中已有 capture 名，遇到不支持语言时返回空 list 并标 `unsupported`，不猜。
- **Budget tokenizer 选择:** R0 已有 estimated_tokens 计算；R2 `--budget` 必须复用同一 tokenizer，否则 prompt-cache key 漂移；planning 阶段需 lock 共享路径。
- **Pi-mono fork 安全:** harness 留本 repo，但文档/示例若提到 pi-mono 文件路径必须强调 "fork only, never push to upstream"，沿用 CLAUDE.md remote safety 规则。
- **`no_graph` sentinel 与 XXH3 命名空间冲突:** 必须保证 `"no_graph"` 不在合法 XXH3 hex 字符集内（XXH3 是 hex digits，"no_graph" 含下划线，安全），但 contract 中需显式声明 sentinel 不与任何合法 id 冲突。

---

## Outstanding Questions

### Resolve Before Planning

（无。范围、深度、边界、验收形态、enum 扩展策略都已锁定。）

### Deferred to Planning

- [Affects R2.1, R2.2][Technical] orientation 字段的 sexpr / json schema 精确形态：`top_referenced` 列表元素结构、`stats` 子结构、`schema_version` 取值规则、`postprocess_unavailable` 是裸符号还是带 reason 子结构。
- [Affects R2.5, R2.7][Technical] HEAD 读 helper 的 module 落点（建议 `crates/context::store` 或新 `crates/context::head`）；与 R1 既有 HEAD 写路径合并的代码组织。
- [Affects R2.7, R2.10][Technical] HEAD 读 typed error 的命名（`Missing` / `Corrupt` / `SchemaMismatch`），与 R1 status / verify 既有 typed error 命名对齐。
- [Affects R2.4][Technical] budget 截断各部分的优先级是否需要可配置（`--include` 白名单），还是 R2 固定为 stats > top_referenced > entry_points。
- [Affects R2.11][Technical] harness 脚本最终路径与 CI 集成方式（独立 GitHub Action job 还是塞进现有 Rust integration test 体系）。
- [Affects R2.13][Technical] grep `"unknown"` 全集；R0 plan / cli-v1-contract / orientation-compaction-v2-contract 中需要同步更新的示例输出位置。
- [Affects R2.15][Needs research] R3 thin→rich 升级时是否需要新增**必填**字段；如果会，R2 是否需要预留更多 reserved key 避免下次 break。

---

## Next Steps

`-> /ce-plan` 把 R2 拆成实施计划。建议路径：

1. 先做 HEAD 读 helper（`crates/context` 共享层）+ bundle freshness 接通（最小贯通；只动 `graph_snapshot_id` / `orientation_freshness` 两个字段语义）。
2. 改 R0 sexpr / contract 文档中所有 `"unknown"` 硬断言与示例。
3. 实现 `orientation get` 子命令（thin stats + reserved postprocess + budget + sexpr/json）。
4. 写 `scripts/orientation-handshake-harness.mjs` 与 fixture，端到端三条断言作 CI gate。
5. 性能复测：cold orientation get + bundle 仍需保 R12 门限，否则触发 daemon 决策的 R1 历史决议。

postprocess、pi-mono 上游集成、R3 工具集都不在本计划。
