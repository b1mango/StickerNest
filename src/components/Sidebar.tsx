import type { ReactNode } from 'react';
import { ChevronsUpDown, Clock, Folder, Images, Library, MessageCircle, Music2, Plus, ScanFace, ScanSearch, Star, Trash2 } from 'lucide-react';
import { sameNav, SOURCES, type LibraryIndex, type Nav } from '../lib/derive';

const SOURCE_ICONS: Record<string, ReactNode> = {
  微信: <MessageCircle size={16} />,
  抖音: <Music2 size={16} />,
  本地: <Folder size={16} />,
};

function Row({ icon, label, count, active, onSelect, onContextMenu, dropName, dropActive }: {
  icon: ReactNode;
  label: string;
  count?: number;
  active: boolean;
  onSelect: () => void;
  onContextMenu?: (event: React.MouseEvent) => void;
  dropName?: string;
  dropActive?: boolean;
}) {
  return (
    <button
      className={`side-row${active ? ' active' : ''}${dropActive ? ' drop' : ''}`}
      aria-current={active ? 'page' : undefined}
      onClick={onSelect}
      onContextMenu={onContextMenu}
      data-collection-drop={dropName}
      title={label}
    >
      <span className="side-icon">{icon}</span>
      <span className="side-label">{label}</span>
      {count ? <span className="side-count">{count}</span> : null}
    </button>
  );
}

export function Sidebar({ index, nav, libraryName, dropTarget, charactersEnabled, onNav, onNewCollection, onCollectionMenu, onLibraryMenu }: {
  index: LibraryIndex;
  nav: Nav;
  libraryName: string;
  dropTarget: string;
  charactersEnabled: boolean;
  onNav: (nav: Nav) => void;
  onNewCollection: () => void;
  onCollectionMenu: (name: string, event: React.MouseEvent) => void;
  onLibraryMenu: (anchor: DOMRect) => void;
}) {
  const is = (target: Nav) => sameNav(nav, target);
  return (
    <aside className="sidebar" aria-label="资料库导航">
      <div className="side-titlebar" data-window-drag-region />
      <nav className="side-scroll">
        <div className="side-section">
          <Row icon={<Images size={16} />} label="全部表情" count={index.counts.all} active={is({ kind: 'all' })} onSelect={() => onNav({ kind: 'all' })} />
          <Row icon={<Star size={16} />} label="收藏" count={index.counts.favorites} active={is({ kind: 'favorites' })} onSelect={() => onNav({ kind: 'favorites' })} />
          <Row icon={<Clock size={16} />} label="最近使用" count={index.counts.recent} active={is({ kind: 'recent' })} onSelect={() => onNav({ kind: 'recent' })} />
        </div>

        {SOURCES.some(source => index.counts.sources[source]) ? <div className="side-section">
          <p className="side-heading">来源</p>
          {SOURCES.filter(source => index.counts.sources[source]).map(source => (
            <Row key={source} icon={SOURCE_ICONS[source]} label={source === '本地' ? '本地导入' : source}
              count={index.counts.sources[source] ?? 0}
              active={is({ kind: 'source', value: source })}
              onSelect={() => onNav({ kind: 'source', value: source })} />
          ))}
        </div> : null}

        <div className="side-section">
          <div className="side-heading-row">
            <p className="side-heading">合集</p>
            <button className="side-add" onClick={onNewCollection} aria-label="新建合集" title="新建合集"><Plus size={14} /></button>
          </div>
          {index.collections.length ? index.collections.map(([name, count]) => (
            <Row key={name} icon={<Library size={16} />} label={name} count={count}
              active={is({ kind: 'collection', value: name })}
              onSelect={() => onNav({ kind: 'collection', value: name })}
              onContextMenu={event => onCollectionMenu(name, event)}
              dropName={name} dropActive={dropTarget === name} />
          )) : <p className="side-empty">用合集按主题整理表情，例如「日常」「工作」。</p>}
        </div>

        <div className="side-section">
          <p className="side-heading">整理</p>
          <Row icon={<ScanSearch size={16} />} label="相似项" active={is({ kind: 'similar' })} onSelect={() => onNav({ kind: 'similar' })} />
          {charactersEnabled ? <Row icon={<ScanFace size={16} />} label="形象识别" active={is({ kind: 'characters' })} onSelect={() => onNav({ kind: 'characters' })} /> : null}
          <Row icon={<Trash2 size={16} />} label="回收站" count={index.counts.trash} active={is({ kind: 'trash' })} onSelect={() => onNav({ kind: 'trash' })} />
        </div>
      </nav>

      <button className="library-switch" onClick={event => onLibraryMenu(event.currentTarget.getBoundingClientRect())} title="资料库操作">
        <img className="library-glyph" src="/icon.png" alt="" draggable={false} />
        <span className="library-text">
          <span className="library-name">{libraryName}</span>
          <span className="library-meta">{index.counts.all} 个表情</span>
        </span>
        <ChevronsUpDown size={14} className="library-chevron" />
      </button>
    </aside>
  );
}
