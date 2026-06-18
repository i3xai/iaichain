# Phase 0 Research: IAI 核心任务编排与结算闭环

技术栈由用户指定（Rust + Claude Code CLI 风格），故主要不确定项为「具体选型」与「关键模式」，
逐项解决如下。

## R1. CLI / TUI 框架（Claude Code 风格终端体验）

- **Decision**: `clap`（derive）做子命令解析；交互式体验用 `ratatui` + `crossterm`，
  默认非交互一次性命令，`iai` 无参或 `iai chat` 进入 TUI。
- **Rationale**: clap 是 Rust 事实标准；ratatui 是当前活跃的 TUI 库，能复刻 Claude Code 的
  流式终端体验。文本 I/O + `--json` 双格式满足章程「CLI 文本协议」精神。
- **Alternatives considered**: `structopt`（已并入 clap，弃）；纯 `println!` 无 TUI（体验不足）;
  `cursive`（生态弱于 ratatui）。

## R2. 异步运行时与 Provider 调用

- **Decision**: `tokio` 多线程运行时 + `reqwest`（异步）调用模型 Provider HTTP API；
  Provider 抽象为 trait `ModelProvider`，按 capability 路由。
- **Rationale**: 编排器需并发驱动多个子任务/节点；tokio + reqwest 是成熟组合。trait 抽象
  满足章程 I「仅经能力契约交互，不耦合具体模型」。
- **Alternatives considered**: `async-std`（生态收缩）；同步阻塞（违背异步市场模型/SC-008）。

## R3. 账本防篡改与可独立核验（章程 IV / FR-010/013）

- **Decision**: SQLite append-only 账本表，每条记录存 `prev_hash` 与 `entry_hash`
  （`sha2` 对规范化序列化求哈希形成哈希链）；提供 `iai ledger verify` 重算校验整链。
- **Rationale**: 哈希链在单机即可实现 tamper-evident 且可被任意参与方独立重算核验，
  无需引入区块链复杂度即满足 V1 范围。
- **Alternatives considered**: 区块链/分布式账本（超出 V1，违 YAGNI）；仅数据库行（无防篡改）。

## R4. 任务生命周期状态机（章程 II / FR-002）

- **Decision**: 以 Rust `enum TaskState` + 显式合法转移函数实现七态单向状态机；每次转移写
  审计记录（输入摘要、负责节点、结果指纹）。
- **Rationale**: 类型系统强约束非法转移；显式转移函数便于测试与审计（FR-016）。
- **Alternatives considered**: 隐式按字段推断状态（易产生非法态，弃）。

## R5. 节点匹配算法（FR-004，澄清 Q4）

- **Decision**: 能力满足为硬过滤；候选集按 `score = w1*price_competitiveness +
  w2*historical_success_rate` 加权排序择优，权重可配置。
- **Rationale**: 满足澄清结论「按市场价格与历史成功率加权」，避免纯随机/纯最低价。
- **Alternatives considered**: 纯最低价（鼓励劣质供给）；纯随机（不可控质量）；轮询（忽略质量）。

## R6. 质量门禁（FR-008，澄清 Q1/Q2）

- **Decision**: 两阶段——(1) 规则校验（非空/格式/必含要素）；(2) 裁判模型（model-as-judge）
  打分，达可配置阈值通过。多节点冲突取最高分并记录全部候选来源与评分。
- **Rationale**: 自动化、低延迟、可复算，契合章程 VI；阈值与裁判模型可配置以适应不同任务。
- **Alternatives considered**: 人工评审（吞吐瓶颈）；同行节点投票（V1 复杂度过高）。

## R7. 贡献点计量（FR-011，澄清 Q3）

- **Decision**: `credit = subtask_market_price * quality_factor`，其中 quality_factor 由质量
  评分归一化得到；结算时写入账本，与执行记录一一对应。
- **Rationale**: 以市场价为基础按质量调整，直接落实澄清结论与章程 V。
- **Alternatives considered**: 固定每子任务计点（忽略市场/质量）；纯算力计费（V1 无统一度量）。

## R8. 可观测性（章程 VI）

- **Decision**: `tracing` + `tracing-subscriber`，关键路径（parse/decompose/match/execute/
  aggregate/settle）发出结构化 span 与事件；`--json` 时输出机器可读日志。
- **Rationale**: Rust 生态标准，支持结构化日志/指标，满足全链路可观测要求。
- **Alternatives considered**: `log` crate（无 span/结构化，弱于 tracing）。

## 遗留项处理

- **数据规模上限**（spec Outstanding）：V1 定为单机 SQLite，目标 ~数百并发任务 / ~数百节点；
  超此规模的水平扩展与分布式账本列为后续特性，不在本计划范围。
