import { Fragment, useEffect, useMemo, useRef, useState } from 'react';
import { Archive, ArchiveRestore, ArrowDownToLine, Check, CheckCircle2, ChevronLeft, ChevronRight, CircleAlert, Folder, FolderOpen, FolderPlus, GalleryHorizontal, Grid3X3, HardDriveDownload, Images, LayoutGrid, List, LoaderCircle, MessageCircle, Music2, ScanSearch, Search, Trash2, X } from 'lucide-react';
import { chooseLibrary, currentLibrary, imageUrl, importImages, isDesktop, getManagement, saveMetadata, setTrash, backupLibrary, restoreBackup, openLibraryPath, scanDuplicates, disbandGroup, ignorePair, exportAssets, saveGroupMetadata, batchRename, batchLabels, addCollection } from './api';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { SOURCE_LABELS, formatBytes, type ExportSummary, type ImportReport, type ScanReport, type Snapshot, type Sticker, type ManagementSnapshot, type AssetMetadata } from './types';
import { StickerDetail } from './components/StickerDetail';
import { StickerPreview } from './components/StickerPreview';
import { SimilarReview } from './components/SimilarReview';
import { BatchPanel } from './components/BatchPanel';
import { ContextMenu, type ContextMenuItem } from './components/ContextMenu';
import { CollectDialog } from './components/CollectDialog';
import { WeChatImportDialog } from './components/WeChatImportDialog';
import './styles.css';

const PAGE_SIZES = [50, 100, 200];
const PAGE_SIZE_KEY = 'stickernest.pageSize';
const VIEW_KEY = 'stickernest.view';
type ViewMode = 'large' | 'small' | 'list' | 'gallery';
const VIEW_MODES: { id: ViewMode; label: string; icon: typeof LayoutGrid }[] = [
  { id: 'large', label: '大图标', icon: LayoutGrid },
  { id: 'small', label: '小图标', icon: Grid3X3 },
  { id: 'list', label: '列表', icon: List },
  { id: 'gallery', label: '画廊', icon: GalleryHorizontal },
];
const navigation = [
  { id: 'all', label: '全部表情', icon: Images },
  { id: '微信', label: '微信', icon: MessageCircle },
  { id: '抖音', label: '抖音', icon: Music2 },
  { id: '本地', label: '本地文件', icon: Folder },
];

/** Unnamed hash-like file names shorten in the grid; full text stays in tooltip and detail. */
export function shortDisplayName(name: string): string {
  return /^[0-9a-f]{32,}\.[a-z0-9]+$/i.test(name) ? `${name.slice(0, 8)}…${name.slice(-7)}` : name;
}

function GroupMetaEditor({ group, disabled, onSave }: {
  group: { id: string; tags: string[]; collections: string[] };
  disabled: boolean;
  onSave: (groupId: string, tags: string[], collections: string[]) => Promise<void>;
}) {
  const [editing, setEditing] = useState(false);
  const [tags, setTags] = useState('');
  const [collections, setCollections] = useState('');
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');
  const savingRef = useRef(false);
  function startEditing() {
    setTags(group.tags.join('，')); setCollections(group.collections.join('，')); setError(''); setEditing(true);
  }
  async function save() {
    if (savingRef.current || disabled) return;
    savingRef.current = true; setSaving(true); setError('');
    const split = (value: string) => [...new Set(value.split(/[,，]/).map(value => value.trim()).filter(Boolean))];
    try { await onSave(group.id, split(tags), split(collections)); setEditing(false); }
    catch (reason) { setError(String(reason)); }
    finally { savingRef.current = false; setSaving(false); }
  }
  return <div className="group-meta">
    {editing ? <form className="group-meta-editor" onSubmit={event => { event.preventDefault(); void save(); }}>
      <label className="field-label">组标签<input value={tags} disabled={saving} onChange={event => setTags(event.target.value)} /></label>
      <label className="field-label">组合集<input value={collections} disabled={saving} onChange={event => setCollections(event.target.value)} /></label>
      {error ? <p className="field-error" role="alert">{error}</p> : null}
      <div className="editor-actions"><button className="button primary" type="submit" disabled={saving}>{saving ? '正在保存…' : '保存'}</button><button className="button secondary" type="button" disabled={saving} onClick={() => setEditing(false)}>取消</button></div>
    </form> : <div className="group-meta-row">
      <span className="group-meta-labels">组标签：{group.tags.join('、') || '未设置'} · 组合集：{group.collections.join('、') || '未设置'}</span>
      <button className="group-expand-btn" type="button" disabled={disabled} onClick={startEditing}>编辑</button>
    </div>}
  </div>;
}

