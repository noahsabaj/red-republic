// ============================================================
// Pointer/keyboard input controller — pure state machine, no DOM.
//
// GameCanvas adapts real PointerEvents into NormPointerEvents and
// injects callbacks; this class owns the gesture rules:
//  - gestures can only START from a canvas-originated pointerdown, so
//    drags that begin on overlaying UI never pan the camera
//  - painting suspends while the (captured) pointer is over UI
//  - two concurrent touches pinch-zoom about their midpoint
//  - hotkeys are gated so modals can disable them
// Being DOM-free makes every rule unit-testable.
// ============================================================

export type Tool = { kind: 'select' } | { kind: 'build'; defId: string } | { kind: 'bulldoze' };

export interface NormPointerEvent {
  x: number;          // canvas-local CSS px
  y: number;
  pointerId: number;
  isTouch: boolean;
  button: number;     // 0/1/2 on down/up; -1 on moves
  onCanvas: boolean;  // topmost element at this point is the canvas
}

export interface NormKeyEvent { key: string; code: string; repeat: boolean }

export interface InputCallbacks {
  getTool(): Tool;
  hotkeysEnabled(): boolean;
  panBy(dx: number, dy: number): void;
  zoomAt(sx: number, sy: number, factor: number): void;
  placeAt(sx: number, sy: number): void;  // build tool, non-road, single-shot
  beginPaint(): void;                     // reset paint dedupe before a stroke
  paintAt(sx: number, sy: number): void;  // road / bulldoze stroke step
  selectAt(sx: number, sy: number): void;
  setHover(sx: number, sy: number): void;
  clearHover(): void;
  cancelTool(): void;
  togglePause(): void;
  setSpeed(s: 1 | 2 | 4): void;
}

type Mode = 'idle' | 'pan' | 'maybeSelect' | 'paint' | 'pinch';

interface PointerInfo { x: number; y: number; startX: number; startY: number; isTouch: boolean }

// physical key positions (works on AZERTY etc.) → screen-space pan direction.
// Pressing W scrolls the view north: the map content moves down, i.e. the
// same panBy a downward MMB drag produces.
const PAN_KEYS: Record<string, readonly [number, number]> = {
  KeyW: [0, 1], KeyS: [0, -1], KeyA: [1, 0], KeyD: [-1, 0],
};
export const KEY_PAN_SPEED = 550; // screen px per second

export class InputController {
  private cb: InputCallbacks;
  private mode: Mode = 'idle';
  private pointers = new Map<number, PointerInfo>();
  private gestureId = -1;                       // pointer driving pan/maybeSelect/paint
  private pinchIds: [number, number] | null = null;
  private dragged = false;

  constructor(cb: InputCallbacks) {
    this.cb = cb;
  }

  pointerDown(e: NormPointerEvent): void {
    if (!e.onCanvas) return; // gestures never start on UI
    this.pointers.set(e.pointerId, { x: e.x, y: e.y, startX: e.x, startY: e.y, isTouch: e.isTouch });

    // second concurrent touch → pinch (whatever was in progress stops)
    if (e.isTouch) {
      const touches = [...this.pointers.entries()].filter(([, p]) => p.isTouch);
      if (touches.length === 2) {
        this.mode = 'pinch';
        this.pinchIds = [touches[0][0], touches[1][0]];
        this.gestureId = -1;
        this.cb.clearHover();
        return;
      }
    }
    if (this.mode !== 'idle') return;

    if (e.button === 1 || e.button === 2) {
      this.mode = 'pan';
      this.gestureId = e.pointerId;
      return;
    }
    if (e.button !== 0) return;

    const tool = this.cb.getTool();
    if (tool.kind === 'build' && tool.defId !== 'road') {
      this.cb.placeAt(e.x, e.y); // single-shot; no gesture
      return;
    }
    if ((tool.kind === 'build' && tool.defId === 'road') || tool.kind === 'bulldoze') {
      this.mode = 'paint';
      this.gestureId = e.pointerId;
      this.cb.beginPaint();
      this.cb.paintAt(e.x, e.y);
      return;
    }
    this.mode = 'maybeSelect';
    this.gestureId = e.pointerId;
    this.dragged = false;
  }

