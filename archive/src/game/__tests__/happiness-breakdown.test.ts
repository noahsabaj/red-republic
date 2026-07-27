import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { HappinessCard } from '../../components/HUD';
import { GameEngine } from '../engine';
import { makeEngine, runDays } from './helpers';

describe('happiness breakdown engine calculations', () => {
  it('returns initial happiness breakdown with 7 factors and correct weights', () => {
    const engine = new GameEngine();
    const breakdown = engine.happinessBreakdown();

    expect(breakdown.overall).toBe(70);
    expect(breakdown.factors).toHaveLength(7);

    const factorWeights = breakdown.factors.reduce((sum, f) => sum + f.weightPct, 0);
    expect(factorWeights).toBe(100);

    const foodFactor = breakdown.factors.find(f => f.id === 'food');
    expect(foodFactor).toBeDefined();
    expect(foodFactor?.weightPct).toBe(30);

    const clothesFactor = breakdown.factors.find(f => f.id === 'clothes');
    expect(clothesFactor).toBeDefined();
    expect(clothesFactor?.weightPct).toBe(14);
  });

  it('reflects changes in satisfaction factors accurately', () => {
    const engine = new GameEngine();
    // Simulate drop in food and power satisfaction
    engine.sat.food = 0.5;
    engine.sat.power = 0.0;

    const breakdown = engine.happinessBreakdown();
    const foodFactor = breakdown.factors.find(f => f.id === 'food')!;
    const powerFactor = breakdown.factors.find(f => f.id === 'power')!;

    expect(foodFactor.satPct).toBe(50);
    expect(powerFactor.satPct).toBe(0);
    expect(breakdown.target).toBeLessThan(70);
  });

  it('calculates pollution penalty when housing is polluted', () => {
    const engine = new GameEngine();
    engine.sat.pollution = 0.8; // 20% pollution penalty

    const breakdown = engine.happinessBreakdown();
    expect(breakdown.modifiers.pollutionPenaltyPct).toBe(20);
  });

  it('preserves and displays sunny morale in half-percent steps', () => {
    const engine = makeEngine({ weather: () => ({ condition: 'clear' }) });

    runDays(engine, 1);
    const firstSunnyDay = engine.happinessBreakdown().modifiers.weatherMoralePct;
    expect(firstSunnyDay).toBe(0.5);
    expect(renderToStaticMarkup(createElement(HappinessCard, { engine }))).toContain('+0.5%');

    runDays(engine, 1);
    const secondSunnyDay = engine.happinessBreakdown().modifiers.weatherMoralePct;
    expect(secondSunnyDay).toBe(1);
    expect(renderToStaticMarkup(createElement(HappinessCard, { engine }))).toContain('+1%');
  });
});
