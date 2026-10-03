import { useEffect, useMemo, useRef, useState } from 'react';
import { Archive, ArrowDownToLine, CheckCircle2, ChevronLeft, ChevronRight, CircleAlert, Folder, FolderOpen, FolderPlus, HardDrive, Images, LoaderCircle, MessageCircle, Music2, Search, Smile, X } from 'lucide-react';
import { chooseLibrary, currentLibrary, imageUrl, importImages, isDesktop, getManagement, saveMetadata, importProvenance } from './api';
import { SOURCE_LABELS, formatBytes, type ImportReport, type Snapshot, type Sticker, type ManagementSnapshot, type AssetMetadata } from './types';
import { StickerDetail } from './components/StickerDetail';
import { StickerPreview } from './components/StickerPreview';
import { ManagementPanel } from './components/ManagementPanel';
import './styles.css';

const PAGE_SIZE = 60;
const navigation = [
  { id: 'all', label: '全部表情', icon: Images },
  { id: '微信', label: '微信', icon: MessageCircle },
  { id: '抖音', label: '抖音', icon: Music2 },
  { id: '本地', label: '本地文件', icon: Folder },
];

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
  const [selected, setSelected] = useState<Sticker | null>(null);
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

  function openLibrary(create: boolean) {
    void run(create ? '正在创建资料库' : '正在打开资料库', async () => {
      const snapshot = await chooseLibrary(create);
      if (snapshot) {
        setLibrary(snapshot); setManagementNotice(''); setManagement(null); setManagementError(''); setTag(''); setCollection(''); setReport(null); setSelected(null); setQuery(''); setSource('all'); setPage(1);
        try { setManagement(await getManagement()); } catch (reason) { setManagementError(String(reason)); }
      }
    });
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

  const counts = useMemo(() => {
    const values: Record<string, number> = { all: library?.items.length ?? 0, 本地: 0, 微信: 0, 抖音: 0 };
    for (const item of library?.items ?? []) for (const origin of item.sources) values[origin] = (values[origin] ?? 0) + 1;
    return values;
  }, [library]);
  const displayItems = useMemo(() => (library?.items ?? []).map(item => ({ ...item, name: management?.metadata[item.id]?.name || item.name })), [library, management]);
  const facets = useMemo(() => {
    const tags = new Set<string>();
    const collections = new Set<string>();
    for (const value of Object.values(management?.metadata ?? {})) {
      value.tags.forEach(value => tags.add(value));
      value.collections.forEach(value => collections.add(value));
    }
    return { tags: [...tags].sort(), collections: [...collections].sort() };
  }, [management]);
  const filtered = useMemo(() => {
    const search = query.trim().toLocaleLowerCase();
    return displayItems.filter(item => {
      const meta = management?.metadata[item.id];
      return (source === 'all' || item.sources.includes(source))
        && (!tag || meta?.tags.includes(tag)) && (!collection || meta?.collections.includes(collection))
        && (!search || [item.name, ...(meta?.tags ?? [])].some(value => value.toLocaleLowerCase().includes(search)));
    });
  }, [displayItems, management, query, source, tag, collection]);
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

  return (
    <div className="app-shell">
      <aside className="sidebar" aria-label="资料库导航">
        <div className="brand"><span className="brand-mark"><Smile size={25} strokeWidth={1.7} /></span><div><strong>拾趣</strong><span>StickerNest</span></div></div>
        <p className="nav-caption">我的资料库</p>
        <nav>{navigation.map(({ id, label, icon: Icon }) => <button key={id} className={`nav-item ${source === id ? 'selected' : ''}`} aria-current={source === id ? 'page' : undefined} onClick={() => { setSource(id); setPage(1); }}><Icon size={18} /><span>{label}</span><span className="count">{counts[id]}</span></button>)}</nav>
        <div className="sidebar-bottom">
          <div className="local-indicator"><HardDrive size={16} /><span>本地存储</span><span className="status-dot" /></div>
          <button className="button secondary full-width" disabled={disabled} onClick={() => openLibrary(false)}><FolderOpen size={16} />打开资料库</button>
          {library ? <button className="text-button full-width" disabled={disabled} onClick={() => openLibrary(true)}>新建资料库</button> : null}
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
        {managementError ? <div className="notice error" role="alert"><div><strong>整理记录未能读取</strong><p>{managementError}</p><p>编辑已停用，原记录保留；修复整理文件后重开资料库。</p></div></div> : null}
        {!isDesktop ? <div className="notice"><CircleAlert size={18} /><p>请使用桌面应用打开本地资料库。</p></div> : null}
        {busy ? <div className="notice" role="status"><LoaderCircle className="spin" size={18} /><p>{busy}…</p></div> : null}
        {error ? <div className="notice error" role="alert"><CircleAlert size={18} /><div><strong>操作未完成</strong><p>{error}</p><p>检查文件或权限后重试。</p></div><button className="icon-button" aria-label="关闭错误提示" onClick={() => setError('')}><X size={17} /></button></div> : null}
        {previewWarning ? <div className="notice warning" role="status"><CircleAlert size={18} /><div><strong>预览提示</strong><p>{previewWarning}</p><p>请检查原文件是否仍在。</p></div><button className="icon-button" aria-label="关闭预览提示" onClick={() => setPreviewWarning('')}><X size={17} /></button></div> : null}
        {report ? <div className={`notice import-report ${report.failed.length ? 'warning' : 'success'}`} role="status"><CheckCircle2 size={18} /><div><strong>导入完成</strong><p>新增 {report.added} 个 · 重复 {report.duplicates} 个 · 失败 {report.failed.length} 个</p>{report.failed.length ? <details><summary>查看失败文件</summary><ul>{report.failed.map((failure, index) => <li key={`${index}-${failure.name}`}><b>{failure.name}</b>：{failure.error}</li>)}</ul><p>修复后重新导入，已入库项会自动去重。</p></details> : null}</div><button className="icon-button" aria-label="关闭导入结果" onClick={() => setReport(null)}><X size={17} /></button></div> : null}

        {!library ? <section className="empty-state welcome"><span className="empty-icon"><Archive size={38} strokeWidth={1.4} /></span><h2>打开你的表情库</h2><p>新建或打开一个本地文件夹。</p><div className="empty-actions"><button className="button primary" disabled={disabled} onClick={() => openLibrary(true)}><FolderPlus size={18} />新建资料库</button><button className="button secondary" disabled={disabled} onClick={() => openLibrary(false)}><FolderOpen size={18} />打开已有资料库</button></div></section> : visible.length === 0 ? <section className="empty-state"><span className="empty-icon"><Images size={36} strokeWidth={1.4} /></span><h2>{query || tag || collection || source !== 'all' ? '没有匹配的表情' : '还没有表情'}</h2><p>{query || tag || collection || source !== 'all' ? '试试其他关键词或筛选。' : '导入本地图片或动图。'}</p>{query || tag || collection || source !== 'all' ? <button className="button secondary" onClick={() => { setQuery(''); setTag(''); setCollection(''); setSource('all'); setPage(1); }}>查看全部表情</button> : <button className="button primary" disabled={disabled} onClick={importFiles}><ArrowDownToLine size={18} />导入表情</button>}</section> : <><div className="collection-caption"><span>{query ? `找到 ${filtered.length} 个表情` : `共 ${filtered.length} 个表情`}</span></div><section className="sticker-grid" aria-label="表情列表">{visible.map(item => <button className="sticker-card" key={item.id} onClick={() => setSelected(library.items.find(original => original.id === item.id) ?? item)} aria-label={`查看 ${item.name}`}><div className="sticker-image"><StickerPreview key={`${library.root}/${item.fileName}`} src={imageUrl(library.root, item.fileName)} name={item.name} /><span className="format-badge">{item.format.toUpperCase()}</span></div><div className="sticker-info"><strong title={item.name}>{item.name}</strong><span>{item.sources.map(origin => SOURCE_LABELS[origin] ?? origin).join(' / ')}<span>{formatBytes(item.bytes)}</span></span></div></button>)}</section><div className="pagination"><span>第 {visiblePage} / {totalPages} 页</span><button className="icon-button" aria-label="上一页" disabled={visiblePage === 1} onClick={() => setPage(visiblePage - 1)}><ChevronLeft size={20} /></button><button className="icon-button" aria-label="下一页" disabled={visiblePage === totalPages} onClick={() => setPage(visiblePage + 1)}><ChevronRight size={20} /></button></div></>}
        <footer className="library-footer"><Folder size={14} /><span title={library?.root}>{library?.root ?? '尚未选择资料库'}</span><span className="footer-count">{library ? `${library.items.length} 个原始素材` : '本地资料库'}</span></footer>
      </main>
      {selected && library ? <StickerDetail key={selected.id} item={selected} root={library.root} metadata={management?.metadata[selected.id]} editable={!!management && !disabled} onSave={metadata => editMetadata(selected.id, metadata)} onClose={() => setSelected(null)} /> : null}
    </div>
  );
}
