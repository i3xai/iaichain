# Contract: CLI 命令（应用层对外接口）

Claude Code CLI 风格：单一可执行 `iai`，子命令制，文本协议——参数/ stdin 输入，stdout 输出，
错误 → stderr。所有命令支持 `--json`（机器可读）与默认人类可读双格式。退出码：`0` 成功，
`1` 用户错误，`2` 系统错误。

## `iai task submit`

提交任务（FR-001）。

- 输入: `iai task submit "<description>"` 或 `--file <path>` / stdin
- 选项: `--json`
- 输出: 任务标识与初始状态
  ```json
  { "task_id": "uuid", "state": "Created" }
  ```
- 验收: 对应 spec US1-AS1（返回可追踪标识，进入 Created）。

## `iai task status <task_id>`

查询任务状态与结果（FR-014）。

- 输出（人类可读）：当前状态 + 进度；Settled 时输出聚合结果与执行来源摘要。
- 输出（`--json`）：`{ task_id, state, subtasks:[...], aggregated_result?, sources:[...] }`
- 验收: US1-AS4（Settled 后可查看完整结果与来源摘要）。

## `iai node register`

注册标准化节点（FR-005）。

- 输入: `--id <node_id> --capabilities reasoning,coding --models claude,qwen`
  或 `--file node.json`：
  ```json
  { "node_id": "iai_node_001", "capabilities": ["reasoning","coding"], "models": ["claude","qwen"] }
  ```
- 输出: `{ "node_id": "...", "status": "Available" }`
- 验收: US2-AS1（声明契约后进入可发现可用状态）；node_id 重复 → 退出码 1。

## `iai node list`

列出已注册节点及状态/累计贡献点。

## `iai ledger show [--node <id>] [--task <id>]`

查询账本记录（FR-013/015）。

- 输出: 贡献点、奖励、执行证据指纹、哈希链字段。
- 验收: US3-AS1（每个参与节点贡献与执行对应）、US3-AS3（参与方查询自身明细）。

## `iai ledger verify`

重算哈希链核验完整性（FR-013, SC-004）。

- 输出: `{ "valid": true, "entries": N }`；篡改时定位首个不一致 seq 并退出码 2。

## `iai market price [<capability>]`

查询公开定价及供需依据（FR-012, SC-006）。

- 输出: `{ capability, price, supply_demand_basis, published_at }`，可复算。
- 验收: US3-AS2（价格依据公开供需信号，对称可见）。

## `iai chat`（可选 TUI）

进入交互式终端（ratatui），流式呈现任务编排过程，对标 Claude Code 体验。非测试关键路径。
