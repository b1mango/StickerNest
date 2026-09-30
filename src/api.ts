import { convertFileSrc, invoke, isTauri } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import type { Snapshot, ImportReport } from './types';

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
