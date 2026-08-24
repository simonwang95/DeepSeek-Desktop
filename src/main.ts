import "./style.css";
import {
  cancelOperation,
  checkHarnessUpdate,
  checkLmStudio,
  getSnapshot,
  installHarness,
  openWebUi,
  restartHarness,
  runHarnessUpdate,
  saveConfig,
  startHarness,
  stopHarness,
  subscribeToEvents,
} from "./api";
import type { AppConfig, AppSnapshot, HarnessStatus, LogEntry, OperationProgress } from "./types";
import { phaseLabel } from "./domain/updateMachine";

const app = document.querySelector<HTMLDivElement>("#app");
if (!app) throw new Error("找不到应用根节点");
const root = app;

let snapshot: AppSnapshot | null = null;
let page: "overview" | "settings" = "overview";
let busy = false;
let errorMessage: string | null = null;
let logs: LogEntry[] = [];

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "\"": "&quot;", "'": "&#39;" })[character] ?? character);
}

function fallbackConfig(): AppConfig {
  return {
    harness: {
      sourceDir: "",
      repoUrl: "https://github.com/deepseek-ai/deepseek-harness.git",
      branch: "main",
      port: 3080,
      startCommand: { program: "pnpm", args: ["dsh", "web", "--no-open"] },
      installCommand: { program: "pnpm", args: ["install", "--frozen-lockfile"] },
      buildCommand: { program: "pnpm", args: ["run", "build"] },
      cleanPaths: ["dist"],
    },
    lmStudio: { apiUrl: "http://127.0.0.1:1234/v1/models" },
    update: { remoteName: "origin", allowPrerelease: false },
  };
}

