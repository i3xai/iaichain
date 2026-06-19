// 落地页逻辑（ES module）。数据一律经 ../shared/api.js 接缝层取得。
import { getMarketBook, getPriceSeries, buyAtLowest, getVersion } from "/shared/api.js";

var reduceMotion = window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;

// ---------- copy ----------
document.addEventListener("click", function (e) {
  var b = e.target.closest(".copy");
  if (!b) return;
  var txt = b.getAttribute("data-copy") || "";
  if (navigator.clipboard) navigator.clipboard.writeText(txt.trim());
  var old = b.textContent;
  b.textContent = "已复制 ✓";
  b.style.color = "var(--green)";
  setTimeout(function () { b.textContent = old; b.style.color = ""; }, 1400);
});

// ---------- hero load sequence ----------
window.addEventListener("load", function () {
  document.querySelectorAll(".hero .stagger").forEach(function (el, i) {
    setTimeout(function () { el.classList.add("in"); }, reduceMotion ? 0 : 120 + i * 90);
  });
});

// ---------- node-network canvas ----------
(function () {
  var c = document.getElementById("net-canvas");
  if (!c) return;
  var ctx = c.getContext("2d"), w, h, dpr, nodes = [];
  function size() {
    dpr = Math.min(window.devicePixelRatio || 1, 2);
    w = c.clientWidth; h = c.clientHeight;
    c.width = w * dpr; c.height = h * dpr; ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  }
  function init() {
    var count = Math.round(Math.min(54, Math.max(22, w / 26)));
    nodes = [];
    for (var i = 0; i < count; i++) nodes.push({ x: Math.random() * w, y: Math.random() * h, vx: (Math.random() - .5) * .25, vy: (Math.random() - .5) * .25, r: Math.random() * 1.6 + 1 });
  }
  function draw() {
    ctx.clearRect(0, 0, w, h);
    for (var i = 0; i < nodes.length; i++) {
      var a = nodes[i];
      for (var j = i + 1; j < nodes.length; j++) {
        var b = nodes[j], dx = a.x - b.x, dy = a.y - b.y, d = Math.sqrt(dx * dx + dy * dy);
        if (d < 128) {
          ctx.globalAlpha = (1 - d / 128) * .22;
          ctx.strokeStyle = "oklch(80% 0.15 195)";
          ctx.lineWidth = 1;
          ctx.beginPath(); ctx.moveTo(a.x, a.y); ctx.lineTo(b.x, b.y); ctx.stroke();
        }
      }
    }
    ctx.globalAlpha = 1;
    for (var k = 0; k < nodes.length; k++) {
      var n = nodes[k];
      ctx.fillStyle = "oklch(80% 0.15 195 / .55)";
      ctx.beginPath(); ctx.arc(n.x, n.y, n.r, 0, Math.PI * 2); ctx.fill();
      n.x += n.vx; n.y += n.vy;
      if (n.x < 0 || n.x > w) n.vx *= -1; if (n.y < 0 || n.y > h) n.vy *= -1;
    }
    if (!reduceMotion) requestAnimationFrame(draw);
  }
  function boot() { size(); init(); draw(); }
  window.addEventListener("resize", function () { size(); init(); });
  boot();
})();

