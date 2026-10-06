import { useEffect, useRef } from 'react';
import { X } from 'lucide-react';

export interface ContextMenuItem {
  key: string;
  label: string;
  danger?: boolean;
  disabled?: boolean;
  onSelect?: () => void;
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
    document.addEventListener('mousedown', handler as EventListener);
    document.addEventListener('keydown', handler as EventListener);
    document.addEventListener('contextmenu', handler as EventListener);
    return () => {
      document.removeEventListener('mousedown', handler as EventListener);
      document.removeEventListener('keydown', handler as EventListener);
      document.removeEventListener('contextmenu', handler as EventListener);
    };
  }, [onClose]);

  const width = 220;
  const itemHeight = 36;
  const totalHeight = items.length * itemHeight + items.filter(i => i.key.startsWith('sep')).length * 10 + 16;
  let left = x;
  let top = y;
  if (left + width > window.innerWidth - 12) left = window.innerWidth - width - 12;
  if (top + totalHeight > window.innerHeight - 12) top = window.innerHeight - totalHeight - 12;
  if (left < 12) left = 12;
  if (top < 12) top = 12;

  return (
    <div ref={root} className="context-menu" role="menu" style={{ left, top }}>
      <div className="context-menu-head">
        <button className="icon-button slim" aria-label="关闭菜单" onClick={onClose}><X size={13} /></button>
      </div>
      {items.map(item => {
        if (item.key.startsWith('sep')) return <div key={item.key} className="context-sep" role="separator" />;
        return <button key={item.key} role="menuitem" className={`context-item${item.danger ? ' danger' : ''}`} disabled={item.disabled} onClick={() => { item.onSelect?.(); onClose(); }}>{item.label}</button>;
      })}
    </div>
  );
}
