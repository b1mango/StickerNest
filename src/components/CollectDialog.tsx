import { useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { Check, Loader2, X } from 'lucide-react';
import { collectDouyinFetch, collectDouyinImport, importProvenanceAtPath } from '../api';
import type { ImportReport, ManagementSnapshot } from '../types';

type Step = 'browser' | 'login' | 'list' | 'download' | 'map';
const STEPS: { key: Step; label: string }[] = [
  { key: 'browser', label: '启动专用浏览器' },
  { key: 'login', label: '确认抖音登录' },
  { key: 'list', label: '读取收藏清单' },
  { key: 'download', label: '下载原图并导入' },
  { key: 'map', label: '记录账号归属' },
];
/** Collector progress stages → the step they belong to. */
const STAGE_STEP: Record<string, Step> = {
  launching: 'browser', 'waiting-login': 'login', collecting: 'list', account: 'list',
  done: 'download', downloading: 'download', importing: 'download',
};
const BAR_LABEL: Record<string, string> = { collecting: '读取清单', downloading: '下载原图', importing: '导入资料库' };

type Progress = { stage: string; done: number; total: number };

type RunState =
  | { phase: 'idle' }
  | { phase: 'running'; step: Step; note: string; waitingLogin: boolean; bar?: Progress }
  | { phase: 'map'; reportPath: string; report: ImportReport }
  | { phase: 'done'; report: ImportReport; account: string };

export function CollectDialog({ root, accounts, onSnapshot, onManagement, onError, onClose }: {
  root: string;
  accounts: { id: string; alias: string }[];
  onSnapshot: (snapshot: import('../types').Snapshot, previewWarning: string) => void;
  onManagement: (management: ManagementSnapshot) => void;
  onError: (message: string) => void;
  onClose: () => void;
}) {
  const [state, setState] = useState<RunState>({ phase: 'idle' });
  const [dialogError, setDialogError] = useState('');
  const [accountId, setAccountId] = useState(accounts[0]?.id ?? 'new');
  const [alias, setAlias] = useState('');
  const [mapping, setMapping] = useState(false);
  const busy = state.phase === 'running' || mapping;
  const dialog = useRef<HTMLDialogElement>(null);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    const trigger = document.activeElement as HTMLElement | null;
    dialog.current?.showModal();
    return () => { mounted.current = false; dialog.current?.close(); trigger?.focus(); };
  }, []);

  useEffect(() => {
    const unlisten = listen<{ stage: string; message: string; done?: number; total?: number }>('douyin-progress', ({ payload }) => {
      if (!mounted.current) return;
      setState(current => current.phase !== 'running' ? current : {
        phase: 'running',
        step: STAGE_STEP[payload.stage] ?? current.step,
        note: payload.message,
        waitingLogin: payload.stage === 'waiting-login',
        bar: typeof payload.done === 'number' && typeof payload.total === 'number' && payload.total > 0
          ? { stage: payload.stage, done: payload.done, total: payload.total }
          : payload.stage === current.bar?.stage ? current.bar : undefined,
      });
    });
    return () => { void unlisten.then(stop => stop()); };
  }, []);

  function fail(reason: unknown) {
    if (!mounted.current) return;
    const message = String(reason).replace(/^Error:\s*/, '');
    setDialogError(message);
    onError(message);
    setState({ phase: 'idle' });
    setMapping(false);
  }

  async function start() {
    setDialogError('');
    setState({ phase: 'running', step: 'browser', note: '正在启动专用浏览器…', waitingLogin: false });
    try {
      const fetched = await collectDouyinFetch();
      if (!mounted.current) return;
      setState({ phase: 'running', step: 'download', note: `${fetched.detail}，正在导入资料库…`, waitingLogin: false });
      const result = await collectDouyinImport();
      if (!mounted.current) return;
      onSnapshot(result.snapshot, result.previewWarnings.length ? `部分素材无法预览：${result.previewWarnings.join('；')}` : '');
      const nickname = fetched.account?.nickname.trim();
      if (!nickname) {
        setState({ phase: 'map', reportPath: result.reportPath, report: result.report });
        return;
      }
      // The logged-in account is known: record provenance without asking.
      setState({ phase: 'running', step: 'map', note: `归属到抖音账号「${nickname}」`, waitingLogin: false });
      const existing = accounts.find(account => account.alias === nickname);
      onManagement(await importProvenanceAtPath(root, result.reportPath, existing?.alias ?? nickname, existing?.id ?? null));
      if (mounted.current) setState({ phase: 'done', report: result.report, account: nickname });
    } catch (reason) {
      fail(reason);
    }
  }

  async function mapAccount() {
    if (state.phase !== 'map') return;
    const account = accounts.find(value => value.id === accountId);
    const name = account?.alias ?? alias.trim();
    setMapping(true);
    try {
      onManagement(await importProvenanceAtPath(root, state.reportPath, name, account?.id ?? null));
      if (mounted.current) { setMapping(false); setState({ phase: 'done', report: state.report, account: name }); }
    } catch (reason) {
      fail(reason);
    }
  }

  const stepIndex = state.phase === 'running' ? STEPS.findIndex(step => step.key === state.step) : -1;

  return (
    <dialog className="detail-dialog collect-dialog" ref={dialog} onClose={() => { if (!dialog.current?.open) onClose(); }} aria-labelledby="collect-title" onCancel={event => { if (busy) event.preventDefault(); }}>
      <div className="detail-heading"><span className="eyebrow">抖音采集</span><button className="icon-button" autoFocus disabled={busy} onClick={() => dialog.current?.close()} aria-label="关闭采集"><X size={20} /></button></div>
      <div className="detail-content collect-content">
        <h2 id="collect-title">从抖音导入收藏表情</h2>
        {dialogError ? <p className="field-error" role="alert">{dialogError}</p> : null}

        {state.phase === 'idle' ? <>
          <p className="field-hint">拾趣会启动一个专用的 Chrome 窗口（与你日常使用的 Chrome 分开），自动读取收藏、下载原图并导入当前资料库。</p>
          <p className="field-hint">第一次需要在弹出的窗口里用抖音 App 扫码登录；之后会记住登录，采集在后台完成，不再弹窗。</p>
          <div className="editor-actions"><button className="button primary" onClick={() => void start()}>{dialogError ? '重试' : '开始采集'}</button><button className="button secondary" onClick={() => dialog.current?.close()}>取消</button></div>
        </> : null}

        {state.phase === 'running' ? <>
          <ol className="collect-steps">
            {STEPS.map((step, index) => (
              <li key={step.key} className={index < stepIndex ? 'done' : index === stepIndex ? 'active' : ''}>
                <span className="collect-step-icon">{index < stepIndex ? <Check size={13} /> : index === stepIndex ? <Loader2 size={13} className="spin" /> : null}</span>
                {step.label}
              </li>
            ))}
          </ol>
          {state.bar ? (
            <div className="collect-progress">
              <div className="collect-progress-label">
                <span>{BAR_LABEL[state.bar.stage] ?? '进度'}</span>
                <span>{state.bar.done} / {state.bar.total}</span>
              </div>
              <progress max={state.bar.total} value={state.bar.done} />
            </div>
          ) : null}
          <p role="status" className={state.waitingLogin ? 'collect-login' : 'field-hint'}>
            {state.waitingLogin ? '请在弹出的 Chrome 窗口里用抖音 App 扫码登录，登录后会自动继续（最多等待 5 分钟）。' : state.note}
          </p>
        </> : null}

        {state.phase === 'map' ? <>
          <p>入库完成：新增 {state.report.added} 个 · 重复 {state.report.duplicates} 个 · 失败 {state.report.failed.length} 个。没能读到抖音昵称，请选择这些收藏的账号归属：</p>
          <label className="field-label">抖音账号<select value={accountId} disabled={mapping} onChange={event => setAccountId(event.target.value)}>{accounts.map(account => <option key={account.id} value={account.id}>{account.alias}</option>)}<option value="new">新增账号…</option></select></label>
          {accountId === 'new' ? <label className="field-label">账号别名<input value={alias} maxLength={80} placeholder="例如：我的抖音" disabled={mapping} onChange={event => setAlias(event.target.value)} /></label> : null}
          <div className="editor-actions">
            <button className="button primary" disabled={mapping || (accountId === 'new' ? !alias.trim() : !accounts.some(a => a.id === accountId))} onClick={() => void mapAccount()}>{mapping ? '正在记录…' : '完成'}</button>
            <button className="button secondary" disabled={mapping} onClick={() => dialog.current?.close()}>稍后再说</button>
          </div>
        </> : null}

        {state.phase === 'done' ? <>
          <p role="status">已从抖音账号「{state.account}」导入：新增 {state.report.added} 个 · 已在库中 {state.report.duplicates} 个{state.report.failed.length ? ` · 失败 ${state.report.failed.length} 个` : ''}。</p>
          {state.report.failed.length ? <p className="field-hint">{state.report.failed.slice(0, 3).map(item => `${item.name}：${item.error}`).join('\n')}</p> : null}
          <div className="editor-actions"><button className="button primary" onClick={() => dialog.current?.close()}>完成</button></div>
        </> : null}
      </div>
    </dialog>
  );
}
