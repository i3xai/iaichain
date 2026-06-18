# IAI Chain 开发计划（从设计页面到可运行产品）

> 目标：把 `design/` 下两个设计页面（`index.html` 落地页、`console.html` 节点控制台）所展示的全部功能，
> 逐步实现为**真实可运行的全栈产品** —— 前端保持「增强版原生 HTML/CSS/JS」，后端为 `specs/001-task-orchestration`
> 规划的 Rust 四层节点引擎。
>
> 本计划按**垂直切片**组织：每个阶段都产出一个可演示、可验收的成果，且严格衔接上一阶段，不留断头路。

---

## 1. 设计页面盘点（要实现的功能清单）

### 落地页 `index.html`（营销/概念展示，`launcher-overview` 角色）
- Hero：节点网络 canvas 动画 + 终端逐字符打字动画（模拟一次真实任务运行）
- 价值区：∞ / P2P / 100% 三段文案
- 工作原理：4 步流程 + 中心「队长」节点 SVG 关系图
- 核心能力：6 张功能卡（组队、节点发现、角色分派、公开开源、贡献币、自由市场）
- 贡献币经济：**D3 价格走势图** + **实时挂卖簿**（按最低价顺序吃单的买入逻辑）
- 安装：macOS / Linux / Windows / Cargo 多 Tab 代码片段 + 复制
- CTA + 页脚

### 控制台 `console.html`（产品主界面，`screen` 角色）—— 5 个视图
| 视图 | 功能 |
|---|---|
| 概览 | 本机节点负载/模型、网络在线数、钱包余额、当前任务、贡献币最低价 spark |
| 任务 | 角色化任务卡（角色 chip + 运行/已采纳/排队状态）、筛选、新建任务 |
| 市场 | 挂卖簿、按最低价买入（含成交估算/余额变动）、24h 价格图、我的挂卖/求购 |
| 团队 | 成员节点表（节点/角色/模型/在线状态/累计贡献）、招募/邀请 |
| 钱包 | 可用余额、任务锁定、本周收益、哈希链流水 |

> 当前这些功能**全部由前端内存里的假数据驱动**（`orders`、`tasks`、`team`、`ledger` 等数组）。
> 本计划的核心工作，就是把这些假数据逐个替换为后端引擎产生的真实数据，同时把后端从零建起来。

---

## 2. 目标架构与「接缝」设计

```
┌─────────────────────────────────────────────┐
│  前端（原生 HTML/CSS/JS，零构建依赖）          │
│  landing/index.html   console/console.html    │
│  shared/design-tokens.css  shared/api.js ◄──── 唯一接缝层
└───────────────┬─────────────────────────────┘
                │  HTTP / SSE（本地回环 127.0.0.1）
┌───────────────▼─────────────────────────────┐
│  iai-cli：`iai serve` 内嵌静态资源 + 本地 API │
│           （axum + rust-embed）+ clap 子命令   │
├───────────────────────────────────────────────┤
│  iai-core    任务生命周期 / 分解 / 匹配 / 聚合 / 质量门禁 │
│  iai-node    节点注册 / P2P 发现 / Provider 适配         │
│  iai-economic 哈希链账本 / 贡献点 / 市场定价             │
└───────────────────────────────────────────────┘
              本地 SQLite（rusqlite）
```

### 让「每次开发都衔接上一次」的关键约定

1. **API 契约先行且稳定**：第 0 阶段就把前端需要的所有端点列成 `api.js` 里的函数（`getNode()`、`getWallet()`、`getMarketBook()`、`buyAtLowest()`、`getTasks()`、`getTeam()` …）。
   每个函数有固定的入参/返回结构，**契约不变**。
2. **`api.js` 是唯一接缝**：第 0 阶段这些函数**先返回设计页里已有的假数据**（原样搬过来）。
   之后每个阶段只做一件事 —— 把其中一两个函数从「返回假数据」翻转为「`fetch` 真实端点」。
   翻转后页面行为不变，但数据变真。**前端任何时候都能跑，永不阻塞于后端进度。**
3. **后端按 crate 单向依赖落地**：`iai-cli → iai-core → {iai-node, iai-economic}`，与 `plan.md` 的章程分层一致。
4. **每阶段验收 = 一个可复现的 demo 命令 + 一次界面截图**，对照设计页确认像素与行为一致。

---

## 3. 分阶段计划

> 节奏建议：每阶段一个可合并的分支，结尾跑通验收 demo + 截图比对。前端改动均落在 `web/`，后端落在 `crates/`。

### 阶段 0 · 地基与接缝（Foundation）
**后端**
- 建立 Cargo workspace 与 4 个空 crate（`iai-cli / iai-core / iai-node / iai-economic`）。
- `iai serve` 子命令：axum 起本地 HTTP 服务，用 `rust-embed` 内嵌 `web/` 静态资源；`GET /api/health`、`GET /api/version` 返回真实版本。
- SQLite 初始化与 migration 脚手架（空表骨架）。

