// 节点控制台逻辑（ES module）。数据一律经 /shared/api.js 接缝层取得。
//
// 此模块不再自启动：所有顶层 await 与 DOM 初始化都集中在 `init()` 函数里，
// 由 `auth.js` 在登录成功后再调用——避免未登录用户进入页面时顶层 fetch 触发 401。
import {
  getMarketBook, getPriceSeries, buyAtLowest, getTasks, createTask,
  getTeam, getLedger, getVersion, getNode, getWallet, getNetwork, authLogout,
  checkRepo, composeTask, getTask, getTaskLog, getModelInstances,
  getNetworkTasks, claimSlot, autoMatch, getHosted, setHosted,
  applyJoinTeam, getJoinRequests, decideJoinRequest,
} from "/shared/api.js";

var reduce = window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;

// 模块状态（在 init() 中填充）
var orders = [];
var tasks = [];
var team = [];
var ledger = [];
var currentFilter = "all";
var pollTimer = null;
var inited = false;

export async function init() {
  if (inited) return;
  inited = true;
  bindNav();
  bindActions();
  bindLogout();

  // 并行拉首屏数据
  var got = await Promise.allSettled([
    getMarketBook(), getTasks(), getTeam(), getLedger(), getVersion(), getNode(), getWallet(), getNetwork(),
  ]);
  if (got[0].status === "fulfilled") orders = got[0].value;
  if (got[1].status === "fulfilled") tasks = got[1].value;
  if (got[2].status === "fulfilled") team = got[2].value;
  if (got[3].status === "fulfilled") ledger = got[3].value;

  if (got[4].status === "fulfilled") {
    var f = document.getElementById("footVer"); if (f) f.textContent = "v" + got[4].value.version;
  }
  if (got[5].status === "fulfilled") renderNode(got[5].value);
  if (got[6].status === "fulfilled") renderWallet(got[6].value);
  if (got[7].status === "fulfilled") renderNetwork(got[7].value);

  // 渲染
  renderAsk();
  renderTasks(currentFilter);
  renderOverviewTasks();
  renderTeam();
  renderLedger();
  ensurePolling();
  lineChart("#ov-spark", 56, false);
  bindTaskModal();
  bindTaskDetail();
  bindNetwork();
  wireTeamJoin();
}

/* ───────────── 任务详情弹框（阶段 9：角色槽 + 贡献分 + 操作日志） ───────────── */

function bindTaskDetail() {
  var modal = document.getElementById("detailModal");
  if (!modal) return;
  var list = document.getElementById("taskList");
  if (list) list.addEventListener("click", function (e) {
    var card = e.target.closest(".task"); if (!card) return;
    var id = card.getAttribute("data-tid"); if (id) openTaskDetail(id);
  });
  document.getElementById("dmClose").addEventListener("click", function () { modal.hidden = true; });
  modal.addEventListener("click", function (e) { if (e.target === modal) modal.hidden = true; });
}

