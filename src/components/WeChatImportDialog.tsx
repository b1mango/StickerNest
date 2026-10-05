import { useEffect, useRef, useState } from 'react';
import { X } from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';
import { collectWechatImport, importWechatManifest, importProvenanceAtPath, wechatDetectAccounts, wechatCheckRunning, wechatDumpAndExport } from '../api';
import type { ImportReport, ManagementSnapshot } from '../types';

const GUIDE = '在 Windows 电脑上登录微信，运行 wxemoticon（见步骤说明）导出 emoticon_urls.txt，把文件拷回 Mac，在这里选择它。下载只访问腾讯表情 CDN，不读聊天内容。';

type DumpState =
  | { phase: 'idle' }
  | { phase: 'running'; note: string }
  | { phase: 'need-quit'; count: number }
  | { phase: 'done'; urlsTxt: string; count: number; stageDetail: string }
  | { phase: 'error'; message: string };

type RunState =
  | { phase: 'pick' }
  | { phase: 'fetching' }
  | { phase: 'importing' }
  | { phase: 'map'; reportPath: string; report: ImportReport; detail: string }
  | { phase: 'done'; text: string };

export function WeChatImportDialog({ root, accounts, onSnapshot, onManagement, onError, onClose }: {
  root: string;
  accounts: { id: string; alias: string }[];
  onSnapshot: (snapshot: import('../types').Snapshot, previewWarning: string) => void;
  onManagement: (management: ManagementSnapshot) => void;
  onError: (message: string) => void;
  onClose: () => void;
}) {
  const [state, setState] = useState<RunState>({ phase: 'pick' });
  const [manifestPath, setManifestPath] = useState('');
  const [dialogError, setDialogError] = useState('');
  const [accountId, setAccountId] = useState(accounts[0]?.id ?? 'new');
  const [alias, setAlias] = useState('');
  const [mapping, setMapping] = useState(false);
  const [dump, setDump] = useState<DumpState>({ phase: 'idle' });
  const [wxAccounts, setWxAccounts] = useState<{ wxid: string; hasDb: boolean; hasCachedKey: boolean }[]>([]);
  const [selectedWxid, setSelectedWxid] = useState('');
  const [detecting, setDetecting] = useState(false);
  const busy = state.phase === 'fetching' || state.phase === 'importing' || mapping;
  const dialog = useRef<HTMLDialogElement>(null);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    const trigger = document.activeElement as HTMLElement | null;
    dialog.current?.showModal();
    return () => { mounted.current = false; dialog.current?.close(); trigger?.focus(); };
  }, []);

  async function detectWxAccounts() {
    if (detecting) return;
    setDetecting(true);
    setDialogError('');
    try {
      const list = await wechatDetectAccounts();
      if (!mounted.current) return;
      setWxAccounts(list);
      const first = list.find(a => a.hasDb) ?? list[0];
      setSelectedWxid(first?.wxid ?? '');
    } catch (reason) {
      if (!mounted.current) return;
      setDialogError(String(reason));
    } finally {
      if (mounted.current) setDetecting(false);
    }
  }

  async function dumpNative() {
    if (!selectedWxid) { setDialogError('请先选择微信账号。'); return; }
    if (busy || dump.phase === 'running') return;
    setDump({ phase: 'running', note: '正在检查环境' });
    try {
      const running = await wechatCheckRunning();
      if (!mounted.current) return;
      if (running > 0) {
        setDump({ phase: 'need-quit', count: running });
        return;
      }
      await runDump();
    } catch (reason) {
      if (!mounted.current) return;
      setDump({ phase: 'error', message: String(reason) });
    }
  }

  async function runDump() {
    setDump({ phase: 'running', note: '正在获取表情清单（首次需要完全退出微信；之后每次免打扰）' });
    try {
      const result = await wechatDumpAndExport(selectedWxid);
      if (!mounted.current) return;
      setDump({ phase: 'done', urlsTxt: result.urlsTxt, count: result.count, stageDetail: result.stageDetail });
      // Flow straight into the existing download → import step with the fresh list.
      setManifestPath(result.urlsTxt);
      setDialogError('');
      await startWith(result.urlsTxt);
    } catch (reason) {
      if (!mounted.current) return;
      const message = String(reason);
      if (message.startsWith('WECHAT_RUNNING')) {
        const count = Number((message.match(/（(\d+) 个进程）/) ?? [])[1] ?? '1') || 1;
        setDump({ phase: 'need-quit', count });
      } else {
        setDump({ phase: 'error', message });
      }
    }
  }

  async function continueAfterQuit() {
    const running = await wechatCheckRunning();
    if (!mounted.current) return;
    if (running > 0) {
      setDump({ phase: 'need-quit', count: running });
      return;
    }
    await runDump();
  }

  async function startWith(path: string) {
    setState({ phase: 'fetching' });
    try {
      const staged = await importWechatManifest(path);
      if (!mounted.current) return;
      setState({ phase: 'importing' });
      const summary = staged.detail;
      const result = await collectWechatImport();
      if (!mounted.current) return;
      onSnapshot(result.snapshot, result.previewWarnings.length ? `部分素材无法预览：${result.previewWarnings.join('；')}` : '');
      setState({ phase: 'map', reportPath: result.reportPath, report: result.report, detail: summary });
    } catch (reason) {
      if (!mounted.current) return;
      const message = String(reason);
      setDialogError(message);
      onError(message);
      setState({ phase: 'pick' });
    }
  }

  async function pickManifest() {
    const path = await open({ multiple: false, directory: false, title: '选择 emoticon_urls.txt', filters: [{ name: '表情清单', extensions: ['txt'] }] });
    if (!path || typeof path !== 'string') return;
    setManifestPath(path);
    setDialogError('');
  }

  async function start() {
    if (!manifestPath) { setDialogError('请先选择 emoticon_urls.txt 清单文件。'); return; }
    await startWith(manifestPath);
  }

  async function mapAccount() {
    if (state.phase !== 'map') return;
    const account = accounts.find(value => value.id === accountId);
    setMapping(true);
    try {
      const management = await importProvenanceAtPath(root, state.reportPath, account?.alias ?? alias.trim(), account?.id ?? null, '微信');
      if (!mounted.current) return;
      onManagement(management);
      setState({ phase: 'done', text: '微信账号映射完成。完整性见“来源与精确去重”。' });
    } catch (reason) {
      if (!mounted.current) return;
      const message = String(reason);
      setDialogError(message);
      onError(message);
      setMapping(false);
    }
  }

  return (
    <dialog className="detail-dialog collect-dialog" ref={dialog} onClose={() => { if (!dialog.current?.open) onClose(); }} aria-labelledby="wechat-import-title" onCancel={event => { if (busy) event.preventDefault(); }}>
      <div className="detail-heading"><span className="eyebrow">微信导入</span><button className="icon-button" autoFocus disabled={busy} onClick={() => dialog.current?.close()} aria-label="关闭导入"><X size={20} /></button></div>
      <div className="detail-content collect-content">
        <h2 id="wechat-import-title">导入微信导出清单</h2>
        {dialogError ? <p className="field-error" role="alert">{dialogError}</p> : null}
        {state.phase === 'pick' ? <>
          <div className="native-dump">
            <h3>直接从本机获取</h3>
            <p className="field-hint">首次需要完全退出微信并在本机做一个一次性本地副本；之后会缓存密钥，后续每次获取都不打扰微信。仅读取表情数据库，不碰聊天内容。</p>
            {wxAccounts.length === 0 ? <div className="batch-section"><button className="button secondary" disabled={detecting || busy || mapping} onClick={() => void detectWxAccounts()}>{detecting ? '正在检测…' : '检测本机微信账号'}</button></div> : <>
              <label className="field-label">微信账号<select value={selectedWxid} disabled={busy || mapping} onChange={event => setSelectedWxid(event.target.value)}>{wxAccounts.filter(a => a.hasDb).map(account => <option key={account.wxid} value={account.wxid}>{account.wxid}{account.hasCachedKey ? '（密钥已缓存，免打扰）' : ''}</option>)}</select></label>
              {dump.phase === 'idle' ? <div className="editor-actions"><button className="button primary" disabled={busy || mapping || !selectedWxid} onClick={() => void dumpNative()}>直接从本机获取表情清单</button></div> : null}
              {dump.phase === 'running' ? <p role="status">{dump.note}…</p> : null}
              {dump.phase === 'need-quit' ? <div className="trash-notice" role="alert"><p>检测到微信仍在运行（约 {dump.count} 个进程）。请先完全退出微信（菜单栏微信图标 → 退出，或 ⌘Q），然后点继续。</p><div className="editor-actions"><button className="button primary" onClick={() => void continueAfterQuit()}>我已退出，继续</button><button className="button secondary" onClick={() => setDump({ phase: 'idle' })}>稍后</button></div></div> : null}
              {dump.phase === 'error' ? <p className="field-error" role="alert">{dump.message}</p> : null}
              {dump.phase === 'done' ? <p className="field-hint">取得 {dump.count} 条地址。{dump.stageDetail}</p> : null}
            </>}
          </div>
          <h3>或：使用 Windows 导出的清单</h3>
          <p className="field-hint">{GUIDE}</p>
          <div className="batch-section">
            <button className="button secondary" disabled={busy} onClick={() => void pickManifest()}>选择清单文件</button>
            {manifestPath ? <span className="field-hint" title={manifestPath}>已选 {manifestPath.split('/').pop()}</span> : null}
          </div>
          <div className="editor-actions"><button className="button primary" disabled={busy || !manifestPath} onClick={() => void start()}>开始下载并导入</button><button className="button secondary" onClick={() => dialog.current?.close()}>取消</button></div>
        </> : null}
        {state.phase === 'fetching' ? <p role="status">正在根据清单下载表情原件…</p> : null}
        {state.phase === 'importing' ? <p role="status">下载完成，正在入库…</p> : null}
        {state.phase === 'map' ? <>
          <p>{state.detail}。入库结果：新增 {state.report.added} 个 · 重复 {state.report.duplicates} 个 · 失败 {state.report.failed.length} 个。请选择这批表情的账号归属：</p>
          <label className="field-label">微信账号<select value={accountId} disabled={mapping} onChange={event => setAccountId(event.target.value)}>{accounts.map(account => <option key={account.id} value={account.id}>{account.alias}</option>)}<option value="new">新增账号…</option></select></label>
          {accountId === 'new' ? <label className="field-label">账号别名<input value={alias} maxLength={80} placeholder="例如：微信大号" disabled={mapping} onChange={event => setAlias(event.target.value)} /></label> : null}
          <div className="editor-actions">
            <button className="button primary" disabled={mapping || (accountId === 'new' ? !alias.trim() : !accounts.some(a => a.id === accountId))} onClick={() => void mapAccount()}>{mapping ? '正在映射…' : '完成账号映射'}</button>
            <button className="button secondary" disabled={mapping} onClick={() => dialog.current?.close()}>稍后映射</button>
          </div>
        </> : null}
        {state.phase === 'done' ? <>
          <p role="status">{state.text}</p>
          <div className="editor-actions"><button className="button primary" onClick={() => dialog.current?.close()}>完成</button></div>
        </> : null}
      </div>
    </dialog>
  );
}
