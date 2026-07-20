// ============================================================
// The pinned campaign: a deterministic map, weather script, and build
// order that a competent player could follow. campaign-pacing.test.ts
// asserts the viability milestones; the scratch runner uses the same
// driver to observe trajectories while tuning balance.
// ============================================================
import { GameEngine } from '../engine';
import { BALANCE } from '../config';
import type { MapData } from '../mapgen';
import type { DayWeather } from '../weather';
import { flatBorderMap } from './helpers';

/** flatBorderMap (western border, start 24,24, road row 23 from x=4..25)
 *  plus hand-stamped deposits and a forest — all deterministic tile edits. */
export function campaignMap(): MapData {
  const map = flatBorderMap();
  const t = map.tiles;
  // forest for the woodcutter (north of the road row)
  for (let y = 20; y <= 21; y++) for (let x = 16; x <= 19; x++) t[y][x].terrain = 'forest';
  // gravel south, coal and iron north
  for (const [x, y] of [[18, 24], [18, 25], [19, 24], [19, 25]]) { t[y][x].deposit = 'gravel'; t[y][x].terrain = 'rock'; }
  for (const [x, y] of [[21, 22], [21, 21], [22, 21]]) t[y][x].deposit = 'coal';
  for (const [x, y] of [[23, 22], [24, 22], [23, 21]]) t[y][x].deposit = 'ironOre';
  return map;
}

/** Clear-sky seasonal sinusoid: winters bite (heating), summers scorch
 *  (drought pressure), zero storm noise. doy 15 ≈ −12 °C, doy 194 ≈ +24 °C. */
export function campaignWeather(idx: number): Partial<DayWeather> {
  const doy = idx % 360;
  const tempC = Math.round((6 + 18 * Math.sin((2 * Math.PI * (doy - 104)) / 360)) * 10) / 10;
  return { tempC, condition: 'clear', snowDepth: 0, riverFrozen: false };
}

export interface CampaignStep { day: number; act: (e: GameEngine) => void }

/** Assert-ok placement helper for build-order steps. */
function place(e: GameEngine, defId: string, x: number, y: number) {
  const res = e.tryPlace(defId, x, y);
  if (!res.ok) throw new Error(`campaign: place ${defId}@${x},${y} failed: ${res.reason}`);
}

function buyMachines(e: GameEngine, n: number) {
  const res = e.buy('machinery', n, 'east');
  if (!res.ok) throw new Error(`campaign: buying ${n} machinery failed: ${res.msg}`);
}

/**
 * The build order. Positions sit on the pre-built road row 23 (north side
 * row 22, south side row 24); the starting base occupies depot (24,24 2×2),
 * construction office (22,24), customs (2,24 2×2).
 */