async function openTaskDetail(id) {
  var $ = function (x) { return document.getElementById(x); };
  var esc = function (s) { return String(s == null ? "" : s).replace(/[&<>]/g, function (c) { return { "&": "&amp;", "<": "&lt;", ">": "&gt;" }[c]; }); };
  try {
    var d = await getTask(id);
    var log = await getTaskLog(id);
    $("dmTitle").textContent = d.t || "任务详情";
    $("dmMeta").innerHTML =
      '<div class="dm-row"><span class="dm-k">状态</span><span class="dm-v">' + esc(d.state || d.st) + " · " + d.pct + '%</span></div>' +
      '<div class="dm-row"><span class="dm-k">仓库</span><span class="dm-v">' + esc(d.repo) + "</span></div>";
    var assigns = d.assignments || [];
    $("dmAssigns").innerHTML = assigns.length ? assigns.map(function (a) {
      var st = a.status === "done" ? '<span class="tag done">完成</span>' : a.status === "working" ? '<span class="spin"></span>' : '<span class="qty" style="font-size:11px">' + esc(a.status) + "</span>";
      return '<div class="aslot"><span class="tag role">' + esc(a.role) + '</span><span class="a-node">' + esc(a.node || "未领取") + '</span><span class="a-model">' + esc(a.model || "") + "</span>" + st + '<span class="a-tok">' + (a.tokens || 0) + " tok</span></div>";
    }).join("") : '<div class="empty" style="padding:8px 0">暂无</div>';
    var rewards = d.rewards || [];
    $("dmRewards").innerHTML = rewards.length ? rewards.map(function (r) {
      return '<div class="dm-row"><span class="dm-k">' + esc(r.role) + " · " + esc(r.node) + '</span><span class="dm-v" style="color:var(--gold)">+' + r.credits + " 币 (" + esc(r.basis) + ")</span></div>";
    }).join("") : '<div class="empty" style="padding:8px 0">未结算</div>';
    $("dmLog").innerHTML = log.length ? log.map(function (l) {
      return '<div class="tl-item"><span class="tl-act">' + esc(l.action) + '</span><span class="tl-detail">' + esc(l.detail || "") + "</span></div>";
    }).join("") : '<div class="empty" style="padding:8px 0">无日志</div>';
    document.getElementById("detailModal").hidden = false;
  } catch (e) { /* 静默 */ }
}

/* ───────────── 任务创建弹框（阶段 8） ───────────── */

