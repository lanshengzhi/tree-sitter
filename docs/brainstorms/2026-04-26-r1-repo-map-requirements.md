---
title: "R1 First-Class Repo Map via Symbol Graph Requirements"
type: requirements
status: draft
date: 2026-04-26
origin:
  - "docs/ideation/2026-04-26-tree-sitter-pi-integration-ideation.md#4-first-class-repo-map-via-symbol-graph"
  - "docs/brainstorms/2026-04-26-r0-context-firewall-requirements.md"
dependencies:
  - "docs/plans/2026-04-26-001-feat-r0-context-firewall-plan.md"
  - "docs/plans/tree-sitter-context-cli-v1-contract.md"
  - "docs/plans/sexpr-canonical-form-v1.md"
  - "docs/plans/r0-orientation-compaction-v2-contract.md"
  - "docs/plans/r0-context-firewall-performance-report-2026-04-26.md"
  - "docs/solutions/workflow-issues/tree-sitter-context-branch-review-2026-04-25.md"
---

# R1 First-Class Repo Map via Symbol Graph 需求

## Problem Frame

R0 v1 已完成并证明了最小 agent-facing 切片：`tree-sitter-context bundle <path> --stable-id <id> --tier sig --format sexpr` 通过 `path + stable_id` 做单文件定位，输出 canonical S-expression，并在 Provenance 中保留 `graph_snapshot_id` 与 `orientation_freshness` 字段。但 R0 只能返回 `"unknown"`，因为 repo graph 尚不存在。

R1 的目标是把 ideation #4 的 "First-Class Repo Map via Symbol Graph" 落成可构建、可更新、可 diff、可被后续 R2/R3/v2 消费的基础设施。R1 不是新的 agent 查询工具集；它是 graph substrate：用现有 `tree-sitter-loader` 做语言发现、用现有 tags 查询作为首选符号来源，构建跨文件定义/引用/导入关系，并为每次 graph 状态生成确定性的 `graph_snapshot_id`。

R1 必须把 R0 锁住的未来字段变成真实上游数据源，同时保持 R0 v1 CLI 合同不变。R0 性能报告的 R12 gate 当前为 PASS，subprocess 路径 p95 远低于 100ms，因此 R1 不默认引入 daemon。

---

## Actors

- A1. **R1 graph builder / updater** — 扫描 repo、解析支持文件、生成 graph snapshot、维护 `.tree-sitter-context-mcp/HEAD`。
- A2. **tree-sitter loader + tags 基础设施** — 提供语言发现、grammar 加载、`tags.scm` / `locals.scm` 查询，R1 优先复用而不是重写 parser/query 层。
- A3. **Graph store** — 持久化节点、边、文件内容 hash、snapshot manifest、diff 所需索引和 graph schema version。
- A4. **R0 / pi-mono bridge consumer** — 继续按 R0 v1 合同调用 `bundle`；未来 v2 启动时读取 HEAD 生成 orientation，工具结果用 snapshot id 判断 freshness。
- A5. **R2/R3 实施者** — R2 在 graph 节点上扩展 AstCell / Provenance，R3 在 graph 之上暴露 find-callers、defs、impact 等查询原语。
- A6. **operator / CI** — 在本地和 CI 中触发 build/update/verify，检查 graph 是否新鲜、可复现、可 diff。

---

## Key Flows

- F1. **Cold graph build**
  - **Trigger:** operator 或 CI 在 repo root 运行 graph build。
  - **Actors:** A6 -> A1 -> A2 -> A3
  - **Steps:** 发现 repo root 与支持文件 -> loader 判定语言与 tags config -> parse + tags extraction -> 生成 chunk / symbol / import / reference records -> 写入 graph store -> 生成 canonical snapshot manifest -> 原子更新 `.tree-sitter-context-mcp/HEAD`。
  - **Outcome:** 当前 repo 有一个可查询的 graph snapshot，HEAD 指向它，snapshot id 可被 R0/v2 合同引用。
  - **Covered by:** R1.1, R1.2, R1.3, R1.6, R1.7

