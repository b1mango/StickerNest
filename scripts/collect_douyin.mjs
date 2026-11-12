import { createRequire } from 'node:module';
import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, renameSync, unlinkSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import process from 'node:process';

const require = createRequire(import.meta.url);
const WebSocket = require('ws');
const DOUYIN_URL = 'https://www.douyin.com/';
const LIST_URL = 'https://www-hj.douyin.com/aweme/v1/web/im/resource/list/aggregation/?device_platform=webapp&aid=6383&channel=channel_pc_web&scenes=CUSTOM_STICKER_PAGE&custom_cursor=';
const ACCOUNT_URL = 'https://www.douyin.com/aweme/v1/web/user/profile/self/?device_platform=webapp&aid=6383&channel=channel_pc_web';

function argument(name) {
  const index = process.argv.indexOf(name);
  const value = process.argv[index + 1];
  if (index < 0 || !value || value.startsWith('--')) {
    throw new Error(`缺少参数 ${name}`);
  }
  return resolve(value);
}

function progress(stage, message) {
  process.stderr.write(`PROGRESS ${JSON.stringify({ stage, message })}\n`);
}

function messageOf(error) {
  return String(error.message ?? error).replace(/[\r\n]+/g, ' ');
}

function deadline(milliseconds, parent, message) {
  const controller = new AbortController();
  const abortFromParent = () => controller.abort(parent.reason);
  if (parent.aborted) abortFromParent();
  else parent.addEventListener('abort', abortFromParent, { once: true });
  const timer = setTimeout(() => controller.abort(new Error(message)), milliseconds);
  return {
    signal: controller.signal,
    cancel() {
      clearTimeout(timer);
      parent.removeEventListener('abort', abortFromParent);
    },
  };
}

function delay(milliseconds, signal) {
  return new Promise((resolvePromise, rejectPromise) => {
    if (signal.aborted) {
      rejectPromise(signal.reason);
      return;
    }
    const timer = setTimeout(() => {
      signal.removeEventListener('abort', abort);
      resolvePromise();
    }, milliseconds);
    const abort = () => {
      clearTimeout(timer);
      rejectPromise(signal.reason);
    };
    signal.addEventListener('abort', abort, { once: true });
  });
}

function browserExecutable() {
  const candidates = [
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    join(homedir(), 'Applications/Google Chrome.app/Contents/MacOS/Google Chrome'),
    '/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge',
    '/Applications/Chromium.app/Contents/MacOS/Chromium',
    '/Applications/Brave Browser.app/Contents/MacOS/Brave Browser',
  ];
  const executable = candidates.find(path => existsSync(path));
  if (!executable) throw new Error('未找到 Chrome 或 Edge，请先安装 Google Chrome');
  return executable;
}

function readEndpoint(path) {
  if (!existsSync(path)) return null;
  const [portLine, browserPath] = readFileSync(path, 'utf8').split(/\r?\n/);
  const port = Number(portLine);
  if (!Number.isInteger(port) || port < 1 || port > 65535
    || !browserPath?.startsWith('/devtools/browser/')) {
    throw new Error('浏览器调试端口文件无效');
  }
  return { port, browserPath };
}

async function browserSocketUrl({ port, browserPath }, signal) {
  const limit = deadline(2_000, signal, '浏览器调试接口连接超时');
  try {
    const response = await fetch(`http://127.0.0.1:${port}/json/version`, { signal: limit.signal });
    if (!response.ok) throw new Error(`浏览器调试接口返回 HTTP ${response.status}`);
    const version = await response.json();
    const url = new URL(version.webSocketDebuggerUrl);
    if (url.protocol !== 'ws:' || url.hostname !== '127.0.0.1' || Number(url.port) !== port
      || url.pathname !== browserPath) {
      throw new Error('浏览器调试地址无效');
    }
    return url.href;
  } finally {
    limit.cancel();
  }
}