function bindTaskModal() {
  var modal = document.getElementById("taskModal");
  if (!modal) return;
  var $ = function (id) { return document.getElementById(id); };
  var repoChecked = false, repoKind = "opensource", vis = "network", balance = 0;

  function setMsg(t, cls) { var m = $("tmMsg"); m.textContent = t || ""; m.className = "modal-msg" + (cls ? " " + cls : ""); }
  function setChk(t, cls) { var c = $("tmCheckMsg"); c.textContent = t || ""; c.className = "chk" + (cls ? " " + cls : ""); }
  function setSeg(id, val, attr) { [].slice.call($(id).children).forEach(function (b) { b.classList.toggle("on", b.getAttribute(attr) === val); }); }
  function showRepoFields() { $("tmOpensource").hidden = repoKind !== "opensource"; $("tmInternal").hidden = repoKind !== "internal"; }

  function addRoleCard() {
    var d = document.createElement("div");
    d.className = "role-card";
    d.innerHTML =
      '<div class="rc-top"><input class="rc-name" placeholder="角色名，如 后端"><button type="button" class="rc-del">删除</button></div>' +
      '<textarea class="rc-prompt" placeholder="该角色负责的事情（提示词）"></textarea>' +
      '<div class="rc-meta"><label>招募数<input class="rc-count" type="number" min="1" value="1"></label>' +
      '<label>模型筛选<input class="rc-filter" placeholder="any / claude-3-5-sonnet" value="any"></label></div>';
    d.querySelector(".rc-del").addEventListener("click", function () { if ($("tmRoles").children.length > 1) { d.remove(); updateBtn(); } });
    d.querySelector(".rc-name").addEventListener("input", updateBtn);
    $("tmRoles").appendChild(d);
  }
  function collectRoles() {
    return [].slice.call($("tmRoles").children).map(function (c) {
      return {
        name: c.querySelector(".rc-name").value.trim(),
        prompt: c.querySelector(".rc-prompt").value.trim(),
        recruit_count: parseInt(c.querySelector(".rc-count").value, 10) || 1,
        model_filter: c.querySelector(".rc-filter").value.trim() || "any",
      };
    }).filter(function (r) { return r.name; });
  }
  function updateBtn() {
    var title = $("tmTitle").value.trim();
    var reward = parseInt($("tmReward").value, 10) || 0;
    var roles = collectRoles();
    if (reward > balance) setMsg("奖励金超过可用余额 " + balance, "bad"); else setMsg("");
    $("tmCreate").disabled = !(title && repoChecked && roles.length > 0 && reward >= 0 && reward <= balance);
  }

  async function open() {
    ["tmTitle", "tmUrl", "tmHost", "tmPath", "tmBranch"].forEach(function (id) { $(id).value = ""; });
    $("tmReward").value = "0";
    repoChecked = false; setChk(""); setMsg("");
    $("tmRoles").innerHTML = ""; addRoleCard();
    repoKind = "opensource"; setSeg("tmRepoKind", "opensource", "data-k"); showRepoFields();
    vis = "network"; setSeg("tmVis", "network", "data-v");
    try { var w = await getWallet(); balance = w.balance || 0; } catch (e) { balance = 0; }
    $("tmBalance").textContent = "可用 " + balance + " 币";
    updateBtn();
    modal.hidden = false;
  }
  function close() { modal.hidden = true; }

  $("tmRepoKind").addEventListener("click", function (e) { var b = e.target.closest("button"); if (!b) return; repoKind = b.getAttribute("data-k"); setSeg("tmRepoKind", repoKind, "data-k"); showRepoFields(); repoChecked = false; setChk(""); updateBtn(); });
  $("tmVis").addEventListener("click", function (e) { var b = e.target.closest("button"); if (!b) return; vis = b.getAttribute("data-v"); setSeg("tmVis", vis, "data-v"); });

  $("tmCheck").addEventListener("click", async function () {
    var body = { kind: repoKind, branch: $("tmBranch").value.trim() };
    if (repoKind === "opensource") { body.url = $("tmUrl").value.trim(); if (!body.url) { setChk("请填仓库地址", "bad"); return; } }
    else { body.host = $("tmHost").value.trim(); body.path = $("tmPath").value.trim(); if (!body.host || !body.path) { setChk("请填服务器与目录", "bad"); return; } }
    setChk("检测中…", "wait"); this.disabled = true;
    try {
      var r = await checkRepo(body);
      if (r.ok) { repoChecked = true; setChk("✓ 连通 · 分支 " + (r.branches || []).slice(0, 5).join("/"), "ok"); }
      else { repoChecked = false; setChk("✗ " + (r.error || "无法连通"), "bad"); }
    } catch (e) { repoChecked = false; setChk("✗ " + e.message, "bad"); }
    this.disabled = false; updateBtn();
  });

  $("tmAddRole").addEventListener("click", function () { addRoleCard(); updateBtn(); });
  $("tmTitle").addEventListener("input", updateBtn);
  $("tmReward").addEventListener("input", updateBtn);

  $("tmCreate").addEventListener("click", async function () {
    var body = {
      title: $("tmTitle").value.trim(),
      reward: parseInt($("tmReward").value, 10) || 0,
      visibility: vis,
      roles: collectRoles(),
      repo: { kind: repoKind, branch: $("tmBranch").value.trim(), url: $("tmUrl").value.trim(), host: $("tmHost").value.trim(), path: $("tmPath").value.trim() },
    };
    this.disabled = true; setMsg("创建中…");
    try {
      var r = await composeTask(body);
      setMsg("✓ 已创建任务 " + r.taskId, "ok");
      tasks = await getTasks(); renderTasks(currentFilter); renderOverviewTasks();
      setTimeout(close, 800);
    } catch (e) { setMsg("✗ " + e.message, "bad"); this.disabled = false; }
  });

  var nt = $("newTask"); if (nt) nt.addEventListener("click", function (e) { e.preventDefault(); open(); });
  $("tmClose").addEventListener("click", close);
  $("tmCancel").addEventListener("click", close);
  modal.addEventListener("click", function (e) { if (e.target === modal) close(); });
}

/* ───────────── nav + actions ───────────── */

function bindNav() {
  document.addEventListener("click", function (e) {
    var t = e.target.closest("[data-view]");
    if (t) show(t.dataset.view);
  });
  var menuBtn = document.getElementById("menuBtn");
  if (menuBtn) {
    menuBtn.addEventListener("click", function () {
      var open = document.getElementById("side").classList.toggle("open");
      menuBtn.setAttribute("aria-expanded", open ? "true" : "false");
    });
  }
  document.querySelectorAll("[data-view]").forEach(function (el) {
    if (el.tagName !== "BUTTON") { el.setAttribute("role", "button"); el.setAttribute("tabindex", "0"); }
  });
  document.addEventListener("keydown", function (e) {
    if (e.key !== "Enter" && e.key !== " ") return;
    var t = e.target.closest("[data-view]");
    if (t && t.tagName !== "BUTTON") { e.preventDefault(); show(t.dataset.view); }
  });
}

