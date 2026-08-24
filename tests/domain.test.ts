import { describe, expect, it } from "vitest";
import { lmStudioSummary, startBlocker, workspaceDecision } from "../src/domain/decision";
import { buildCommandFor, installCommandFor, startCommandFor, validatePort, validateRepoUrl, validateSourceDir } from "../src/domain/commands";
import { createUpdateMachine, enterPhase, failUpdate } from "../src/domain/updateMachine";
import type { HarnessStatus } from "../src/types";

const baseStatus: HarnessStatus = {
  sourceDir: "/tmp/harness",
  branch: "main",
  commit: "abc",
  tag: null,
  dirty: false,
  updateAvailable: false,
  dependencies: [
    { name: "git", available: true, version: "2.0", command: "git", suggestion: "" },
    { name: "node", available: true, version: "20", command: "node", suggestion: "" },
    { name: "pnpm", available: true, version: "10", command: "pnpm", suggestion: "" },
  ],
  buildArtifactsPresent: true,
  serviceRunning: false,
  serviceReady: false,
  orphanedProcess: false,
  pid: null,
  port: null,
  webUrl: null,
  lastAction: null,
  lastError: null,
};

describe("命令生成和校验", () => {
  it("生成与 Harness README 一致的启动命令", () => {
    expect(startCommandFor()).toEqual({ program: "pnpm", args: ["dsh", "web", "--no-open"] });
    expect(installCommandFor()).toEqual({ program: "pnpm", args: ["install", "--frozen-lockfile"] });
    expect(buildCommandFor()).toEqual({ program: "pnpm", args: ["run", "build"] });
  });

  it("拒绝危险或无效配置", () => {
    expect(validatePort(0)).not.toBeNull();
    expect(validatePort(65536)).not.toBeNull();
    expect(validateSourceDir("/")).not.toBeNull();
    expect(validateRepoUrl("not a url")).not.toBeNull();
    expect(validateRepoUrl("https://github.com/example/harness.git")).toBeNull();
  });
});

describe("启动与更新决策", () => {
  it("覆盖正常启动、端口冲突和遗留进程", () => {
    expect(startBlocker(baseStatus)).toBeNull();
    expect(startBlocker({ ...baseStatus, port: 3080 })).toBe("portConflict");
    expect(startBlocker({ ...baseStatus, orphanedProcess: true })).toBe("orphanedProcess");
    expect(startBlocker({ ...baseStatus, dependencies: [{ ...baseStatus.dependencies[0], available: false }] })).toBe("missingDependencies");
    expect(startBlocker({ ...baseStatus, buildArtifactsPresent: false })).toBe("missingBuildArtifacts");
  });

  it("脏工作区默认中止，显式选择后才创建 stash", () => {
    expect(workspaceDecision(true, false)).toBe("abort");
    expect(workspaceDecision(true, true)).toBe("stash");
    expect(workspaceDecision(false, false)).toBe("continue");
  });

  it("保留更新失败路径和 LM Studio 不可达提示", () => {
    let machine = createUpdateMachine();
    machine = enterPhase(machine, "checking");
    machine = enterPhase(machine, "validatingWorkspace");
    machine = failUpdate(machine, "构建失败，已保留日志");
    expect(machine.phase).toBe("failed");
    expect(machine.error).toContain("构建失败");
    expect(lmStudioSummary({ reachable: false, apiUrl: "http://127.0.0.1:1234/v1/models", models: [], error: "连接被拒绝", checkedAt: "" })).toBe("连接被拒绝");
  });

  it("禁止更新状态机回退到更早步骤", () => {
    const machine = enterPhase(enterPhase(createUpdateMachine(), "checking"), "fetching");
    expect(() => enterPhase(machine, "stopping")).toThrow("不能回退");
  });
});
