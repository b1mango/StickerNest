import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import ts from 'typescript';
const source = readFileSync(new URL('../src/App.tsx', import.meta.url), 'utf8');
const tree = ts.createSourceFile('App.tsx', source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
function functionSource(name) {
  let result;
  function visit(node) {
    if (ts.isFunctionDeclaration(node) && node.name?.text === name) result = node.getText(tree);
    ts.forEachChild(node, visit);
  }
  visit(tree);
  assert(result, `Missing ${name}`);
  return result;
}
const js = code => ts.transpileModule(code, { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;
const onKeySource = source.slice(source.indexOf('const onKey = (event: KeyboardEvent)'), source.indexOf("window.addEventListener('keydown', onKey)"));
function keyboard({ editing = false, modal = false, composing = false, handled = false, legacyIme = false } = {}) {
  let selected, prevented = false;
  const handler = new Function('document', 'inSelectable', 'sourceRef', 'filteredRef', 'setCheckedIds', js(onKeySource) + '; return onKey;')(
    { querySelector: () => modal }, () => true, { current: 'all' }, { current: [{ id: 'A' }, { id: 'B' }] }, next => selected = next
  );
  handler({ key: 'a', ctrlKey: true, metaKey: false, isComposing: composing, defaultPrevented: handled, keyCode: legacyIme ? 229 : 65, target: { closest: () => editing }, preventDefault: () => prevented = true });
  return { selected, prevented };
}
test('global selection respects editing, modal, IME, and handled events', () => {
  for (const name of ['editing', 'modal', 'composing', 'handled', 'legacyIme']) assert.deepEqual(keyboard({ [name]: true }), { selected: undefined, prevented: false });
  assert.deepEqual([...keyboard().selected], ['A', 'B']);
});
test('ignoring a pair preserves overlapping pairs and exact groups', async () => {
  let report = { exactGroups: [{ assetIds: ['A', 'B', 'C'] }], similarPairs: [{ baseId: 'A', otherId: 'B' }, { baseId: 'A', otherId: 'C' }], animationPairs: [{ baseId: 'B', otherId: 'A' }, { baseId: 'B', otherId: 'D' }] };
  const ignore = new Function('library', 'ignorePair', 'setManagement', 'setScanReport', js(functionSource('ignoreSimilarPair')) + '; return ignoreSimilarPair;')(
    { root: '/fixture' }, async () => ({}), () => {}, updater => report = updater(report)
  );
  await ignore('B', 'A');
  assert.deepEqual(report.similarPairs, [{ baseId: 'A', otherId: 'C' }]);
  assert.deepEqual(report.animationPairs, [{ baseId: 'B', otherId: 'D' }]);
  assert.equal(report.exactGroups.length, 1);
});
function createContext(addCollection) {
  const observed = { dialogOpen: true, collection: '', checked: new Set(['A']), management: undefined };
  const locked = { current: false };
  const bindings = { library: { root: '/fixture' }, management: { collections: [] }, collectionEntries: [], locked, busy: '', isDesktop: true, setBusy: () => {}, setError: () => {}, addCollection, selectedInDisplayOrder: () => ['A'], setManagement: value => observed.management = value, setCheckedIds: value => observed.checked = value, setCollection: value => observed.collection = value, setSource: () => {}, setPage: () => {}, setShowNewCollection: value => observed.dialogOpen = value };
  const create = new Function(...Object.keys(bindings), js(functionSource('createCollection')) + '; return createCollection;')(...Object.values(bindings));
  return { observed, locked, create };
}
test('collection write failure preserves modal, input destination, and selection', async () => {
  const { observed, locked, create } = createContext(async () => { throw Error('write failed'); });
  await assert.rejects(create('new'), /write failed/);
  assert.equal(observed.dialogOpen, true); assert.equal(observed.collection, ''); assert.deepEqual([...observed.checked], ['A']); assert.equal(locked.current, false);
});
test('collection success uses one backend write before closing', async () => {
  let calls = 0;
  const { observed, create } = createContext(async (root, name, ids) => { calls++; assert.equal(root, '/fixture'); assert.equal(name, 'new'); assert.deepEqual(ids, ['A']); return { collections: ['new'] }; });
  await create('new');
  assert.equal(calls, 1); assert.equal(observed.dialogOpen, false); assert.equal(observed.collection, 'new'); assert.equal(observed.checked.size, 0);
});
