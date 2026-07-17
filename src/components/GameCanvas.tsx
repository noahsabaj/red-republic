import { useEffect, useRef } from 'react';
import type { GameEngine } from '@/game/engine';
import { render, screenToTile, pickBuilding, type Camera, type Selection, type UIState } from '@/game/render';
import { InputController, type NormPointerEvent, type Tool } from '@/game/input';
import { MAP_W, MAP_H } from '@/game/mapgen';

export type { Selection, Tool };

interface Props {
  engine: GameEngine;
  tool: Tool;
  setTool: (t: Tool) => void;
  selected: Selection;
  setSelected: (s: Selection) => void;
  instantBuild: boolean;
  hotkeysEnabled: boolean;
  onError: (msg: string) => void;
}

export default function GameCanvas({ engine, tool, setTool, selected, setSelected, instantBuild, hotkeysEnabled, onError }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const camRef = useRef<Camera>({ x: 0, y: 0, z: 0.8 });
  const uiRef = useRef<UIState>({ hoverTile: null, tool, selection: selected, time: 0 });
  const engineRef = useRef(engine);
  const cbRef = useRef({ setTool, setSelected, onError });
  const instantRef = useRef(instantBuild);
  const hotkeysRef = useRef(hotkeysEnabled);

  // mirror props into refs after render (writing refs during render is
  // illegal under React 19 concurrent rendering)
  useEffect(() => {
    uiRef.current.tool = tool;
    uiRef.current.selection = selected;
    engineRef.current = engine;
    cbRef.current = { setTool, setSelected, onError };
    instantRef.current = instantBuild;
    hotkeysRef.current = hotkeysEnabled;
  });

  useEffect(() => {
    const canvas = canvasRef.current!;
    const ctx = canvas.getContext('2d')!;
    let raf = 0;
    let last = performance.now();
    let vw = 0, vh = 0;
    let dpr = 1;
    let initialized = false;

    const resize = () => {
      dpr = Math.min(2, window.devicePixelRatio || 1); // re-read: monitor moves / zoom change it
      vw = canvas.clientWidth; vh = canvas.clientHeight;
      canvas.width = Math.round(vw * dpr);
      canvas.height = Math.round(vh * dpr);
      if (!initialized && vw > 0) {
        initialized = true;
        const z = camRef.current.z;
        camRef.current.x = vw / 2;
        camRef.current.y = vh / 2 - ((MAP_W + MAP_H) / 2) * 16 * z;
      }
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(canvas);

    // browser zoom changes clientWidth (ResizeObserver fires), but moving the
    // window to another monitor only changes devicePixelRatio — watch it too
    let mq: MediaQueryList | null = null;
    function onDprChange() { resize(); watchDpr(); }
    function watchDpr() {
      mq?.removeEventListener('change', onDprChange);
      mq = window.matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
      mq.addEventListener('change', onDprChange);
    }
    watchDpr();

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
    let lastPaintTile = '';
    const tileOf = (sx: number, sy: number) => screenToTile(sx, sy, camRef.current);

    const ctrl = new InputController({
      getTool: () => uiRef.current.tool,
      hotkeysEnabled: () => hotkeysRef.current,
      panBy: (dx, dy) => { camRef.current.x += dx; camRef.current.y += dy; },
      zoomAt: (sx, sy, factor) => {
        const cam = camRef.current;
        const z2 = Math.min(2.2, Math.max(0.35, cam.z * factor));
        cam.x = sx - (sx - cam.x) * (z2 / cam.z);
        cam.y = sy - (sy - cam.y) * (z2 / cam.z);
        cam.z = z2;
      },
      placeAt: (sx, sy) => {
        const tl = uiRef.current.tool;
        if (tl.kind !== 'build') return;
        const t = tileOf(sx, sy);
        const res = engineRef.current.tryPlace(tl.defId, t.x, t.y, instantRef.current);
        if (!res.ok && res.reason) cbRef.current.onError(res.reason);
      },
      beginPaint: () => { lastPaintTile = ''; },
      paintAt: (sx, sy) => {
        const t = tileOf(sx, sy);
        const key = `${t.x},${t.y}`;
        if (key === lastPaintTile) return;
        lastPaintTile = key;
        const tl = uiRef.current.tool;
        const eng = engineRef.current;
        if (tl.kind === 'build' && tl.defId === 'road') {
          eng.tryPlace('road', t.x, t.y, instantRef.current);
        } else if (tl.kind === 'bulldoze') {
          if (eng.bulldozeAt(t.x, t.y)) cbRef.current.setSelected(null);
        }
      },
      selectAt: (sx, sy) => {
        const eng = engineRef.current;
        const b = pickBuilding(eng, sx, sy, camRef.current);
        if (b) {
          cbRef.current.setSelected({ kind: 'building', id: b.id });
          return;
        }
        // bare ground: deposit tiles are inspectable too
        const t = tileOf(sx, sy);
        const tile = eng.tiles[t.y]?.[t.x];
        if (tile?.deposit && !tile.buildingId) {
          cbRef.current.setSelected({ kind: 'deposit', x: t.x, y: t.y });
          return;
        }
        cbRef.current.setSelected(null);
      },
      setHover: (sx, sy) => { uiRef.current.hoverTile = tileOf(sx, sy); },
      clearHover: () => { uiRef.current.hoverTile = null; },
      cancelTool: () => cbRef.current.setTool({ kind: 'select' }),
      togglePause: () => engineRef.current.togglePause(),
      setSpeed: (s) => engineRef.current.setSpeed(s),
    });

    const norm = (e: PointerEvent): NormPointerEvent => {
      const r = canvas.getBoundingClientRect();
      // elementFromPoint sees overlaying UI even while the pointer is captured
      const el = document.elementFromPoint(e.clientX, e.clientY);
      return {
        x: e.clientX - r.left,
        y: e.clientY - r.top,
        pointerId: e.pointerId,
        isTouch: e.pointerType === 'touch',
        button: e.button,
        onCanvas: el === canvas,
      };
    };

    const onDown = (e: PointerEvent) => {
      try {
        canvas.setPointerCapture(e.pointerId);
      } catch {
        // pointer already released (or synthetic) — gesture still works, uncaptured
      }
      ctrl.pointerDown(norm(e));
    };
    const onMove = (e: PointerEvent) => ctrl.pointerMove(norm(e));
    const onUp = (e: PointerEvent) => ctrl.pointerUp(norm(e));
    const onCancel = (e: PointerEvent) => ctrl.pointerCancel(norm(e));
    const onLeave = () => ctrl.pointerLeave();
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const r = canvas.getBoundingClientRect();
      ctrl.wheel({ x: e.clientX - r.left, y: e.clientY - r.top, deltaY: e.deltaY });
    };
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)) return;
      if (ctrl.key({ key: e.key, code: e.code, repeat: e.repeat })) e.preventDefault();
    };
    const onCtx = (e: Event) => e.preventDefault();

    if (import.meta.env.DEV) {
      // debug hook for the console and automated verification
      (window as unknown as Record<string, unknown>).__redRepublic = {
        engine: engineRef.current,
        cam: camRef.current,
      };
    }

    canvas.addEventListener('pointerdown', onDown);
    canvas.addEventListener('pointermove', onMove);
    canvas.addEventListener('pointerup', onUp);
    canvas.addEventListener('pointercancel', onCancel);
    canvas.addEventListener('pointerleave', onLeave);
    canvas.addEventListener('wheel', onWheel, { passive: false });
    window.addEventListener('keydown', onKey);
    canvas.addEventListener('contextmenu', onCtx);

    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      mq?.removeEventListener('change', onDprChange);
      canvas.removeEventListener('pointerdown', onDown);
      canvas.removeEventListener('pointermove', onMove);
      canvas.removeEventListener('pointerup', onUp);
      canvas.removeEventListener('pointercancel', onCancel);
      canvas.removeEventListener('pointerleave', onLeave);
      canvas.removeEventListener('wheel', onWheel);
      window.removeEventListener('keydown', onKey);
      canvas.removeEventListener('contextmenu', onCtx);
      ctrl.reset();
    };
  }, []);

  // touch-action: none is load-bearing — without it the browser hijacks touch
  // drags for scrolling and fires pointercancel mid-gesture
  return <canvas ref={canvasRef} className="absolute inset-0 h-full w-full cursor-crosshair touch-none" />;
}
