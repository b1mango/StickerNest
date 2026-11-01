/**
 * Browser-only stand-in for the Tauri backend so the UI can be developed and
 * screenshotted with `npm run dev` and `?mock` in the URL. Never bundled into
 * the desktop build: main.tsx only imports it under import.meta.env.DEV.
 */
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import type { ManagementSnapshot, Snapshot, Sticker } from '../types';

const ROOT = '/Users/demo/Pictures/表情库';
const NAMES = ['狗头', '我太难了', '好的收到', '摸鱼', '震惊', '谢谢老板', '晚安', '哈哈哈哈', '无语', '加油', '在吗', '流泪猫猫', '芜湖起飞', '奥利给', '摆烂', '冲鸭', '吃瓜', '溜了溜了', '比心', '问号脸', '委屈', '开心', '生气', '害羞'];
const HUES = [12, 38, 52, 140, 175, 200, 220, 262, 300, 330];

function face(index: number): string {
  const hue = HUES[index % HUES.length];
  const mouth = ['M38 66 Q50 78 62 66', 'M38 70 Q50 62 62 70', 'M40 68 H60', 'M38 64 Q50 84 62 64'][index % 4];
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
    <circle cx="50" cy="50" r="40" fill="hsl(${hue} 85% 62%)"/>
    <circle cx="36" cy="42" r="5" fill="#222"/><circle cx="64" cy="42" r="5" fill="#222"/>
    <path d="${mouth}" stroke="#222" stroke-width="4" fill="none" stroke-linecap="round"/>
  </svg>`;
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
}

const now = Math.floor(Date.now() / 1000);
const items: Sticker[] = Array.from({ length: 72 }, (_, i) => {
  const source = i % 5 === 0 ? '本地' : i % 2 ? '抖音' : '微信';
  const format = source === '抖音' ? (i % 3 ? 'webp' : 'gif') : i % 4 ? 'gif' : 'png';
  return {
    id: `asset-${i}`,
    name: `${NAMES[i % NAMES.length]}${i >= NAMES.length ? ` ${Math.floor(i / NAMES.length) + 1}` : ''}`,
    fileName: `asset-${i}.${format}`,
    format,
    width: 240 + (i % 3) * 60,
    height: 240,
    bytes: 18_000 + ((i * 7919) % 1_200_000),
    importedAt: now - i * 5400,
    sources: [source],
  };
});

let management: ManagementSnapshot = {
  version: 3,
  metadata: {
    'asset-0': { name: '', tags: ['日常'], collections: ['日常'] },
    'asset-1': { name: '', tags: [], collections: ['日常', '工作'] },
    'asset-2': { name: '', tags: [], collections: ['工作'] },
    'asset-5': { name: '', tags: [], collections: ['阴阳怪气'] },
  },
  accounts: [{ id: 'acc-1', platform: '微信', alias: '我的微信' }, { id: 'acc-2', platform: '抖音', alias: '我的抖音' }],
  batches: [],
  references: [],
  trash: { 'asset-70': now - 3600, 'asset-71': now - 7200 },
  groups: [{ id: 'group-1', mainAssetId: 'asset-3', memberIds: ['asset-3', 'asset-27', 'asset-51'], tags: [], collections: [], createdAt: now }],
  ignoredPairs: [],
  collections: ['日常', '工作', '阴阳怪气'],
  favorites: { 'asset-0': now, 'asset-4': now - 10, 'asset-9': now - 20 },
  recent: { 'asset-4': now - 60, 'asset-0': now - 600 },
};

const snapshot = (): Snapshot => ({ root: ROOT, items });
const meta = (id: string) => (management.metadata[id] ??= { name: '', tags: [], collections: [] });
const update = (fn: (draft: ManagementSnapshot) => void) => {
  management = structuredClone(management);
  fn(management);
  return management;
};

type Args = Record<string, unknown>;
const handlers: Record<string, (args: Args) => unknown> = {
  current_library: () => (new URLSearchParams(location.search).has('welcome') ? null : snapshot()),
  select_library: () => snapshot(),
  get_management: () => management,
  set_favorite: ({ assetIds, favorite }) => update(m => {
    m.favorites ??= {};
    for (const id of assetIds as string[]) if (favorite) m.favorites[id] = now; else delete m.favorites[id];
  }),
  mark_used: ({ assetIds }) => update(m => { m.recent ??= {}; for (const id of assetIds as string[]) m.recent[id] = Math.floor(Date.now() / 1000); }),
  set_trash: ({ assetIds, trashed }) => update(m => {
    for (const id of assetIds as string[]) if (trashed) m.trash[id] = now; else delete m.trash[id];
  }),
  save_metadata: ({ assetId, name, tags, collections }) => update(m => { m.metadata[assetId as string] = { name: name as string, tags: tags as string[], collections: collections as string[] }; }),
  add_collection: ({ name, assetIds }) => update(m => {
    if (!m.collections.includes(name as string)) m.collections.push(name as string);
    for (const id of assetIds as string[]) { const entry = (m.metadata[id] ??= { name: '', tags: [], collections: [] }); if (!entry.collections.includes(name as string)) entry.collections.push(name as string); }
  }),
  batch_labels: ({ assetIds, addCollections, removeCollections, addTags, removeTags }) => update(m => {
    for (const id of assetIds as string[]) {
      const entry = (m.metadata[id] ??= { name: '', tags: [], collections: [] });
      entry.collections = [...new Set([...entry.collections, ...(addCollections as string[])])].filter(c => !(removeCollections as string[]).includes(c));
      entry.tags = [...new Set([...entry.tags, ...(addTags as string[])])].filter(t => !(removeTags as string[]).includes(t));
    }
  }),
  rename_collection: ({ from, to }) => update(m => {
    m.collections = m.collections.map(c => (c === from ? to as string : c));
    for (const entry of Object.values(m.metadata)) entry.collections = entry.collections.map(c => (c === from ? to as string : c));
  }),
  delete_collection: ({ name }) => update(m => {
    m.collections = m.collections.filter(c => c !== name);
    for (const entry of Object.values(m.metadata)) entry.collections = entry.collections.filter(c => c !== name);
  }),
  batch_rename: ({ assetIds, prefix, start }) => update(m => {
    (assetIds as string[]).forEach((id, i) => { meta(id); m.metadata[id].name = `${prefix}-${String((start as number) + i).padStart(3, '0')}`; });
  }),
  disband_group: ({ groupId }) => update(m => { m.groups = m.groups.filter(g => g.id !== groupId); }),
  copy_to_clipboard: ({ assetIds }) => ({ count: (assetIds as string[]).length, converted: (assetIds as string[]).filter(id => items.find(i => i.id === id)?.format === 'webp').length, oversized: [] }),
  reveal_in_finder: () => undefined,
  scan_duplicates: () => ({ scannedStatics: 40, scannedAnimations: 30, skippedTiny: 0, failed: [], exactGroups: [{ pixelHash: 'x', assetIds: ['asset-6', 'asset-30'] }], similarPairs: [{ baseId: 'asset-7', otherId: 'asset-31', distance: 4 }], animationPairs: [] }),
  ignore_pair: () => management,
  delete_permanently: ({ assetIds }) => {
    const gone = new Set(assetIds as string[]);
    items.splice(0, items.length, ...items.filter(i => !gone.has(i.id)));
    return { snapshot: snapshot(), management: update(m => { for (const id of gone) delete m.trash[id]; }), deleted: gone.size };
  },
  'plugin:event|listen': () => 1,
  'plugin:event|unlisten': () => undefined,
  empty_trash: () => {
    const gone = new Set(Object.keys(management.trash));
    items.splice(0, items.length, ...items.filter(i => !gone.has(i.id)));
    return { snapshot: snapshot(), management: update(m => { m.trash = {}; }), deleted: gone.size };
  },
};

mockWindows('main');
mockIPC((command, args) => {
  const handler = handlers[command];
  if (!handler) throw new Error(`mock: ${command} 未实现`);
  return handler((args ?? {}) as Args);
});

export const assetUrl = (fileName: string) => face(Number(fileName.match(/\d+/)?.[0] ?? 0));
