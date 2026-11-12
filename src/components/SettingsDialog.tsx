import { useEffect, useState, type ReactNode } from 'react';
import { getVersion } from '@tauri-apps/api/app';
import { HardDrive, Info, Music2, Palette, PlayCircle } from 'lucide-react';
import * as api from '../api';
import type { AnimatePref, Prefs, ThemePref } from '../hooks/usePrefs';
import { formatBytes } from '../types';
import { Modal } from './Modal';

const THEMES: [ThemePref, string][] = [['system', '跟随系统'], ['light', '浅色'], ['dark', '深色']];
const ANIMATE: [AnimatePref, string][] = [['hover', '悬停时播放'], ['always', '始终播放'], ['never', '不播放']];
const CACHES: { kind: api.CacheKind; label: string; hint: string }[] = [
  { kind: 'thumbs', label: '缩略图', hint: '清理后会自动重新生成' },
  { kind: 'recognition', label: '形象识别缓存', hint: '图像特征与文字识别结果，下次识别时重新计算；不影响已忽略的建议' },
  { kind: 'downloads', label: '采集下载暂存', hint: '抖音、微信采集时下载的原件副本，已导入资料库的表情不受影响；清理后无法补做「稍后映射」账号' },
];

function Section({ icon, title, children }: { icon: ReactNode; title: string; children: ReactNode }) {
  return <section className="settings-section">
    <div className="settings-row">
      <span className="settings-icon">{icon}</span>
      <div className="settings-text"><strong>{title}</strong>{children}</div>
    </div>
  </section>;
}

function Segmented<T extends string>({ value, options, onChange, label }: { value: T; options: [T, string][]; onChange: (value: T) => void; label: string }) {
  return <div className="segmented settings-segmented" role="group" aria-label={label}>
    {options.map(([key, text]) => <button key={key} aria-pressed={value === key} onClick={() => onChange(key)}>{text}</button>)}
  </div>;
}

export function SettingsDialog({ root, prefs, onPrefs, onCacheCleared, onClose }: {
  root: string;
  prefs: Prefs;
  onPrefs: (patch: Partial<Prefs>) => void;
  onCacheCleared: (kind: api.CacheKind) => void;
  onClose: () => void;
}) {
  const [usage, setUsage] = useState<api.CacheUsage | null>(null);
  const [douyin, setDouyin] = useState<{ exists: boolean; bytes: number } | null>(null);
  const [version, setVersion] = useState('');
  const [busy, setBusy] = useState('');
  const [error, setError] = useState('');

  const refresh = () => {
    if (root) api.cacheUsage(root).then(setUsage, () => setUsage(null));
    api.douyinBrowserStatus().then(setDouyin, () => setDouyin(null));
  };
  useEffect(() => {
    refresh();
    getVersion().then(setVersion, () => setVersion(''));
  }, [root]);

  async function act(key: string, action: () => Promise<void>) {
    setBusy(key); setError('');
    try { await action(); refresh(); } catch (reason) { setError(String(reason).replace(/^Error:\s*/, '')); } finally { setBusy(''); }
  }

  return (
    <Modal title="设置" onClose={onClose} busy={!!busy} width={500} className="settings-sheet">
      <Section icon={<Palette size={18} />} title="外观">
        <p>窗口配色；跟随系统时随 macOS 浅色 / 深色切换。</p>
        <Segmented label="外观" value={prefs.theme} options={THEMES} onChange={theme => onPrefs({ theme })} />
      </Section>

      <Section icon={<PlayCircle size={18} />} title="动图播放">
        <p>网格里的 GIF、动图 WebP 怎么播放。动图原图较大，「始终播放」在表情很多时可能卡顿。</p>
        <Segmented label="动图播放" value={prefs.animate} options={ANIMATE} onChange={animate => onPrefs({ animate })} />
      </Section>

      <Section icon={<Music2 size={18} />} title="抖音采集">
        <p>采集用的专用浏览器会记住抖音登录，下次无需扫码。</p>
        <div className="settings-line">
          <span className="settings-meta">{douyin?.exists ? `已保存登录数据 · ${formatBytes(douyin.bytes)}` : '未登录'}</span>
          <button className="button secondary danger-text" disabled={!douyin?.exists || !!busy}
            onClick={() => void act('douyin', api.clearDouyinBrowser)}>{busy === 'douyin' ? '正在清除…' : '退出登录并清除'}</button>
        </div>
      </Section>

      <Section icon={<HardDrive size={18} />} title="存储">
        <p>以下都是可重新生成的数据，清理不会删除任何表情。</p>
        {CACHES.map(cache => (
          <div key={cache.kind} className="settings-line" title={cache.hint}>
            <span className="settings-cache">
              <span>{cache.label}</span>
              <small>{cache.hint}</small>
            </span>
            <span className="settings-meta">{usage ? formatBytes(usage[cache.kind]) : '—'}</span>
            <button className="button secondary" disabled={!root || !usage?.[cache.kind] || !!busy}
              onClick={() => void act(cache.kind, async () => { await api.clearCache(root, cache.kind); onCacheCleared(cache.kind); })}>
              {busy === cache.kind ? '正在清理…' : '清理'}
            </button>
          </div>
        ))}
      </Section>

      <Section icon={<Info size={18} />} title="关于">
        <p>拾趣 · StickerNest{version ? ` ${version}` : ''} · github.com/b1mango/StickerNest</p>
      </Section>

      {error ? <p className="field-error" role="alert">{error}</p> : null}
      <div className="editor-actions"><button className="button primary" disabled={!!busy} onClick={onClose}>完成</button></div>
    </Modal>
  );
}
