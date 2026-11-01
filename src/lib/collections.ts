/** Names are trimmed; empty, overlong or duplicate names are rejected before any backend write. */
export function validateCollectionName(raw: string, existing: Iterable<string>, current = ''): string {
  const name = raw.trim();
  if (!name) return '请输入合集名称。';
  if (name.length > 40) return '合集名称最多 40 个字。';
  if (name === current) return '';
  for (const entry of existing) if (entry === name) return `合集「${name}」已存在。`;
  return '';
}
