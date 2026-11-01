// Pure frontend logic, loaded straight from TypeScript via Node's type stripping.
import assert from 'node:assert/strict';
import test from 'node:test';
import { clickSelect, emptySelection, inOrder, moveSelect, shouldHandleGlobalKey } from '../src/lib/selection.ts';
import { pruneReviewed, withoutPair } from '../src/lib/scan.ts';
import { validateCollectionName } from '../src/lib/collections.ts';
import { GRID_PAD, gridGeometry, gridHits, listHits, LIST_HEAD, LIST_ROW, tileBox } from '../src/lib/geometry.ts';
import { buildIndex, filterItems, sameNav, shortDisplayName } from '../src/lib/derive.ts';

const ids = selection => [...selection.ids].sort();
const key = (overrides = {}) => ({ defaultPrevented: false, isComposing: false, keyCode: 65, target: { closest: () => null }, ...overrides });

test('global shortcuts stay out of text fields, dialogs, IME and handled events', () => {
  assert.equal(shouldHandleGlobalKey(key(), false), true);
  assert.equal(shouldHandleGlobalKey(key(), true), false);
  assert.equal(shouldHandleGlobalKey(key({ defaultPrevented: true }), false), false);
  assert.equal(shouldHandleGlobalKey(key({ isComposing: true }), false), false);
  assert.equal(shouldHandleGlobalKey(key({ keyCode: 229 }), false), false);
  assert.equal(shouldHandleGlobalKey(key({ target: { closest: () => ({}) } }), false), false);
  assert.equal(shouldHandleGlobalKey(key({ target: null }), false), true);
});

test('click selection follows Finder rules', () => {
  const order = ['A', 'B', 'C', 'D', 'E'];
  let sel = clickSelect(emptySelection(), order, 'B', { meta: false, shift: false });
  assert.deepEqual(ids(sel), ['B']);
  sel = clickSelect(sel, order, 'D', { meta: false, shift: true });
  assert.deepEqual(ids(sel), ['B', 'C', 'D']);
  assert.equal(sel.anchor, 'B', 'shift keeps the anchor');
  sel = clickSelect(sel, order, 'A', { meta: false, shift: true });
  assert.deepEqual(ids(sel), ['A', 'B'], 'a new shift range replaces the old one');
  sel = clickSelect(sel, order, 'E', { meta: true, shift: false });
  assert.deepEqual(ids(sel), ['A', 'B', 'E']);
  sel = clickSelect(sel, order, 'A', { meta: true, shift: false });
  assert.deepEqual(ids(sel), ['B', 'E'], 'command toggles');
  sel = clickSelect({ ids: new Set(['A']), anchor: 'gone' }, order, 'C', { meta: false, shift: true });
  assert.deepEqual(ids(sel), ['C'], 'stale anchor falls back to a plain click');
  assert.deepEqual(inOrder(new Set(['D', 'A']), order), ['A', 'D']);
});

test('arrow keys move and extend selection through the grid', () => {
  const order = ['A', 'B', 'C', 'D', 'E', 'F'];
  assert.equal(moveSelect(emptySelection(), [], 3, 'ArrowRight', false), null);
  assert.equal(moveSelect(emptySelection(), order, 3, 'Enter', false), null);
  let sel = moveSelect(emptySelection(), order, 3, 'ArrowDown', false);
  assert.deepEqual(ids(sel), ['A'], 'first arrow lands on the first item');
  sel = moveSelect(sel, order, 3, 'ArrowDown', false);
  assert.deepEqual(ids(sel), ['D']);
  sel = moveSelect(sel, order, 3, 'ArrowRight', true);
  assert.deepEqual(ids(sel), ['D', 'E']);
  sel = moveSelect(sel, order, 3, 'ArrowDown', false);
  assert.deepEqual(ids(sel), ['F'], 'clamps at the end');
  sel = moveSelect(sel, order, 3, 'ArrowUp', false);
  assert.deepEqual(ids(sel), ['C']);
});

test('ignoring a pair keeps overlapping pairs and exact groups', () => {
  const report = {
    scannedStatics: 0, scannedAnimations: 0, skippedTiny: 0, failed: [],
    exactGroups: [{ pixelHash: 'h', assetIds: ['A', 'B', 'C'] }],
    similarPairs: [{ baseId: 'A', otherId: 'B', distance: 1 }, { baseId: 'A', otherId: 'C', distance: 2 }],
    animationPairs: [{ baseId: 'B', otherId: 'A' }, { baseId: 'B', otherId: 'D' }],
  };
  const next = withoutPair(report, 'B', 'A');
  assert.deepEqual(next.similarPairs.map(p => p.otherId), ['C']);
  assert.deepEqual(next.animationPairs.map(p => p.otherId), ['D']);
  assert.equal(next.exactGroups.length, 1);

  const pruned = pruneReviewed(report, ['C']);
  assert.equal(pruned.exactGroups.length, 0, 'a group touching a reviewed asset is resolved');
  assert.deepEqual(pruned.similarPairs.map(p => p.otherId), ['B']);
  assert.equal(pruned.animationPairs.length, 2);
});

