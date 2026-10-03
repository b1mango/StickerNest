export interface Sticker {
  id: string;
  name: string;
  fileName: string;
  format: string;
  width: number;
  height: number;
  bytes: number;
  importedAt: number;
  sources: string[];
}

export interface Snapshot {
  root: string;
  items: Sticker[];
}

export interface ImportReport {
  added: number;
  duplicates: number;
  failed: { name: string; error: string }[];
}

export const SOURCE_LABELS: Record<string, string> = {
  本地: '本地文件',
  微信: '微信',
  抖音: '抖音',
};

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export interface AssetMetadata {
  name: string;
  tags: string[];
  collections: string[];
}

export interface ManagementSnapshot {
  version: number;
  metadata: Record<string, AssetMetadata>;
  accounts: { id: string; platform: string; alias: string }[];
  batches: { id: string; accountId: string; collectionItems: number; mappedResources: number; failedResources: number }[];
  references: { accountId: string; stickerId: string; assetId: string; resourceIdentity: string }[];
}
