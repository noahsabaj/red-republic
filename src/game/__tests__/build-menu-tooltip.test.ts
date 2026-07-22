import { describe, expect, it } from 'vitest';
import { BUILDINGS, CATEGORIES, SUBCATEGORIES } from '../config';

describe('build menu tooltip configuration & push-apart math', () => {
  it('defines building tooltips and categories cleanly', () => {
    expect(CATEGORIES.length).toBeGreaterThan(0);
    const industryCat = CATEGORIES.find(c => c.id === 'industry');
    expect(industryCat).toBeDefined();

    const industrySubs = SUBCATEGORIES.industry;
    expect(industrySubs.length).toBeGreaterThan(0);

    const firstSub = industrySubs[0];
    expect(firstSub.ids.length).toBeGreaterThan(0);

    const buildingDef = BUILDINGS[firstSub.ids[0]];
    expect(buildingDef.name).toBeDefined();
    expect(buildingDef.description).toBeDefined();
  });

  it('calculates push-apart offsets for overlapping tooltips', () => {
    const tooltipWidth = 224;
    const gap = 6;
    const requiredDistance = tooltipWidth + gap; // 230px

    const centerA = 100;
    const centerB = 180;
    const distance = centerB - centerA; // 80px

    expect(distance).toBeLessThan(requiredDistance);

    const overlap = requiredDistance - distance; // 150px
    const shift = overlap / 2; // 75px

    const shiftedCenterA = centerA - shift; // 25px
    const shiftedCenterB = centerB + shift; // 255px

    const leftRightEdge = shiftedCenterA + tooltipWidth / 2; // 25 + 112 = 137px
    const rightLeftEdge = shiftedCenterB - tooltipWidth / 2; // 255 - 112 = 143px

    expect(rightLeftEdge - leftRightEdge).toBe(gap); // 6px gap
  });
});