function show(v) {
  document.querySelectorAll(".view").forEach(function (s) { s.classList.toggle("on", s.dataset.view === v); });
  document.querySelectorAll(".nav-item").forEach(function (n) {
    var on = n.dataset.view === v;
    n.classList.toggle("on", on);
    if (on) { n.setAttribute("aria-current", "page"); } else { n.removeAttribute("aria-current"); }
  });
  document.getElementById("side").classList.remove("open");
  if (v === "market") { drawMarket(); renderNetworkTasks(); }
  if (v === "models") renderModels();
}

/* ───────────── 网络任务公告板（阶段 10b：领取 / 自动匹配 / 托管） ───────────── */

async function renderNetworkTasks() {
  var box = document.getElementById("netTasks");
  if (!box) return;
  var r = await getNetworkTasks();
  if (!r.relay) { box.innerHTML = '<div class="empty" style="padding:14px 0">未配置中继（IAI_RELAY）· 单机模式</div>'; return; }
  if (!r.tasks.length) { box.innerHTML = '<div class="empty" style="padding:14px 0">网络上暂无可领取任务</div>'; return; }
  box.innerHTML = r.tasks.map(function (t) {
    var slots = t.openSlots.map(function (s) {
      return '<div class="aslot"><span class="tag role">' + s.role + '</span><span class="a-model">' + s.modelFilter + '</span><button class="btn btn-ghost claim-btn" data-slot="' + s.slotId + '" style="margin-left:auto;padding:4px 10px;font-size:11px">领取</button></div>';
    }).join("");
    return '<div class="task" style="margin-bottom:10px"><div class="top"><span class="ttl">' + t.title + '</span><span class="tag" style="color:var(--gold);border-color:transparent;background:oklch(83% 0.13 80 / .12)">奖金 ' + t.reward + '</span><span class="repo">' + t.repo + ' · ' + t.publisher + '</span></div>' + slots + '</div>';
  }).join("");
}

function bindNetwork() {
  var amBtn = document.getElementById("autoMatchBtn");
  if (amBtn) amBtn.addEventListener("click", async function () {
    var self = this, old = this.textContent;
    this.disabled = true; this.textContent = "匹配中…";
    try { var r = await autoMatch(); self.textContent = r.claimed ? "已领取" : "暂无匹配"; }
    catch (e) { self.textContent = "失败"; }
    await renderNetworkTasks();
    setTimeout(function () { self.textContent = old; self.disabled = false; }, 1500);
  });
  var hosted = document.getElementById("hostedToggle");
  if (hosted) {
    getHosted().then(function (h) { hosted.checked = !!h; });
    hosted.addEventListener("change", async function () {
      try { await setHosted(this.checked); } catch (e) { this.checked = !this.checked; }
    });
  }
  var box = document.getElementById("netTasks");
  if (box) box.addEventListener("click", async function (e) {
    var b = e.target.closest(".claim-btn"); if (!b) return;
    b.disabled = true; b.textContent = "领取中…";
    try { await claimSlot(b.getAttribute("data-slot")); b.textContent = "已领取"; }
    catch (e2) { b.textContent = "已被领取"; }
    setTimeout(renderNetworkTasks, 800);
  });
}

async function renderModels() {
  var box = document.getElementById("modelBody");
  if (!box) return;
  var rows = await getModelInstances();
  if (!rows.length) { box.innerHTML = '<tr><td colspan="6"><div class="empty" style="padding:18px 0">暂无模型工作记录 · 发起任务后出现</div></td></tr>'; return; }
  box.innerHTML = rows.map(function (m) {
    var st = m.status === "busy" ? '<span class="tag run">忙</span>' : '<span class="tag done">闲</span>';
    var s = m.workSeconds || 0;
    var dur = s < 60 ? s + "s" : s < 3600 ? Math.round(s / 60) + "m" : (s / 3600).toFixed(1) + "h";
    return '<tr><td><span class="mono">' + m.node + '</span></td><td>' + m.model + '</td><td>' + st + '</td><td class="qty">' + (m.currentTask || "—") + '</td><td class="px tnum">' + (m.tokensUsed || 0).toLocaleString() + '</td><td class="qty">' + dur + '</td></tr>';
  }).join("");
}

