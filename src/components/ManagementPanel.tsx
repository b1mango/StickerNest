import { useMemo, useState } from 'react';
import type { ManagementSnapshot } from '../types';

export function ManagementPanel({ management, busy, onImport }: {
  management: ManagementSnapshot;
  busy: boolean;
  onImport: (alias: string, accountId: string | null) => Promise<void>;
}) {
  const [accountId, setAccountId] = useState('');
  const [alias, setAlias] = useState('');
  const stats = useMemo(() => {
    const entries = new Set<string>();
    const assets = new Map<string, Set<string>>();
    for (const reference of management.references) {
      const key = JSON.stringify([reference.accountId, reference.stickerId]);
      entries.add(key);
      const group = assets.get(reference.assetId) ?? new Set<string>();
      group.add(key);
      assets.set(reference.assetId, group);
    }
    return { entries: entries.size, assets: assets.size, shared: [...assets.values()].filter(group => group.size > 1).length };
  }, [management]);
  const recentBatches = useMemo(() => {
    const latest = new Map<string, ManagementSnapshot['batches'][number]>();
    for (const batch of management.batches) latest.set(batch.accountId, batch);
    return management.accounts.flatMap(account => {
      const batch = latest.get(account.id);
      return batch ? [{ ...batch, alias: account.alias }] : [];
    });
  }, [management]);
  const account = management.accounts.find(value => value.id === accountId);
  const valid = accountId === 'new' ? !!alias.trim() : !!account;
  return <details className="management-panel">
    <summary>来源与精确去重 <span>{stats.entries} 项收藏 · {stats.assets} 个素材 · {stats.shared} 组共用文件</span></summary>
    <p>仅统计导入报告；相同文件只存一份。</p>
    {recentBatches.length ? <div className="batch-completeness" aria-label="各账号最近记录批次完整性"><strong>最近导入</strong>{recentBatches.map(batch => {
      const unprocessed = batch.collectionItems - batch.mappedResources - batch.failedResources;
      const incomplete = batch.failedResources > 0 || unprocessed > 0;
      return <div key={batch.accountId} className={incomplete ? 'batch-row incomplete' : 'batch-row'}><b>{batch.alias}</b><p>清单 {batch.collectionItems} 项 · 已映射 {batch.mappedResources} 项 · 失败 {batch.failedResources} 项 · 未处理 {unprocessed} 项</p><p>{incomplete ? '来源映射待补，请补齐报告后重导。' : '本批次映射完成。'}</p></div>;
    })}</div> : null}
    <div className="management-import">
      <label className="field-label">抖音账号<select value={accountId} disabled={busy} onChange={event => setAccountId(event.target.value)}><option value="">选择账号</option>{management.accounts.filter(value => value.platform === '抖音' || value.platform === 'douyin').map(value => <option key={value.id} value={value.id}>{value.alias}</option>)}<option value="new">新增账号…</option></select></label>
      {accountId === 'new' ? <label className="field-label">账号别名<input value={alias} maxLength={80} placeholder="例如：抖音小号" disabled={busy} onChange={event => setAlias(event.target.value)} /></label> : null}
      <button className="button secondary" disabled={busy || !valid} onClick={() => void onImport(account?.alias ?? alias.trim(), account?.id ?? null)}>导入本地报告</button>
    </div>
    <p>重导请选已有账号；其他账号请新建。</p>
  </details>;
}
