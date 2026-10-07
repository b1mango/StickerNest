import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
function option(long, short) {
  const index = args.findIndex((arg) => arg === long || arg === short);
  return index >= 0 ? args[index + 1] : args.find((arg) => arg.startsWith(`${long}=`))?.split("=")[1];
}
function run(script, scriptArgs) {
  const result = spawnSync(process.execPath, [script, ...scriptArgs], { cwd: root, stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status || 1);
}

run(resolve(root, "node_modules/@tauri-apps/cli/tauri.js"), ["build", ...args]);
if (process.platform === "darwin" && !args.includes("--no-bundle")) {
  const target = option("--target", "-t") || "";
  const profile = args.includes("--debug") || args.includes("-d") ? "debug" : "release";
  const targetDir = process.env.CARGO_TARGET_DIR
    ? resolve(root, "src-tauri", process.env.CARGO_TARGET_DIR)
    : resolve(root, "src-tauri/target");
  const bundles = option("--bundles", "-b") || "app,dmg";
  run(resolve(root, "scripts/finalize-macos-icons.mjs"), [resolve(targetDir, target, profile, "bundle"), bundles]);
}