class DevTools {
  constructor(socket) {
    this.socket = socket;
    this.nextId = 0;
    this.pending = new Map();
    socket.on('message', data => {
      let message;
      try {
        message = JSON.parse(data.toString());
      } catch (error) {
        this.fail(new Error(`浏览器调试响应无效：${messageOf(error)}`));
        socket.close();
        return;
      }
      const request = this.pending.get(message.id);
      if (!request) return;
      this.pending.delete(message.id);
      request.cleanup();
      if (message.error) request.reject(new Error(message.error.message));
      else request.resolve(message.result);
    });
    socket.on('error', error => this.fail(error));
    socket.on('close', () => this.fail(new Error('浏览器调试连接已断开')));
  }

  static async connect(url, signal) {
    const socket = new WebSocket(url, { maxPayload: 64 * 1024 * 1024 });
    await new Promise((resolvePromise, rejectPromise) => {
      const cleanup = () => {
        signal.removeEventListener('abort', abort);
        socket.removeListener('open', open);
        socket.removeListener('error', error);
      };
      const abort = () => {
        cleanup();
        socket.on('error', () => {});
        socket.terminate();
        rejectPromise(signal.reason);
      };
      const open = () => { cleanup(); resolvePromise(); };
      const error = failure => { cleanup(); rejectPromise(failure); };
      if (signal.aborted) {
        abort();
        return;
      }
      signal.addEventListener('abort', abort, { once: true });
      socket.once('open', open);
      socket.once('error', error);
    });
    return new DevTools(socket);
  }

  fail(error) {
    for (const request of this.pending.values()) {
      request.cleanup();
      request.reject(error);
    }
    this.pending.clear();
  }

  send(method, params = {}, sessionId, signal) {
    return new Promise((resolvePromise, rejectPromise) => {
      if (signal.aborted) {
        rejectPromise(signal.reason);
        return;
      }
      if (this.socket.readyState !== WebSocket.OPEN) {
        rejectPromise(new Error('浏览器调试连接已断开'));
        return;
      }
      const id = ++this.nextId;
      const abort = () => {
        this.pending.delete(id);
        rejectPromise(signal.reason);
      };
      const cleanup = () => signal.removeEventListener('abort', abort);
      this.pending.set(id, { resolve: resolvePromise, reject: rejectPromise, cleanup });
      signal.addEventListener('abort', abort, { once: true });
      this.socket.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }), error => {
        if (!error || !this.pending.has(id)) return;
        this.pending.delete(id);
        cleanup();
        rejectPromise(error);
      });
    });
  }

  close() {
    this.socket.close();
  }
}

class BrowserSession {
  constructor(profile, signal) {
    this.profile = profile;
    this.portFile = join(profile, 'DevToolsActivePort');
    this.signal = signal;
    this.process = null;
    this.client = null;
    this.createdTarget = null;
    this.sessionId = null;
    this.headless = false;
  }

  async open() {
    mkdirSync(this.profile, { recursive: true });
    if (existsSync(this.portFile)) {
      let url;
      try {
        const endpoint = readEndpoint(this.portFile);
        url = await browserSocketUrl(endpoint, this.signal);
      } catch (error) {
        if (this.signal.aborted) throw this.signal.reason;
        unlinkSync(this.portFile);
      }
      if (url) {
        await this.attach(url, this.signal);
        return;
      }
    }
    const headless = existsSync(join(this.profile, 'Default', 'Cookies'));
    await this.launch(headless);
  }

