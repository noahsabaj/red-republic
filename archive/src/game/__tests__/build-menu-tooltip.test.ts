import { describe, expect, it } from 'vitest';
import { computeTooltipShifts, type TooltipGeometry } from '../../components/BottomBar';

const geometry = (
  id: string,
  anchorCenterX: number,
  width: number,
  top = 20,
  bottom = 140,
): TooltipGeometry => ({ id, anchorCenterX, width, top, bottom });

describe('build menu tooltip collision geometry', () => {
  it('uses measured centers and widths to leave an exact six-pixel border gap', () => {
    const tooltips = [
      geometry('selected', 150, 210),
      geometry('hovered', 310, 250),
    ];

    const shifts = computeTooltipShifts(tooltips);
    const selectedRight = tooltips[0].anchorCenterX + shifts.selected + tooltips[0].width / 2;
    const hoveredLeft = tooltips[1].anchorCenterX + shifts.hovered - tooltips[1].width / 2;

    expect(shifts).toEqual({ selected: -38, hovered: 38 });
    expect(hoveredLeft - selectedRight).toBe(6);
  });

  it('uses physical left/right order even when responsive wrapping reverses source order', () => {
    const shifts = computeTooltipShifts([
      geometry('last-on-first-row', 520, 224),
      geometry('first-on-next-row', 80, 224),
    ]);

    expect(shifts).toEqual({});
  });

  it('leaves already-separated tooltips on the same row at their card centers', () => {
    const shifts = computeTooltipShifts([
      geometry('left', 120, 180),
      geometry('right', 420, 200),
    ]);

    expect(shifts).toEqual({});
  });

  it('does not move horizontally overlapping tooltips whose vertical ranges do not intersect', () => {
    const shifts = computeTooltipShifts([
      geometry('upper-row', 140, 224, 10, 90),
      geometry('lower-row', 140, 224, 96, 176),
    ]);

    expect(shifts).toEqual({});
  });

  it('assigns shift directions from measured positions rather than input order', () => {
    const shifts = computeTooltipShifts([
      geometry('right', 260, 224),
      geometry('left', 150, 224),
    ]);

    expect(shifts.left).toBeLessThan(0);
    expect(shifts.right).toBeGreaterThan(0);
  });

  it('moves an overlapping pair back inside viewport bounds without changing its gap', () => {
    const tooltips = [
      geometry('left', 80, 224),
      geometry('right', 160, 224),
    ];
    const shifts = computeTooltipShifts(tooltips, 6, { left: 6, right: 1394 });
    const leftEdge = tooltips[0].anchorCenterX + shifts.left - tooltips[0].width / 2;
    const leftRightEdge = tooltips[0].anchorCenterX + shifts.left + tooltips[0].width / 2;
    const rightLeftEdge = tooltips[1].anchorCenterX + shifts.right - tooltips[1].width / 2;

    expect(leftEdge).toBe(6);
    expect(rightLeftEdge - leftRightEdge).toBe(6);
  });

  it('keeps a single edge tooltip inside viewport bounds', () => {
    const tooltip = geometry('hovered', 40, 224);
    const shifts = computeTooltipShifts([tooltip], 6, { left: 6, right: 1018 });
    const leftEdge = tooltip.anchorCenterX + shifts.hovered - tooltip.width / 2;

    expect(shifts).toEqual({ hovered: 78 });
    expect(leftEdge).toBe(6);
  });

  it('clamps an already-separated pair as a group without changing its gap', () => {
    const tooltips = [
      geometry('left', 80, 224),
      geometry('right', 420, 224),
    ];
    const initialGap = tooltips[1].anchorCenterX - tooltips[1].width / 2
      - (tooltips[0].anchorCenterX + tooltips[0].width / 2);
    const shifts = computeTooltipShifts(tooltips, 6, { left: 6, right: 1018 });
    const leftEdge = tooltips[0].anchorCenterX + shifts.left - tooltips[0].width / 2;
    const shiftedGap = tooltips[1].anchorCenterX + shifts.right - tooltips[1].width / 2
      - (tooltips[0].anchorCenterX + shifts.left + tooltips[0].width / 2);

    expect(shifts).toEqual({ left: 38, right: 38 });
    expect(leftEdge).toBe(6);
    expect(shiftedGap).toBe(initialGap);
  });
});
