import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { Layers, Star } from 'lucide-react';
import { imageUrl, thumbUrl } from '../api';
import { shortDisplayName, type LibraryIndex } from '../lib/derive';
import { GRID_PAD, gridGeometry, gridHits, LIST_HEAD, LIST_ROW, listHits, tileBox, type Rect } from '../lib/geometry';
import { clickSelect, inOrder, type Selection } from '../lib/selection';
import { formatBytes, type Sticker } from '../types';
import { StickerPreview } from './StickerPreview';

const OVERSCAN = 2;
const DRAG_THRESHOLD = 4;
const EDGE = 36;
const ANIMATABLE = new Set(['gif', 'webp']);

function formatDate(seconds: number) {
  return new Date(seconds * 1000).toLocaleDateString('zh-CN', { year: 'numeric', month: '2-digit', day: '2-digit' });
}

type Pointer =
  | { kind: 'tile'; id: string; x: number; y: number; collapse: boolean }
  | { kind: 'marquee'; originX: number; originY: number; clientX: number; clientY: number; base: Set<string>; moved: boolean; additive: boolean };

export function StickerGrid({ items, root, index, view, tileSize, thumbVersion, selection, resetKey, empty, onSelection, onColumns, onOpen, onContextMenu, onDragOut }: {
  items: Sticker[];
  root: string;
  index: LibraryIndex;
  view: 'grid' | 'list';
  tileSize: number;
  /** Bumped when thumbnails finish generating so tiles that fell back to originals retry. */
  thumbVersion: number;
  selection: Selection;
  /** Changing this scrolls back to the top (new view, new search). */
  resetKey: string;
  empty: ReactNode;
  onSelection: (selection: Selection) => void;
  onColumns: (columns: number) => void;
  onOpen: (id: string) => void;
  onContextMenu: (id: string | null, event: React.MouseEvent) => void;
  onDragOut: (ids: string[]) => void;
}) {
  const scroller = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ width: 0, height: 0 });
  const [scrollTop, setScrollTop] = useState(0);
  const [marquee, setMarquee] = useState<Rect | null>(null);
  // Animated originals only play under the pointer; everything else shows its small static thumbnail.
  const [hovered, setHovered] = useState('');
  const pointer = useRef<Pointer | null>(null);
  const latest = useRef({ selection, items, onSelection });
  latest.current = { selection, items, onSelection };

  useLayoutEffect(() => {
    const element = scroller.current;
    if (!element) return;
    const observer = new ResizeObserver(() => setSize({ width: element.clientWidth, height: element.clientHeight }));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (scroller.current) scroller.current.scrollTop = 0;
    setScrollTop(0);
  }, [resetKey]);

  const geometry = gridGeometry(size.width, tileSize, items.length);
  const isGrid = view === 'grid';
  const columns = isGrid ? geometry.columns : 1;
  const contentHeight = isGrid ? geometry.height : items.length ? LIST_HEAD + items.length * LIST_ROW : 0;
  useEffect(() => { onColumns(columns); }, [columns, onColumns]);

  // Keep the keyboard cursor visible.
  useEffect(() => {
    const element = scroller.current;
    const at = items.findIndex(item => item.id === selection.anchor);
    if (!element || at < 0) return;
    const top = isGrid ? tileBox(geometry, at).y : LIST_HEAD + at * LIST_ROW;
    const height = isGrid ? geometry.cell : LIST_ROW;
    const headroom = isGrid ? GRID_PAD : LIST_HEAD;
    if (top - headroom < element.scrollTop) element.scrollTop = top - headroom;
    else if (top + height + GRID_PAD > element.scrollTop + element.clientHeight) element.scrollTop = top + height + GRID_PAD - element.clientHeight;
  }, [selection.anchor, view, size.width, tileSize]);

  function contentPoint(clientX: number, clientY: number) {
    const box = scroller.current!.getBoundingClientRect();
    return { x: clientX - box.left, y: clientY - box.top + scroller.current!.scrollTop };
  }

  function hitsFor(rect: Rect): string[] {
    const list = latest.current.items;
    const found = isGrid ? gridHits(geometry, list.length, rect) : listHits(list.length, rect);
    return found.map(i => list[i].id);
  }

  function updateMarquee() {
    const state = pointer.current;
    if (state?.kind !== 'marquee') return;
    const here = contentPoint(state.clientX, state.clientY);
    const rect = {
      left: Math.min(state.originX, here.x), right: Math.max(state.originX, here.x),
      top: Math.min(state.originY, here.y), bottom: Math.max(state.originY, here.y),
    };
    setMarquee(rect);
    const hits = hitsFor(rect);
    latest.current.onSelection({ ids: new Set([...state.base, ...hits]), anchor: hits[hits.length - 1] ?? latest.current.selection.anchor });
  }

  // Marquee auto-scroll: while the pointer sits near an edge, keep scrolling and re-hit-testing.
  useEffect(() => {
    if (!marquee) return;
    let frame = 0;
    const tick = () => {
      const state = pointer.current;
      const element = scroller.current;
      if (state?.kind === 'marquee' && element) {
        const box = element.getBoundingClientRect();
        const delta = state.clientY < box.top + EDGE ? -(box.top + EDGE - state.clientY) / 2
          : state.clientY > box.bottom - EDGE ? (state.clientY - (box.bottom - EDGE)) / 2 : 0;
        if (delta) { element.scrollTop += Math.max(-24, Math.min(24, delta)); updateMarquee(); }
      }
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [marquee !== null]);

  function onMouseDown(event: React.MouseEvent) {
    if (event.button !== 0) return;
    const { selection: current, items: list, onSelection: select } = latest.current;
    const order = list.map(item => item.id);
    const tile = (event.target as Element).closest<HTMLElement>('[data-id]');
    if (tile) {
      const id = tile.dataset.id!;
      const mods = { meta: event.metaKey, shift: event.shiftKey };
      let collapse = false;
      if (mods.meta || mods.shift) select(clickSelect(current, order, id, mods));
      else if (!current.ids.has(id)) select(clickSelect(current, order, id, mods));
      // A plain press on an already-selected tile keeps the group so it can be dragged together.
      else collapse = current.ids.size > 1;
      pointer.current = { kind: 'tile', id, x: event.clientX, y: event.clientY, collapse };
    } else {
      if ((event.target as Element).closest('.list-head')) return;
      event.preventDefault();
      const start = contentPoint(event.clientX, event.clientY);
      const additive = event.metaKey || event.shiftKey;
      pointer.current = {
        kind: 'marquee', originX: start.x, originY: start.y, clientX: event.clientX, clientY: event.clientY,
        base: additive ? new Set(current.ids) : new Set(), moved: false, additive,
      };
    }

    const move = (moveEvent: MouseEvent) => {
      const state = pointer.current;
      if (!state) return;
      if (state.kind === 'tile') {
        if (Math.hypot(moveEvent.clientX - state.x, moveEvent.clientY - state.y) < DRAG_THRESHOLD) return;
        const chosen = latest.current.selection.ids;
        const ids = chosen.has(state.id) ? inOrder(chosen, latest.current.items.map(item => item.id)) : [state.id];
        finish();
        onDragOut(ids);
        return;
      }
      state.clientX = moveEvent.clientX;
      state.clientY = moveEvent.clientY;
      if (!state.moved && Math.hypot(moveEvent.clientX - event.clientX, moveEvent.clientY - event.clientY) < DRAG_THRESHOLD) return;
      state.moved = true;
      updateMarquee();
    };
    const up = () => {
      const state = pointer.current;
      if (state?.kind === 'tile' && state.collapse) latest.current.onSelection({ ids: new Set([state.id]), anchor: state.id });
      if (state?.kind === 'marquee' && !state.moved && !state.additive) latest.current.onSelection({ ids: new Set(), anchor: '' });
      finish();
    };
    const finish = () => {
      pointer.current = null;
      setMarquee(null);
      window.removeEventListener('mousemove', move);
      window.removeEventListener('mouseup', up);
    };
    window.addEventListener('mousemove', move);
    window.addEventListener('mouseup', up);
  }

  function onDoubleClick(event: React.MouseEvent) {
    const tile = (event.target as Element).closest<HTMLElement>('[data-id]');
    if (tile) onOpen(tile.dataset.id!);
  }

  function onContext(event: React.MouseEvent) {
    event.preventDefault();
    const tile = (event.target as Element).closest<HTMLElement>('[data-id]');
    onContextMenu(tile?.dataset.id ?? null, event);
  }

  const viewTop = scrollTop;
  const viewBottom = scrollTop + size.height;
  let first = 0;
  let last = -1;
  if (isGrid) {
    first = Math.max(0, (Math.floor((viewTop - GRID_PAD) / geometry.stride) - OVERSCAN) * geometry.columns);
    last = Math.min(items.length - 1, (Math.floor((viewBottom - GRID_PAD) / geometry.stride) + OVERSCAN + 1) * geometry.columns - 1);
  } else {
    first = Math.max(0, Math.floor((viewTop - LIST_HEAD) / LIST_ROW) - OVERSCAN * 4);
    last = Math.min(items.length - 1, Math.floor((viewBottom - LIST_HEAD) / LIST_ROW) + OVERSCAN * 4);
  }
  const visible = size.width ? items.slice(first, last + 1) : [];

  return (
    <div
      ref={scroller}
      className={`grid-scroller${marquee ? ' selecting' : ''}`}
      role="listbox"
      aria-multiselectable="true"
      aria-label="表情"
      onScroll={event => setScrollTop(event.currentTarget.scrollTop)}
      onMouseOver={event => setHovered((event.target as Element).closest<HTMLElement>('[data-id]')?.dataset.id ?? '')}
      onMouseLeave={() => setHovered('')}
      onMouseDown={onMouseDown}
      onDoubleClick={onDoubleClick}
      onContextMenu={onContext}
    >
      {items.length ? (
        <div className="grid-sizer" style={{ height: contentHeight }}>
          {isGrid ? null : (
            <div className="list-head list-cols" role="presentation">
              <span /><span>名称</span><span>来源</span><span>格式</span><span>尺寸</span><span>大小</span><span>导入时间</span>
            </div>
          )}
          {visible.map((item, offset) => {
            const at = first + offset;
            const selected = selection.ids.has(item.id);
            const group = index.groupByMain.get(item.id);
            const favorite = !!index.favorites[item.id];
            const label = item.name;
            if (isGrid) {
              const box = tileBox(geometry, at);
              return (
                <div key={item.id} data-id={item.id} role="option" aria-selected={selected} title={label}
                  className={`tile${selected ? ' selected' : ''}${group ? ' stacked' : ''}`}
                  style={{ transform: `translate(${box.x}px, ${box.y}px)`, width: box.size, height: box.size }}>
                  <div className="tile-frame">
                    {hovered === item.id && ANIMATABLE.has(item.format)
                      ? <StickerPreview src={imageUrl(root, item.fileName)} name={label} eager />
                      : <StickerPreview key={thumbVersion} src={thumbUrl(root, item.id, thumbVersion)} fallback={imageUrl(root, item.fileName)} name={label} eager />}
                  </div>
                  {favorite ? <span className="tile-fav" aria-label="已收藏"><Star size={11} fill="currentColor" /></span> : null}
                  <span className="tile-badges">
                    {group ? <span className="tile-badge" title={`${group.memberIds.length} 个版本`}><Layers size={10} />{group.memberIds.length}</span> : null}
                    {item.format === 'gif' ? <span className="tile-badge">GIF</span> : null}
                  </span>
                </div>
              );
            }
            return (
              <div key={item.id} data-id={item.id} role="option" aria-selected={selected}
                className={`list-row list-cols${selected ? ' selected' : ''}${at % 2 ? ' odd' : ''}`}
                style={{ transform: `translateY(${LIST_HEAD + at * LIST_ROW}px)` }}>
                <span className="list-thumb"><StickerPreview key={thumbVersion} src={thumbUrl(root, item.id, thumbVersion)} fallback={imageUrl(root, item.fileName)} name={label} eager /></span>
                <span className="list-name" title={label}>
                  {shortDisplayName(label)}
                  {favorite ? <Star size={11} fill="currentColor" className="list-fav" /> : null}
                  {group ? <span className="tile-badge inline"><Layers size={10} />{group.memberIds.length}</span> : null}
                </span>
                <span className="list-dim">{item.sources.join('、')}</span>
                <span className="list-dim">{item.format.toUpperCase()}</span>
                <span className="list-dim">{item.width}×{item.height}</span>
                <span className="list-dim">{formatBytes(item.bytes)}</span>
                <span className="list-dim">{formatDate(item.importedAt)}</span>
              </div>
            );
          })}
          {marquee ? <div className="marquee" style={{ left: marquee.left, top: marquee.top, width: marquee.right - marquee.left, height: marquee.bottom - marquee.top }} /> : null}
        </div>
      ) : empty}
    </div>
  );
}
