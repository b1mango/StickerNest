import { useEffect, useRef } from 'react';
import { ArchiveRestore, FolderOpen, FolderPlus, HardDriveDownload, MessageCircle, Music2 } from 'lucide-react';

export function LibraryMenu({ disabled, onOpen, onCreate, onBackup, onRestore, onCollectDouyin, onImportWechat, onClose }: {
  disabled: boolean;
  onOpen: () => void;
  onCreate: () => void;
  onBackup: () => void;
  onRestore: () => void;
  onCollectDouyin: () => void;
  onImportWechat: () => void;
  onClose: () => void;
}) {
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const handle = (event: MouseEvent | KeyboardEvent) => {
      if (event instanceof KeyboardEvent && event.key !== 'Escape') return;
      if (event instanceof MouseEvent && root.current?.contains(event.target as Node)) return;
      onClose();
    };
    document.addEventListener('mousedown', handle as EventListener);
    document.addEventListener('keydown', handle as EventListener);
    return () => {
      document.removeEventListener('mousedown', handle as EventListener);
      document.removeEventListener('keydown', handle as EventListener);
    };
  }, [onClose]);
  const item = (icon: React.ReactNode, label: string, hint: string, onClick: () => void, key: string) => (
    <button key={key} className="menu-item" disabled={disabled} onClick={() => { onClick(); onClose(); }}>
      {icon}
      <span className="menu-item-text">
        <strong>{label}</strong>
        <span className="menu-item-hint">{hint}</span>
      </span>
    </button>
  );
  return (
    <div className="library-menu" ref={root} role="menu" aria-label="库动作">
      {item(<FolderOpen size={16} />, '打开资料库', '选择已有的 StickerNest 资料库', onOpen, 'open')}
      {item(<FolderPlus size={16} />, '新建资料库', '在本地新建一个空的资料库', onCreate, 'create')}
      <div className="menu-sep" role="separator" />
      {item(<Music2 size={16} />, '采集抖音收藏', '从网页登录的抖音把收藏表情收进来', onCollectDouyin, 'douyin')}
      {item(<MessageCircle size={16} />, '导入微信清单', '本机直接获取或使用 Windows 导出的清单', onImportWechat, 'wechat')}
      <div className="menu-sep" role="separator" />
      {item(<HardDriveDownload size={16} />, '备份这个库', '写一份带清单校验的完整备份', onBackup, 'backup')}
      {item(<ArchiveRestore size={16} />, '从备份恢复', '恢复到新位置并打开', onRestore, 'restore')}
    </div>
  );
}
