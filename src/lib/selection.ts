/** Finder-style selection arithmetic, kept pure so it can be tested without a DOM. */

export interface Selection {
  ids: Set<string>;
  /** Where Shift-ranges start; also the item keyboard arrows move from. */
  anchor: string;
}

export const emptySelection = (): Selection => ({ ids: new Set(), anchor: '' });

export interface ClickModifiers { meta: boolean; shift: boolean }

export function clickSelect(current: Selection, order: string[], id: string, mods: ClickModifiers): Selection {
  if (mods.shift && current.anchor && order.includes(current.anchor)) {
    const from = order.indexOf(current.anchor);
    const to = order.indexOf(id);
    const [lo, hi] = from < to ? [from, to] : [to, from];
    const range = order.slice(lo, hi + 1);
    const base = mods.meta ? current.ids : new Set<string>();
    return { ids: new Set([...base, ...range]), anchor: current.anchor };
  }
  if (mods.meta) {
    const ids = new Set(current.ids);
    if (ids.has(id)) ids.delete(id); else ids.add(id);
    return { ids, anchor: id };
  }
  return { ids: new Set([id]), anchor: id };
}

/** Arrow-key movement through a grid of `columns`; Shift extends from the anchor. */
export function moveSelect(current: Selection, order: string[], columns: number, key: string, shift: boolean): Selection | null {
  if (!order.length) return null;
  const step = key === 'ArrowLeft' ? -1 : key === 'ArrowRight' ? 1 : key === 'ArrowUp' ? -columns : key === 'ArrowDown' ? columns : 0;
  if (!step) return null;
  const cursor = current.anchor && order.includes(current.anchor) ? current.anchor : '';
  if (!cursor) return { ids: new Set([order[0]]), anchor: order[0] };
  const index = order.indexOf(cursor);
  const nextIndex = Math.min(order.length - 1, Math.max(0, index + step));
  const next = order[nextIndex];
  if (!shift) return { ids: new Set([next]), anchor: next };
  return { ids: new Set([...current.ids, next]), anchor: next };
}

export function inOrder(selection: Set<string>, order: string[]): string[] {
  return order.filter(id => selection.has(id));
}

/** Global shortcuts must stay out of text fields, open dialogs, IME composition and handled events. */
export function shouldHandleGlobalKey(event: {
  defaultPrevented: boolean; isComposing: boolean; keyCode: number;
  target: EventTarget | { closest?: (selector: string) => unknown } | null;
}, dialogOpen: boolean): boolean {
  if (event.defaultPrevented || event.isComposing || event.keyCode === 229) return false;
  if (dialogOpen) return false;
  const target = event.target as { closest?: (selector: string) => unknown } | null;
  const editable = target?.closest?.('input, textarea, select, [contenteditable]:not([contenteditable="false"]), [role="textbox"]');
  return !editable;
}
