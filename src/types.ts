export type DependencyName = "git" | "node" | "pnpm";

export interface CommandConfig {
  program: string;
  args: string[];
}

export interface HarnessConfig {
  sourceDir: string;
  repoUrl: string;
  branch: string;
  port: number;
  startCommand: CommandConfig;
  installCommand: CommandConfig;
  buildCommand: CommandConfig;
  cleanPaths: string[];
}

export interface LmStudioConfig {
  apiUrl: string;
}

export interface UpdateConfig {
  remoteName: string;
  allowPrerelease: boolean;
}

export interface AppConfig {
  harness: HarnessConfig;
  lmStudio: LmStudioConfig;
  update: UpdateConfig;
}

export interface DependencyStatus {
  name: DependencyName;
  available: boolean;
  version: string | null;
  command: string;
  suggestion: string;
}

export interface HarnessStatus {
  sourceDir: string;
  branch: string | null;
  commit: string | null;
  tag: string | null;
  dirty: boolean | null;
  updateAvailable: boolean | null;
  dependencies: DependencyStatus[];
  buildArtifactsPresent: boolean | null;
  serviceRunning: boolean;
  orphanedProcess: boolean;
  pid: number | null;
  port: number | null;
  webUrl: string | null;
  lastAction: string | null;
  lastError: string | null;
}

export interface LmStudioStatus {
  reachable: boolean;
  apiUrl: string;
  models: string[];
  error: string | null;
  checkedAt: string;
}

export type UpdatePhase =
  | "idle"
  | "checking"
  | "stopping"
  | "validatingWorkspace"
  | "stashing"
  | "fetching"
  | "fastForwarding"
  | "installingDependencies"
  | "cleaningBuild"
  | "building"
  | "restarting"
  | "completed"
  | "failed"
  | "cancelled";

export interface OperationProgress {
  operation: "harnessUpdate" | "dependencyCheck" | "harnessStart" | "harnessStop";
  phase: UpdatePhase;
  message: string;
  detail: string | null;
  percent: number | null;
  cancellable: boolean;
  timestamp: string;
}

export interface LogEntry {
  stream: "stdout" | "stderr" | "system";
  line: string;
  timestamp: string;
}

export interface AppSnapshot {
  config: AppConfig;
  harness: HarnessStatus;
  lmStudio: LmStudioStatus;
  lastProgress: OperationProgress | null;
}
