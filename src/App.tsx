import { Fragment, useEffect, useMemo, useRef, useState } from 'react';
import { Archive, ArchiveRestore, ArrowDownToLine, CheckCircle2, ChevronLeft, ChevronRight, CircleAlert, Copy, Folder, FolderOpen, FolderPlus, HardDrive, HardDriveDownload, Images, Layers, LoaderCircle, MessageCircle, Music2, Search, Smile, Trash2, X } from 'lucide-react';
import { chooseLibrary, currentLibrary, imageUrl, importImages, isDesktop, getManagement, saveMetadata, importProvenance, setTrash, backupLibrary, restoreBackup, openLibraryPath, scanDuplicates, disbandGroup, ignorePair, exportAssets, saveGroupMetadata, batchRename, batchLabels } from './api';
import { SOURCE_LABELS, formatBytes, type ExportSummary, type ImportReport, type ScanReport, type Snapshot, type Sticker, type ManagementSnapshot, type AssetMetadata } from './types';
import { StickerDetail } from './components/StickerDetail';
import { StickerPreview } from './components/StickerPreview';
import { ManagementPanel } from './components/ManagementPanel';
import { DuplicateReview } from './components/DuplicateReview';
import { BatchPanel } from './components/BatchPanel';
import { CollectDialog } from './components/CollectDialog';
import './styles.css';

