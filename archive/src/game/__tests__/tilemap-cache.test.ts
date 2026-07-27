import { afterEach, describe, expect, it, vi } from 'vitest';
import { GameEngine } from '../engine';
import type { Camera, FrameStyle, UIState } from '../render';
import { TilemapCache } from '../tilemap-cache';
import { CALM_WEATHER, flatMap } from './helpers';

const cam: Camera = { x: 100, y: 50, z: 1 };
const ui: UIState = { hoverTile: null, tool: { kind: 'select' }, selection: [], time: 0 };
const frame = {} as FrameStyle;

function makeEngine(width = 4, height = 4) {
  return new GameEngine({
    seed: 1,
    map: flatMap(width, height),
    skipStartingBase: true,
    weatherScript: CALM_WEATHER,
  });
}

function harness(dpr = 1) {
  const setTransform = vi.fn();
  const clearRect = vi.fn();
  const offscreenCtx = { setTransform, clearRect } as unknown as CanvasRenderingContext2D;
  const offscreen = {
    width: 0,
    height: 0,
    getContext: vi.fn(() => offscreenCtx),
  } as unknown as HTMLCanvasElement;
  const createElement = vi.fn(() => offscreen);
  vi.stubGlobal('document', { createElement });

  const drawImage = vi.fn();
  const save = vi.fn();
  const restore = vi.fn();
  const targetCtx = {
    drawImage,
    getTransform: vi.fn(() => ({ a: dpr, b: 0, c: 0, d: dpr })),
    save,
    restore,
  } as unknown as CanvasRenderingContext2D;
  const renderGroundTile = vi.fn();
  const cache = new TilemapCache();
  const draw = (
    engine: GameEngine,
    fieldTiles = new Set<number>(),
    viewportW = 800,
    viewportH = 600,
  ) => {
    cache.drawStaticLayer(
      targetCtx,
      engine,
      cam,
      ui,
      frame,
      renderGroundTile,
      fieldTiles,
      viewportW,
      viewportH,
    );
  };
  return {
    cache,
    clearRect,
    createElement,
    draw,
    drawImage,
    offscreen,
    renderGroundTile,
    setTransform,
    targetCtx,
  };
}

afterEach(() => vi.unstubAllGlobals());

describe('TilemapCache', () => {
  it('reuses an unchanged raster, but never shares it across engine identities', () => {
    const { draw, drawImage, renderGroundTile } = harness();
    const first = makeEngine();
    const second = makeEngine();

    draw(first);
    draw(first);
    expect(renderGroundTile).toHaveBeenCalledTimes(16);

    draw(second);
    expect(renderGroundTile).toHaveBeenCalledTimes(32);
    expect(drawImage).toHaveBeenCalledTimes(3);
  });

  it('rebuilds for a visual tile mutation that leaves routing revisions unchanged', () => {
    const { draw, renderGroundTile } = harness();
    const engine = makeEngine();
    draw(engine);
    const landRevision = engine.topologyRevision('land');

    engine.applyTilePatches([{ x: 1, y: 1, terrain: 'forest' }]);
    expect(engine.topologyRevision('land')).toBe(landRevision);
    draw(engine);

    expect(renderGroundTile).toHaveBeenCalledTimes(32);
  });

  it('does not rerasterize after an unrelated observable engine change', () => {
    const { draw, renderGroundTile } = harness();
    const engine = makeEngine();
    draw(engine);

    engine.setForeignLaborEnabled(false);
    draw(engine);

    expect(renderGroundTile).toHaveBeenCalledTimes(16);
  });

  it('invalidates explicitly', () => {
    const { cache, draw, renderGroundTile } = harness();
    const engine = makeEngine();
    draw(engine);
    cache.invalidate();
    draw(engine);
    expect(renderGroundTile).toHaveBeenCalledTimes(32);
  });

  it('rasterizes and blits the whole-map cache at the target DPR', () => {
    const { draw, drawImage, offscreen, setTransform } = harness(2);

    draw(makeEngine());

    expect(offscreen.width).toBe(576);
    expect(offscreen.height).toBe(320);
    expect(setTransform).toHaveBeenCalledWith(2, 0, 0, 2, 0, 0);
    expect(drawImage).toHaveBeenCalledWith(offscreen, -44, 34, 288, 160);
  });

  it('avoids an oversized whole-map allocation and draws only visible tiles directly', () => {
    const { createElement, draw, drawImage, renderGroundTile, targetCtx } = harness(2);
    const engine = makeEngine(128, 128);

    draw(engine, new Set(), 200, 100);

    expect(createElement).not.toHaveBeenCalled();
    expect(drawImage).not.toHaveBeenCalled();
    expect(renderGroundTile).toHaveBeenCalled();
    expect(renderGroundTile.mock.calls.length).toBeLessThan(engine.mapW * engine.mapH);
    expect(renderGroundTile.mock.calls.some((call) => call[0] === targetCtx && call[2] === 0 && call[3] === 0)).toBe(true);
    expect(renderGroundTile.mock.calls.some((call) => call[2] === 127 && call[3] === 127)).toBe(false);
  });
});