  async launch(headless) {
    this.headless = headless;
    if (existsSync(this.portFile)) unlinkSync(this.portFile);
    const args = [
      `--user-data-dir=${this.profile}`,
      '--remote-debugging-port=0',
      '--no-first-run',
      '--no-default-browser-check',
      '--disable-features=Translate',
    ];
    if (headless) args.push('--headless=new');
    args.push(DOUYIN_URL);
    const child = spawn(browserExecutable(), args, { detached: true, stdio: 'ignore' });
    const state = { child, exited: false, error: null };
    state.exit = new Promise(resolvePromise => {
      child.once('exit', () => { state.exited = true; resolvePromise(); });
      child.once('error', error => { state.error = error; state.exited = true; resolvePromise(); });
    });
    this.process = state;
    child.unref();
    const limit = deadline(20_000, this.signal, '启动浏览器超时');
    let lastError;
    try {
      while (!limit.signal.aborted) {
        if (state.exited) throw new Error(`浏览器启动失败${state.error ? `：${messageOf(state.error)}` : '，进程已退出'}`);
        let url;
        try {
          const endpoint = readEndpoint(this.portFile);
          if (endpoint) url = await browserSocketUrl(endpoint, limit.signal);
        } catch (error) {
          if (limit.signal.aborted) throw limit.signal.reason;
          lastError = error;
        }
        if (url) {
          limit.cancel();
          await this.attach(url, this.signal);
          return;
        }
        await delay(250, limit.signal);
      }
      throw limit.signal.reason;
    } catch (error) {
      if (limit.signal.aborted && !this.signal.aborted && lastError) {
        throw new Error(`启动浏览器超时：${messageOf(lastError)}`);
      }
      throw error;
    } finally {
      limit.cancel();
    }
  }

  async attach(url, signal) {
    this.client = await DevTools.connect(url, signal);
    const { targetInfos } = await this.client.send('Target.getTargets', {}, undefined, signal);
    let target = targetInfos.find(info => {
      if (info.type !== 'page') return false;
      try { return new URL(info.url).hostname === 'www.douyin.com'; }
      catch { return false; }
    });
    if (!target) {
      const created = await this.client.send('Target.createTarget', { url: DOUYIN_URL }, undefined, signal);
      this.createdTarget = created.targetId;
      target = { targetId: created.targetId };
    }
    const attached = await this.client.send('Target.attachToTarget', {
      targetId: target.targetId,
      flatten: true,
    }, undefined, signal);
    this.sessionId = attached.sessionId;
    this.targetId = target.targetId;
    await this.waitForPage();
  }

  async waitForPage() {
    const limit = deadline(30_000, this.signal, '等待抖音页面加载超时');
    let lastError;
    try {
      while (!limit.signal.aborted) {
        try {
          const page = await this.evaluate('({ ready: document.readyState, host: location.hostname })', limit.signal);
          if (page.ready === 'complete' && (page.host === 'douyin.com' || page.host.endsWith('.douyin.com'))) return;
        } catch (error) {
          if (limit.signal.aborted) throw limit.signal.reason;
          lastError = error;
        }
        await delay(500, limit.signal);
      }
      throw limit.signal.reason;
    } catch (error) {
      if (limit.signal.aborted && !this.signal.aborted && lastError) {
        throw new Error(`等待抖音页面加载超时：${messageOf(lastError)}`);
      }
      throw error;
    } finally {
      limit.cancel();
    }
  }

  async evaluate(expression, signal = this.signal) {
    const result = await this.client.send('Runtime.evaluate', {
      expression,
      awaitPromise: true,
      returnByValue: true,
    }, this.sessionId, signal);
    if (result.exceptionDetails) {
      throw new Error(result.exceptionDetails.exception?.description ?? result.exceptionDetails.text);
    }
    if (!Object.hasOwn(result.result, 'value')) throw new Error('页面未返回结果');
    return result.result.value;
  }

  async activate() {
    await this.client.send('Target.activateTarget', { targetId: this.targetId }, undefined, this.signal);
  }