function bindActions() {
  document.getElementById("buyQty").addEventListener("input", estimate);
  document.getElementById("buyBtn").addEventListener("click", async function () {
    var need = parseInt(document.getElementById("buyQty").value, 10) || 0;
    var self = this;
    try {
      var res = await buyAtLowest(orders, need);
      orders = res.orders; renderAsk();
      renderWallet();
      self.textContent = "成交 " + res.filled + " 币 · ¥" + Number(res.cost).toFixed(2);
    } catch (e) {
      self.textContent = "成交失败";
    }
    setTimeout(function () { self.textContent = "按最低价买入"; }, 2200);
  });

  document.getElementById("taskFilter").addEventListener("click", function (e) {
    var b = e.target.closest("button"); if (!b) return;
    this.querySelectorAll("button").forEach(function (x) { x.classList.toggle("on", x === b); });
    currentFilter = b.dataset.f; renderTasks(currentFilter);
  });

  document.getElementById("taskRunBtn").addEventListener("click", async function () {
    var prompt = document.getElementById("taskPrompt").value.trim();
    var repo = document.getElementById("taskRepo").value.trim();
    var msg = document.getElementById("taskRunMsg");
    if (!prompt) { msg.style.display = ""; msg.innerHTML = '<span style="color:var(--amber)">请先描述需求</span>'; return; }
    this.disabled = true;
    try {
      await createTask(prompt, repo);
      document.getElementById("taskPrompt").value = "";
      msg.style.display = ""; msg.innerHTML = '<b>已发起</b><span>任务已分派给团队节点，执行中…</span>';
      await refreshTasks();
      ensurePolling();
    } catch (e) {
      msg.style.display = ""; msg.innerHTML = '<span style="color:var(--red)">发起失败：' + e.message + '</span>';
    }
    this.disabled = false;
  });
}

function bindLogout() {
  var btn = document.getElementById("logoutBtn");
  if (!btn) return;
  btn.addEventListener("click", async function () {
    btn.disabled = true;
    await authLogout();
    // auth.js 监听到 401 也会兜底清 token；这里直接刷新走 auth 流程即可。
    location.reload();
  });
}

/* ───────────── order book ───────────── */

function lowest() { return orders.slice().sort(function (a, b) { return a.px - b.px; })[0]; }
function renderAsk() {
  var tb = document.getElementById("askBody"); if (!tb) return; tb.innerHTML = "";
  var sorted = orders.slice().sort(function (a, b) { return a.px - b.px; });
  if (sorted.length === 0) {
    tb.innerHTML = '<tr><td colspan="3"><div class="empty"><span class="ic">◷</span>挂单已被买空 · 等待卖方挂出新单</div></td></tr>';
    ["ov-px", "mk-px"].forEach(function (id) { var el = document.getElementById(id); if (el) el.textContent = "—"; });
    estimate(); return;
  }
  sorted.forEach(function (o, i) {
    var tr = document.createElement("tr"); if (i === 0) tr.className = "best";
    tr.innerHTML = '<td class="px tnum">¥' + o.px.toFixed(2) + '</td><td class="qty tnum">' + o.qty + '</td><td class="qty">' + o.node + "</td>";
    tb.appendChild(tr);
  });
  var lo = sorted[0];
  ["ov-px", "mk-px"].forEach(function (id) { var el = document.getElementById(id); if (el) el.textContent = "¥" + lo.px.toFixed(2); });
  estimate();
}
function estimate() {
  var btn = document.getElementById("buyBtn");
  var need = parseInt(document.getElementById("buyQty").value, 10) || 0;
  var avail = orders.reduce(function (s, o) { return s + o.qty; }, 0);
  var s = orders.slice().sort(function (a, b) { return a.px - b.px; }), cost = 0, n = need;
  for (var i = 0; i < s.length && n > 0; i++) { var t = Math.min(n, s[i].qty); cost += t * s[i].px; n -= t; }
  var est = document.getElementById("buyEst");
  if (avail === 0) { est.textContent = "暂无挂单可成交"; }
  else if (need <= 0) { est.textContent = "请输入买入数量"; }
  else { est.textContent = "预计 ≈ ¥" + cost.toFixed(2) + (n > 0 ? "（仅 " + (need - n) + " 可成交）" : ""); }
  if (btn) btn.disabled = (need <= 0 || avail === 0);
}

