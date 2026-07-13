/* IAI Chain · 前端唯一「接缝层」(api.js)
 *
 * 设计要点（见 DEVELOPMENT-PLAN.md §2）：
 *   - 前端任何取数都只经过本文件，页面逻辑不直接碰 fetch 或硬编码数组。
 *   - 阶段 0：除 getVersion()/getHealth() 已对接真实后端外，其余函数先返回设计稿里的
 *     假数据（Promise 形式，模拟将来的网络调用）。
 *   - 后续每个阶段，只需把对应函数从「返回假数据」翻转为「fetch 真实端点」，
 *     页面行为不变、数据变真。每个函数上方标注了它将在哪个阶段被翻转。
 *   - 鉴权：除 `/api/auth/*`、`/api/health`、`/api/version` 外，其它 `/api/*` 都需要
 *     `Authorization: Bearer <token>`。Token 存 localStorage（`iai.console.token`），
 *     由 `setToken()` / `clearToken()` 管理；fetch 在收到 401 时触发回调（由
 *     console 的 auth.js 注册，用于回到登录页）。
 *
 * 约定：所有函数返回 Promise；调用方一律 await。
 */

const BASE = "";

/* ───────────── 鉴权 · token 管理 ───────────── */

const TOKEN_KEY = "iai.console.token";
const TOKEN_EXP_KEY = "iai.console.tokenExpires";
const _unauthListeners = new Set();

/** 注册「收到 401」回调（如：清 token + 回到登录页）。返回反注册函数。 */
export function onUnauthorized(fn) {
  _unauthListeners.add(fn);
  return () => _unauthListeners.delete(fn);
}

function fireUnauthorized(path) {
  for (const fn of _unauthListeners) {
    try { fn(path); } catch (_) { /* 监听器异常不影响其它 */ }
  }
}

/** 读取 localStorage 中的 token（如存在且未过期则返回，否则返回 null）。 */
export function getToken() {
  try {
    const t = localStorage.getItem(TOKEN_KEY);
    if (!t) return null;
    const exp = Number(localStorage.getItem(TOKEN_EXP_KEY) || "0");
    // 提前 30 秒判过期，避免「正好到期」导致请求才到一半
    if (exp && exp * 1000 - 30000 < Date.now()) {
      clearToken();
      return null;
    }
    return t;
  } catch {
    return null;
  }
}

export function setToken(token, expiresAt) {
  try {
    localStorage.setItem(TOKEN_KEY, token);
    if (expiresAt) localStorage.setItem(TOKEN_EXP_KEY, String(expiresAt));
  } catch { /* localStorage 不可用时静默 */ }
}

export function clearToken() {
  try {
    localStorage.removeItem(TOKEN_KEY);
    localStorage.removeItem(TOKEN_EXP_KEY);
  } catch { /* */ }
}

export function hasToken() {
  return getToken() !== null;
}

function authHeaders() {
  const t = getToken();
  return t ? { Authorization: `Bearer ${t}` } : {};
}

/* ───────────── 统一 fetch ───────────── */

class AuthError extends Error {
  constructor(path, payload) {
    super(`401 ${path}` + (payload && payload.message ? ` · ${payload.message}` : ""));
    this.name = "AuthError";
    this.path = path;
    this.payload = payload;
  }
}

async function getJSON(path) {
  const res = await fetch(BASE + path, {
    headers: { Accept: "application/json", ...authHeaders() },
  });
  if (res.status === 401) {
    const body = await res.json().catch(() => ({}));
    clearToken();
    fireUnauthorized(path);
    throw new AuthError(path, body);
  }
  if (!res.ok) throw new Error(`${path} -> HTTP ${res.status}`);
  return res.json();
}

async function postJSON(path, body) {
  const res = await fetch(BASE + path, {
    method: "POST",
    headers: { "Content-Type": "application/json", Accept: "application/json", ...authHeaders() },
    body: JSON.stringify(body),
  });
  if (res.status === 401) {
    const payload = await res.json().catch(() => ({}));
    clearToken();
    fireUnauthorized(path);
    throw new AuthError(path, payload);
  }
  if (!res.ok) {
    const e = await res.json().catch(() => ({}));
    throw new Error(e.error || e.message || `HTTP ${res.status}`);
  }
  return res.json();
}

/* ───────────── 鉴权接口（公开） ───────────── */

/** GET /api/auth/status —— 控制台是否启用密码保护（启动时已默认开启）。 */
export async function getAuthStatus() {
  try {
    return await getJSON("/api/auth/status"); // { passwordSet: bool }
  } catch {
    return { passwordSet: false };
  }
}

/** POST /api/auth/login —— 校验密码、签发 session token。 */
export async function authLogin(password) {
  const r = await postJSON("/api/auth/login", { password }); // { token, expiresAt, ttlSeconds }
  setToken(r.token, r.expiresAt);
  return r;
}

/** POST /api/auth/logout —— 注销当前 token（即使后端失败也清本地）。 */
export async function authLogout() {
  try { await postJSON("/api/auth/logout", {}); } catch (_) { /* 忽略 */ }
  clearToken();
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
  return postJSON("/api/node/models", body);
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
  return postJSON("/api/team/invite", body);
}

/** 在线空闲队员：GET /api/team/idle → { members: [...] }。 */
export async function getIdleMembers() {
  try {
    return await getJSON("/api/team/idle");
  } catch {
    return { members: [] };
  }
}

