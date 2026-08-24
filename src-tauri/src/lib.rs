#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod models;
mod validation;

use models::{
    AppConfig, AppSnapshot, CommandConfig, DependencyStatus, HarnessStatus, LmStudioStatus,
    LogEntry, ManagedProcess, OperationProgress, ProcessRecord, UpdatePhase,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};
use thiserror::Error;

#[derive(Debug, Error)]
enum AppError {
    #[error("{0}")]
    Message(String),
    #[error("文件操作失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON 操作失败: {0}")]
    Json(#[from] serde_json::Error),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

struct AppState {
    config: Mutex<AppConfig>,
    data_dir: Mutex<Option<PathBuf>>,
    managed_process: Mutex<Option<ManagedProcess>>,
    process_record: Mutex<Option<ProcessRecord>>,
    last_action: Mutex<Option<String>>,
    last_error: Mutex<Option<String>>,
    last_progress: Mutex<Option<OperationProgress>>,
    cancel_requested: AtomicBool,
}

impl AppState {
    fn new() -> Self {
        Self {
            config: Mutex::new(AppConfig::default()),
            data_dir: Mutex::new(None),
            managed_process: Mutex::new(None),
            process_record: Mutex::new(None),
            last_action: Mutex::new(None),
            last_error: Mutex::new(None),
            last_progress: Mutex::new(None),
            cancel_requested: AtomicBool::new(false),
        }
    }

    fn initialize(&self, data_dir: PathBuf) -> Result<(), AppError> {
        fs::create_dir_all(&data_dir)?;
        *self.data_dir.lock().map_err(lock_error)? = Some(data_dir.clone());
        if let Some(config) = read_json::<AppConfig>(&data_dir.join("config.json"))? {
            validation::validate_config(&config).map_err(AppError::Message)?;
            *self.config.lock().map_err(lock_error)? = config;
        } else {
            persist_json(&data_dir.join("config.json"), &AppConfig::default())?;
        }
        *self.process_record.lock().map_err(lock_error)? =
            read_runtime_record(&data_dir.join("runtime.json"))?;
        Ok(())
    }

    fn data_dir(&self) -> Result<PathBuf, AppError> {
        self.data_dir
            .lock()
            .map_err(lock_error)?
            .clone()
            .ok_or_else(|| AppError::Message("应用数据目录尚未初始化".into()))
    }

    fn config(&self) -> Result<AppConfig, AppError> {
        Ok(self.config.lock().map_err(lock_error)?.clone())
    }

    fn save_runtime(&self) -> Result<(), AppError> {
        let path = self.data_dir()?.join("runtime.json");
        let record = self.process_record.lock().map_err(lock_error)?.clone();
        persist_json(&path, &record)
    }

    fn set_progress(&self, progress: OperationProgress) -> Result<(), AppError> {
        *self.last_progress.lock().map_err(lock_error)? = Some(progress);
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOptions {
    pub create_stash: bool,
}

fn lock_error<T>(_: std::sync::PoisonError<T>) -> AppError {
    AppError::Message("应用状态锁已损坏，请重启 DeepSeek Desktop".into())
}

fn now() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>, AppError> {
    if !path.exists() {
        return Ok(None);
    }
    let value = fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&value)?))
}

fn read_runtime_record(path: &Path) -> Result<Option<ProcessRecord>, AppError> {
    if !path.exists() {
        return Ok(None);
    }
    let value = fs::read_to_string(path)?;
    Ok(parse_runtime_record(&value))
}

fn parse_runtime_record(value: &str) -> Option<ProcessRecord> {
    if value.trim().is_empty() || value.trim() == "null" {
        return None;
    }
    serde_json::from_str(value).ok()
}

fn persist_json<T: Serialize>(path: &Path, value: &T) -> Result<(), AppError> {
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, serde_json::to_vec_pretty(value)?)?;
    fs::rename(temp, path)?;
    Ok(())
}

fn set_action(state: &AppState, action: &str, error: Option<String>) -> Result<(), AppError> {
    *state.last_action.lock().map_err(lock_error)? = Some(action.into());
    *state.last_error.lock().map_err(lock_error)? = error;
    Ok(())
}

fn emit_log(app: &AppHandle, stream: &str, line: impl AsRef<str>) {
    let entry = LogEntry {
        stream: stream.into(),
        line: validation::redact(line.as_ref()),
        timestamp: now(),
    };
    let _ = app.emit("harness-log", entry);
}

fn emit_progress(
    app: &AppHandle,
    state: &AppState,
    phase: UpdatePhase,
    message: &str,
    detail: Option<String>,
    percent: Option<u8>,
    cancellable: bool,
) {
    let progress = OperationProgress {
        operation: "harnessUpdate".into(),
        phase: phase.as_str().into(),
        message: message.into(),
        detail,
        percent,
        cancellable,
        timestamp: now(),
    };
    let _ = state.set_progress(progress.clone());
    let _ = app.emit("operation-progress", progress);
}

