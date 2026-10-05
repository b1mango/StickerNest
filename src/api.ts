import { convertFileSrc, invoke, isTauri } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import type { BackupSummary, Snapshot, ImportReport } from './types';

export const isDesktop = isTauri();

export async function chooseLibrary(create: boolean): Promise<Snapshot | null> {
  const path = await open({ directory: true, multiple: false,
    title: create ? '选择资料库的存放位置' : '打开 StickerNest Library 文件夹' });
  if (!path) return null;
  return invoke<Snapshot>('select_library', { path, create });
}

export async function currentLibrary(): Promise<Snapshot | null> {
  if (!isDesktop) return null;
  return invoke<Snapshot | null>('current_library');
}

export async function importImages(source: string): Promise<{ snapshot: Snapshot; report: ImportReport; previewWarnings: string[] } | null> {
  const paths = await open({ multiple: true, directory: false, title: '选择要导入的表情',
    filters: [{ name: '图片与动图', extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp'] }] });
  if (!paths) return null;
  return invoke('import_images', { paths: typeof paths === 'string' ? [paths] : paths, source });
}

export function imageUrl(root: string, fileName: string): string {
  return convertFileSrc(`${root}/assets/${fileName}`);
}

export async function getManagement(): Promise<import('./types').ManagementSnapshot> {
  return invoke('get_management');
}

export async function saveMetadata(expectedRoot: string, assetId: string, metadata: import('./types').AssetMetadata): Promise<import('./types').ManagementSnapshot> {
  return invoke('save_metadata', { expectedRoot, assetId, ...metadata });
}

export async function importProvenance(expectedRoot: string, accountAlias: string, accountId: string | null): Promise<import('./types').ManagementSnapshot | null> {
  const path = await open({ multiple: false, directory: false, title: '选择抖音本地采集报告', filters: [{ name: '采集报告', extensions: ['json'] }] });
  if (!path) return null;
  return invoke('import_provenance', { expectedRoot, path, accountAlias, accountId });
}

export async function setTrash(expectedRoot: string, assetIds: string[], trashed: boolean): Promise<import('./types').ManagementSnapshot> {
  return invoke('set_trash', { expectedRoot, assetIds, trashed });
}

export async function backupLibrary(): Promise<BackupSummary | null> {
  const path = await open({ directory: true, multiple: false, title: '选择备份存放位置（将新建备份文件夹）' });
  if (!path) return null;
  return invoke('backup_library', { targetParent: path });
}

export async function restoreBackup(): Promise<string | null> {
  const backup = await open({ directory: true, multiple: false, title: '选择 StickerNest Backup 文件夹' });
  if (!backup) return null;
  const parent = await open({ directory: true, multiple: false, title: '选择恢复位置（将新建 StickerNest Library）' });
  if (!parent) return null;
  return invoke('restore_backup', { backupDir: backup, targetParent: parent });
}

export function openLibraryPath(path: string): Promise<Snapshot> {
  return invoke<Snapshot>('select_library', { path, create: false });
}

export async function scanDuplicates(): Promise<import('./types').ScanReport> {
  return invoke('scan_duplicates');
}

export async function createGroup(expectedRoot: string, memberIds: string[], mainAssetId: string, tags: string[], collections: string[]): Promise<import('./types').ManagementSnapshot> {
  return invoke('create_group', { expectedRoot, memberIds, mainAssetId, tags, collections });
}

export async function disbandGroup(expectedRoot: string, groupId: string): Promise<import('./types').ManagementSnapshot> {
  return invoke('disband_group', { expectedRoot, groupId });
}

export async function saveGroupMetadata(expectedRoot: string, groupId: string, tags: string[], collections: string[]): Promise<import('./types').ManagementSnapshot> {
  return invoke('save_group_metadata', { expectedRoot, groupId, tags, collections });
}

export async function batchRename(expectedRoot: string, assetIds: string[], prefix: string, start: number): Promise<import('./types').ManagementSnapshot> {
  return invoke('batch_rename', { expectedRoot, assetIds, prefix, start });
}

export async function batchLabels(expectedRoot: string, assetIds: string[], addTags: string[], removeTags: string[], addCollections: string[], removeCollections: string[]): Promise<import('./types').ManagementSnapshot> {
  return invoke('batch_labels', { expectedRoot, assetIds, addTags, removeTags, addCollections, removeCollections });
}

export async function ignorePair(expectedRoot: string, assetA: string, assetB: string): Promise<import('./types').ManagementSnapshot> {
  return invoke('ignore_pair', { expectedRoot, assetA, assetB });
}

export async function exportAssets(expectedRoot: string, assetIds: string[], nameMap: Record<string, string>): Promise<import('./types').ExportSummary | null> {
  const path = await open({ directory: true, multiple: false, title: '选择导出位置（将新建导出文件夹）' });
  if (!path) return null;
  return invoke('export_assets', { expectedRoot, assetIds, targetParent: path, nameMap });
}
