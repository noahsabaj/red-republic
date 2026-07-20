import { useEffect, useRef } from 'react';
import type { GameEngine } from '@/game/engine';
import { render, screenToTile, pickBuilding, STATUS_PALETTES, type Camera, type UIState } from '@/game/render';
import { InputController, type NormPointerEvent, type Tool } from '@/game/input';
import type { SelectionItem } from '@/game/selection';
import { getSettings, subscribeSettings } from '@/app/settings';
import { audio } from '@/audio';

export type { SelectionItem, Tool };

/** The build-menu placement defaults stamped onto each new site. The foreign-labor
 *  default lives in engine.foreignLaborEnabled; this is a valid engine PlacePolicy. */
export interface BuildPolicy { autoBuy: boolean; currency: 'east' | 'west'; instant: boolean; plan: boolean }

interface Props {
  engine: GameEngine;
  tool: Tool;
  setTool: (t: Tool) => void;
  selection: SelectionItem[];
  /** item = null means empty ground was clicked */
  onSelect: (item: SelectionItem | null, additive: boolean) => void;
  policy: BuildPolicy;
  hotkeysEnabled: boolean;
  onError: (msg: string) => void;
  /** Escape with no tool armed — App opens the pause menu. */
  onOpenMenu: () => void;
}

export default function GameCanvas({ engine, tool, setTool, selection, onSelect, policy, hotkeysEnabled, onError, onOpenMenu }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const camRef = useRef<Camera>({ x: 0, y: 0, z: 0.8 });
  const uiRef = useRef<UIState>({ hoverTile: null, tool, selection, time: 0 });
  const engineRef = useRef(engine);
  const cbRef = useRef({ setTool, onSelect, onError, onOpenMenu });
  const policyRef = useRef(policy);
  const hotkeysRef = useRef(hotkeysEnabled);

  // mirror props into refs after render (writing refs during render is
  // illegal under React 19 concurrent rendering)
  useEffect(() => {
    uiRef.current.tool = tool;
    uiRef.current.selection = selection;
    engineRef.current = engine;
    cbRef.current = { setTool, onSelect, onError, onOpenMenu };
    policyRef.current = policy;
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
      // re-read every time: monitor moves / zoom / the sharpness setting change it
      dpr = Math.min(getSettings().dprCap, window.devicePixelRatio || 1);
      vw = canvas.clientWidth; vh = canvas.clientHeight;
      canvas.width = Math.round(vw * dpr);
      canvas.height = Math.round(vh * dpr);
      if (!initialized && vw > 0) {
        initialized = true;
        const eng = engineRef.current;
        const z = camRef.current.z;
        camRef.current.x = vw / 2;
        camRef.current.y = vh / 2 - ((eng.mapW + eng.mapH) / 2) * 16 * z;
      }
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(canvas);
    const unsubSettings = subscribeSettings(resize); // dprCap changes re-rasterize

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
      ctrl.tick(dt); // held-key (WASD) and edge panning
      engineRef.current.advance(dt);
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      const ui = uiRef.current;
      const s = getSettings();
      ui.time = now;
      ui.showGrid = s.showGrid;
      ui.reducedMotion = s.reducedMotion;
      ui.palette = s.colorblind ? STATUS_PALETTES.colorblind : STATUS_PALETTES.default;
      render(ctx, engineRef.current, camRef.current, ui, vw, vh);
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);

    // ---------- input ----------
    let lastPaintTile = '';
    const tileOf = (sx: number, sy: number) => screenToTile(sx, sy, camRef.current);

    const ctrl = new InputController({
      getTool: () => uiRef.current.tool,
      hotkeysEnabled: () => hotkeysRef.current,
      getViewport: () => ({ w: vw, h: vh }),
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
        const res = engineRef.current.tryPlace(tl.defId, t.x, t.y, policyRef.current);
        audio.sfx(res.ok ? 'buildPlace' : 'error');
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
          if (eng.tryPlace('road', t.x, t.y, policyRef.current).ok) audio.sfx('roadPaint');
        } else if (tl.kind === 'bulldoze') {
          if (eng.bulldozeAt(t.x, t.y)) {
            audio.sfx('bulldoze');
            cbRef.current.onSelect(null, false);
          }
        }
      },
      selectAt: (sx, sy, additive) => {
        const eng = engineRef.current;
        const b = pickBuilding(eng, sx, sy, camRef.current);
        if (b) {
          audio.ui('select');
          cbRef.current.onSelect({ kind: 'building', id: b.id }, additive);
          return;
        }
        // bare ground: deposit tiles are inspectable too
        const t = tileOf(sx, sy);
        const tile = eng.tiles[t.y]?.[t.x];
        if (tile?.deposit && !tile.buildingId) {
          audio.ui('select');
          cbRef.current.onSelect({ kind: 'deposit', x: t.x, y: t.y }, additive);
          return;
        }
        cbRef.current.onSelect(null, additive);
      },
      setHover: (sx, sy) => { uiRef.current.hoverTile = tileOf(sx, sy); },
      clearHover: () => { uiRef.current.hoverTile = null; },
      // setTool flows through App's setToolSfx funnel → toolCancel on disarm
      cancelTool: () => cbRef.current.setTool({ kind: 'select' }),
      toggleBulldoze: () => cbRef.current.setTool(uiRef.current.tool.kind === 'bulldoze' ? { kind: 'select' } : { kind: 'bulldoze' }),
      openMenu: () => { audio.ui('open'); cbRef.current.onOpenMenu(); },
      togglePause: () => { audio.ui('speed'); engineRef.current.togglePause(); },
      setSpeed: (s) => { audio.ui('speed'); engineRef.current.setSpeed(s); },
    }, getSettings); // Settings is a structural superset of InputOptions

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
        additive: e.shiftKey || e.ctrlKey || e.metaKey,
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
    const onKeyUp = (e: KeyboardEvent) => ctrl.keyUp({ key: e.key, code: e.code, repeat: false });
    const onBlur = () => ctrl.reset(); // keyup can be missed while unfocused — no stuck keys
    const onCtx = (e: Event) => e.preventDefault();

    if (import.meta.env.DEV) {
      // debug hook for the console and automated verification
      (window as unknown as Record<string, unknown>).__redRepublic = {
        engine: engineRef.current,
        cam: camRef.current,
        ctrl,
      };
    }

    canvas.addEventListener('pointerdown', onDown);
    canvas.addEventListener('pointermove', onMove);
    canvas.addEventListener('pointerup', onUp);
    canvas.addEventListener('pointercancel', onCancel);
    canvas.addEventListener('pointerleave', onLeave);
    canvas.addEventListener('wheel', onWheel, { passive: false });
    window.addEventListener('keydown', onKey);
    window.addEventListener('keyup', onKeyUp);
    window.addEventListener('blur', onBlur);
    canvas.addEventListener('contextmenu', onCtx);

    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      unsubSettings();
      mq?.removeEventListener('change', onDprChange);
      canvas.removeEventListener('pointerdown', onDown);
      canvas.removeEventListener('pointermove', onMove);
      canvas.removeEventListener('pointerup', onUp);
      canvas.removeEventListener('pointercancel', onCancel);
      canvas.removeEventListener('pointerleave', onLeave);
      canvas.removeEventListener('wheel', onWheel);
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('keyup', onKeyUp);
      window.removeEventListener('blur', onBlur);
      canvas.removeEventListener('contextmenu', onCtx);
      ctrl.reset();
    };
  }, []);

  // touch-action: none is load-bearing — without it the browser hijacks touch
  // drags for scrolling and fires pointercancel mid-gesture
  return <canvas ref={canvasRef} className="absolute inset-0 h-full w-full cursor-crosshair touch-none" />;
}
