import { useEffect, useRef, useState } from 'react';
import { X } from 'lucide-react';
import { collectDouyinFetch, collectDouyinImport, importProvenanceAtPath } from '../api';
import type { ImportReport, ManagementSnapshot } from '../types';

const CHROME_HINT = '需要：1) 本机装有 Node.js、python3 与 Pillow；2) 用调试模式启动的 Chrome 并登录抖音网页版，点开一次私信里的表情收藏面板（不用发送）。启动命令示例：/Applications/Google Chrome.app/Contents/MacOS/Google Chrome --remote-debugging-port=9222 --user-data-dir=~/chrome-debug（新的目录里需要重新登录抖音）。从项目目录运行（npm run desktop）时采集脚本才能被找到。';

type RunState =
  | { phase: 'idle' }
  | { phase: 'fetching' }
  | { phase: 'importing' }
  | { phase: 'map'; reportPath: string; report: ImportReport }
  | { phase: 'done'; text: string };

export function CollectDialog({ root, accounts, onSnapshot, onManagement, onError, onClose }: {
  root: string;
  accounts: { id: string; alias: string }[];
  onSnapshot: (snapshot: import('../types').Snapshot, previewWarning: string) => void;
  onManagement: (management: ManagementSnapshot) => void;
  onError: (message: string) => void;
  onClose: () => void;
}) {
  const [state, setState] = useState<RunState>({ phase: 'idle' });
  const [fetchDetail, setFetchDetail] = useState('');
  const [dialogError, setDialogError] = useState('');
  const [accountId, setAccountId] = useState(accounts[0]?.id ?? 'new');
  const [alias, setAlias] = useState('');
  const [mapping, setMapping] = useState(false);
  const busy = state.phase === 'fetching' || state.phase === 'importing' || mapping;
  const dialog = useRef<HTMLDialogElement>(null);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    const trigger = document.activeElement as HTMLElement | null;
    dialog.current?.showModal();
    return () => { mounted.current = false; dialog.current?.close(); trigger?.focus(); };
  }, []);

  async function start() {
    setState({ phase: 'fetching' });
    try {
      const detail = await collectDouyinFetch();
      if (!mounted.current) return;
      setFetchDetail(detail.detail);
      setState({ phase: 'importing' });
      const result = await collectDouyinImport();
      if (!mounted.current) return;
      onSnapshot(result.snapshot, result.previewWarnings.length ? `部分素材无法预览：${result.previewWarnings.join('；')}` : '');
      setState({ phase: 'map', reportPath: result.reportPath, report: result.report });
    } catch (reason) {
      if (!mounted.current) return;
      // Show the failure inside the modal; the global bar also remembers it.
      const message = String(reason);
      setDialogError(message);
      onError(message);
      setState({ phase: 'idle' });
    }
  }

  async function mapAccount() {
    if (state.phase !== 'map') return;
    const account = accounts.find(value => value.id === accountId);
    setMapping(true);
    try {
      const management = await importProvenanceAtPath(root, state.reportPath, account?.alias ?? alias.trim(), account?.id ?? null);
      if (!mounted.current) return;
      onManagement(management);
      setState({ phase: 'done', text: '账号映射完成。完整性见“来源与精确去重”。' });
    } catch (reason) {
      if (!mounted.current) return;
      const message = String(reason);
      setDialogError(message);
      onError(message);
      setMapping(false);
    }
  }

  return (
    <dialog className="detail-dialog collect-dialog" ref={dialog} onClose={() => { if (!dialog.current?.open) onClose(); }} aria-labelledby="collect-title" onCancel={event => { if (busy) event.preventDefault(); }}>
      <div className="detail-heading"><span className="eyebrow">抖音采集</span><button className="icon-button" autoFocus disabled={busy} onClick={() => dialog.current?.close()} aria-label="关闭采集"><X size={20} /></button></div>
      <div className="detail-content collect-content">
        <h2 id="collect-title">采集抖音收藏表情</h2>
        {dialogError ? <p className="field-error" role="alert">{dialogError}</p> : null}
        {state.phase === 'idle' ? <>
          <p className="field-hint">{CHROME_HINT}</p>
          <p className="field-hint">清单读取 → 原件下载（中断可重试）→ 导入当前库 → 账号映射，四步自动完成；下载与导入只保存到本机。</p>
          <div className="editor-actions"><button className="button primary" onClick={() => void start()}>开始采集</button><button className="button secondary" onClick={() => dialog.current?.close()}>取消</button></div>
        </> : null}
        {state.phase === 'fetching' ? <p role="status">正在连接 Chrome 并读取收藏清单…</p> : null}
        {state.phase === 'importing' ? <p role="status">清单完成（{fetchDetail}），正在下载并导入…</p> : null}
        {state.phase === 'map' ? <>
          <p>入库完成：新增 {state.report.added} 个 · 重复 {state.report.duplicates} 个 · 失败 {state.report.failed.length} 个。请选择这些收藏的账号归属：</p>
          <label className="field-label">抖音账号<select value={accountId} disabled={mapping} onChange={event => setAccountId(event.target.value)}>{accounts.map(account => <option key={account.id} value={account.id}>{account.alias}</option>)}<option value="new">新增账号…</option></select></label>
          {accountId === 'new' ? <label className="field-label">账号别名<input value={alias} maxLength={80} placeholder="例如：我的抖音" disabled={mapping} onChange={event => setAlias(event.target.value)} /></label> : null}
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
