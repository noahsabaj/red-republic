import { describe, expect, it } from 'vitest';
import { TilemapCache } from '../tilemap-cache';

describe('TilemapCache', () => {
  it('instantiates and invalidates cleanly', () => {
    const cache = new TilemapCache();
    expect(cache).toBeDefined();
    cache.invalidate();
  });
});