- F2. **Incremental update**
  - **Trigger:** operator、git hook 或 CI 运行 graph update，或工作树变化需要刷新 graph。
  - **Actors:** A6 -> A1 -> A3
  - **Steps:** 读取 HEAD 上一 snapshot -> 计算 changed/deleted/renamed files -> 用内容 hash 跳过未变文件 -> 重新解析变更文件 -> 用 import/reference 边标记需要重检的依赖文件 -> 写入新 snapshot -> 更新 HEAD。
  - **Outcome:** 新 snapshot 只包含已重新计算或确认未变的 graph state；旧 snapshot 可用于 diff。
  - **Covered by:** R1.4, R1.5, R1.6, R1.8

- F3. **Snapshot diff**
  - **Trigger:** R0/v2 freshness 判断、未来 `should_reorient()`、R3 impact analysis 或 operator 调试请求两个 snapshot 的差异。
  - **Actors:** A4/A5/A6 -> A1 -> A3
  - **Steps:** 载入 from/to snapshot -> 比较文件、chunks、symbols、edges -> 返回 added/removed/modified/renamed/stale/unknown buckets 与原因 -> 标明是否存在 orientation-severe 变化。
  - **Outcome:** 消费者知道哪些 chunks 变化、为什么变化、置信度如何；不会把 graph 不可用或未计算误报为"无变化"。
  - **Covered by:** R1.9, R1.10, R1.11

- F4. **Cross-file reference indexing**
  - **Trigger:** build/update 解析一个文件的 definitions、references、imports。
  - **Actors:** A1 -> A2 -> A3
  - **Steps:** tags records 生成 symbol candidates -> import/module hints 建立文件间候选边 -> resolver 将 reference 关联到 definition candidates -> 对 confirmed/ambiguous/unresolved/unsupported 分别记录边与 Provenance。
  - **Outcome:** R1 具备跨文件引用解析基础设施，但不承诺完整类型检查或语言服务器级精度。
  - **Covered by:** R1.12, R1.13, R1.14

- F5. **R0 compatibility path**
  - **Trigger:** pi-mono 按 R0 v1 合同调用 `get_context_bundle(path, stable_id, tier:"sig", output_format:"sexpr")`。
  - **Actors:** A4 -> existing R0 CLI
  - **Steps:** 现有 `bundle` 参数、结果类型、S-expression canonical form 不变；R1 graph build/update 是相邻能力，不改变 R0 v1 调用形态。
  - **Outcome:** R1 可以被开发和发布，而不会破坏 R0 已完成的 bridge contract。
  - **Covered by:** R1.15

---

## Requirements Trace

