import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { Clock, FolderOpen, Images, Inbox, Library, MessageCircle, Music2, SearchX, Star, Trash2 } from 'lucide-react';
import * as api from './api';
import { ContextMenu, type ContextMenuItem } from './components/ContextMenu';
import { CharacterReview } from './components/CharacterReview';
import { CollectDialog } from './components/CollectDialog';
import { SettingsDialog } from './components/SettingsDialog';
import { EmptyState, Welcome } from './components/EmptyState';
import { Inspector, type InspectorActions } from './components/Inspector';
import { BatchRenameDialog, ConfirmDialog, PromptDialog } from './components/Modal';
import { QuickLook } from './components/QuickLook';
import { SelectionBar } from './components/SelectionBar';
import { Sidebar } from './components/Sidebar';
import { SimilarReview } from './components/SimilarReview';
import { StickerGrid } from './components/StickerGrid';
import { Toasts } from './components/Toasts';
import { Toolbar } from './components/Toolbar';
import { WeChatImportDialog } from './components/WeChatImportDialog';
import { errorText, useLibrary } from './hooks/useLibrary';
import { usePrefs } from './hooks/usePrefs';
import { useToasts } from './hooks/useToasts';
import { validateCollectionName } from './lib/collections';
import { baseName, buildIndex, filterItems, sameNav, SORT_LABELS, type Nav, type SortKey } from './lib/derive';
import { pruneReviewed, withoutPair } from './lib/scan';
import { emptySelection, inOrder, moveSelect, shouldHandleGlobalKey, type Selection } from './lib/selection';
import type { ScanReport } from './types';

type Dialog =
  | { kind: 'new-collection'; ids: string[] }
  | { kind: 'rename-collection'; name: string }
  | { kind: 'delete-collection'; name: string }
  | { kind: 'delete-forever'; ids: string[] | 'all' }
  | { kind: 'batch-rename'; ids: string[] }
  | { kind: 'collect-douyin' }
  | { kind: 'import-wechat' }
  | { kind: 'settings' };

interface Menu { x: number; y: number; items: ContextMenuItem[]; above?: boolean }


function navTitle(nav: Nav): string {
  switch (nav.kind) {
    case 'all': return '全部表情';
    case 'favorites': return '收藏';
    case 'recent': return '最近使用';
    case 'source': return nav.value === '本地' ? '本地导入' : nav.value;
    case 'collection': return nav.value;
    case 'similar': return '相似项';
    case 'characters': return '形象识别';
    case 'trash': return '回收站';
  }
}

function libraryName(root: string): string {
  return root.split('/').filter(Boolean).pop() ?? root;
}

/** Parent folder with the home directory shortened, e.g. "~/Pictures". */
function libraryPlace(root: string): string {
  return root.replace(/\/[^/]+\/?$/, '').replace(/^\/Users\/[^/]+/, '~') || '/';
}

/** Menus that open from a button sit below it; ones near the bottom open upward (their bottom edge at `y`). */
function menuAt(anchor: DOMRect, items: ContextMenuItem[], above = false): Menu {
  return { x: anchor.left, y: above ? anchor.top - 4 : anchor.bottom + 4, items, above };
}