/* ───────────── tasks ───────────── */

function roleChip(r) {
  var st = r[1] === "done" ? '<span class="tag done">已采纳</span>' : r[1] === "run" ? '<span class="spin"></span>' : '<span class="qty" style="font-size:11px">排队</span>';
  return '<div class="role-chip"><span class="tag role">' + r[0] + '</span><span class="st">' + st + "</span></div>";
}
function renderTasks(f) {
  var box = document.getElementById("taskList"); box.innerHTML = "";
  var list = tasks.filter(function (t) { return f === "all" || t.st === f; });
  if (list.length === 0) { box.innerHTML = '<div class="empty"><span class="ic">▤</span>该筛选下暂无任务</div>'; return; }
  list.forEach(function (t) {
    var roles = t.roles.map(roleChip).join("");
    var badge = t.st === "done" ? '<span class="tag done">已采纳 · 发起方获得功能</span>' : '<span class="tag run">运行中 ' + t.pct + "%</span>";
    box.innerHTML += '<div class="task" data-tid="' + (t.id || "") + '" style="cursor:pointer"><div class="top"><span class="ttl">' + t.t + "</span>" + badge + '<span class="repo">' + t.repo + ' (public)</span></div><div class="roles">' + roles + "</div></div>";
  });
}
function renderOverviewTasks() {
  var box = document.getElementById("ovTasks"); if (!box) return;
  var top = tasks.slice(0, 3);
  if (top.length === 0) { box.innerHTML = '<div class="empty" style="padding:14px 0">暂无任务 · 去任务页发起</div>'; return; }
  box.innerHTML = top.map(function (t) {
    var badge = t.st === "done" ? '<span class="tag done">已采纳</span>' : '<span class="tag run">运行中 ' + t.pct + '%</span>';
    return '<div class="kv"><span class="k">' + t.t + '</span><span class="v">' + badge + '</span></div>';
  }).join("");
}
async function refreshTasks() {
  try { tasks = await getTasks(); renderTasks(currentFilter); renderOverviewTasks(); } catch (_) {}
}
function ensurePolling() {
  var hasRun = tasks.some(function (t) { return t.st === "run"; });
  if (hasRun && !pollTimer) {
    pollTimer = setInterval(async function () {
      await refreshTasks();
      if (!tasks.some(function (t) { return t.st === "run"; })) { clearInterval(pollTimer); pollTimer = null; }
    }, 2000);
  }
}

/* ───────────── team ───────────── */

function renderTeam() {
  var tb = document.getElementById("teamBody"); tb.innerHTML = "";
  team.forEach(function (m) {
    var av = m[0].indexOf("captain") > -1 ? "C" : m[0].slice(5, 7).toUpperCase();
    tb.innerHTML += '<tr><td><div class="who"><span class="av">' + av + '</span><span class="mono">' + m[0] + '</span></div></td><td><span class="tag role">' + m[1] + '</span></td><td class="qty">' + m[2] + '</td><td><span class="dot-s ' + (m[3] ? "on" : "off") + '"></span>' + (m[3] ? "在线" : "离线") + '</td><td class="px tnum">' + m[4] + "</td></tr>";
  });
}

