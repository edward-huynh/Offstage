// Builds the local language model sidecar (`offstage-llm-host`) in release mode and puts it where
// Tauri looks for an external binary: `apps/desktop/src-tauri/binaries/<name>-<target triple>`.
// Tauri then bundles it next to the app's own executable, which is where the app expects it
// (`apps/desktop/src-tauri/src/local_llm.rs`). Run by `npm run build`.
//
// `OFFSTAGE_LLM_FEATURES` adds Cargo features to the sidecar, for example `vulkan` to run the
// model on a Vulkan GPU on Windows (see docs/RELEASE.md).

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

const features = (process.env.OFFSTAGE_LLM_FEATURES ?? "").trim();
const args = ["build", "--release", "-p", "offstage-llm-host", ...(features ? ["--features", features] : [])];
execFileSync("cargo", args, { cwd: root, stdio: "inherit" });

const targetDir = join(root, "apps", "desktop", "src-tauri", "binaries");
mkdirSync(targetDir, { recursive: true });
const from = join(root, "target", "release", `offstage-llm-host${exe}`);
const to = join(targetDir, `offstage-llm-host-${triple}${exe}`);
copyFileSync(from, to);
console.log(`sidecar: ${to}`);