export default function App() {
  const toast = useToasts();
  const lib = useLibrary(toast);
  const [prefs, setPrefs] = usePrefs();
  const [nav, setNav] = useState<Nav>({ kind: 'all' });
  const [query, setQuery] = useState('');
  const [selection, setSelection] = useState<Selection>(emptySelection);
  const [columns, setColumns] = useState(1);
  const [menu, setMenu] = useState<Menu | null>(null);
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const [quickLook, setQuickLook] = useState('');
  const [scanReport, setScanReport] = useState<ScanReport | null>(null);
  const [scanning, setScanning] = useState(false);
  const [dropCount, setDropCount] = useState(0);
  const [dropTarget, setDropTarget] = useState('');
  const [focusToken, setFocusToken] = useState(0);
  const searchRef = useRef<HTMLInputElement>(null);
  /** Ids being dragged out of the grid; drops of our own drag never import. */
  const ownDrag = useRef<string[] | null>(null);

  const { library, management } = lib;
  const root = library?.root ?? '';
  const index = useMemo(() => buildIndex(library, management), [library, management]);
  const items = useMemo(() => filterItems(index, nav, query, prefs.sort), [index, nav, query, prefs.sort]);
  const order = useMemo(() => items.map(item => item.id), [items]);
  const selectedIds = useMemo(() => inOrder(selection.ids, order), [selection, order]);
  const inTrash = nav.kind === 'trash';

  // Items that leave the view (trashed, filtered out, deleted) leave the selection too.
  useEffect(() => {
    setSelection(current => {
      if (![...current.ids].some(id => !order.includes(id))) return current;
      const ids = new Set([...current.ids].filter(id => order.includes(id)));
      return { ids, anchor: ids.has(current.anchor) ? current.anchor : '' };
    });
  }, [order]);

  const navigate = useCallback((next: Nav) => {
    setNav(next);
    setSelection(emptySelection());
    setQuickLook('');
  }, []);

  // ---- Actions (all take explicit ids so menus, bar, inspector and keys share them) ----

  const nameMap = (ids: string[]) => Object.fromEntries(ids.map(id => [id, baseName(index.byId.get(id)?.name ?? id)]));

  const act = {
    copy: (ids: string[]) => { if (ids.length) void lib.copy(ids); },
    favorite: (ids: string[], on: boolean) => { if (ids.length) void lib.setFavorite(ids, on); },
    reveal: (ids: string[]) => lib.reveal(ids),
    exportIds: (ids: string[]) => { if (ids.length) void lib.exportIds(ids, nameMap(ids)); },
    trash: (ids: string[]) => { if (ids.length) void lib.trash(ids); },
    restore: (ids: string[]) => { if (ids.length) void lib.restore(ids); },
    deleteForever: (ids: string[] | 'all') => setDialog({ kind: 'delete-forever', ids }),
    addTo: (ids: string[], name: string) => { if (ids.length) void lib.addToCollection(ids, name); },
    leave: (ids: string[], name: string) => {
      const groupIds = ids.map(id => index.groupByMain.get(id)).filter(group => group?.collections.includes(name)).map(group => group!.id);
      void lib.removeFromCollection(ids, name, groupIds);
    },
    newCollection: (ids: string[]) => setDialog({ kind: 'new-collection', ids }),
    quickLook: (id: string) => setQuickLook(id),
    rename: (ids: string[]) => {
      if (ids.length > 1) { setDialog({ kind: 'batch-rename', ids }); return; }
      setPrefs({ inspector: true });
      setFocusToken(token => token + 1);
    },
  };

  /** Saves through the write lock; failures toast and rethrow so inline editors can roll back. */
  async function saveMeta(id: string, patch: { name?: string; tags?: string[] }) {
    const current = management?.metadata[id] ?? { name: '', tags: [], collections: [] };
    try {
      lib.setManagement(await lib.write(rootPath => api.saveMetadata(rootPath, id, { ...current, ...patch })));
    } catch (reason) {
      toast.push({ kind: 'error', title: '保存失败', detail: errorText(reason) });
      throw reason;
    }
  }

  function collectionItems(ids: string[]): ContextMenuItem[] {
    const current = nav.kind === 'collection' ? nav.value : '';
    return [
      ...index.collections.filter(([name]) => name !== current).map(([name]) => ({ key: `c:${name}`, label: name, onSelect: () => act.addTo(ids, name) })),
      ...(index.collections.length > (current ? 1 : 0) ? [{ key: 'sep-new', label: '' }] : []),
      { key: 'new', label: '新建合集…', onSelect: () => act.newCollection(ids) },
    ];
  }

  function itemMenu(ids: string[]): ContextMenuItem[] {
    if (inTrash) {
      return [
        { key: 'restore', label: ids.length > 1 ? `恢复 ${ids.length} 项` : '恢复', onSelect: () => act.restore(ids) },
        { key: 'look', label: '快速查看', hint: '空格', disabled: ids.length !== 1, onSelect: () => act.quickLook(ids[0]) },
        { key: 'sep1', label: '' },
        { key: 'delete', label: '彻底删除…', danger: true, onSelect: () => act.deleteForever(ids) },
      ];
    }
    const allFavorite = ids.every(id => index.favorites[id]);
    const group = ids.length === 1 ? index.groupByMain.get(ids[0]) : undefined;
    return [
      { key: 'copy', label: ids.length > 1 ? `复制 ${ids.length} 个表情` : '复制', hint: '⌘C', onSelect: () => act.copy(ids) },
      { key: 'look', label: '快速查看', hint: '空格', disabled: ids.length !== 1, onSelect: () => act.quickLook(ids[0]) },
      { key: 'sep1', label: '' },
      { key: 'fav', label: allFavorite ? '取消收藏' : '收藏', onSelect: () => act.favorite(ids, !allFavorite) },
      { key: 'collect', label: '加入合集', children: collectionItems(ids) },
      ...(nav.kind === 'collection' ? [{ key: 'leave', label: `从「${nav.value}」移出`, onSelect: () => act.leave(ids, nav.value) }] : []),
      { key: 'sep2', label: '' },
      { key: 'rename', label: ids.length > 1 ? '批量重命名…' : '重命名', hint: ids.length > 1 ? undefined : '↩', onSelect: () => act.rename(ids) },
      { key: 'reveal', label: '在访达中显示', onSelect: () => act.reveal(ids) },
      { key: 'export', label: '导出原图…', onSelect: () => act.exportIds(ids) },
      ...(group ? [{ key: 'disband', label: '拆分版本分组', onSelect: () => void lib.disband(group.id) }] : []),
      { key: 'sep3', label: '' },
      { key: 'trash', label: '移入回收站', hint: '⌘⌫', danger: true, onSelect: () => act.trash(ids) },
    ];
  }

  const importItems: ContextMenuItem[] = [
    { key: 'files', label: '本地图片…', onSelect: () => void lib.importPick(false) },
    { key: 'folder', label: '文件夹…', onSelect: () => void lib.importPick(true) },
    { key: 'sep', label: '' },
    { key: 'wechat', label: '从微信导入…', onSelect: () => setDialog({ kind: 'import-wechat' }) },
    { key: 'douyin', label: '从抖音导入…', onSelect: () => setDialog({ kind: 'collect-douyin' }) },
  ];

  function openGridMenu(id: string | null, event: React.MouseEvent) {
    if (!id) {
      if (inTrash) return;
      setMenu({ x: event.clientX, y: event.clientY, items: [
        ...importItems,
        { key: 'sep-all', label: '' },
        { key: 'all', label: '全选', hint: '⌘A', disabled: !order.length, onSelect: () => setSelection({ ids: new Set(order), anchor: order[order.length - 1] ?? '' }) },
      ] });
      return;
    }
    // Right-clicking outside the selection retargets it, as in Finder.
    let ids = selectedIds;
    if (!selection.ids.has(id)) { ids = [id]; setSelection({ ids: new Set([id]), anchor: id }); }
    setMenu({ x: event.clientX, y: event.clientY, items: itemMenu(ids) });
  }

  function openCollectionMenu(name: string, event: React.MouseEvent) {
    event.preventDefault();
    setMenu({ x: event.clientX, y: event.clientY, items: [
      { key: 'rename', label: '重命名…', onSelect: () => setDialog({ kind: 'rename-collection', name }) },
      { key: 'sep', label: '' },
      { key: 'delete', label: '删除合集…', danger: true, onSelect: () => setDialog({ kind: 'delete-collection', name }) },
    ] });
  }

  async function openLibraryMenu(anchor: DOMRect) {
    const recent = (await api.recentLibraries().catch(() => [] as string[])).filter(path => path !== lib.library?.root);
    const items: ContextMenuItem[] = [
      { key: 'reveal', label: '在访达中显示资料库', onSelect: () => lib.reveal([]) },
      { key: 'sep1', label: '' },
      ...recent.map(path => ({ key: `recent:${path}`, label: `${libraryName(path)} · ${libraryPlace(path)}`,
        onSelect: () => void lib.openRecent(path).then(ok => ok && navigate({ kind: 'all' })) })),
      ...(recent.length ? [{ key: 'sep0', label: '' }] : []),
      { key: 'open', label: '打开其他资料库…', onSelect: () => void lib.openLibrary(false).then(ok => ok && navigate({ kind: 'all' })) },
      { key: 'create', label: '新建资料库…', onSelect: () => void lib.openLibrary(true).then(ok => ok && navigate({ kind: 'all' })) },
      { key: 'sep2', label: '' },
      { key: 'backup', label: '备份资料库…', onSelect: () => void lib.backup() },
      { key: 'restore', label: '从备份恢复…', onSelect: () => void lib.restoreBackup().then(ok => ok && navigate({ kind: 'all' })) },
      { key: 'sep3', label: '' },
      { key: 'settings', label: '设置…', onSelect: () => setDialog({ kind: 'settings' }) },
    ];
    setMenu(menuAt(anchor, items, true));
  }

  function openSortMenu(anchor: DOMRect) {
    setMenu(menuAt(anchor, (Object.keys(SORT_LABELS) as SortKey[]).map(key => ({
      key, label: SORT_LABELS[key], checked: prefs.sort === key, onSelect: () => setPrefs({ sort: key }),
    }))));
  }

  // ---- Similar review ----

  async function runScan() {
    setScanning(true);
    await lib.run('正在扫描相似项', async () => { setScanReport(await api.scanDuplicates()); }, '扫描未完成');
    setScanning(false);
  }

  // ---- Drag out, drop in ----

  async function dragOut(ids: string[]) {
    if (!root || inTrash) return;
    ownDrag.current = ids;
    try {
      const dropped = await api.dragOut(root, ids);
      // A drop on our own sidebar is an internal move, not a use.
      if (dropped && ownDrag.current) void api.markUsed(root, ids).then(lib.setManagement).catch(() => undefined);
    } catch (reason) {
      toast.push({ kind: 'error', title: '无法拖出', detail: errorText(reason) });
    } finally {
      // The webview's own drop event can trail the drag callback; give it a beat.
      window.setTimeout(() => { ownDrag.current = null; setDropTarget(''); }, 250);
    }
  }

  const dropHandlers = useRef({ importPaths: lib.importPaths, addTo: act.addTo, hasLibrary: !!library });
  dropHandlers.current = { importPaths: lib.importPaths, addTo: act.addTo, hasLibrary: !!library };

  useEffect(() => {
    if (!api.isDesktop) return;
    const collectionAt = (position: { x: number; y: number }) => {
      const scale = window.devicePixelRatio || 1;
      const element = document.elementFromPoint(position.x / scale, position.y / scale);
      return element?.closest<HTMLElement>('[data-collection-drop]')?.dataset.collectionDrop ?? '';
    };
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void getCurrentWebview().onDragDropEvent(event => {
      const payload = event.payload;
      const dragged = ownDrag.current;
      if (dragged) {
        if (payload.type === 'enter' || payload.type === 'over') setDropTarget(collectionAt(payload.position));
        else if (payload.type === 'drop') {
          const name = collectionAt(payload.position);
          ownDrag.current = null;
          if (name) dropHandlers.current.addTo(dragged, name);
          setDropTarget('');
        } else setDropTarget('');
        return;
      }
      if (!dropHandlers.current.hasLibrary) return;
      if (payload.type === 'enter') setDropCount(payload.paths.length);
      else if (payload.type === 'drop') { setDropCount(0); if (payload.paths.length) void dropHandlers.current.importPaths(payload.paths); }
      else if (payload.type === 'leave') setDropCount(0);
    }).then(fn => { if (disposed) fn(); else unlisten = fn; });
    return () => { disposed = true; unlisten?.(); };
  }, []);

  // One handler owns native window dragging and double-click zoom.
  useEffect(() => {
    if (!api.isDesktop) return;
    const onMouseDown = (event: MouseEvent) => {
      if (event.button !== 0 || !(event.target instanceof Element)) return;
      const target = event.target;
      if (!target.closest('[data-window-drag-region]')) return;
      if (target.closest('button, input, select, textarea, a, [role="menu"]')) return;
      event.preventDefault();
      const window = getCurrentWindow();
      void (event.detail === 2 ? window.toggleMaximize() : window.startDragging()).catch(() => undefined);
    };
    window.addEventListener('mousedown', onMouseDown);
    return () => window.removeEventListener('mousedown', onMouseDown);
  }, []);

  // ---- Keyboard ----

  const keyState = useRef({ selection, selectedIds, order, columns, inTrash, nav, menu, view: prefs.view, act });
  keyState.current = { selection, selectedIds, order, columns, inTrash, nav, menu, view: prefs.view, act };

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!shouldHandleGlobalKey(event, !!document.querySelector('dialog[open]'))) return;
      const state = keyState.current;
      if (state.menu) return;
      const meta = event.metaKey || event.ctrlKey;
      const key = event.key.toLowerCase();
      const ids = state.selectedIds;
      if (meta && event.key === ',') { event.preventDefault(); setDialog({ kind: 'settings' }); return; }
      const browsing = state.nav.kind !== 'similar' && state.nav.kind !== 'characters';

      if (meta && key === 'f') { event.preventDefault(); searchRef.current?.focus(); searchRef.current?.select(); return; }
      if (meta && key === 'i') { event.preventDefault(); setPrefs({ inspector: !prefsRef.current.inspector }); return; }
      if (meta && (key === '1' || key === '2')) { event.preventDefault(); setPrefs({ view: key === '1' ? 'grid' : 'list' }); return; }
      if (!browsing) return;
      if (meta && key === 'a') { event.preventDefault(); setSelection({ ids: new Set(state.order), anchor: state.order[state.order.length - 1] ?? '' }); return; }
      if (meta && key === 'c' && ids.length && !state.inTrash) { event.preventDefault(); state.act.copy(ids); return; }
      if (meta && (key === 'backspace' || key === 'delete') && ids.length) {
        event.preventDefault();
        if (state.inTrash) state.act.deleteForever(ids); else state.act.trash(ids);
        return;
      }
      if (meta || event.altKey) return;
      if (key === 'escape' && ids.length) { event.preventDefault(); setSelection(emptySelection()); return; }
      if (key === ' ' && ids.length) {
        event.preventDefault();
        state.act.quickLook(state.selection.ids.has(state.selection.anchor) ? state.selection.anchor : ids[0]);
        return;
      }
      if (key === 'enter' && ids.length && !state.inTrash) { event.preventDefault(); state.act.rename(ids); return; }
      if (key.startsWith('arrow')) {
        const columnsNow = state.view === 'grid' ? state.columns : 1;
        if (state.view === 'list' && (key === 'arrowleft' || key === 'arrowright')) return;
        const next = moveSelect(state.selection, state.order, columnsNow, event.key, event.shiftKey);
        if (next) { event.preventDefault(); setSelection(next); }
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [setPrefs]);
  const prefsRef = useRef(prefs);
  prefsRef.current = prefs;

  // Grid thumbnails: (re)generate whenever the library or its size changes; bump to retry tiles that fell back.
  const [thumbVersion, setThumbVersion] = useState(0);
  const itemCount = library?.items.length ?? 0;
  useEffect(() => {
    if (!api.isDesktop || !root || !itemCount) return;
    let active = true;
    api.ensureThumbnails(root)
      .then(report => { if (active && report.created) setThumbVersion(version => version + 1); })
      .catch(reason => console.warn('缩略图生成失败', reason));
    return () => { active = false; };
  }, [root, itemCount]);

  // Character recognition is opt-in: the sidebar entry only appears once the model is installed.
  const [classifier, setClassifier] = useState<api.ClassifierStatus | null>(null);
  const [classifyResult, setClassifyResult] = useState<api.ClassifyResult | null>(null);
  const [classifying, setClassifying] = useState(false);
  useEffect(() => {
    if (api.isDesktop) api.classifierStatus().then(setClassifier, () => setClassifier(null));
  }, []);
  useEffect(() => { setClassifyResult(null); }, [root]);
  const runClassify = useCallback(async () => {
    if (!root || classifying) return;
    setClassifying(true);
    try { setClassifyResult(await api.classifyLibrary(root)); }
    catch (reason) { toast.push({ kind: 'error', title: '形象识别失败', detail: errorText(reason) }); }
    finally { setClassifying(false); }
  }, [root, classifying, toast]);
  useEffect(() => {
    if (nav.kind === 'characters' && !classifyResult && !classifying && classifier?.enabled) void runClassify();
  }, [nav.kind]);
  useEffect(() => {
    if (nav.kind === 'characters' && classifier && !classifier.enabled) navigate({ kind: 'all' });
  }, [classifier, nav.kind, navigate]);
  function withoutSuggestions(character: string, ids: string[]) {
    const gone = new Set(ids);
    setClassifyResult(result => result && {
      ...result,
      suggestions: result.suggestions.filter(s => !(s.character === character && gone.has(s.assetId))),
    });
  }

  const [welcomeRecent, setWelcomeRecent] = useState<string[]>([]);
  useEffect(() => {
    if (!library && !lib.loading) api.recentLibraries().then(setWelcomeRecent, () => setWelcomeRecent([]));
  }, [library, lib.loading]);

  // ---- Render ----

  if (lib.loading) return <div className="app-loading" data-window-drag-region />;

  if (!library) {
    return <>
      <Welcome busy={!!lib.busy} recent={welcomeRecent.map(path => ({ path, name: libraryName(path), place: libraryPlace(path) }))}
        onOpenRecent={path => void lib.openRecent(path)}
        onCreate={() => void lib.openLibrary(true)} onOpen={() => void lib.openLibrary(false)} onRestore={() => void lib.restoreBackup()} />
      <Toasts toasts={toast.toasts} onDismiss={toast.dismiss} />
    </>;
  }

  const inspectorActions: InspectorActions = {
    copy: act.copy,
    favorite: act.favorite,
    reveal: act.reveal,
    more: (ids, anchor) => setMenu(menuAt(anchor, [
      { key: 'export', label: '导出原图…', onSelect: () => act.exportIds(ids) },
      { key: 'reveal', label: '在访达中显示', onSelect: () => act.reveal(ids) },
      ...(ids.length > 1 ? [{ key: 'rename', label: '批量重命名…', onSelect: () => act.rename(ids) }] : []),
      { key: 'sep', label: '' },
      { key: 'trash', label: '移入回收站', danger: true, onSelect: () => act.trash(ids) },
    ])),
    trash: act.trash,
    restore: act.restore,
    deleteForever: act.deleteForever,
    rename: (id, name) => saveMeta(id, { name }),
    saveTags: (id, tags) => saveMeta(id, { tags }),
    collectionPicker: (ids, anchor) => setMenu(menuAt(anchor, collectionItems(ids))),
    leaveCollection: act.leave,
    batchRename: ids => setDialog({ kind: 'batch-rename', ids }),
    disband: groupId => void lib.disband(groupId),
    openVersion: id => setQuickLook(id),
  };

  const title = navTitle(nav);
  const mode = nav.kind === 'similar' ? 'similar' : nav.kind === 'characters' ? 'characters' : inTrash ? 'trash' : 'browse';
  const subtitle = mode === 'similar' ? '找出重复与相似的表情'
    : mode === 'characters' ? (classifyResult ? `${classifyResult.suggestions.length} 个建议待确认` : '按形象自动归类')
    : query ? `找到 ${items.length} 个`
    : inTrash ? (items.length ? `${items.length} 项 · 彻底删除后移到系统废纸篓` : '')
    : `${items.length} 个表情`;

  const quickLookOrder = quickLook && order.includes(quickLook) ? order : [quickLook];
  const quickLookItem = quickLook ? index.byId.get(quickLook) : undefined;
  const collectionNames = index.collections.map(([name]) => name);

  return (
    <div className={`app${prefs.inspector ? ' with-inspector' : ''}`}>
      <Sidebar index={index} nav={nav} libraryName={libraryName(root)} dropTarget={dropTarget}
        onNav={navigate} onNewCollection={() => act.newCollection([])}
        charactersEnabled={!!classifier?.enabled}
        onCollectionMenu={openCollectionMenu} onLibraryMenu={openLibraryMenu} />

      <main className="main">
        <Toolbar ref={searchRef} title={title} subtitle={subtitle} mode={mode} prefs={prefs} query={query}
          canEmptyTrash={index.counts.trash > 0} scanning={mode === 'characters' ? classifying : scanning}
          onQuery={value => { setQuery(value); }} onPrefs={setPrefs}
          onSortMenu={openSortMenu} onImportMenu={anchor => setMenu(menuAt(anchor, importItems))}
          onEmptyTrash={() => act.deleteForever('all')} onRescan={() => void (mode === 'characters' ? runClassify() : runScan())} />

        {lib.managementError ? <p className="banner" role="alert">整理记录读取失败：{lib.managementError}。收藏、合集与回收站暂不可用。</p> : null}

        {nav.kind === 'characters' ? (
          <div className="similar-scroller">
            <CharacterReview result={classifyResult} running={classifying} items={index.items} root={root} thumbVersion={thumbVersion}
              onRun={() => void runClassify()}
              onAccept={async (character, accepted) => {
                const ids = accepted.map(s => s.assetId);
                const exists = (management?.collections ?? []).includes(character) || index.collections.some(([name]) => name === character);
                const moves = new Map<string, string[]>();
                for (const s of accepted) if (s.replaces) moves.set(s.replaces, [...(moves.get(s.replaces) ?? []), s.assetId]);
                lib.setManagement(await lib.write(async rootPath => {
                  let next = exists
                    ? await api.batchLabels(rootPath, ids, [], [], [character], [])
                    : await api.addCollection(rootPath, character, ids);
                  // Misfiled look-alikes (e.g. plain 熊猫头 under 龙图) leave their old collection.
                  for (const [from, moved] of moves) next = await api.batchLabels(rootPath, moved, [], [], [], [from]);
                  return next;
                }));
                withoutSuggestions(character, ids);
                const moved = [...moves.values()].reduce((n, list) => n + list.length, 0);
                toast.push({ kind: 'success', title: `已加入「${character}」`,
                  detail: `${ids.length} 个表情${moved ? `（其中 ${moved} 个从「${[...moves.keys()].join('」「')}」移出）` : ''}，之后会作为识别样例。` });
              }}
              onDismiss={async (character, ids) => {
                await lib.write(rootPath => api.dismissSuggestions(rootPath, ids.map(assetId => ({ assetId, character }))));
                withoutSuggestions(character, ids);
              }} />
          </div>
        ) : nav.kind === 'similar' ? (
          <div className="similar-scroller">
            <SimilarReview report={scanReport} scanning={scanning} items={index.items} root={root}
              existingGroups={management?.groups ?? []}
              onRescan={() => void runScan()}
              onKeep={async (_keepId, removedIds) => {
                lib.setManagement(await lib.write(rootPath => api.setTrash(rootPath, removedIds, true)));
                setScanReport(report => report && pruneReviewed(report, removedIds));
              }}
              onIgnorePair={async (a, b) => {
                lib.setManagement(await lib.write(rootPath => api.ignorePair(rootPath, a, b)));
                setScanReport(report => report && withoutPair(report, a, b));
              }} />
          </div>
        ) : (
          <StickerGrid items={items} root={root} index={index} view={prefs.view} tileSize={prefs.tileSize} thumbVersion={thumbVersion}
            selection={selection} resetKey={`${nav.kind}:${'value' in nav ? nav.value : ''}:${query}:${prefs.sort}`}
            onSelection={setSelection} onColumns={setColumns} onOpen={act.quickLook}
            onContextMenu={openGridMenu} onDragOut={ids => void dragOut(ids)}
            empty={<GridEmpty nav={nav} query={query} libraryEmpty={!library.items.length}
              onClearQuery={() => setQuery('')}
              onImport={() => void lib.importPick(false)}
              onWechat={() => setDialog({ kind: 'import-wechat' })}
              onDouyin={() => setDialog({ kind: 'collect-douyin' })} />} />
        )}

        {selectedIds.length > 1 && nav.kind !== 'similar' && nav.kind !== 'characters' ? (
          <SelectionBar count={selectedIds.length} inTrash={inTrash}
            onCopy={() => act.copy(selectedIds)}
            onCollect={anchor => setMenu(menuAt(anchor, collectionItems(selectedIds), true))}
            onExport={() => act.exportIds(selectedIds)}
            onTrash={() => act.trash(selectedIds)}
            onRestore={() => act.restore(selectedIds)}
            onDelete={() => act.deleteForever(selectedIds)}
            onClear={() => setSelection(emptySelection())} />
        ) : null}

        {dropCount ? (
          <div className="drop-overlay" aria-live="polite">
            <div className="drop-card"><Inbox size={28} /><strong>松开以导入</strong><span>{dropCount} 个项目，文件夹会包含其中的图片</span></div>
          </div>
        ) : null}
      </main>

      {prefs.inspector ? (
        <Inspector ids={nav.kind === 'similar' || nav.kind === 'characters' ? [] : selectedIds} index={index} management={management} root={root}
          libraryName={libraryName(root)} inTrash={inTrash} focusToken={focusToken} actions={inspectorActions} />
      ) : null}

      {menu ? <ContextMenu x={menu.x} y={menu.y} above={menu.above} items={menu.items} onClose={() => setMenu(null)} /> : null}

      {quickLookItem ? (
        <QuickLook item={quickLookItem} root={root}
          position={quickLookOrder.indexOf(quickLook)} total={quickLookOrder.length}
          favorite={!!index.favorites[quickLook]}
          onStep={delta => {
            const next = quickLookOrder[quickLookOrder.indexOf(quickLook) + delta];
            if (!next) return;
            setQuickLook(next);
            if (order.includes(next)) setSelection({ ids: new Set([next]), anchor: next });
          }}
          onCopy={() => act.copy([quickLook])}
          onFavorite={() => act.favorite([quickLook], !index.favorites[quickLook])}
          onClose={() => setQuickLook('')} />
      ) : null}

      {dialog?.kind === 'new-collection' ? (
        <PromptDialog title={dialog.ids.length ? `用 ${dialog.ids.length} 个表情新建合集` : '新建合集'} label="合集名称" placeholder="例如：日常、工作、阴阳怪气" confirmLabel="创建"
          validate={value => validateCollectionName(value, collectionNames)}
          onSubmit={async name => {
            lib.setManagement(await lib.write(rootPath => api.addCollection(rootPath, name, dialog.ids)));
            if (dialog.ids.length) toast.push({ kind: 'success', title: `已创建「${name}」`, detail: `加入了 ${dialog.ids.length} 个表情` });
            else navigate({ kind: 'collection', value: name });
          }}
          onClose={() => setDialog(null)} />
      ) : null}

      {dialog?.kind === 'rename-collection' ? (
        <PromptDialog title="重命名合集" label="合集名称" initial={dialog.name} confirmLabel="重命名"
          validate={value => validateCollectionName(value, collectionNames, dialog.name)}
          onSubmit={async name => {
            if (name === dialog.name) return;
            lib.setManagement(await lib.write(rootPath => api.renameCollection(rootPath, dialog.name, name)));
            if (sameNav(nav, { kind: 'collection', value: dialog.name })) setNav({ kind: 'collection', value: name });
          }}
          onClose={() => setDialog(null)} />
      ) : null}

      {dialog?.kind === 'delete-collection' ? (
        <ConfirmDialog title={`删除合集「${dialog.name}」？`} confirmLabel="删除合集" danger
          message="合集里的表情不会被删除，仍在资料库中。"
          onConfirm={async () => {
            lib.setManagement(await lib.write(rootPath => api.deleteCollection(rootPath, dialog.name)));
            if (sameNav(nav, { kind: 'collection', value: dialog.name })) navigate({ kind: 'all' });
          }}
          onClose={() => setDialog(null)} />
      ) : null}

      {dialog?.kind === 'delete-forever' ? (
        <ConfirmDialog
          title={dialog.ids === 'all' ? `清空回收站中的 ${index.counts.trash} 项？` : `彻底删除 ${dialog.ids.length} 项？`}
          confirmLabel="彻底删除" danger
          message="原文件会移到系统废纸篓，并从资料库、合集和整理记录中移除。"
          onConfirm={async () => { await lib.deleteForever(dialog.ids); }}
          onClose={() => setDialog(null)} />
      ) : null}

      {dialog?.kind === 'batch-rename' ? (
        <BatchRenameDialog count={dialog.ids.length}
          onSubmit={async (prefix, start) => {
            lib.setManagement(await lib.write(rootPath => api.batchRename(rootPath, dialog.ids, prefix, start)));
            toast.push({ kind: 'success', title: `已重命名 ${dialog.ids.length} 个表情` });
          }}
          onClose={() => setDialog(null)} />
      ) : null}

      {dialog?.kind === 'collect-douyin' && management ? (
        <CollectDialog root={root} accounts={management.accounts.filter(account => account.platform === '抖音')}
          onSnapshot={(snapshot, warning) => { lib.setLibrary(snapshot); if (warning) toast.push({ kind: 'error', title: '部分素材无法预览', detail: warning }); }}
          onManagement={lib.setManagement} onError={() => undefined} onClose={() => setDialog(null)} />
      ) : null}

      {dialog?.kind === 'settings' ? (
        <SettingsDialog status={classifier} onStatus={status => { setClassifier(status); setClassifyResult(null); }} onClose={() => setDialog(null)} />
      ) : null}

      {dialog?.kind === 'import-wechat' && management ? (
        <WeChatImportDialog root={root} accounts={management.accounts.filter(account => account.platform === '微信')}
          onSnapshot={(snapshot, warning) => { lib.setLibrary(snapshot); if (warning) toast.push({ kind: 'error', title: '部分素材无法预览', detail: warning }); }}
          onManagement={lib.setManagement} onError={() => undefined} onClose={() => setDialog(null)} />
      ) : null}

      <Toasts toasts={toast.toasts} onDismiss={toast.dismiss} />
    </div>
  );
}

