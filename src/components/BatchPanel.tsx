import { useRef, useState } from 'react';
import { X } from 'lucide-react';

function splitValues(value: string): string[] {
  return [...new Set(value.split(/[,，]/).map(v => v.trim()).filter(Boolean))];
}

export function BatchPanel({ count, disabled, onRename, onLabels, onTrash, onClose }: {
  count: number;
  disabled: boolean;
  onRename: (prefix: string, start: number) => Promise<void>;
  onLabels: (addTags: string[], removeTags: string[], addCollections: string[], removeCollections: string[]) => Promise<void>;
  onTrash: () => Promise<void>;
  onClose: () => void;
}) {
  const [prefix, setPrefix] = useState('');
  const [start, setStart] = useState('1');
  const [addTags, setAddTags] = useState('');
  const [removeTags, setRemoveTags] = useState('');
  const [confirmTrash, setConfirmTrash] = useState(false);
  const [saving, setSaving] = useState('');
  const [error, setError] = useState('');
  const savingRef = useRef(false);
  async function act(key: string, action: () => Promise<void>) {
    if (savingRef.current) return;
    savingRef.current = true; setSaving(key); setError('');
    try { await action(); }
    catch (reason) { setError(String(reason)); }
    finally { savingRef.current = false; setSaving(''); }
  }
  const startNumber = Number.parseInt(start, 10);
  return <section className="batch-panel" aria-label="批量整理">
    <div className="batch-heading"><strong>批量整理 {count} 个项目</strong><button className="icon-button" aria-label="关闭批量整理" onClick={onClose}><X size={16} /></button></div>
    <div className="batch-section">
      <span className="batch-label">重命名</span>
      <input value={prefix} maxLength={190} disabled={!!saving} placeholder="前缀（如 龙图）" aria-label="重命名前缀" onChange={event => setPrefix(event.target.value)} />
      <input value={start} inputMode="numeric" className="batch-number" disabled={!!saving} aria-label="起始序号" onChange={event => setStart(event.target.value)} />
      <button className="button secondary" disabled={!!saving || disabled || !prefix.trim() || !Number.isFinite(startNumber) || startNumber <= 0 || String(startNumber) !== start.trim()} onClick={() => void act('rename', () => onRename(prefix.trim(), startNumber))}>{saving === 'rename' ? '正在重命名…' : '重命名'}</button>
      <p className="field-hint">显示名会变为 {prefix.trim() || '前缀'}-001 这样的序号名，不改动原文件。</p>
    </div>
    <div className="batch-section">
      <span className="batch-label">标签</span>
      <input value={addTags} disabled={!!saving} placeholder="批量添加，逗号分隔" aria-label="批量添加标签" onChange={event => setAddTags(event.target.value)} />
      <input value={removeTags} disabled={!!saving} placeholder="批量移除，逗号分隔" aria-label="批量移除标签" onChange={event => setRemoveTags(event.target.value)} />
      <button className="button secondary" disabled={!!saving || disabled || (!addTags.trim() && !removeTags.trim())} onClick={() => void act('tags', () => {
        const add = splitValues(addTags), remove = splitValues(removeTags);
        const both = add.filter(v => remove.includes(v));
        if (both.length) throw new Error(`“${both.join('、')}”同时在添加和移除中，请保留一边。`);
        return onLabels(add, remove, [], []);
      })}>应用标签</button>
    </div>
    {error ? <p className="field-error" role="alert">{error}</p> : null}
    {!confirmTrash
      ? <button className="text-button danger-link" disabled={!!saving || disabled} onClick={() => { setConfirmTrash(true); setError(''); }}>移入回收站</button>
      : <div className="batch-section" role="alert">
          <p>选中的 {count} 项会移到回收站；文件保留在本地。分组中的素材会先被拒绝，需要拆分分组。</p>
          <div className="editor-actions"><button className="button danger" disabled={!!saving} onClick={() => void act('trash', onTrash)}>{saving === 'trash' ? '正在移入…' : '确认移入'}</button><button className="button secondary" disabled={!!saving} onClick={() => setConfirmTrash(false)}>取消</button></div>
        </div>}
  </section>;
}
