import type { ScanReport } from '../types';

/** Drop reviewed assets from a scan report so the user never needs a full rescan. */
export function pruneReviewed(report: ScanReport, doneIds: string[]): ScanReport {
  const done = new Set(doneIds);
  return {
    ...report,
    exactGroups: report.exactGroups.filter(group => !group.assetIds.some(id => done.has(id))),
    similarPairs: report.similarPairs.filter(pair => !done.has(pair.baseId) && !done.has(pair.otherId)),
    animationPairs: report.animationPairs.filter(pair => !done.has(pair.baseId) && !done.has(pair.otherId)),
  };
}

/** Ignoring one pair removes only that pair; overlapping pairs and exact groups stay. */
export function withoutPair(report: ScanReport, a: string, b: string): ScanReport {
  const matches = (pair: { baseId: string; otherId: string }) =>
    (pair.baseId === a && pair.otherId === b) || (pair.baseId === b && pair.otherId === a);
  return {
    ...report,
    similarPairs: report.similarPairs.filter(pair => !matches(pair)),
    animationPairs: report.animationPairs.filter(pair => !matches(pair)),
  };
}
