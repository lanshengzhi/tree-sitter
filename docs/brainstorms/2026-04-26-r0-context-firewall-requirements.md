---
date: 2026-04-26
topic: r0-context-firewall-pi-mono
---

# R0 Agent Interface Contract / Context Firewall — pi-mono 集成层 v1 需求

## Problem Frame

pi-mono 当前的 coding-agent (`pi-mono/packages/coding-agent/src/core/tools/index.ts:83`) 暴露 `read / bash / edit / write / grep / find / ls` 七个工具，没有任何代码结构感知原语；上下文压缩走 `compact()` (`pi-mono/packages/coding-agent/src/core/compaction/compaction.ts`) 的 LLM 总结路径，可被 prompt injection 改写（Anthropic 2026-03-31 源码泄露 + Adversa AI 审计）；系统提示无 cache-stable 仓库定向块，每会话冷启动；工具返回纯文本，LLM 无 stable handle 可引用。

R0 的目标是把 pi-mono 重塑成 tree-sitter-context graph 的纯消费者：S-expression 序列化、stable handle、显式负信号、确定性图感知 compaction、cache-stable orientation。但 R0 依赖的 R1 (graph build/update)、R2 (AstCell + Provenance)、R3 (query primitives) 都未实现，且现有 `crates/context` 是单文件 scope（cross-file 是 RFC 明确的 v1 非目标）。

R0 v1 的工作不是把这些一次性建出来，而是 **(a) 把契约写死，(b) 用一条最小垂直切片证明契约站得住**，让 R1/R2/R3 后续按契约填充时不返工，并把现有 `crates/context` 的 dishonest flag 顺手修掉。

---

## Actors

- A1. **pi-mono 会话运行时 (TS)** — 拥有工具分发、system prompt 构造、compaction 触发；R0 在此注册新工具并 hook 进 compaction 路径。
- A2. **`tree-sitter-context` CLI (Rust binary)** — 拥有 AST 解析、chunk、stable_id 生成、S-expr 序列化。v1 切片由其新增的 `bundle` 子命令承担。
- A3. **LLM (消费者)** — 收 system prompt + tool 输出，发 tool call。R0 的真正客户。
- A4. **R1 graph builder (未来)** — 必须按 R0 契约 emit `graph_snapshot_id` (XXH3 deterministic)、支持 snapshot diff、写 `.tree-sitter-context-mcp/HEAD`。v1 不实现，但接口被 R0 锁定。
- A5. **pi-mono operator (人类)** — 写 R0 spec、opt-in/out compaction strategy、运行 CI golden test。

---

## Key Flows

- F1. **v1 切片：单工具调用 (real path)**
  - **Trigger:** LLM 发出 `get_context_bundle({path, stable_id, tier:"sig", max_tokens:2000, output_format:"sexpr"})`
  - **Actors:** A3 → A1 → A2 → A1 → A3
  - **Steps:** pi-mono extension spawn `tree-sitter-context bundle <path> --stable-id <id> --tier sig --max-tokens 2000 --format sexpr --quiet --budget 2000` → CLI 写 canonical S-expr 到 stdout，正常退出码 → pi-mono parse + canonical re-emit → 包成 tool result 透传给 LLM
  - **Outcome:** LLM 收到字节稳定的 S-expr，含 `(provenance ... strategy ... confidence ... graph_snapshot_id "unknown" orientation_freshness "unknown" ...)`、可引用 `(path ...)` 与 `(stable_id ...)`。`unknown` 是 v1 明确占位，R1 graph 未落地前不得伪造 snapshot。
  - **Covered by:** R1, R2, R3, R4, R5, R6, R8, R12

- F2. **失败路径：Rust 输出不合法 S-expr**
  - **Trigger:** CLI 因 bug 产生缺括号 / 未知节点的 S-expr
  - **Actors:** A2 → A1 → A3
  - **Steps:** pi-mono parser 校验失败 → 抛 typed Provenance 错误 (`strategy: "rust-output-invalid"`, `confidence: 0`, `reason` 含具体校验失败点) → 工具返回 `(error (kind rust-output-invalid) (reason "..."))`
  - **Outcome:** LLM 看到 typed 错误，知道工具坏了，不会把噪音当结果。**禁止"parse 失败 → 把原文塞进去"** —— 那等于退化成不透明透传。
  - **Covered by:** R6, R7