export default function App() {
  const [library, setLibrary] = useState<Snapshot | null>(null);
  const [management, setManagement] = useState<ManagementSnapshot | null>(null);
  const [managementError, setManagementError] = useState('');
  const [tag, setTag] = useState('');
  const [collection, setCollection] = useState('');
  const [source, setSource] = useState('all');

  const [query, setQuery] = useState('');
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(() => {
    const saved = Number(localStorage.getItem(PAGE_SIZE_KEY));
    return PAGE_SIZES.includes(saved) ? saved : 50;
  });
  const [view, setView] = useState<ViewMode>(() => {
    const saved = localStorage.getItem(VIEW_KEY);
    return VIEW_MODES.some(mode => mode.id === saved) ? (saved as ViewMode) : 'large';
  });
  const [busy, setBusy] = useState(isDesktop ? '正在读取资料库' : '');
  const [error, setError] = useState('');
  const [previewWarning, setPreviewWarning] = useState('');
  const [report, setReport] = useState<ImportReport | null>(null);
  const [backupNotice, setBackupNotice] = useState<{ text: string; path: string } | null>(null);
  const [exportNotice, setExportNotice] = useState<ExportSummary | null>(null);
  const [scanReport, setScanReport] = useState<ScanReport | null>(null);
  const [selected, setSelected] = useState<Sticker | null>(null);
  const [selectedEdit, setSelectedEdit] = useState(false);
  const [checkedIds, setCheckedIds] = useState<Set<string>>(new Set());
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  const [showBatch, setShowBatch] = useState(false);
  const [showNewCollection, setShowNewCollection] = useState(false);
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; item: Sticker | null } | null>(null);
  const [showCollect, setShowCollect] = useState(false);
  const [showMenu, setShowMenu] = useState(false);
  const [showWechat, setShowWechat] = useState(false);
  const [focusId, setFocusId] = useState('');
  const [marqueeRect, setMarqueeRect] = useState<{ left: number; top: number; width: number; height: number } | null>(null);
  const locked = useRef(false);
  const searchRef = useRef<HTMLInputElement>(null);
  const anchorId = useRef('');
  const marqueeState = useRef<{ x0: number; y0: number; base: Set<string>; active: boolean } | null>(null);
  const showMenuRef = useRef(showMenu);
  showMenuRef.current = showMenu;
  const ctxMenuRef = useRef(ctxMenu);
  ctxMenuRef.current = ctxMenu;
  const viewRef = useRef(view);
  viewRef.current = view;
  const focusIdRef = useRef(focusId);
  focusIdRef.current = focusId;
  const visibleRef = useRef<Sticker[]>([]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        if (event.defaultPrevented || document.querySelector('dialog[open]')) return;
        if (showMenuRef.current) { setShowMenu(false); return; }
        if (ctxMenuRef.current) return; // 右键菜单自己的 Esc 处理（也会 preventDefault）
        setCheckedIds(current => current.size ? new Set() : current);
        return;
      }
      // 画廊模式：方向键移动聚焦的表情，缩略条随之滚动
      if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
        if (viewRef.current !== 'gallery' || document.querySelector('dialog[open]')) return;
        const target = event.target as HTMLElement | null;
        if (target && ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)) return;
        event.preventDefault();
        const list = visibleRef.current;
        if (!list.length) return;
        const index = Math.max(0, list.findIndex(item => item.id === focusIdRef.current));
        const next = list[Math.min(list.length - 1, Math.max(0, index + (event.key === 'ArrowRight' ? 1 : -1)))];
        setFocusId(next.id);
        return;
      }
      const mod = event.metaKey || event.ctrlKey;
      if (!mod) return;
      if (event.key.toLowerCase() === 'f') { event.preventDefault(); searchRef.current?.focus(); searchRef.current?.select(); }
      if (event.key.toLowerCase() === 'a' && inSelectable(sourceRef.current) && filteredRef.current.length) {
        event.preventDefault();
        setCheckedIds(new Set(filteredRef.current.map(item => item.id)));
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);
  const filteredRef = useRef<Sticker[]>([]);
  const sourceRef = useRef(source);
  sourceRef.current = source;
  function inSelectable(value: string) { return value !== 'trash' && value !== 'similar'; }

  // 点击导入拆分按钮以外的地方时收起下拉
  useEffect(() => {
    if (!showMenu) return;
    const onDown = (event: MouseEvent) => {
      if (!(event.target as HTMLElement).closest('.import-split')) setShowMenu(false);
    };
    document.addEventListener('mousedown', onDown, true);
    return () => document.removeEventListener('mousedown', onDown, true);
  }, [showMenu]);

  // One handler owns native dragging and double-click zoom, including brand children.
  useEffect(() => {
    if (!isDesktop) return;
    const onMouseDown = (event: MouseEvent) => {
      if (event.button !== 0 || !(event.target instanceof Element)) return;
      const target = event.target;
      if (!target.closest('[data-window-drag-region]')) return;
      if (target.closest('button, input, select, textarea, a, [role="checkbox"], [role="menu"], .library-menu')) return;
      event.preventDefault();
      const window = getCurrentWindow();
      const operation = event.detail === 2 ? window.toggleMaximize() : window.startDragging();
      void operation.catch(reason => setError(`窗口操作失败：${String(reason)}`));
    };
    window.addEventListener('mousedown', onMouseDown);
    return () => window.removeEventListener('mousedown', onMouseDown);
  }, []);

  // 画廊模式下让聚焦的缩略图始终滚进视野；只动缩略条自身，绝不滚动页面（否则导航按钮会跳动）
  const stripRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (view !== 'gallery' || !focusId) return;
    const strip = stripRef.current;
    const thumb = strip?.querySelector('.gallery-thumb-btn.focused') as HTMLElement | null;
    if (!strip || !thumb) return;
    const stripBox = strip.getBoundingClientRect();
    const thumbBox = thumb.getBoundingClientRect();
    strip.scrollTo({ left: strip.scrollLeft + (thumbBox.left - stripBox.left) - (strip.clientWidth - thumbBox.width) / 2, behavior: 'smooth' });
  }, [focusId, view]);

  useEffect(() => { localStorage.setItem(VIEW_KEY, view); }, [view]);
  useEffect(() => { localStorage.setItem(PAGE_SIZE_KEY, String(pageSize)); }, [pageSize]);

  useEffect(() => {
    if (!isDesktop) return;
    let active = true;
    currentLibrary().then(async snapshot => {
      if (!active) return;
      setLibrary(snapshot);
      if (snapshot) {
        try { const value = await getManagement(); if (active) setManagement(value); }
        catch (reason) { if (active) setManagementError(String(reason)); }
      }
    })
      .catch(reason => { if (active) setError(String(reason)); })
      .finally(() => { if (active) setBusy(''); });
    return () => { active = false; };
  }, []);

  async function run(label: string, action: () => Promise<void>) {
    if (locked.current || busy || !isDesktop) return;
    locked.current = true;
    setBusy(label);
    setError('');
    setPreviewWarning('');
    try { await action(); }
    catch (reason) { setError(String(reason)); }
    finally { locked.current = false; setBusy(''); }
  }

  async function applyLibrary(snapshot: Snapshot) {
    setLibrary(snapshot); setManagement(null); setManagementError(''); setTag(''); setCollection(''); setReport(null); setSelected(null); setQuery(''); setSource('all'); setPage(1); setBackupNotice(null); setExportNotice(null); setScanReport(null); setCheckedIds(new Set()); setExpandedGroups(new Set()); setShowBatch(false); setFocusId(''); setMarqueeRect(null);
    anchorId.current = '';
    try { setManagement(await getManagement()); } catch (reason) { setManagementError(String(reason)); }
  }

  function openLibrary() {
    void run('正在打开资料库', async () => {
      const snapshot = await chooseLibrary(false);
      if (snapshot) await applyLibrary(snapshot);
    });
  }

  function backupCurrentLibrary() {
    void run('正在备份资料库', async () => {
      const result = await backupLibrary();
      if (result) setBackupNotice({ text: `已备份 ${result.assetCount} 个素材（${formatBytes(result.totalBytes)}${result.includesManagement ? '，含整理记录' : ''}），哈希校验通过。`, path: result.path });
    });
  }

  function restoreFromBackup() {
    void run('正在校验并恢复备份', async () => {
      const path = await restoreBackup();
      if (path) {
        await applyLibrary(await openLibraryPath(path));
        setBackupNotice({ text: '已从备份恢复并在新位置打开，原备份保持不变。', path });
      }
    });
  }

  async function setItemsTrash(ids: string[], trashed: boolean) {
    // Like editMetadata, the modal blocks library switching while this save is pending.
    if (!library) throw new Error('请先打开资料库。');
    setManagement(await setTrash(library.root, ids, trashed));
    setSelected(null);
  }

  function runScan() {
    void run('正在扫描相似项', async () => {
      setScanReport(await scanDuplicates());
    });
  }

  /** 保留/忽略后就把涉及的条目从报告里剔除，避免全量重扫。 */
  function pruneReviewed(doneIds: string[]) {
    const done = new Set(doneIds);
    setScanReport(current => current ? {
      ...current,
      exactGroups: current.exactGroups.filter(group => !group.assetIds.some(id => done.has(id))),
      similarPairs: current.similarPairs.filter(pair => !done.has(pair.baseId) && !done.has(pair.otherId)),
      animationPairs: current.animationPairs.filter(pair => !done.has(pair.baseId) && !done.has(pair.otherId)),
    } : current);
  }

  async function keepOneAsset(_keepId: string, removedIds: string[]) {
    if (!library) throw new Error('请先打开资料库。');
    // Others move to the recycle bin, recoverable any time from the trash view.
    setManagement(await setTrash(library.root, removedIds, true));
    setCheckedIds(current => new Set([...current].filter(id => !removedIds.includes(id))));
    pruneReviewed(removedIds);
  }

  async function ignoreSimilarPair(a: string, b: string) {
    if (!library) throw new Error('请先打开资料库。');
    setManagement(await ignorePair(library.root, a, b));
    pruneReviewed([a, b]);
  }

  function exportSelection() {
    void run('正在导出素材', async () => {
      if (!library || !checkedIds.size) return;
      const ids = [...checkedIds];
      // nameMap must carry bare display names only; the manifest name retains
      // its original extension, which the Rust side strips when rebuilding.
      const nameMap = Object.fromEntries(displayItems.filter(item => ids.includes(item.id)).map(item => [item.id, management?.metadata[item.id]?.name || item.name.replace(/\.[a-z0-9]+$/i, '')]));
      const result = await exportAssets(library.root, ids, nameMap);
      if (result) { setExportNotice(result); setCheckedIds(new Set()); }
    });
  }

  function selectedInDisplayOrder(): string[] {
    // Numbering follows the visible grid order, not the click order.
    return displayItems.filter(item => checkedIds.has(item.id)).map(item => item.id);
  }

  async function batchRenameSelected(prefix: string, start: number) {
    if (!library) throw new Error('请先打开资料库。');
    await run('正在批量重命名', async () => {
      setManagement(await batchRename(library.root, selectedInDisplayOrder(), prefix, start));
      setShowBatch(false);
    });
  }

  async function batchLabelsSelected(addTags: string[], removeTags: string[], addCollections: string[], removeCollections: string[]) {
    if (!library) throw new Error('请先打开资料库。');
    await run('正在批量整理', async () => {
      setManagement(await batchLabels(library.root, selectedInDisplayOrder(), addTags, removeTags, addCollections, removeCollections));
    });
  }

  async function trashSelected() {
    if (!library) throw new Error('请先打开资料库。');
    await run('正在移入回收站', async () => {
      setManagement(await setTrash(library.root, [...checkedIds], true));
      setCheckedIds(new Set());
      setShowBatch(false);
    });
  }

  async function disband(groupId: string) {
    if (!library) return;
    await run('正在拆分分组', async () => {
      setManagement(await disbandGroup(library.root, groupId));
    });
  }

  async function saveGroupMeta(groupId: string, tags: string[], collections: string[]) {
    if (!library) throw new Error('请先打开资料库。');
    setManagement(await saveGroupMetadata(library.root, groupId, tags, collections));
  }

  function importFiles() {
    void run('正在导入，请稍候', async () => {
      const result = await importImages('本地');
      if (result) {
        setLibrary(result.snapshot); setReport(result.report); setPage(1);
        if (result.previewWarnings.length) setPreviewWarning(`导入结果已保存，部分素材无法预览：${result.previewWarnings.join('；')}`);
      }
    });
  }

  /** ids join a collection; `leave` removes the collection being viewed (a move). */
  function labelCollection(ids: string[], name: string, leave: string) {
    void run(leave ? '正在移动到合集' : '正在复制到合集', async () => {
      if (!library) return;
      setManagement(await batchLabels(library.root, ids, [], [], [name], leave ? [leave] : []));
    });
  }

  function leaveCollection(ids: string[], name: string) {
    void run('正在移出合集', async () => {
      if (!library) return;
      setManagement(await batchLabels(library.root, ids, [], [], [], [name]));
    });
  }

  async function createCollection(name: string) {
    if (!library) throw new Error('请先打开资料库。');
    const existing = new Set([...(management?.collections ?? []), ...collectionEntries.map(([entry]) => entry)]);
    if (existing.has(name)) throw new Error(`合集“${name}”已存在。`);
    await run('正在创建合集', async () => {
      if (!library) return;
      let next = await addCollection(library.root, name);
      if (checkedIds.size) {
        next = await batchLabels(library.root, selectedInDisplayOrder(), [], [], [name], []);
        setCheckedIds(new Set());
      }
      setManagement(next);
    });
    setCollection(name); setSource('all'); setPage(1); setShowNewCollection(false);
  }

  const trashSet = useMemo(() => new Set(Object.keys(management?.trash ?? {})), [management]);
  const groupByMain = useMemo(() => new Map((management?.groups ?? []).map(group => [group.mainAssetId, group])), [management]);
  const groupedNonMain = useMemo(() => new Set((management?.groups ?? []).flatMap(group => group.memberIds.filter(id => id !== group.mainAssetId))), [management]);
  const counts = useMemo(() => {
    const values: Record<string, number> = { all: 0, 本地: 0, 微信: 0, 抖音: 0, trash: 0 };
    for (const item of library?.items ?? []) {
      if (trashSet.has(item.id)) { values.trash += 1; continue; }
      if (groupedNonMain.has(item.id)) continue;
      values.all += 1;
      for (const origin of item.sources) values[origin] = (values[origin] ?? 0) + 1;
    }
    return values;
  }, [library, trashSet, groupedNonMain]);
  const displayItems = useMemo(() => (library?.items ?? []).map(item => ({ ...item, name: management?.metadata[item.id]?.name || item.name })), [library, management]);
  const tagFacets = useMemo(() => {
    const tags = new Set<string>();
    for (const value of Object.values(management?.metadata ?? {})) value.tags.forEach(value => tags.add(value));
    for (const group of management?.groups ?? []) group.tags.forEach(value => tags.add(value));
    return [...tags].sort();
  }, [management]);
  const collectionEntries = useMemo(() => {
    const map = new Map<string, number>();
    // Empty collections stay on the roster so they appear in the sidebar at 0.
    for (const name of management?.collections ?? []) map.set(name, 0);
    for (const item of library?.items ?? []) {
      if (trashSet.has(item.id) || groupedNonMain.has(item.id)) continue;
      for (const name of management?.metadata[item.id]?.collections ?? []) {
        map.set(name, (map.get(name) ?? 0) + 1);
      }
    }
    for (const group of management?.groups ?? []) {
      for (const name of group.collections) map.set(name, (map.get(name) ?? 0) + 1);
    }
    return [...map.entries()].sort((a, b) => a[0].localeCompare(b[0], 'zh'));
  }, [library, management, trashSet, groupedNonMain]);
  const effectiveLabelsOf = useMemo(() => {
    // Main versions answer for the group: member metadata plus group-level
    // tags and collections, as the design requires (displayed as a union).
    const map = new Map<string, { tags: string[]; collections: string[] }>();
    for (const item of displayItems) {
      const meta = management?.metadata[item.id];
      const group = groupByMain.get(item.id);
      map.set(item.id, {
        tags: [...new Set([...(meta?.tags ?? []), ...(group?.tags ?? [])])],
        collections: [...new Set([...(meta?.collections ?? []), ...(group?.collections ?? [])])],
      });
    }
    return map;
  }, [displayItems, management, groupByMain]);
  const filtered = useMemo(() => {
    const search = query.trim().toLocaleLowerCase();
    return displayItems.filter(item => {
      if (source === 'trash') {
        if (!trashSet.has(item.id)) return false;
      } else {
        if (trashSet.has(item.id) || groupedNonMain.has(item.id)) return false;
        if (source !== 'all' && !item.sources.includes(source)) return false;
      }
      const labels = effectiveLabelsOf.get(item.id) ?? { tags: [], collections: [] };
      return (!tag || labels.tags.includes(tag)) && (!collection || labels.collections.includes(collection))
        && (!search || [item.name, ...labels.tags].some(value => value.toLocaleLowerCase().includes(search)));
    });
  }, [displayItems, management, query, source, tag, collection, trashSet, groupedNonMain, effectiveLabelsOf]);
  async function editMetadata(assetId: string, metadata: AssetMetadata) {
    // The modal prevents switching libraries while its save is pending.
    if (!library) throw new Error('请先打开资料库。');
    setManagement(await saveMetadata(library.root, assetId, metadata));
  }
  filteredRef.current = filtered;
  const totalPages = Math.max(1, Math.ceil(filtered.length / pageSize));
  const visiblePage = Math.min(page, totalPages);
  const visible = filtered.slice((visiblePage - 1) * pageSize, visiblePage * pageSize);
  visibleRef.current = visible;
  const disabled = !!busy || !isDesktop;
  const inSelectableView = inSelectable(source);
  const inReviewView = source === 'similar';
  const heading = collection || (source === 'trash' ? '回收站' : source === 'similar' ? '相似项对比' : (navigation.find(item => item.id === source)?.label ?? '全部表情'));
  const headingCount = inReviewView ? null : collection ? filtered.length : (counts[source] ?? filtered.length);
  const isFiltering = !!(query || tag || collection) || (source !== 'all' && source !== 'trash');
  const inEmptyTrash = source === 'trash' && !isFiltering;
  const inEmptyCollection = !!collection && !query && !tag;
  const emptyTitle = inEmptyTrash ? '回收站是空的' : inEmptyCollection ? '这个合集还是空的' : isFiltering ? '没有匹配的表情' : '还没有表情';
  const emptyHint = inEmptyTrash ? '移入回收站后随时可恢复，不会真正删除。'
    : inEmptyCollection ? '勾选表情后，在右键菜单里选择「复制到合集」或「移动到合集」。'
    : isFiltering ? '试试其他关键词或筛选。'
    : '导入本地图片、GIF 或 WebP 开始整理你的表情画册。';

  const focusIndex = Math.max(0, visible.findIndex(item => item.id === focusId));
  const focusItem = visible[focusIndex] ?? visible[0];
  function moveFocus(step: number) {
    if (!visible.length) return;
    const next = visible[Math.min(visible.length - 1, Math.max(0, focusIndex + step))];
    setFocusId(next.id);
  }

  /** Finder-style click select: plain click toggles, Shift+click fills the range from the anchor. */
  function handleSelect(item: Sticker, event: { shiftKey: boolean }) {
    if (event.shiftKey && anchorId.current) {
      const ids = filtered.map(entry => entry.id);
      const from = ids.indexOf(anchorId.current);
      const to = ids.indexOf(item.id);
      if (from >= 0 && to >= 0) {
        const [lo, hi] = from < to ? [from, to] : [to, from];
        const range = ids.slice(lo, hi + 1);
        setCheckedIds(current => new Set([...current, ...range]));
        return;
      }
    }
    setCheckedIds(current => { const next = new Set(current); if (next.has(item.id)) next.delete(item.id); else next.add(item.id); return next; });
    anchorId.current = item.id;
  }

  /** Rubber-band selection started on empty grid space; Shift keeps the existing selection. */
  function startMarquee(event: React.MouseEvent) {
    if (!inSelectableView || event.button !== 0) return;
    const element = event.target as HTMLElement;
    if (element.closest('[data-marquee-id], button, input, select, dialog, .group-row, .group-tray, .gallery-strip, .pagination')) return;
    marqueeState.current = { x0: event.clientX, y0: event.clientY, base: event.shiftKey ? new Set(checkedIds) : new Set(), active: false };
    const onMove = (move: MouseEvent) => {
      const state = marqueeState.current;
      if (!state) return;
      const x1 = move.clientX;
      const y1 = move.clientY;
      if (!state.active && Math.abs(x1 - state.x0) < 5 && Math.abs(y1 - state.y0) < 5) return;
      state.active = true;
      const rect = { left: Math.min(state.x0, x1), top: Math.min(state.y0, y1), width: Math.abs(x1 - state.x0), height: Math.abs(y1 - state.y0) };
      setMarqueeRect(rect);
      const hits: string[] = [];
      for (const node of document.querySelectorAll<HTMLElement>('[data-marquee-id]')) {
        const box = node.getBoundingClientRect();
        if (box.left < rect.left + rect.width && box.right > rect.left && box.top < rect.top + rect.height && box.bottom > rect.top) {
          hits.push(node.dataset.marqueeId!);
        }
      }
      setCheckedIds(new Set([...state.base, ...hits]));
    };
    const onUp = () => {
      const state = marqueeState.current;
      marqueeState.current = null;
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
      setMarqueeRect(null);
      // A plain click on empty space (no drag) clears the selection, like Finder.
      if (state && !state.active) setCheckedIds(state.base);
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }

  function openCardMenu(event: React.MouseEvent, item: Sticker | null) {
    event.preventDefault();
    if (!item && checkedIds.size === 0) return;
    setCtxMenu({ x: event.clientX, y: event.clientY, item });
    if (item && !checkedIds.has(item.id)) setCheckedIds(current => new Set([...current, item.id]));
  }

  function contextMenuItems(): ContextMenuItem[] {
    const target = ctxMenu?.item ?? null;
    const targetId = target?.id ?? '';
    const ids = selectedInDisplayOrder();
    const multi = checkedIds.size > 1;
    const collectionNames = collectionEntries.map(([name]) => name);
    const items: ContextMenuItem[] = [];
    if (target) items.push({ key: 'detail', label: '查看详情', onSelect: () => { setSelected(library!.items.find(original => original.id === targetId) ?? target); setSelectedEdit(false); } });
    if (inSelectableView && ids.length) {
      const openEdit = () => { setSelected(library!.items.find(original => original.id === targetId) ?? target!); setSelectedEdit(true); };
      const copyChildren: ContextMenuItem[] = collectionNames.map(name => ({ key: `copy:${name}`, label: `复制到「${name}」`, onSelect: () => labelCollection(ids, name, '') }));
      copyChildren.push({ key: 'sepcopy', label: '' }, { key: 'copy-new', label: '新建合集并加入…', onSelect: () => setShowNewCollection(true) });
      items.push({ key: 'sep1', label: '' });
      if (multi) {
        // 多选：批量三件套放在同一段
        items.push(
          { key: 'rename', label: `批量重命名（${checkedIds.size} 项）`, onSelect: () => setShowBatch(true) },
          { key: 'tags', label: `批量设置标签（${checkedIds.size} 项）`, onSelect: () => setShowBatch(true) },
          { key: 'copy-col', label: '批量加入合集', children: copyChildren },
        );
      } else if (target) {
        // 单选：重命名、标签、合集、导出放在同一段
        items.push(
          { key: 'rename-one', label: '重命名…', onSelect: openEdit },
          { key: 'tags-one', label: '设置标签…', onSelect: openEdit },
          { key: 'copy-col', label: collection ? '复制到合集' : '加入合集', children: copyChildren },
        );
      }
      if (collection) {
        const moveChildren: ContextMenuItem[] = collectionNames.filter(name => name !== collection).map(name => ({ key: `move:${name}`, label: `移动到「${name}」`, onSelect: () => labelCollection(ids, name, collection) }));
        moveChildren.push({ key: 'sepmove', label: '' }, { key: 'move-new', label: '新建合集并移入…', onSelect: () => setShowNewCollection(true) });
        items.push({ key: 'move-col', label: `移动到合集（移出「${collection}」）`, children: moveChildren });
        items.push({ key: 'leave-col', label: `从「${collection}」移出（${ids.length} 项）`, onSelect: () => leaveCollection(ids, collection) });
      }
      items.push({ key: 'sep-col', label: '' });
      items.push({ key: 'export', label: multi ? `导出所选（${checkedIds.size} 项）` : '导出', onSelect: exportSelection });
      items.push({ key: 'sep2', label: '' });
    }
    if (multi || target) items.push({ key: 'trash', label: multi ? `移入回收站（${checkedIds.size} 项）` : '移入回收站', danger: true, onSelect: () => { const trashIds = multi ? ids : [targetId]; void setTrash(library!.root, trashIds, true).then(setManagement).catch(reason => setError(String(reason))); setCheckedIds(new Set()); } });
    return items;
  }

  /** DOM 勾选框：打勾是真实 SVG（不依赖 CSS data-uri，WebKit 稳出）。 */
  function selectBox(item: Sticker, isChecked: boolean) {
    if (!inSelectableView) return null;
    return <span className={`select-box${isChecked ? ' checked' : ''}`} role="checkbox" aria-checked={isChecked} aria-label={`选择 ${item.name}`} tabIndex={0}
      onClick={event => { event.stopPropagation(); handleSelect(item, event); }}
      onKeyDown={event => { if (event.key === ' ' || event.key === 'Enter') { event.preventDefault(); event.stopPropagation(); handleSelect(item, event); } }}>
      {isChecked ? <Check size={14} strokeWidth={4} /> : null}
    </span>;
  }

  function renderBadges(item: Sticker) {
    const group = groupByMain.get(item.id);
    if (!group) return null;
    return <span className="group-badge" title={`版本分组 ${group.memberIds.length} 项`}><Images size={11} />{group.memberIds.length}</span>;
  }

  function renderGridCell(item: Sticker) {
    const group = groupByMain.get(item.id);
    const isChecked = checkedIds.has(item.id);
    const memberItems = group ? group.memberIds.map(id => library!.items.find(original => original.id === id)).filter((value): value is Sticker => !!value) : [];
    return <Fragment key={item.id}>
      <div className="sticker-cell" data-marquee-id={item.id}>
        {selectBox(item, isChecked)}
        <button className={`sticker-card${isChecked ? ' checked' : ''}`} onClick={event => inSelectableView ? handleSelect(item, event) : setSelected(item)} onContextMenu={event => openCardMenu(event, item)} aria-label={`选择 ${item.name}`} aria-pressed={isChecked}>
          <div className="sticker-image"><StickerPreview key={`${library!.root}/${item.fileName}`} src={imageUrl(library!.root, item.fileName)} name={item.name} />{renderBadges(item)}</div>
          <div className="sticker-info"><strong title={item.name}>{shortDisplayName(item.name)}</strong><span>{item.sources.map(origin => SOURCE_LABELS[origin] ?? origin).join(' / ')}<span>{formatBytes(item.bytes)}</span></span></div>
        </button>
        {group ? <div className="group-tray"><button className="group-expand-btn" type="button" disabled={disabled} onClick={() => setExpandedGroups(current => { const next = new Set(current); if (next.has(group.id)) next.delete(group.id); else next.add(group.id); return next; })}>{expandedGroups.has(group.id) ? '收起版本' : `展开 ${group.memberIds.length} 个版本`}</button><button className="group-disband-btn" type="button" disabled={disabled} onClick={() => void disband(group.id)}>拆分分组</button>{group.tags.length || group.collections.length ? <span className="group-meta-inline" title={`组标签：${group.tags.join('、') || '无'} · 组合集：${group.collections.join('、') || '无'}`}>{group.tags.join('、')}{group.tags.length && group.collections.length ? ' · ' : ''}{group.collections.join('、')}</span> : null}</div> : null}
      </div>
      {group && expandedGroups.has(group.id) ? <div className="group-row"><div className="group-members">{memberItems.map(member => <button className="group-member" key={member.id} onClick={() => setSelected(member)} aria-label={`查看 ${member.name}`}><StickerPreview src={imageUrl(library!.root, member.fileName)} name={member.name} /><span className="pair-caption" title={member.name}>{shortDisplayName(member.name)}</span></button>)}</div><GroupMetaEditor group={group} disabled={disabled} onSave={saveGroupMeta} /></div> : null}
    </Fragment>;
  }

  function renderListRow(item: Sticker) {
    const group = groupByMain.get(item.id);
    const isChecked = checkedIds.has(item.id);
    return <div className="sticker-row" key={item.id} data-marquee-id={item.id}>
      {inSelectableView ? selectBox(item, isChecked) : <span className="row-box" />}
      <button className={`row-card${isChecked ? ' checked' : ''}`} onClick={event => inSelectableView ? handleSelect(item, event) : setSelected(item)} onContextMenu={event => openCardMenu(event, item)} aria-label={`选择 ${item.name}`} aria-pressed={isChecked}>
        <span className="row-thumb"><StickerPreview src={imageUrl(library!.root, item.fileName)} name={item.name} /></span>
        <span className="row-name"><strong title={item.name}>{shortDisplayName(item.name)}</strong><span>{item.sources.map(origin => SOURCE_LABELS[origin] ?? origin).join(' / ')}{group ? ` · 版本分组 ${group.memberIds.length} 项` : ''}</span></span>
        <span className="row-cell">{item.format.toUpperCase()}</span>
        <span className="row-cell">{item.width}×{item.height}</span>
        <span className="row-cell">{formatBytes(item.bytes)}</span>
        <span className="row-cell">{new Date(item.importedAt * 1000).toLocaleDateString('zh-CN')}</span>
      </button>
    </div>;
  }

  function renderGallery() {
    return <section className="gallery-view" aria-label="表情列表" onMouseDown={startMarquee}>
      {focusItem ? <div className="gallery-stage" tabIndex={0}>
        <button className="icon-button gallery-nav prev" aria-label="上一个" disabled={focusIndex <= 0} onClick={() => moveFocus(-1)}><ChevronLeft size={22} /></button>
        <StickerPreview src={imageUrl(library!.root, focusItem.fileName)} name={focusItem.name} eager />
        <button className="icon-button gallery-nav next" aria-label="下一个" disabled={focusIndex >= visible.length - 1} onClick={() => moveFocus(1)}><ChevronRight size={22} /></button>
        <div className="gallery-caption"><strong title={focusItem.name}>{focusItem.name}</strong><span>{focusItem.format.toUpperCase()} · {focusItem.width}×{focusItem.height} · {formatBytes(focusItem.bytes)} · 第 {focusIndex + 1} / {visible.length} 项</span></div>
      </div> : null}
      <div className="gallery-strip" role="listbox" aria-label="缩略图" ref={stripRef}>
        {visible.map(item => {
          const isChecked = checkedIds.has(item.id);
          return <div className="gallery-thumb" key={item.id} data-marquee-id={item.id}>
            {selectBox(item, isChecked)}
            <button className={`gallery-thumb-btn${item.id === focusItem?.id ? ' focused' : ''}${isChecked ? ' checked' : ''}`} role="option" aria-selected={item.id === focusItem?.id} title={item.name} onClick={event => { setFocusId(item.id); if (event.shiftKey) handleSelect(item, event); }} onContextMenu={event => { setFocusId(item.id); openCardMenu(event, item); }}>
              <StickerPreview src={imageUrl(library!.root, item.fileName)} name={item.name} />
            </button>
          </div>;
        })}
      </div>
    </section>;
  }

  return (
    <div className={`app-shell${marqueeRect ? ' marqueeing' : ''}`}>
      {busy ? <div className="busy-overlay" aria-hidden="true" /> : null}
      <aside className="sidebar" aria-label="资料库导航" data-window-drag-region>
        <div className="window-handle" data-window-drag-region aria-hidden="true" />
        <div className="brand" data-window-drag-region><img className="brand-icon" src="/icon.png" alt="" draggable={false} /><div><strong>拾趣</strong><span>StickerNest</span></div></div>
        <p className="nav-caption" data-window-drag-region>我的资料库</p>
        <nav>{navigation.map(({ id, label, icon: Icon }) => <button key={id} className={`nav-item ${source === id && !collection ? 'selected' : ''}`} aria-current={source === id && !collection ? 'page' : undefined} onClick={() => { setSource(id); setCollection(''); setPage(1); }}><Icon size={17} /><span>{label}</span><span className="count">{counts[id]}</span></button>)}</nav>
        <div className="nav-divider" role="separator" />
        <p className="nav-caption nav-caption-gap" data-window-drag-region>合集</p>
        <nav aria-label="我的合集">
          {collectionEntries.map(([name, count]) => <button key={name} className={`nav-item ${collection === name ? 'selected' : ''}`} aria-current={collection === name ? 'page' : undefined} onClick={() => { setCollection(current => current === name ? '' : name); setSource('all'); setPage(1); }}><FolderOpen size={15} /><span>{name}</span><span className="count">{count}</span></button>)}
          <button className="nav-item new-collection" disabled={!library || disabled} onClick={() => setShowNewCollection(true)}><FolderPlus size={15} /><span>新建合集</span></button>
        </nav>
        <div className="nav-divider" role="separator" />
        <p className="nav-caption nav-caption-gap" data-window-drag-region>整理</p>
        <nav>
          <button className={`nav-item ${source === 'similar' ? 'selected' : ''}`} aria-current={source === 'similar' ? 'page' : undefined} disabled={!library || !management} onClick={() => { setSource('similar'); setCollection(''); setPage(1); }}><ScanSearch size={16} /><span>相似项对比</span></button>
          <button className={`nav-item ${source === 'trash' ? 'selected' : ''}`} aria-current={source === 'trash' ? 'page' : undefined} onClick={() => { setSource('trash'); setCollection(''); setPage(1); }}><Trash2 size={16} /><span>回收站</span><span className="count">{counts.trash}</span></button>
        </nav>
        <div className="sidebar-bottom">
          <div className="settings-card" role="group" aria-label="库设置">
            <button className="settings-item" disabled={disabled} onClick={openLibrary}><FolderOpen size={16} /><span>打开资料库</span></button>
            <div className="settings-sep" role="separator" />
            <button className="settings-item" disabled={disabled || !library} onClick={backupCurrentLibrary}><HardDriveDownload size={16} /><span>备份这个库</span></button>
            <button className="settings-item" disabled={disabled} onClick={restoreFromBackup}><ArchiveRestore size={16} /><span>从备份恢复</span></button>
          </div>
        </div>
      </aside>

      <main className="workspace" aria-busy={!!busy}>
        <div className="page-top" data-window-drag-region>
        <header className="page-header" data-window-drag-region>
          <h1 data-window-drag-region>{heading}{headingCount !== null ? <span className="heading-count">{headingCount}</span> : null}</h1>
          <div className="page-actions">
            <div className="import-split">
              <button className="button primary import-main" disabled={disabled || !library} onClick={importFiles}><ArrowDownToLine size={16} />导入表情</button>
              <button className="button primary import-caret" aria-label="获取方式" aria-haspopup="menu" disabled={disabled} onClick={() => setShowMenu(open => !open)}><ChevronRight className="rotate-left" size={15} /></button>
              {showMenu ? <div className="library-menu import-menu" role="menu" aria-label="获取表情的方式">
                <button className="menu-item" onClick={() => { importFiles(); setShowMenu(false); }}><FolderOpen size={15} /><span className="menu-item-text"><strong>从本地图片导入</strong><span className="menu-item-hint">选择 PNG / GIF / WebP 文件</span></span></button>
                <button className="menu-item" disabled={disabled || !library} onClick={() => { setShowWechat(true); setShowMenu(false); }}><MessageCircle size={15} /><span className="menu-item-text"><strong>从微信获取</strong><span className="menu-item-hint">本机直接获取或清单导入</span></span></button>
                <button className="menu-item" disabled={disabled || !library} onClick={() => { setShowCollect(true); setShowMenu(false); }}><Music2 size={15} /><span className="menu-item-text"><strong>采集抖音收藏</strong><span className="menu-item-hint">从网页登录的抖音面板</span></span></button>
              </div> : null}
            </div>
          </div>
        </header>
        {inReviewView ? null : <div className="toolbar" data-window-drag-region>
          <label className="search-field"><Search size={18} /><input ref={searchRef} type="search" placeholder="搜索名称或标签…" aria-label="搜索名称或标签" value={query} onChange={event => { setQuery(event.target.value); setPage(1); }} disabled={!library} /></label>
          <label className="source-select compact"><span>标签</span><select aria-label="按标签筛选" value={tag} onChange={event => { setTag(event.target.value); setPage(1); }} disabled={disabled || !library}><option value="">全部标签</option>{tagFacets.map(value => <option key={value}>{value}</option>)}</select></label>
          {library ? <div className="toolbar-tail">
            <span className="filtered-count">{query || tag || collection ? `筛选出 ${filtered.length} 项` : `共 ${filtered.length} 项`}</span>
            <div className="view-switch" role="group" aria-label="显示模式">{VIEW_MODES.map(({ id, label, icon: Icon }) => <button key={id} type="button" className={view === id ? 'active' : ''} aria-pressed={view === id} title={label} onClick={() => setView(id)}><Icon size={15} /></button>)}</div>
          </div> : null}
        </div>}
        </div>
        {backupNotice ? <div className="notice success" role="status"><CheckCircle2 size={18} /><div><strong>{backupNotice.text}</strong><p className="notice-path" title={backupNotice.path}>{backupNotice.path}</p></div><button className="icon-button" aria-label="关闭备份结果" onClick={() => setBackupNotice(null)}><X size={17} /></button></div> : null}
        {exportNotice ? <div className="notice success" role="status"><CheckCircle2 size={18} /><div><strong>已导出 {exportNotice.exported} 个素材{exportNotice.skipped ? `，跳过 ${exportNotice.skipped} 个` : ''}（原格式原字节）</strong><p className="notice-path" title={exportNotice.path}>{exportNotice.path}</p></div><button className="icon-button" aria-label="关闭导出结果" onClick={() => setExportNotice(null)}><X size={17} /></button></div> : null}
        {managementError ? <div className="notice error" role="alert"><div><strong>整理记录未能读取</strong><p>{managementError}</p><p>编辑已停用，原记录保留；修复整理文件后重开资料库。</p></div></div> : null}
        {!isDesktop ? <div className="notice"><CircleAlert size={18} /><p>请使用桌面应用打开本地资料库。</p></div> : null}
        {busy ? <div className="notice" role="status"><LoaderCircle className="spin" size={18} /><p>{busy}…</p></div> : null}
        {error ? <div className="notice error" role="alert"><CircleAlert size={18} /><div><strong>操作未完成</strong><p>{error}</p><p>检查文件或权限后重试。</p></div><button className="icon-button" aria-label="关闭错误提示" onClick={() => setError('')}><X size={17} /></button></div> : null}
        {previewWarning ? <div className="notice warning" role="status"><CircleAlert size={18} /><div><strong>预览提示</strong><p>{previewWarning}</p><p>请检查原文件是否仍在。</p></div><button className="icon-button" aria-label="关闭预览提示" onClick={() => setPreviewWarning('')}><X size={17} /></button></div> : null}
        {report ? <div className={`notice import-report ${report.failed.length ? 'warning' : 'success'}`} role="status"><CheckCircle2 size={18} /><div><strong>导入完成</strong><p>新增 {report.added} 个 · 重复 {report.duplicates} 个 · 失败 {report.failed.length} 个</p>{report.duplicates > 0 && trashSet.size > 0 ? <p>重复项未显示时，可能在回收站中。</p> : null}{report.failed.length ? <details><summary>查看失败文件</summary><ul>{report.failed.map((failure, index) => <li key={`${index}-${failure.name}`}><b>{failure.name}</b>：{failure.error}</li>)}</ul><p>修复后重新导入，已入库项会自动去重。</p></details> : null}</div><button className="icon-button" aria-label="关闭导入结果" onClick={() => setReport(null)}><X size={17} /></button></div> : null}

        {!library ? <section className="empty-state welcome"><span className="empty-icon"><Archive size={38} strokeWidth={1.4} /></span><h2>打开你的表情库</h2><p>把散落在各处的表情收进一个安静的画册。打开本地的 StickerNest Library 文件夹开始。</p><div className="empty-actions"><button className="button primary" disabled={disabled} onClick={openLibrary}><FolderOpen size={18} />打开资料库</button><button className="button secondary" disabled={disabled} onClick={restoreFromBackup}><ArchiveRestore size={18} />从备份恢复</button></div></section> : inReviewView ? <>
        {management ? <SimilarReview report={scanReport} scanning={busy === '正在扫描相似项'} items={library.items} root={library.root} existingGroups={management.groups} onKeep={keepOneAsset} onIgnorePair={ignoreSimilarPair} onRescan={runScan} /> : null}
        </> : visible.length === 0 ? <section className="empty-state"><span className="empty-icon">{inEmptyTrash ? <Trash2 size={36} strokeWidth={1.4} /> : <Images size={36} strokeWidth={1.4} />}</span><h2>{emptyTitle}</h2><p>{emptyHint}</p>{isFiltering ? <button className="button secondary" onClick={() => { setQuery(''); setTag(''); setCollection(''); setSource('all'); setPage(1); }}>查看全部表情</button> : source === 'trash' ? null : <button className="button primary" disabled={disabled} onClick={importFiles}><ArrowDownToLine size={18} />导入表情</button>}</section> : <>{showBatch && checkedIds.size ? <BatchPanel count={checkedIds.size} disabled={disabled} onRename={batchRenameSelected} onLabels={batchLabelsSelected} onTrash={trashSelected} onClose={() => setShowBatch(false)} /> : null}
        {view === 'list' ? <section className="sticker-rows" aria-label="表情列表" onMouseDown={startMarquee}><div className="sticker-row head"><span className="row-box" /><div className="row-card"><span className="row-thumb" /><span className="row-name">名称</span><span className="row-cell">格式</span><span className="row-cell">尺寸</span><span className="row-cell">大小</span><span className="row-cell">导入时间</span></div></div>{visible.map(renderListRow)}</section> : view === 'gallery' ? renderGallery() : <section className={`sticker-grid${view === 'small' ? ' compact' : ''}`} aria-label="表情列表" onMouseDown={startMarquee}>{visible.map(renderGridCell)}</section>}
        <div className="pagination"><label className="page-size">每页<select aria-label="每页显示数量" value={pageSize} onChange={event => { setPageSize(Number(event.target.value)); setPage(1); }}>{PAGE_SIZES.map(size => <option key={size} value={size}>{size}</option>)}</select></label><span>第 {visiblePage} / {totalPages} 页</span><button className="icon-button" aria-label="上一页" disabled={visiblePage === 1} onClick={() => setPage(visiblePage - 1)}><ChevronLeft size={20} /></button><button className="icon-button" aria-label="下一页" disabled={visiblePage === totalPages} onClick={() => setPage(visiblePage + 1)}><ChevronRight size={20} /></button></div></>}
        {checkedIds.size > 0 && inSelectableView ? <div className="batch-dock" role="region" aria-label="批量操作">
          <span className="batch-dock-count">已选 <b>{checkedIds.size}</b> 项</span>
          <button className="button danger" disabled={disabled || !library} onClick={() => void trashSelected()}><Trash2 size={15} />移入回收站</button>
          <button className="text-button" disabled={disabled} onClick={() => setCheckedIds(new Set())}>清除选择</button>
        </div> : null}
        <footer className="library-footer"><Folder size={14} /><span title={library?.root}>{library?.root ?? '尚未选择资料库'}</span><span className="footer-count">{library ? `${library.items.length} 个原始素材` : '本地资料库'}</span></footer>
      </main>
      {marqueeRect ? <div className="marquee-rect" style={marqueeRect} aria-hidden="true" /> : null}
      {selected && library ? <StickerDetail key={selected.id} item={selected} root={library.root} metadata={management?.metadata[selected.id]} editable={!!management && !disabled} trashed={!!management?.trash[selected.id]} trashedAt={management?.trash[selected.id]} edit={selectedEdit} onSave={metadata => editMetadata(selected.id, metadata)} onSetTrash={value => setItemsTrash([selected.id], value)} onClose={() => { setSelected(null); setSelectedEdit(false); }} /> : null}
      {showCollect && library && management ? <CollectDialog root={library.root} accounts={management.accounts.filter(a => a.platform === '抖音')} onSnapshot={(snapshot, warning) => { setLibrary(snapshot); setPage(1); if (warning) setPreviewWarning(warning); }} onManagement={setManagement} onError={setError} onClose={() => setShowCollect(false)} /> : null}
      {showWechat && library && management ? <WeChatImportDialog root={library.root} accounts={management.accounts.filter(a => a.platform === '微信')} onSnapshot={(snapshot, warning) => { setLibrary(snapshot); setPage(1); if (warning) setPreviewWarning(warning); }} onManagement={setManagement} onError={setError} onClose={() => setShowWechat(false)} /> : null}
      {ctxMenu && library && management ? <ContextMenu x={ctxMenu.x} y={ctxMenu.y} onClose={() => setCtxMenu(null)} items={contextMenuItems()} /> : null}
      {showNewCollection && library && management ? <NewCollectionDialog checkedCount={checkedIds.size} existingNames={collectionEntries.map(([name]) => name)} disabled={disabled} onCreate={createCollection} onClose={() => setShowNewCollection(false)} /> : null}
    </div>
  );
}

