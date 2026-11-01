import { useEffect, useRef } from 'react';
import { ChevronLeft, ChevronRight, Copy, Star, X } from 'lucide-react';
import { imageUrl } from '../api';
import { formatBytes, type Sticker } from '../types';
import { StickerPreview } from './StickerPreview';

/** Space-bar preview over the grid. Arrow keys walk the current list; Space or Esc closes. */
export function QuickLook({ item, root, position, total, favorite, onStep, onCopy, onFavorite, onClose }: {
  item: Sticker;
  root: string;
  position: number;
  total: number;
  favorite: boolean;
  onStep: (delta: number) => void;
  onCopy: () => void;
  onFavorite: () => void;
  onClose: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    dialog.current?.showModal();
    // Focus the sheet itself so Space/arrows never land on a button.
    dialog.current?.focus();
    return () => dialog.current?.close();
  }, []);
  return (
    <dialog
      ref={dialog}
      className="quicklook"
      tabIndex={-1}
      aria-label={`快速查看：${item.name}`}
      onCancel={event => { event.preventDefault(); onClose(); }}
      onMouseDown={event => { if (event.target === dialog.current) onClose(); }}
      onKeyDown={event => {
        if (event.key === ' ' ) { event.preventDefault(); onClose(); }
        else if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') { event.preventDefault(); onStep(-1); }
        else if (event.key === 'ArrowRight' || event.key === 'ArrowDown') { event.preventDefault(); onStep(1); }
        else if (event.key === 'c' && event.metaKey) { event.preventDefault(); onCopy(); }
      }}
    >
      <div className="quicklook-panel">
        <header className="quicklook-bar">
          <button className="icon-btn" aria-label="关闭" title="关闭（空格）" onClick={onClose}><X size={15} /></button>
          <div className="quicklook-title">
            <strong title={item.name}>{item.name}</strong>
            <span>{item.format.toUpperCase()} · {item.width}×{item.height} · {formatBytes(item.bytes)} · {position + 1}/{total}</span>
          </div>
          <button className={`icon-btn${favorite ? ' on-fav' : ''}`} aria-pressed={favorite} title={favorite ? '取消收藏' : '收藏'} onClick={onFavorite}><Star size={15} fill={favorite ? 'currentColor' : 'none'} /></button>
          <button className="btn btn-primary" onClick={onCopy}><Copy size={14} />复制</button>
        </header>
        <div className="quicklook-stage">
          <button className="quicklook-nav prev" aria-label="上一个" disabled={position <= 0} onClick={() => onStep(-1)}><ChevronLeft size={22} /></button>
          <StickerPreview key={item.id} src={imageUrl(root, item.fileName)} name={item.name} eager />
          <button className="quicklook-nav next" aria-label="下一个" disabled={position >= total - 1} onClick={() => onStep(1)}><ChevronRight size={22} /></button>
        </div>
      </div>
    </dialog>
  );
}
