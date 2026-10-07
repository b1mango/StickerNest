import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readdirSync, renameSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

if (process.platform !== "darwin") process.exit(0);

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const bundleDir = resolve(process.argv[2] || join(root, "src-tauri/target/release/bundle"));
const apps = readdirSync(join(bundleDir, "macos")).filter((name) => name.endsWith(".app"));
if (apps.length !== 1) throw new Error(`Expected one app in ${bundleDir}/macos, found ${apps.length}`);
const appName = apps[0];
const appPath = join(bundleDir, "macos", appName);
const plist = JSON.parse(execFileSync("plutil", ["-convert", "json", "-o", "-", join(appPath, "Contents/Info.plist")], { encoding: "utf8" }));
const architectures = execFileSync("lipo", ["-archs", join(appPath, "Contents/MacOS", plist.CFBundleExecutable)], { encoding: "utf8" }).trim().split(/\s+/);
const architecture = architectures.length > 1 ? "universal" : architectures[0] === "arm64" ? "aarch64" : "x64";
const dmgPath = join(bundleDir, "dmg", `${appName.slice(0, -4)}_${plist.CFBundleShortVersionString}_${architecture}.dmg`);
const iconPath = join(root, "src-tauri/icons/icon.png");
const scratch = mkdtempSync(join(tmpdir(), "stickernest-icons-"));

function run(command, args, options = {}) {
  return execFileSync(command, args, { stdio: "inherit", ...options });
}

function setIcon(path) {
  run("xcrun", ["swift", join(root, "scripts/set-macos-icon.swift"), iconPath, path], {
    env: { ...process.env, CLANG_MODULE_CACHE_PATH: join(scratch, "module-cache") }
  });
}

let mountPoint;
let outputScratch;
try {
  if (!existsSync(appPath)) throw new Error(`App bundle not found: ${appPath}`);

  // Unsigned Tauri builds only carry the linker's ad-hoc executable signature.
  // Seal the complete local bundle before adding Finder's custom icon metadata.
  const signature = spawnSync("codesign", ["-dv", appPath], { encoding: "utf8" });
  if (!existsSync(join(appPath, "Contents/_CodeSignature/CodeResources")) &&
      (signature.stderr.includes("Signature=adhoc") || signature.stderr.includes("not signed at all"))) {
    run("codesign", ["--force", "--deep", "--sign", "-", appPath]);
  }
  setIcon(appPath);
  run("codesign", ["--verify", "--deep", appPath]);

  if (!process.argv[3] || process.argv[3].split(",").includes("dmg")) {
    if (!existsSync(dmgPath)) throw new Error(`Installer not found: ${dmgPath}`);
    const writable = join(scratch, "writable.dmg");
    outputScratch = mkdtempSync(join(dirname(dmgPath), ".icons-"));
    const finished = join(outputScratch, "finished.dmg");
    run("hdiutil", ["convert", dmgPath, "-format", "UDRW", "-o", writable]);
    const attachment = run("hdiutil", ["attach", writable, "-nobrowse", "-noautoopen", "-plist"], { stdio: "pipe" });
    const volumeInfo = run("plutil", ["-convert", "json", "-o", "-", "-"], { input: attachment, stdio: "pipe" });
    mountPoint = JSON.parse(volumeInfo)["system-entities"].find((entry) => entry["mount-point"])?.["mount-point"];
    if (!mountPoint) throw new Error("The writable installer did not mount a volume");

    // ditto retains the custom icon resource fork and FinderInfo across installation.
    const mountedApp = join(mountPoint, appName);
    if (!existsSync(join(mountedApp, "Contents/Info.plist"))) throw new Error("Installer app is missing");
    rmSync(mountedApp, { recursive: true });
    run("ditto", [appPath, mountedApp]);
    run("codesign", ["--verify", "--deep", mountedApp]);
    setIcon(mountPoint);
    run("hdiutil", ["detach", mountPoint]);
    mountPoint = undefined;
    run("hdiutil", ["convert", writable, "-format", "UDZO", "-o", finished]);
    run("hdiutil", ["verify", finished]);
    setIcon(finished);
    renameSync(finished, dmgPath);
    console.log(`Installer with transparent icons: ${dmgPath}`);
  }
} finally {
  try {
    if (mountPoint) run("hdiutil", ["detach", mountPoint]);
    rmSync(scratch, { recursive: true, force: true });
  } finally {
    if (outputScratch) rmSync(outputScratch, { recursive: true, force: true });
  }
}
