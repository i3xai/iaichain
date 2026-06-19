// 控制台登录闸门：
//   - 若已有 token：先尝试拉一次 `/api/version`（公开接口）确认后端可达 + token 仍有效；
//     401 时清 token 并落到登录界面。
//   - 否则：显示登录表单，提交时调 `authLogin(password)`，成功后隐藏闸门、调用 `app.js#init()`。
//   - 注册 `onUnauthorized` 回调：token 在使用中过期时立刻回到登录界面。
import {
  getToken, getAuthStatus, authLogin, getVersion, onUnauthorized,
} from "/shared/api.js";
import { init as initApp } from "/console/app.js";

const GATE_ID = "authGate";
const APP_ID  = "appRoot";

function $(id) { return document.getElementById(id); }

function showGate(view) {
  // view: "login" | "connecting" | "expired"
  var gate = $(GATE_ID); if (!gate) return;
  gate.style.display = "grid";
  $(APP_ID).style.display = "none";
  gate.dataset.view = view || "login";
}

function hideGate() {
  var gate = $(GATE_ID); if (gate) gate.style.display = "none";
  $(APP_ID).style.display = "";
}

function setLoginError(msg, kind) {
  var box = $("loginErr");
  if (!box) return;
  if (!msg) { box.style.display = "none"; box.innerHTML = ""; return; }
  box.style.display = "";
  box.className = "login-err" + (kind ? " " + kind : "");
  box.innerHTML = msg;
}

function setBusy(busy) {
  var btn = $("loginBtn");
  if (!btn) return;
  btn.disabled = busy;
  btn.textContent = busy ? "验证中…" : "解锁控制台";
  var inp = $("loginPw");
  if (inp) inp.disabled = busy;
}

async function doLogin() {
  var inp = $("loginPw");
  var pw = (inp && inp.value) || "";
  if (!pw) { setLoginError("请输入密码"); return; }
  setLoginError("");
  setBusy(true);
  try {
    await authLogin(pw);
    hideGate();
    if (inp) inp.value = "";
    await initApp();
  } catch (e) {
    var hint = "";
    if (e && e.payload) {
      if (e.payload.error === "invalid_password") hint = "密码错误，请重试。";
      else if (e.payload.message) hint = e.payload.message;
    }
    setLoginError(hint || (e && e.message) || "登录失败");
    setBusy(false);
    if (inp) { inp.focus(); inp.select(); }
  }
}

function bindLoginUi() {
  var btn = $("loginBtn");
  var pw = $("loginPw");
  var form = $("loginForm");
  if (btn) btn.addEventListener("click", function (e) { e.preventDefault(); doLogin(); });
  if (form) form.addEventListener("submit", function (e) { e.preventDefault(); doLogin(); });
  if (pw) pw.addEventListener("keydown", function (e) {
    if (e.key === "Enter") { e.preventDefault(); doLogin(); }
  });
  // 填一次密码后自动 focus
  if (pw) setTimeout(function () { pw.focus(); }, 50);

  // 显示/隐藏密码切换
  var tog = $("loginPwToggle");
  if (tog && pw) {
    tog.addEventListener("click", function () {
      var show = pw.type === "password";
      pw.type = show ? "text" : "password";
      tog.textContent = show ? "隐藏" : "显示";
      pw.focus();
    });
  }

  var copy = document.querySelectorAll("[data-copy]");
  copy.forEach(function (el) {
    el.addEventListener("click", function () {
      var text = el.getAttribute("data-copy");
      if (navigator.clipboard && navigator.clipboard.writeText) {
        navigator.clipboard.writeText(text).then(function () {
          var old = el.textContent; el.textContent = "已复制 ✓";
          setTimeout(function () { el.textContent = old; }, 1200);
        }).catch(function () { /* ignore */ });
      }
    });
  });
}

async function bootWithExistingToken() {
  showGate("connecting");
  try {
    // /api/version 是公开接口，不会触发 401；但若服务挂了照样 catch。
    await getVersion();
    // 试着取一个受保护接口，验证 token 真的有效。
    // 用 Promise.race 把 401 转 AuthError，由 api.js 内部 fireUnauthorized → 回到登录。
    hideGate();
    await initApp();
  } catch (_) {
    // /api/version 失败通常是网络问题；让用户停在 connecting 提示一下
    setLoginError("后端不可达，请确认 iai serve 正在运行。", "warn");
    showGate("login");
    setBusy(false);
  }
}

export async function boot() {
  bindLoginUi();

  // 401 兜底：使用中 token 过期 / 被服务端吊销
  onUnauthorized(function () {
    showGate("expired");
    setLoginError("会话已过期，请重新登录。", "warn");
    setBusy(false);
  });

  // 始终确认后端在线 + auth_status
  var status;
  try { status = await getAuthStatus(); } catch (_) {
    status = { passwordSet: false };
  }

  if (getToken()) {
    await bootWithExistingToken();
  } else {
    showGate("login");
    setBusy(false);
    if (!status.passwordSet) {
      // 极端情况：用户清空 localStorage 后进入未设密码的实例（理论上不会发生，因为启动期保证密码存在）。
      setLoginError("控制台密码保护未生效，请联系管理员重启 iai 服务。", "warn");
    }
  }
}

document.addEventListener("DOMContentLoaded", boot);