test('collection names are trimmed, bounded and unique', () => {
  assert.equal(validateCollectionName('  ', []), '请输入合集名称。');
  assert.equal(validateCollectionName('x'.repeat(41), []), '合集名称最多 40 个字。');
  assert.equal(validateCollectionName(' 日常 ', ['日常']), '合集「日常」已存在。');
  assert.equal(validateCollectionName('日常', ['日常'], '日常'), '', 'renaming to itself is fine');
  assert.equal(validateCollectionName('工作', new Set(['日常'])), '');
});

test('grid geometry fills the row and hit-tests marquee rectangles', () => {
  const geo = gridGeometry(1000, 120, 10);
  assert.equal(geo.columns, 7);
  assert.equal(GRID_PAD * 2 + geo.columns * geo.cell + (geo.columns - 1) * geo.gap <= 1000, true);
  assert.deepEqual(tileBox(geo, 8), { x: GRID_PAD + geo.stride, y: GRID_PAD + geo.stride, size: geo.cell });
  assert.equal(gridGeometry(1000, 120, 0).height, 0);
  assert.equal(gridGeometry(10, 120, 3).columns, 1, 'never zero columns');

  const box = tileBox(geo, 1);
  assert.deepEqual(gridHits(geo, 10, { left: box.x + 1, top: box.y + 1, right: box.x + 2, bottom: box.y + 2 }), [1]);
  assert.deepEqual(gridHits(geo, 10, { left: 0, top: 0, right: 1000, bottom: 10_000 }), [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
  assert.deepEqual(gridHits(geo, 10, { left: 0, top: 0, right: GRID_PAD - 1, bottom: GRID_PAD - 1 }), [], 'padding hits nothing');
  assert.deepEqual(listHits(5, { left: 0, top: LIST_HEAD + LIST_ROW, right: 10, bottom: LIST_HEAD + LIST_ROW * 2 + 1 }), [1, 2]);
  assert.deepEqual(listHits(2, { left: 0, top: 0, right: 10, bottom: 10_000 }), [0, 1]);
});

const sticker = (id, extra = {}) => ({ id, name: id, fileName: `${id}.png`, format: 'png', width: 1, height: 1, bytes: 1, importedAt: 0, sources: ['微信'], ...extra });

test('library index counts visible items and applies names, groups and trash', () => {
  const library = {
    root: '/lib',
    items: [
      sticker('a', { importedAt: 3, sources: ['抖音'] }),
      sticker('b', { importedAt: 2, bytes: 50 }),
      sticker('c', { importedAt: 1 }),
      sticker('d', { importedAt: 4 }),
    ],
  };
  const management = {
    version: 3,
    metadata: { a: { name: '狗头', tags: ['阴阳'], collections: ['日常'] } },
    accounts: [], batches: [], references: [], ignoredPairs: [],
    trash: { d: 10 },
    groups: [{ id: 'g', mainAssetId: 'b', memberIds: ['b', 'c'], tags: [], collections: ['工作'], createdAt: 0 }],
    collections: ['日常', '工作', '空的'],
    favorites: { b: 5 },
    recent: { a: 1, b: 2 },
  };
  const index = buildIndex(library, management);
  assert.equal(index.byId.get('a').name, '狗头');
  assert.deepEqual(index.counts, { all: 2, favorites: 1, recent: 2, trash: 1, sources: { 抖音: 1, 微信: 1 } });
  assert.deepEqual(index.collections, [['工作', 1], ['日常', 1], ['空的', 0]].sort((x, y) => x[0].localeCompare(y[0], 'zh')));

  const names = (nav, query = '', sort = 'newest') => filterItems(index, nav, query, sort).map(item => item.id);
  assert.deepEqual(names({ kind: 'all' }), ['a', 'b'], 'group members and trash are hidden');
  assert.deepEqual(names({ kind: 'all' }, '', 'size'), ['b', 'a']);
  assert.deepEqual(names({ kind: 'trash' }), ['d']);
  assert.deepEqual(names({ kind: 'recent' }), ['b', 'a'], 'recent is most-recent first');
  assert.deepEqual(names({ kind: 'favorites' }), ['b']);
  assert.deepEqual(names({ kind: 'source', value: '抖音' }), ['a']);
  assert.deepEqual(names({ kind: 'collection', value: '工作' }), ['b'], 'group collections apply to the main');
  assert.deepEqual(names({ kind: 'all' }, '阴阳'), ['a'], 'search matches tags');
  assert.deepEqual(names({ kind: 'all' }, '狗'), ['a'], 'search matches custom names');
  assert.deepEqual(names({ kind: 'similar' }), []);
  assert.equal(buildIndex(null, null).counts.all, 0);
});

test('nav comparison and display names', () => {
  assert.equal(sameNav({ kind: 'collection', value: 'x' }, { kind: 'collection', value: 'x' }), true);
  assert.equal(sameNav({ kind: 'collection', value: 'x' }, { kind: 'collection', value: 'y' }), false);
  assert.equal(sameNav({ kind: 'all' }, { kind: 'trash' }), false);
  assert.equal(shortDisplayName(`${'a'.repeat(32)}.webp`), 'aaaaaaaa…aa.webp');
  assert.equal(shortDisplayName('狗头.gif'), '狗头.gif');
});
