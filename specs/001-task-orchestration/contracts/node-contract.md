# Contract: 节点能力契约与执行接口（核心层 ↔ 节点层）

满足章程 I：核心调度层仅通过本契约与节点交互，不依赖具体模型实现。

## 节点能力契约（注册声明）

```json
{
  "node_id": "iai_node_001",
  "capabilities": ["reasoning", "coding", "writing"],
  "models": ["gpt-4.1", "claude", "qwen"]
}
```

约束：
- `node_id` 全局唯一（FR-005）。
- `capabilities` 非空；匹配仅依据此列表（FR-006）。
- `models` 为节点可调用的 Provider 标识，核心层不感知其内部实现。

## 执行接口（trait `ModelProvider` / 节点执行）

抽象签名（概念，非实现）：

```text
execute(subtask: SubtaskRequest) -> Result<ExecutionOutput, ExecError>

SubtaskRequest  = { subtask_id, required_capability, payload }
ExecutionOutput = { result_content, source_node_id, raw_meta }
ExecError       = Timeout | ProviderError | CapabilityMismatch
```

行为约束：
- 节点 MUST 仅承接其声明 capability 范围内的子任务（越权 → CapabilityMismatch）。
- 执行超时/错误 → 返回 ExecError，由核心层触发重匹配（FR-009），不得静默失败。
- 输出 MUST 携带 source_node_id 以支撑来源追溯与冲突取舍（Q2 / FR-016）。

## 质量门禁契约（核心层调用，FR-008）

```text
quality_gate(result: ExecutionOutput) -> QualityVerdict
QualityVerdict = { rules_passed: bool, judge_score: f32, gate_passed: bool }
```

- `gate_passed = rules_passed && judge_score >= configured_threshold`。
- gate_passed=false 的结果不得进入聚合/结算（SC-007）。

## 结算契约（核心层 → 经济层，FR-010/011）

```text
settle(subtask_id, node_id, market_price, quality_score) -> LedgerEntry
credits = market_price * normalize(quality_score)
```

- MUST 生成 append-only 账本记录（含 prev_hash/entry_hash），否则任务不得 Settled。
