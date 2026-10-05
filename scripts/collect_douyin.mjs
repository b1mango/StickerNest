// Collect the logged-in Douyin sticker collection list via a local Chrome
// DevTools session. Requires Chrome started with --remote-debugging-port=9222
// and an open Douyin tab where the sticker panel has been shown at least once.
// Writes the raw sticker JSON array to the file named by --out.
// Usage: node scripts/collect_douyin.mjs --out PATH [--port 9222]
import { createRequire } from 'node:module';
import { writeFileSync, mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import process from 'node:process';

const require = createRequire(import.meta.url);

function argValue(name, fallback) {
  const index = process.argv.indexOf(name);
  return index >= 0 && process.argv[index + 1] ? process.argv[index + 1] : fallback;
}

const outPath = argValue('--out', null);
const port = Number(argValue('--port', '9222'));
if (!outPath) {
  console.error('missing --out PATH');
  process.exit(1);
}
if (!Number.isInteger(port) || port <= 0 || port > 65535) {
  console.error('invalid --port');
  process.exit(1);
}

function logStatus(text) {
  process.stderr.write(`${text}\n`);
}

async function httpJson(pathname) {
  const response = await fetch(`http://127.0.0.1:${port}${pathname}`, {
    signal: AbortSignal.timeout(10_000),
  });
  if (!response.ok) {
    throw new Error(`Chrome 调试接口返回 ${response.status}（打开了调试模式但没有页面目标；请确认 Chrome 已全部关闭后用 --remote-debugging-port=${port} 启动）`);
  }
  return response.json();
}

async function main() {
  const WebSocket = require('ws');
  let targets;
  try {
    targets = await httpJson('/json');
  } catch (error) {
    throw new Error(`无法连接 Chrome 调试端口 ${port}。请先用调试模式启动 Chrome（见指引），错误：${error.message ?? error}`);
  }
  const page = targets.find(target =>
    target.type === 'page' && /douyin\.com/.test(target.url ?? ''));
  if (!page) {
    throw new Error('没有找到已打开的抖音页面。请在 Chrome 中登录抖音网页版，打开私信中的表情收藏面板后再试。');
  }
  logStatus(`找到页面：${page.url}`);
  const socket = new WebSocket(page.webSocketDebuggerUrl, { maxPayload: 64 * 1024 * 1024 });
  await new Promise((resolvePromise, rejectPromise) => {
    const timer = setTimeout(() => rejectPromise(new Error('连接 Chrome 页面超时')), 10_000);
    socket.once('open', () => { clearTimeout(timer); resolvePromise(); });
    socket.once('error', error => { clearTimeout(timer); rejectPromise(error); });
  });
  let counter = 0;
  const pending = new Map();
  socket.on('close', () => {
    for (const { rejectPromise } of pending.values()) {
      rejectPromise(new Error('Chrome 调试连接已断开'));
    }
    pending.clear();
  });
  socket.on('message', data => {
    let message;
    try {
      message = JSON.parse(data.toString());
    } catch {
      return;
    }
    if (message.id && pending.has(message.id)) {
      const { resolvePromise, rejectPromise } = pending.get(message.id);
      pending.delete(message.id);
      if (message.error) rejectPromise(new Error(message.error.message));
      else resolvePromise(message.result);
    }
  });
  const send = (method, params = {}) => new Promise((resolvePromise, rejectPromise) => {
    const id = ++counter;
    pending.set(id, { resolvePromise, rejectPromise });
    socket.send(JSON.stringify({ id, method, params }));
  });

  // In the page context: find the last aggregation request URL, then replay it
  // with advancing custom_cursor until the scene reports completion.
  const result = await send('Runtime.evaluate', {
    expression: `(async () => {
      const entry = performance.getEntriesByType('resource')
        .filter(r => /\\/aweme\\/v1\\/web\\/im\\/resource\\/list\\/aggregation\\//.test(r.name)
          && r.name.includes('CUSTOM_STICKER_PAGE'))
        .pop();
      if (!entry) return { error: 'no-template' };
      const collected = [];
      const seen = new Set();
      let cursor = 0;
      let template = entry.name.replace(/([?&]custom_cursor=)\\d+/, '$1');
      for (let page = 0; page < 60; page += 1) {
        const url = template.replace(/([?&]custom_cursor=)/, '$1' + cursor);
        const response = await fetch(url, { credentials: 'include', signal: AbortSignal.timeout(15_000) });
        if (response.status === 401 || response.status === 403) return { error: 'auth-expired' };
        if (!response.ok) return { error: 'http-' + response.status };
        const data = await response.json();
        const list = data?.data?.custom_sticker_page_list;
        if (!list) return { error: 'schema' };
        for (const resource of list.resources ?? []) {
          for (const sticker of resource.stickers ?? []) {
            if (sticker.id_str && !seen.has(sticker.id_str)) {
              seen.add(sticker.id_str);
              collected.push(sticker);
            }
          }
        }
        if (list.is_completed) return { stickers: collected, pages: page + 1, completed: true };
        const next = Number(list.custom_cursor ?? cursor);
        if (next <= cursor) return { stickers: collected, pages: page + 1, completed: false, reason: 'cursor-stalled' };
        cursor = next;
      }
      return { stickers: collected, pages: 60, completed: false, reason: 'page-limit' };
    })()`,
    awaitPromise: true,
    returnByValue: true,
  });
  const value = result?.result?.value;
  if (!value) {
    const exception = result?.exceptionDetails?.exception?.description
      ?? result?.exceptionDetails?.text;
    throw new Error(`页面处理失败${exception ? `：${exception}` : '。请在 Chrome 里打开一次表情收藏面板后再试'}`);
  }
  if (value.error === 'no-template') {
    throw new Error('页面还没有加载过收藏表情。请在 Chrome 私信中点开一次表情收藏面板（不用发送），再重新采集。');
  }
  if (value.error === 'auth-expired') {
    throw new Error('页面登录状态已过期，请在 Chrome 里重新登录抖音后再试。');
  }
  if (value.error) throw new Error(`收藏清单读取失败：${value.error}`);
  if (!value.completed) {
    throw new Error(`收藏清单未读取完整（${value.reason ?? '未知原因'}），已停止，未写入。请重试。`);
  }
  if (!value.stickers.length) throw new Error('收藏清单为空，未写入。');
  mkdirSync(dirname(resolve(outPath)), { recursive: true });
  writeFileSync(outPath, JSON.stringify(value.stickers));
  logStatus(`清单 ${value.stickers.length} 项，共 ${value.pages} 页，已写入 ${outPath}`);
  console.log(JSON.stringify({ stickers: value.stickers.length, pages: value.pages, out: outPath }));
}

main().catch(error => {
  console.error(String(error.message ?? error));
  process.exit(1);
});