/** 设置本机角色：PUT /api/node { role: 'captain'|'member' }。 */
export async function setNodeRole(role) {
  return reqJSON("PUT", "/api/node", { role });
}

/** 申请加入队长团队：POST /api/team/join { captainNodeId, role?, model?, message? }。 */
export async function applyJoinTeam(body) {
  return postJSON("/api/team/join", body);
}

/** 队长查看入队申请：GET /api/team/join-requests → { requests: [...] }。 */
export async function getJoinRequests() {
  try {
    return await getJSON("/api/team/join-requests");
  } catch {
    return { requests: [] };
  }
}

/** 批准/拒绝入队：POST /api/team/join-requests/decide。 */
export async function decideJoinRequest(body) {
  return postJSON("/api/team/join-requests/decide", body);
}

/** 申请任务角色：POST /api/tasks/:id/recruit/apply。 */
export async function applyRecruit(taskId, body) {
  return postJSON(`/api/tasks/${encodeURIComponent(taskId)}/recruit/apply`, body);
}

/** 队长查看招募申请。 */
export async function getRecruitApplications(taskId) {
  try {
    return await getJSON(`/api/tasks/${encodeURIComponent(taskId)}/recruit/applications`);
  } catch {
    return { applications: [] };
  }
}

/** 队长审批招募申请。 */
export async function decideRecruit(taskId, body) {
  return postJSON(`/api/tasks/${encodeURIComponent(taskId)}/recruit/decide`, body);
}

/** 队长启动任务（招募完成后）。 */
export async function startTask(taskId) {
  return postJSON(`/api/tasks/${encodeURIComponent(taskId)}/start`, {});
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
  return postJSON("/api/market/buy", { qty: need });
}

/** 挂出卖单：POST /api/market/sell { px, qty, node? }。 */
export async function sellAsk(body) {
  return postJSON("/api/market/sell", body);
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
  return postJSON("/api/tasks", { prompt, repo });
}

/** 团队成员节点。返回 [[name, role, model, online01, creditsStr]]。 */
export async function getTeam() {
  try {
    return await getJSON("/api/team");
  } catch {
    return [];
  }
}

/* ───────────── 阶段 8：协作任务市场 V2（角色库 / 仓库检测 / 创建） ───────────── */

async function reqJSON(method, path, body) {
  const res = await fetch(BASE + path, {
    method,
    headers: { "Content-Type": "application/json", Accept: "application/json", ...authHeaders() },
    body: body ? JSON.stringify(body) : undefined,
  });
  if (res.status === 401) {
    const p = await res.json().catch(() => ({}));
    clearToken();
    fireUnauthorized(path);
    throw new AuthError(path, p);
  }
  if (!res.ok) {
    const e = await res.json().catch(() => ({}));
    throw new Error(e.error || e.message || `HTTP ${res.status}`);
  }
  return res.json();
}

/** 本机角色库 `[{id,name,prompt,isCaptain,modelFilter}]`（队长在前）。 */
export async function getRoles() {
  try {
    return await getJSON("/api/roles");
  } catch {
    return [];
  }
}
export async function addRole(body) {
  return postJSON("/api/roles", body);
}
export async function updateRole(id, body) {
  return reqJSON("PUT", `/api/roles/${id}`, body);
}
export async function deleteRole(id) {
  return reqJSON("DELETE", `/api/roles/${id}`);
}

/** 仓库连通性检测。返回 `{ ok, branches?|error? }`（不通也是 HTTP 200）。 */
export async function checkRepo(body) {
  return postJSON("/api/repo/check", body);
}

/** V2 任务创建（仓库+多角色+招募+奖金）。 */
export async function composeTask(body) {
  return postJSON("/api/tasks/compose", body);
}

/** 任务详情（含 assignments + rewards + state + result）。 */
export async function getTask(id) {
  return getJSON(`/api/tasks/${id}`);
}

/** 模型工作态（需求 8/9）。返回 [{node,model,status,currentTask,tokensUsed,workSeconds}]。 */
export async function getModelInstances() {
  try {
    return await getJSON("/api/models/instances");
  } catch {
    return [];
  }
}

/* ───────────── 阶段 10b：网络任务（中继领取 / 自动匹配 / 托管） ───────────── */

/** 网络可领取任务。返回 { relay, tasks:[{taskId,title,repo,reward,publisher,openSlots:[{slotId,role,modelFilter}]}] }。 */
export async function getNetworkTasks() {
  try {
    return await getJSON("/api/network/tasks");
  } catch {
    return { relay: false, tasks: [] };
  }
}

/** 领取网络任务槽。 */
export async function claimSlot(slotId) {
  return postJSON("/api/network/claim", { slot_id: slotId });
}

/** 一键自动匹配（选奖金最高的可匹配槽）。 */
export async function autoMatch() {
  return postJSON("/api/match/auto", {});
}

/** 托管开关状态。 */
export async function getHosted() {
  try {
    return (await getJSON("/api/match/hosted")).hosted;
  } catch {
    return false;
  }
}

/** 设置托管开关。 */
export async function setHosted(enabled) {
  return reqJSON("PUT", "/api/match/hosted", { enabled });
}

/** 任务操作日志。 */
export async function getTaskLog(id) {
  try {
    return await getJSON(`/api/tasks/${id}/log`);
  } catch {
    return [];
  }
}
