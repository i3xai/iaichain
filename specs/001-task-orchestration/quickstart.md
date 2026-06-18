# Quickstart & Validation: IAI 核心任务编排与结算闭环

本指南验证核心闭环端到端可用。实体/契约细节见 [data-model.md](./data-model.md)、
[contracts/](./contracts/)。

## 前置条件

- Rust 1.83+（`rustup`）、`cargo nextest`（可选）。
- 至少一个模型 Provider 的 API Key（环境变量，如 `IAI_PROVIDER_API_KEY`）。
- 构建：`cargo build --release`，二进制位于 `target/release/iai`。

## 场景 1：节点注册（US2）

```bash
iai node register --id iai_node_001 --capabilities reasoning,writing --models claude
iai node list --json
```
**预期**: 节点 `status=Available` 出现在列表。重复 id 注册退出码 1。

## 场景 2：提交任务并获得聚合结果（US1，MVP）

```bash
TASK=$(iai task submit "分析这份报告并给出改进建议" --json | jq -r .task_id)
iai task status "$TASK" --json
```
**预期**: 提交即返回 `task_id`（state=Created）；轮询后 state 推进至 `Settled`，
输出覆盖任务要求的聚合结果与执行来源摘要（SC-001、SC-008）。

## 场景 3：单节点失败容错（FR-009 / SC-003）

注册两个同能力节点，令其一不可用后提交任务。
**预期**: 失败子任务被重新匹配至可用节点，任务仍成功到达 Settled。

## 场景 4：质量门禁拦截（FR-008 / SC-007）

提交一个会触发低质量评分的子任务（或配置极高阈值）。
**预期**: 未通过门禁的结果不进入结算，任务不进入 Settled，记录可见门禁失败原因。

## 场景 5：账本可追溯与防篡改（FR-010/013 / SC-004）

```bash
iai ledger show --task "$TASK" --json
iai ledger verify --json
```
**预期**: 每个参与节点有与执行对应的贡献点/奖励记录；`verify` 返回 `valid=true`。
手动改动一条账本后再 `verify` → 定位首个不一致 seq 并退出码 2。

## 场景 6：市场定价透明可复算（FR-012 / SC-006）

```bash
iai market price reasoning --json
```
**预期**: 返回 price 与公开 supply_demand_basis，按公开规则复算值与系统一致。

## 测试入口

```bash
cargo nextest run            # 单元 + 集成
cargo test --test contract   # 契约测试
```
契约测试覆盖 cli-commands 与 node-contract；集成测试覆盖场景 2/3/4/5 端到端。
