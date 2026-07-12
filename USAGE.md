<!--
ai-agent-guide: 本文件可被 AI 代理直接读取以自动完成「安装 → 配置 → 运行 → 验证」。
machine-summary:
  project: IAI Chain — 去中心化 AI 能力与任务市场，单一 Rust 二进制 `iai`
  language: Rust 1.83+ (edition 2021), Cargo workspace
  install: curl -fsSL https://raw.githubusercontent.com/i3xai/iaichain/main/install.sh | bash
  build: cargo build --release   # 可选；产物 target/release/iai
  run:   iai serve               # 默认 http://127.0.0.1:8787 ，仅绑回环
  data:  $IAI_HOME (默认 ~/.iai)，SQLite 库 iai.db
  health-check: GET http://127.0.0.1:8787/api/health → {"status":"ok","node":"iai-chain"}
  start-doc: START.md            # 中继 + 双节点演示
-->

# IAI Chain 使用文档

> 把每台配好大模型的服务器变成网络里的一个 AI 节点：组队、按角色分派任务、链式协作、
> 自由市场结算。一个二进制 `iai` 同时提供 **Web 控制台 + 本地 HTTP API + 终端 CLI**。

新人快速开始（安装 + 中继双节点）：见 **[START.md](START.md)**。

---

## 0. 给 AI 代理 / 自动化（先读这一节）

如果你是一个 AI 代理，按以下**确定性步骤**即可从零把系统跑起来。每步都给了成功判据。

```sh
# 1) 安装 iai（优先：GitHub Release 预编译包，无需 Rust）
curl -fsSL https://raw.githubusercontent.com/i3xai/iaichain/main/install.sh | bash
iai version                # 成功判据：输出 "iai-chain 0.4.2"（版本号随 Release 变化）

# 若 install.sh / Release 不可用，再源码构建（需 Rust 1.83+）：
#   cargo --version && git clone https://github.com/i3xai/iaichain.git && cd iaichain
#   cargo build --release && export PATH="$PWD/target/release:$PATH"

# 2) 配置一个模型（让本机成为有 AI 能力的节点）
iai model add openai --key "$OPENAI_API_KEY"   # ollama 本地无需 key: iai model add ollama --model qwen
iai node status            # 成功判据：含 "队长"、模型名、"reasoning"

# 3) 启动节点（后台），等待就绪
iai serve &                # 默认端口 8787，仅绑 127.0.0.1
sleep 1
curl -s http://127.0.0.1:8787/api/health   # 成功判据：{"status":"ok","node":"iai-chain"}
```

要点（供推理用）：
- **纯本地、无中心服务器**；节点 API 仅监听 `127.0.0.1`（协调中继另见 START.md）。
- 所有状态在 `$IAI_HOME`（默认 `~/.iai`）的 SQLite 库 `iai.db`；删库即重置。
- 任务发起有**公开仓库守卫**：`--repo` 必须含 `github.com`，否则拒绝。
- 已有真实 LLM HTTP 适配（OpenAI / Anthropic / Ollama / MiniMax）；无 key 时可回退 Mock。
- 命令既可走 **CLI**，也可走 **HTTP API**（见第 6 节）；两者操作同一个本地库。

---

## 1. 前置条件