**前端**
- 把 `index.html` / `console.html` 拆分为 `web/landing/` 与 `web/console/`（screen-file-first，保持两个独立 surface）。
- 抽取 `web/shared/design-tokens.css`（color/type/spacing/radius/shadow/motion，来自 `DESIGN.md`），两页共用。
- 把内联 JS 拆成 ES module：`net-canvas.js`、`terminal.js`、`market.js`、`charts.js`、`console-views.js` 等。
- **新建 `web/shared/api.js`**：列全所有端点函数，**全部先 `return` 设计页里的假数据**。所有页面改为从 `api.js` 取数（行为零变化）。

**验收**：`iai serve` 启动后浏览器打开落地页与控制台，外观/动画/交互与原设计一致；`/api/health`、`/api/version` 返回真实数据。

---

### 阶段 1 · 节点身份与模型配置（垂直切片）
衔接：第一次让 `api.js` 的 `getNode()` 翻转为真实端点。

**后端**（iai-node）
- 节点身份实体：`node_id / role / capabilities / models / load / online`，落 SQLite，符合 `contracts/node-contract.md`。
- `iai model add <provider> --key …`、`iai node status`。
- API：`GET /api/node`、`GET /api/node/models`、`POST /api/node/models`。

**前端**
- 控制台 topbar 的节点 id / 「在线·模型已配置」pill、概览页「本机节点」负载卡 → 改用 `getNode()` 真实数据。
- 落地页安装区第二块「配置大模型」示例与 `iai --version` 输出对齐真实版本。

**验收**：`iai model add openai --key sk-…` 后刷新控制台，节点卡显示真实角色、已配置模型、负载。

---

### 阶段 2 · 钱包与哈希链账本（经济地基）
衔接：`getWallet()`、`getLedger()` 翻转为真实端点；为后续市场/结算提供记账底座。

**后端**（iai-economic）
- `ledger.rs`：append-only + 前序哈希链（tamper-evident，满足 FR-010/013），可独立核验。
- `credit.rs`：贡献币余额、锁定额、收益统计（由账本推导）。
- `iai wallet`、`iai ledger`、`iai ledger verify`（校验哈希链完整性）。
- API：`GET /api/wallet`、`GET /api/ledger`。

**前端**
- 控制台「钱包」视图（可用余额 / 任务锁定 / 本周收益 / 近期流水表）、概览「钱包」卡、topbar 钱包余额 → 真实数据。

**验收**：手动写入几条账本记录，控制台流水与余额正确；`iai ledger verify` 通过；篡改一条后校验失败。

---

### 阶段 3 · 贡献币市场（挂卖簿 + 最低价撮合 + 价格图）
衔接：`getMarketBook()`、`getPriceSeries()`、`buyAtLowest()`、`sell()` 翻转；买入动作落到第 2 阶段账本，余额联动。

**后端**（iai-economic）
- `market.rs`：挂卖簿（asks）、**「从最低价逐笔向上吃单」撮合引擎**（与设计页买入逻辑完全一致）、价格历史序列。
- `iai market`、`iai market sell --px --qty`、`iai market buy --qty`。
- API：`GET /api/market/book`、`GET /api/market/price?range=24h`、`POST /api/market/buy`、`POST /api/market/sell`。

**前端**
- 控制台「市场」视图（挂卖簿、买入估算/成交、D3 价格图、我的挂卖/求购）+ 概览最低价 spark + **落地页经济区**的挂卖簿与 D3 图 → 全部接真实端点。
- 买入成交后服务端更新余额，前端钱包同步（与阶段 2 联动）。

**验收**：在控制台按最低价买入 120 币 → 服务端撮合 → 账本新增买入流水 → 钱包余额变化；挂卖簿被吃空时显示设计里的空状态。

---

### 阶段 4 · 团队、节点注册与 P2P 发现
衔接：`getTeam()`、`getNetwork()` 翻转；为阶段 5 的角色分派准备「成员节点」。

**后端**（iai-node）
- `registry.rs`：节点注册/发现；团队创建、招募广播、定向邀请。
- P2P 发现：先用 mDNS/局域网 + 可选 tracker 起步（真实跨网 P2P 作为后续特性），满足「无中心服务器」方向。
- `iai team create --recruit "…"`、`iai team`、`iai net scan`。
- API：`GET /api/team`、`GET /api/network`、`POST /api/team/recruit`、`POST /api/team/invite`。

**前端**
- 控制台「团队」视图（成员节点表：节点/角色/模型/在线/累计贡献）+ 概览「网络」卡（在线成员数、P2P 发现节点数）→ 真实数据。

**验收**：两台机器（或两个本地实例）互相发现并组队，团队名册与在线状态真实；招募广播可被另一节点扫描到。

---

### 阶段 5 · 任务编排（核心垂直切片）
衔接：`getTasks()`、`createTask()`、`getTask(id)` 翻转 + 引入实时进度推送；调用阶段 4 的角色节点与阶段 1 的 Provider。

