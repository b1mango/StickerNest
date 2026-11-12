import { useState } from 'react';
import { ImageOff } from 'lucide-react';

/** Shows `src`; if it fails and a `fallback` is given (e.g. thumbnail not generated yet), shows that instead. */
export function StickerPreview({ src, fallback, name, eager = false }: { src: string; fallback?: string; name: string; eager?: boolean }) {
  const [failed, setFailed] = useState<string[]>([]);
  const current = !failed.includes(src) ? src : fallback && !failed.includes(fallback) ? fallback : null;
  return current === null ? (
    <span className="image-error" title="预览不可用"><ImageOff size={20} /></span>
  ) : (
    <img src={current} alt={name} draggable={false} loading={eager ? 'eager' : 'lazy'} decoding="async"
      onError={() => setFailed(list => [...list, current])} />
  );
}
