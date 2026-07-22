import { describe, expect, it } from 'vitest';
import { BUILDINGS, CATEGORIES, SUBCATEGORIES } from '../config';

describe('build menu tooltip configuration', () => {
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
});
