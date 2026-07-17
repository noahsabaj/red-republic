import { beforeEach, describe, expect, it } from 'vitest';
import { InputController, KEY_PAN_SPEED, type InputCallbacks, type NormPointerEvent, type Tool } from '../input';

interface Log { calls: [string, ...unknown[]][] }

function makeCtrl(overrides: { tool?: Tool; hotkeys?: boolean } = {}) {
  const log: Log = { calls: [] };
  let tool: Tool = overrides.tool ?? { kind: 'select' };
  let hotkeys = overrides.hotkeys ?? true;
  const cb: InputCallbacks = {
    getTool: () => tool,
    hotkeysEnabled: () => hotkeys,
    panBy: (dx, dy) => log.calls.push(['panBy', dx, dy]),
    zoomAt: (x, y, f) => log.calls.push(['zoomAt', x, y, f]),
    placeAt: (x, y) => log.calls.push(['placeAt', x, y]),
    beginPaint: () => log.calls.push(['beginPaint']),
    paintAt: (x, y) => log.calls.push(['paintAt', x, y]),
    selectAt: (x, y, additive) => log.calls.push(['selectAt', x, y, additive]),
    setHover: (x, y) => log.calls.push(['setHover', x, y]),
    clearHover: () => log.calls.push(['clearHover']),
    cancelTool: () => log.calls.push(['cancelTool']),
    togglePause: () => log.calls.push(['togglePause']),
    setSpeed: (s) => log.calls.push(['setSpeed', s]),
  };
  const ctrl = new InputController(cb);
  return {
    ctrl, log,
    setTool: (t: Tool) => { tool = t; },
    setHotkeys: (v: boolean) => { hotkeys = v; },
    names: () => log.calls.map(c => c[0]),
    of: (name: string) => log.calls.filter(c => c[0] === name),
  };
}

const ev = (partial: Partial<NormPointerEvent>): NormPointerEvent => ({
  x: 0, y: 0, pointerId: 1, isTouch: false, button: -1, onCanvas: true, additive: false, ...partial,
});

let t: ReturnType<typeof makeCtrl>;
beforeEach(() => { t = makeCtrl(); });

describe('UI-originated drags (the camera-teleport bug)', () => {
  it('a move with no tracked pointerdown never pans', () => {
    // simulates dragging a HUD button: pointerdown hit the button, not the canvas
    t.ctrl.pointerMove(ev({ x: 500, y: 300 }));
    t.ctrl.pointerMove(ev({ x: 900, y: 100 }));
    expect(t.of('panBy')).toHaveLength(0);
  });

  it('a pointerdown on UI is ignored entirely', () => {
    t.ctrl.pointerDown(ev({ x: 10, y: 10, button: 0, onCanvas: false }));
    t.ctrl.pointerMove(ev({ x: 200, y: 200 }));
    expect(t.of('panBy')).toHaveLength(0);
    expect(t.of('selectAt')).toHaveLength(0);
  });
});

describe('select vs drag', () => {
  it('a canvas drag past the threshold pans and does not select', () => {
    t.ctrl.pointerDown(ev({ x: 100, y: 100, button: 0 }));
    t.ctrl.pointerMove(ev({ x: 110, y: 100 }));
    t.ctrl.pointerMove(ev({ x: 130, y: 105 }));
    t.ctrl.pointerUp(ev({ x: 130, y: 105, button: 0 }));
    expect(t.of('panBy').length).toBeGreaterThan(0);
    expect(t.of('panBy')[1]).toEqual(['panBy', 20, 5]);
    expect(t.of('selectAt')).toHaveLength(0);
  });

  it('a tap (sub-threshold) selects and never pans', () => {
    t.ctrl.pointerDown(ev({ x: 100, y: 100, button: 0 }));
    t.ctrl.pointerMove(ev({ x: 102, y: 101 }));
    t.ctrl.pointerUp(ev({ x: 102, y: 101, button: 0 }));
    expect(t.of('panBy')).toHaveLength(0);
    expect(t.of('selectAt')).toEqual([['selectAt', 102, 101, false]]);
  });

  it('shift/ctrl taps pass the additive flag through', () => {
    t.ctrl.pointerDown(ev({ x: 50, y: 50, button: 0, additive: true }));
    t.ctrl.pointerUp(ev({ x: 50, y: 50, button: 0, additive: true }));
    expect(t.of('selectAt')).toEqual([['selectAt', 50, 50, true]]);
  });
});

