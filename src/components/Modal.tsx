import { useEffect, useRef, useState, type ReactNode } from 'react';
import { errorText } from '../hooks/useLibrary';

/** Native modal sheet. Closing (Esc, backdrop, buttons) always routes through onClose. */
export function Modal({ title, children, onClose, busy = false, width = 400, className = '' }: {
  title: string;
  children: ReactNode;
  onClose: () => void;
  busy?: boolean;
  width?: number;
  className?: string;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const trigger = document.activeElement as HTMLElement | null;
    dialog.current?.showModal();
    return () => { dialog.current?.close(); trigger?.focus?.(); };
  }, []);
  return (
    <dialog
      ref={dialog}
      className={`sheet ${className}`}
      style={{ width }}
      aria-label={title}
      onCancel={event => { event.preventDefault(); if (!busy) onClose(); }}
      onMouseDown={event => { if (event.target === dialog.current && !busy) onClose(); }}
    >
      <div className="sheet-body">
        <h2 className="sheet-title">{title}</h2>
        {children}
      </div>
    </dialog>
  );
}

/** Runs an async submit, keeping the dialog open with an inline error when it fails. */
function useSubmit(onDone: () => void) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  async function submit(action: () => Promise<void>) {
    if (busy) return;
    setBusy(true); setError('');
    try { await action(); onDone(); }
    catch (reason) { setError(errorText(reason)); setBusy(false); }
  }
  return { busy, error, setError, submit };
}

export function PromptDialog({ title, label, initial = '', placeholder, confirmLabel = '好', validate, onSubmit, onClose }: {
  title: string;
  label?: string;
  initial?: string;
  placeholder?: string;
  confirmLabel?: string;
  validate?: (value: string) => string;
  onSubmit: (value: string) => Promise<void>;
  onClose: () => void;
}) {
  const [value, setValue] = useState(initial);
  const { busy, error, setError, submit } = useSubmit(onClose);
  return (
    <Modal title={title} onClose={onClose} busy={busy}>
      <form onSubmit={event => {
        event.preventDefault();
        const problem = validate?.(value) ?? '';
        if (problem) { setError(problem); return; }
        void submit(() => onSubmit(value.trim()));
      }}>
        <label className="field">
          {label ? <span className="field-name">{label}</span> : null}
          <input className="input" value={value} placeholder={placeholder} autoFocus disabled={busy}
            onFocus={event => event.currentTarget.select()}
            onChange={event => { setValue(event.target.value); setError(''); }} />
        </label>
        {error ? <p className="field-error" role="alert">{error}</p> : null}
        <div className="sheet-actions">
          <button type="button" className="btn" disabled={busy} onClick={onClose}>取消</button>
          <button type="submit" className="btn btn-primary" disabled={busy || !value.trim()}>{confirmLabel}</button>
        </div>
      </form>
    </Modal>
  );
}

export function ConfirmDialog({ title, message, confirmLabel, danger = false, onConfirm, onClose }: {
  title: string;
  message: ReactNode;
  confirmLabel: string;
  danger?: boolean;
  onConfirm: () => Promise<void> | void;
  onClose: () => void;
}) {
  const { busy, error, submit } = useSubmit(onClose);
  return (
    <Modal title={title} onClose={onClose} busy={busy}>
      <div className="sheet-message">{message}</div>
      {error ? <p className="field-error" role="alert">{error}</p> : null}
      <div className="sheet-actions">
        <button type="button" className="btn" autoFocus disabled={busy} onClick={onClose}>取消</button>
        <button type="button" className={`btn ${danger ? 'btn-danger' : 'btn-primary'}`} disabled={busy}
          onClick={() => void submit(async () => { await onConfirm(); })}>{confirmLabel}</button>
      </div>
    </Modal>
  );
}

export function BatchRenameDialog({ count, onSubmit, onClose }: {
  count: number;
  onSubmit: (prefix: string, start: number) => Promise<void>;
  onClose: () => void;
}) {
  const [prefix, setPrefix] = useState('');
  const [start, setStart] = useState('1');
  const { busy, error, setError, submit } = useSubmit(onClose);
  const first = Math.max(1, Number.parseInt(start, 10) || 1);
  // Mirrors the backend: `prefix-001`, numbered in display order.
  const sample = (n: number) => `${prefix.trim()}-${String(n).padStart(3, '0')}`;
  return (
    <Modal title={`重命名 ${count} 个表情`} onClose={onClose} busy={busy}>
      <form onSubmit={event => {
        event.preventDefault();
        if (!prefix.trim()) { setError('请输入名称前缀。'); return; }
        void submit(() => onSubmit(prefix.trim(), first));
      }}>
        <div className="field-row">
          <label className="field grow">
            <span className="field-name">名称前缀</span>
            <input className="input" value={prefix} autoFocus disabled={busy} placeholder="例如：猫猫"
              onChange={event => { setPrefix(event.target.value); setError(''); }} />
          </label>
          <label className="field narrow">
            <span className="field-name">起始编号</span>
            <input className="input" inputMode="numeric" value={start} disabled={busy}
              onChange={event => setStart(event.target.value.replace(/\D/g, '').slice(0, 6))} />
          </label>
        </div>
        <p className="field-hint">按当前排列顺序命名：{prefix.trim() ? `${sample(first)}、${sample(first + 1)}${count > 2 ? ' …' : ''}` : '输入前缀后预览'}</p>
        {error ? <p className="field-error" role="alert">{error}</p> : null}
        <div className="sheet-actions">
          <button type="button" className="btn" disabled={busy} onClick={onClose}>取消</button>
          <button type="submit" className="btn btn-primary" disabled={busy || !prefix.trim()}>重命名</button>
        </div>
      </form>
    </Modal>
  );
}