  pointerMove(e: NormPointerEvent): void {
    if (this.mode !== 'pinch') {
      if (e.onCanvas) this.cb.setHover(e.x, e.y);
      else this.cb.clearHover();
    }

    const p = this.pointers.get(e.pointerId);
    if (!p) return; // untracked pointer (e.g. drag that started on UI) — hover only

    if (this.mode === 'pinch' && this.pinchIds) {
      const [ia, ib] = this.pinchIds;
      if (e.pointerId !== ia && e.pointerId !== ib) return;
      const a = this.pointers.get(ia)!, b = this.pointers.get(ib)!;
      const prevMidX = (a.x + b.x) / 2, prevMidY = (a.y + b.y) / 2;
      const prevDist = Math.hypot(a.x - b.x, a.y - b.y);
      p.x = e.x; p.y = e.y;
      const midX = (a.x + b.x) / 2, midY = (a.y + b.y) / 2;
      const dist = Math.hypot(a.x - b.x, a.y - b.y);
      this.cb.panBy(midX - prevMidX, midY - prevMidY);
      if (prevDist > 1 && dist > 1) this.cb.zoomAt(midX, midY, dist / prevDist);
      return;
    }

    if (e.pointerId !== this.gestureId) { p.x = e.x; p.y = e.y; return; }

    if (this.mode === 'pan') {
      this.cb.panBy(e.x - p.x, e.y - p.y);
    } else if (this.mode === 'maybeSelect') {
      const threshold = p.isTouch ? 10 : 6;
      if (Math.abs(e.x - p.startX) + Math.abs(e.y - p.startY) > threshold) this.dragged = true;
      if (this.dragged) this.cb.panBy(e.x - p.x, e.y - p.y);
    } else if (this.mode === 'paint') {
      if (e.onCanvas) this.cb.paintAt(e.x, e.y); // suspended while over UI
    }
    p.x = e.x; p.y = e.y;
  }

  pointerUp(e: NormPointerEvent): void {
    const p = this.pointers.get(e.pointerId);
    if (!p) return;

    if (this.mode === 'pinch' && this.pinchIds) {
      this.pointers.delete(e.pointerId);
      const survivor = this.pinchIds.find(id => id !== e.pointerId && this.pointers.has(id));
      this.pinchIds = null;
      if (survivor !== undefined) {
        this.mode = 'pan'; // continue as a one-finger pan, re-anchored on the survivor
        this.gestureId = survivor;
      } else {
        this.mode = 'idle';
        this.gestureId = -1;
      }
      return;
    }

    if (e.pointerId === this.gestureId) {
      if (this.mode === 'maybeSelect' && !this.dragged) this.cb.selectAt(e.x, e.y);
      this.mode = 'idle';
      this.gestureId = -1;
    }
    this.pointers.delete(e.pointerId);
  }

  pointerCancel(e: NormPointerEvent): void {
    this.pointers.delete(e.pointerId);
    if (this.mode === 'pinch') {
      const remaining = this.pinchIds?.find(id => this.pointers.has(id));
      this.pinchIds = null;
      this.mode = remaining !== undefined ? 'pan' : 'idle';
      this.gestureId = remaining ?? -1;
    } else if (e.pointerId === this.gestureId) {
      this.mode = 'idle';
      this.gestureId = -1;
    }
  }

  pointerLeave(): void {
    this.cb.clearHover();
  }

  wheel(e: { x: number; y: number; deltaY: number }): void {
    this.cb.zoomAt(e.x, e.y, e.deltaY < 0 ? 1.15 : 0.87);
  }

  private heldPan = new Set<string>();

  /** Per-frame update — applies held-WASD panning. Call from the rAF loop. */
  tick(dtMs: number): void {
    if (this.heldPan.size === 0) return;
    if (!this.cb.hotkeysEnabled()) { this.heldPan.clear(); return; }
    let dx = 0, dy = 0;
    for (const code of this.heldPan) {
      const dir = PAN_KEYS[code];
      dx += dir[0]; dy += dir[1];
    }
    if (dx === 0 && dy === 0) return; // opposing keys cancel
    const v = (KEY_PAN_SPEED * dtMs) / 1000 / Math.hypot(dx, dy); // diagonals not faster
    this.cb.panBy(dx * v, dy * v);
  }

  keyUp(e: NormKeyEvent): void {
    this.heldPan.delete(e.code);
  }

  /** Returns true when the key was consumed (adapter should preventDefault). */
  key(e: NormKeyEvent): boolean {
    if (!this.cb.hotkeysEnabled()) return false;
    if (e.code in PAN_KEYS) { this.heldPan.add(e.code); return true; }
    if (e.key === 'Escape') { this.cb.cancelTool(); return true; }
    if (e.code === 'Space') {
      if (!e.repeat) this.cb.togglePause(); // auto-repeat must not rapid-toggle
      return true;
    }
    if (e.key === '1') { this.cb.setSpeed(1); return true; }
    if (e.key === '2') { this.cb.setSpeed(2); return true; }
    if (e.key === '3') { this.cb.setSpeed(4); return true; }
    return false;
  }

  /** Abort any gesture and held keys (unmount / window blur). */
  reset(): void {
    this.pointers.clear();
    this.pinchIds = null;
    this.mode = 'idle';
    this.gestureId = -1;
    this.dragged = false;
    this.heldPan.clear();
    this.cb.clearHover();
  }
}
