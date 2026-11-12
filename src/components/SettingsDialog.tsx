import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { ScanFace } from 'lucide-react';
import * as api from '../api';
import { formatBytes } from '../types';
import { Modal } from './Modal';

/** Approximate download for the opt-in recognizer, shown before enabling. */
const DOWNLOAD_HINT = '约 176 MB';

export function SettingsDialog({ status, onStatus, onClose }: {
  status: api.ClassifierStatus | null;
  onStatus: (status: api.ClassifierStatus) => void;
  onClose: () => void;
}) {
  const [busy, setBusy] = useState<'' | 'enable' | 'disable'>('');
  const [progress, setProgress] = useState<api.CountProgress | null>(null);
  const [error, setError] = useState('');

  useEffect(() => {
    const unlisten = listen<api.CountProgress>('classifier-progress', ({ payload }) => setProgress(payload));
    return () => { void unlisten.then(stop => stop()); };
  }, []);

  async function run(kind: 'enable' | 'disable') {
    setBusy(kind); setError(''); setProgress(null);
    try {
      if (kind === 'enable') onStatus(await api.enableClassifier());
      else { await api.disableClassifier(); onStatus(await api.classifierStatus()); }
    } catch (reason) {
      setError(String(reason).replace(/^Error:\s*/, ''));
    } finally {
      setBusy(''); setProgress(null);
    }
  }

  const enabled = !!status?.enabled;
  return (
    <Modal title="设置" onClose={onClose} busy={!!busy} width={460}>
      <section className="settings-section">
        <div className="settings-row">
          <span className="settings-icon"><ScanFace size={18} /></span>
          <div className="settings-text">
            <strong>形象识别</strong>
            <p>识别表情里的形象（如草地牛、奶龙、chiikawa），建议归入同名合集。全部在本机运行，图片不会上传。</p>
            <p className="settings-meta">
              {enabled
                ? `已启用 · ${status!.characters.length} 个形象 · 占用 ${formatBytes(status!.bytes)}`
                : `未启用 · 启用时需下载识别模型与形象包（${DOWNLOAD_HINT}）`}
            </p>
          </div>
        </div>

        {busy === 'enable' ? (
          <div className="collect-progress">
            <div className="collect-progress-label">
              <span>正在下载识别模型</span>
              <span>{progress?.total ? `${formatBytes(progress.done)} / ${formatBytes(progress.total)}` : '准备中…'}</span>
            </div>
            <progress max={progress?.total || 1} value={progress?.total ? progress.done : undefined} />
          </div>
        ) : null}
        {error ? <p className="field-error" role="alert">{error}</p> : null}

        <div className="editor-actions">
          {enabled ? <>
            <button className="button secondary" disabled={!!busy} onClick={() => void run('enable')}>{busy === 'enable' ? '正在更新…' : '更新形象包'}</button>
            <button className="button secondary danger-text" disabled={!!busy} onClick={() => void run('disable')}>{busy === 'disable' ? '正在停用…' : '停用并删除模型'}</button>
          </> : (
            <button className="button primary" disabled={!!busy} onClick={() => void run('enable')}>{busy === 'enable' ? '正在下载…' : `启用（下载${DOWNLOAD_HINT}）`}</button>
          )}
        </div>
      </section>
      <div className="editor-actions settings-footer">
        <button className="button secondary" disabled={!!busy} onClick={onClose}>完成</button>
      </div>
    </Modal>
  );
}
