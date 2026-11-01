import { useEffect, useRef, useState } from 'react';
import { AlertTriangle, Copy, FolderSearch, Layers, MoreHorizontal, Plus, RotateCcw, Star, Trash2, X } from 'lucide-react';
import { imageUrl } from '../api';
import { baseName, SOURCES, type LibraryIndex } from '../lib/derive';
import { formatBytes, type ManagementSnapshot, type Sticker } from '../types';
import { StickerPreview } from './StickerPreview';

const WECHAT_LIMIT = 1024 * 1024;

export interface InspectorActions {
  copy: (ids: string[]) => void;
  favorite: (ids: string[], on: boolean) => void;
  reveal: (ids: string[]) => void;
  more: (ids: string[], anchor: DOMRect) => void;
  trash: (ids: string[]) => void;
  restore: (ids: string[]) => void;
  deleteForever: (ids: string[]) => void;
  rename: (id: string, name: string) => Promise<void>;
  saveTags: (id: string, tags: string[]) => Promise<void>;
  collectionPicker: (ids: string[], anchor: DOMRect) => void;
  leaveCollection: (ids: string[], name: string) => void;
  batchRename: (ids: string[]) => void;
  disband: (groupId: string) => void;
  openVersion: (id: string) => void;
}

function formatDateTime(seconds: number) {
  return new Date(seconds * 1000).toLocaleString('zh-CN', { year: 'numeric', month: 'long', day: 'numeric', hour: '2-digit', minute: '2-digit' });
}

function Meta({ label, children }: { label: string; children: React.ReactNode }) {
  return <div className="meta-row"><dt>{label}</dt><dd>{children}</dd></div>;
}

function NameField({ item, original, onRename, focusToken }: {
  item: Sticker; original: string; onRename: (name: string) => Promise<void>; focusToken: number;
}) {
  const [value, setValue] = useState(item.name);
  const input = useRef<HTMLInputElement>(null);
  const skipBlur = useRef(false);
  useEffect(() => { setValue(item.name); }, [item.id, item.name]);
  useEffect(() => { if (focusToken) { input.current?.focus(); input.current?.select(); } }, [focusToken]);
  async function commit() {
    const next = value.trim();
    if (next === item.name || (!next && item.name === original)) { setValue(item.name); return; }
    try { await onRename(next); }
    catch { setValue(item.name); }
  }
  return (
    <input
      ref={input}
      className="name-field"
      value={value}
      aria-label="名称"
      placeholder={original}
      title="点按编辑名称；清空恢复原始文件名"
      onChange={event => setValue(event.target.value)}
      onBlur={() => { if (skipBlur.current) { skipBlur.current = false; return; } void commit(); }}
      onKeyDown={event => {
        if (event.nativeEvent.isComposing) return;
        if (event.key === 'Enter') { event.preventDefault(); event.currentTarget.blur(); }
        if (event.key === 'Escape') { event.preventDefault(); skipBlur.current = true; setValue(item.name); event.currentTarget.blur(); }
      }}
    />
  );
}

function TagsField({ id, tags, onSave }: { id: string; tags: string[]; onSave: (tags: string[]) => Promise<void> }) {
  const joined = tags.join('，');
  const [value, setValue] = useState(joined);
  useEffect(() => { setValue(joined); }, [id, joined]);
  const parse = (text: string) => [...new Set(text.split(/[,，、\s]+/).map(tag => tag.trim()).filter(Boolean))];
  return (
    <input className="tags-field" value={value} placeholder="添加标签，用逗号分隔" aria-label="标签"
      onChange={event => setValue(event.target.value)}
      onBlur={() => {
        const next = parse(value);
        if (next.join('，') === joined) { setValue(joined); return; }
        void onSave(next).catch(() => setValue(joined));
      }}
      onKeyDown={event => { if (event.key === 'Enter' && !event.nativeEvent.isComposing) event.currentTarget.blur(); }} />
  );
}

function CollectionChips({ ids, names, counts, actions }: {
  ids: string[]; names: string[]; counts?: Map<string, number>; actions: InspectorActions;
}) {
  return (
    <div className="chips">
      {names.map(name => (
        <span key={name} className="chip" title={counts && counts.get(name)! < ids.length ? `选中项中有 ${counts.get(name)} 个在此合集` : undefined}>
          {name}{counts && counts.get(name)! < ids.length ? <span className="chip-count">{counts.get(name)}</span> : null}
          <button aria-label={`从「${name}」移出`} title="移出合集" onClick={() => actions.leaveCollection(ids, name)}><X size={11} /></button>
        </span>
      ))}
      <button className="chip chip-add" onClick={event => actions.collectionPicker(ids, event.currentTarget.getBoundingClientRect())}><Plus size={12} />加入合集</button>
    </div>
  );
}

