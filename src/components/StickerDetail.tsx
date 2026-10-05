import { useEffect, useRef, useState } from 'react';
import { X } from 'lucide-react';
import { imageUrl } from '../api';
import { formatBytes, SOURCE_LABELS, type Sticker, type AssetMetadata } from '../types';
import { StickerPreview } from './StickerPreview';

export function StickerDetail({ item, root, metadata, editable, trashed, trashedAt, onSave, onSetTrash, onClose }: {
  item: Sticker; root: string; metadata?: AssetMetadata; editable: boolean;
  trashed: boolean; trashedAt?: number;
  onSave: (metadata: AssetMetadata) => Promise<void>;
  onSetTrash: (trashed: boolean) => Promise<void>; onClose: () => void;
}) {
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState(metadata?.name || item.name);
  const [tags, setTags] = useState(metadata?.tags.join('，') ?? '');
  const [collections, setCollections] = useState(metadata?.collections.join('，') ?? '');
  const [saving, setSaving] = useState(false);
  const [confirmingTrash, setConfirmingTrash] = useState(false);
  const [trashing, setTrashing] = useState(false);
  const [error, setError] = useState('');
  const savingRef = useRef(false);
  const trashingRef = useRef(false);
  const displayName = metadata?.name || item.name;
  function startEditing() {
    // Metadata may finish loading after this dialog was opened.
    setName(displayName); setTags(metadata?.tags.join('，') ?? ''); setCollections(metadata?.collections.join('，') ?? ''); setError(''); setEditing(true);
  }
  function cancelEditing() {
    setName(displayName); setTags(metadata?.tags.join('，') ?? ''); setCollections(metadata?.collections.join('，') ?? ''); setError(''); setEditing(false);
  }
  async function save() {
    if (savingRef.current || !editable) return;
    if (!name.trim()) { setError('请填写表情名称。'); return; }
    await saveWith(name, tags, collections);
  }
  async function saveWith(nameValue: string, tagsValue: string, collectionsValue: string) {
    savingRef.current = true; setSaving(true); setError('');
    const split = (value: string) => [...new Set(value.split(/[,，]/).map(value => value.trim()).filter(Boolean))];
    // An empty name falls back to the manifest file name (reset to default).
    try { await onSave({ name: nameValue.trim(), tags: split(tagsValue), collections: split(collectionsValue) }); setEditing(false); }
    catch (reason) { setError(String(reason)); }
    finally { savingRef.current = false; setSaving(false); }
  }
  async function resetToDefaultName() {
    if (savingRef.current || !editable) return;
    await saveWith('', tags, collections);
  }
  async function setTrashState(value: boolean) {
    if (trashingRef.current) return;
    trashingRef.current = true; setTrashing(true); setError('');
    // On success the parent closes this dialog; only a failure needs a state reset.
    try { await onSetTrash(value); }
    catch (reason) { setError(String(reason)); trashingRef.current = false; setTrashing(false); }
  }
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const trigger = document.activeElement as HTMLElement | null;
    const element = dialog.current;
    element?.showModal();
    return () => {
      element?.close();
      trigger?.focus();
    };
  }, []);

  return (
    <dialog className="detail-dialog" ref={dialog} onClose={() => { if (!dialog.current?.open) onClose(); }} aria-labelledby="detail-title" onCancel={event => { if (savingRef.current || trashingRef.current) event.preventDefault(); }}>
      <div className="detail-heading"><span className="eyebrow">素材详情</span><button className="icon-button" autoFocus disabled={saving || trashing} onClick={() => dialog.current?.close()} aria-label="关闭详情"><X size={20} /></button></div>
      <div className="detail-preview"><StickerPreview src={imageUrl(root, item.fileName)} name={displayName} eager /></div>
      <div className="detail-content">
        <h2 id="detail-title">{displayName}</h2>
        {editing ? <form className="metadata-editor" onSubmit={event => { event.preventDefault(); void save(); }}>
          <label className="field-label">名称<input value={name} maxLength={200} required disabled={saving} onChange={event => setName(event.target.value)} /></label>
          <label className="field-label">标签<input value={tags} disabled={saving} placeholder="例如：开心，猫咪" onChange={event => setTags(event.target.value)} /></label>
          <label className="field-label">合集<input value={collections} disabled={saving} placeholder="例如：日常回复，工作群" onChange={event => setCollections(event.target.value)} /></label>
          <p className="field-hint">多个标签或合集用逗号分隔。</p>
          {error ? <p className="field-error" role="alert">{error}</p> : null}
          <div className="editor-actions"><button className="button primary" disabled={saving || !name.trim()} type="submit">{saving ? '正在保存…' : '保存整理'}</button><button className="button secondary" type="button" disabled={saving} onClick={cancelEditing}>取消</button>{metadata?.name ? <button className="text-button" type="button" disabled={saving} onClick={() => void resetToDefaultName()}>恢复默认名</button> : null}</div>
        </form> : <div className="detail-organization"><p>标签：{metadata?.tags.join('、') || '未添加'}</p><p>合集：{metadata?.collections.join('、') || '未加入'}</p><div className="organization-actions"><button className="text-button" disabled={!editable} onClick={startEditing}>编辑名称、标签与合集</button>{!trashed && !confirmingTrash ? <button className="text-button danger-link" disabled={!editable || trashing} onClick={() => { setConfirmingTrash(true); setError(''); }}>移入回收站</button> : null}</div>
          {trashed ? <div className="trash-notice" role="status"><p>已移入回收站{trashedAt ? `（${new Date(trashedAt * 1000).toLocaleString('zh-CN')}）` : ''}，文件仍保留在本地。</p><button className="button secondary" disabled={!editable || trashing} onClick={() => void setTrashState(false)}>{trashing ? '正在恢复…' : '恢复素材'}</button></div> : null}
          {confirmingTrash ? <div className="trash-notice" role="alert"><p>移入回收站后不再显示在列表中；文件保留在本地，可随时恢复。</p>{error ? <p className="field-error" role="alert">{error}</p> : null}<div className="editor-actions"><button className="button danger" disabled={trashing} onClick={() => void setTrashState(true)}>{trashing ? '正在移入…' : '确认移入'}</button><button className="button secondary" disabled={trashing} onClick={() => setConfirmingTrash(false)}>取消</button></div></div> : null}
          {trashed && error ? <p className="field-error" role="alert">{error}</p> : null}
        </div>}
        <dl className="metadata">
          <div><dt>来源</dt><dd>{item.sources.map(source => SOURCE_LABELS[source] ?? source).join('、')}</dd></div>
          <div><dt>文件格式</dt><dd>{item.format.toUpperCase()}</dd></div>
          <div><dt>尺寸</dt><dd>{item.width} × {item.height} px</dd></div>
          <div><dt>文件大小</dt><dd>{formatBytes(item.bytes)}</dd></div>
          <div><dt>导入时间</dt><dd>{new Date(item.importedAt * 1000).toLocaleString('zh-CN')}</dd></div>
        </dl>
        <p className="detail-path">{root}/assets/{item.fileName}</p>
      </div>
    </dialog>
  );
}