async function refreshJoinRequests() {
  var box = document.getElementById("joinReqList");
  if (!box) return;
  try {
    var data = await getJoinRequests();
    var reqs = (data && data.requests) || [];
    var pending = reqs.filter(function (r) { return r.status === "pending"; });
    if (!pending.length) {
      box.className = "empty";
      box.style.padding = "4px 0";
      box.textContent = "暂无待审批申请";
      return;
    }
    box.className = "";
    box.innerHTML = pending.map(function (r) {
      return '<div class="aslot" style="margin:6px 0">' +
        '<span class="mono">' + r.applicantNodeId + '</span>' +
        '<span class="tag role">' + (r.role || "开发") + '</span>' +
        '<button class="btn btn-ghost join-approve" data-node="' + r.applicantNodeId + '" style="margin-left:auto;padding:4px 10px;font-size:11px">批准</button>' +
        '<button class="btn btn-ghost join-reject" data-node="' + r.applicantNodeId + '" style="padding:4px 10px;font-size:11px">拒绝</button></div>';
    }).join("");
  } catch (e) {
    box.textContent = "加载失败：" + (e.message || e);
  }
}

function wireTeamJoin() {
  var panel = document.getElementById("teamJoinPanel");
  var btn = document.getElementById("btnTeamPanel");
  if (btn && panel) {
    btn.addEventListener("click", function () {
      var on = panel.style.display !== "none";
      panel.style.display = on ? "none" : "block";
      if (!on) refreshJoinRequests();
    });
  }
  var applyBtn = document.getElementById("btnApplyJoin");
  if (applyBtn) {
    applyBtn.addEventListener("click", async function () {
      var msg = document.getElementById("joinApplyMsg");
      var captain = (document.getElementById("joinCaptainId") || {}).value || "";
      var role = (document.getElementById("joinRole") || {}).value || "开发";
      try {
        await applyJoinTeam({ captainNodeId: captain.trim(), role: role.trim() });
        if (msg) msg.textContent = "已提交申请，等待队长批准";
      } catch (e) {
        if (msg) msg.textContent = e.message || String(e);
      }
    });
  }
  var list = document.getElementById("joinReqList");
  if (list) {
    list.addEventListener("click", async function (e) {
      var a = e.target.closest(".join-approve");
      var r = e.target.closest(".join-reject");
      var node = (a || r) && (a || r).getAttribute("data-node");
      if (!node) return;
      try {
        await decideJoinRequest({ applicantNodeId: node, approve: !!a });
        await refreshJoinRequests();
        try { team = await getTeam(); renderTeam(); } catch (_) {}
      } catch (err) {
        alert(err.message || String(err));
      }
    });
  }
}

/* ───────────── ledger ───────────── */

function renderLedger() {
  var tb = document.getElementById("ledgerBody"); if (!tb) return; tb.innerHTML = "";
  ledger.forEach(function (l) {
    var col = l.delta.charAt(0) === "+" ? "var(--green)" : "var(--amber)";
    tb.innerHTML += '<tr><td class="qty">' + l.time + '</td><td><span class="tag">' + l.type + '</span></td><td>' + l.note + '</td><td class="tnum" style="text-align:right;color:' + col + ';font-family:var(--font-mono)">' + l.delta + "</td></tr>";
  });
}

/* ───────────── charts ───────────── */

