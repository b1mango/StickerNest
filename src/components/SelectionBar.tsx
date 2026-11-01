import { Copy, FolderPlus, Download, RotateCcw, Trash2, X } from 'lucide-react';

/** Floating bar for multi-selection; mirrors the inspector so it works with the inspector hidden. */
export function SelectionBar({ count, inTrash, onCopy, onCollect, onExport, onTrash, onRestore, onDelete, onClear }: {
  count: number;
  inTrash: boolean;
  onCopy: () => void;
  onCollect: (anchor: DOMRect) => void;
  onExport: () => void;
  onTrash: () => void;
  onRestore: () => void;
  onDelete: () => void;
  onClear: () => void;
}) {
  return (
    <div className="selection-bar" role="toolbar" aria-label="批量操作">
      <span className="selection-count">已选 {count} 项</span>
      <span className="selection-sep" />
      {inTrash ? <>
        <button onClick={onRestore}><RotateCcw size={14} />恢复</button>
        <button className="danger" onClick={onDelete}><Trash2 size={14} />彻底删除</button>
      </> : <>
        <button onClick={onCopy}><Copy size={14} />复制</button>
        <button onClick={event => onCollect(event.currentTarget.getBoundingClientRect())}><FolderPlus size={14} />加入合集</button>
        <button onClick={onExport}><Download size={14} />导出</button>
        <button onClick={onTrash}><Trash2 size={14} />移入回收站</button>
      </>}
      <button className="selection-close" aria-label="取消选择" title="取消选择（Esc）" onClick={onClear}><X size={14} /></button>
    </div>
  );
}