function NewCollectionDialog({ checkedCount, existingNames, disabled, onCreate, onClose }: {
  checkedCount: number;
  existingNames: string[];
  disabled: boolean;
  onCreate: (name: string) => Promise<void>;
  onClose: () => void;
}) {
  const [name, setName] = useState('');
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');
  const dialog = useRef<HTMLDialogElement>(null);
  const savingRef = useRef(false);
  useEffect(() => {
    const trigger = document.activeElement as HTMLElement | null;
    dialog.current?.showModal();
    return () => { dialog.current?.close(); trigger?.focus(); };
  }, []);
  async function save() {
    if (savingRef.current) return;
    const trimmed = name.trim();
    if (!trimmed) { setError('请填写合集名称。'); return; }
    if (trimmed.length > 50) { setError('合集名称最多 50 字。'); return; }
    if (existingNames.includes(trimmed)) { setError(`合集“${trimmed}”已存在。`); return; }
    savingRef.current = true; setSaving(true); setError('');
    try { await onCreate(trimmed); }
    catch (reason) { setError(String(reason)); savingRef.current = false; setSaving(false); }
  }
  return (
    <dialog className="detail-dialog collect-dialog" ref={dialog} onClose={() => { if (!dialog.current?.open) onClose(); }} aria-labelledby="new-collection-title" onCancel={event => { if (savingRef.current) event.preventDefault(); }}>
      <div className="detail-heading"><span className="eyebrow">新建合集</span><button className="icon-button" autoFocus disabled={saving} onClick={() => dialog.current?.close()} aria-label="关闭"><X size={18} /></button></div>
      <div className="detail-content collect-content">
        <h2 id="new-collection-title">新建合集</h2>
        <p className="field-hint">{checkedCount ? `当前已勾选 ${checkedCount} 项表情，创建后会一并放入合集。` : '可以直接创建空合集，之后勾选表情，在右键菜单里选择「复制到合集」或「移动到合集」。'}</p>
        <form className="metadata-editor" onSubmit={event => { event.preventDefault(); void save(); }}>
          <label className="field-label">合集名称<input value={name} maxLength={50} autoFocus={false} disabled={saving || disabled} onChange={event => setName(event.target.value)} /></label>
          {error ? <p className="field-error" role="alert">{error}</p> : null}
          <div className="editor-actions"><button className="button primary" type="submit" disabled={saving || disabled || !name.trim()}>{saving ? '正在创建…' : checkedCount ? '创建并放入所选' : '创建空合集'}</button><button className="button secondary" type="button" disabled={saving} onClick={() => dialog.current?.close()}>取消</button></div>
        </form>
      </div>
    </dialog>
  );
}
