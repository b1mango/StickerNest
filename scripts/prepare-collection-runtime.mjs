// Package existing, relocatable runtimes; this build step never downloads them.
import { execFileSync } from 'node:child_process';
import { cpSync, mkdirSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const target = join(root, 'output/collection-runtime');
const pythonQuery = `import sys, json, PIL, importlib.metadata as m
print(json.dumps({'prefix': sys.base_prefix, 'version': '%d.%d' % sys.version_info[:2], 'fullVersion': sys.version.split()[0], 'pillowVersion': PIL.__version__, 'pillow': PIL.__path__[0], 'distribution': str(m.distribution('Pillow')._path)}))`;
function run(command, args) { return execFileSync(command, args, { encoding: 'utf8' }).trim(); }
if (process.platform !== 'darwin') throw new Error('The desktop runtime bundle currently targets macOS.');
let python;
try {
  python = JSON.parse(process.env.STICKERNEST_PYTHON
    ? run(process.env.STICKERNEST_PYTHON, ['-c', pythonQuery])
    : run('uv', ['run', '--offline', '--no-project', '--python', '3.11', '--with', 'pillow==13.0.0', 'python', '-c', pythonQuery]));
} catch {
  throw new Error('需要已有的可搬移 CPython 3.11 + Pillow。可用 uv 离线缓存，或设置 STICKERNEST_PYTHON 指向已准备好的解释器；此构建不会自动安装运行时。');
}
if (python.version !== '3.11' || python.pillowVersion !== '13.0.0') throw new Error('运行时需要 CPython 3.11 与 Pillow 13.0.0。');
const arch = process.arch === 'arm64' ? 'arm64' : 'x86_64';
const targetArch = process.env.TAURI_ENV_ARCH;
if (targetArch && targetArch !== (arch === 'arm64' ? 'aarch64' : arch)) throw new Error('运行时仅支持与构建机同架构的 macOS 目标；请在对应架构机器构建。');
const pythonBin = join(python.prefix, 'bin', `python${python.version}`);
if (!run('lipo', ['-archs', pythonBin]).split(/\s+/).includes(arch)) throw new Error('Python 与应用目标架构不一致。');
const dependencies = run('otool', ['-L', pythonBin]);
if (dependencies.split('\n').slice(1).some(line => line.trim() && !/^\s*\/(usr\/lib|System\/Library)\//.test(line))) {
  throw new Error('Python 不是独立可搬移版本；请使用 python-build-standalone / uv managed Python。');
}
const nodeVersion = process.versions.node;
const nodeLicense = join(root, 'tools/runtime-licenses/Node-LICENSE.txt');
const nodeDependencies = run('otool', ['-L', process.execPath]);
if (nodeDependencies.split('\n').some(line => /^\s+\S/.test(line) && !/^\s*\/(usr\/lib|System\/Library)\//.test(line))) {
  throw new Error('Node 链接了构建机外部动态库；请使用官方独立 Node 二进制。');
}
if (nodeVersion !== '24.22.1') throw new Error('构建运行时固定使用 Node 24.22.1；升级时同时更新 tools/runtime-licenses/Node-LICENSE.txt。');
rmSync(target, { recursive: true, force: true });
mkdirSync(join(target, 'scripts/node_modules'), { recursive: true });
mkdirSync(join(target, 'python/bin'), { recursive: true });
const arches = run('lipo', ['-archs', process.execPath]).split(/\s+/);
if (!arches.includes(arch)) throw new Error(`Node 缺少 ${arch} 架构`);
if (arches.length > 1) run('lipo', [process.execPath, '-thin', arch, '-output', join(target, 'node')]);
else cpSync(process.execPath, join(target, 'node'));
cpSync(pythonBin, join(target, 'python/bin/python3'), { dereference: true });
const stdlib = join(target, 'python/lib', `python${python.version}`);
cpSync(join(python.prefix, 'lib', `python${python.version}`), stdlib, {
  recursive: true, dereference: true,
  filter: source => !['site-packages', '__pycache__'].includes(source.split('/').at(-1)),
});
const packages = join(stdlib, 'site-packages');
mkdirSync(packages, { recursive: true });
cpSync(python.pillow, join(packages, 'PIL'), { recursive: true, dereference: true, filter: source => !source.endsWith('/__pycache__') });
cpSync(python.distribution, join(packages, python.distribution.split('/').at(-1)), { recursive: true, dereference: true });
// Preserve any separately shipped wheel libraries alongside Pillow.
for (const entry of readdirSync(dirname(python.pillow))) {
  if (entry.toLowerCase() === 'pillow.libs') cpSync(join(dirname(python.pillow), entry), join(packages, entry), { recursive: true, dereference: true });
}
for (const script of ['collect_douyin.mjs', 'download_douyin.py', 'download_wechat.py']) {
  cpSync(join(root, 'scripts', script), join(target, 'scripts', script));
}
cpSync(join(root, 'node_modules/ws'), join(target, 'scripts/node_modules/ws'), { recursive: true, dereference: true });
cpSync(nodeLicense, join(target, 'Node-LICENSE.txt'));
writeFileSync(join(target, 'versions.json'), JSON.stringify({ node: nodeVersion, python: python.fullVersion, pillow: python.pillowVersion, architecture: arch }, null, 2));
// Check the exact copied interpreter, with no user packages or Python env overrides.
run(join(target, 'python/bin/python3'), ['-I', '-B', '-c', 'from PIL import Image; import ssl, urllib.request; assert Image.new("RGBA", (2, 2)).size == (2, 2)']);
run(join(target, 'node'), ['--check', join(target, 'scripts/collect_douyin.mjs')]);
console.log(`Collection runtime ready: ${target}`);
