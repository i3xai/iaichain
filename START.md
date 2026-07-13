# IAI Chain · 快速开始（START）

拿到 `iai` 命令，并在本机用「中继 + 队长 + 队员」跑通 Phase 1 演示。

在线版手册：https://iaiaiai.ai/docs/  

更完整的命令与 API 说明见 [USAGE.md](USAGE.md)；产品规格见 [specs/STATUS.md](specs/STATUS.md)。

---

## 1. 怎么得到 `iai` 命令

### 方式 A：一键安装脚本（推荐）

预编译包由维护者打进 GitHub Releases 后，本机执行：

```sh
curl -fsSL https://raw.githubusercontent.com/i3xai/iaichain/main/install.sh | bash
```

常用选项：

```sh
# 指定版本
curl -fsSL https://raw.githubusercontent.com/i3xai/iaichain/main/install.sh | bash -s -- --version v0.4.5

# 安装到自定义目录
curl -fsSL https://raw.githubusercontent.com/i3xai/iaichain/main/install.sh | bash -s -- --dir ~/.local/bin
```

脚本会按当前系统选择资产（如 `macos-aarch64` / `linux-x86_64`），校验 `.sha256`（若有），并安装 `iai`。  
默认装到可写的 `/usr/local/bin`，否则 `~/.local/bin`（必要时提示你把该目录加入 PATH）。

验证：

```sh
iai version
# 期望类似：iai-chain 0.4.5
```

### 升级（已安装过）

```sh
iai upgrade check              # 仅检查
iai upgrade run -y             # 升到最新 Release
iai upgrade run --to v0.4.5 -y # 指定版本
iai --version
```

也可再次执行上方 `install.sh`（等价重装）。
> 若下载失败，说明该平台的 Release 资产尚未上传。可改用下方源码构建，或请维护者执行：
> `scripts/publish.sh --upload`（产物名：`iai-v<VER>-<TARGET>.tar.gz`）。

### 方式 B：从源码构建

前置：[Rust 1.83+](https://rustup.rs)（`cargo --version` 能跑即可）。

```sh
git clone https://github.com/i3xai/iaichain.git
cd iaichain
cargo build --release
```

产物：`target/release/iai`。

装进 PATH（任选其一）：

```sh
# 当前终端临时可用
export PATH="$PWD/target/release:$PATH"

# 或拷到系统目录（需写权限）
cp target/release/iai /usr/local/bin/
```

### 方式 C：手动下载 Release 包

1. 打开：[https://github.com/i3xai/iaichain/releases](https://github.com/i3xai/iaichain/releases)
2. 按系统下载对应 `iai-v*-*.tar.gz`
3. 解压后得到目录内的 `iai`，放到 PATH：

```sh
tar -xzf iai-v*-*.tar.gz
# 包内一般为 iai-v<VER>-<TARGET>/iai
sudo mv iai-v*/iai /usr/local/bin/
iai version
```

### 方式 D：仓库内直接跑相对路径

```sh
./target/release/iai version
```

下文命令里的 `iai` 也可一律换成该路径。
---

## 2. 三个进程分别做什么

双机（其实是本机两个数据目录）演示需要开 **3 个终端**：

| # | 命令 | 角色 | 做什么 |
|---|------|------|--------|
| 0 | `iai relay --port 8790` | **协调中继** | 任务公告板：发布/列表、原子领取、入队申请与批准状态。不跑前端、不记账。 |
| 1 | `IAI_HOME=~/.iai-cap IAI_RELAY=… iai serve --port 8787` | **队长节点** | 本地 API + 控制台。发任务、审批入队。数据在 `~/.iai-cap`。 |
| 2 | `IAI_HOME=~/.iai-mem IAI_RELAY=… iai serve --port 8788` | **队员节点** | 另一个独立节点。申请入队、领取网络任务槽。数据在 `~/.iai-mem`。 |

关系示意：

```text
  ┌─────────────┐     公告 / 领取 / 入队状态      ┌──────────────────┐
  │  队长 serve │ ◄────────────────────────────► │  iai relay :8790  │
  │  :8787      │                                 │  （协调中继）      │
  └─────────────┘                                 └────────▲─────────┘
                                                           │
  ┌─────────────┐     同上（IAI_RELAY 指向中继）            │
  │  队员 serve │ ◄───────────────────────────────────────┘
  │  :8788      │
  └─────────────┘
```

要点：

- **`IAI_HOME` 必须不同**，否则队长和队员会共用同一个 SQLite，演示无效。
- **`IAI_RELAY` 两边都要设**，否则无法跨节点申请入队 / 领网络任务。
- 节点 API **只绑 `127.0.0.1`**；中继绑 `0.0.0.0`，供本机多实例连接。

---

## 3. 一键抄写：本机双节点演示

```sh
# 终端 0 · 中继
iai relay --port 8790

# 终端 1 · 队长
IAI_HOME=~/.iai-cap IAI_RELAY=http://127.0.0.1:8790 iai serve --port 8787

# 终端 2 · 队员
IAI_HOME=~/.iai-mem IAI_RELAY=http://127.0.0.1:8790 iai serve --port 8788
```

浏览器：

| 角色 | 地址 |
|------|------|
| 队长控制台 | http://127.0.0.1:8787/console |
| 队员控制台 | http://127.0.0.1:8788/console |

首次启动会生成控制台密码（看启动日志或 `iai password show`，注意各自的 `IAI_HOME`）。

### 建议操作顺序（Phase 1）

1. 队长：`iai node status`（在 `IAI_HOME=~/.iai-cap` 下）记下队长 `node_id`。
2. 队员：控制台 → 团队 →「申请 / 审批」→ 填入队长 `node_id` → 提交申请。
3. 队长：同一面板批准申请。
4. 队长：创建 `visibility=network` 的任务（带开放角色槽）。
5. 队员：网络任务列表 → 领取槽位。未批准应被拒绝；重复领取应冲突。

---

## 4. 可选：配置模型

Phase 1 入队/领取不强制真实 LLM；Phase 2 协作执行再需要。

```sh
# 示例（在对应 IAI_HOME 下执行）
IAI_HOME=~/.iai-cap iai model add ollama --model llama3
# 或
IAI_HOME=~/.iai-cap iai model add openai --key sk-...
```

---

## 5. 常见问题

| 现象 | 处理 |
|------|------|
| `command not found: iai` | 检查 PATH，或用 `./target/release/iai` |
| 申请入队报未配置中继 | 启动 `serve` 时带上 `IAI_RELAY=http://127.0.0.1:8790` |
| 领取 403 | 队长尚未批准入队 |
| 领取 409 | 槽已被别人领走 |
| 两个端口数据串了 | 确认两个 `IAI_HOME` 路径不同 |

---

## 6. 相关文档

- [README.md](README.md) — 项目总览
- [USAGE.md](USAGE.md) — 完整使用与 API
- [install.sh](install.sh) — 一键从 Release 安装
- [scripts/publish.sh](scripts/publish.sh) — 维护者打多平台包并上传 Release
- [specs/003-open-collab-market/quickstart.md](specs/003-open-collab-market/quickstart.md) — 规格侧快启

### 维护者：把编译包放到 GitHub

```sh
# 本机构建 + Linux 多架构（见 publish.sh 说明），并上传 Release
scripts/publish.sh --upload

# 或只打本机包再手动上传
scripts/publish.sh --targets host
gh release create v0.4.5 ./dist/* --title "v0.4.5"
```

上传成功后，用户即可用文首的 `curl … | bash` 安装。