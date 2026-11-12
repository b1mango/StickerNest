import { useCallback, useEffect, useRef, useState } from 'react';
import * as api from '../api';
import { formatBytes, type ImportResult, type ManagementSnapshot, type Snapshot } from '../types';
import type { ToastApi } from './useToasts';

export function errorText(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}

/**
 * Owns the open library, its management record and every backend write.
 * Writes run one at a time; progress and outcomes surface as toasts.
 */
export function useLibrary(toast: ToastApi) {
  const [library, setLibrary] = useState<Snapshot | null>(null);
  const [management, setManagement] = useState<ManagementSnapshot | null>(null);
  const [managementError, setManagementError] = useState('');
  const [busy, setBusy] = useState('');
  const [loading, setLoading] = useState(api.isDesktop);
  const locked = useRef(false);
  const libraryRef = useRef(library);
  libraryRef.current = library;

  const loadManagement = useCallback(async () => {
    try { setManagement(await api.getManagement()); setManagementError(''); }
    catch (reason) { setManagement(null); setManagementError(errorText(reason)); }
  }, []);

  useEffect(() => {
    if (!api.isDesktop) return;
    let active = true;
    api.currentLibrary()
      .then(async snapshot => {
        if (!active) return;
        setLibrary(snapshot);
        if (snapshot) await loadManagement();
      })
      .catch(reason => { if (active) toast.push({ kind: 'error', title: '资料库未能打开', detail: errorText(reason) }); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [loadManagement, toast.push]);

  /** Runs one write at a time. Returns false when another write holds the lock or the action failed. */
  const run = useCallback(async (label: string, action: () => Promise<void>, failureTitle = '操作未完成'): Promise<boolean> => {
    if (locked.current || !api.isDesktop) {
      if (locked.current) toast.push({ kind: 'info', title: '请稍候', detail: '上一个操作还没完成。' });
      return false;
    }
    locked.current = true;
    setBusy(label);
    const progress = toast.push({ kind: 'progress', title: label });
    try { await action(); return true; }
    catch (reason) { toast.push({ kind: 'error', title: failureTitle, detail: errorText(reason) }); return false; }
    finally { toast.dismiss(progress); locked.current = false; setBusy(''); }
  }, [toast]);

  /** Same lock as `run`, but failures are thrown to the caller (dialogs show them inline). */
  const write = useCallback(async <T,>(action: (root: string) => Promise<T>): Promise<T> => {
    const root = libraryRef.current?.root;
    if (!root) throw new Error('请先打开资料库。');
    if (locked.current) throw new Error('上一个操作还没完成，请稍候。');
    locked.current = true;
    try { return await action(root); }
    finally { locked.current = false; }
  }, []);

  const applyLibrary = useCallback(async (snapshot: Snapshot) => {
    setLibrary(snapshot);
    setManagement(null);
    await loadManagement();
  }, [loadManagement]);

  const root = library?.root ?? '';

  function reportImport(result: ImportResult) {
    setLibrary(result.snapshot);
    const { added, duplicates, failed } = result.report;
    const parts = [`新增 ${added} 个`];
    if (duplicates) parts.push(`${duplicates} 个已在库中`);
    if (failed.length) parts.push(`${failed.length} 个失败`);
    toast.push({
      kind: failed.length ? 'error' : 'success',
      title: added ? '导入完成' : '没有新表情',
      detail: parts.join(' · ') + (failed.length ? `\n${failed.slice(0, 5).map(item => `${item.name}：${item.error}`).join('\n')}` : ''),
    });
    if (result.previewWarnings.length) toast.push({ kind: 'error', title: '部分素材无法预览', detail: result.previewWarnings.join('\n') });
  }

  return {
    library, management, managementError, busy, loading, run, write,
    setLibrary, setManagement, applyLibrary, reportImport,

    openLibrary: (create: boolean) => run(create ? '正在创建资料库' : '正在打开资料库', async () => {
      const snapshot = await api.chooseLibrary(create);
      if (snapshot) await applyLibrary(snapshot);
    }, create ? '资料库未能创建' : '资料库未能打开'),

    openRecent: (path: string) => run('正在打开资料库', async () => {
      await applyLibrary(await api.openLibraryPath(path));
    }, '资料库未能打开'),

    restoreBackup: () => run('正在校验并恢复备份', async () => {
      const path = await api.restoreBackup();
      if (!path) return;
      await applyLibrary(await api.openLibraryPath(path));
      toast.push({ kind: 'success', title: '已从备份恢复', detail: `在新位置打开，原备份保持不变。\n${path}` });
    }, '恢复未完成'),

    backup: () => run('正在备份资料库', async () => {
      const result = await api.backupLibrary();
      if (!result) return;
      toast.push({
        kind: 'success', title: `已备份 ${result.assetCount} 个素材`,
        detail: `${formatBytes(result.totalBytes)}${result.includesManagement ? '，含整理记录' : ''}，校验通过。\n${result.path}`,
      });
    }, '备份未完成'),

    reveal: (ids: string[]) => { if (root) api.revealInFinder(root, ids).catch(reason => toast.push({ kind: 'error', title: '无法在访达中显示', detail: errorText(reason) })); },

    importPaths: (paths: string[]) => run('正在导入表情', async () => reportImport(await api.importPaths(paths)), '导入未完成'),

    importPick: (folder: boolean) => run('正在导入表情', async () => {
      const paths = await api.pickImportPaths(folder);
      if (paths?.length) reportImport(await api.importPaths(paths));
    }, '导入未完成'),

    copy: (ids: string[]) => run('正在复制', async () => {
      const result = await api.copyToClipboard(root, ids, true);
      setManagement(await api.getManagement());
      const notes = [];
      if (result.converted) notes.push(`${result.converted} 个 WebP 已转成微信可用格式`);
      if (result.oversized.length) notes.push(`${result.oversized.length} 个超过 1 MB，微信可能无法添加为表情`);
      toast.push({ kind: 'success', title: `已复制 ${result.count} 个表情，可直接粘贴到微信`, detail: notes.join('；') || undefined });
    }, '复制失败'),

    setFavorite: (ids: string[], favorite: boolean) => run(favorite ? '正在收藏' : '正在取消收藏', async () => {
      setManagement(await api.setFavorite(root, ids, favorite));
    }),

    exportIds: (ids: string[], nameMap: Record<string, string>) => run('正在导出', async () => {
      const result = await api.exportAssets(root, ids, nameMap);
      if (!result) return;
      toast.push({
        kind: 'success', title: `已导出 ${result.exported} 个原图`,
        detail: result.skipped ? `跳过 ${result.skipped} 个` : undefined,
        action: { label: '在访达中显示', run: () => void api.revealInFinder(root, []).catch(() => undefined) },
      });
    }, '导出未完成'),

    trash: (ids: string[]) => run('正在移入回收站', async () => {
      setManagement(await api.setTrash(root, ids, true));
      toast.push({
        kind: 'success', title: `已移入回收站 ${ids.length} 项`,
        action: { label: '撤销', run: () => void api.setTrash(root, ids, false).then(setManagement).catch(reason => toast.push({ kind: 'error', title: '撤销失败', detail: errorText(reason) })) },
      });
    }),

    restore: (ids: string[]) => run('正在恢复', async () => {
      setManagement(await api.setTrash(root, ids, false));
      toast.push({ kind: 'success', title: `已恢复 ${ids.length} 项` });
    }),

    deleteForever: (ids: string[] | 'all') => run('正在彻底删除', async () => {
      const result = ids === 'all' ? await api.emptyTrash(root) : await api.deletePermanently(root, ids);
      setLibrary(result.snapshot);
      setManagement(result.management);
      toast.push({ kind: 'success', title: `已删除 ${result.deleted} 项`, detail: '原文件已移到系统废纸篓。' });
    }, '删除未完成'),

    addToCollection: (ids: string[], name: string, leave = '') => run(leave ? '正在移动' : '正在加入合集', async () => {
      setManagement(await api.batchLabels(root, ids, [], [], [name], leave ? [leave] : []));
      toast.push({ kind: 'success', title: leave ? `已移动 ${ids.length} 项到「${name}」` : `已加入「${name}」`, detail: leave ? undefined : `${ids.length} 项` });
    }),

    removeFromCollection: (ids: string[], name: string, groupIds: string[]) => run('正在移出合集', async () => {
      let next = await api.batchLabels(root, ids, [], [], [], [name]);
      // Group-level membership lives on the group record, not on the main asset.
      for (const groupId of groupIds) {
        const group = next.groups.find(item => item.id === groupId);
        if (group?.collections.includes(name)) next = await api.saveGroupMetadata(root, groupId, group.tags, group.collections.filter(value => value !== name));
      }
      setManagement(next);
    }),

    setTags: (ids: string[], add: string[], remove: string[]) => run('正在保存标签', async () => {
      setManagement(await api.batchLabels(root, ids, add, remove, [], []));
    }),

    disband: (groupId: string) => run('正在拆分分组', async () => {
      setManagement(await api.disbandGroup(root, groupId));
    }),
  };
}

export type LibraryApi = ReturnType<typeof useLibrary>;
