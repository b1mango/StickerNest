import { useEffect, useRef, useState } from 'react';
import { Check, X } from 'lucide-react';
import { imageUrl } from '../api';
import { formatBytes, type ScanReport, type Sticker } from '../types';
import { StickerPreview } from './StickerPreview';

function small(item?: Sticker) {
  return item ? `${item.format.toUpperCase()} · ${item.width}×${item.height} · ${formatBytes(item.bytes)}` : '';
}

function CandidateCard({ item, root, busy, keeping, onKeep }: {
  item?: Sticker; root: string; busy: boolean; keeping: string; onKeep: (id: string) => void;
}) {
  if (!item) return <div className="pair-card missing">素材缺失</div>;
  const key = `keep:${item.id}`;
  return <div className="pair-card keep-card">
    <StickerPreview src={imageUrl(root, item.fileName)} name={item.name} eager />
    <span className="pair-caption" title={item.name}>{item.name}</span>
    <span className="pair-meta">{small(item)}</span>
    <button className="button primary slim" disabled={busy} onClick={() => onKeep(item.id)}><Check size={14} />{keeping === key ? '正在保留…' : '保留'}</button>
  </div>;
}

export function DuplicateReview({ report, items, root, existingGroups, onKeep, onIgnorePair, onRescan, onClose }: {
  report: ScanReport;
  items: Sticker[];
  root: string;
  existingGroups: { mainAssetId: string; memberIds: string[] }[];
  onKeep: (keepId: string, removedIds: string[]) => Promise<void>;
  onIgnorePair: (a: string, b: string) => Promise<void>;
  onRescan: () => Promise<void>;
  onClose: () => void;
}) {
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
  function visible(ids: string[]) { return ids.filter(id => !groupedIds.has(id)); }
  async function act(key: string, action: () => Promise<void>) {
    if (busyKey) return;
    setBusyKey(key); setError('');
    // Keeping/ignoring removes an entry; rescan so the list stays truthful.
    try { await action(); await onRescan(); }
    catch (reason) { setError(String(reason)); }
    finally { setBusyKey(''); }
  }
  async function keepOne(keepId: string, members: string[]) {
    const removed = members.filter(id => id !== keepId && byId.has(id));
    await act(`keep:${keepId}`, () => onKeep(keepId, removed));
  }
  const exactVisible = report.exactGroups
    .map(group => ({ key: `exact:${group.pixelHash}`, members: visible(group.assetIds) }))
    .filter(entry => entry.members.length >= 2);
  const similarVisible = report.similarPairs.filter(pair => !groupedIds.has(pair.baseId) && !groupedIds.has(pair.otherId));
  const animationVisible = report.animationPairs.filter(pair => !groupedIds.has(pair.baseId) && !groupedIds.has(pair.otherId));
  return (
    <dialog className="detail-dialog duplicates-dialog" ref={dialog} onClose={() => { if (!dialog.current?.open) onClose(); }} aria-labelledby="duplicates-title">
      <div className="detail-heading"><span className="eyebrow">查重审阅</span><button className="icon-button" autoFocus disabled={!!busyKey} onClick={() => dialog.current?.close()} aria-label="关闭查重"><X size={20} /></button></div>
      <div className="detail-content duplicates-content">
        <h2 id="duplicates-title">去重审阅</h2>
        <p className="duplicates-summary">
          已扫描 {report.scannedStatics} 个静态素材、{report.scannedAnimations} 个动画；过小 {report.skippedTiny} 个{report.failed.length ? `；失败 ${report.failed.length} 个` : ''}。
          在每组里点「保留」留下要的那张，其余移入回收站（可随时在回收站恢复）。也可以整组忽略。
        </p>
        {error ? <p className="field-error" role="alert">{error}</p> : null}
        {exactVisible.length === 0 && similarVisible.length === 0 && animationVisible.length === 0 ? <p className="duplicates-empty">没有需要处理的重复或相似候选。</p> : null}
        {exactVisible.map(entry => (
          <div className="duplicate-section" key={entry.key}>
            <h3>画面完全相同 · {entry.members.length} 个版本</h3>
            <div className="pair-row">{entry.members.map(id => <CandidateCard key={id} item={byId.get(id)} root={root} busy={!!busyKey} keeping={busyKey} onKeep={keepId => void keepOne(keepId, entry.members)} />)}</div>
          </div>
        ))}
        {similarVisible.map(pair => {
          const key = `pair:${pair.baseId}:${pair.otherId}`;
          const members = [pair.baseId, pair.otherId];
          return <div className="duplicate-section" key={key}>
            <h3>疑似相似 · 距离 {pair.distance}</h3>
            <div className="pair-row">{members.map(id => <CandidateCard key={id} item={byId.get(id)} root={root} busy={!!busyKey} keeping={busyKey} onKeep={keepId => void keepOne(keepId, members)} />)}</div>
            <button className="text-button" disabled={!!busyKey} onClick={() => void act(`ignore:${key}`, () => onIgnorePair(pair.baseId, pair.otherId))}>忽略这对</button>
          </div>;
        })}
        {animationVisible.map(pair => {
          const key = `anim:${pair.baseId}:${pair.otherId}`;
          const members = [pair.baseId, pair.otherId];
          return <div className="duplicate-section" key={key}>
            <h3>疑似相同动画 · 距离 {pair.distance} · {pair.frames} 帧 · {Math.round(pair.durationMs / 100) / 10} 秒</h3>
            <div className="pair-row">{members.map(id => <CandidateCard key={id} item={byId.get(id)} root={root} busy={!!busyKey} keeping={busyKey} onKeep={keepId => void keepOne(keepId, members)} />)}</div>
            <p className="field-hint">两段动画可分别播放核对；保留前请人工确认，另一段会移入回收站（可恢复）。</p>
            <button className="text-button" disabled={!!busyKey} onClick={() => void act(`ignore:${key}`, () => onIgnorePair(pair.baseId, pair.otherId))}>忽略这对</button>
          </div>;
        })}
      </div>
    </dialog>
  );
}
