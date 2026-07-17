import { useEffect, useRef } from 'react';
import type { GameEngine } from '@/game/engine';
import { render, screenToTile, pickBuilding, type Camera, type UIState } from '@/game/render';
import { MAP_W, MAP_H } from '@/game/mapgen';

export type Tool = { kind: 'select' } | { kind: 'build'; defId: string } | { kind: 'bulldoze' };

interface Props {
  engine: GameEngine;
  tool: Tool;
  setTool: (t: Tool) => void;
  selectedId: number | null;
  setSelectedId: (id: number | null) => void;
  instantBuild: boolean;
  onError: (msg: string) => void;
}

export default function GameCanvas({ engine, tool, setTool, selectedId, setSelectedId, instantBuild, onError }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const camRef = useRef<Camera>({ x: 0, y: 0, z: 0.8 });
  const uiRef = useRef<UIState>({ hoverTile: null, tool, selectedId, time: 0 });
  uiRef.current.tool = tool;
  uiRef.current.selectedId = selectedId;
  const engineRef = useRef(engine);
  engineRef.current = engine;
  const cbRef = useRef({ setTool, setSelectedId, onError });
  cbRef.current = { setTool, setSelectedId, onError };
  const instantRef = useRef(instantBuild);
  instantRef.current = instantBuild;

  useEffect(() => {
    const canvas = canvasRef.current!;
    const ctx = canvas.getContext('2d')!;
    let raf = 0;
    let last = performance.now();
    let vw = 0, vh = 0;
    let initialized = false;

    const dpr = Math.min(2, window.devicePixelRatio || 1);
    const resize = () => {
      vw = canvas.clientWidth; vh = canvas.clientHeight;
      canvas.width = vw * dpr; canvas.height = vh * dpr;
      if (!initialized) {
        initialized = true;
        const z = camRef.current.z;
        camRef.current.x = vw / 2;
        camRef.current.y = vh / 2 - ((MAP_W + MAP_H) / 2) * 16 * z;
      }
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(canvas);

    const loop = (now: number) => {
      const dt = Math.min(120, now - last);
      last = now;
      engineRef.current.advance(dt);
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      uiRef.current.time = now;
      render(ctx, engineRef.current, camRef.current, uiRef.current, vw, vh);
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);

    // ---------- input ----------
    let panning = false;
    let painting = false;
    let downX = 0, downY = 0, lastX = 0, lastY = 0;
    let dragged = false;
    let lastPaintTile = '';

    const tileAt = (e: MouseEvent) => {
      const r = canvas.getBoundingClientRect();
      return screenToTile(e.clientX - r.left, e.clientY - r.top, camRef.current);
    };

    const paintAt = (e: MouseEvent) => {
      const t = tileAt(e);
      const key = `${t.x},${t.y}`;
      if (key === lastPaintTile) return;
      lastPaintTile = key;
      const eng = engineRef.current;
      const tl = uiRef.current.tool;
      if (tl.kind === 'build' && tl.defId === 'road') {
        eng.tryPlace('road', t.x, t.y, instantRef.current);
      } else if (tl.kind === 'bulldoze') {
        if (eng.bulldozeAt(t.x, t.y)) cbRef.current.setSelectedId(null);
      }
    };

    const onDown = (e: MouseEvent) => {
      if (e.button === 1 || e.button === 2) { panning = true; lastX = e.clientX; lastY = e.clientY; return; }
      if (e.button !== 0) return;
      downX = e.clientX; downY = e.clientY; lastX = e.clientX; lastY = e.clientY;
      dragged = false;
      const tl = uiRef.current.tool;
      const eng = engineRef.current;
      if (tl.kind === 'build') {
        const t = tileAt(e);
        if (tl.defId === 'road') { painting = true; lastPaintTile = ''; paintAt(e); }
        else {
          const res = eng.tryPlace(tl.defId, t.x, t.y, instantRef.current);
          if (!res.ok && res.reason) cbRef.current.onError(res.reason);
        }
      } else if (tl.kind === 'bulldoze') {
        painting = true; lastPaintTile = ''; paintAt(e);
      }
    };

    const onMove = (e: MouseEvent) => {
      const r = canvas.getBoundingClientRect();
      const sx = e.clientX - r.left, sy = e.clientY - r.top;
      uiRef.current.hoverTile = screenToTile(sx, sy, camRef.current);
      if (panning) {
        camRef.current.x += e.clientX - lastX;
        camRef.current.y += e.clientY - lastY;
        lastX = e.clientX; lastY = e.clientY;
        return;
      }
      if (e.buttons & 1) {
        const tl = uiRef.current.tool;
        if (painting) { paintAt(e); return; }
        if (tl.kind === 'select') {
          if (Math.abs(e.clientX - downX) + Math.abs(e.clientY - downY) > 6) dragged = true;
          if (dragged) {
            camRef.current.x += e.clientX - lastX;
            camRef.current.y += e.clientY - lastY;
          }
        }
        lastX = e.clientX; lastY = e.clientY;
      }
    };

    const onUp = (e: MouseEvent) => {
      if (e.button === 1 || e.button === 2) { panning = false; return; }
      if (e.button !== 0) return;
      const wasPainting = painting;
      painting = false;
      const tl = uiRef.current.tool;
      if (tl.kind === 'select' && !dragged && !wasPainting) {
        const r = canvas.getBoundingClientRect();
        const b = pickBuilding(engineRef.current, e.clientX - r.left, e.clientY - r.top, camRef.current);
        cbRef.current.setSelectedId(b ? b.id : null);
      }
    };

    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const cam = camRef.current;
      const r = canvas.getBoundingClientRect();
      const sx = e.clientX - r.left, sy = e.clientY - r.top;
      const z2 = Math.min(2.2, Math.max(0.35, cam.z * (e.deltaY < 0 ? 1.15 : 0.87)));
      cam.x = sx - (sx - cam.x) * (z2 / cam.z);
      cam.y = sy - (sy - cam.y) * (z2 / cam.z);
      cam.z = z2;
    };

    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') cbRef.current.setTool({ kind: 'select' });
      if (e.code === 'Space') {
        e.preventDefault();
        const eng = engineRef.current;
        eng.setSpeed(eng.speed === 0 ? 1 : 0);
      }
      if (e.key === '1') engineRef.current.setSpeed(1);
      if (e.key === '2') engineRef.current.setSpeed(2);
      if (e.key === '3') engineRef.current.setSpeed(4);
    };

    const onCtx = (e: Event) => e.preventDefault();

    canvas.addEventListener('mousedown', onDown);
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    canvas.addEventListener('wheel', onWheel, { passive: false });
    window.addEventListener('keydown', onKey);
    canvas.addEventListener('contextmenu', onCtx);

    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      canvas.removeEventListener('mousedown', onDown);
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
      canvas.removeEventListener('wheel', onWheel);
      window.removeEventListener('keydown', onKey);
      canvas.removeEventListener('contextmenu', onCtx);
    };
  }, []);

  return <canvas ref={canvasRef} className="absolute inset-0 h-full w-full cursor-crosshair" />;
}
