import { useEffect, useRef, useState } from 'react';
import { X } from 'lucide-react';
import { imageUrl } from '../api';
import { formatBytes, type ScanReport, type Sticker } from '../types';
import { StickerPreview } from './StickerPreview';

function small(item?: Sticker) {
  return item ? `${item.format.toUpperCase()} · ${item.width}×${item.height} · ${formatBytes(item.bytes)}` : '';
}

function Card({ item, root, size = 72 }: { item?: Sticker; root: string; size?: number }) {
  if (!item) return <div className="pair-card missing">素材缺失</div>;
  return <div className="pair-card" style={{ width: size }}>
    <StickerPreview src={imageUrl(root, item.fileName)} name={item.name} eager />
    <span className="pair-caption" title={item.name}>{item.name}</span>
    <span className="pair-meta">{small(item)}</span>
  </div>;
}

export function DuplicateReview({ report, items, root, existingGroups, onGroup, onIgnorePair, onRescan, onClose }: {
  report: ScanReport;
  items: Sticker[];
  root: string;
  existingGroups: { mainAssetId: string; memberIds: string[] }[];
  onGroup: (memberIds: string[], mainAssetId: string) => Promise<void>;
  onIgnorePair: (a: string, b: string) => Promise<void>;
  onRescan: () => Promise<void>;
  onClose: () => void;
}) {
  const [mainChoice, setMainChoice] = useState<Record<string, string>>({});
  const [busyKey, setBusyKey] = useState('');
  const [error, setError] = useState('');
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const trigger = document.activeElement as HTMLElement | null;
    dialog.current?.showModal();
    return () => { dialog.current?.close(); trigger?.focus(); };
  }, []);
  const byId = new Map(items.map(item => [item.id, item]));
  const groupedIds = new Set(existingGroups.flatMap(group => group.memberIds));
  async function act(key: string, action: () => Promise<void>) {
    if (busyKey) return;
    setBusyKey(key); setError('');
    // Grouping or ignoring removes entries; rescan so the list stays truthful.
    try { await action(); await onRescan(); }
    catch (reason) { setError(String(reason)); }
    finally { setBusyKey(''); }
  }
  return (
    <dialog className="detail-dialog duplicates-dialog" ref={dialog} onClose={() => { if (!dialog.current?.open) onClose(); }} aria-labelledby="duplicates-title">
      <div className="detail-heading"><span className="eyebrow">查重审阅</span><button className="icon-button" autoFocus disabled={!!busyKey} onClick={() => dialog.current?.close()} aria-label="关闭查重"><X size={20} /></button></div>
      <div className="detail-content duplicates-content">
        <h2 id="duplicates-title">重复与相似候选</h2>
        <p className="duplicates-summary">
          已扫描 {report.scannedStatics} 个静态素材、{report.scannedAnimations} 个动画；过小 {report.skippedTiny} 个{report.failed.length ? `；失败 ${report.failed.length} 个` : ''}。相似对仅供人工确认，不自动合并；动画要求帧数相同、时长接近，不与静图混比。
        </p>
        {error ? <p className="field-error" role="alert">{error}</p> : null}
        {report.exactGroups.every(group => group.assetIds.filter(id => !groupedIds.has(id)).length < 2) && report.similarPairs.filter(pair => !groupedIds.has(pair.baseId) && !groupedIds.has(pair.otherId)).length === 0 && report.animationPairs.filter(pair => !groupedIds.has(pair.baseId) && !groupedIds.has(pair.otherId)).length === 0 ? <p className="duplicates-empty">没有需要处理的重复或相似候选。</p> : null}
        {report.exactGroups.map(group => {
          const key = `exact:${group.pixelHash}`;
          const pending = group.assetIds.filter(id => !groupedIds.has(id));
          if (pending.length < 2) return null;
          const main = mainChoice[key] ?? pending[0];
          return <div className="duplicate-section" key={key}>
            <h3>画面完全相同 · {pending.length} 个版本</h3>
            <div className="pair-row">{pending.map(id => <label className="pair-choice" key={id}><Card item={byId.get(id)} root={root} /><span className="pair-radio"><input type="radio" name={key} checked={main === id} onChange={() => setMainChoice(choices => ({ ...choices, [key]: id }))} />主展示</span></label>)}</div>
            <button className="button primary" disabled={!!busyKey} onClick={() => void act(key, () => onGroup(pending, main))}>归为版本组</button>
          </div>;
        })}
        {report.similarPairs.filter(pair => !groupedIds.has(pair.baseId) && !groupedIds.has(pair.otherId)).map(pair => {
          const key = `pair:${pair.baseId}:${pair.otherId}`;
          const main = mainChoice[key] ?? pair.baseId;
          return <div className="duplicate-section" key={key}>
            <h3>疑似相似 · 距离 {pair.distance}</h3>
            <div className="pair-row">
              {[pair.baseId, pair.otherId].map(id => <label className="pair-choice" key={id}><Card item={byId.get(id)} root={root} size={120} /><span className="pair-radio"><input type="radio" name={key} checked={main === id} onChange={() => setMainChoice(choices => ({ ...choices, [key]: id }))} />主展示</span></label>)}
            </div>
            <div className="editor-actions">
              <button className="button primary" disabled={!!busyKey} onClick={() => void act(key, () => onGroup([pair.baseId, pair.otherId], main))}>归为版本组</button>
              <button className="button secondary" disabled={!!busyKey} onClick={() => void act(key, () => onIgnorePair(pair.baseId, pair.otherId))}>忽略这对</button>
            </div>
          </div>;
        })}
        {report.animationPairs.filter(pair => !groupedIds.has(pair.baseId) && !groupedIds.has(pair.otherId)).map(pair => {
          const key = `anim:${pair.baseId}:${pair.otherId}`;
          const main = mainChoice[key] ?? pair.baseId;
          return <div className="duplicate-section" key={key}>
            <h3>疑似相同动画 · 距离 {pair.distance} · {pair.frames} 帧 · {Math.round(pair.durationMs / 100) / 10} 秒</h3>
            <div className="pair-row">
              {[pair.baseId, pair.otherId].map(id => <label className="pair-choice" key={id}><Card item={byId.get(id)} root={root} size={120} /><span className="pair-radio"><input type="radio" name={key} checked={main === id} onChange={() => setMainChoice(choices => ({ ...choices, [key]: id }))} />主展示</span></label>)}
            </div>
            <p className="field-hint">两段动画可分别播放核对；样本帧接近不代表完全一致，归组前请人工确认。</p>
            <div className="editor-actions">
              <button className="button primary" disabled={!!busyKey} onClick={() => void act(key, () => onGroup([pair.baseId, pair.otherId], main))}>归为版本组</button>
              <button className="button secondary" disabled={!!busyKey} onClick={() => void act(key, () => onIgnorePair(pair.baseId, pair.otherId))}>忽略这对</button>
            </div>
          </div>;
        })}
      </div>
    </dialog>
  );
}