function GridEmpty({ nav, query, libraryEmpty, onClearQuery, onImport, onWechat, onDouyin }: {
  nav: Nav; query: string; libraryEmpty: boolean;
  onClearQuery: () => void; onImport: () => void; onWechat: () => void; onDouyin: () => void;
}) {
  if (query.trim()) {
    return <EmptyState icon={<SearchX size={30} />} title={`没有找到「${query.trim()}」`}
      actions={<button className="btn" onClick={onClearQuery}>清除搜索</button>}>搜索会匹配表情名称和标签。</EmptyState>;
  }
  const importActions = <>
    <button className="btn" onClick={onWechat}><MessageCircle size={14} />从微信导入</button>
    <button className="btn" onClick={onDouyin}><Music2 size={14} />从抖音导入</button>
    <button className="btn" onClick={onImport}><FolderOpen size={14} />本地图片</button>
  </>;
  switch (nav.kind) {
    case 'all':
      return libraryEmpty
        ? <EmptyState icon={<Images size={30} />} title="资料库还是空的" actions={importActions}>从微信、抖音导入收藏的表情，或者直接把图片、文件夹拖进窗口。</EmptyState>
        : <EmptyState icon={<Images size={30} />} title="没有可显示的表情">所有表情都在回收站里。</EmptyState>;
    case 'favorites':
      return <EmptyState icon={<Star size={30} />} title="还没有收藏">选中表情后点星标，常用的就会集中在这里。</EmptyState>;
    case 'recent':
      return <EmptyState icon={<Clock size={30} />} title="还没有使用记录">复制或拖出过的表情会按时间出现在这里。</EmptyState>;
    case 'source':
      return <EmptyState icon={nav.value === '微信' ? <MessageCircle size={30} /> : nav.value === '抖音' ? <Music2 size={30} /> : <FolderOpen size={30} />}
        title={`还没有${nav.value === '本地' ? '本地导入' : `来自${nav.value}`}的表情`}
        actions={<button className="btn btn-primary" onClick={nav.value === '微信' ? onWechat : nav.value === '抖音' ? onDouyin : onImport}>
          {nav.value === '微信' ? '从微信导入…' : nav.value === '抖音' ? '从抖音导入…' : '导入本地图片…'}</button>} />;
    case 'collection':
      return <EmptyState icon={<Library size={30} />} title="合集是空的">右键表情选择「加入合集」，或把表情拖到左侧的合集上。</EmptyState>;
    case 'trash':
      return <EmptyState icon={<Trash2 size={30} />} title="回收站是空的">移入回收站的表情可以随时恢复。</EmptyState>;
    default:
      return null;
  }
}