  async close() {
    let cleanupError;
    if (this.client && this.createdTarget) {
      const timeout = AbortSignal.timeout(2_000);
      try {
        await this.client.send('Target.closeTarget', { targetId: this.createdTarget }, undefined, timeout);
      } catch (error) {
        if (!this.process) cleanupError = new Error(`关闭采集标签页失败：${messageOf(error)}`);
      }
    }
    if (this.process && !this.process.exited) {
      if (this.client) {
        try {
          await this.client.send('Browser.close', {}, undefined, AbortSignal.timeout(2_000));
        } catch {
          // An interrupted close is followed by a process-group shutdown below.
        }
      }
      await Promise.race([this.process.exit, delay(2_000, AbortSignal.timeout(2_500))]);
      if (!this.process.exited) {
        try {
          process.kill(-this.process.child.pid, 'SIGKILL');
        } catch (error) {
          if (error.code !== 'ESRCH') cleanupError = new Error(`关闭浏览器失败：${messageOf(error)}`);
        }
        await Promise.race([this.process.exit, delay(2_000, AbortSignal.timeout(2_500))]);
        if (!this.process.exited) cleanupError = new Error('关闭浏览器失败：进程未退出');
      }
    }
    if (this.client) this.client.close();
    this.client = null;
    this.process = null;
    this.createdTarget = null;
    this.sessionId = null;
    if (cleanupError) throw cleanupError;
  }
}

async function requestPage(browser, cursor, signal) {
  const expression = `(async () => {
    const response = await fetch(${JSON.stringify(LIST_URL)} + encodeURIComponent(${JSON.stringify(cursor)}), {
      credentials: 'include', signal: AbortSignal.timeout(15000),
    });
    if (!response.ok) throw new Error('HTTP ' + response.status);
    const data = await response.json();
    if (data.status_code !== 0) throw new Error('状态码 ' + data.status_code);
    const list = data.custom_sticker_page_list ?? data.data?.custom_sticker_page_list;
    if (!list || !Array.isArray(list.resources)) throw new Error('收藏清单格式无效');
    return list;
  })()`;
  return browser.evaluate(expression, signal);
}

async function loggedInPage(browser, signal) {
  let lastError;
  try {
    return await requestPage(browser, '0', signal);
  } catch (error) {
    if (signal.aborted) throw signal.reason;
    lastError = error;
  }
  if (browser.process && browser.headless) {
    await browser.close();
    await browser.launch(false);
  }
  progress('waiting-login', '请在弹出的浏览器窗口中扫码登录抖音');
  await browser.activate();
  const timeoutMessage = '等待登录超时，请重新点击开始采集并在弹出的 Chrome 窗口里扫码登录抖音';
  const limit = deadline(5 * 60_000, signal, timeoutMessage);
  try {
    while (!limit.signal.aborted) {
      await delay(3_000, limit.signal);
      try {
        return await requestPage(browser, '0', limit.signal);
      } catch (error) {
        if (limit.signal.aborted) throw limit.signal.reason;
        lastError = error;
      }
    }
  } catch (error) {
    if (signal.aborted) throw signal.reason;
    if (!limit.signal.aborted) throw error;
  } finally {
    limit.cancel();
  }
  throw new Error(`${timeoutMessage}；最后一次原因：${messageOf(lastError)}`);
}

async function fetchPageWithRetry(browser, cursor, signal) {
  for (let attempt = 0; attempt < 3; attempt += 1) {
    try {
      return await requestPage(browser, cursor, signal);
    } catch (error) {
      if (signal.aborted) throw signal.reason;
      if (attempt === 2) throw new Error(`收藏清单读取失败：${messageOf(error)}`);
    }
  }
}

