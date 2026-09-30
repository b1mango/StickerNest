import { useEffect, useRef } from 'react';
import { X } from 'lucide-react';
import { imageUrl } from '../api';
import { formatBytes, SOURCE_LABELS, type Sticker } from '../types';
import { StickerPreview } from './StickerPreview';

export function StickerDetail({ item, root, onClose }: { item: Sticker; root: string; onClose: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const trigger = document.activeElement as HTMLElement | null;
    const element = dialog.current;
    element?.showModal();
    return () => {
      element?.close();
      trigger?.focus();
    };
  }, []);

  return (
    <dialog className="detail-dialog" ref={dialog} onClose={() => { if (!dialog.current?.open) onClose(); }} aria-labelledby="detail-title">
      <div className="detail-heading"><span className="eyebrow">素材详情</span><button className="icon-button" autoFocus onClick={() => dialog.current?.close()} aria-label="关闭详情"><X size={20} /></button></div>
      <div className="detail-preview"><StickerPreview src={imageUrl(root, item.fileName)} name={item.name} eager /></div>
      <div className="detail-content">
        <h2 id="detail-title">{item.name}</h2>
        <dl className="metadata">
          <div><dt>来源</dt><dd>{item.sources.map(source => SOURCE_LABELS[source] ?? source).join('、')}</dd></div>
          <div><dt>文件格式</dt><dd>{item.format.toUpperCase()}</dd></div>
          <div><dt>尺寸</dt><dd>{item.width} × {item.height} px</dd></div>
          <div><dt>文件大小</dt><dd>{formatBytes(item.bytes)}</dd></div>
          <div><dt>导入时间</dt><dd>{new Date(item.importedAt * 1000).toLocaleString('zh-CN')}</dd></div>
        </dl>
        <p className="detail-path">{root}/assets/{item.fileName}</p>
      </div>
    </dialog>
  );
}
