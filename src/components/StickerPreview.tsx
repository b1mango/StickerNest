import { useState } from 'react';
import { ImageOff } from 'lucide-react';

export function StickerPreview({ src, name, eager = false }: { src: string; name: string; eager?: boolean }) {
  const [failed, setFailed] = useState(false);
  return failed ? (
    <span className="image-error" title="预览不可用"><ImageOff size={20} /></span>
  ) : (
    <img src={src} alt={name} draggable={false} loading={eager ? 'eager' : 'lazy'} decoding="async" onError={() => setFailed(true)} />
  );
}
