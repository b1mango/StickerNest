/** Layout math for the virtualized sticker grid, shared by rendering, hit-testing and tests. */

export const GRID_PAD = 20;
export const LIST_HEAD = 30;
export const LIST_ROW = 40;

export interface GridGeometry {
  columns: number;
  cell: number;
  gap: number;
  stride: number;
  height: number;
}

export function gridGeometry(width: number, tileSize: number, count: number): GridGeometry {
  const gap = tileSize < 96 ? 8 : 12;
  const inner = Math.max(0, width - GRID_PAD * 2);
  const columns = Math.max(1, Math.floor((inner + gap) / (tileSize + gap)));
  // Tiles stretch to fill the row so the right edge lines up with the padding.
  const cell = Math.max(40, Math.floor((inner - gap * (columns - 1)) / columns));
  const stride = cell + gap;
  const rows = Math.ceil(count / columns);
  const height = rows ? GRID_PAD * 2 + rows * stride - gap : 0;
  return { columns, cell, gap, stride, height };
}

export function tileBox(geometry: GridGeometry, index: number) {
  const col = index % geometry.columns;
  const row = Math.floor(index / geometry.columns);
  return { x: GRID_PAD + col * geometry.stride, y: GRID_PAD + row * geometry.stride, size: geometry.cell };
}

export interface Rect { left: number; top: number; right: number; bottom: number }

/** Indexes of grid tiles intersecting a rectangle given in content coordinates. */
export function gridHits(geometry: GridGeometry, count: number, rect: Rect): number[] {
  const { columns, stride, cell } = geometry;
  const firstRow = Math.max(0, Math.floor((rect.top - GRID_PAD) / stride));
  const lastRow = Math.floor((rect.bottom - GRID_PAD) / stride);
  const hits: number[] = [];
  for (let row = firstRow; row <= lastRow; row += 1) {
    for (let col = 0; col < columns; col += 1) {
      const index = row * columns + col;
      if (index >= count) return hits;
      const x = GRID_PAD + col * stride;
      const y = GRID_PAD + row * stride;
      if (x < rect.right && x + cell > rect.left && y < rect.bottom && y + cell > rect.top) hits.push(index);
    }
  }
  return hits;
}

export function listHits(count: number, rect: Rect): number[] {
  const first = Math.max(0, Math.floor((rect.top - LIST_HEAD) / LIST_ROW));
  const last = Math.min(count - 1, Math.floor((rect.bottom - LIST_HEAD) / LIST_ROW));
  const hits: number[] = [];
  for (let index = first; index <= last; index += 1) hits.push(index);
  return hits;
}
