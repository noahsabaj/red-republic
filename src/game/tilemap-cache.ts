// ============================================================
// Offscreen Static Tilemap Canvas Cache
// ============================================================
import type { GameEngine } from './engine';
import type { Camera, FrameStyle, UIState } from './render';

const TILE_W = 64;
const TILE_H = 32;

export class TilemapCache {
  private canvas: HTMLCanvasElement | null = null;
  private ctx: CanvasRenderingContext2D | null = null;
  private key: string = '';
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
  ) {
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

    const currentKey = `${mapW}_${mapH}_${roadRev}_${landRev}_${waterRev}_${season}_${snowDepth}_${riverFrozen}_${showGrid}_${camZ}`;

    if (this.key !== currentKey || !this.canvas || !this.ctx) {
      this.rebuild(engine, camZ, showGrid, frame, renderGroundTile, fieldTiles, currentKey);
    }

    if (this.canvas) {
      const destX = Math.round(cam.x - this.originX);
      const destY = Math.round(cam.y - this.originY);
      targetCtx.drawImage(this.canvas, destX, destY);
    }
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
  ) {
    const mapW = engine.mapW;
    const mapH = engine.mapH;
    const hwz = (TILE_W / 2) * camZ;
    const hhz = (TILE_H / 2) * camZ;

    this.bufferW = Math.ceil((mapW + mapH) * hwz + 32 * camZ);
    this.bufferH = Math.ceil((mapW + mapH) * hhz + 32 * camZ);
    this.originX = mapH * hwz + 16 * camZ;
    this.originY = 16 * camZ;

    if (!this.canvas) {
      this.canvas = document.createElement('canvas');
    }
    this.canvas.width = this.bufferW;
    this.canvas.height = this.bufferH;
    this.ctx = this.canvas.getContext('2d');
    if (!this.ctx) return;

    const virtualCam: Camera = { x: this.originX, y: this.originY, z: camZ };

    for (let y = 0; y < mapH; y++) {
      for (let x = 0; x < mapW; x++) {
        renderGroundTile(this.ctx, engine, x, y, virtualCam, frame, showGrid, fieldTiles);
      }
    }

    this.key = key;
  }
}