function LibrarySummary({ index, library }: { index: LibraryIndex; library: string }) {
  const bytes = index.items.reduce((sum, item) => sum + (index.trashed.has(item.id) ? 0 : item.bytes), 0);
  return (
    <div className="inspector-body">
      <div className="inspector-hero empty">
        <img src="/icon.png" alt="" draggable={false} />
      </div>
      <h2 className="inspector-title">{library}</h2>
      <p className="inspector-sub">{index.counts.all} 个表情 · {formatBytes(bytes)}</p>
      <section className="inspector-section">
        <h3>来源</h3>
        <dl className="meta">
          {SOURCES.map(source => <Meta key={source} label={source === '本地' ? '本地导入' : source}>{index.counts.sources[source] ?? 0}</Meta>)}
          <Meta label="合集">{index.collections.length}</Meta>
          <Meta label="回收站">{index.counts.trash}</Meta>
        </dl>
      </section>
      <section className="inspector-section">
        <h3>小技巧</h3>
        <ul className="tips">
          <li><kbd>⌘C</kbd> 复制选中的表情，直接粘贴到微信</li>
          <li><kbd>空格</kbd> 快速查看，<kbd>←</kbd><kbd>→</kbd> 切换</li>
          <li>把表情拖到微信聊天窗口或左侧合集</li>
          <li>把图片或文件夹拖进窗口即可导入</li>
        </ul>
      </section>
    </div>
  );
}