describe('painting', () => {
  beforeEach(() => { t = makeCtrl({ tool: { kind: 'build', defId: 'road' } }); });

  it('paints on down and while moving on-canvas, suspends over UI', () => {
    t.ctrl.pointerDown(ev({ x: 10, y: 10, button: 0 }));
    expect(t.names()).toEqual(['beginPaint', 'paintAt']);
    t.ctrl.pointerMove(ev({ x: 20, y: 10 }));
    expect(t.of('paintAt')).toHaveLength(2);
    t.ctrl.pointerMove(ev({ x: 40, y: 10, onCanvas: false })); // captured, but over a panel
    expect(t.of('paintAt')).toHaveLength(2); // no paint under UI
    t.ctrl.pointerMove(ev({ x: 60, y: 10 }));
    expect(t.of('paintAt')).toHaveLength(3); // resumes back on canvas
  });

  it('a non-road build tool places once on down, no gesture', () => {
    const b = makeCtrl({ tool: { kind: 'build', defId: 'house' } });
    b.ctrl.pointerDown(ev({ x: 15, y: 25, button: 0 }));
    b.ctrl.pointerMove(ev({ x: 80, y: 25 }));
    expect(b.of('placeAt')).toEqual([['placeAt', 15, 25]]);
    expect(b.of('panBy')).toHaveLength(0);
  });
});

describe('touch pinch', () => {
  it('two touches zoom about their midpoint and pan with it', () => {
    t.ctrl.pointerDown(ev({ pointerId: 1, x: 100, y: 100, button: 0, isTouch: true }));
    t.ctrl.pointerDown(ev({ pointerId: 2, x: 200, y: 100, button: 0, isTouch: true }));
    t.ctrl.pointerMove(ev({ pointerId: 2, x: 300, y: 100, isTouch: true }));
    const zooms = t.of('zoomAt');
    expect(zooms).toHaveLength(1);
    const [, x, y, f] = zooms[0] as [string, number, number, number];
    expect(x).toBe(200); // new midpoint
    expect(y).toBe(100);
    expect(f).toBeCloseTo(2, 9); // 100px → 200px spread
    expect(t.of('panBy')[0]).toEqual(['panBy', 50, 0]); // midpoint moved right
  });

  it('lifting one finger continues as a pan without a jump', () => {
    t.ctrl.pointerDown(ev({ pointerId: 1, x: 100, y: 100, button: 0, isTouch: true }));
    t.ctrl.pointerDown(ev({ pointerId: 2, x: 200, y: 100, button: 0, isTouch: true }));
    t.ctrl.pointerUp(ev({ pointerId: 2, x: 200, y: 100, button: 0, isTouch: true }));
    t.log.calls.length = 0;
    t.ctrl.pointerMove(ev({ pointerId: 1, x: 105, y: 108, isTouch: true }));
    expect(t.of('panBy')).toEqual([['panBy', 5, 8]]); // delta from survivor, not stale coords
  });
});

describe('hotkeys', () => {
  it('are dead while a modal disables them', () => {
    t.setHotkeys(false);
    expect(t.ctrl.key({ key: ' ', code: 'Space', repeat: false })).toBe(false);
    expect(t.ctrl.key({ key: '1', code: 'Digit1', repeat: false })).toBe(false);
    expect(t.ctrl.key({ key: 'Escape', code: 'Escape', repeat: false })).toBe(false);
    expect(t.log.calls).toHaveLength(0);
  });

  it('Space toggles pause once; auto-repeat is swallowed', () => {
    expect(t.ctrl.key({ key: ' ', code: 'Space', repeat: false })).toBe(true);
    expect(t.ctrl.key({ key: ' ', code: 'Space', repeat: true })).toBe(true);
    expect(t.ctrl.key({ key: ' ', code: 'Space', repeat: true })).toBe(true);
    expect(t.of('togglePause')).toHaveLength(1);
  });

  it('Esc cancels the tool; digits set speed', () => {
    t.ctrl.key({ key: 'Escape', code: 'Escape', repeat: false });
    t.ctrl.key({ key: '3', code: 'Digit3', repeat: false });
    expect(t.of('cancelTool')).toHaveLength(1);
    expect(t.of('setSpeed')).toEqual([['setSpeed', 4]]);
  });
});

