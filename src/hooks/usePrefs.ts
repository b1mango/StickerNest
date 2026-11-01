import { useCallback, useEffect, useState } from 'react';
import type { SortKey } from '../lib/derive';

export interface Prefs {
  view: 'grid' | 'list';
  tileSize: number;
  sort: SortKey;
  inspector: boolean;
}

const KEY = 'stickernest.prefs.v2';
export const TILE_MIN = 72;
export const TILE_MAX = 200;
const DEFAULTS: Prefs = { view: 'grid', tileSize: 112, sort: 'newest', inspector: true };

function load(): Prefs {
  try {
    const stored = JSON.parse(localStorage.getItem(KEY) ?? '{}') as Partial<Prefs>;
    const prefs = { ...DEFAULTS, ...stored };
    prefs.tileSize = Math.min(TILE_MAX, Math.max(TILE_MIN, Number(prefs.tileSize) || DEFAULTS.tileSize));
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
