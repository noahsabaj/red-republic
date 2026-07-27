import { describe, expect, it } from 'vitest';
import { GameEngine } from '../engine';
import { generateMap } from '../mapgen';
import { packTiles, unpackTiles } from '../save-format';
import type { SaveGameV1 } from '../save-format';
import { flatMap, layRoad, placeBuilt } from './helpers';

describe('mapgen and save serialization performance assessment', () => {
  const MAP_SIZES: [number, number][] = [
    [32, 32],
    [48, 48],
    [64, 64],
    [96, 96],
    [128, 128],
  ];

  describe('map generation performance across map sizes', () => {
    it.each(MAP_SIZES)('benchmarks mapgen execution speed for size %ix%i', (w, h) => {
      const iterations = 5;
      const start = performance.now();

      for (let i = 0; i < iterations; i++) {
        generateMap(100 + i, w, h);
      }

      const elapsedMs = performance.now() - start;
      const avgMs = elapsedMs / iterations;

      console.log(`[PERF MAPGEN] ${w}x${h}: avg ${avgMs.toFixed(2)}ms across ${iterations} iterations`);

      // Performance sanity thresholds:
      // Even large 128x128 map generation should complete under 200ms per map
      expect(avgMs).toBeLessThan(200);
    });
  });

  describe('base64 tile codec throughput (packTiles / unpackTiles)', () => {
    it.each(MAP_SIZES)('benchmarks packing and unpacking tiles for size %ix%i', (w, h) => {
      const mapData = generateMap(42, w, h);
      const iterations = 50;

      // Benchmark packTiles
      const startPack = performance.now();
      let packedTiles = '';
      let packedVariants = '';
      for (let i = 0; i < iterations; i++) {
        const res = packTiles(mapData.tiles);
        packedTiles = res.tilesPacked;
        packedVariants = res.variantsPacked;
      }
      const elapsedPack = performance.now() - startPack;
      const avgPackMs = elapsedPack / iterations;

      // Benchmark unpackTiles
      const startUnpack = performance.now();
      let unpacked: ReturnType<typeof unpackTiles> = [];
      for (let i = 0; i < iterations; i++) {
        unpacked = unpackTiles(packedTiles, packedVariants, w, h);
      }
      const elapsedUnpack = performance.now() - startUnpack;
      const avgUnpackMs = elapsedUnpack / iterations;

      console.log(
        `[PERF CODEC] ${w}x${h}: packTile ${avgPackMs.toFixed(3)}ms, unpackTile ${avgUnpackMs.toFixed(3)}ms | base64 bytes: ${packedTiles.length}`
      );

      expect(avgPackMs).toBeLessThan(15);
      expect(avgUnpackMs).toBeLessThan(15);
      expect(unpacked).toHaveLength(h);
      expect(unpacked[0]).toHaveLength(w);
    });
  });

  describe('serialize() and fromSave() throughput', () => {
    it.each(MAP_SIZES)('benchmarks empty map serialization/hydration for size %ix%i', (w, h) => {
      const engine = new GameEngine({
        seed: 123,
        map: flatMap(w, h),
        skipStartingBase: true,
      });

      const iterations = 20;

      // Benchmark serialize
      const startSer = performance.now();
      let lastSnapStr = '';
      let snapObj: ReturnType<typeof engine.serialize> | null = null;
      for (let i = 0; i < iterations; i++) {
        snapObj = engine.serialize();
        lastSnapStr = JSON.stringify(snapObj);
      }
      const elapsedSer = performance.now() - startSer;
      const avgSerMs = elapsedSer / iterations;
      const sizeKB = (lastSnapStr.length / 1024).toFixed(2);

      // Benchmark fromSave
      const startDes = performance.now();
      for (let i = 0; i < iterations; i++) {
        const parsed = JSON.parse(lastSnapStr) as SaveGameV1;
        GameEngine.fromSave(parsed);
      }
      const elapsedDes = performance.now() - startDes;
      const avgDesMs = elapsedDes / iterations;

      console.log(
        `[PERF SERIALIZE EMPTY] ${w}x${h}: serialize+JSON ${avgSerMs.toFixed(3)}ms, parse+fromSave ${avgDesMs.toFixed(3)}ms | payload size: ${sizeKB} KB`
      );

      expect(avgSerMs).toBeLessThan(30);
      expect(avgDesMs).toBeLessThan(30);
    });

    it('benchmarks heavily populated map serialization/hydration (500+ buildings & roads)', () => {
      const w = 128, h = 128;
      const engine = new GameEngine({
        seed: 999,
        map: flatMap(w, h),
        skipStartingBase: true,
      });

      // Build grid of roads
      for (let y = 10; y < 110; y += 4) {
        layRoad(engine, 10, y, 110, y);
      }
      for (let x = 10; x < 110; x += 4) {
        layRoad(engine, x, 10, x, 110);
      }

      // Place 500+ buildings
      let bCount = 0;
      for (let y = 12; y < 108 && bCount < 500; y += 4) {
        for (let x = 12; x < 108 && bCount < 500; x += 4) {
          placeBuilt(engine, 'house', x, y);
          bCount++;
        }
      }

      expect(engine.buildings.size).toBeGreaterThanOrEqual(500);

      const iterations = 10;

      // Benchmark serialize
      const startSer = performance.now();
      let lastSnapStr = '';
      for (let i = 0; i < iterations; i++) {
        const snap = engine.serialize();
        lastSnapStr = JSON.stringify(snap);
      }
      const avgSerMs = (performance.now() - startSer) / iterations;
      const sizeKB = (lastSnapStr.length / 1024).toFixed(2);

      // Benchmark fromSave
      const startDes = performance.now();
      for (let i = 0; i < iterations; i++) {
        const parsed = JSON.parse(lastSnapStr) as SaveGameV1;
        GameEngine.fromSave(parsed);
      }
      const avgDesMs = (performance.now() - startDes) / iterations;

      console.log(
        `[PERF SERIALIZE HEAVY] 128x128 (500 buildings): serialize+JSON ${avgSerMs.toFixed(3)}ms, parse+fromSave ${avgDesMs.toFixed(3)}ms | payload size: ${sizeKB} KB`
      );

      expect(avgSerMs).toBeLessThan(100);
      expect(avgDesMs).toBeLessThan(100);
    });
  });
});