describe('WASD panning', () => {
  const key = (code: string) => ({ key: code.slice(3).toLowerCase(), code, repeat: false });

  it('holding W pans the view north at the configured speed', () => {
    expect(t.ctrl.key(key('KeyW'))).toBe(true);
    t.ctrl.tick(1000);
    // north = map content moves down = positive y pan (same as dragging the map down)
    expect(t.of('panBy')).toEqual([['panBy', 0, KEY_PAN_SPEED]]);
    t.ctrl.tick(500);
    expect(t.of('panBy')[1]).toEqual(['panBy', 0, KEY_PAN_SPEED / 2]); // dt-scaled
  });

  it('diagonals are normalized and released keys stop panning', () => {
    t.ctrl.key(key('KeyW'));
    t.ctrl.key(key('KeyD'));
    t.ctrl.tick(1000);
    const [, dx, dy] = t.of('panBy')[0] as [string, number, number];
    expect(Math.hypot(dx, dy)).toBeCloseTo(KEY_PAN_SPEED, 6); // not sqrt(2) faster
    expect(dx).toBeLessThan(0); // D pulls the view east → content west
    t.ctrl.keyUp(key('KeyW'));
    t.ctrl.keyUp(key('KeyD'));
    t.ctrl.tick(1000);
    expect(t.of('panBy')).toHaveLength(1); // nothing further
  });

  it('opposing keys cancel out', () => {
    t.ctrl.key(key('KeyW'));
    t.ctrl.key(key('KeyS'));
    t.ctrl.tick(1000);
    expect(t.of('panBy')).toHaveLength(0);
  });

  it('is dead behind modals, and held keys clear when one opens mid-hold', () => {
    t.setHotkeys(false);
    expect(t.ctrl.key(key('KeyW'))).toBe(false);
    t.ctrl.tick(1000);
    expect(t.of('panBy')).toHaveLength(0);

    t.setHotkeys(true);
    t.ctrl.key(key('KeyW'));
    t.setHotkeys(false); // modal opens while W is held
    t.ctrl.tick(1000);
    t.setHotkeys(true);  // modal closes — the stale hold must not resume
    t.ctrl.tick(1000);
    expect(t.of('panBy')).toHaveLength(0);
  });

  it('reset() (window blur) drops held keys', () => {
    t.ctrl.key(key('KeyW'));
    t.ctrl.reset();
    t.ctrl.tick(1000);
    expect(t.of('panBy')).toHaveLength(0);
  });
});

describe('wheel + hover', () => {
  it('wheel zooms at the cursor', () => {
    t.ctrl.wheel({ x: 55, y: 66, deltaY: -1 });
    expect(t.of('zoomAt')).toEqual([['zoomAt', 55, 66, 1.15]]);
    t.ctrl.wheel({ x: 55, y: 66, deltaY: 1 });
    expect(t.of('zoomAt')[1]).toEqual(['zoomAt', 55, 66, 0.87]);
  });

  it('hover follows on-canvas moves and clears over UI or on leave', () => {
    t.ctrl.pointerMove(ev({ x: 10, y: 20 }));
    expect(t.of('setHover')).toEqual([['setHover', 10, 20]]);
    t.ctrl.pointerMove(ev({ x: 10, y: 20, onCanvas: false }));
    expect(t.of('clearHover')).toHaveLength(1);
    t.ctrl.pointerLeave();
    expect(t.of('clearHover')).toHaveLength(2);
  });
});
