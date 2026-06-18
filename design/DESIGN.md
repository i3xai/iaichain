# IAI Chain · 设计系统（DESIGN.md）

> 唯一事实源。方向：深化「开发者基础设施」暗色风。参考 Cloudflare / Sentry / Rust 官网 / libp2p。
> 气质：可信、硬核、带去中心化「运动/宣言」立场。受众主体是工程师 —— AI 味零容忍。

## 色彩（OKLch）

| Token | 值 | 用途 |
|---|---|---|
| `--bg` | `oklch(15% 0.012 255)` | 页面底色（近黑微蓝） |
| `--bg-deep` | `oklch(11% 0.01 255)` | 终端 / 代码块 / 图表底 |
| `--surface` | `oklch(19% 0.014 255)` | 卡片 |
| `--surface-2` | `oklch(23% 0.016 255)` | 卡片悬浮 / 内嵌控件 |
| `--fg` | `oklch(95% 0.008 240)` | 主文字 |
| `--muted` | `oklch(68% 0.015 240)` | 次文字 |
| `--faint` | `oklch(54% 0.015 240)` | 元信息 / mono 标签 |
| `--border` | `oklch(28% 0.014 255)` | 分隔线 |
| `--border-bright` | `oklch(38% 0.02 255)` | 强调边框 / 悬浮 |
| `--accent` | `oklch(80% 0.15 195)` | **唯一主强调色（青）**，每屏最多两次 |
| `--gold` | `oklch(83% 0.13 80)` | 贡献币经济专属第二色 |
| `--green` | `oklch(80% 0.18 150)` | 终端 prompt / 成功状态 |
| `--red` | `oklch(68% 0.19 25)` | 下跌 / 危险状态 |

## 字体

- Display：`Space Grotesk`（700/500）—— 几何、有辨识度，**替掉系统字默认脸**
- Mono：`JetBrains Mono` —— 命令、ID、数值、价格
- Body：system-ui（中文走苹方/微软雅黑）
- 规则：display ≠ body；数值一律 `tabular-nums`

## 姿态规则

1. **去模板感**：删掉「网格背景 + 双径向光晕」套餐。Hero 用真实 AI 生成的节点网络纹理 + 单层暗角；其余区块干净纯色底。
2. **标题不用整条渐变字**：纯色 + 单个 accent 词。
3. **半径**：卡片 12–14px，控件 8–9px，几乎无大圆角。
4. **边框做活**：暗色靠 1px 边框 + 留白分区，不靠阴影。阴影只给终端/弹层。
5. **一个决定性动作**：Hero 载入编排（错峰入场）+ 终端逐字符真打字。其余只用滚动渐显。
6. **强调预算**：青色每屏≤2 次；金色只出现在贡献币语境；状态色只标真实状态。

## 文案语气

宣言式、锋利、工程师口吻。短句、有立场、拒绝营销腔。例：「算力不该闲着」「价格由市场说了算，不由我们」。

## 资源管线

诊断 `design-review` → 配色 `color-expert` → 素材 `fal-generate(flux-pro-ultra)` → 数据 `d3-visualization` → 动效 `emilkowalski-motion` → 实现 `frontend-design` → 验证 `design-review` + chrome-devtools。
