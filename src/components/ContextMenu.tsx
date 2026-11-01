import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { Check, ChevronRight } from 'lucide-react';

export interface ContextMenuItem {
  key: string;
  label: string;
  danger?: boolean;
  disabled?: boolean;
  /** Keyboard shortcut shown on the right, e.g. "⌘C". */
  hint?: string;
  /** Radio-style checkmark (sort order and similar picks). */
  checked?: boolean;
  onSelect?: () => void;
  /** One level of hover submenu, for "copy/move to collection" style picks. */
  children?: ContextMenuItem[];
}

const WIDTH = 220;
const MARGIN = 12;

function SubItems({ items, onClose }: { items: ContextMenuItem[]; onClose: () => void }) {
  return <>
    {items.map(item => {
      if (item.key.startsWith('sep')) return <div key={item.key} className="context-sep" role="separator" />;
      if (item.children?.length) {
        return <div className="context-item-wrap" key={item.key}>
          <button role="menuitem" aria-haspopup="menu" className="context-item" disabled={item.disabled} onClick={() => { if (item.onSelect) { onClose(); item.onSelect(); } }}>
            <span>{item.label}</span><ChevronRight size={13} />
          </button>
          <div className="context-sub" role="menu"><SubItems items={item.children} onClose={onClose} /></div>
        </div>;
      }
      if (item.key.startsWith('head')) return <div key={item.key} className="context-head">{item.label}</div>;
      return <button key={item.key} role={item.checked === undefined ? 'menuitem' : 'menuitemradio'} aria-checked={item.checked}
        className={`context-item${item.danger ? ' danger' : ''}`} disabled={item.disabled}
        onClick={() => { onClose(); item.onSelect?.(); }}>
        {item.checked === undefined ? null : <span className="context-check">{item.checked ? <Check size={12} /> : null}</span>}
        <span>{item.label}</span>{item.hint ? <kbd className="context-hint">{item.hint}</kbd> : null}
      </button>;
    })}
  </>;
}

export function ContextMenu({ x, y, above = false, items, onClose }: {
  x: number;
  y: number;
  /** Open upward: `y` is where the menu's bottom edge goes. */
  above?: boolean;
  items: ContextMenuItem[];
  onClose: () => void;
}) {
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const handler = (event: MouseEvent | KeyboardEvent) => {
      if (event instanceof KeyboardEvent) {
        if (event.key !== 'Escape') return;
        // 拦截这次 Esc，避免 window 上的「清除选择」同拍触发
        event.preventDefault();
        onClose();
        return;
      }
      if (root.current?.contains(event.target as Node)) return;
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

  // Position from the rendered size so separators and future row styles never skew it.
  const [top, setTop] = useState<number | null>(null);
  useLayoutEffect(() => {
    const height = root.current?.offsetHeight ?? 0;
    const wanted = above ? y - height : y;
    setTop(Math.max(MARGIN, Math.min(wanted, window.innerHeight - height - MARGIN)));
  }, [y, above, items]);
  const left = Math.max(MARGIN, Math.min(x, window.innerWidth - WIDTH - MARGIN));
  // Submenus flip to the left when there is no room on the right.
  const flip = left + WIDTH + WIDTH > window.innerWidth - MARGIN;

  return (
    <div ref={root} className={`context-menu${flip ? ' sub-left' : ''}`} role="menu" style={{ left, top: top ?? y, visibility: top === null ? 'hidden' : undefined }}>
      <SubItems items={items} onClose={onClose} />
    </div>
  );
}
