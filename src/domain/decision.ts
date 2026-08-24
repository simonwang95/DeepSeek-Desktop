import type { HarnessStatus, LmStudioStatus } from "../types";

export type StartBlocker =
  | "missingSource"
  | "missingDependencies"
  | "portConflict"
  | "orphanedProcess"
  | null;

export function startBlocker(status: HarnessStatus): StartBlocker {
  if (!status.sourceDir) return "missingSource";
  if (status.dependencies.some((item) => !item.available)) return "missingDependencies";
  if (status.orphanedProcess) return "orphanedProcess";
  if (status.port !== null && !status.serviceRunning) return "portConflict";
  return null;
}

export function workspaceDecision(dirty: boolean, allowStash: boolean): "continue" | "stash" | "abort" {
  if (!dirty) return "continue";
  return allowStash ? "stash" : "abort";
}

export function lmStudioSummary(status: LmStudioStatus): string {
  if (!status.reachable) return status.error ?? "LM Studio API 不可达";
  if (status.models.length === 0) return "LM Studio 已连接，但当前没有加载模型";
  return `已连接，${status.models.length} 个模型可用`;
}
