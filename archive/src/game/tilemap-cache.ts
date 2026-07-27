// ============================================================
// Offscreen Static Tilemap Canvas Cache
// ============================================================
import type { GameEngine } from './engine';
import type { Camera, FrameStyle, UIState } from './render';

const TILE_W = 64;
const TILE_H = 32;

/**
 * Conservative cross-browser backing-store limits. The pixel cap bounds a
 * whole-map RGBA buffer to roughly 64 MiB before browser/GPU overhead.
 */
const MAX_CANVAS_DIMENSION = 8_192;
const MAX_CANVAS_PIXELS = 16_777_216;

interface RasterSize {
  scale: number;
  width: number;
  height: number;
}

export class TilemapCache {
  private canvas: HTMLCanvasElement | null = null;
  private ctx: CanvasRenderingContext2D | null = null;
  private key: string = '';
  private engine: GameEngine | null = null;
  private engineVersion: number = -1;
  private staticContentKey: string = '';
  private originX: number = 0;
  private originY: number = 0;
  private bufferW: number = 0;
  private bufferH: number = 0;

  /** Invalidate the static offscreen cache. */
  invalidate() {
    this.key = '';
  }

  /**
   * Ensure offscreen canvas holds fresh static ground layer and render it onto target ctx.
   * Static layer includes: water, grass, rock, forest ground, grid lattice, farm fields, deposits, roads, and border lines.
   */
  drawStaticLayer(
    targetCtx: CanvasRenderingContext2D,
    engine: GameEngine,
    cam: Camera,
    ui: UIState,
    frame: FrameStyle,
    renderGroundTile: (
      offCtx: CanvasRenderingContext2D,
      engine: GameEngine,
      x: number,
      y: number,
      virtualCam: Camera,
      frame: FrameStyle,
      showGrid: boolean,
      fieldTiles: Set<number>,
    ) => void,
    fieldTiles: Set<number>,
    viewportW: number,
    viewportH: number,
  ) {
    const engineVersion = engine.getVersion();
    if (this.engine !== engine || this.engineVersion !== engineVersion) {
      if (this.engine !== engine) this.key = '';
      this.engine = engine;
      this.engineVersion = engineVersion;
      this.staticContentKey = this.contentKey(engine, fieldTiles);
    }

    const mapW = engine.mapW;
    const mapH = engine.mapH;
    const roadRev = engine.topologyRevision('road');
    const landRev = engine.topologyRevision('land');
    const waterRev = engine.topologyRevision('water');
    const season = engine.season();
    const snowDepth = engine.weather.snowDepth;
    const riverFrozen = engine.weather.riverFrozen;
    const showGrid = ui.showGrid ?? false;
    const camZ = cam.z;
    const logicalSize = this.logicalSize(engine, camZ);
    const rasterSize = this.safeRasterSize(
      logicalSize.width,
      logicalSize.height,
      this.effectiveDpr(targetCtx),
    );

    // A 1x whole-map backing store would exceed a conservative browser-safe
    // limit. Draw only intersecting tiles into the DPR-scaled target instead;
    // this avoids a doomed allocation and preserves native display sharpness.
    if (!rasterSize) {
      this.releaseCanvas();
      this.drawVisibleTiles(
        targetCtx,
        engine,
        cam,
        frame,
        showGrid,
        renderGroundTile,
        fieldTiles,
        viewportW,
        viewportH,
      );
      return;
    }

    const currentKey = `${mapW}_${mapH}_${roadRev}_${landRev}_${waterRev}_${season}_${snowDepth}_${riverFrozen}_${showGrid}_${camZ}_${rasterSize.scale}_${rasterSize.width}_${rasterSize.height}_${this.staticContentKey}`;

    if (this.key !== currentKey || !this.canvas || !this.ctx) {
      this.rebuild(
        engine,
        camZ,
        showGrid,
        frame,
        renderGroundTile,
        fieldTiles,
        currentKey,
        logicalSize,
        rasterSize,
      );
    }

    if (this.canvas) {
      const destX = Math.round(cam.x - this.originX);
      const destY = Math.round(cam.y - this.originY);
      targetCtx.drawImage(this.canvas, destX, destY, this.bufferW, this.bufferH);
    }
  }

  private logicalSize(engine: GameEngine, camZ: number) {
    const hwz = (TILE_W / 2) * camZ;
    const hhz = (TILE_H / 2) * camZ;
    return {
      width: Math.ceil((engine.mapW + engine.mapH) * hwz + 32 * camZ),
      height: Math.ceil((engine.mapW + engine.mapH) * hhz + 32 * camZ),
    };
  }

  private effectiveDpr(ctx: CanvasRenderingContext2D): number {
    const transform = ctx.getTransform();
    const scaleX = Math.hypot(transform.a, transform.b);
    const scaleY = Math.hypot(transform.c, transform.d);
    const scale = Math.max(scaleX, scaleY);
    return Number.isFinite(scale) && scale > 1 ? scale : 1;
  }