- F3. **(v2 契约，v1 不实现) 会话启动 orientation 注入**
  - **Trigger:** pi-mono session 启动
  - **Actors:** A1 reads `.tree-sitter-context-mcp/HEAD` (snapshot id) → A2 生成 orientation S-expr → A1 注入 system prompt prefix
  - **Steps:** snapshot 进 prefix，会话内不重生成 → 每次工具返回的 `provenance.graph_snapshot_id` 与 prefix `orientation-snapshot.id` 比对 → 不一致时仅在 tool result 标 `orientation_freshness: stale`，prefix 字节不动
  - **Outcome:** prompt cache 全程命中 + LLM 显式知道 orientation 可能过期
  - **Covered by:** R16, R22, R24

- F4. **(v2 契约，v1 不实现) compaction 触发**
  - **Trigger:** pi-mono 检测 token 阈值溢出 OR `should_reorient()` 返回 `poison|drift|loop` 且置信达标
  - **Actors:** A1 → A2 (graph query) → A1
  - **Steps:** `compact(messages, {strategy:"graph-aware", fallback:"skip-with-error"})` → CLI 查图：每个老消息里出现的 stable_id 还存在吗？取 signature → 写 `(compaction-result details: (graph-aware-folding ...))` → 图不可用时抛 `CompactionUnavailableError`，pi-mono surface 给 LLM 为 `(compaction-failed reason ... recommendation "request /clear or delegate to subagent")`
  - **Outcome:** **永远不回退到 LLM 总结路径**（这是 R0(f) 的全部安全论点）
  - **Covered by:** R18, R19, R20, R21, R25, R26

---

## Requirements

**v1 垂直切片范围（必须真实端到端）**

- R1. v1 切片只包含一个原语：`get_context_bundle(path, stable_id, tier:"sig", max_tokens, output_format:"sexpr")`，单文件 scope，无 cross-file refs。`path + stable_id` 是 v1 定位键；不得依赖 R1 graph/index 来从裸 `stable_id` 反查文件。若同一文件内匹配多个 chunk，必须返回 typed `(ambiguous_stable_id ...)`，不得任意取第一个。
- R2. v1 切片必须用真实 `crates/context` 数据（不是 mock）端到端跑通：CLI 真解析 → 真 S-expr → pi-mono 真 parse → LLM 真消费。
- R3. v1 切片范围内强制顺手修掉 `estimated_tokens` 被 cap 的 P1 bug（参见 `docs/solutions/workflow-issues/tree-sitter-context-branch-review-2026-04-25.md`）—— 切片返回的 token 数必须诚实。
- R4. v1 切片必须实现显式负信号路径：传入 `stable_id` 不存在时返回 `(not_found (stable_id "...") (reason ...))`；同一文件内匹配多个 chunk 时返回 `(ambiguous_stable_id (stable_id "...") (candidates ...))`；预算耗尽时返回 `(exhausted (depth ...) (omitted-stable_ids ...))`。

**Cross-boundary 序列化契约（S-expression）**

- R5. Rust 侧 `tree-sitter-context bundle --format sexpr` 输出必须是字节确定的 canonical S-expression。canonical 形式定义产物：`docs/plans/sexpr-canonical-form-v1.md`，2 页内含缩进规则（2 空格）、子节点排序规则（参数 / refs / omissions 列表按 stable_id 字典序）、字符串转义规则（R7RS 子集）、注释禁出现。
- R6. pi-mono 侧实现轻量 S-expr parser + canonicalizer（≈ 50–80 LOC，零运行时依赖），对每个 CLI 输出做 parse + canonical re-emit 后再透传给 LLM，**不允许直接透传原文**。Parse 失败时按 F2 路径抛 typed Provenance 错误。pi-mono v1 仅消费 `output_format:"sexpr"`；JSON 不进入 bridge/parser/canonicalizer 路径。
- R7. AstCell / Provenance / Bundle 三个 struct 单源生成：Rust 持源 + 自定义 serde Serializer 强制 canonical；TS 类型由 ts-rs（或等价工具）从 Rust 自动派生。**禁止"两侧各写一份 schema 然后口头对齐"**，那是 100% 漂移概率。
- R8. CI 字节级 golden test 必须有两道闸（缺一道则等同 R6 退化）：(a) Rust 输出 100 次取并集，验证 canonical bytes 等价集是单元素（Rust 端确定性证明）；(b) pi-mono parse + re-emit 的 byte-equal 校验。两道每 PR 跑。

