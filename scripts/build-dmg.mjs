import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, symlinkSync, mkdtempSync, statSync } from "node:fs";
import { spawnSync } from "node:child_process";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const packageJson = JSON.parse(readFileSync(path.join(projectRoot, "package.json"), "utf8"));
const productName = "DeepSeek Desktop";
const architecture = process.arch === "arm64" ? "aarch64" : process.arch === "x64" ? "x86_64" : process.arch;
const appPath = path.join(projectRoot, "src-tauri", "target", "release", "bundle", "macos", `${productName}.app`);
const outputDirectory = path.join(projectRoot, "src-tauri", "target", "release", "bundle", "dmg");
const outputPath = path.join(outputDirectory, `${productName}_${packageJson.version}_${architecture}.dmg`);
const temporaryDirectory = mkdtempSync(path.join(os.tmpdir(), "deepseek-desktop-dmg-"));
const stagingDirectory = path.join(temporaryDirectory, productName);
const rawImagePath = path.join(temporaryDirectory, `${productName}.udrw.dmg`);

function run(command, args) {
  const result = spawnSync(command, args, {
    cwd: projectRoot,
    stdio: "inherit",
    windowsHide: false,
  });

  if (result.error) {
    throw result.error;
  }

  if (result.status !== 0) {
    throw new Error(`${command} 退出码为 ${result.status ?? 1}`);
  }
}

try {
  if (process.platform !== "darwin") {
    throw new Error("tauri:build:dmg 只支持 macOS；Windows 请使用 pnpm tauri:build 生成 .exe/.msi。");
  }

  if (!existsSync(appPath)) {
    throw new Error(`未找到 macOS 应用：${appPath}。请先运行 pnpm tauri:build，或先执行 pnpm exec tauri build --bundles app。`);
  }

  mkdirSync(outputDirectory, { recursive: true });
  mkdirSync(stagingDirectory, { recursive: true });
  cpSync(appPath, path.join(stagingDirectory, `${productName}.app`), { recursive: true });
  symlinkSync("/Applications", path.join(stagingDirectory, "Applications"));

  rmSync(rawImagePath, { force: true });
  rmSync(outputPath, { force: true });

  // makehybrid creates the HFS volume directly and does not require hdiutil
  // to create or attach a writable virtual disk device.
  run("hdiutil", [
    "makehybrid",
    "-default-volume-name",
    productName,
    "-hfs",
    "-o",
    rawImagePath,
    stagingDirectory,
  ]);

  run("hdiutil", [
    "convert",
    "-format",
    "UDZO",
    "-imagekey",
    "zlib-level=9",
    "-o",
    outputPath,
    rawImagePath,
  ]);

  const outputSize = statSync(outputPath).size;
  if (outputSize === 0) {
    throw new Error(`DMG 文件为空：${outputPath}`);
  }

  console.log(`已生成 macOS DMG：${outputPath}（${outputSize} 字节）`);
  console.log("该镜像包含应用和 Applications 快捷方式，不需要挂载可写虚拟磁盘设备。");
} finally {
  rmSync(temporaryDirectory, { recursive: true, force: true });
}