| ID | Requirement | Rationale | Acceptance |
| --- | --- | --- | --- |
| R1.1 | 提供 additive graph CLI namespace，至少覆盖 `build`, `update`, `status`, `verify`, `clean` 能力；不得改变 `tree-sitter-context bundle` v1 参数或输出语义。 | R1 需要可操作入口，但 R0 CLI 合同已冻结。 | AE1, AE8 |
| R1.2 | `build` 必须从 repo root 扫描支持文件，遵守 ignore / generated-file 策略，并通过 `tree-sitter-loader` 做语言发现。 | repo map 不能靠硬编码 Rust 路径；loader 是现有基础设施。 | AE1, AE7 |
| R1.3 | symbol extraction 首选现有 tags 查询；缺少 tags config 时必须记录 typed unsupported/degraded diagnostic，不得伪造 defs/refs。 | ideation #4 的直接基础是 tags；honesty 是 branch review 的核心规则。 | AE6, AE7 |
| R1.4 | `update` 必须基于 previous HEAD snapshot 与 git/worktree diff 找出 changed/deleted/renamed files，并用内容 hash 跳过实际未变文件。 | 更新机制要快且可解释，避免每次全量重建。 | AE3 |
| R1.5 | `update` 必须能通过已有 import/reference 边标记 dependent files 需要重检；无法确定依赖时返回 degraded confidence，而不是静默跳过。 | 跨文件 graph 的正确性不能只看直接改动文件。 | AE3, AE6 |
| R1.6 | 每个 snapshot 必须写入 durable graph store 与 canonical snapshot manifest；store 中所有路径必须 repo-relative，不允许绝对路径进入 hash 输入。 | snapshot 要可复现、可搬迁、可审计。 | AE1, AE2 |
| R1.7 | `graph_snapshot_id` 必须是 graph state 的 deterministic XXH3 digest，输入包括 graph schema version、repo-relative file records、nodes、edges、content hashes、diagnostics summary，并按 canonical order 排序；不得包含时间戳、DB row id、绝对路径或随机顺序。 | R0 已把 `graph_snapshot_id` 锁成 determinism contract。 | AE2 |
| R1.8 | `.tree-sitter-context-mcp/HEAD` 必须原子更新为当前 snapshot id；build/update 失败时保留旧 HEAD；status 必须能报告 HEAD 是否存在、是否指向可读取 snapshot、是否 stale。 | v2 orientation 会在 session start 读取 HEAD；不能读到半写状态。 | AE4, AE5 |
| R1.9 | Snapshot diff API 必须接受 `from_snapshot_id` 与 `to_snapshot_id`，返回 changed files、changed chunks、changed symbols、changed edges；chunk buckets 至少包括 `added`, `removed`, `modified`, `renamed_or_moved`, `unchanged`, `unknown`。 | 用户明确要求知道哪些 chunks 变化了。 | AE3, AE4 |
| R1.10 | Diff records 必须携带 reason、strategy、confidence、old/new `path + stable_id`、content hash / signature hash 信息；graph unavailable/version mismatch 必须 typed error。 | 后续 R3/v2 需要可分支的负信号，不需要猜测。 | AE3, AE5 |
| R1.11 | Snapshot diff 必须为 R0 v2 合同预留 severe change 判断：god-node 删除/重命名、community 重组等 postprocess 信号未计算时返回 `postprocess_unavailable`，不得返回 false。 | R0 锁定了 `should_reorient()` 的严重触发条件；R1 不能让缺失数据伪装成安全。 | AE4 |
| R1.12 | Graph node key 必须 collision-aware：内部关系键不得只依赖裸 `stable_id`，至少绑定 repo-relative path 与 anchor/content component；重复 stable_id 必须可表示为 ambiguous。 | R0 v1 已拒绝裸 stable_id；branch review 记录过 duplicate overwrite 风险。 | AE6 |
| R1.13 | Cross-file resolver 必须先覆盖 definition/import/reference 基础边，边状态至少为 `confirmed`, `ambiguous`, `unresolved`, `unsupported`，并记录来源策略。 | R1 是基础设施，不是完整语言服务器；状态必须诚实。 | AE6, AE7 |
| R1.14 | Cross-file graph 必须支持未来 compaction 所需查询：给定 `path + stable_id` 判断节点是否仍存在并返回当前 signature；给定一批 handles 返回 signature-only records。 | R0 R25 已锁定 graph-aware compaction 的最低查询义务。 | AE9 |
| R1.15 | R1 不得实现 v2 orientation 注入、graph-aware compaction runtime、R3 query primitives 或 daemon；daemon 仅当 R12 门限未来失败或 workload 变化时重新考虑。 | 用户约束与 R0 性能报告都要求维持范围。 | AE8, AE10 |

---

## Acceptance Examples

