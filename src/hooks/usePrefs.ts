import { useCallback, useEffect, useState } from 'react';
import type { SortKey } from '../lib/derive';

export type ThemePref = 'system' | 'light' | 'dark';
/** How animated stickers play in the grid; originals are heavy, so hover is the default. */
export type AnimatePref = 'hover' | 'always' | 'never';

export interface Prefs {
  view: 'grid' | 'list';
  tileSize: number;
  sort: SortKey;
  inspector: boolean;
  theme: ThemePref;
  animate: AnimatePref;
}

const KEY = 'stickernest.prefs.v2';
export const TILE_MIN = 72;
export const TILE_MAX = 200;
const DEFAULTS: Prefs = { view: 'grid', tileSize: 112, sort: 'newest', inspector: true, theme: 'system', animate: 'hover' };

function load(): Prefs {
  try {
    const stored = JSON.parse(localStorage.getItem(KEY) ?? '{}') as Partial<Prefs>;
    const prefs = { ...DEFAULTS, ...stored };
    prefs.tileSize = Math.min(TILE_MAX, Math.max(TILE_MIN, Number(prefs.tileSize) || DEFAULTS.tileSize));
    if (!['system', 'light', 'dark'].includes(prefs.theme)) prefs.theme = DEFAULTS.theme;
    if (!['hover', 'always', 'never'].includes(prefs.animate)) prefs.animate = DEFAULTS.animate;
    return prefs;
  } catch {
    return DEFAULTS;
  }
}

export function usePrefs() {
  const [prefs, setPrefs] = useState(load);
  useEffect(() => { localStorage.setItem(KEY, JSON.stringify(prefs)); }, [prefs]);
  const update = useCallback((patch: Partial<Prefs>) => setPrefs(current => ({ ...current, ...patch })), []);
  return [prefs, update] as const;
}