function initialSnapshot(): AppSnapshot {
  const config = fallbackConfig();
  return {
    config,
    harness: {
      sourceDir: config.harness.sourceDir, branch: null, commit: null, tag: null, dirty: null, updateAvailable: null,
      dependencies: [], buildArtifactsPresent: null, serviceRunning: false, serviceReady: false, orphanedProcess: false, pid: null, port: null,
      webUrl: null, lastAction: null, lastError: null,
    },
    lmStudio: { reachable: false, apiUrl: config.lmStudio.apiUrl, models: [], error: "正在连接桌面应用后端", checkedAt: "" },
    lastProgress: null,
  };
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function appendSystemLog(line: string): void {
  logs = [...logs, { stream: "system" as const, line, timestamp: new Date().toISOString() }].slice(-500);
}

async function refresh(): Promise<void> {
  try {
    snapshot = await getSnapshot();
    errorMessage = null;
  } catch (error) {
    if (!snapshot) snapshot = initialSnapshot();
    errorMessage = "桌面后端暂不可用：" + errorText(error);
  }
  render();
}

async function runAction(action: () => Promise<unknown>, successMessage?: string): Promise<void> {
  busy = true;
  errorMessage = null;
  render();
  try {
    await action();
    if (successMessage) appendSystemLog(successMessage);
    await refresh();
  } catch (error) {
    errorMessage = errorText(error);
    render();
  } finally {
    busy = false;
    render();
  }
}

function formatStatus(status: HarnessStatus): { label: string; tone: string } {
  if (status.orphanedProcess) return { label: "发现遗留进程", tone: "warning" };
  if (status.serviceReady) return { label: "运行中", tone: "success" };
  if (status.serviceRunning) return { label: "启动中", tone: "warning" };
  if (status.port !== null) return { label: "端口占用", tone: "warning" };
  if (status.branch === null) return { label: "尚未安装", tone: "muted" };
  return { label: "已停止", tone: "muted" };
}

function dependencyRows(status: HarnessStatus): string {
  if (status.dependencies.length === 0) return '<p class="muted">正在检测依赖…</p>';
  return status.dependencies.map((dependency) =>
    '<div class="dependency-row"><span class="dot ' + (dependency.available ? "success" : "danger") + '"></span>' +
    '<span class="dependency-name">' + escapeHtml(dependency.name) + '</span><code>' + escapeHtml(dependency.version ?? "未找到") + '</code>' +
    '<span class="muted">' + escapeHtml(dependency.available ? "可用" : dependency.suggestion) + "</span></div>"
  ).join("");
}

function progressHtml(progress: OperationProgress | null): string {
  if (!progress || progress.phase === "idle") return "";
  const bar = progress.percent === null ? "" : '<div class="progress-track"><div class="progress-value" style="width:' + progress.percent + '%"></div></div>';
  const detail = progress.detail ? '<div class="muted small">' + escapeHtml(progress.detail) + "</div>" : "";
  const cancel = progress.cancellable && busy ? '<button class="text-button" data-action="cancel">安全取消</button>' : "";
  return '<div class="progress-card ' + (progress.phase === "failed" ? "danger-border" : "") + '"><div class="progress-heading"><strong>' +
    escapeHtml(phaseLabel(progress.phase)) + "</strong><span>" + escapeHtml(progress.message) + "</span><span>" + (progress.percent ?? "—") +
    "%</span></div>" + bar + detail + cancel + "</div>";
}

function logsHtml(): string {
  const body = logs.length === 0 ? '<div class="empty-log">操作日志会显示在这里。敏感字段会自动脱敏。</div>' :
    logs.map((entry) => '<div class="log-line ' + (entry.stream === "stderr" ? "stderr" : "") + '"><time>' +
      escapeHtml(entry.timestamp.slice(11, 19)) + "</time><span>" + escapeHtml(entry.line) + "</span></div>").join("");
  return '<section class="logs-panel"><div class="section-heading"><div><span class="eyebrow">LIVE LOG</span><h2>实时日志</h2></div>' +
    '<div class="inline-actions"><button class="secondary-button small-button" data-action="copyLogs">复制日志</button>' +
    '<button class="secondary-button small-button" data-action="clearLogs">清空</button></div></div><div class="log-window">' + body + "</div></section>";
}

function renderOverview(current: AppSnapshot): string {
  const status = formatStatus(current.harness);
  const lm = current.lmStudio;
  const updateHint = current.harness.dirty ? "工作区有本地修改，更新前需要明确创建备份 stash" :
    current.harness.updateAvailable ? "上游存在可用更新" : "尚未发现可用更新";
  const error = errorMessage ? '<div class="alert danger-alert"><strong>需要处理</strong><span>' + escapeHtml(errorMessage) +
    '</span><button class="icon-button" data-action="dismissError">×</button></div>' : "";
  const install = current.harness.branch === null ? '<button class="secondary-button" data-action="install" ' + (busy ? "disabled" : "") + '>首次安装</button>' : "";
  const open = current.harness.webUrl ? '<button class="secondary-button" data-action="openWeb" ' + (busy ? "disabled" : "") + '>打开 Web UI</button>' : "";
  const models = lm.models.length ? lm.models.map((model) => '<span class="model-pill">' + escapeHtml(model) + "</span>").join("") :
    '<span class="muted">未读取到已加载模型</span>';
  return '<main class="content"><section class="hero"><div><span class="eyebrow">UNOFFICIAL LOCAL LAUNCHER</span><h1>Harness 总览</h1>' +
    "<p>管理本机 DeepSeek Harness 的源码、进程、构建和更新。</p></div><div class=\"hero-actions\">" +
    '<button class="primary-button" data-action="start" ' + (busy || current.harness.serviceRunning ? "disabled" : "") + '>启动服务</button>' +
    '<button class="secondary-button" data-action="stop" ' + (busy || (!current.harness.serviceRunning && !current.harness.orphanedProcess) ? "disabled" : "") + '>停止服务</button>' +
    '<button class="secondary-button" data-action="restart" ' + (busy ? "disabled" : "") + '>重启</button>' + open + '</div></section>' + error +
    progressHtml(current.lastProgress) + '<section class="metrics-grid">' +
    '<article class="metric-card"><span class="metric-label">服务状态</span><strong class="metric-value ' + status.tone + '">' + status.label +
    '</strong><span class="metric-detail">' + (current.harness.pid ? "PID " + current.harness.pid : "等待操作") + "</span></article>" +
    '<article class="metric-card"><span class="metric-label">源码版本</span><strong class="metric-value">' + escapeHtml(current.harness.branch ?? "未安装") +
    '</strong><span class="metric-detail">' + escapeHtml(current.harness.commit?.slice(0, 12) ?? "—") +
    (current.harness.tag ? " · " + escapeHtml(current.harness.tag) : "") + "</span></article>" +
    '<article class="metric-card"><span class="metric-label">Web UI</span><strong class="metric-value">' + (current.harness.port ?? "—") +
    '</strong><span class="metric-detail">' + escapeHtml(current.harness.webUrl ?? "未监听") + "</span></article>" +
    '<article class="metric-card"><span class="metric-label">LM Studio</span><strong class="metric-value ' + (lm.reachable ? "success" : "warning") +
    '">' + (lm.reachable ? "已连接" : "不可达") + '</strong><span class="metric-detail">' + lm.models.length + " 个模型</span></article></section>" +
    '<section class="two-column"><article class="panel"><div class="section-heading"><div><span class="eyebrow">HARNESS</span><h2>源码与版本</h2></div>' +
    '<span class="status-chip ' + status.tone + '">' + status.label + "</span></div><dl class=\"details\">" +
    '<div><dt>源码目录</dt><dd class="path-value">' + escapeHtml(current.config.harness.sourceDir || "尚未配置") + "</dd></div>" +
    "<div><dt>Git 工作区</dt><dd>" + (current.harness.dirty === null ? "—" : current.harness.dirty ? "有未提交或未跟踪文件" : "干净") + "</dd></div>" +
    "<div><dt>上游更新</dt><dd>" + escapeHtml(updateHint) + "</dd></div><div><dt>构建产物</dt><dd>" +
    (current.harness.buildArtifactsPresent === null ? "—" : current.harness.buildArtifactsPresent ? "存在" : "未找到") + "</dd></div></dl>" +
    '<div class="dependency-list">' + dependencyRows(current.harness) + '</div><div class="inline-actions"><button class="secondary-button" data-action="checkUpdate" ' + (busy ? "disabled" : "") + '>检查更新</button>' +
    '<button class="primary-button" data-action="safeUpdate" ' + (busy || current.harness.branch === null ? "disabled" : "") + '>安全更新</button>' + install + "</div></article>" +
    '<article class="panel"><div class="section-heading"><div><span class="eyebrow">LM STUDIO</span><h2>模型连接</h2></div>' +
    '<button class="secondary-button small-button" data-action="checkLm" ' + (busy ? "disabled" : "") + '>重新检测</button></div>' +
    '<div class="lm-url">' + escapeHtml(lm.apiUrl) + "</div>" + (lm.error ? '<div class="inline-error">' + escapeHtml(lm.error) + "</div>" : "") +
    '<div class="model-list">' + models + '</div><p class="hint">验证目标：<code>qwen3.6-35b-a3b-nvfp4</code>、<code>qwen3.8-27b-nvfp4</code>。应用不会把模型列表限制为这两个名称。</p></article></section>' +
    logsHtml() + "</main>";
}

function renderSettings(current: AppSnapshot): string {
  const config = current.config;
  const error = errorMessage ? '<div class="alert danger-alert"><strong>保存失败</strong><span>' + escapeHtml(errorMessage) + "</span></div>" : "";
  return '<main class="content"><section class="hero compact-hero"><div><span class="eyebrow">CONFIGURATION</span><h1>设置</h1>' +
    "<p>DeepSeek Desktop 自身状态与 Harness 用户配置分开保存。</p></div></section>" + error +
    '<form class="settings-form" id="settingsForm"><section class="panel form-panel"><div class="section-heading"><div><span class="eyebrow">HARNESS SOURCE</span><h2>源码与启动</h2></div></div>' +
    '<label>Harness 源码目录<input name="sourceDir" required value="' + escapeHtml(config.harness.sourceDir) + '" placeholder="/Users/you/Projects/deepseek-harness"></label>' +
    '<label>上游 Git URL<input name="repoUrl" required value="' + escapeHtml(config.harness.repoUrl) + '"></label><div class="form-grid">' +
    '<label>分支<input name="branch" required value="' + escapeHtml(config.harness.branch) + '"></label><label>Web 端口<input name="port" required type="number" min="1" max="65535" value="' + config.harness.port + '"></label></div>' +
    '<label>启动命令 <span class="label-hint">参数按数组传递，不经过 shell</span><input name="startProgram" required value="' + escapeHtml(config.harness.startCommand.program) + '">' +
    '<input name="startArgs" value="' + escapeHtml(config.harness.startCommand.args.join(" ")) + '"></label>' +
    '<label>清理构建路径 <span class="label-hint">源码目录内相对路径，逗号分隔</span><input name="cleanPaths" value="' + escapeHtml(config.harness.cleanPaths.join(", ")) + '"></label></section>' +
    '<section class="panel form-panel"><div class="section-heading"><div><span class="eyebrow">LM STUDIO</span><h2>本地 API</h2></div></div>' +
    '<label>模型 API 地址<input name="lmApiUrl" required value="' + escapeHtml(config.lmStudio.apiUrl) + '"></label><p class="hint">默认检测 <code>http://127.0.0.1:1234/v1/models</code>，不会在日志中发送或展示 API Key。</p></section>' +
    '<div class="form-actions"><button type="submit" class="primary-button" ' + (busy ? "disabled" : "") + '>保存设置</button>' +
    '<button type="button" class="secondary-button" data-action="backOverview">返回总览</button></div></form>' + logsHtml() + "</main>";
}

function render(): void {
  const current = snapshot ?? initialSnapshot();
  root.innerHTML = '<div class="shell"><aside class="sidebar"><div class="brand"><div class="brand-mark">D</div><div><strong>DeepSeek</strong><span>Desktop</span></div></div>' +
    '<div class="nav-label">工作台</div><button class="nav-item ' + (page === "overview" ? "active" : "") + '" data-action="overview"><span>◈</span>总览</button>' +
    '<button class="nav-item ' + (page === "settings" ? "active" : "") + '" data-action="settings"><span>⚙</span>设置</button><div class="sidebar-footer"><span class="neutral-dot"></span><span>本地运行 · 非官方</span></div></aside>' +
    '<div class="main-shell"><header class="topbar"><div class="breadcrumb">DeepSeek Desktop <span>/</span> ' + (page === "overview" ? "Harness 总览" : "设置") +
    '</div><div class="topbar-status"><span class="neutral-dot"></span>本机管理模式</div></header>' + (page === "overview" ? renderOverview(current) : renderSettings(current)) + "</div></div>";
  wireEvents();
}

function wireEvents(): void {
  root.querySelectorAll<HTMLElement>("[data-action]").forEach((element) => {
    element.addEventListener("click", () => void handleAction(element.dataset.action ?? ""));
  });
  root.querySelector<HTMLFormElement>("#settingsForm")?.addEventListener("submit", (event) => {
    event.preventDefault();
    void handleSaveSettings(event.currentTarget as HTMLFormElement);
  });
}

async function handleAction(action: string): Promise<void> {
  if (action === "overview") { page = "overview"; render(); return; }
  if (action === "settings") { page = "settings"; render(); return; }
  if (action === "backOverview") { page = "overview"; render(); return; }
  if (action === "dismissError") { errorMessage = null; render(); return; }
  if (action === "clearLogs") { logs = []; render(); return; }
  if (action === "copyLogs") { await navigator.clipboard.writeText(logs.map((entry) => "[" + entry.timestamp + "] " + entry.stream + ": " + entry.line).join("\n")); appendSystemLog("日志已复制"); render(); return; }
  if (action === "cancel") { await runAction(cancelOperation, "已发送取消请求"); return; }
  if (action === "start") { await runAction(startHarness, "启动请求已完成"); return; }
  if (action === "stop") { await runAction(stopHarness, "停止请求已完成"); return; }
  if (action === "restart") { await runAction(restartHarness, "重启请求已完成"); return; }
  if (action === "openWeb" && snapshot?.harness.webUrl) { await runAction(() => openWebUi(snapshot?.harness.webUrl ?? ""), "已请求打开 Web UI"); return; }
  if (action === "install") { await runAction(installHarness, "首次安装完成"); return; }
  if (action === "checkUpdate") { await runAction(checkHarnessUpdate, "更新检查完成"); return; }
  if (action === "checkLm") { await runAction(checkLmStudio, "LM Studio 检测完成"); return; }
  if (action === "safeUpdate") {
    const dirty = snapshot?.harness.dirty === true;
    const createStash = dirty && window.confirm("检测到本地修改。是否创建备份 stash 后继续？stash 会保留在 Harness 仓库中，应用不会自动丢弃或覆盖这些修改。");
    if (dirty && !createStash) { appendSystemLog("更新已中止：工作区有本地修改"); render(); return; }
    await runAction(() => runHarnessUpdate(createStash), "安全更新完成");
  }
}

async function handleSaveSettings(form: HTMLFormElement): Promise<void> {
  if (!snapshot) return;
  const data = new FormData(form);
  const args = String(data.get("startArgs") ?? "").trim().split(/\s+/).filter(Boolean);
  const config: AppConfig = {
    ...snapshot.config,
    harness: {
      ...snapshot.config.harness,
      sourceDir: String(data.get("sourceDir") ?? "").trim(),
      repoUrl: String(data.get("repoUrl") ?? "").trim(),
      branch: String(data.get("branch") ?? "").trim(),
      port: Number(data.get("port")),
      startCommand: { program: String(data.get("startProgram") ?? "").trim(), args },
      cleanPaths: String(data.get("cleanPaths") ?? "").split(",").map((value) => value.trim()).filter(Boolean),
    },
    lmStudio: { apiUrl: String(data.get("lmApiUrl") ?? "").trim() },
  };
  await runAction(() => saveConfig(config), "设置已保存");
  page = "overview";
  render();
}

async function bootstrap(): Promise<void> {
  try {
    await subscribeToEvents((entry) => { logs = [...logs, entry].slice(-500); render(); }, (progress) => { if (snapshot) snapshot.lastProgress = progress; render(); });
  } catch (error) {
    appendSystemLog("事件监听未就绪：" + errorText(error));
  }
  await refresh();
}

render();
void bootstrap();