// ---------- terminal: real char-by-char typing ----------
(function () {
  var term = document.getElementById("term");
  if (!term) return;
  var script = [
    { t: "cmd", seg: [["c-pmt", "$ "], ["", 'iai team create --recruit "实现一个 Rust JWT 鉴权模块"']] },
    { t: "out", seg: [["c-ok", "✓ "], ["c-mut", "本节点已成为 "], ["c-acc", "队长节点"], ["c-mut", " · captain.7f3a"]] },
    { t: "out", seg: [["c-ok", "✓ "], ["c-mut", "已向网络广播招募 · P2P 节点发现中…"]] },
    { t: "out", seg: [["c-mut", "  → 发现 5 个具备 AI 能力的成员节点"]] },
    { t: "cmd", seg: [["c-pmt", "$ "], ["", "iai task run --repo github.com/acme/auth-lib "], ["c-cmt", "(public)"]] },
    { t: "out", seg: [["c-acc", "  前端节点"], ["c-mut", "  → 接口与示例      "], ["c-ok", "done"]] },
    { t: "out", seg: [["c-acc", "  后端节点"], ["c-mut", "  → JWT 签发/校验   "], ["c-ok", "done"]] },
    { t: "out", seg: [["c-acc", "  测试节点"], ["c-mut", "  → 单元 + 集成测试 "], ["c-ok", "done"]] },
    { t: "out", seg: [["c-ok", "✓ "], ["c-mut", "提交已采纳 · 发起方获得功能"]] },
    { t: "out", seg: [["c-warn", "◎ "], ["c-gold", "贡献者结算 +320 贡献币"], ["c-mut", " 已分配至 3 个节点"]] }
  ];
  var li = 0, cursor = null;
  function ensureCursor(line) { cursor = document.createElement("span"); cursor.className = "cursor"; line.appendChild(cursor); }
  function nextLine() {
    if (li >= script.length) { return; }
    var item = script[li], line = document.createElement("div"); line.className = "l"; term.appendChild(line);
    if (item.t === "out" || reduceMotion) {
      item.seg.forEach(function (s) { var sp = document.createElement("span"); sp.className = s[0]; sp.textContent = s[1]; line.appendChild(sp); });
      li++; setTimeout(nextLine, reduceMotion ? 0 : 260);
      return;
    }
    var spans = item.seg.map(function (s) { var sp = document.createElement("span"); sp.className = s[0]; line.appendChild(sp); return { el: sp, text: s[1], i: 0 }; });
    ensureCursor(line);
    var si = 0;
    (function type() {
      if (si >= spans.length) { if (cursor) cursor.remove(); li++; setTimeout(nextLine, 420); return; }
      var cur = spans[si];
      if (cur.i >= cur.text.length) { si++; return type(); }
      cur.el.textContent += cur.text.charAt(cur.i++);
      if (cursor) line.appendChild(cursor);
      setTimeout(type, 26 + Math.random() * 34);
    })();
  }
  setTimeout(nextLine, 700);
})();

// ---------- install tabs ----------
var snippets = {
  mac: '<span class="c-cmt"># Homebrew</span>\n<span class="c-pmt">$</span> brew install iai-chain\n<span class="c-cmt"># 或直接安装脚本</span>\n<span class="c-pmt">$</span> curl -fsSL https://iaiaiai.ai/install.sh | sh',
  linux: '<span class="c-pmt">$</span> curl -fsSL https://iaiaiai.ai/install.sh | sh\n<span class="c-cmt"># 验证安装</span>\n<span class="c-pmt">$</span> iai --version\niai-chain <span class="c-acc">0.4.2</span> (rustc 1.84)',
  win: '<span class="c-cmt"># PowerShell</span>\n<span class="c-pmt">></span> irm https://iaiaiai.ai/install.ps1 | iex\n<span class="c-cmt"># 或 winget</span>\n<span class="c-pmt">></span> winget install iai.chain',
  cargo: '<span class="c-cmt"># 从源码用 Cargo 构建</span>\n<span class="c-pmt">$</span> cargo install iai-chain\n<span class="c-cmt"># 需要 Rust 1.80+</span>'
};
var copyText = { mac: "brew install iai-chain", linux: "curl -fsSL https://iaiaiai.ai/install.sh | sh", win: "irm https://iaiaiai.ai/install.ps1 | iex", cargo: "cargo install iai-chain" };
var codeEl = document.getElementById("inst-code"), copyEl = document.getElementById("inst-copy");
function setOS(os) { codeEl.innerHTML = snippets[os]; copyEl.setAttribute("data-copy", copyText[os]); document.querySelectorAll(".tab").forEach(function (t) { t.classList.toggle("on", t.dataset.os === os); }); }
document.querySelectorAll(".tab").forEach(function (t) { t.addEventListener("click", function () { setOS(t.dataset.os); }); });
// 安装区版本对齐真实二进制（/api/version）
try { var ver = (await getVersion()).version; snippets.linux = snippets.linux.replace(/\d+\.\d+\.\d+/, ver); } catch (e) { /* 离线保留默认 */ }
setOS("mac");

// ---------- feature hover glow ----------
document.querySelectorAll(".feat").forEach(function (f) {
  f.addEventListener("mousemove", function (e) { var r = f.getBoundingClientRect(); f.style.setProperty("--mx", (e.clientX - r.left) + "px"); f.style.setProperty("--my", (e.clientY - r.top) + "px"); });
});

