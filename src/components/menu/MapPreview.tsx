import { useDeferredValue, useEffect, useRef } from 'react';
import { generateMap } from '@/game/mapgen';

const TERRAIN_COLORS: Record<string, string> = {
  grass: '#5a7d3a',
  forest: '#3d5c2a',
  water: '#3a6ea5',
  rock: '#8b8b8b',
};

const DEPOSIT_COLORS: Record<string, string> = {
  coal: '#1c1c1c',
  ironOre: '#8a4b2f',
  oil: '#2a2337',
  gravel: '#b5b5b5',
};

/** Minimap of the map a seed will generate — makes the reroll button mean something. */
export function MapPreview({ seed, tiles }: { seed: number; tiles: number }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const deferredSeed = useDeferredValue(seed);

  useEffect(() => {
    const canvas = canvasRef.current!;
    const px = Math.max(2, Math.floor(220 / tiles));
    canvas.width = tiles * px;
    canvas.height = tiles * px;
    const ctx = canvas.getContext('2d')!;
    const m = generateMap(deferredSeed, tiles, tiles);

    for (let y = 0; y < tiles; y++) {
      for (let x = 0; x < tiles; x++) {
        const t = m.tiles[y][x];
        ctx.fillStyle = t.foreign
          ? (t.terrain === 'water' ? '#2d4a72' : '#6e2b22')
          : TERRAIN_COLORS[t.terrain];
        ctx.fillRect(x * px, y * px, px, px);
        if (t.deposit && !t.foreign) {
          ctx.fillStyle = DEPOSIT_COLORS[t.deposit];
          ctx.fillRect(x * px, y * px, px, px);
        }
      }
    }
    // the crossing lane and the founding site
    if (m.crossX !== undefined && m.crossY !== undefined) {
      ctx.fillStyle = '#e8e2d4';
      ctx.fillRect(m.crossX * px, m.crossY * px, 2 * px, 2 * px);
    }
    ctx.fillStyle = '#f5c518';
    ctx.beginPath();
    ctx.arc((m.startX + 0.5) * px, (m.startY + 0.5) * px, Math.max(2.5, px), 0, Math.PI * 2);
    ctx.fill();
  }, [deferredSeed, tiles]);

  return (
    <div className="flex flex-col items-center gap-1.5">
      <canvas
        ref={canvasRef}
        className="rounded border-2 border-yellow-600/40 bg-red-950/60 w-[220px] h-[220px]"
        style={{ imageRendering: 'pixelated' }}
        aria-label={`Map preview for seed ${seed}`}
      />
      <div className="text-[0.625rem] uppercase tracking-wider text-yellow-200/60">
        Seed {seed} · {tiles}×{tiles} · gold dot marks the founding site
      </div>
    </div>
  );
}
