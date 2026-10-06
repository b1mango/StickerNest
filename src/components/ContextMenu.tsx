import { useEffect, useRef } from 'react';
import { ChevronRight, X } from 'lucide-react';

export interface ContextMenuItem {
  key: string;
  label: string;
  danger?: boolean;
  disabled?: boolean;
  onSelect?: () => void;
  /** One level of hover submenu, for "copy/move to collection" style picks. */
  children?: ContextMenuItem[];
}

const WIDTH = 240;
const ITEM_HEIGHT = 32;

function SubItems({ items, onClose }: { items: ContextMenuItem[]; onClose: () => void }) {
  return <>
    {items.map(item => {
      if (item.key.startsWith('sep')) return <div key={item.key} className="context-sep" role="separator" />;
      if (item.children?.length) {
        return <div className="context-item-wrap" key={item.key}>
          <button role="menuitem" aria-haspopup="menu" className="context-item" disabled={item.disabled} onClick={() => { item.onSelect?.(); if (item.onSelect) onClose(); }}>
            <span>{item.label}</span><ChevronRight size={13} />
          </button>
          <div className="context-sub" role="menu"><SubItems items={item.children} onClose={onClose} /></div>
        </div>;
      }
      return <button key={item.key} role="menuitem" className={`context-item${item.danger ? ' danger' : ''}`} disabled={item.disabled} onClick={() => { item.onSelect?.(); onClose(); }}>{item.label}</button>;
    })}
  </>;
}

export function ContextMenu({ x, y, items, onClose }: {
  x: number;
  y: number;
  items: ContextMenuItem[];
  onClose: () => void;
}) {
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const handler = (event: MouseEvent | KeyboardEvent) => {
      if (event instanceof KeyboardEvent && event.key !== 'Escape') return;
      if (event instanceof MouseEvent && root.current?.contains(event.target as Node)) return;
      onClose();
    };
    // Capture phase: the click that opened this menu is still bubbling when the
    // listener attaches; capture listeners added mid-dispatch only see future events.
    document.addEventListener('mousedown', handler as EventListener, true);
    document.addEventListener('keydown', handler as EventListener);
    document.addEventListener('contextmenu', handler as EventListener, true);
    return () => {
      document.removeEventListener('mousedown', handler as EventListener, true);
      document.removeEventListener('keydown', handler as EventListener);
      document.removeEventListener('contextmenu', handler as EventListener, true);
    };
  }, [onClose]);

  const totalHeight = items.length * ITEM_HEIGHT + items.filter(i => i.key.startsWith('sep')).length * 10 + 44;
  let left = x;
  let top = y;
  if (left + WIDTH > window.innerWidth - 12) left = window.innerWidth - WIDTH - 12;
  if (top + totalHeight > window.innerHeight - 12) top = Math.max(12, window.innerHeight - totalHeight - 12);
  if (left < 12) left = 12;
  // Submenus flip to the left when there is no room on the right.
  const flip = left + WIDTH + WIDTH > window.innerWidth - 12;

  return (
    <div ref={root} className={`context-menu${flip ? ' sub-left' : ''}`} role="menu" style={{ left, top }}>
      <div className="context-menu-head">
        <button className="icon-button slim" aria-label="关闭菜单" onClick={onClose}><X size={13} /></button>
      </div>
      <SubItems items={items} onClose={onClose} />
    </div>
  );
}
