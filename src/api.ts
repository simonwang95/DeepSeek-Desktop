import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AppConfig, AppSnapshot, HarnessStatus, LmStudioStatus, LogEntry, OperationProgress } from "./types";

export async function getSnapshot(): Promise<AppSnapshot> {
  return invoke<AppSnapshot>("get_snapshot");
}

export async function saveConfig(config: AppConfig): Promise<AppConfig> {
  return invoke<AppConfig>("save_config", { config });
}

export async function installHarness(): Promise<void> {
  return invoke<void>("install_harness");
}

export async function startHarness(): Promise<void> {
  return invoke<void>("start_harness");
}

export async function stopHarness(): Promise<void> {
  return invoke<void>("stop_harness");
}

export async function restartHarness(): Promise<void> {
  return invoke<void>("restart_harness");
}

export async function checkHarnessUpdate(): Promise<HarnessStatus> {
  return invoke<HarnessStatus>("check_harness_update");
}

export async function runHarnessUpdate(createStash: boolean): Promise<AppSnapshot> {
  return invoke<AppSnapshot>("run_harness_update", { options: { createStash } });
}

export async function checkLmStudio(): Promise<LmStudioStatus> {
  return invoke<LmStudioStatus>("check_lm_studio");
}

export async function openWebUi(url: string): Promise<void> {
  return invoke<void>("open_web_ui", { url });
}

export async function cancelOperation(): Promise<void> {
  return invoke<void>("cancel_operation");
}

export async function subscribeToEvents(onLog: (entry: LogEntry) => void, onProgress: (progress: OperationProgress) => void): Promise<UnlistenFn[]> {
  const logUnlisten = await listen<LogEntry>("harness-log", (event) => onLog(event.payload));
  const progressUnlisten = await listen<OperationProgress>("operation-progress", (event) => onProgress(event.payload));
  return [logUnlisten, progressUnlisten];
}
