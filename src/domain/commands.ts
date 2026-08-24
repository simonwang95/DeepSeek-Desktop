import type { CommandConfig } from "../types";

export interface CommandSpec {
  program: string;
  args: string[];
}

export function validatePort(port: number): string | null {
  if (!Number.isInteger(port) || port < 1 || port > 65_535) {
    return "端口必须是 1 到 65535 之间的整数";
  }
  return null;
}

export function validateSourceDir(sourceDir: string): string | null {
  const value = sourceDir.trim();
  if (!value) return "Harness 源码目录不能为空";
  if (value.includes("\0")) return "源码目录包含非法字符";
  if (value === "/" || /^[A-Za-z]:[\\/]?$/.test(value)) {
    return "为了避免误操作，源码目录不能是文件系统根目录";
  }
  return null;
}

export function validateRepoUrl(repoUrl: string): string | null {
  const value = repoUrl.trim();
  if (!value || /\s|\0/.test(value)) return "Git URL 不能为空或包含空白字符";
  const accepted = /^(https?:\/\/|ssh:\/\/|git@[^:]+:)[^/].+/i.test(value);
  return accepted ? null : "Git URL 需要使用 HTTPS、SSH 或 git@host:path 格式";
}

export function validateCommand(command: CommandConfig): string | null {
  if (!command.program.trim() || command.program.includes("\0")) {
    return "可执行文件不能为空或包含非法字符";
  }
  if (command.args.some((arg) => arg.includes("\0"))) {
    return "命令参数包含非法字符";
  }
  return null;
}

export function toCommandSpec(command: CommandConfig): CommandSpec {
  const error = validateCommand(command);
  if (error) throw new Error(error);
  return { program: command.program.trim(), args: [...command.args] };
}

export function startCommandFor(program = "pnpm"): CommandSpec {
  return { program, args: ["dsh", "web", "--no-open"] };
}

export function installCommandFor(program = "pnpm"): CommandSpec {
  return { program, args: ["install", "--frozen-lockfile"] };
}

export function buildCommandFor(program = "pnpm"): CommandSpec {
  return { program, args: ["run", "build"] };
}
