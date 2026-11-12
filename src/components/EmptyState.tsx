import type { ReactNode } from 'react';
import { ArchiveRestore, Clock, FolderOpen, FolderPlus } from 'lucide-react';

export function EmptyState({ icon, title, children, actions }: { icon: ReactNode; title: string; children?: ReactNode; actions?: ReactNode }) {
  return (
    <div className="empty">
      <span className="empty-icon">{icon}</span>
      <h2>{title}</h2>
      {children ? <p>{children}</p> : null}
      {actions ? <div className="empty-actions">{actions}</div> : null}
    </div>
  );
}

/** First run, or the remembered library is gone. */
export function Welcome({ busy, recent, onOpenRecent, onCreate, onOpen, onRestore }: {
  busy: boolean;
  recent: { path: string; name: string; place: string }[];
  onOpenRecent: (path: string) => void;
  onCreate: () => void;
  onOpen: () => void;
  onRestore: () => void;
}) {
  return (
    <main className="welcome" data-window-drag-region>
      <div className="welcome-card">
        <img src="/icon.png" alt="" className="welcome-icon" draggable={false} />
        <h1>拾趣</h1>
        <p>把微信、抖音里收藏的表情集中归档，随时挑出来再发到微信。</p>
        <div className="welcome-actions">
          {recent.map(item => (
            <button key={item.path} className="welcome-option" disabled={busy} onClick={() => onOpenRecent(item.path)} title={item.path}>
              <Clock size={20} /><span><strong>{item.name}</strong><small>最近打开 · {item.place}</small></span>
            </button>
          ))}
          <button className="welcome-option" disabled={busy} onClick={onCreate}>
            <FolderPlus size={20} /><span><strong>新建资料库</strong><small>选择一个文件夹存放表情</small></span>
          </button>
          <button className="welcome-option" disabled={busy} onClick={onOpen}>
            <FolderOpen size={20} /><span><strong>打开资料库</strong><small>选择已有的资料库文件夹</small></span>
          </button>
          <button className="welcome-option" disabled={busy} onClick={onRestore}>
            <ArchiveRestore size={20} /><span><strong>从备份恢复</strong><small>校验备份后在新位置打开</small></span>
          </button>
        </div>
      </div>
    </main>
  );
}