fn command_from(config: &CommandConfig) -> Result<CommandSpec, AppError> {
    validation::validate_command(config).map_err(AppError::Message)?;
    Ok(CommandSpec {
        program: config.program.trim().into(),
        args: config.args.clone(),
    })
}

#[derive(Clone)]
struct CommandSpec {
    program: String,
    args: Vec<String>,
}

struct CommandResult {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

fn command_output(spec: &CommandSpec, cwd: &Path) -> Result<CommandResult, AppError> {
    let output = Command::new(&spec.program)
        .args(&spec.args)
        .current_dir(cwd)
        .output()?;
    Ok(CommandResult {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn run_logged(
    app: &AppHandle,
    spec: &CommandSpec,
    cwd: &Path,
    label: &str,
) -> Result<CommandResult, AppError> {
    emit_log(
        app,
        "system",
        format!("执行 {label}: {} {}", spec.program, spec.args.join(" ")),
    );
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Message("无法读取命令 stdout".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::Message("无法读取命令 stderr".into()))?;
    let out_app = app.clone();
    let err_app = app.clone();
    let out_thread = thread::spawn(move || read_stream(stdout, out_app, "stdout"));
    let err_thread = thread::spawn(move || read_stream(stderr, err_app, "stderr"));
    let status = child.wait()?;
    let stdout = out_thread
        .join()
        .map_err(|_| AppError::Message("读取 stdout 的线程异常退出".into()))?;
    let stderr = err_thread
        .join()
        .map_err(|_| AppError::Message("读取 stderr 的线程异常退出".into()))?;
    if !status.success() {
        emit_log(
            app,
            "system",
            format!("{label} 失败，退出码 {:?}", status.code()),
        );
    } else {
        emit_log(app, "system", format!("{label} 完成"));
    }
    Ok(CommandResult {
        status,
        stdout,
        stderr,
    })
}

fn read_stream<R: Read>(reader: R, app: AppHandle, stream: &str) -> String {
    let mut all = String::new();
    for line in BufReader::new(reader).lines() {
        match line {
            Ok(line) => {
                all.push_str(&line);
                all.push('\n');
                emit_log(&app, stream, line);
            }
            Err(error) => emit_log(&app, "system", format!("读取 {stream} 失败: {error}")),
        }
    }
    all
}

fn ensure_success(result: CommandResult, label: &str) -> Result<String, AppError> {
    if result.status.success() {
        return Ok(result.stdout);
    }
    let detail = format!(
        "{label} 失败（退出码 {:?}）\n{}{}",
        result.status.code(),
        result.stdout,
        result.stderr
    );
    Err(AppError::Message(validation::redact(&detail)))
}

fn configured_source(state: &AppState) -> Result<(AppConfig, PathBuf), AppError> {
    let config = state.config()?;
    validation::validate_config(&config).map_err(AppError::Message)?;
    let source = PathBuf::from(&config.harness.source_dir);
    Ok((config, source))
}

fn git_spec(args: &[&str]) -> CommandSpec {
    CommandSpec {
        program: "git".into(),
        args: args.iter().map(|value| (*value).into()).collect(),
    }
}

fn git_value(source: &Path, args: &[&str]) -> Option<String> {
    command_output(&git_spec(args), source)
        .ok()
        .filter(|result| result.status.success())
        .map(|result| result.stdout.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn inspect_harness(app: &AppHandle, state: &AppState) -> Result<HarnessStatus, AppError> {
    refresh_managed_process(app, state)?;
    let config = state.config()?;
    let source = PathBuf::from(&config.harness.source_dir);
    let mut status = HarnessStatus {
        source_dir: config.harness.source_dir.clone(),
        ..HarnessStatus::default()
    };
    if !source.is_dir() || !source.join(".git").exists() {
        status.dependencies = dependency_statuses(state)?;
        status.orphaned_process = orphan_is_alive(state)?;
        return Ok(status);
    }
    status.branch = git_value(&source, &["rev-parse", "--abbrev-ref", "HEAD"]);
    status.commit = git_value(&source, &["rev-parse", "HEAD"]);
    status.tag = git_value(&source, &["describe", "--tags", "--exact-match"]);
    status.dirty = Some(
        git_value(&source, &["status", "--porcelain"])
            .map(|value| !value.is_empty())
            .unwrap_or(false),
    );
    status.update_available = git_value(&source, &["rev-list", "--count", "HEAD..@{u}"])
        .and_then(|value| value.parse::<u64>().ok())
        .map(|count| count > 0);
    status.dependencies = dependency_statuses(state)?;
    status.build_artifacts_present = Some(
        config
            .harness
            .clean_paths
            .iter()
            .any(|item| source.join(item).exists()),
    );
    if let Some(managed) = state.managed_process.lock().map_err(lock_error)?.as_ref() {
        status.service_running = true;
        status.pid = Some(managed.pid);
        status.service_ready = port_is_open(managed.port);
        status.port = status.service_ready.then_some(managed.port);
        status.web_url = status
            .service_ready
            .then(|| format!("http://127.0.0.1:{}", managed.port));
    } else if let Some(record) = state.process_record.lock().map_err(lock_error)?.clone() {
        if record_alive(&record) {
            status.orphaned_process = true;
            status.pid = Some(record.pid);
            status.service_ready = port_is_open(record.port);
            status.port = status.service_ready.then_some(record.port);
            status.web_url = status
                .service_ready
                .then(|| format!("http://127.0.0.1:{}", record.port));
        }
    }
    if !status.service_running && port_is_open(config.harness.port) {
        status.port = Some(config.harness.port);
    }
    status.last_action = state.last_action.lock().map_err(lock_error)?.clone();
    status.last_error = state.last_error.lock().map_err(lock_error)?.clone();
    Ok(status)
}

fn dependency_statuses(state: &AppState) -> Result<Vec<DependencyStatus>, AppError> {
    let config = state.config()?;
    let pnpm_program = config.harness.install_command.program.clone();
    let items = [
        (
            "git",
            "git".to_string(),
            vec!["--version".into()],
            "安装 Git 并确保它位于 PATH 中".to_string(),
        ),
        (
            "node",
            "node".to_string(),
            vec!["--version".into()],
            "安装 Node.js 20 或更高版本".to_string(),
        ),
        (
            "pnpm",
            pnpm_program,
            vec!["--version".into()],
            "安装 pnpm，并确认与 Harness README 要求的版本兼容".to_string(),
        ),
    ];
    let mut result = Vec::new();
    for (name, program, args, suggestion) in items {
        let spec = CommandSpec { program, args };
        let value = Command::new(&spec.program).args(&spec.args).output();
        match value {
            Ok(output) if output.status.success() => result.push(DependencyStatus {
                name: name.into(),
                available: true,
                version: Some(String::from_utf8_lossy(&output.stdout).trim().into()),
                command: spec.program,
                suggestion,
            }),
            Ok(output) => result.push(DependencyStatus {
                name: name.into(),
                available: false,
                version: None,
                command: spec.program,
                suggestion: format!("{}（退出码 {:?}）", suggestion, output.status.code()),
            }),
            Err(error) => result.push(DependencyStatus {
                name: name.into(),
                available: false,
                version: None,
                command: spec.program,
                suggestion: format!("{suggestion}。检测错误: {error}"),
            }),
        }
    }
    Ok(result)
}

fn refresh_managed_process(app: &AppHandle, state: &AppState) -> Result<(), AppError> {
    let finished = {
        let managed = state.managed_process.lock().map_err(lock_error)?;
        if let Some(process) = managed.as_ref() {
            process
                .child
                .lock()
                .map_err(lock_error)?
                .try_wait()?
                .is_some()
        } else {
            false
        }
    };
    if finished {
        emit_log(app, "system", "Harness 服务进程已退出");
        *state.managed_process.lock().map_err(lock_error)? = None;
        *state.process_record.lock().map_err(lock_error)? = None;
        state.save_runtime()?;
        set_action(state, "服务已退出", None)?;
    }
    Ok(())
}

fn process_commandline(pid: u32) -> Option<String> {
    #[cfg(unix)]
    {
        let output = Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "command="])
            .output()
            .ok()?;
        if output.status.success() {
            return Some(String::from_utf8_lossy(&output.stdout).trim().into());
        }
    }
    #[cfg(windows)]
    {
        let output = Command::new("wmic")
            .args([
                "process",
                "where",
                &format!("ProcessId={pid}"),
                "get",
                "CommandLine",
                "/value",
            ])
            .output()
            .ok()?;
        if output.status.success() {
            return Some(String::from_utf8_lossy(&output.stdout).trim().into());
        }
    }
    None
}

fn process_cwd(pid: u32) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("lsof")
            .args(["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])
            .output()
            .ok()?;
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout)
                .lines()
                .find_map(|line| line.strip_prefix('n').map(str::to_string));
        }
    }
    #[cfg(target_os = "linux")]
    {
        return fs::read_link(format!("/proc/{pid}/cwd"))
            .ok()
            .map(|path| path.to_string_lossy().into_owned());
    }
    None
}

fn record_alive(record: &ProcessRecord) -> bool {
    let command_matches = process_commandline(record.pid)
        .map(|command| command.contains("dsh") && command.contains("web"))
        .unwrap_or(false);
    let cwd_matches = process_cwd(record.pid)
        .map(|cwd| Path::new(&cwd).starts_with(&record.source_dir))
        .unwrap_or(false);
    command_matches
        && (cwd_matches
            || process_commandline(record.pid)
                .map(|command| command.contains(&record.source_dir))
                .unwrap_or(false))
}

fn orphan_is_alive(state: &AppState) -> Result<bool, AppError> {
    Ok(state
        .process_record
        .lock()
        .map_err(lock_error)?
        .as_ref()
        .map(record_alive)
        .unwrap_or(false))
}

fn port_is_open(port: u16) -> bool {
    format!("127.0.0.1:{port}")
        .to_socket_addrs()
        .ok()
        .and_then(|mut addresses| addresses.next())
        .map(|address| TcpStream::connect_timeout(&address, Duration::from_millis(250)).is_ok())
        .unwrap_or(false)
}

fn wait_for_port(port: u16, should_be_open: bool, timeout: Duration) -> bool {
    let started = SystemTime::now();
    loop {
        if port_is_open(port) == should_be_open {
            return true;
        }
        if started.elapsed().unwrap_or(timeout) >= timeout {
            return false;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn process_still_alive(
    pid: u32,
    managed_child: &Option<Arc<Mutex<std::process::Child>>>,
) -> Result<bool, AppError> {
    if let Some(child) = managed_child {
        return Ok(child.lock().map_err(lock_error)?.try_wait()?.is_none());
    }
    Ok(process_commandline(pid).is_some())
}

fn wait_for_stop(
    pid: u32,
    port: u16,
    managed_child: &Option<Arc<Mutex<std::process::Child>>>,
    timeout: Duration,
) -> Result<bool, AppError> {
    let started = SystemTime::now();
    loop {
        if !port_is_open(port) && !process_still_alive(pid, managed_child)? {
            return Ok(true);
        }
        if started.elapsed().unwrap_or(timeout) >= timeout {
            return Ok(false);
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn spawn_log_reader<R: Read + Send + 'static>(reader: R, app: AppHandle, stream: &'static str) {
    thread::spawn(move || {
        let _ = read_stream(reader, app, stream);
    });
}

fn start_failure_message(port: u16, process_exited: bool) -> String {
    if process_exited {
        format!("Harness 进程已退出，端口 {port} 未监听；请查看实时日志并检查构建产物和启动命令")
    } else {
        format!("Harness 进程仍在运行，但端口 {port} 未监听；请查看实时日志")
    }
}

fn start_process(app: &AppHandle, state: &AppState) -> Result<(), AppError> {
    refresh_managed_process(app, state)?;
    if state.managed_process.lock().map_err(lock_error)?.is_some() {
        return Err(AppError::Message("Harness 已在运行".into()));
    }
    if orphan_is_alive(state)? {
        return Err(AppError::Message(
            "发现由上一次运行遗留的 Harness 进程，请先确认并停止它".into(),
        ));
    }
    let (config, source) = configured_source(state)?;
    if !source.is_dir() || !source.join(".git").exists() {
        return Err(AppError::Message(
            "找不到 Harness 源码，请先完成首次安装".into(),
        ));
    }
    if port_is_open(config.harness.port) {
        return Err(AppError::Message(format!(
            "端口 {} 已被占用，请修改端口或停止占用它的服务",
            config.harness.port
        )));
    }
    let spec = command_from(&config.harness.start_command)?;
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .current_dir(&source)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|error| AppError::Message(format!("启动 Harness 失败: {error}")))?;
    let pid = child.id();
    if let Some(stdout) = child.stdout.take() {
        spawn_log_reader(stdout, app.clone(), "stdout");
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_log_reader(stderr, app.clone(), "stderr");
    }
    let managed = ManagedProcess {
        pid,
        port: config.harness.port,
        child: Arc::new(Mutex::new(child)),
    };
    *state.managed_process.lock().map_err(lock_error)? = Some(managed);
    *state.process_record.lock().map_err(lock_error)? = Some(ProcessRecord {
        pid,
        source_dir: source.to_string_lossy().into_owned(),
        port: config.harness.port,
        command: std::iter::once(spec.program).chain(spec.args).collect(),
        started_at: now(),
    });
    state.save_runtime()?;
    emit_log(
        app,
        "system",
        format!(
            "Harness 已启动，PID {pid}，等待端口 {}",
            config.harness.port
        ),
    );
    let ready = wait_for_port(config.harness.port, true, Duration::from_secs(15));
    if ready {
        set_action(state, "Harness Web 服务已就绪", None)?;
        emit_progress(
            app,
            state,
            UpdatePhase::Completed,
            "Harness Web 服务已就绪",
            Some(format!("PID {pid}，端口 {}", config.harness.port)),
            Some(100),
            false,
        );
    } else {
        let process_exited = state
            .managed_process
            .lock()
            .map_err(lock_error)?
            .as_ref()
            .map(|managed| {
                managed
                    .child
                    .lock()
                    .map_err(lock_error)
                    .and_then(|mut child| child.try_wait().map_err(AppError::from))
                    .map(|status| status.is_some())
            })
            .transpose()?
            .unwrap_or(false);
        let message = start_failure_message(config.harness.port, process_exited);
        set_action(state, "Harness 启动失败", Some(message.clone()))?;
        emit_log(app, "system", &message);
        emit_progress(
            app,
            state,
            UpdatePhase::Failed,
            "Harness 启动失败，Web 端口未就绪",
            Some(format!("PID {pid}")),
            None,
            false,
        );
        if process_exited {
            *state.managed_process.lock().map_err(lock_error)? = None;
            *state.process_record.lock().map_err(lock_error)? = None;
            state.save_runtime()?;
        }
        return Err(AppError::Message(message));
    }
    Ok(())
}

fn send_signal(pid: u32, signal: &str) -> Result<(), AppError> {
    #[cfg(unix)]
    {
        let group = format!("-{pid}");
        let result = Command::new("kill").args([signal, &group]).status()?;
        if !result.success() {
            let _ = Command::new("kill")
                .args([signal, &pid.to_string()])
                .status()?;
        }
        return Ok(());
    }
    #[cfg(windows)]
    {
        if signal == "-TERM" {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T"])
                .status()?;
        }
        return Ok(());
    }
    #[allow(unreachable_code)]
    Ok(())
}

fn stop_process(app: &AppHandle, state: &AppState) -> Result<(), AppError> {
    refresh_managed_process(app, state)?;
    let (pid, port, managed_child) = {
        let managed = state.managed_process.lock().map_err(lock_error)?;
        if let Some(process) = managed.as_ref() {
            (process.pid, process.port, Some(process.child.clone()))
        } else if let Some(record) = state.process_record.lock().map_err(lock_error)?.clone() {
            if record_alive(&record) {
                (record.pid, record.port, None)
            } else {
                return Err(AppError::Message("没有发现仍在运行的 Harness 进程".into()));
            }
        } else {
            return Err(AppError::Message("没有发现仍在运行的 Harness 进程".into()));
        }
    };
    emit_log(
        app,
        "system",
        format!("向 Harness 进程组 {pid} 发送中断信号"),
    );
    send_signal(pid, "-INT")?;
    if !wait_for_stop(pid, port, &managed_child, Duration::from_secs(10))? {
        emit_log(app, "system", "中断信号超时，发送 TERM 信号");
        send_signal(pid, "-TERM")?;
        if !wait_for_stop(pid, port, &managed_child, Duration::from_secs(5))? {
            return Err(AppError::Message(
                "Harness 未在超时内停止，已停止后续强制操作；请检查进程和日志".into(),
            ));
        }
    }
    if let Some(child) = managed_child {
        let _ = child.lock().map_err(lock_error)?.wait();
    }
    *state.managed_process.lock().map_err(lock_error)? = None;
    *state.process_record.lock().map_err(lock_error)? = None;
    state.save_runtime()?;
    set_action(state, "Harness 已停止", None)?;
    emit_log(app, "system", "Harness 服务已停止，端口已释放");
    Ok(())
}

fn run_git_step(
    app: &AppHandle,
    source: &Path,
    args: &[&str],
    label: &str,
) -> Result<String, AppError> {
    let result = run_logged(app, &git_spec(args), source, label)?;
    ensure_success(result, label)
}

fn parse_lm_url(url: &str) -> Result<(String, u16, String), AppError> {
    validation::validate_lm_url(url).map_err(AppError::Message)?;
    let without_scheme = url
        .split_once("://")
        .map(|(_, value)| value)
        .ok_or_else(|| AppError::Message("LM Studio URL 缺少协议".into()))?;
    let (authority, path) = without_scheme
        .split_once('/')
        .unwrap_or((without_scheme, ""));
    let (host, port) = if let Some((host, port)) = authority.rsplit_once(':') {
        (
            host.to_string(),
            port.parse::<u16>()
                .map_err(|_| AppError::Message("LM Studio URL 端口无效".into()))?,
        )
    } else {
        (authority.to_string(), 80)
    };
    if host.is_empty() || path.is_empty() {
        return Err(AppError::Message("LM Studio URL 缺少主机或路径".into()));
    }
    Ok((host, port, format!("/{path}")))
}

fn check_lm(url: &str) -> LmStudioStatus {
    let checked_at = now();
    let parsed = parse_lm_url(url);
    if let Err(error) = parsed {
        return LmStudioStatus {
            reachable: false,
            api_url: url.into(),
            models: Vec::new(),
            error: Some(error.to_string()),
            checked_at,
        };
    }
    let (host, port, path) = parsed.expect("checked above");
    let address = format!("{host}:{port}")
        .to_socket_addrs()
        .ok()
        .and_then(|mut values| values.next());
    let Some(address) = address else {
        return LmStudioStatus {
            reachable: false,
            api_url: url.into(),
            models: Vec::new(),
            error: Some("无法解析 LM Studio 主机".into()),
            checked_at,
        };
    };
    let result =
        TcpStream::connect_timeout(&address, Duration::from_secs(2)).and_then(|mut stream| {
            stream.set_read_timeout(Some(Duration::from_secs(3)))?;
            stream.write_all(
                format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )?;
            let mut response = String::new();
            stream.read_to_string(&mut response)?;
            Ok(response)
        });
    match result {
        Ok(response) => {
            let (headers, body) = response
                .split_once("\r\n\r\n")
                .unwrap_or(("", response.as_str()));
            if !headers.starts_with("HTTP/1.1 2") && !headers.starts_with("HTTP/2 2") {
                return LmStudioStatus {
                    reachable: false,
                    api_url: url.into(),
                    models: Vec::new(),
                    error: Some(format!(
                        "LM Studio 返回异常状态: {}",
                        headers.lines().next().unwrap_or("未知")
                    )),
                    checked_at,
                };
            }
            let models = serde_json::from_str::<serde_json::Value>(body)
                .ok()
                .and_then(|value| value.get("data").cloned())
                .and_then(|data| data.as_array().cloned())
                .unwrap_or_default()
                .into_iter()
                .filter_map(|value| {
                    value
                        .get("id")
                        .and_then(|id| id.as_str())
                        .map(str::to_string)
                })
                .collect();
            LmStudioStatus {
                reachable: true,
                api_url: url.into(),
                models,
                error: None,
                checked_at,
            }
        }
        Err(error) => LmStudioStatus {
            reachable: false,
            api_url: url.into(),
            models: Vec::new(),
            error: Some(format!("LM Studio API 不可达: {error}")),
            checked_at,
        },
    }
}

fn app_snapshot(app: &AppHandle, state: &AppState) -> Result<AppSnapshot, AppError> {
    let config = state.config()?;
    let harness = inspect_harness(app, state)?;
    let lm_studio = check_lm(&config.lm_studio.api_url);
    let last_progress = state.last_progress.lock().map_err(lock_error)?.clone();
    Ok(AppSnapshot {
        config,
        harness,
        lm_studio,
        last_progress,
    })
}

#[tauri::command]
fn get_snapshot(app: AppHandle, state: State<'_, AppState>) -> Result<AppSnapshot, AppError> {
    app_snapshot(&app, &state)
}

#[tauri::command]
fn save_config(config: AppConfig, state: State<'_, AppState>) -> Result<AppConfig, AppError> {
    validation::validate_config(&config).map_err(AppError::Message)?;
    *state.config.lock().map_err(lock_error)? = config.clone();
    persist_json(&state.data_dir()?.join("config.json"), &config)?;
    set_action(&state, "设置已保存", None)?;
    Ok(config)
}

#[tauri::command]
fn detect_dependencies(state: State<'_, AppState>) -> Result<Vec<DependencyStatus>, AppError> {
    dependency_statuses(&state)
}

#[tauri::command]
fn install_harness(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    let (config, source) = configured_source(&state)?;
    if source.exists() {
        if !source.is_dir() || source.read_dir()?.next().transpose()?.is_some() {
            return Err(AppError::Message(
                "目标目录已存在且非空；为保护用户文件，未执行覆盖或清理".into(),
            ));
        }
    } else if let Some(parent) = source.parent() {
        fs::create_dir_all(parent)?;
    }
    let spec = CommandSpec {
        program: "git".into(),
        args: vec![
            "clone".into(),
            config.harness.repo_url,
            source.to_string_lossy().into_owned(),
        ],
    };
    emit_progress(
        &app,
        &state,
        UpdatePhase::Checking,
        "准备首次安装",
        None,
        Some(5),
        false,
    );
    let result = run_logged(
        &app,
        &spec,
        source.parent().unwrap_or(Path::new(".")),
        "git clone",
    )?;
    ensure_success(result, "git clone")?;
    set_action(&state, "Harness 首次安装完成", None)?;
    Ok(())
}

#[tauri::command]
fn start_harness(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    start_process(&app, &state)
}

#[tauri::command]
fn stop_harness(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    stop_process(&app, &state)
}

#[tauri::command]
fn restart_harness(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    if state.managed_process.lock().map_err(lock_error)?.is_some() || orphan_is_alive(&state)? {
        stop_process(&app, &state)?;
    }
    start_process(&app, &state)
}

#[tauri::command]
fn check_harness_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<HarnessStatus, AppError> {
    let (config, source) = configured_source(&state)?;
    if !source.join(".git").exists() {
        return Err(AppError::Message("尚未安装 Harness 源码".into()));
    }
    let remote = config.update.remote_name;
    let branch = config.harness.branch;
    let result = run_logged(
        &app,
        &git_spec(&["fetch", "--dry-run", "--prune", &remote, &branch]),
        &source,
        "检查 Harness 上游更新",
    )?;
    ensure_success(result, "检查 Harness 上游更新")?;
    inspect_harness(&app, &state)
}

fn stash_if_requested(
    app: &AppHandle,
    state: &AppState,
    source: &Path,
    create_stash: bool,
) -> Result<(), AppError> {
    let dirty = git_value(source, &["status", "--porcelain"])
        .map(|value| !value.is_empty())
        .unwrap_or(false);
    if !dirty {
        return Ok(());
    }
    if !create_stash {
        return Err(AppError::Message(
            "工作区存在本地修改或未跟踪文件。更新已中止；如需继续，请明确选择创建备份 stash".into(),
        ));
    }
    emit_progress(
        app,
        state,
        UpdatePhase::Stashing,
        "创建本地修改备份",
        Some("stash 将保留在 Harness Git 仓库中，更新不会自动删除它".into()),
        Some(25),
        true,
    );
    let result = run_git_step(
        app,
        source,
        &[
            "stash",
            "push",
            "--include-untracked",
            "-m",
            "DeepSeek-Desktop backup",
        ],
        "创建备份 stash",
    )?;
    if result.contains("No local changes") {
        return Err(AppError::Message(
            "Git 未能创建备份 stash，更新已中止".into(),
        ));
    }
    Ok(())
}

fn safe_clean(source: &Path, paths: &[String]) -> Result<(), AppError> {
    for relative in paths {
        validation::validate_relative_path(relative).map_err(AppError::Message)?;
        let target = source.join(relative);
        if target.exists() {
            if target.is_dir() {
                fs::remove_dir_all(&target)?;
            } else {
                fs::remove_file(&target)?;
            }
        }
    }
    Ok(())
}

fn run_harness_update_inner(
    app: &AppHandle,
    state: &AppState,
    options: UpdateOptions,
) -> Result<AppSnapshot, AppError> {
    state.cancel_requested.store(false, Ordering::SeqCst);
    let (config, source) = configured_source(&state)?;
    if !source.join(".git").exists() {
        return Err(AppError::Message("尚未安装 Harness 源码".into()));
    }
    emit_progress(
        &app,
        &state,
        UpdatePhase::Checking,
        "开始安全更新",
        None,
        Some(5),
        true,
    );
    let was_running =
        state.managed_process.lock().map_err(lock_error)?.is_some() || orphan_is_alive(&state)?;
    if was_running {
        emit_progress(
            &app,
            &state,
            UpdatePhase::Stopping,
            "停止 Harness 并确认端口释放",
            None,
            Some(12),
            true,
        );
        stop_process(&app, &state)?;
        if !wait_for_port(config.harness.port, false, Duration::from_secs(5)) {
            return Err(AppError::Message(
                "服务端口仍被占用，未继续替换构建产物".into(),
            ));
        }
    }
    emit_progress(
        &app,
        &state,
        UpdatePhase::ValidatingWorkspace,
        "检查 Git 工作区",
        None,
        Some(20),
        true,
    );
    stash_if_requested(&app, &state, &source, options.create_stash)?;
    if state.cancel_requested.load(Ordering::SeqCst) {
        emit_progress(
            &app,
            &state,
            UpdatePhase::Cancelled,
            "更新已取消",
            None,
            None,
            false,
        );
        return Err(AppError::Message("更新已取消".into()));
    }
    let remote = config.update.remote_name;
    let branch = config.harness.branch;
    emit_progress(
        &app,
        &state,
        UpdatePhase::Fetching,
        "获取 Harness 上游 Git 更新",
        Some(format!("remote={remote}, branch={branch}")),
        Some(32),
        true,
    );
    run_git_step(
        &app,
        &source,
        &["fetch", "--prune", &remote, &branch],
        "git fetch",
    )?;
    emit_progress(
        &app,
        &state,
        UpdatePhase::FastForwarding,
        "仅执行 fast-forward 更新",
        None,
        Some(42),
        true,
    );
    let upstream = format!("refs/remotes/{remote}/{branch}");
    run_git_step(
        &app,
        &source,
        &["merge", "--ff-only", &upstream],
        "git merge --ff-only",
    )?;
    emit_progress(
        &app,
        &state,
        UpdatePhase::InstallingDependencies,
        "安装锁定依赖",
        None,
        Some(55),
        true,
    );
    let install = command_from(&config.harness.install_command)?;
    ensure_success(
        run_logged(&app, &install, &source, "安装 Harness 依赖")?,
        "安装 Harness 依赖",
    )?;
    emit_progress(
        &app,
        &state,
        UpdatePhase::CleaningBuild,
        "清理配置中指定的旧构建产物",
        Some(config.harness.clean_paths.join(", ")),
        Some(68),
        true,
    );
    safe_clean(&source, &config.harness.clean_paths)?;
    emit_progress(
        &app,
        &state,
        UpdatePhase::Building,
        "重新构建 Harness",
        None,
        Some(78),
        true,
    );
    let build = command_from(&config.harness.build_command)?;
    ensure_success(
        run_logged(&app, &build, &source, "构建 Harness")?,
        "构建 Harness",
    )?;
    if was_running {
        emit_progress(
            &app,
            &state,
            UpdatePhase::Restarting,
            "构建成功，重新启动 Harness",
            None,
            Some(92),
            true,
        );
        start_process(&app, &state)?;
    }
    set_action(&state, "Harness 更新完成", None)?;
    emit_progress(
        &app,
        &state,
        UpdatePhase::Completed,
        "Harness 更新完成",
        None,
        Some(100),
        false,
    );
    app_snapshot(&app, &state)
}

#[tauri::command]
fn run_harness_update(
    app: AppHandle,
    state: State<'_, AppState>,
    options: UpdateOptions,
) -> Result<AppSnapshot, AppError> {
    let result = run_harness_update_inner(&app, &state, options);
    if let Err(error) = &result {
        let detail = error.to_string();
        emit_progress(
            &app,
            &state,
            UpdatePhase::Failed,
            "Harness 更新失败，后续步骤已停止",
            Some(detail.clone()),
            None,
            false,
        );
        let _ = set_action(&state, "Harness 更新失败", Some(detail));
    }
    result
}

#[tauri::command]
fn check_lm_studio(state: State<'_, AppState>) -> Result<LmStudioStatus, AppError> {
    let config = state.config()?;
    Ok(check_lm(&config.lm_studio.api_url))
}

#[tauri::command]
fn open_web_ui(url: String) -> Result<(), AppError> {
    if !(url.starts_with("http://127.0.0.1:") || url.starts_with("http://localhost:"))
        || url.chars().any(|ch| ch.is_whitespace() || ch == '\0')
    {
        return Err(AppError::Message(
            "只允许打开本机 Harness Web UI 地址".into(),
        ));
    }
    let port = url
        .rsplit(':')
        .next()
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| AppError::Message("Web UI 地址端口无效".into()))?;
    if !port_is_open(port) {
        return Err(AppError::Message(format!(
            "Web UI 端口 {port} 当前未监听，请先等待 Harness 启动完成"
        )));
    }
    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(&url).status()?;
    #[cfg(target_os = "linux")]
    let result = Command::new("xdg-open").arg(&url).status()?;
    #[cfg(windows)]
    let result = Command::new("explorer.exe").arg(&url).status()?;
    if result.success() {
        Ok(())
    } else {
        Err(AppError::Message(
            "无法打开 Web UI，请复制地址到浏览器".into(),
        ))
    }
}

#[tauri::command]
fn cancel_operation(state: State<'_, AppState>) -> Result<(), AppError> {
    state.cancel_requested.store(true, Ordering::SeqCst);
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .manage(AppState::new())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)?;
            app.state::<AppState>()
                .initialize(data_dir)
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            save_config,
            detect_dependencies,
            install_harness,
            start_harness,
            stop_harness,
            restart_harness,
            check_harness_update,
            run_harness_update,
            check_lm_studio,
            open_web_ui,
            cancel_operation
        ])
        .run(tauri::generate_context!())
        .expect("DeepSeek Desktop 运行失败");
}

#[cfg(test)]
mod tests {
    use super::models::AppConfig;
    use super::start_failure_message;
    use super::validation;

    #[test]
    fn default_config_does_not_touch_harness_user_config() {
        let config = AppConfig::default();
        assert_eq!(config.lm_studio.api_url, "http://127.0.0.1:1234/v1/models");
        assert!(validation::validate_config(&config).is_ok());
    }

    #[test]
    fn accepts_empty_or_null_runtime_records() {
        assert!(super::parse_runtime_record("").is_none());
        assert!(super::parse_runtime_record("null").is_none());
        assert!(super::parse_runtime_record("not-json").is_none());
    }

    #[test]
    fn explains_start_failure_when_process_exits_before_port_is_ready() {
        assert!(start_failure_message(3080, true).contains("进程已退出"));
        assert!(start_failure_message(3080, false).contains("仍在运行"));
        assert!(start_failure_message(3080, true).contains("3080"));
    }
}