- AE1. **Covers R1.1, R1.2, R1.6.** 给定一个含 Rust fixture 的 repo，运行 graph build 后，`.tree-sitter-context-mcp/HEAD` 存在并指向一个 snapshot；graph store 中包含 repo-relative file record、chunk/node record、symbol record，且 `status` 报告 clean。
- AE2. **Covers R1.6, R1.7.** 给定同一 checkout、同一 graph schema version、同一 ignore 配置，连续运行两次 cold build，生成的 `graph_snapshot_id` 完全相同；移动 checkout 绝对路径后重建，snapshot id 仍相同。
- AE3. **Covers R1.4, R1.5, R1.9, R1.10.** 给定 snapshot S1，修改一个函数体但不改签名后运行 update 得到 S2；snapshot diff 返回该 chunk 为 `modified`，reason 指向 content hash 变化，signature hash 未变，confidence 不高于证据能支持的等级。
- AE4. **Covers R1.8, R1.9, R1.11.** 给定 S1 中存在一个高连接 symbol，重命名后得到 S2；diff 返回 removed/added 或 renamed_or_moved chunk，并在 postprocess 已运行时标记 severe orientation candidate；postprocess 未运行时返回 `postprocess_unavailable`。
- AE5. **Covers R1.8, R1.10.** 模拟 graph store 写入中断或 schema version 不匹配，`verify` 返回 typed graph error，HEAD 仍指向上一个完整 snapshot，不生成半有效 snapshot id。
- AE6. **Covers R1.3, R1.12, R1.13.** 给定 `a.rs` 引用 `b.rs` 中定义的 public function，build 后 graph 有 reference/import candidate edge；如果两个候选定义同名，edge 状态为 `ambiguous` 且列出 candidates，不静默选第一个。
- AE7. **Covers R1.2, R1.3, R1.13.** 给定一个 loader 能识别语言但无 tags config 的文件，build 仍记录文件解析状态，但 symbol/ref extraction 返回 unsupported diagnostic；graph 不伪造空引用集为 confirmed truth。
- AE8. **Covers R1.1, R1.15.** R1 合并前后，`tree-sitter-context bundle <path> --stable-id <id> --tier sig --format sexpr --max-tokens N --budget N` 的 v1 contract tests 继续通过；无新增必填参数，无输出字段语义改变。
- AE9. **Covers R1.14.** 给定一个旧消息中引用的 `path + stable_id`，graph query 能返回 `exists + current signature`；删除该 symbol 后 update，query 返回 typed missing/stale record，而不是空字符串。
- AE10. **Covers R1.15.** 在 R0 performance report gate 仍为 PASS 的情况下，R1 不启动常驻 daemon、不要求 background service，也不修改 pi-mono compaction/runtime 行为。

---

## Success Criteria

**Operator outcome**
- 一个新 checkout 能通过一次 graph build 获得 repo map、HEAD、deterministic snapshot id，并能用 status/verify 判断 graph 是否可信。
- 日常 edit 后能通过 update + diff 知道哪些 chunks/files/edges 变化了，且每条变化都有 reason/strategy/confidence。

**Downstream handoff quality**
- R2 能在 graph 节点上扩展 AstCell / canonical symbol path / Provenance，不需要重新定义 graph identity。
- R3 能在 graph store 上实现 find-defs、find-callers、impact-analysis 等查询，而不需要重新扫描整个 repo。
- v2 orientation/compaction 能读取 HEAD 与 graph queries，但 R1 本身不注入 prompt、不替换 compaction。

---

## Scope Boundaries

- 不改变 R0 v1 `tree-sitter-context bundle` CLI 合同、S-expression canonical form、pi-mono bridge 入参或现有七个工具输出。
- 不实现 v2 orientation block 注入，不在 pi-mono system prompt 中读取或刷新 orientation。
- 不实现 graph-aware compaction runtime，不替换 `compact()`，不调用 LLM summary fallback。
- 不实现 R3 agent-facing query primitives，例如 `/find-callers`, `/find-defs`, `/impact-analysis`, `shortest_path`, `safe_edit`, `query_semantic_symbols`。
- 不默认引入 daemon、MCP server、stdio JSON-RPC 服务、N-API 或 WASM bridge。R12 当前 PASS；daemon 只在未来性能门限失败或真实 workload 改变时重新评估。
- 不承诺语言服务器级类型解析、宏展开、动态 dispatch、build-system-specific module resolution 或跨包依赖图完整性。
- 不引入 vector search、embedding lookup、LLM semantic search、TUI/sidebar、exploration overlay 或 edit validation。
- 不把裸 `stable_id` 当作全局唯一 key；所有 graph 关系必须能处理重复、ambiguous 和 unsupported。
- 不把缺少 tags config、graph postprocess 未运行、store 锁住或 schema drift 解释为"没有变化"或"没有引用"。

---

## Key Decisions