async function collect(browser, firstPage, signal) {
  const stickers = [];
  const seen = new Set();
  let cursor = '0';
  let total;
  for (let page = 0; page < 60; page += 1) {
    const list = page === 0 ? firstPage : await fetchPageWithRetry(browser, cursor, signal);
    if (typeof list.total_counts === 'number') {
      if (total !== undefined && total !== list.total_counts) throw new Error('收藏总数发生变化，未写入清单');
      total = list.total_counts;
    }
    for (const resource of list.resources) {
      if (!Array.isArray(resource.stickers)) throw new Error('收藏表情数据格式无效，未写入清单');
      for (const sticker of resource.stickers) {
        if (!sticker || typeof sticker.id_str !== 'string' || !sticker.id_str) {
          throw new Error('收藏表情缺少 ID，未写入清单');
        }
        if (!seen.has(sticker.id_str)) {
          seen.add(sticker.id_str);
          stickers.push(sticker);
        }
      }
    }
    const count = total === undefined ? '总数未知' : `共 ${total} 项`;
    progress('collecting', `已读取 ${stickers.length} 项，${count}`);
    if (list.is_completed) {
      if (total !== undefined && total !== stickers.length) {
        throw new Error(`收藏数量不一致：读取 ${stickers.length} 项，预期 ${total} 项，未写入清单`);
      }
      if (!stickers.length) throw new Error('收藏清单为空，未写入');
      return { stickers, pages: page + 1 };
    }
    let next = list.next_cursor;
    if (next === undefined || next === null || next === '') next = list.custom_cursor;
    const nextCursor = String(next);
    if (!/^\d+$/.test(nextCursor) || BigInt(nextCursor) <= BigInt(cursor)) {
      throw new Error('收藏清单游标未前进，未写入清单');
    }
    cursor = nextCursor;
  }
  throw new Error('收藏清单超过 60 页，未写入清单');
}

async function fetchAccount(browser, signal) {
  try {
    const account = await browser.evaluate(`(async () => {
      const response = await fetch(${JSON.stringify(ACCOUNT_URL)}, {
        credentials: 'include', signal: AbortSignal.timeout(10000),
      });
      if (!response.ok) throw new Error('HTTP ' + response.status);
      const data = await response.json();
      return { uid: data.user.uid, nickname: data.user.nickname };
    })()`, signal);
    if (typeof account.uid !== 'string' || typeof account.nickname !== 'string') return null;
    return account;
  } catch (error) {
    if (signal.aborted) throw signal.reason;
    return null;
  }
}

function writeJsonAtomic(path, value) {
  mkdirSync(dirname(path), { recursive: true });
  const temporary = join(dirname(path), `.${basename(path)}-${randomUUID()}.tmp`);
  try {
    writeFileSync(temporary, JSON.stringify(value), { flag: 'wx' });
    renameSync(temporary, path);
  } finally {
    if (existsSync(temporary)) unlinkSync(temporary);
  }
}

async function main() {
  const outPath = argument('--out');
  const profile = argument('--profile');
  const accountPath = argument('--account-out');
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(new Error('采集超时（8 分钟）')), 8 * 60_000);
  const browser = new BrowserSession(profile, controller.signal);
  let result;
  let failure;
  try {
    progress('launching', '正在启动专用浏览器');
    await browser.open();
    const firstPage = await loggedInPage(browser, controller.signal);
    const collected = await collect(browser, firstPage, controller.signal);
    progress('account', '正在读取抖音账号信息');
    const account = await fetchAccount(browser, controller.signal);
    writeJsonAtomic(accountPath, account);
    writeJsonAtomic(outPath, collected.stickers);
    result = { stickers: collected.stickers.length, pages: collected.pages, out: outPath };
  } catch (error) {
    failure = error;
  } finally {
    try {
      await browser.close();
    } catch (error) {
      failure = failure
        ? new Error(`${messageOf(failure)}；${messageOf(error)}`)
        : error;
    }
    clearTimeout(timer);
  }
  if (controller.signal.aborted && !failure) failure = controller.signal.reason;
  if (failure) throw failure;
  progress('done', `收藏清单读取完成，共 ${result.stickers} 项`);
  process.stdout.write(`${JSON.stringify(result)}\n`);
}

main().catch(error => {
  process.stderr.write(`采集失败：${messageOf(error)}\n`);
  process.exitCode = 1;
});
