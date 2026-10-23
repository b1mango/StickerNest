// Use a relocated runtime, no developer PATH, and fake local CDP / saved files.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createServer } from 'node:http';
import { WebSocketServer } from 'ws';
const source = resolve(process.argv[2] || 'output/collection-runtime');
const temp = mkdtempSync(join(tmpdir(), 'stickernest-runtime-'));
const runtime = join(temp, 'relocated runtime');
cpSync(source, runtime, { recursive: true, dereference: true });
const env = { PATH: '/usr/bin:/bin', HOME: temp, PYTHONHOME: '/nonexistent', PYTHONPATH: '/nonexistent' };
const run = (exe, args) => new Promise((resolveRun, reject) => {
  const child = spawn(exe, args, { cwd: temp, env });
  let stdout = '', stderr = '';
  child.stdout.on('data', data => stdout += data); child.stderr.on('data', data => stderr += data);
  const timer = setTimeout(() => { child.kill('SIGKILL'); reject(Error('runtime timed out')); }, 20000);
  child.once('error', error => { clearTimeout(timer); reject(error); });
  child.once('close', code => { clearTimeout(timer); code === 0 ? resolveRun(stdout) : reject(Error(`${code}: ${stderr}`)); });
});
const server = createServer((req, res) => {
  res.setHeader('content-type', 'application/json');
  res.end(JSON.stringify([{ type: 'page', url: 'https://www.douyin.com/', webSocketDebuggerUrl: `ws://127.0.0.1:${server.address().port}/page` }]));
});
const ws = new WebSocketServer({ server });
ws.on('connection', socket => socket.on('message', bytes => {
  const message = JSON.parse(bytes);
  socket.send(JSON.stringify({ id: message.id, result: { result: { value: { stickers: [{ id_str: '1', static_url: { url_list: ['https://p3-im-emoticon-sign.byteimg.com/offline.png'], uri: 'fixture' } }], pages: 1, completed: true } } } }));
}));
try {
  await new Promise(resolveListen => server.listen(0, '127.0.0.1', resolveListen));
  const list = join(temp, 'stickers.json');
  await run(join(runtime, 'node'), [join(runtime, 'scripts/collect_douyin.mjs'), '--out', list, '--port', String(server.address().port)]);
  assert.equal(JSON.parse(readFileSync(list))[0].id_str, '1');
  const probe = join(temp, 'probe.py');
  writeFileSync(probe, `import importlib.util, pathlib, sys, io, json, hashlib, subprocess
from PIL import Image
runtime, dest = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
def load(name):
 spec=importlib.util.spec_from_file_location(name, runtime/'scripts'/f'{name}.py'); m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
d=load('download_douyin'); image=Image.new('RGBA',(3,3),(255,0,0,255)); b=io.BytesIO();image.save(b,format='PNG');data=b.getvalue()
items=json.loads((dest/'stickers.json').read_text()); d.collect(items,dest/'douyin',fetcher=lambda url:data)
def no_network(url): raise AssertionError('resume unexpectedly fetched')
r=d.collect(items,dest/'douyin',fetcher=no_network);assert r['items'][0]['status']=='verified'
w=dest/'wechat';(w/'originals').mkdir(parents=True);digest=hashlib.sha256(data).hexdigest();(w/'originals'/f'{digest}.png').write_bytes(data)
url='https://mmbiz.qpic.cn/offline.png';(dest/'manifest.txt').write_text(url)
(w/'report.json').write_text(json.dumps({'schema_version':1,'expected_items':1,'items':[{'id':'1','status':'verified','url':url,'sha256':digest,'format':'png'}]}))
subprocess.run([str(runtime/'python/bin/python3'),'-I','-B',str(runtime/'scripts/download_wechat.py'),str(dest/'manifest.txt'),str(w)],check=True)
print('Pillow decode, Douyin resume and WeChat resume passed without network')
`);
  console.log((await run(join(runtime, 'python/bin/python3'), ['-I', '-B', probe, runtime, temp])).trim());
  console.log('Relocated runtime and local CDP collector passed; child exited normally.');
} finally {
  for (const client of ws.clients) client.terminate();
  ws.close(); server.close();
  rmSync(temp, { recursive: true, force: true });
}