**Bridge 与 pi-mono 集成姿态**

- R9. v1 桥用 subprocess CLI 调用（而非 N-API / WASM / daemon）。pi-mono 注册 `get_context_bundle` 为 extension，spawn `tree-sitter-context bundle <path> --stable-id ...`。spawn 必须使用 argv array（`shell:false` / Bun 等价非 shell 路径），`path` 限制在 cwd/repo root 内，`tier` / `output_format` 做 enum 校验，`stable_id` 做长度与字符集限制，`max_tokens` clamp 到 operator 配置上限。
- R10. 输出协议固定为：stdout 关流 + 退出码 0 = 正常结束；非零退出码 + stderr = 错误。**禁止把错误塞进 stdout S-expr**。与 grep/find/ls 协议对齐，pi-mono 端零新增错误处理代码。
- R11. v1 切片范围内现有 7 个工具 (`read/bash/edit/write/grep/find/ls`) 输出格式不动。仅 `get_context_bundle` 走 S-expr。"全局 cache stability" 不是 v1 论证目标。
- R12. v1 必测 spawn 延迟分布：cold / warm 各 100 次，记录单调用 p50/p95；并跑一段真实交互看一回合常调几次 bundle。**三档门控规则**：p95 < 100ms 且真实交互 p50 每回合 ≤ 3 次调用 = 通过；100ms ≤ p95 ≤ 200ms 或真实交互 p95 每回合 > 3 次调用 = 记录 24h cooling-off + operator 决策；p95 > 200ms 或真实交互 p50 每回合 > 3 次调用 = 把 daemon (option 2) 从 R1 提到 v1.5，不要拖到 R1 才动。

**`crates/context` CLI 兼容路径（v1 切片的硬前置）**

- R13. v1 之前必须修复 `--budget` / `--quiet` / `--grammar-path` 三个 dishonest flag，实际生效。`--budget` 与 R3 的 `estimated_tokens` 诚实化是孪生工作，同一 PR 完成。
- R14. v1 新增并冻结切片专属调用形态：`bundle <path> --stable-id <id>` (新增单文件单点定位能力)、`--tier {sig}`、`--format {sexpr}`、`--max-tokens <n>`。`--format json` 可作为 CLI human/debug-only 输出保留，但不属于 pi-mono v1 bridge contract，不要求 TS 侧 JSON parser/canonicalizer。`id|sig+doc|full` tier 在 v1 只能作为 reserved value 返回 typed unsupported，不冻结语义。一旦发布给 pi-mono 调用，breaking change 视同破 pi-mono。冻结产物：`docs/plans/tree-sitter-context-cli-v1-contract.md`，明确"v1 之后不动这些已启用 flag 语义，新功能加新 flag 或先解除 reserved"。
- R15. v1 切片对 `crates/context` 的单文件 scope **不破坏**，cross-file 字段返回 `(unknown_cross_file (reason "v1-non-goal"))`。

**v2 契约（v1 仅锁 schema 与生命周期，不实现代码）**

以下契约基于 R0 brainstorm 时的理解，用来阻止 v1 期间偷回 LLM 总结路径或破坏 prompt cache。R1/R2 实施过程中如发现 graph snapshot、orientation、compaction 假设不成立，必须触发 v1 contract 修订 PR；不得在实现里沉默改语义。

- R16. orientation block schema 与生命周期固化（v1 不生成，不注入 system prompt）。
  - 顶层结构：`(orientation (snapshot id ... generated_at ... graph_root_hash ...) (architecture ...) (god_nodes ...) (entry_points ...))`。
  - 生命周期：会话启动一次性快照，会话内冻结。Prefix 字节不动。
  - 新鲜度信号走 tool result：每个 R3 工具的 Provenance envelope 携带 `graph_snapshot_id` 与 `orientation_freshness ∈ {fresh|stale|unknown}`，`orientation_snapshot.id` 与 `provenance.graph_snapshot_id` 不等时设 stale。
- R17. `should_reorient()` 严重升级触发条件（区别于"普通陈旧"）：仅当 graph diff 含 god_node 重命名/删除 OR community 重组（Louvain 变化超阈值）才触发；普通 edit 不触发。
- R18. graph-aware compaction 替换边界：v2 全量替换 pi-mono 的 `compact()`。v1 契约把 `compact()` 入参从 hard-coded 路径改为：
  ```
  compact(messages, {
    strategy: "graph-aware" | "skip-with-error",
    fallback: "skip-with-error" | "operator-override-llm"
  })
  ```
  默认 `strategy: "graph-aware"`、`fallback: "skip-with-error"`。LLM 总结路径仅在 operator 显式 `operator-override-llm` 时启用。
