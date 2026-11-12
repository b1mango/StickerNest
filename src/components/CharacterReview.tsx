import { useEffect, useMemo, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { Check, ScanFace } from 'lucide-react';
import { imageUrl, thumbUrl, type CharacterSuggestion, type ClassifyResult, type CountProgress } from '../api';
import type { Sticker } from '../types';
import { StickerPreview } from './StickerPreview';

type Group = { character: string; items: CharacterSuggestion[] };

/** Groups suggestions per character, strongest first; confident ones start checked. */
function grouped(result: ClassifyResult | null): Group[] {
  if (!result) return [];
  const by = new Map<string, CharacterSuggestion[]>();
  for (const suggestion of result.suggestions) {
    const list = by.get(suggestion.character) ?? [];
    list.push(suggestion);
    by.set(suggestion.character, list);
  }
  return [...by.entries()]
    .map(([character, items]) => ({ character, items: items.sort((a, b) => b.score - a.score) }))
    .sort((a, b) => b.items.length - a.items.length);
}

export function CharacterReview({ result, running, items, root, thumbVersion, onRun, onAccept, onDismiss }: {
  result: ClassifyResult | null;
  running: boolean;
  items: Sticker[];
  root: string;
  thumbVersion: number;
  onRun: () => void;
  onAccept: (character: string, ids: string[]) => Promise<void>;
  onDismiss: (character: string, ids: string[]) => Promise<void>;
}) {
  const [progress, setProgress] = useState<CountProgress | null>(null);
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState('');
  const [error, setError] = useState('');
  const byId = useMemo(() => new Map(items.map(item => [item.id, item])), [items]);
  const groups = useMemo(() => grouped(result), [result]);

  useEffect(() => {
    const unlisten = listen<CountProgress>('classify-progress', ({ payload }) => setProgress(payload));
    return () => { void unlisten.then(stop => stop()); };
  }, []);
  useEffect(() => { if (!running) setProgress(null); }, [running]);
  // A fresh result re-seeds the checkboxes from confidence.
  useEffect(() => {
    setChecked(new Set(result?.suggestions.filter(s => s.confident).map(s => `${s.character}\u0000${s.assetId}`) ?? []));
  }, [result]);

  const key = (character: string, id: string) => `${character}\u0000${id}`;
  function toggle(character: string, id: string) {
    setChecked(current => {
      const next = new Set(current);
      const k = key(character, id);
      if (next.has(k)) next.delete(k); else next.add(k);
      return next;
    });
  }
  async function act(label: string, action: () => Promise<void>) {
    if (busy) return;
    setBusy(label); setError('');
    try { await action(); } catch (reason) { setError(String(reason).replace(/^Error:\s*/, '')); } finally { setBusy(''); }
  }

  if (running || !result) {
    return <section className="similar-page" aria-label="形象识别">
      <div className="similar-empty">
        <ScanFace size={30} className="empty-icon-plain" />
        <h2>{running ? '正在识别表情里的形象…' : '识别表情里的形象'}</h2>
        {running ? (
          <div className="collect-progress classify-progress">
            <div className="collect-progress-label">
              <span>分析表情</span>
              <span>{progress?.total ? `${progress.done} / ${progress.total}` : '加载模型…'}</span>
            </div>
            <progress max={progress?.total || 1} value={progress?.total ? progress.done : undefined} />
          </div>
        ) : <>
          <p>按形象包里的样例，找出可能属于各形象的表情，确认后加入同名合集。已加入合集的表情会作为新样例，越用越准。</p>
          <button className="button primary" onClick={onRun}>开始识别</button>
        </>}
      </div>
    </section>;
  }

  if (!groups.length) {
    return <section className="similar-page" aria-label="形象识别">
      <div className="similar-empty">
        <Check size={30} className="empty-icon-plain" />
        <h2>没有新的建议</h2>
        <p>{result.characters.length ? `已检查 ${result.characters.length} 个形象：${result.characters.join('、')}。` : '形象包里还没有形象。'}想识别得更全，可以把更多表情加入与形象同名的合集作为样例，再重新识别。</p>
      </div>
    </section>;
  }

  return <section className="similar-page" aria-label="形象识别">
    {error ? <p className="field-error" role="alert">{error}</p> : null}
    {groups.map(group => {
      const selected = group.items.filter(s => checked.has(key(group.character, s.assetId))).map(s => s.assetId);
      const rest = group.items.filter(s => !checked.has(key(group.character, s.assetId))).map(s => s.assetId);
      return <div key={group.character} className="character-group">
        <div className="character-head">
          <h2>{group.character}<span>{group.items.length} 个建议</span></h2>
          <div className="editor-actions">
            <button className="button secondary" disabled={!!busy || !rest.length}
              onClick={() => void act(`dismiss:${group.character}`, () => onDismiss(group.character, rest))}>
              {busy === `dismiss:${group.character}` ? '正在忽略…' : `忽略未勾选（${rest.length}）`}
            </button>
            <button className="button primary" disabled={!!busy || !selected.length}
              onClick={() => void act(`accept:${group.character}`, () => onAccept(group.character, selected))}>
              {busy === `accept:${group.character}` ? '正在加入…' : `加入「${group.character}」（${selected.length}）`}
            </button>
          </div>
        </div>
        <div className="character-grid">
          {group.items.map(suggestion => {
            const item = byId.get(suggestion.assetId);
            if (!item) return null;
            const on = checked.has(key(group.character, suggestion.assetId));
            return <button key={suggestion.assetId} className={`character-tile${on ? ' on' : ''}`} aria-pressed={on}
              title={`${item.name} · 相似度 ${suggestion.score.toFixed(2)}`}
              onClick={() => toggle(group.character, suggestion.assetId)}>
              <span className="character-thumb">
                <StickerPreview key={thumbVersion} src={thumbUrl(root, item.id, thumbVersion)} fallback={imageUrl(root, item.fileName)} name={item.name} eager />
              </span>
              <span className="character-check">{on ? <Check size={12} /> : null}</span>
              <span className={`character-score${suggestion.confident ? ' strong' : ''}`}>{Math.round(suggestion.score * 100)}</span>
            </button>;
          })}
        </div>
      </div>;
    })}
  </section>;
}
