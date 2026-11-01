import { useCallback, useRef, useState } from 'react';

export type ToastKind = 'success' | 'error' | 'info' | 'progress';

export interface Toast {
  id: number;
  kind: ToastKind;
  title: string;
  detail?: string;
  action?: { label: string; run: () => void };
}

const AUTO_DISMISS_MS = 4200;

export function useToasts() {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const nextId = useRef(1);
  const timers = useRef(new Map<number, number>());

  const dismiss = useCallback((id: number) => {
    window.clearTimeout(timers.current.get(id));
    timers.current.delete(id);
    setToasts(current => current.filter(toast => toast.id !== id));
  }, []);

  const push = useCallback((toast: Omit<Toast, 'id'>) => {
    const id = nextId.current++;
    // Keep the stack short: the oldest settled toast gives way.
    setToasts(current => {
      const settled = current.filter(item => item.kind === 'success' || item.kind === 'info');
      const drop = new Set(settled.slice(0, Math.max(0, settled.length - 2)).map(item => item.id));
      return [...current.filter(item => !drop.has(item.id)), { ...toast, id }];
    });
    // Errors and progress stay until handled; everything else fades on its own.
    if (toast.kind === 'success' || toast.kind === 'info') {
      timers.current.set(id, window.setTimeout(() => dismiss(id), toast.action ? AUTO_DISMISS_MS * 1.6 : AUTO_DISMISS_MS));
    }
    return id;
  }, [dismiss]);

  return { toasts, push, dismiss };
}

export type ToastApi = ReturnType<typeof useToasts>;