- R19. compaction 触发器是 token 阈值 + `should_reorient()` 复合，不互斥。两者都走 graph-aware 路径。pi-mono 现有 token 阈值逻辑保留；should_reorient 是新增触发。
- R20. **失败回退禁止 LLM 总结**：图不可用（SQLite 锁死 / 索引损坏 / 版本不兼容，预期 P99+）时抛 typed `CompactionUnavailableError`，pi-mono surface 给 LLM 为 `(compaction-failed reason ... remaining_budget <n> recommendation "request /clear or delegate to subagent")`。**graph 失败 → 偷偷调 LLM 摘要 = 安全论点的死洞，明令禁止**。
- R21. `CompactionResult.details` schema 锁定（复用 `pi-mono/packages/coding-agent/src/core/compaction/compaction.ts:108` 既有扩展点）：
  ```
  (compaction-result
    details: (graph-aware-folding
      (preserved-handles (stable_id ...) ...)
      (folded-messages (message-id ... original-tokens ... folded-tokens ... signature-only-stable_ids (...)))
      (compaction-snapshot-id "<XXH3>")
      (graph_snapshot_id "<XXH3>")))
  ```
  复用 R16 的 `graph_snapshot_id` 协议，不引入新概念。

**R1 / R2 对 R0 的契约义务（v1 锁，R1/R2 实施时必须满足）**

- R22. R1 graph 必须 emit deterministic `graph_snapshot_id` (XXH3 of graph state)。
- R23. R1 必须支持 snapshot diff API：给两个 snapshot id，返回是否含 god_node / community 变化（供 R17 should_reorient 判定）。
- R24. R1 必须把当前 snapshot id 写入 `.tree-sitter-context-mcp/HEAD`，pi-mono 启动时一次性读取。
- R25. R1 必须支持 graph-aware compaction 所需的两个查询：(a) 给定 stable_id，是否仍存在 + 当前 signature；(b) 给定消息内容里出现的 stable_ids，返回 signature-only S-expr（不含 body）。
- R26. R1 错误必须 typed：`GraphLockedError` / `GraphCorruptedError` / `GraphVersionMismatchError`，compact() 据此决定 surface 给 LLM 的 reason 字符串。
- R27. R2 `StableId` 重名静默折叠 P1 必须先于 R1 graph / cross-file 关系键启用（参见硬化计划）。R0 v1 切片不得把全局唯一 `StableId` 当作前提；v1 只能用 `path + stable_id` 在单文件内定位。若 hardening PR 已先合并，R0 直接复用修复。

---

## Acceptance Examples

