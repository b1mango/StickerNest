import type { ManagementSnapshot, Snapshot, Sticker, VersionGroup } from '../types';

/** Where the user is looking: one library scope plus optional filters on top. */
export type Nav =
  | { kind: 'all' }
  | { kind: 'favorites' }
  | { kind: 'recent' }
  | { kind: 'source'; value: string }
  | { kind: 'collection'; value: string }
  | { kind: 'similar' }
  | { kind: 'characters' }
  | { kind: 'trash' };

export type SortKey = 'newest' | 'oldest' | 'name' | 'size';

export const SORT_LABELS: Record<SortKey, string> = {
  newest: '最近导入',
  oldest: '最早导入',
  name: '名称',
  size: '文件大小',
};

export const SOURCES = ['微信', '抖音', '本地'] as const;

export function sameNav(a: Nav, b: Nav): boolean {
  return a.kind === b.kind && ('value' in a ? a.value : '') === ('value' in b ? b.value : '');
}

export interface LibraryIndex {
  /** Stickers with display names applied (custom name overrides the manifest name). */
  items: Sticker[];
  byId: Map<string, Sticker>;
  trashed: Set<string>;
  favorites: Record<string, number>;
  recent: Record<string, number>;
  groupByMain: Map<string, VersionGroup>;
  /** Non-main group members never show in the main grid; their main stands in for them. */
  hiddenMembers: Set<string>;
  collectionsOf: Map<string, string[]>;
  tagsOf: Map<string, string[]>;
  counts: { all: number; favorites: number; recent: number; trash: number; sources: Record<string, number> };
  collections: [string, number][];
}

export function buildIndex(library: Snapshot | null, management: ManagementSnapshot | null): LibraryIndex {
  const trashed = new Set(Object.keys(management?.trash ?? {}));
  const favorites = management?.favorites ?? {};
  const recent = management?.recent ?? {};
  const groups = management?.groups ?? [];
  const groupByMain = new Map(groups.map(group => [group.mainAssetId, group]));
  const hiddenMembers = new Set(groups.flatMap(group => group.memberIds.filter(id => id !== group.mainAssetId)));
  const items = (library?.items ?? []).map(item => {
    const custom = management?.metadata[item.id]?.name;
    return custom ? { ...item, name: custom } : item;
  });
  const byId = new Map(items.map(item => [item.id, item]));
  const collectionsOf = new Map<string, string[]>();
  const tagsOf = new Map<string, string[]>();
  const counts = { all: 0, favorites: 0, recent: 0, trash: 0, sources: {} as Record<string, number> };
  const collectionCounts = new Map<string, number>((management?.collections ?? []).map(name => [name, 0]));

  for (const item of items) {
    const meta = management?.metadata[item.id];
    const group = groupByMain.get(item.id);
    // A group's main answers for the whole group: its own labels plus group-level ones.
    const collections = [...new Set([...(meta?.collections ?? []), ...(group?.collections ?? [])])];
    const tags = [...new Set([...(meta?.tags ?? []), ...(group?.tags ?? [])])];
    collectionsOf.set(item.id, collections);
    tagsOf.set(item.id, tags);
    if (trashed.has(item.id)) { counts.trash += 1; continue; }
    if (hiddenMembers.has(item.id)) continue;
    counts.all += 1;
    if (favorites[item.id]) counts.favorites += 1;
    if (recent[item.id]) counts.recent += 1;
    for (const origin of item.sources) counts.sources[origin] = (counts.sources[origin] ?? 0) + 1;
    for (const name of collections) collectionCounts.set(name, (collectionCounts.get(name) ?? 0) + 1);
  }
  const collections = [...collectionCounts.entries()].sort((a, b) => a[0].localeCompare(b[0], 'zh'));
  return { items, byId, trashed, favorites, recent, groupByMain, hiddenMembers, collectionsOf, tagsOf, counts, collections };
}

export function filterItems(index: LibraryIndex, nav: Nav, query: string, sort: SortKey): Sticker[] {
  if (nav.kind === 'similar' || nav.kind === 'characters') return [];
  const search = query.trim().toLocaleLowerCase();
  const list = index.items.filter(item => {
    if (nav.kind === 'trash') {
      if (!index.trashed.has(item.id)) return false;
    } else {
      if (index.trashed.has(item.id) || index.hiddenMembers.has(item.id)) return false;
      if (nav.kind === 'favorites' && !index.favorites[item.id]) return false;
      if (nav.kind === 'recent' && !index.recent[item.id]) return false;
      if (nav.kind === 'source' && !item.sources.includes(nav.value)) return false;
      if (nav.kind === 'collection' && !index.collectionsOf.get(item.id)?.includes(nav.value)) return false;
    }
    if (!search) return true;
    return [item.name, ...(index.tagsOf.get(item.id) ?? [])].some(value => value.toLocaleLowerCase().includes(search));
  });
  // Favorites and recents have a natural order of their own.
  if (nav.kind === 'recent') return list.sort((a, b) => index.recent[b.id] - index.recent[a.id]);
  if (nav.kind === 'favorites' && sort === 'newest') return list.sort((a, b) => index.favorites[b.id] - index.favorites[a.id]);
  const compare: Record<SortKey, (a: Sticker, b: Sticker) => number> = {
    newest: (a, b) => b.importedAt - a.importedAt,
    oldest: (a, b) => a.importedAt - b.importedAt,
    name: (a, b) => a.name.localeCompare(b.name, 'zh'),
    size: (a, b) => b.bytes - a.bytes,
  };
  return list.sort(compare[sort]);
}

/** Unnamed hash-like file names shorten for display; the full text stays in tooltips. */
export function shortDisplayName(name: string): string {
  return /^[0-9a-f]{32,}\.[a-z0-9]+$/i.test(name) ? `${name.slice(0, 8)}…${name.slice(-7)}` : name;
}

export function baseName(name: string): string {
  return name.replace(/\.[a-z0-9]+$/i, '');
}