  private safeRasterSize(logicalW: number, logicalH: number, targetScale: number): RasterSize | null {
    const at = (scale: number): RasterSize => ({
      scale,
      width: Math.ceil(logicalW * scale),
      height: Math.ceil(logicalH * scale),
    });
    const isSafe = ({ width, height }: RasterSize) => (
      width <= MAX_CANVAS_DIMENSION
      && height <= MAX_CANVAS_DIMENSION
      && width * height <= MAX_CANVAS_PIXELS
    );

    const oneX = at(1);
    if (!isSafe(oneX)) return null;

    const requested = at(targetScale);
    if (isSafe(requested)) return requested;

    // Retain as much display density as the caps allow when 1x is safe but the
    // target DPR is not. Binary search accounts for integer ceil boundaries.
    let low = 1;
    let high = targetScale;
    for (let i = 0; i < 24; i++) {
      const mid = (low + high) / 2;
      if (isSafe(at(mid))) low = mid;
      else high = mid;
    }
    return at(low);
  }

  private releaseCanvas() {
    this.canvas = null;
    this.ctx = null;
    this.key = '';
  }

  private drawVisibleTiles(
    targetCtx: CanvasRenderingContext2D,
    engine: GameEngine,
    cam: Camera,
    frame: FrameStyle,
    showGrid: boolean,
    renderGroundTile: (
      offCtx: CanvasRenderingContext2D,
      engine: GameEngine,
      x: number,
      y: number,
      virtualCam: Camera,
      frame: FrameStyle,
      showGrid: boolean,
      fieldTiles: Set<number>,
    ) => void,
    fieldTiles: Set<number>,
    viewportW: number,
    viewportH: number,
  ) {
    if (viewportW <= 0 || viewportH <= 0) return;

    const hwz = (TILE_W / 2) * cam.z;
    const tileH = TILE_H * cam.z;
    const margin = Math.max(4, 4 * cam.z);
    targetCtx.save();
    for (let y = 0; y < engine.mapH; y++) {
      for (let x = 0; x < engine.mapW; x++) {
        const topX = (x - y) * hwz + cam.x;
        const topY = (x + y) * (TILE_H / 2) * cam.z + cam.y;
        if (
          topX + hwz + margin < 0
          || topX - hwz - margin > viewportW
          || topY + tileH + margin < 0
          || topY - margin > viewportH
        ) continue;
        renderGroundTile(targetCtx, engine, x, y, cam, frame, showGrid, fieldTiles);
      }
    }
    targetCtx.restore();
  }

  /**
   * Exact pixel-input signature for the cached layer. Routing revisions are not
   * sufficient: forest/grass swaps, deposits, variants, and field membership
   * can change without changing any routing cost. Recompute this only after an
   * observable engine bump, then reuse the raster if the pixel inputs match.
   */
  private contentKey(engine: GameEngine, fieldTiles: Set<number>): string {
    const cells = new Array<string>(engine.mapW * engine.mapH);
    let i = 0;
    for (let y = 0; y < engine.mapH; y++) {
      for (let x = 0; x < engine.mapW; x++, i++) {
        const tile = engine.tiles[y][x];
        cells[i] = `${tile.terrain}|${tile.road ? 1 : 0}|${tile.foreign ? 1 : 0}|${tile.deposit ?? ''}|${tile.variant}|${fieldTiles.has(i) ? 1 : 0}`;
      }
    }
    return cells.join(';');
  }

  private rebuild(
    engine: GameEngine,
    camZ: number,
    showGrid: boolean,
    frame: FrameStyle,
    renderGroundTile: (
      offCtx: CanvasRenderingContext2D,
      engine: GameEngine,
      x: number,
      y: number,
      virtualCam: Camera,
      frame: FrameStyle,
      showGrid: boolean,
      fieldTiles: Set<number>,
    ) => void,
    fieldTiles: Set<number>,
    key: string,
    logicalSize: { width: number; height: number },
    rasterSize: RasterSize,
  ) {
    const mapW = engine.mapW;
    const mapH = engine.mapH;
    const hwz = (TILE_W / 2) * camZ;

    this.bufferW = logicalSize.width;
    this.bufferH = logicalSize.height;
    this.originX = mapH * hwz + 16 * camZ;
    this.originY = 16 * camZ;

    if (!this.canvas) {
      this.canvas = document.createElement('canvas');
    }
    if (this.canvas.width !== rasterSize.width) this.canvas.width = rasterSize.width;
    if (this.canvas.height !== rasterSize.height) this.canvas.height = rasterSize.height;
    this.ctx = this.canvas.getContext('2d');
    if (!this.ctx) return;
    this.ctx.setTransform(rasterSize.scale, 0, 0, rasterSize.scale, 0, 0);
    this.ctx.clearRect(0, 0, this.bufferW, this.bufferH);

    const virtualCam: Camera = { x: this.originX, y: this.originY, z: camZ };

    for (let y = 0; y < mapH; y++) {
      for (let x = 0; x < mapW; x++) {
        renderGroundTile(this.ctx, engine, x, y, virtualCam, frame, showGrid, fieldTiles);
      }
    }

    this.key = key;
  }
}