- 平台：macOS / Linux（Windows 预编译包视 Release 资产而定）
- **一键安装**：仅需 `curl`、`tar`；从 [Releases](https://github.com/i3xai/iaichain/releases) 拉对应平台包
- **源码构建**（可选）：[Rust 1.83+](https://rustup.rs)
- 无需数据库服务；节点侧无强制中心服务（SQLite 已 bundled）。多节点演示可起本地 `iai relay`。

## 2. 安装

### 2.1 一键安装（推荐）

从 GitHub Releases 下载当前系统对应的预编译 `iai` 并装入 PATH：

```sh
curl -fsSL https://raw.githubusercontent.com/i3xai/iaichain/main/install.sh | bash
```

常用选项：

```sh
# 指定版本（tag）
curl -fsSL https://raw.githubusercontent.com/i3xai/iaichain/main/install.sh | bash -s -- --version v0.4.2

# 安装到自定义目录
curl -fsSL https://raw.githubusercontent.com/i3xai/iaichain/main/install.sh | bash -s -- --dir ~/.local/bin
```

脚本行为摘要：

- 自动识别平台：`macos-aarch64` / `macos-x86_64` / `linux-x86_64` / `linux-aarch64`
- 下载 `iai-v<VER>-<TARGET>.tar.gz`，有 `.sha256` 则校验
- 默认装到可写的 `/usr/local/bin`，否则 `~/.local/bin`（必要时提示加入 PATH）

验证：

```sh
iai version
# 期望类似：iai-chain 0.4.2
```

> 若报 404：确认 `main` 上已有 [install.sh](https://github.com/i3xai/iaichain/blob/main/install.sh)，且对应 Release 含本机平台资产。  
> 维护者上传包：`scripts/publish.sh --upload`。

也可克隆仓库后本地执行：

```sh
git clone https://github.com/i3xai/iaichain.git
cd iaichain
bash install.sh --version v0.4.2
```

### 2.2 源码构建（开发 / 无预编译包时）

```sh
# 在仓库根目录
cargo build --release          # 开发期用 cargo build（更快，产物 target/debug/iai）

# 把二进制放进 PATH（任选其一）
export PATH="$PWD/target/release:$PATH"     # 临时（当前 shell）
# 或
cp target/release/iai /usr/local/bin/       # 永久（需写权限）
```

验证：`iai version` → `iai-chain 0.4.2`。

### 2.3 手动下载 Release

打开 [Releases](https://github.com/i3xai/iaichain/releases)，下载 `iai-v*-<TARGET>.tar.gz`，解压后将其中的 `iai` 放到 PATH。

## 3. 五分钟上手

```sh
iai model add openai --key sk-xxxx     # 配置模型（anthropic / ollama 同理）
iai serve                              # 启动；浏览器打开 http://127.0.0.1:8787
```

- 落地页：`http://127.0.0.1:8787/`
- 控制台：`http://127.0.0.1:8787/console`

不想开浏览器？所有功能都有等价 CLI（下一节）。

多节点（中继 + 队长 + 队员）抄写命令见 [START.md](START.md)。

## 4. CLI 工作流

> 数据目录可用环境变量切换：`IAI_HOME=/path/to/dir iai <cmd>`（便于隔离 / 多节点演示）。

### 4.1 节点与模型
```sh
iai model add openai --key sk-xxx           # provider: openai|anthropic|ollama|<其他>
iai model add ollama --model qwen           # 本地 provider 无需 --key
iai model list                              # 列出已配置模型（不显示 key）
iai node status                             # 节点 id / 角色(队长) / 状态 / 模型 / 能力
```

### 4.2 团队与网络
```sh
iai team create --recruit "需要 Rust 限流中间件"           # 创建团队并发布招募
iai team invite --node node.4a91 --role 后端 --model "Claude 3.5" --credits 2180
iai team invite --node node.cc70 --role 文档 --model "本地 Qwen" --offline   # 标记离线
iai team list                                            # 成员表（本机在前，按贡献降序）
iai net                                                  # 在线成员 / 已知节点 / 公开团队
```

### 4.3 发起任务（核心闭环）
```sh
iai task run --repo github.com/acme/auth-lib --prompt "实现一个 Rust JWT 鉴权模块，附文档"
# 流程：解析 → 分解(按角色) → 匹配节点 → 执行(质量门禁) → 聚合(采纳) → 结算(贡献点分发)
iai task list                                # 任务列表（状态 / 子任务完成数）
iai task status <task-id>                    # 详情：各角色节点 + 状态 + 聚合结果
```
非 GitHub 仓库会被守卫拒绝：`Error: 仅支持公开 GitHub 仓库（地址需含 github.com）`。

### 4.4 贡献币市场
```sh
iai market sell --px 0.90 --qty 100          # 挂卖单（--node 可指定卖方，默认本机）
iai market book                              # 挂卖簿（价格升序，最低价在前）
iai market buy --qty 120                     # 按最低价逐笔吃单，成交计入账本
```

### 4.5 钱包与账本
```sh
iai wallet                                   # 可用余额 / 任务锁定 / 本周收益（账本推导）
iai ledger list --limit 20                   # 近期流水（最新在前）
iai ledger verify                            # 重算哈希链，校验防篡改完整性
iai ledger record --kind settle --amount 180 --note "手工调账"   # 运维/演示记账
#   --kind: settle|reward|lock|unlock|buy|sell ；--locked 调整锁定池；--node 指定账户
```

## 5. Web 控制台（`iai serve` 后访问 /console）

| 视图 | 内容 |
|---|---|
| 概览 | 本机节点负载/模型、网络在线、钱包、当前任务、贡献币最低价 |
| 任务 | 发起表单 + 角色化任务卡（运行中/已采纳，运行中任务 2s 自动刷新）|
| 市场 | 挂卖簿 + 按最低价买入 + 24h 价格图 |
| 团队 | 成员节点表（角色/模型/在线/累计贡献）|
| 钱包 | 余额 / 锁定 / 本周收益 / 哈希链流水 |

## 6. 本地 HTTP API 参考（AI 调用友好）

基址 `http://127.0.0.1:8787`。请求/响应均为 JSON。

| 方法 | 路径 | 请求体 | 说明 |
|---|---|---|---|
| GET | `/api/health` | — | `{"status":"ok","node":"iai-chain"}` |
| GET | `/api/version` | — | `{"name":"iai-chain","version":"0.4.2"}` |
| GET | `/api/node` | — | 本机节点 `{id,role,online,load,models[],capabilities[],modelConfigured}` |
| GET / POST | `/api/node/models` | `{provider,model?,key?}` | 列出 / 新增模型（不回传 key）|
| GET | `/api/wallet` | — | `{balance,locked,weekly,lockedTasks,weeklyAccepted}`（本机视角）|
| GET | `/api/ledger` | — | 流水数组 `[{time,type,note,delta}]` |
| GET | `/api/market/book` | — | 挂卖簿 `[{px,qty,node}]`（价格升序）|
| GET | `/api/market/price` | — | 价格点 `[{i,px}]` |
| POST | `/api/market/buy` | `{qty}` | 按最低价撮合，返回 `{orders,filled,cost}` |
| POST | `/api/market/sell` | `{px,qty,node?}` | 挂卖单 |
| GET | `/api/team` | — | `[[name,role,model,online01,creditsStr]]` |
| POST | `/api/team/invite` | `{node,role,model,credits?,online?}` | 邀请成员 |
| POST | `/api/team/recruit` | `{name?,recruit}` | 创建团队并招募 |
| GET | `/api/network` | — | `{membersOnline,discovered,publicTeams}` |
| GET | `/api/tasks` | — | 任务卡 `[{id,t,repo,st,pct,roles}]` |
| POST | `/api/tasks` | `{prompt,repo}` | 发起任务（同步建+异步执行），返回 `{ok,taskId}` |
| GET | `/api/tasks/:id` | — | 任务详情（含 `state` 与聚合 `result`）|

curl 示例：
```sh
# 发起任务
curl -s -X POST http://127.0.0.1:8787/api/tasks \
  -H 'Content-Type: application/json' \
  -d '{"prompt":"实现一个 Rust JWT 鉴权模块","repo":"github.com/acme/auth-lib"}'
# → {"ok":true,"taskId":"task.xxxxxxxx"}

# 轮询任务状态（运行中 → 已采纳/已结算）
curl -s http://127.0.0.1:8787/api/tasks

# 按最低价买入贡献币
curl -s -X POST http://127.0.0.1:8787/api/market/buy \
  -H 'Content-Type: application/json' -d '{"qty":120}'
```

## 7. 配置与数据

| 项 | 默认 | 说明 |
|---|---|---|
| 数据目录 | `~/.iai` | 用 `IAI_HOME` 覆盖；含 `iai.db`（SQLite）|
| 端口 | `8787` | `iai serve --port <n>` |
| 绑定地址 | `127.0.0.1` | 仅回环，不对外暴露 |
| 日志级别 | `info` | `RUST_LOG=error iai serve` 调整 |

## 8. 故障排查

| 现象 | 处理 |
|---|---|
| `iai: command not found` | 未加入 PATH；用 `./target/release/iai` 或 `export PATH=...` |
| 端口被占用 | `iai serve --port 8888` |
| 任务一直「运行中」 | 进程被中断会使异步执行停止；`iai serve` 持续运行或用 `iai task run`（同步跑完）|
| `仅支持公开 GitHub 仓库` | `--repo` 需含 `github.com` |
| 想从干净状态开始 | 删除数据目录：`rm -rf ~/.iai`（不可恢复）|

## 9. 重置 / 卸载

```sh
rm -rf ~/.iai                 # 清空所有本地数据（账本、市场、团队、任务）
rm -f /usr/local/bin/iai      # 若曾安装到系统目录
```

---

更多：[README.md](README.md)（架构总览）· [DEVELOPMENT-PLAN.md](DEVELOPMENT-PLAN.md)（阶段计划）·
[specs/001-task-orchestration](specs/001-task-orchestration)（规格与契约）。
