import { useEffect, useRef, useState } from 'react';
import { GameEngine } from '@/game/engine';
import { seedDemoTown } from '@/game/demo';
import { render } from '@/game/render';
import type { Camera, UIState } from '@/game/render';
import { getSettings } from '@/app/settings';

/**
 * Attract mode behind the main menu: the classic demo town, frozen
 * (never advance()d — render() animates water/lights off ui.time alone)
 * with a slow drifting camera. Deliberately NOT a GameCanvas: no input
 * machinery, no dev hook, no simulation.
 */
export function MenuBackdrop() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [engine, setEngine] = useState<GameEngine | null>(null);

  // Defer engine creation and seeding so main menu initial mount is instant
  useEffect(() => {
    let active = true;
    const init = () => {
      const e = new GameEngine({ seed: 1961 });
      seedDemoTown(e);
      e.setSpeed(0);
      if (active) setEngine(e);
    };
    // Yield to browser main thread paint before running 100-day seeding
    const timer = setTimeout(init, 0);
    return () => { active = false; clearTimeout(timer); };
  }, []);

  useEffect(() => {
    if (!engine) return;
    const canvas = canvasRef.current!;
    const ctx = canvas.getContext('2d')!;
    let raf = 0;
    let vw = 0, vh = 0, dpr = 1;
    const cam: Camera = { x: 0, y: 0, z: 0.9 };
    let baseX = 0, baseY = 0;

    const resize = () => {
      dpr = Math.min(2, window.devicePixelRatio || 1);
      vw = canvas.clientWidth; vh = canvas.clientHeight;
      canvas.width = Math.round(vw * dpr);
      canvas.height = Math.round(vh * dpr);
      baseX = vw / 2;
      baseY = vh / 2 - ((engine.mapW + engine.mapH) / 2) * 16 * cam.z;
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(canvas);

    const ui: UIState = { hoverTile: null, tool: { kind: 'select' }, selection: [], time: 0 };
    let lastRenderTime = 0;
    let renderedOnce = false;

    const frame = (now: number) => {
      const reduced = getSettings().reducedMotion;
      const t = reduced ? 0 : now / 1000;
      cam.x = baseX + Math.sin(t * 0.05) * 90;
      cam.y = baseY + Math.cos(t * 0.037) * 55;
      ui.time = reduced ? 0 : now;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      render(ctx, engine, cam, ui, vw, vh);
    };

    const loop = (now: number) => {
      if (document.hidden) {
        raf = requestAnimationFrame(loop);
        return;
      }

      const reduced = getSettings().reducedMotion;
      if (reduced) {
        if (!renderedOnce) {
          frame(now);
          renderedOnce = true;
        }
        raf = requestAnimationFrame(loop);
        return;
      }

      // Cap backdrop attract animation to ~30 FPS (33ms interval)
      if (now - lastRenderTime >= 32) {
        lastRenderTime = now;
        frame(now);
      }
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);

    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  }, [engine]);

  return (
    <>
      <canvas ref={canvasRef} className="absolute inset-0 h-full w-full" aria-hidden="true" />
      <div className="absolute inset-0 bg-gradient-to-b from-black/60 via-black/30 to-black/80 pointer-events-none" />
    </>
  );
}