async function lineChart(sel, h, withArea) {
  if (typeof d3 === "undefined") return;
  var svg = d3.select(sel); if (svg.empty()) return;
  var el = document.querySelector(sel), W = el.clientWidth || 400;
  var data = await getPriceSeries(lowest() ? lowest().px : 0.86); var N = data.length;
  svg.attr("viewBox", "0 0 " + W + " " + h).selectAll("*").remove();
  var pad = { t: 10, r: 6, b: 6, l: 6 }, x = d3.scaleLinear().domain([0, N - 1]).range([pad.l, W - pad.r]);
  var ex = d3.extent(data, function (d) { return d.px; }), y = d3.scaleLinear().domain([ex[0] - .02, ex[1] + .02]).range([h - pad.b, pad.t]);
  if (withArea) {
    svg.append("defs").append("linearGradient").attr("id", "g_" + sel.slice(1)).attr("x1", 0).attr("y1", 0).attr("x2", 0).attr("y2", 1)
      .call(function (g) { g.append("stop").attr("offset", "0%").attr("stop-color", "oklch(83% 0.13 80)").attr("stop-opacity", .28); g.append("stop").attr("offset", "100%").attr("stop-color", "oklch(83% 0.13 80)").attr("stop-opacity", 0); });
    var area = d3.area().x(function (d) { return x(d.i); }).y0(h - pad.b).y1(function (d) { return y(d.px); }).curve(d3.curveMonotoneX);
    svg.append("path").datum(data).attr("d", area).attr("fill", "url(#g_" + sel.slice(1) + ")");
  }
  var line = d3.line().x(function (d) { return x(d.i); }).y(function (d) { return y(d.px); }).curve(d3.curveMonotoneX);
  svg.append("path").datum(data).attr("d", line).attr("fill", "none").attr("stroke", "var(--gold)").attr("stroke-width", 2);
  var last = data[N - 1]; svg.append("circle").attr("cx", x(last.i)).attr("cy", y(last.px)).attr("r", 3.5).attr("fill", "var(--gold)");
  var first = data[0].px, chg = ((last.px - first) / first * 100);
  var id = sel === "#ov-spark" ? "ov-chg" : "mk-chg"; var ce = document.getElementById(id);
  if (ce) { ce.textContent = (chg >= 0 ? "▲ +" : "▼ ") + chg.toFixed(1) + "%"; ce.style.color = chg >= 0 ? "var(--green)" : "var(--red)"; }
}
function drawMarket() { lineChart("#mk-chart", 150, true); }

/* ───────────── node / wallet / network ───────────── */

function setPill(id, online, text) {
  var el = document.getElementById(id); if (!el) return;
  el.className = online ? "pill online" : "pill";
  el.innerHTML = '<span class="dot"></span>' + text;
}
function renderNode(n) {
  var role = n.role || "队长";
  var models = n.models || [];
  var setTxt = function (id, t) { var el = document.getElementById(id); if (el) el.textContent = t; };
  setTxt("barRole", role);
  setTxt("barNodeId", n.id);
  setTxt("footNode", n.id);
  setTxt("nodeLoad", n.load);
  var meter = document.getElementById("nodeMeter"); if (meter) meter.style.width = (n.load || 0) + "%";
  setTxt("nodeModels", (models.length ? "模型 " + models.join(" · ") : "未配置模型（iai model add …）") + "｜角色：" + role);
  setPill("barPill", n.online, n.online ? (n.modelConfigured ? "在线 · 模型已配置" : "在线 · 未配置模型") : "离线");
  setPill("nodePill", n.online, n.online ? "在线" : "离线");
}
async function renderWallet(w) {
  if (!w) { try { w = await getWallet(); } catch (_) { return; } }
  var fmt = function (n) { return Number(n).toLocaleString(); };
  var setTxt = function (id, t) { var el = document.getElementById(id); if (el) el.textContent = t; };
  var setHtml = function (id, h) { var el = document.getElementById(id); if (el) el.innerHTML = h; };
  setTxt("wbal", fmt(w.balance));
  setTxt("ovWalletBal", fmt(w.balance));
  setHtml("ovWalletSub", '本周 <span style="color:var(--green)">+' + w.weekly + '</span> · 已锁定 ' + w.locked + '（任务中）');
  setTxt("walBal", fmt(w.balance));
  setTxt("walLocked", fmt(w.locked));
  setTxt("walLockedSub", w.lockedTasks + " 个进行中任务");
  setTxt("walWeekly", "+" + w.weekly);
  setTxt("walWeeklySub", w.weeklyAccepted + " 笔被采纳的提交");
}
async function renderNetwork(s) {
  if (!s) { try { s = await getNetwork(); } catch (_) { return; } }
  var setTxt = function (id, t) { var el = document.getElementById(id); if (el) el.textContent = t; };
  setTxt("netMembers", s.membersOnline);
  setTxt("netDiscovered", s.discovered);
  setTxt("netTeams", s.publicTeams);
}
