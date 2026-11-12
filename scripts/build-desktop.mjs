import { execFileSync, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, renameSync, rmSync } from "node:fs";
import { delimiter, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const nodeVersion = "24.22.1";
const args = process.argv.slice(2);

function validateNode(node) {
  let version;
  try {
    version = execFileSync(node, ["--version"], { encoding: "utf8" }).trim();
  } catch (error) {
    throw new Error(`无法运行 Node ${node}：${error.message}`);
  }
  if (version !== `v${nodeVersion}`) {
    throw new Error(`Node 版本必须是 v${nodeVersion}，${node} 的版本为 ${version}。`);
  }
}

export function ensureNode() {
  let node;
  if (process.versions.node === nodeVersion) {
    node = process.execPath;
  } else if (process.env.STICKERNEST_NODE) {
    node = resolve(process.env.STICKERNEST_NODE);
    validateNode(node);
  } else {
    if (process.platform !== "darwin" || !["arm64", "x64"].includes(process.arch)) {
      throw new Error("自动准备 Node 仅支持 macOS arm64 和 x64。请在对应架构的 macOS 构建。");
    }
    const name = `node-v${nodeVersion}-darwin-${process.arch}`;
    const tools = resolve(root, "output/tools");
    const destination = join(tools, name);
    node = join(destination, "bin/node");
    if (existsSync(node)) {
      validateNode(node);
    } else {
      try {
        mkdirSync(tools, { recursive: true });
        const temporary = mkdtempSync(join(tools, `.node-v${nodeVersion}-`));
        try {
          const archiveName = `${name}.tar.gz`;
          const archive = join(temporary, archiveName);
          const checksums = join(temporary, "SHASUMS256.txt");
          const url = `https://nodejs.org/dist/v${nodeVersion}`;
          execFileSync("curl", ["-fL", "-o", archive, `${url}/${archiveName}`], { stdio: "inherit" });
          execFileSync("curl", ["-fL", "-o", checksums, `${url}/SHASUMS256.txt`], { stdio: "inherit" });
          const expected = readFileSync(checksums, "utf8").split(/\r?\n/)
            .map((line) => line.trim().split(/\s+/))
            .find(([, filename]) => filename === archiveName)?.[0];
          if (!expected || !/^[a-f\d]{64}$/i.test(expected)) {
            throw new Error(`校验文件中没有 ${archiveName} 的 SHA-256。`);
          }
          const actual = createHash("sha256").update(readFileSync(archive)).digest("hex");
          if (actual !== expected.toLowerCase()) throw new Error(`${archiveName} 的 SHA-256 校验失败。`);
          execFileSync("tar", ["-xzf", archive, "-C", temporary], { stdio: "inherit" });
          validateNode(join(temporary, name, "bin/node"));
          renameSync(join(temporary, name), destination);
        } finally {
          rmSync(temporary, { recursive: true, force: true });
        }
      } catch (error) {
        throw new Error(`准备 Node ${nodeVersion} 失败：${error.message}`);
      }
    }
  }
  console.log(`使用 Node ${nodeVersion}：${node}`);
  return node;
}

function option(long, short) {
  const index = args.findIndex((arg) => arg === long || arg === short);
  return index >= 0 ? args[index + 1] : args.find((arg) => arg.startsWith(`${long}=`))?.split("=")[1];
}
function run(script, scriptArgs, node = process.execPath, env = process.env) {
  const result = spawnSync(node, [script, ...scriptArgs], { cwd: root, stdio: "inherit", env });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status || 1);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const node = ensureNode();
  const env = { ...process.env, PATH: `${dirname(node)}${delimiter}${process.env.PATH ?? ""}` };
  run(resolve(root, "node_modules/@tauri-apps/cli/tauri.js"), ["build", ...args], node, env);
  if (process.platform === "darwin" && !args.includes("--no-bundle")) {
    const target = option("--target", "-t") || "";
    const profile = args.includes("--debug") || args.includes("-d") ? "debug" : "release";
    const targetDir = process.env.CARGO_TARGET_DIR
      ? resolve(root, "src-tauri", process.env.CARGO_TARGET_DIR)
      : resolve(root, "src-tauri/target");
    const bundles = option("--bundles", "-b") || "app,dmg";
    run(resolve(root, "scripts/finalize-macos-icons.mjs"), [resolve(targetDir, target, profile, "bundle"), bundles]);
  }
}