export function Inspector({ ids, index, management, root, libraryName, inTrash, focusToken, actions }: {
  ids: string[];
  index: LibraryIndex;
  management: ManagementSnapshot | null;
  root: string;
  libraryName: string;
  inTrash: boolean;
  focusToken: number;
  actions: InspectorActions;
}) {
  const items = ids.map(id => index.byId.get(id)).filter((item): item is Sticker => !!item);
  if (!items.length) return <aside className="inspector" aria-label="简介"><LibrarySummary index={index} library={libraryName} /></aside>;

  if (items.length === 1) {
    const item = items[0];
    const group = index.groupByMain.get(item.id);
    const favorite = !!index.favorites[item.id];
    const collections = index.collectionsOf.get(item.id) ?? [];
    const ownTags = management?.metadata[item.id]?.tags ?? [];
    const original = baseName(item.fileName);
    const trashedAt = management?.trash[item.id];
    return (
      <aside className="inspector" aria-label="简介">
        <div className="inspector-body">
          <div className="inspector-hero"><StickerPreview key={item.id} src={imageUrl(root, item.fileName)} name={item.name} eager /></div>
          <NameField item={item} original={original} focusToken={focusToken} onRename={name => actions.rename(item.id, name)} />
          <p className="inspector-sub">{item.format.toUpperCase()} · {item.width}×{item.height} · {formatBytes(item.bytes)}</p>

          {inTrash ? (
            <div className="inspector-actions">
              <button className="btn btn-primary grow" onClick={() => actions.restore(ids)}><RotateCcw size={14} />恢复</button>
              <button className="btn btn-danger-quiet" onClick={() => actions.deleteForever(ids)}>彻底删除</button>
            </div>
          ) : (
            <div className="inspector-actions">
              <button className="btn btn-primary grow" onClick={() => actions.copy(ids)} title="复制到剪贴板（⌘C），可直接粘贴到微信"><Copy size={14} />复制</button>
              <button className={`icon-btn${favorite ? ' on-fav' : ''}`} aria-pressed={favorite} title={favorite ? '取消收藏' : '收藏'} onClick={() => actions.favorite(ids, !favorite)}><Star size={15} fill={favorite ? 'currentColor' : 'none'} /></button>
              <button className="icon-btn" title="在访达中显示" onClick={() => actions.reveal(ids)}><FolderSearch size={15} /></button>
              <button className="icon-btn" title="更多" onClick={event => actions.more(ids, event.currentTarget.getBoundingClientRect())}><MoreHorizontal size={15} /></button>
            </div>
          )}

          {item.format === 'webp' || item.bytes > WECHAT_LIMIT ? (
            <p className={`wechat-note${item.bytes > WECHAT_LIMIT ? ' warn' : ''}`}>
              {item.bytes > WECHAT_LIMIT ? <><AlertTriangle size={13} />超过 1 MB，微信可能无法添加为表情</> : '复制或拖出时会自动转成微信可用的 GIF / PNG'}
            </p>
          ) : null}

          {inTrash ? null : (
            <section className="inspector-section">
              <h3>合集</h3>
              <CollectionChips ids={ids} names={collections} actions={actions} />
            </section>
          )}

          {group ? (
            <section className="inspector-section">
              <h3><Layers size={12} />{group.memberIds.length} 个版本</h3>
              <div className="versions">
                {group.memberIds.map(id => {
                  const member = index.byId.get(id);
                  return member ? (
                    <button key={id} className={`version${id === group.mainAssetId ? ' main' : ''}`} title={`${member.name}${id === group.mainAssetId ? '（主版本）' : ''}`} onClick={() => actions.openVersion(id)}>
                      <StickerPreview src={imageUrl(root, member.fileName)} name={member.name} />
                    </button>
                  ) : null;
                })}
              </div>
              <button className="link-btn" onClick={() => actions.disband(group.id)}>拆分为独立表情</button>
            </section>
          ) : null}

          <section className="inspector-section">
            <h3>信息</h3>
            <dl className="meta">
              <Meta label="来源">{item.sources.join('、') || '—'}</Meta>
              <Meta label="导入时间">{formatDateTime(item.importedAt)}</Meta>
              {trashedAt ? <Meta label="删除时间">{formatDateTime(trashedAt)}</Meta> : null}
              <Meta label="文件"><span className="mono" title={item.fileName}>{item.fileName}</span></Meta>
            </dl>
          </section>

          {inTrash ? null : (
            <section className="inspector-section quiet">
              <h3>标签</h3>
              <TagsField id={item.id} tags={ownTags} onSave={tags => actions.saveTags(item.id, tags)} />
            </section>
          )}
        </div>
      </aside>
    );
  }

  const bytes = items.reduce((sum, item) => sum + item.bytes, 0);
  const counts = new Map<string, number>();
  for (const item of items) for (const name of index.collectionsOf.get(item.id) ?? []) counts.set(name, (counts.get(name) ?? 0) + 1);
  const shared = [...counts.keys()].sort((a, b) => a.localeCompare(b, 'zh'));
  const allFavorite = items.every(item => index.favorites[item.id]);
  return (
    <aside className="inspector" aria-label="简介">
      <div className="inspector-body">
        <div className="inspector-hero stack">
          {items.slice(0, 3).reverse().map((item, layer, shown) => (
            <span key={item.id} className="stack-card" style={{ '--layer': shown.length - 1 - layer } as React.CSSProperties}>
              <StickerPreview src={imageUrl(root, item.fileName)} name={item.name} eager />
            </span>
          ))}
        </div>
        <h2 className="inspector-title">已选 {items.length} 项</h2>
        <p className="inspector-sub">共 {formatBytes(bytes)}</p>
        {inTrash ? (
          <div className="inspector-actions">
            <button className="btn btn-primary grow" onClick={() => actions.restore(ids)}><RotateCcw size={14} />全部恢复</button>
            <button className="btn btn-danger-quiet" onClick={() => actions.deleteForever(ids)}>彻底删除</button>
          </div>
        ) : <>
          <div className="inspector-actions">
            <button className="btn btn-primary grow" onClick={() => actions.copy(ids)}><Copy size={14} />复制 {items.length} 个</button>
            <button className={`icon-btn${allFavorite ? ' on-fav' : ''}`} title={allFavorite ? '全部取消收藏' : '全部收藏'} onClick={() => actions.favorite(ids, !allFavorite)}><Star size={15} fill={allFavorite ? 'currentColor' : 'none'} /></button>
            <button className="icon-btn" title="移入回收站" onClick={() => actions.trash(ids)}><Trash2 size={15} /></button>
            <button className="icon-btn" title="更多" onClick={event => actions.more(ids, event.currentTarget.getBoundingClientRect())}><MoreHorizontal size={15} /></button>
          </div>
          <section className="inspector-section">
            <h3>合集</h3>
            <CollectionChips ids={ids} names={shared} counts={counts} actions={actions} />
          </section>
          <section className="inspector-section">
            <button className="btn wide" onClick={() => actions.batchRename(ids)}>批量重命名…</button>
          </section>
        </>}
      </div>
    </aside>
  );
}
