import { AlertCircle, CheckCircle2, Info, Loader2, X } from 'lucide-react';
import type { Toast } from '../hooks/useToasts';

const ICONS = {
  success: <CheckCircle2 size={16} />,
  error: <AlertCircle size={16} />,
  info: <Info size={16} />,
  progress: <Loader2 size={16} className="spin" />,
};

export function Toasts({ toasts, onDismiss }: { toasts: Toast[]; onDismiss: (id: number) => void }) {
  return (
    <div className="toasts" aria-live="polite">
      {toasts.map(toast => (
        <div key={toast.id} className={`toast ${toast.kind}`} role={toast.kind === 'error' ? 'alert' : 'status'}>
          <span className="toast-icon">{ICONS[toast.kind]}</span>
          <div className="toast-text">
            <strong>{toast.title}</strong>
            {toast.detail ? <p>{toast.detail}</p> : null}
          </div>
          {toast.action ? (
            <button className="toast-action" onClick={() => { toast.action!.run(); onDismiss(toast.id); }}>{toast.action.label}</button>
          ) : null}
          {toast.kind === 'progress' ? null : (
            <button className="toast-close" aria-label="关闭" onClick={() => onDismiss(toast.id)}><X size={12} /></button>
          )}
        </div>
      ))}
    </div>
  );
}
