import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const pnpm = process.platform === "win32" ? "pnpm.cmd" : "pnpm";

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
    process.exit(result.status ?? 1);
  }
}

if (process.platform === "darwin") {
  // Tauri's default DMG helper needs a writable virtual disk device. Some
  // sandboxed/managed macOS environments do not expose that device, while
  // hdiutil makehybrid remains available. Build the app first and let the
  // project fallback packager create the DMG without attaching an image.
  run(pnpm, ["exec", "tauri", "build", "--bundles", "app"]);
  run(pnpm, ["run", "tauri:build:dmg"]);
} else {
  // Keep Tauri's native installer selection for Windows and Linux.
  run(pnpm, ["exec", "tauri", "build"]);
}
