/* IAI Chain · 前端唯一「接缝层」(api.js)
 *
 * 设计要点（见 DEVELOPMENT-PLAN.md §2）：
 *   - 前端任何取数都只经过本文件，页面逻辑不直接碰 fetch 或硬编码数组。
 *   - 阶段 0：除 getVersion()/getHealth() 已对接真实后端外，其余函数先返回设计稿里的
 *     假数据（Promise 形式，模拟将来的网络调用）。
 *   - 后续每个阶段，只需把对应函数从「返回假数据」翻转为「fetch 真实端点」，
 *     页面行为不变、数据变真。每个函数上方标注了它将在哪个阶段被翻转。
 *
 * 约定：所有函数返回 Promise；调用方一律 await。
 */

const BASE = "";

async function getJSON(path) {
  const res = await fetch(BASE + path, { headers: { Accept: "application/json" } });
  if (!res.ok) throw new Error(`${path} -> HTTP ${res.status}`);
  return res.json();
}

/* ───────────── 阶段 0：已对接真实后端 ───────────── */

/** 节点版本（落地页安装区 / 控制台页脚据此展示真实版本）。 */
export async function getVersion() {
  try {
    return await getJSON("/api/version"); // { name, version }
  } catch {
    return { name: "iai-chain", version: "0.4.2" }; // 离线/静态预览兜底
  }
}

/** 健康检查。 */
export async function getHealth() {
  try {
    return await getJSON("/api/health");
  } catch {
    return { status: "offline" };
  }
}

/* ───────────── 阶段 1：已对接真实后端（节点 / 模型） ───────────── */

/** 本机节点状态。返回 { id, role, online, load, models[], capabilities[], modelConfigured }。 */
export async function getNode() {
  try {
    return await getJSON("/api/node");
  } catch {
    // 离线/静态预览兜底（保持设计稿观感）
    return { id: "captain.7f3a", role: "队长", online: false, load: 0, models: [], capabilities: [], modelConfigured: false };
  }
}

/** 已配置模型列表（不含 key）。 */
export async function getModels() {
  try {
    const r = await getJSON("/api/node/models");
    return r.models || [];
  } catch {
    return [];
  }
}

/** 新增模型配置：POST /api/node/models { provider, model?, key? }。 */
export async function addModel(body) {
  const res = await fetch(BASE + "/api/node/models", {
    method: "POST",
    headers: { "Content-Type": "application/json", Accept: "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    const e = await res.json().catch(() => ({}));
    throw new Error(e.error || `HTTP ${res.status}`);
  }
  return res.json();
}

/* ───────────── 阶段 4：已对接真实后端（网络 / 团队） ───────────── */

/** 网络概况。返回 { membersOnline, discovered, publicTeams }。 */
export async function getNetwork() {
  try {
    return await getJSON("/api/network");
  } catch {
    return { membersOnline: 0, discovered: 0, publicTeams: 0 };
  }
}

/** 邀请 / 登记成员节点：POST /api/team/invite。 */
export async function inviteMember(body) {
  const res = await fetch(BASE + "/api/team/invite", {
    method: "POST",
    headers: { "Content-Type": "application/json", Accept: "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

/* ───────────── 阶段 2：已对接真实后端（钱包 / 账本） ───────────── */

/** 钱包视图。返回 { balance, locked, weekly, lockedTasks, weeklyAccepted }。 */
export async function getWallet() {
  try {
    return await getJSON("/api/wallet");
  } catch {
    return { balance: 0, locked: 0, weekly: 0, lockedTasks: 0, weeklyAccepted: 0 };
  }
}

/** 账本流水（最新在前）。返回 [{ time, type, note, delta }]。 */
export async function getLedger() {
  try {
    return await getJSON("/api/ledger");
  } catch {
    return [];
  }
}

/* ───────────── 阶段 3：已对接真实后端（市场） ───────────── */

/** 挂卖簿（价格升序）。返回 [{ px, qty, node }]。 */
export async function getMarketBook() {
  try {
    return await getJSON("/api/market/book");
  } catch {
    return [];
  }
}

/** 价格走势序列。返回 [{ i, px }]。
 *  真实价格点 ≥2 时直接用；不足时退化为以 endPx 收尾的随机游走（保持设计观感）。 */
export async function getPriceSeries(endPx) {
  try {
    const pts = await getJSON("/api/market/price");
    if (Array.isArray(pts) && pts.length >= 2) return pts;
  } catch {
    /* 离线兜底走下方合成序列 */
  }
  const N = 64;
  const data = [];
  let p = 0.78;
  for (let i = 0; i < N; i++) {
    p += (Math.random() - 0.48) * 0.018;
    p = Math.max(0.62, Math.min(1.05, p));
    data.push({ i, px: p });
  }
  if (endPx && endPx > 0) data[N - 1].px = endPx;
  return data;
}

/** 按最低价买入：POST /api/market/buy，由服务端撮合 + 记账（FR-012）。
 *  返回 { orders（新簿）, filled, cost }。`orders` 形参仅为兼容旧签名，撮合以服务端为准。 */
export async function buyAtLowest(orders, need) {
  const res = await fetch(BASE + "/api/market/buy", {
    method: "POST",
    headers: { "Content-Type": "application/json", Accept: "application/json" },
    body: JSON.stringify({ qty: need }),
  });
  if (!res.ok) {
    const e = await res.json().catch(() => ({}));
    throw new Error(e.error || `HTTP ${res.status}`);
  }
  return res.json(); // { orders, filled, cost }
}

/** 挂出卖单：POST /api/market/sell { px, qty, node? }。 */
export async function sellAsk(body) {
  const res = await fetch(BASE + "/api/market/sell", {
    method: "POST",
    headers: { "Content-Type": "application/json", Accept: "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    const e = await res.json().catch(() => ({}));
    throw new Error(e.error || `HTTP ${res.status}`);
  }
  return res.json();
}

/* ───────────── 阶段 5：已对接真实后端（任务） ───────────── */

/** 任务列表。返回 [{ id, t, repo, st, pct, roles }]。 */
export async function getTasks() {
  try {
    return await getJSON("/api/tasks");
  } catch {
    return [];
  }
}

/** 发起任务：POST /api/tasks { prompt, repo }（服务端解析→分解→匹配→异步执行）。 */
export async function createTask(prompt, repo) {
  const res = await fetch(BASE + "/api/tasks", {
    method: "POST",
    headers: { "Content-Type": "application/json", Accept: "application/json" },
    body: JSON.stringify({ prompt, repo }),
  });
  if (!res.ok) {
    const e = await res.json().catch(() => ({}));
    throw new Error(e.error || `HTTP ${res.status}`);
  }
  return res.json();
}

/** 团队成员节点。返回 [[name, role, model, online01, creditsStr]]。 */
export async function getTeam() {
  try {
    return await getJSON("/api/team");
  } catch {
    return [];
  }
}