- **R1 是 graph substrate，不是 agent tool surface。** 理由：R0 已完成单工具 bridge，R3 才是 find/impact/query primitive 层；R1 先把 graph state、snapshot、diff 和 xref 索引做可靠。
- **优先复用 loader 和 tags。** 理由：`crates/context/src/symbols.rs` 已封装 `tree_sitter_tags::TagsContext`，`crates/cli/src/context.rs` 已经通过 loader 获取 `tags_config`；R1 应沿现有 seams 扩展，不重写语言发现。
- **Snapshot id 只 hash canonical graph state。** 理由：HEAD 与 prompt-cache freshness 依赖字节稳定；绝对路径、时间戳、DB row order 和随机 map 顺序都会破坏 determinism。
- **Diff 比 build 更重要。** 理由：R0/v2 的 freshness、compaction 和 future impact analysis 都需要知道变化边界；只有 HEAD 没有 diff，LLM 仍然不知道旧 context 是否可信。
- **Cross-file refs 先做 honest infrastructure。** 理由：tags/import 能给出第一层结构感知；遇到 ambiguous/unresolved/unsupported 要显式暴露，不能追求假完整。

---

## Risks & Dependencies

- **Stable identity dependency:** R1 必须继承 R0/hardening 对 duplicate stable_id 的保守姿态；若 R2 canonical symbol path 尚未完成，R1 内部 key 需要绑定 path/anchor/content 以避免静默折叠。
- **XXH3 dependency:** R0 合同指定 deterministic XXH3。当前计划阶段需要确认引入的 XXH3 实现、canonical byte encoding 和跨平台一致性；若不可行，必须先修订 R0/R1 contract，而不是偷偷换 hash。
- **Tags coverage:** 部分语言可能没有 tags query 或 refs 捕获不足。R1 必须把这些标为 unsupported/degraded，后续可用 language-specific query packs 增强。
- **Incremental correctness:** Git diff、rename detection、deleted files、untracked files、generated files 和 import-dependent invalidation 都可能漏边。R1 需要 verify/status 暴露 stale/degraded，而不是只优化 happy path。
- **Graph store migration:** 持久 store 需要 schema version、verify、clean 和 typed version mismatch；否则旧 graph 可能污染新查询。
- **Worktree concurrency:** `.tree-sitter-context-mcp/HEAD` 与 graph store 写入需要 per-worktree lock/atomic rename，避免两个 update 同时写坏 snapshot。
- **Postprocess availability:** God-node/community diff 属于更高阶 graph postprocess；R1 如未实现完整 postprocess，必须返回 `postprocess_unavailable` 供 v2 判断，而不是 false negative。
- **Monorepo scale:** 冷 build 可能超过可接受时间；R1 应先保证 streaming/progress/status 与 incremental update 正确，再由实际 benchmark 决定是否需要 daemon 或 deeper caching。

---

## Outstanding Questions

### Resolve Before Planning

（无。R1 的产品边界、前置条件和非目标已经足够明确，可以进入 `/ce-plan`。）

### Deferred to Planning

- [Affects R1.1][Technical] Graph namespace 的最终命令拼写是复用 `tree-sitter-context graph ...`，还是单独引入 `tree-sitter-context-mcp graph ...`；无论选择哪一个，都必须保持 R0 `bundle` 不变。
- [Affects R1.6][Technical] Graph store 的最小持久形态：SQLite WAL + canonical snapshot JSON，还是先以 JSON snapshot + memory index 起步；计划阶段用查询需求和测试成本决定。
- [Affects R1.7][Technical] XXH3 digest 的 crate、byte encoding、schema version 输入和 cross-platform golden fixtures。
- [Affects R1.9][Technical] `renamed_or_moved` 的判定阈值：stable_id、symbol path、signature hash、content hash 哪些组合足以从 remove/add 提升为 rename。
- [Affects R1.13][Needs research] 每个首批支持语言的 tags query 是否同时暴露 definition 与 reference；缺失 refs 的语言如何降级。
- [Affects R1.14][Technical] Signature-only record 复用 R0 `sig` tier S-expression，还是 graph store 存 canonical signature bytes。

---

## Next Steps

`-> /ce-plan` 为 R1 拆实施计划。建议先从最小可验证路径开始：graph build 生成 deterministic snapshot + HEAD，然后加 update/diff，再接 cross-file refs。不要先做 R3 查询工具或 daemon。