- AE1. **Covers R5, R6, R8.** 给定同一份源文件，连续调 100 次 `tree-sitter-context bundle crates/context/src/lib.rs --stable-id Foo::bar --tier sig --format sexpr --quiet`，stdout 字节序列必须 100% 相同。pi-mono 端 parse + canonical re-emit 后 byte-equal 输入。
- AE2. **Covers R4, R6.** 调 `get_context_bundle({path: "crates/context/src/lib.rs", stable_id: "DoesNotExist", ...})`，工具返回顶层 Provenance envelope，且同一 result 内含 `(not_found (stable_id "DoesNotExist") (reason "no chunk matches stable_id"))` 节点；envelope 的 `confidence` 为 `0`，`graph_snapshot_id` 与 `orientation_freshness` 在 v1 均为 `unknown`。
- AE3. **Covers R6, F2.** 测试用 fixture 让 CLI 故意输出 `(bundle (foo` (缺括号)，pi-mono 必须返回 `(error (kind rust-output-invalid) (reason "unbalanced parens at offset 12"))`，**不允许把原文塞进 tool result**。
- AE4. **Covers R3, R13.** 调 `bundle crates/context/src/lib.rs --budget 500 --max-tokens 5000`：返回的 included chunk 总 `estimated_tokens` ≤ 500（budget 真生效）；若存在 oversized / omitted chunk，其自身 `estimated_tokens` 仍保留真实值，可大于 500（不是被 cap 到 budget 或 max_tokens 的伪安全值）。
- AE5. **Covers R20.** （v2 实施时验证）模拟 graph SQLite 锁死，调 `compact(messages, {strategy:"graph-aware"})`：抛 `CompactionUnavailableError`，pi-mono 注入 `(compaction-failed reason "graph-locked" ...)` 进消息流，**不调 LLM 总结**。
- AE6. **Covers R12.** v1 切片 ship 前必跑性能 fixture：cold 100 调 + warm 100 调，输出 p50/p95 报告，并附真实交互每回合调用次数分布。报告必须落入 R12 三档门控之一；进入中间档时记录 operator 决策，进入失败档时触发 v1.5 daemon 议程提前。
- AE7. **Covers R7.** 修改 Rust 侧 AstCell / Provenance / Bundle fixture 加一字段后，运行 ts-rs（或等价生成器）但未提交对应 TS 类型变更，CI 必须失败；提交生成产物后 CI 通过。
- AE8. **Covers R11.** v1 切片合并前后，`read/bash/edit/write/grep/find/ls` 七个工具的 tool result schema fixture 字节相同；新增 `get_context_bundle` 不改变既有工具输出。
- AE9. **Covers R4.** 调 `get_context_bundle({path: "crates/context/src/lib.rs", stable_id: "...", max_tokens: 1, output_format: "sexpr"})` 触发预算耗尽时，result 内含 `(exhausted (depth ...) (omitted-stable_ids ...))`，且 omitted stable_ids 顺序符合 R5 canonical 排序规则。
- AE10. **Covers R1, R4, R27.** 给定一个 fixture 文件中两个 chunk 暂时产生相同 `stable_id`（R27 未合并前的退化场景），调 `get_context_bundle({path, stable_id, ...})` 必须返回 `(ambiguous_stable_id (stable_id "...") (candidates ...))`，不得静默选择第一个候选。
- AE11. **Covers R16, R22, R24.** （v2 实施时验证）session 启动只读 `.tree-sitter-context-mcp/HEAD` 一次生成 orientation prefix；同一会话内 prefix 字节不变。tool result 的 `graph_snapshot_id` 与 prefix snapshot 不一致时只标 `orientation_freshness: stale`，不重写 prefix。
- AE12. **Covers R17, R23.** （v2 实施时验证）普通 edit 的 snapshot diff 不触发 `should_reorient()`；god_node 重命名/删除或 community 重组超过阈值时触发，并返回可审计的 diff reason。
- AE13. **Covers R21, R25, R26.** （v2 实施时验证）graph-aware compaction 成功时 `CompactionResult.details` 含 preserved handles、folded messages、`compaction-snapshot-id` 与 `graph_snapshot_id`；graph locked/corrupted/version mismatch 三类错误分别映射到 typed reason，不走 LLM 总结。

---

## Success Criteria

**人类结果（pi-mono operator 视角）**
- v1 切片下，pi-mono 端 LLM 调一次 `get_context_bundle` 拿到的字符串能稳定 prompt cache（同 path + stable_id + tier 字节相同）。
- 现有 `crates/context` 的 P1 dishonest flag 在 v1 切片合并前消失（`--budget` / `--quiet` / `--grammar-path` 真生效，`estimated_tokens` 真诚实）。
- 三份 v1 契约文档可独立工作：`sexpr-canonical-form-v1.md`、`tree-sitter-context-cli-v1-contract.md`、本需求文档。

**下游交接质量（R1/R2/R3 实施者视角）**
- R1 graph 实施者打开本文档 + 三份契约后能直接动手，无需再 brainstorm；接口设计被 R22–R26 锁死。
- R2 AstCell / Provenance 类型表面被 R7 单源约束，R2 实施时 ts-rs 派生即可，禁止两侧手写。
- R3 后续原语实施时只需在 R0 切片基础上加新 CLI 子命令；S-expr canonical / Provenance envelope / 负信号约定全部复用。

---

## Scope Boundaries