const PAGE_SIZE = 60;
const navigation = [
  { id: 'all', label: '全部表情', icon: Images },
  { id: '微信', label: '微信', icon: MessageCircle },
  { id: '抖音', label: '抖音', icon: Music2 },
  { id: '本地', label: '本地文件', icon: Folder },
  { id: 'groups', label: '版本分组', icon: Layers },
  { id: 'trash', label: '回收站', icon: Trash2 },
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
  const [managementNotice, setManagementNotice] = useState('');
  const [tag, setTag] = useState('');
  const [collection, setCollection] = useState('');
  const [source, setSource] = useState('all');
  const [importSource, setImportSource] = useState('本地');
  const [query, setQuery] = useState('');
  const [page, setPage] = useState(1);
  const [busy, setBusy] = useState(isDesktop ? '正在读取资料库' : '');
  const [error, setError] = useState('');
  const [previewWarning, setPreviewWarning] = useState('');
  const [report, setReport] = useState<ImportReport | null>(null);
  const [backupNotice, setBackupNotice] = useState<{ text: string; path: string } | null>(null);
  const [exportNotice, setExportNotice] = useState<ExportSummary | null>(null);
  const [scanReport, setScanReport] = useState<ScanReport | null>(null);
  const [showDuplicates, setShowDuplicates] = useState(false);
  const [selected, setSelected] = useState<Sticker | null>(null);
  const [checkedIds, setCheckedIds] = useState<Set<string>>(new Set());
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());
  const [showBatch, setShowBatch] = useState(false);
  const [includeVersions, setIncludeVersions] = useState(false);
  const [showCollect, setShowCollect] = useState(false);
  const locked = useRef(false);

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
    setLibrary(snapshot); setManagementNotice(''); setManagement(null); setManagementError(''); setTag(''); setCollection(''); setReport(null); setSelected(null); setQuery(''); setSource('all'); setPage(1); setBackupNotice(null); setExportNotice(null); setScanReport(null); setShowDuplicates(false); setCheckedIds(new Set()); setExpandedGroups(new Set()); setShowBatch(false); setIncludeVersions(false);
    try { setManagement(await getManagement()); } catch (reason) { setManagementError(String(reason)); }
  }

  function openLibrary(create: boolean) {
    void run(create ? '正在创建资料库' : '正在打开资料库', async () => {
      const snapshot = await chooseLibrary(create);
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
    void run('正在扫描重复素材', async () => {
      setScanReport(await scanDuplicates());
      setShowDuplicates(true);
    });
  }

  async function keepOneAsset(_keepId: string, removedIds: string[]) {
    if (!library) throw new Error('请先打开资料库。');
    // Others move to the recycle bin, recoverable any time from the trash view.
    setManagement(await setTrash(library.root, removedIds, true));
    setCheckedIds(current => new Set([...current].filter(id => !removedIds.includes(id))));
  }

  async function ignoreSimilarPair(a: string, b: string) {
    if (!library) throw new Error('请先打开资料库。');
    setManagement(await ignorePair(library.root, a, b));
    // The caller re-scans once after the action, not here.
  }

  function exportSelection() {
    void run('正在导出素材', async () => {
      if (!library) return;
      let ids = checkedIds.size ? [...checkedIds] : filtered.map(item => item.id);
      if (includeVersions && management) {
        const expanded = new Set(ids);
        for (const group of management.groups) {
          if (group.memberIds.some(id => expanded.has(id))) group.memberIds.forEach(id => expanded.add(id));
        }
        ids = [...expanded];
      }
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
      const result = await importImages(importSource);
      if (result) {
        setLibrary(result.snapshot); setReport(result.report); setPage(1);
        if (result.previewWarnings.length) setPreviewWarning(`导入结果已保存，部分素材无法预览：${result.previewWarnings.join('；')}`);
      }
    });
  }

  const trashSet = useMemo(() => new Set(Object.keys(management?.trash ?? {})), [management]);
  const groupByMain = useMemo(() => new Map((management?.groups ?? []).map(group => [group.mainAssetId, group])), [management]);
  const groupedNonMain = useMemo(() => new Set((management?.groups ?? []).flatMap(group => group.memberIds.filter(id => id !== group.mainAssetId))), [management]);
  const counts = useMemo(() => {
    const values: Record<string, number> = { all: 0, 本地: 0, 微信: 0, 抖音: 0, groups: 0, trash: 0 };
    for (const item of library?.items ?? []) {
      if (trashSet.has(item.id)) { values.trash += 1; continue; }
      if (groupedNonMain.has(item.id)) continue;
      values.all += 1;
      if (groupByMain.has(item.id)) values.groups += 1;
      for (const origin of item.sources) values[origin] = (values[origin] ?? 0) + 1;
    }
    return values;
  }, [library, trashSet, groupedNonMain, groupByMain]);
  const displayItems = useMemo(() => (library?.items ?? []).map(item => ({ ...item, name: management?.metadata[item.id]?.name || item.name })), [library, management]);
  const facets = useMemo(() => {
    const tags = new Set<string>();
    const collections = new Set<string>();
    for (const value of Object.values(management?.metadata ?? {})) {
      value.tags.forEach(value => tags.add(value));
      value.collections.forEach(value => collections.add(value));
    }
    for (const group of management?.groups ?? []) {
      group.tags.forEach(value => tags.add(value));
      group.collections.forEach(value => collections.add(value));
    }
    return { tags: [...tags].sort(), collections: [...collections].sort() };
  }, [management]);
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
      } else if (source === 'groups') {
        if (trashSet.has(item.id) || !groupByMain.has(item.id)) return false;
      } else {
        if (trashSet.has(item.id) || groupedNonMain.has(item.id)) return false;
        if (source !== 'all' && !item.sources.includes(source)) return false;
      }
      const labels = effectiveLabelsOf.get(item.id) ?? { tags: [], collections: [] };
      return (!tag || labels.tags.includes(tag)) && (!collection || labels.collections.includes(collection))
        && (!search || [item.name, ...labels.tags].some(value => value.toLocaleLowerCase().includes(search)));
    });
  }, [displayItems, management, query, source, tag, collection, trashSet, groupByMain, groupedNonMain, effectiveLabelsOf]);
  async function editMetadata(assetId: string, metadata: AssetMetadata) {
    // The modal prevents switching libraries while its save is pending.
    if (!library) throw new Error('请先打开资料库。');
    setManagement(await saveMetadata(library.root, assetId, metadata));
  }
  async function importSourceReport(alias: string, accountId: string | null) {
    await run('正在导入来源报告', async () => {
      if (!library) throw new Error('请先打开资料库。');
      const result = await importProvenance(library.root, alias, accountId);
      if (result) { setManagement(result); setManagementNotice('报告已保存，完整性见“来源与精确去重”。'); }
    });
  }
  const totalPages = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE));
  const visiblePage = Math.min(page, totalPages);
  const visible = filtered.slice((visiblePage - 1) * PAGE_SIZE, visiblePage * PAGE_SIZE);
  const disabled = !!busy || !isDesktop;
  const heading = navigation.find(item => item.id === source)?.label ?? '全部表情';
  const isFiltering = !!(query || tag || collection) || (source !== 'all' && source !== 'trash' && source !== 'groups');
  const inEmptyTrash = source === 'trash' && !isFiltering;
  const inEmptyGroups = source === 'groups' && !isFiltering;
  const emptyTitle = inEmptyTrash ? '回收站是空的' : inEmptyGroups ? '还没有版本分组' : isFiltering ? '没有匹配的表情' : '还没有表情';
  const emptyHint = inEmptyTrash ? '移入回收站的素材会保留在这里，可随时恢复。' : inEmptyGroups ? '还没有版本分组。' : isFiltering ? '试试其他关键词或筛选。' : '导入本地图片或动图。';
  const inSelectableView = source !== 'trash' && source !== 'groups';

  return (
    <div className="app-shell">
      {busy ? <div className="busy-overlay" aria-hidden="true" /> : null}
      <aside className="sidebar" aria-label="资料库导航">
        <div className="brand"><span className="brand-mark"><Smile size={25} strokeWidth={1.7} /></span><div><strong>拾趣</strong><span>StickerNest</span></div></div>
        <p className="nav-caption">我的资料库</p>
        <nav>{navigation.map(({ id, label, icon: Icon }) => <button key={id} className={`nav-item ${source === id ? 'selected' : ''}`} aria-current={source === id ? 'page' : undefined} onClick={() => { setSource(id); setPage(1); }}><Icon size={18} /><span>{label}</span><span className="count">{counts[id]}</span></button>)}</nav>
        <div className="sidebar-bottom">
          <div className="local-indicator"><HardDrive size={16} /><span>本地存储</span><span className="status-dot" /></div>
          <div className="sidebar-actions">
            <button className="button secondary full-width" disabled={disabled} onClick={() => openLibrary(false)}><FolderOpen size={16} />打开资料库</button>
            {library ? <button className="button secondary full-width" disabled={disabled} onClick={() => setShowCollect(true)}><Music2 size={16} />采集抖音收藏</button> : null}
            {library ? <button className="button secondary full-width" disabled={disabled} onClick={backupCurrentLibrary}><HardDriveDownload size={16} />备份资料库</button> : null}
          </div>
          <div className="sidebar-links">
            {library ? <button className="text-button slim" disabled={disabled} onClick={() => openLibrary(true)}>新建资料库</button> : null}
            <button className="text-button slim" disabled={disabled} onClick={restoreFromBackup}><ArchiveRestore size={15} />从备份恢复</button>
          </div>
        </div>
      </aside>

      <main className="workspace" aria-busy={!!busy}>
        <header className="page-header"><h1>{heading}<span className="heading-count">{counts[source]}</span></h1></header>
        <div className="toolbar">
          <label className="search-field"><Search size={18} /><input type="search" placeholder="搜索名称或标签…" aria-label="搜索名称或标签" value={query} onChange={event => { setQuery(event.target.value); setPage(1); }} disabled={!library} /></label>
          <div className="import-controls"><label className="source-select"><span>导入来源</span><select aria-label="导入来源" value={importSource} onChange={event => setImportSource(event.target.value)} disabled={disabled || !library}>{Object.entries(SOURCE_LABELS).map(([value, label]) => <option value={value} key={value}>{label}</option>)}</select></label><button className="button primary" onClick={importFiles} disabled={disabled || !library}><ArrowDownToLine size={17} />导入表情</button></div>
        </div>

        {library && management ? <><div className="management-filters"><label className="source-select">标签<select value={tag} onChange={event => { setTag(event.target.value); setPage(1); }}><option value="">全部标签</option>{facets.tags.map(value => <option key={value}>{value}</option>)}</select></label><label className="source-select">合集<select value={collection} onChange={event => { setCollection(event.target.value); setPage(1); }}><option value="">全部合集</option>{facets.collections.map(value => <option key={value}>{value}</option>)}</select></label></div><ManagementPanel key={library.root} management={management} busy={disabled} onImport={importSourceReport} /></> : null}
        {managementNotice ? <div className="notice success" role="status"><p>{managementNotice}</p><button className="icon-button" aria-label="关闭来源导入结果" onClick={() => setManagementNotice('')}><X size={17} /></button></div> : null}
        {backupNotice ? <div className="notice success" role="status"><CheckCircle2 size={18} /><div><strong>{backupNotice.text}</strong><p className="notice-path" title={backupNotice.path}>{backupNotice.path}</p></div><button className="icon-button" aria-label="关闭备份结果" onClick={() => setBackupNotice(null)}><X size={17} /></button></div> : null}
        {exportNotice ? <div className="notice success" role="status"><CheckCircle2 size={18} /><div><strong>已导出 {exportNotice.exported} 个素材{exportNotice.skipped ? `，跳过 ${exportNotice.skipped} 个` : ''}（原格式原字节）</strong><p className="notice-path" title={exportNotice.path}>{exportNotice.path}</p></div><button className="icon-button" aria-label="关闭导出结果" onClick={() => setExportNotice(null)}><X size={17} /></button></div> : null}
        {managementError ? <div className="notice error" role="alert"><div><strong>整理记录未能读取</strong><p>{managementError}</p><p>编辑已停用，原记录保留；修复整理文件后重开资料库。</p></div></div> : null}
        {!isDesktop ? <div className="notice"><CircleAlert size={18} /><p>请使用桌面应用打开本地资料库。</p></div> : null}
        {busy ? <div className="notice" role="status"><LoaderCircle className="spin" size={18} /><p>{busy}…</p></div> : null}
        {error ? <div className="notice error" role="alert"><CircleAlert size={18} /><div><strong>操作未完成</strong><p>{error}</p><p>检查文件或权限后重试。</p></div><button className="icon-button" aria-label="关闭错误提示" onClick={() => setError('')}><X size={17} /></button></div> : null}
        {previewWarning ? <div className="notice warning" role="status"><CircleAlert size={18} /><div><strong>预览提示</strong><p>{previewWarning}</p><p>请检查原文件是否仍在。</p></div><button className="icon-button" aria-label="关闭预览提示" onClick={() => setPreviewWarning('')}><X size={17} /></button></div> : null}
        {report ? <div className={`notice import-report ${report.failed.length ? 'warning' : 'success'}`} role="status"><CheckCircle2 size={18} /><div><strong>导入完成</strong><p>新增 {report.added} 个 · 重复 {report.duplicates} 个 · 失败 {report.failed.length} 个</p>{report.duplicates > 0 && trashSet.size > 0 ? <p>重复项未显示时，可能在回收站中。</p> : null}{report.failed.length ? <details><summary>查看失败文件</summary><ul>{report.failed.map((failure, index) => <li key={`${index}-${failure.name}`}><b>{failure.name}</b>：{failure.error}</li>)}</ul><p>修复后重新导入，已入库项会自动去重。</p></details> : null}</div><button className="icon-button" aria-label="关闭导入结果" onClick={() => setReport(null)}><X size={17} /></button></div> : null}

        {!library ? <section className="empty-state welcome"><span className="empty-icon"><Archive size={38} strokeWidth={1.4} /></span><h2>打开你的表情库</h2><p>新建或打开一个本地文件夹。</p><div className="empty-actions"><button className="button primary" disabled={disabled} onClick={() => openLibrary(true)}><FolderPlus size={18} />新建资料库</button><button className="button secondary" disabled={disabled} onClick={() => openLibrary(false)}><FolderOpen size={18} />打开已有资料库</button></div></section> : visible.length === 0 ? <section className="empty-state"><span className="empty-icon">{inEmptyTrash ? <Trash2 size={36} strokeWidth={1.4} /> : <Images size={36} strokeWidth={1.4} />}</span><h2>{emptyTitle}</h2><p>{emptyHint}</p>{isFiltering ? <button className="button secondary" onClick={() => { setQuery(''); setTag(''); setCollection(''); setSource('all'); setPage(1); }}>查看全部表情</button> : source === 'trash' || source === 'groups' ? null : <button className="button primary" disabled={disabled} onClick={importFiles}><ArrowDownToLine size={18} />导入表情</button>}</section> : <><div className="collection-caption"><span>{query ? `找到 ${filtered.length} 个表情` : `共 ${filtered.length} 个表情`}</span>{inSelectableView ? <div className="grid-actions">{checkedIds.size ? <span className="checked-caption">已选 {checkedIds.size} 项</span> : null}{library && management ? <button className="text-button" type="button" disabled={disabled} onClick={runScan}><Copy size={14} />查重扫描</button> : null}{checkedIds.size ? <button className="text-button" type="button" disabled={disabled} onClick={() => setShowBatch(open => !open)}>批量整理</button> : null}<label className="versions-check" title="导出时把所选素材所在分组的全部版本一并包括"><input type="checkbox" checked={includeVersions} onChange={event => setIncludeVersions(event.target.checked)} />含全部版本</label><button className="text-button" type="button" disabled={disabled || (checkedIds.size === 0 && filtered.length === 0)} onClick={exportSelection}><FolderOpen size={14} />{checkedIds.size ? '导出所选' : '导出当前列表'}</button></div> : null}</div>{showBatch && checkedIds.size ? <BatchPanel count={checkedIds.size} disabled={disabled} onRename={batchRenameSelected} onLabels={batchLabelsSelected} onTrash={trashSelected} onClose={() => setShowBatch(false)} /> : null}
        <section className="sticker-grid" aria-label="表情列表">{visible.map(item => { const group = groupByMain.get(item.id); const isChecked = checkedIds.has(item.id); const memberItems = group ? group.memberIds.map(id => library.items.find(original => original.id === id)).filter((v): v is Sticker => !!v) : []; return <Fragment key={item.id}><div className="sticker-cell">{inSelectableView ? <input className="select-box" type="checkbox" aria-label={`选择 ${item.name}`} checked={isChecked} onChange={() => setCheckedIds(current => { const next = new Set(current); if (next.has(item.id)) next.delete(item.id); else next.add(item.id); return next; })} /> : null}<button className={`sticker-card${isChecked ? ' checked' : ''}`} onClick={() => setSelected(library.items.find(original => original.id === item.id) ?? item)} aria-label={`查看 ${item.name}`}><div className="sticker-image"><StickerPreview key={`${library.root}/${item.fileName}`} src={imageUrl(library.root, item.fileName)} name={item.name} /><span className="format-badge">{item.format.toUpperCase()}</span>{group ? <span className="group-badge" title={`版本分组 ${group.memberIds.length} 项`}><Layers size={11} />{group.memberIds.length}</span> : null}</div><div className="sticker-info"><strong title={item.name}>{shortDisplayName(item.name)}</strong><span>{item.sources.map(origin => SOURCE_LABELS[origin] ?? origin).join(' / ')}<span>{formatBytes(item.bytes)}</span></span></div></button>{group ? <div className="group-tray"><button className="group-expand-btn" type="button" disabled={disabled} onClick={() => setExpandedGroups(current => { const next = new Set(current); if (next.has(group.id)) next.delete(group.id); else next.add(group.id); return next; })}>{expandedGroups.has(group.id) ? '收起版本' : `展开 ${group.memberIds.length} 个版本`}</button><button className="group-disband-btn" type="button" disabled={disabled} onClick={() => void disband(group.id)}>拆分分组</button>{group.tags.length || group.collections.length ? <span className="group-meta-inline" title={`组标签：${group.tags.join('、') || '无'} · 组合集：${group.collections.join('、') || '无'}`}>{group.tags.join('、')}{group.tags.length && group.collections.length ? ' · ' : ''}{group.collections.join('、')}</span> : null}</div> : null}</div>{group && expandedGroups.has(group.id) ? <div className="group-row"><div className="group-members">{memberItems.map(member => <button className="group-member" key={member.id} onClick={() => setSelected(member)} aria-label={`查看 ${member.name}`}><StickerPreview src={imageUrl(library.root, member.fileName)} name={member.name} /><span className="pair-caption" title={member.name}>{shortDisplayName(member.name)}</span></button>)}</div><GroupMetaEditor group={group} disabled={disabled} onSave={saveGroupMeta} /></div> : null}</Fragment>; })}</section><div className="pagination"><span>第 {visiblePage} / {totalPages} 页</span><button className="icon-button" aria-label="上一页" disabled={visiblePage === 1} onClick={() => setPage(visiblePage - 1)}><ChevronLeft size={20} /></button><button className="icon-button" aria-label="下一页" disabled={visiblePage === totalPages} onClick={() => setPage(visiblePage + 1)}><ChevronRight size={20} /></button></div></>}
        <footer className="library-footer"><Folder size={14} /><span title={library?.root}>{library?.root ?? '尚未选择资料库'}</span><span className="footer-count">{library ? `${library.items.length} 个原始素材` : '本地资料库'}</span></footer>
      </main>
      {selected && library ? <StickerDetail key={selected.id} item={selected} root={library.root} metadata={management?.metadata[selected.id]} editable={!!management && !disabled} trashed={!!management?.trash[selected.id]} trashedAt={management?.trash[selected.id]} onSave={metadata => editMetadata(selected.id, metadata)} onSetTrash={value => setItemsTrash([selected.id], value)} onClose={() => setSelected(null)} /> : null}
      {showDuplicates && scanReport && library && management ? <DuplicateReview report={scanReport} items={library.items} root={library.root} existingGroups={management.groups} onKeep={keepOneAsset} onIgnorePair={ignoreSimilarPair} onRescan={async () => setScanReport(await scanDuplicates())} onClose={() => setShowDuplicates(false)} /> : null}
      {showCollect && library && management ? <CollectDialog root={library.root} accounts={management.accounts.filter(a => a.platform === '抖音')} onSnapshot={(snapshot, warning) => { setLibrary(snapshot); setPage(1); if (warning) setPreviewWarning(warning); }} onManagement={setManagement} onError={setError} onClose={() => setShowCollect(false)} /> : null}
    </div>
  );
}