// ---------- order book（数据来自 api.getMarketBook） ----------
var orders = await getMarketBook();
function lowest() { return orders.slice().sort(function (a, b) { return a.px - b.px; })[0]; }
function renderBook() {
  var tb = document.getElementById("book-body"); tb.innerHTML = "";
  orders.slice().sort(function (a, b) { return a.px - b.px; }).forEach(function (o, idx) {
    var tr = document.createElement("tr"); if (idx === 0) tr.className = "best";
    tr.innerHTML = '<td class="px">¥' + o.px.toFixed(2) + '</td><td class="qty">' + o.qty + '</td><td class="qty">' + o.node + "</td>";
    tb.appendChild(tr);
  });
  var lo = lowest(); if (lo) document.getElementById("px-now").innerHTML = "¥" + lo.px.toFixed(2) + " <small>/ 贡献币</small>";
}
renderBook();
document.getElementById("buy-btn").addEventListener("click", async function () {
  var need = parseInt(document.getElementById("buy-qty").value, 10) || 0;
  var res = await buyAtLowest(orders, need);
  orders = res.orders; renderBook();
  this.textContent = "成交 " + res.filled + " 币 · ¥" + res.cost.toFixed(2);
  var self = this; setTimeout(function () { self.textContent = "按最低价买入"; }, 2200);
});

// ---------- D3 price chart（数据来自 api.getPriceSeries） ----------
(async function () {
  if (typeof d3 === "undefined") return;
  var svg = d3.select("#price-chart"); if (svg.empty()) return;
  var data = await getPriceSeries(lowest() ? lowest().px : 0.86);
  var N = data.length;
  var first = data[0].px, last = data[N - 1].px, chg = ((last - first) / first * 100);
  var chgEl = document.getElementById("px-chg");
  chgEl.textContent = (chg >= 0 ? "▲ +" : "▼ ") + chg.toFixed(1) + "%  近24h";
  chgEl.style.color = chg >= 0 ? "var(--green)" : "var(--red)";

  function render() {
    var el = document.getElementById("price-chart"); var W = el.clientWidth || 600, H = 150;
    svg.attr("viewBox", "0 0 " + W + " " + H).selectAll("*").remove();
    var pad = { t: 12, r: 12, b: 8, l: 12 };
    var x = d3.scaleLinear().domain([0, N - 1]).range([pad.l, W - pad.r]);
    var ext = d3.extent(data, function (d) { return d.px; });
    var y = d3.scaleLinear().domain([ext[0] - 0.03, ext[1] + 0.03]).range([H - pad.b, pad.t]);
    var ticks = y.ticks(4);
    svg.append("g").selectAll("line").data(ticks).enter().append("line")
      .attr("x1", pad.l).attr("x2", W - pad.r).attr("y1", function (d) { return y(d); }).attr("y2", function (d) { return y(d); })
      .attr("stroke", "var(--border)").attr("stroke-width", 1);
    var area = d3.area().x(function (d) { return x(d.i); }).y0(H - pad.b).y1(function (d) { return y(d.px); }).curve(d3.curveMonotoneX);
    var line = d3.line().x(function (d) { return x(d.i); }).y(function (d) { return y(d.px); }).curve(d3.curveMonotoneX);
    var grad = svg.append("defs").append("linearGradient").attr("id", "areaGrad").attr("x1", 0).attr("y1", 0).attr("x2", 0).attr("y2", 1);
    grad.append("stop").attr("offset", "0%").attr("stop-color", "oklch(83% 0.13 80)").attr("stop-opacity", .28);
    grad.append("stop").attr("offset", "100%").attr("stop-color", "oklch(83% 0.13 80)").attr("stop-opacity", 0);
    svg.append("path").datum(data).attr("d", area).attr("fill", "url(#areaGrad)");
    svg.append("path").datum(data).attr("d", line).attr("fill", "none").attr("stroke", "var(--gold)").attr("stroke-width", 2);
    var lastP = data[N - 1];
    svg.append("circle").attr("cx", x(lastP.i)).attr("cy", y(lastP.px)).attr("r", 4).attr("fill", "var(--gold)");
    if (!reduceMotion) {
      svg.append("circle").attr("cx", x(lastP.i)).attr("cy", y(lastP.px)).attr("r", 4).attr("fill", "none").attr("stroke", "var(--gold)").attr("stroke-width", 1.5)
        .append("animate").attr("attributeName", "r").attr("from", 4).attr("to", 13).attr("dur", "1.8s").attr("repeatCount", "indefinite");
    }
  }
  render();
  window.addEventListener("resize", render);
  document.getElementById("chart-x").innerHTML = "<span>-24h</span><span>-18h</span><span>-12h</span><span>-6h</span><span>现在</span>";
})();

// ---------- reveal on scroll ----------
var io = new IntersectionObserver(function (es) { es.forEach(function (en) { if (en.isIntersecting) { en.target.classList.add("in"); io.unobserve(en.target); } }); }, { threshold: 0.12 });
document.querySelectorAll(".reveal").forEach(function (el) { io.observe(el); });