**v1 切片之外（已明确不做）**
- 跨文件 refs（绕开 `crates/context` v1 cross-file 非目标，参见 RFC 第 68–82 行）。
- god_nodes / community / Louvain / PageRank（postprocess 路径，R1 任务）。
- 实际跑 graph-aware compaction（v1 仅锁 schema 与触发器，v2 实施）。
- 实际生成 / 注入 orientation block（v1 仅锁 schema 与生命周期，v2 实施）。
- `Two-Corrections-Rule` (`should_reorient()`) v1 stub 返回 `false`，v2 实施真正逻辑。
- `StableId` 重名消歧逻辑（R27 所属 hardening/R2 工作；v1 切片不实现，只用 `path + stable_id` 避免依赖全局唯一性）。
- 替换或修改现有 7 个工具的输出格式（R11）。
- R3 其它原语（`assert_callgraph` / `missing_symbols` / `safe_edit` / `shortest_path` / `get_ranked_architecture` / `semantic_diff`）。v1 不实现也不预留 stub，留 R3 brainstorm 单独安排。
- N-API / WASM / daemon 桥（v2/v1.5 视 R12 性能门控决定）。
- MCP server 形态（RFC 明确 `crates/context` v1 非目标；R0 自身也只到 pi-mono 侧 extension，不开 stdio JSON-RPC server）。
- 通用化为他用户 / 他 agent 可用（pi-mono 深度定制是约束，不考虑通用 MCP 服务）。
- 替换 grep / find（R0 ideation 提到的 .scm query packs 取代 grep 在 v2/R3 周期，v1 不动）。

---

## Key Decisions

- **v1 = 契约 + 最小垂直切片**（拒绝纯契约 / mock stub / 推迟）。理由：`crates/context` 几条契约假设站不住（StableId / estimated_tokens / S-expr 节省比例），纯文档第一天就脱节；mock 让契约假性通过更糟。
- **subprocess CLI 桥**（拒绝 N-API / WASM / daemon）。理由：v1 只调一个工具，与"端到端验证契约"的范围匹配；daemon 是 v2 的正确答案（未来 R1 graph daemon 起来后自然演进，且与 MCP stdio JSON-RPC 形态对齐）。
- **pi-mono 侧 parse + canonical re-emit**（拒绝透传 / 绑 TS 类型 / 推迟）。理由：prompt cache 字节稳定性是 v1 的 forcing function；HashMap 迭代顺序非确定 + Anthropic prompt cache prefix 1 字节不同 = miss，账单上才发现已经晚了。
- **orientation block 会话启动一次性快照 + 会话内冻结 + 新鲜度走 Provenance（不动 prefix 字节）**（拒绝 graph-update 重生成 / delta 附加 / 延迟生成）。理由：edit 频率 1–20 次/会话，每次重生成 = prompt prefix cache 冷启动；delta 附加 = context bloat 反噬；延迟生成丢了"开场就能定位"价值。
- **graph-aware compaction 全量替换 + 失败 = 显式错误（不回退到 LLM 总结）**（拒绝 should_reorient 接管 / 仅注入结构提示 / v1 不谈）。理由：LLM-as-summarizer 是 R0(f) 整个设计要堵的洞；回退把它从前门请回来 = 设计自废。"图不可用"是 P99+ 事件（本地 SQLite，非网络服务），honest 报错让 agent 决定 > 偷偷 LLM 摘要后果未知。
- **`compact()` 暴露 strategy 参数（不是 hard-coded 路径）**。理由：v2 实施可在 strategy 切换里渐进推出，operator 显式 opt-in 才能走回 LLM 路径，安全默认 + 可恢复操作。
- **v1 查询键 = `path + stable_id`，不是裸 `stable_id`**。理由：R1 graph/index 未实现前，裸 stable_id 没有可验证的文件定位来源；单文件 scope 应把文件路径作为显式输入，避免 v1 偷做全局索引或依赖未修完的跨文件 identity。

---

## Dependencies / Assumptions

**外部依赖（已验证存在）**
- `pi-mono/packages/coding-agent/src/core/compaction/compaction.ts:108` 的 `CompactionResult.details?` 字段已被注释为扩展点（"e.g., ArtifactIndex, version markers for structured compaction"）—— R21 复用此缝。已读源码确认。
- `pi-mono/packages/coding-agent/src/core/system-prompt.ts:21–66` 的 `contextFiles` 与 `appendSystemPrompt` 是 v2 orientation 注入点。已读源码确认。
- `pi-mono/packages/coding-agent/src/core/tools/index.ts:83` 的 `ToolName` enum 与 `createCodingTools` factory 是 v1 切片注册点。已读源码确认。
- `pi-mono/packages/coding-agent/src/core/extensions/` 已有 extension loader / runner / wrapper / types，且 `examples/extensions/plan-mode/` 提供模式参考。已读目录确认。
- `crates/context` 已有 chunk / stable_id / range / invalidation / budgeted bundle 单文件能力。

