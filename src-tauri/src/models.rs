use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandConfig {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessConfig {
    pub source_dir: String,
    pub repo_url: String,
    pub branch: String,
    pub port: u16,
    pub start_command: CommandConfig,
    pub install_command: CommandConfig,
    pub build_command: CommandConfig,
    pub clean_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LmStudioConfig {
    pub api_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConfig {
    pub remote_name: String,
    pub allow_prerelease: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub harness: HarnessConfig,
    pub lm_studio: LmStudioConfig,
    pub update: UpdateConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyStatus {
    pub name: String,
    pub available: bool,
    pub version: Option<String>,
    pub command: String,
    pub suggestion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HarnessStatus {
    pub source_dir: String,
    pub branch: Option<String>,
    pub commit: Option<String>,
    pub tag: Option<String>,
    pub dirty: Option<bool>,
    pub update_available: Option<bool>,
    pub dependencies: Vec<DependencyStatus>,
    pub build_artifacts_present: Option<bool>,
    pub service_running: bool,
    pub orphaned_process: bool,
    pub pid: Option<u32>,
    pub port: Option<u16>,
    pub web_url: Option<String>,
    pub last_action: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LmStudioStatus {
    pub reachable: bool,
    pub api_url: String,
    pub models: Vec<String>,
    pub error: Option<String>,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessRecord {
    pub pid: u32,
    pub source_dir: String,
    pub port: u16,
    pub command: Vec<String>,
    pub started_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub stream: String,
    pub line: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationProgress {
    pub operation: String,
    pub phase: String,
    pub message: String,
    pub detail: Option<String>,
    pub percent: Option<u8>,
    pub cancellable: bool,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub config: AppConfig,
    pub harness: HarnessStatus,
    pub lm_studio: LmStudioStatus,
    pub last_progress: Option<OperationProgress>,
}

#[derive(Debug, Clone)]
pub struct ManagedProcess {
    pub pid: u32,
    pub port: u16,
    pub child: std::sync::Arc<std::sync::Mutex<std::process::Child>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdatePhase {
    Checking,
    Stopping,
    ValidatingWorkspace,
    Stashing,
    Fetching,
    FastForwarding,
    InstallingDependencies,
    CleaningBuild,
    Building,
    Restarting,
    Completed,
    Failed,
    Cancelled,
}

impl UpdatePhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Checking => "checking",
            Self::Stopping => "stopping",
            Self::ValidatingWorkspace => "validatingWorkspace",
            Self::Stashing => "stashing",
            Self::Fetching => "fetching",
            Self::FastForwarding => "fastForwarding",
            Self::InstallingDependencies => "installingDependencies",
            Self::CleaningBuild => "cleaningBuild",
            Self::Building => "building",
            Self::Restarting => "restarting",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        let source_dir = default_source_dir();
        Self {
            harness: HarnessConfig {
                source_dir,
                repo_url: "https://github.com/deepseek-ai/deepseek-harness.git".into(),
                branch: "main".into(),
                port: 3080,
                start_command: CommandConfig { program: "pnpm".into(), args: vec!["dsh".into(), "web".into(), "--no-open".into()] },
                install_command: CommandConfig { program: "pnpm".into(), args: vec!["install".into(), "--frozen-lockfile".into()] },
                build_command: CommandConfig { program: "pnpm".into(), args: vec!["run".into(), "build".into()] },
                clean_paths: vec!["dist".into()],
            },
            lm_studio: LmStudioConfig { api_url: "http://127.0.0.1:1234/v1/models".into() },
            update: UpdateConfig { remote_name: "origin".into(), allow_prerelease: false },
        }
    }
}

fn default_source_dir() -> String {
    let home = if cfg!(windows) {
        std::env::var_os("USERPROFILE")
    } else {
        std::env::var_os("HOME")
    };
    home.map(|path| PathBuf::from(path).join("deepseek-harness").to_string_lossy().into_owned())
        .unwrap_or_else(|| "deepseek-harness".into())
}
