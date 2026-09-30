import { useState } from 'react';
import { ImageOff } from 'lucide-react';

export function StickerPreview({ src, name, eager = false }: { src: string; name: string; eager?: boolean }) {
  const [failed, setFailed] = useState(false);
  return failed ? (
    <span className="image-error"><ImageOff size={24} /><span>预览不可用</span></span>
  ) : (
    <img src={src} alt={name} loading={eager ? 'eager' : 'lazy'} decoding="async" onError={() => setFailed(true)} />
  );
}
