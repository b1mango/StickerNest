import { forwardRef } from 'react';
import { ArrowUpDown, ChevronDown, LayoutGrid, List, PanelRight, Plus, RefreshCw, Search, Trash2, X } from 'lucide-react';
import { TILE_MAX, TILE_MIN, type Prefs } from '../hooks/usePrefs';

export const Toolbar = forwardRef<HTMLInputElement, {
  title: string;
  subtitle: string;
  mode: 'browse' | 'trash' | 'similar' | 'characters';
  prefs: Prefs;
  query: string;
  canEmptyTrash: boolean;
  scanning: boolean;
  onQuery: (value: string) => void;
  onPrefs: (patch: Partial<Prefs>) => void;
  onSortMenu: (anchor: DOMRect) => void;
  onImportMenu: (anchor: DOMRect) => void;
  onEmptyTrash: () => void;
  onRescan: () => void;
}>(function Toolbar({ title, subtitle, mode, prefs, query, canEmptyTrash, scanning, onQuery, onPrefs, onSortMenu, onImportMenu, onEmptyTrash, onRescan }, searchRef) {
  return (
    <header className="toolbar" data-window-drag-region>
      <div className="toolbar-title" data-window-drag-region>
        <h1 data-window-drag-region>{title}</h1>
        <span className="toolbar-subtitle" data-window-drag-region>{subtitle}</span>
      </div>

      <div className="toolbar-tools">
        {mode === 'similar' || mode === 'characters' ? (
          <button className="btn" disabled={scanning} onClick={onRescan}>
            <RefreshCw size={14} className={scanning ? 'spin' : undefined} />
            {mode === 'characters' ? (scanning ? '正在识别…' : '重新识别') : (scanning ? '正在扫描…' : '重新扫描')}
          </button>
        ) : <>
          {prefs.view === 'grid' ? (
            <input className="size-slider" type="range" min={TILE_MIN} max={TILE_MAX} step={4} value={prefs.tileSize}
              aria-label="缩略图大小" title="缩略图大小"
              onChange={event => onPrefs({ tileSize: Number(event.target.value) })} />
          ) : null}
          <div className="segmented" role="group" aria-label="显示方式">
            <button aria-pressed={prefs.view === 'grid'} title="图标（⌘1）" onClick={() => onPrefs({ view: 'grid' })}><LayoutGrid size={15} /></button>
            <button aria-pressed={prefs.view === 'list'} title="列表（⌘2）" onClick={() => onPrefs({ view: 'list' })}><List size={15} /></button>
          </div>
          {mode === 'browse' ? (
            <button className="tool-button" title="排序方式" aria-label="排序方式" onClick={event => onSortMenu(event.currentTarget.getBoundingClientRect())}><ArrowUpDown size={15} /></button>
          ) : null}
          <label className="search">
            <Search size={14} />
            <input ref={searchRef} type="search" value={query} placeholder="搜索名称或标签" aria-label="搜索"
              onChange={event => onQuery(event.target.value)}
              onKeyDown={event => { if (event.key === 'Escape') { event.preventDefault(); onQuery(''); event.currentTarget.blur(); } }} />
            {query ? <button className="search-clear" aria-label="清除搜索" onClick={() => onQuery('')}><X size={12} /></button> : null}
          </label>
          {mode === 'trash' ? (
            <button className="btn btn-danger-quiet" disabled={!canEmptyTrash} onClick={onEmptyTrash}><Trash2 size={14} />清空回收站</button>
          ) : (
            <button className="btn btn-primary" onClick={event => onImportMenu(event.currentTarget.getBoundingClientRect())}>
              <Plus size={15} />导入<ChevronDown size={13} className="btn-caret" />
            </button>
          )}
        </>}
        <button className={`tool-button${prefs.inspector ? ' on' : ''}`} title="显示简介（⌘I）" aria-label="显示简介" aria-pressed={prefs.inspector}
          onClick={() => onPrefs({ inspector: !prefs.inspector })}><PanelRight size={16} /></button>
      </div>
    </header>
  );
});