**外部假设（未验证或时间点假设）**
- ts-rs 或等价工具能在 `crates/context` build pipeline 上接出 TS 类型生成（R7 假设，未做技术验证）。`[Affects R7][Needs research]`
- 现有 `tree-sitter-context` CLI 加 `--stable-id` 单点定位需要 `crates/context` 暴露按 stable_id 查询的 API（R14 假设，需检查 `crates/context/src/lib.rs` 是否已支持）。`[Affects R14][Technical]`
- R27 未合并时，同一文件内重复 `stable_id` 可能出现；v1 通过 `ambiguous_stable_id` typed 负信号处理，不把重复项静默折叠。

**组织约束**
- pi-mono 深度定制，不考虑通用性 —— 不做成通用 MCP 服务，所有命名 / 协议可与 pi-mono 强耦合。
- v1 不破坏 `crates/context` 单文件 scope；cross-file 字段一律返回 `(unknown_cross_file (reason "v1-non-goal"))`。
- 与 hardening plan 的协调：R3/R13/R27 对应的 P1 修复 PR 只承担 `docs/plans/tree-sitter-context-hardening-implementation-plan-2026-04-25.md` 列出的责任，不引入 R0 bridge / S-expr / pi-mono 形态。R0 v1 切片在这些修复已合并或已明确不阻塞单文件 `path + stable_id` 查询后再动。

---

## Outstanding Questions

### Resolve Before Planning

（无 —— 五个核心维度均已锁定。）

### Deferred to Planning

- [Affects R7][Needs research] ts-rs 在当前 `crates/context` workspace 上的可行性、生成产物的 TS 类型化质量、对 `lib/binding_node` 现有构建链的影响。
- [Affects R14][Technical] `crates/context/src/lib.rs` 是否已暴露按 `stable_id` 直接查询的 API，还是需要新加 `Bundle::by_stable_id()` —— 计划阶段读源码确定。
- [Affects R12][Needs research] subprocess spawn 在 Bun 运行时的实测延迟（pi-mono 跑在 Bun 上），是否需要 bun-specific 优化（如 `Bun.spawn` 取代 `node:child_process`）。
- [Affects R6][Technical] TS 侧 S-expr parser 50–80 LOC 估算是否经得起 R7RS 转义子集的真实复杂度 —— 计划阶段写最小原型验证。
- [Affects R20][Technical] v2 实施时 `CompactionUnavailableError` 与 pi-mono `compact()` 现有同步签名的兼容方式（throw / Result type / typed null），保留到 v2 计划。
- [Affects R16][Technical] orientation block 在 `customPrompt` vs `appendSystemPrompt` vs `contextFiles` 三个注入点中的最优选择 —— v2 实施时验证哪个真能 cache-stable。

---

## Next Steps

`-> /ce-plan` 拆解 v1 切片实施计划。建议先后顺序（受 R 编号约束）：

1. R13 + R3：确认/合并 dishonest flag + `estimated_tokens` 诚实化 hardening PR（同一 PR，孪生工作；不引入 R0 形态）。
2. R27：确认 StableId 重名消歧 hardening 状态；若未合并，R0 v1 仍必须保持 `path + stable_id` 单文件定位，不引入 cross-file/global StableId key usage。
3. R5 + sexpr-canonical-form-v1.md：写 canonical S-expr 规范产物。
4. R14 + tree-sitter-context-cli-v1-contract.md：CLI 切片调用形态冻结（`bundle <path> --stable-id ... --tier sig --format sexpr`；JSON debug-only；其它 tier reserved）。
5. R7：ts-rs 导出链路落地（`[Needs research]` 先行）。
6. R6 + R8：pi-mono 侧 S-expr parser + golden CI。
7. R9 + R10：pi-mono extension 注册与 subprocess 输出协议，含 argv-array spawn、输入校验、`max_tokens` clamp。
8. R1 + R2 + R4 + R11：v1 切片端到端联调，含 `not_found` / `ambiguous_stable_id` / `exhausted` 负信号路径与既有 7 工具回归 fixture。
9. R12：性能门控（cold/warm 100 调 + 真实交互一回合调用次数，按三档规则决策）。

R16–R21 v2 契约部分由计划阶段决定哪些以"接口锁定 PR"先行（不实现），哪些与 R1 实施并发推进。
