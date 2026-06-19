// 节点控制台逻辑（ES module）。数据一律经 ../shared/api.js 接缝层取得。
import { getMarketBook, getPriceSeries, buyAtLowest, getTasks, getTeam, getLedger, getVersion, getNode, getWallet } from "/shared/api.js";

var reduce = window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;

// ---- view nav ----
function show(v) {
  document.querySelectorAll(".view").forEach(function (s) { s.classList.toggle("on", s.dataset.view === v); });
  document.querySelectorAll(".nav-item").forEach(function (n) { var on = n.dataset.view === v; n.classList.toggle("on", on); if (on) { n.setAttribute("aria-current", "page"); } else { n.removeAttribute("aria-current"); } });
  document.getElementById("side").classList.remove("open");
  if (v === "market") drawMarket();
}
document.addEventListener("click", function (e) { var t = e.target.closest("[data-view]"); if (t) { show(t.dataset.view); } });
var menuBtn = document.getElementById("menuBtn");
menuBtn.addEventListener("click", function () { var open = document.getElementById("side").classList.toggle("open"); menuBtn.setAttribute("aria-expanded", open ? "true" : "false"); });
// keyboard operability for non-native view switchers (nav items + inline links)
document.querySelectorAll("[data-view]").forEach(function (el) { if (el.tagName !== "BUTTON") { el.setAttribute("role", "button"); el.setAttribute("tabindex", "0"); } });
document.addEventListener("keydown", function (e) { if (e.key !== "Enter" && e.key !== " ") return; var t = e.target.closest("[data-view]"); if (t && t.tagName !== "BUTTON") { e.preventDefault(); show(t.dataset.view); } });

// ---- order book（数据来自 api.getMarketBook） ----
var orders = await getMarketBook();
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
document.getElementById("buyQty").addEventListener("input", estimate);
document.getElementById("buyBtn").addEventListener("click", async function () {
  var need = parseInt(document.getElementById("buyQty").value, 10) || 0;
  var self = this;
  try {
    var res = await buyAtLowest(orders, need);
    orders = res.orders; renderAsk();
    renderWallet(); // 服务端已撮合并记账，刷新真实余额与流水
    self.textContent = "成交 " + res.filled + " 币 · ¥" + Number(res.cost).toFixed(2);
  } catch (e) {
    self.textContent = "成交失败";
  }
  setTimeout(function () { self.textContent = "按最低价买入"; }, 2200);
});

// ---- tasks（数据来自 api.getTasks） ----
var tasks = await getTasks();
function renderTasks(f) {
  var box = document.getElementById("taskList"); box.innerHTML = "";
  var list = tasks.filter(function (t) { return f === "all" || t.st === f; });
  if (list.length === 0) { box.innerHTML = '<div class="empty"><span class="ic">▤</span>该筛选下暂无任务</div>'; return; }
  list.forEach(function (t) {
    var roles = t.roles.map(function (r) {
      var st = r[1] === "done" ? '<span class="tag done">已采纳</span>' : r[1] === "run" ? '<span class="spin"></span>' : '<span class="qty" style="font-size:11px">排队</span>';
      return '<div class="role-chip"><span class="tag role">' + r[0] + '</span><span class="st">' + st + "</span></div>";
    }).join("");
    var badge = t.st === "done" ? '<span class="tag done">已采纳 · 发起方获得功能</span>' : '<span class="tag run">运行中 ' + t.pct + "%</span>";
    box.innerHTML += '<div class="task"><div class="top"><span class="ttl">' + t.t + "</span>" + badge + '<span class="repo">' + t.repo + ' (public)</span></div><div class="roles">' + roles + "</div></div>";
  });
}
document.getElementById("taskFilter").addEventListener("click", function (e) { var b = e.target.closest("button"); if (!b) return; this.querySelectorAll("button").forEach(function (x) { x.classList.toggle("on", x === b); }); renderTasks(b.dataset.f); });

// ---- team（数据来自 api.getTeam） ----
var team = await getTeam();
function renderTeam() {
  var tb = document.getElementById("teamBody"); tb.innerHTML = "";
  team.forEach(function (m) {
    var av = m[0].indexOf("captain") > -1 ? "C" : m[0].slice(5, 7).toUpperCase();
    tb.innerHTML += '<tr><td><div class="who"><span class="av">' + av + '</span><span class="mono">' + m[0] + '</span></div></td><td><span class="tag role">' + m[1] + '</span></td><td class="qty">' + m[2] + '</td><td><span class="dot-s ' + (m[3] ? "on" : "off") + '"></span>' + (m[3] ? "在线" : "离线") + '</td><td class="px tnum">' + m[4] + "</td></tr>";
  });
}

// ---- ledger（数据来自 api.getLedger） ----
var ledger = await getLedger();
function renderLedger() {
  var tb = document.getElementById("ledgerBody"); if (!tb) return; tb.innerHTML = "";
  ledger.forEach(function (l) {
    var col = l.delta.charAt(0) === "+" ? "var(--green)" : "var(--amber)";
    tb.innerHTML += '<tr><td class="qty">' + l.time + '</td><td><span class="tag">' + l.type + '</span></td><td>' + l.note + '</td><td class="tnum" style="text-align:right;color:' + col + ';font-family:var(--font-mono)">' + l.delta + "</td></tr>";
  });
}

// ---- charts（价格序列来自 api.getPriceSeries） ----
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

// ---- footer version（已对接真实后端 /api/version） ----
(async function () {
  try { var v = await getVersion(); var f = document.getElementById("footVer"); if (f) f.textContent = "v" + v.version; } catch (e) { /* 离线兜底保留默认 */ }
})();

// ---- 本机节点（已对接真实后端 /api/node） ----
function setPill(id, online, text) {
  var el = document.getElementById(id); if (!el) return;
  el.className = online ? "pill online" : "pill";
  el.innerHTML = '<span class="dot"></span>' + text;
}
(async function renderNode() {
  try {
    var n = await getNode();
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
  } catch (e) { /* 离线兜底：保留页面默认占位 */ }
})();

// ---- 钱包（已对接真实后端 /api/wallet，余额/锁定/本周由账本推导） ----
async function renderWallet() {
  try {
    var w = await getWallet();
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
  } catch (e) { /* 离线兜底：保留页面默认占位 */ }
}

// ---- init ----
renderAsk(); renderTasks("all"); renderTeam(); renderLedger(); renderWallet();
lineChart("#ov-spark", 56, false);
window.addEventListener("resize", function () { lineChart("#ov-spark", 56, false); if (document.querySelector(".view[data-view=market]").classList.contains("on")) drawMarket(); });