// Town planning: housing + services in the WEST (rows 21-22, x 4-12), light
// industry mid-row, heavy/polluting industry EAST and SOUTH (x ≥ 15) — the
// pollution radius (6) never reaches a home.
export const CAMPAIGN_STEPS: CampaignStep[] = [
  {
    day: 1, act: e => {
      // the wooden town: shelter, services, trade goods, food security —
      // everything on the road row (23), plus an eastward extension for the
      // future industrial quarter
      for (let x = 5; x <= 9; x++) place(e, 'house', x, 22);
      place(e, 'pub', 4, 22);
      place(e, 'clinic', 10, 22);
      place(e, 'store', 11, 22);
      place(e, 'farm', 13, 21);
      place(e, 'woodcutter', 17, 22);
      place(e, 'sawmill', 15, 24);
      place(e, 'gravelQuarry', 18, 24);
      place(e, 'brickworks', 16, 24);
      for (let x = 26; x <= 30; x++) place(e, 'road', x, 23);
    },
  },
  {
    day: 60, act: e => {
      place(e, 'foodFactory', 20, 24);  // starting machine #1
      place(e, 'heatingPlant', 13, 24); // starting machine #2 (pollution 2, kept at range)
    },
  },
  {
    day: 75, act: e => {
      // exports fund the machine imports; clothes come from the East
      e.setAutoTradeEnabled(true);
      e.setAutoTradeRule('planks', { mode: 'export', level: 30, currency: 'east' });
      e.setAutoTradeRule('bricks', { mode: 'export', level: 40, currency: 'east' });
      e.setAutoTradeRule('food', { mode: 'export', level: 60, currency: 'east' });
      e.setAutoTradeRule('clothes', { mode: 'import', level: 6, currency: 'east' });
    },
  },
  {
    day: 200, act: e => {
      // pave a residential street north — the crews lay it tile by tile
      for (const y of [22, 21, 20, 19, 18]) place(e, 'road', 12, y);
    },
  },
  {
    day: 240, act: e => {
      // electrification + the housing to staff it
      buyMachines(e, 7); // coal mine 2 + power plant 5
      place(e, 'coalMine', 21, 22);
      place(e, 'powerPlant', 26, 24); // the industrial quarter, far from homes
      place(e, 'apartment', 10, 19); // flanking the new street
      place(e, 'apartment', 13, 19);
      // modest coal exports (winter heating still eats most of it); spares
      // trickle in as machines wear down
      e.setAutoTradeRule('coal', { mode: 'export', level: 60, currency: 'east' });
      e.setAutoTradeRule('machinery', { mode: 'import', level: 12, currency: 'east' });
    },
  },
  {
    day: 300, act: e => {
      // the export industry: clothes are the best ruble earner the town can
      // make, and weaving them ends the standing clothes-import bill
      buyMachines(e, 4); // textile mill 1 + spares (wear bins outrank sites)
      place(e, 'farm', 6, 24);       // second farm feeds the looms
      place(e, 'textileMill', 19, 22);
    },
  },
  {
    day: 430, act: e => {
      buyMachines(e, 3); // iron mine 2 + second heating plant 1
      place(e, 'ironMine', 23, 22);
      place(e, 'heatingPlant', 14, 24);
      place(e, 'apartment', 10, 17);
      // the mill runs — stop buying clothes and start selling them, and let
      // the new income keep more spares on hand (worn plants are dearer)
      e.setAutoTradeRule('clothes', { mode: 'export', level: 12, currency: 'east' });
      e.setAutoTradeRule('machinery', { mode: 'import', level: 16, currency: 'east' });
    },
  },
  {
    day: 500, act: e => {
      // heavy industry needs hands: fill the south side of the main road
      // (x4 is the customs link; farm #2 holds 6-7; heating holds 13-14)
      for (const x of [5, 8, 9, 10, 11, 12, 23]) place(e, 'house', x, 24);
      // a second office doubles the truck fleet — with one office, construction
      // hauling monopolizes every truck and nothing reaches the border to sell
      place(e, 'constructionOffice', 30, 22);
    },
  },
  {
    day: 560, act: e => {
      // the steel town: mill + the second power plant that keeps it lit
      e.buy('steel', 35, 'east'); // frames for the mill and the plant
      buyMachines(e, 13); // steel mill 8 + power plant 5
      place(e, 'steelMill', 28, 24);  // east, away from town
      place(e, 'powerPlant', 28, 21); // beside it, same industrial quarter
    },
  },
  {
    day: 700, act: e => {
      // infill housing on the remaining road-row frontage
      for (const [x, y] of [[15, 22], [16, 22], [18, 22], [25, 22], [21, 24]]) {
        place(e, 'house', x, y);
      }
    },
  },
  {
    day: 800, act: e => {
      e.buy('steel', 30, 'east'); // the last structural import before autarky
      buyMachines(e, 9); // machine works 6 + second coal mine 2 + third boiler 1
      place(e, 'machineWorks', 26, 21); // beside the mill, off the row's north side
      place(e, 'road', 22, 22);         // spur to the second coal seam
      place(e, 'coalMine', 22, 21);     // two plants and three boilers eat coal
      place(e, 'heatingPlant', 20, 22); // winter with 250 souls needs a third boiler
      place(e, 'apartment', 13, 17);
    },
  },
  {
    day: 980, act: e => {
      // autarky in practice: every material below — steel, machinery, bricks —
      // now comes off the republic's own lines, not across the border
      place(e, 'powerPlant', 30, 24);   // third plant closes the power gap
      place(e, 'foodFactory', 17, 24);  // second bakery for 300 mouths
      for (const [x, y] of [[4, 25], [4, 26], [5, 26], [6, 26], [7, 26]]) place(e, 'road', x, y);
      place(e, 'farm', 5, 27);          // southern fields feed the new bakery
    },
  },
];

/** Run the campaign, invoking `probe` after every day. */
export function runCampaign(days: number, probe?: (e: GameEngine, day: number) => void): GameEngine {
  const e = new GameEngine({ seed: 1, map: campaignMap(), weatherScript: campaignWeather });
  e.setSpeed(1);
  const steps = [...CAMPAIGN_STEPS];
  for (let d = 1; d <= days; d++) {
    while (steps.length && steps[0].day <= d) steps.shift()!.act(e);
    e.advance(e.TICK_MS);
    probe?.(e, d);
  }
  return e;
}

/** The road row 23 must exist for the campaign's placements. */
export const CAMPAIGN_SANITY = { roadRowY: 23, borderDepth: BALANCE.borderDepth };
