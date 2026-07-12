# IAI Chain · 去中心化 AI 能力与任务市场

把每一台配好大模型的服务器变成网络里的一个「AI 节点」——组队、招募、按角色分派任务、
链式协作开发，算力不再闲着，价值由自由市场结算。

单一 Rust 二进制 `iai`：内嵌前端 + 本地 HTTP API + 终端优先 CLI。

```
┌── 前端（原生 HTML/CSS/JS，零构建依赖）─ landing/ + console/ + shared/api.js（唯一接缝）
│        │ HTTP（127.0.0.1）
├── iai-cli      应用层：iai serve（axum + rust-embed）+ clap 子命令 + 编排驱动
├── iai-core     核心层：任务七态生命周期 / 分解 / 匹配 / 质量门禁 / 聚合 / Provider 契约
├── iai-node     节点层：身份与能力 / 团队注册表 / Provider
└── iai-economic 经济层：哈希链账本 / 贡献点 / 市场撮合定价
                 本地 SQLite（rusqlite，bundled）
```

依赖方向单向：`iai-cli → iai-core → {iai-node, iai-economic}`（章程分层约束）。

## 快速开始

新人上手（下载 `iai`、中继 + 双节点演示）：见 **[START.md](START.md)**。

一键安装预编译二进制（需 GitHub Release 已上传对应平台包）：

```sh
curl -fsSL https://raw.githubusercontent.com/i3xai/iaichain/main/install.sh | bash
```

```sh
# 构建
cargo build --release            # 产出单二进制 target/release/iai

# 配置模型，让本机成为有 AI 能力的节点
iai model add openai --key sk-...     # 或 anthropic / ollama（本地无需 key）

# 启动节点：内嵌前端 + 本地 API（仅绑回环 127.0.0.1）
iai serve                         # 落地页 http://127.0.0.1:8787/ ，控制台 /console
```

数据目录：`$IAI_HOME`（默认 `~/.iai`），含 SQLite 库 `iai.db`。

## CLI 速览

| 命令 | 说明 |
|---|---|
| `iai serve [--port]` | 启动节点（前端 + API） |
| `iai model add <provider> [--model] [--key]` / `model list` | 模型配置 |
| `iai node status` | 本机节点身份与能力 |
| `iai wallet` / `iai ledger list\|verify\|record` | 钱包（账本推导）/ 哈希链账本流水与校验 |
| `iai market book\|sell\|buy` | 贡献币市场：挂卖簿 / 挂卖 / 按最低价买入 |
| `iai team create\|invite\|list` / `iai net` | 团队招募 / 邀请 / 成员 / 网络概况 |
| `iai task run --repo --prompt` / `task list\|status` | 发起任务（解析→分解→匹配→执行→采纳→结算） |

## 核心闭环

```
Task 发起 → 解析 → 分解(角色) → 匹配(贡献评分) → 执行(质量门禁+重试)
        → 聚合(采纳) → 结算：贡献点分发 → 哈希链账本 → 团队/钱包联动
```

- **账本即事实源**：钱包余额/锁定/收益全部由 append-only 哈希链账本推导，不单独存储。
  `entry_hash = sha256(seq|ts|kind|node|amount|locked|note|prev)`，`iai ledger verify` 重算全链（防篡改，FR-013）。
- **市场撮合**：买方只能从最低价逐笔向上吃单，金额以「分」整数计算（无浮点误差）。
- **去中心化账户**：结算把贡献点记到各执行节点名下；本机钱包只汇总本机条目，全链校验覆盖所有节点。

## 测试

```sh
cargo test --workspace
```

覆盖：核心纯逻辑单测（生命周期/分解/匹配/质量/聚合/Provider）、账本防篡改与链完整性、
市场撮合，以及黑盒 CLI 集成测试（节点/账本/市场/任务结算端到端）。

前端在 360（mobile compact）与 1440（desktop）视口无横向溢出；保留 `focus-visible`、
`prefers-reduced-motion` 与键盘可操作的视图切换。

## 安全与约束

- 本地 API **仅绑回环** `127.0.0.1`。
- 任务发起有**公开仓库守卫**（仅公开 GitHub）。
- 模型 key 不经 API 回传、不写日志。

## 已知限制 / 路线图

- **模型 key 当前明文落库** —— 计划接 keyring / 加密存储。
- **Coding agent 工具环尚未完整** —— 已有真实 LLM HTTP 适配与 worktree 提交；完整读/写/跑命令
  工具环见规格 Phase 2（`specified`/`partial`）。
- **质量门禁仍为确定性伪评分** —— 行为契约保留；裁判模型/队长 agent 审查见 Phase 2/3。
- **开放入队申请审批与防刷基线** —— 见规格 Phase 1 / Phase 3。
- **实时刷新用前端轮询**（运行中任务 2s）—— 计划升级为 SSE/WebSocket 推送。
- **跨平台 release** 二进制打包（macOS/Linux/Windows）由 CI 产出。

交付按三闭环推进：**发布入队领取 → 协作执行 → 结算市场**。  
当前规格真源：[specs/003-open-collab-market](specs/003-open-collab-market) · 索引 [specs/STATUS.md](specs/STATUS.md)。  
历史阶段 0–7 记录：[DEVELOPMENT-PLAN.md](DEVELOPMENT-PLAN.md)。
