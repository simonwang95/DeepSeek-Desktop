import type { UpdatePhase } from "../types";

const sequence: UpdatePhase[] = [
  "checking",
  "stopping",
  "validatingWorkspace",
  "stashing",
  "fetching",
  "fastForwarding",
  "installingDependencies",
  "cleaningBuild",
  "building",
  "restarting",
  "completed",
];

export interface UpdateMachine {
  phase: UpdatePhase;
  error: string | null;
  history: UpdatePhase[];
}

export function createUpdateMachine(): UpdateMachine {
  return { phase: "idle", error: null, history: [] };
}

export function enterPhase(machine: UpdateMachine, phase: UpdatePhase, error: string | null = null): UpdateMachine {
  if (phase === "failed" || phase === "cancelled") {
    return { ...machine, phase, error, history: [...machine.history, phase] };
  }
  if (phase !== "idle" && !sequence.includes(phase)) {
    throw new Error(`未知更新阶段: ${phase}`);
  }
  if (phase !== "idle" && machine.phase !== "idle") {
    const currentIndex = sequence.indexOf(machine.phase);
    const nextIndex = sequence.indexOf(phase);
    if (nextIndex < currentIndex) throw new Error(`更新阶段不能回退: ${machine.phase} -> ${phase}`);
  }
  return { ...machine, phase, error, history: phase === "idle" ? machine.history : [...machine.history, phase] };
}

export function failUpdate(machine: UpdateMachine, reason: string): UpdateMachine {
  return enterPhase(machine, "failed", reason);
}

export function phaseLabel(phase: UpdatePhase): string {
  const labels: Record<UpdatePhase, string> = {
    idle: "空闲",
    checking: "检查中",
    stopping: "停止服务",
    validatingWorkspace: "检查工作区",
    stashing: "创建备份 stash",
    fetching: "获取上游更新",
    fastForwarding: "快进更新",
    installingDependencies: "安装依赖",
    cleaningBuild: "清理旧构建",
    building: "重新构建",
    restarting: "重新启动",
    completed: "已完成",
    failed: "失败",
    cancelled: "已取消",
  };
  return labels[phase];
}