**后端**（iai-core + iai-node/providers）
- 七态生命周期 `lifecycle.rs`（Created→Parsed→Decomposed→Matched→Executed→Aggregated→Settled）+ 每次转移写审计记录。
- `decompose.rs`（提示词→按角色拆子任务）、`matcher.rs`（综合评分匹配 FR-004）、`orchestrator.rs`、`aggregate.rs`、`quality.rs`（结算前质量门禁 FR-008）。
- `providers/`：reqwest 调 OpenAI / Anthropic / 本地 Ollama，执行各角色子任务。
- 失败重匹配（FR-009，单节点失败不致整体失败）。
- `iai task run --repo github.com/acme/public-repo`（仅公开仓库守卫）、`iai task status`。
- API：`GET /api/tasks`、`POST /api/tasks`、`GET /api/tasks/:id`、`GET /api/tasks/:id/events`（SSE 实时进度）。

**前端**
- 控制台「任务」视图（任务卡 + 角色 chip 的 运行/已采纳/排队 三态 + 筛选）、概览「当前任务」卡、「+ 新建任务」流程 → 真实数据 + SSE 实时刷新运行百分比与角色状态。

**验收**：提交一条自然语言任务（绑定公开 GitHub 仓库）→ 自动分解为多角色子任务 → 分派到团队节点 → 各节点大模型执行 → 聚合 → 质量门禁 → 控制台实时看到角色从「运行中」转「已采纳」。

---

### 阶段 6 · 结算闭环 + 实时化 + 落地页真实化
衔接：把阶段 5 的「采纳」接到阶段 2/3 的账本与市场，闭合 `Task → … → Ledger → Market` 整条链。

**后端**
- 任务被采纳 → 触发贡献点分发（`credit.rs`，FR-011）→ 写账本 → 钱包/市场联动更新。
- SSE/WebSocket 广播：账本、挂卖簿、任务进度、团队在线状态实时推送（驱动设计里的 `LIVE` 徽标与 spinner）。

**前端**
- 控制台所有 `LIVE`/进度/余额变化走实时推送。
- 落地页 Hero 终端动画改为**回放一次真实样例运行**（脚本来自真实 `iai task run` 的录制输出），安装区版本/命令对齐真实二进制。

**验收**：跑通端到端：发起任务 → 协作执行 → 采纳 → `+320 贡献币` 自动结算分配到 3 个节点 → 钱包与流水实时更新（对应设计页终端最后两行与钱包流水）。

---

### 阶段 7 · 打包、响应式/可达性、测试与加固
**打包与跨平台**
- `iai serve` 内嵌静态资源，产出**单一静态二进制**（macOS Intel/ARM、Linux、Windows）；落地页安装脚本/winget/brew/cargo 渠道对齐。

**响应式与可达性**（对照 `DESIGN-MANIFEST.json` 的 9 档视口）
- 360×800 / 390×844 / 430×932 / 600×960 / 820×1180 / 1024×768 / 1366×768 / 1440×900 / 1920×1080 全部无横向滚动。
- 保留 focus-visible、`prefers-reduced-motion`、键盘可操作（控制台 nav 已有 role/tabindex 基础）。

**测试与安全**（对齐章程质量门禁）
- 契约测试 `tests/contract/`（CLI 命令 + 节点契约 + API 端点）、集成测试 `tests/integration/`（生命周期/容错/结算/账本核验）。
- 安全：模型 key 安全存储、仅公开仓库守卫、本地 API 仅绑回环、撮合与结算的金额一致性。
- 性能对齐 SC-008（提交即返回标识；供给充足时 95% 任务 ≤ 5 分钟到 Settled；CLI 本地命令 < 200ms）。

**验收**：`cargo test`/`nextest` 全绿；9 档视口截图比对通过；单二进制在三平台启动即用。

---

## 4. 阶段依赖与衔接关系

```
0 地基/接缝 ─┬─► 1 节点身份 ─────────────┐
             ├─► 2 钱包/账本 ─► 3 市场 ───┤
             └─► 4 团队/P2P ──────────────┴─► 5 任务编排 ─► 6 结算闭环+实时 ─► 7 打包/测试/加固
```
- 阶段 1–4 相对独立，可并行推进（都只依赖阶段 0 的接缝与 API 契约）。
- 阶段 5 依赖 1（Provider/节点）、4（角色节点）。
- 阶段 6 依赖 2、3、5（把执行结果接进账本与市场，闭环）。

## 5. 关键决策点（已确认 / 待定）
- ✅ 范围：全栈（前端 + Rust 节点引擎）。
- ✅ 前端栈：增强现有原生 HTML/CSS/JS，零构建依赖，保留 d3。
- ⏳ P2P 起步形态：阶段 4 先 mDNS/局域网 + 可选 tracker，真实跨网 NAT 穿透列为后续特性（可在进入阶段 4 时再定）。
- ⏳ 默认 Provider 与样例仓库：进入阶段 5 前确定一个可演示的公开仓库与默认模型。

## 6. 立即可执行的下一步
进入**阶段 0**：搭 workspace + `iai serve`（内嵌静态资源）+ 拆分 `web/` 目录 + 落地 `web/shared/api.js`（先返回假数据）。
完成后，落地页与控制台即可由真实二进制托管，且接缝就位 —— 后续每个阶段只需「翻转一个 api 函数 + 实现对应后端端点」。
