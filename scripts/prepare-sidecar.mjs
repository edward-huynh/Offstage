// Builds the local language model sidecar (`offstage-llm-host`) in release mode and puts it where
// Tauri looks for an external binary: `apps/desktop/src-tauri/binaries/<name>-<target triple>`.
// Tauri then bundles it next to the app's own executable, which is where the app expects it
// (`apps/desktop/src-tauri/src/local_llm.rs`). Run by `npm run build`.

import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const exe = process.platform === "win32" ? ".exe" : "";

const triple = execFileSync("rustc", ["-vV"], { encoding: "utf8" })
  .split("\n")
  .find((line) => line.startsWith("host:"))
  ?.slice("host:".length)
  .trim();
if (!triple) throw new Error("cannot read the target triple from `rustc -vV`");

execFileSync("cargo", ["build", "--release", "-p", "offstage-llm-host"], { cwd: root, stdio: "inherit" });

const targetDir = join(root, "apps", "desktop", "src-tauri", "binaries");
mkdirSync(targetDir, { recursive: true });
const from = join(root, "target", "release", `offstage-llm-host${exe}`);
const to = join(targetDir, `offstage-llm-host-${triple}${exe}`);
copyFileSync(from, to);
console.log(`sidecar: ${to}`);
