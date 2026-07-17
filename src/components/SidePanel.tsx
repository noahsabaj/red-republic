import { useState } from 'react';
import type { GameEngine, BuildingInst } from '@/game/engine';
import { BUILDINGS, RESOURCES, ALL_RESOURCES, OBJECTIVES, FARM_SEASON } from '@/game/config';
import type { DepositType, ResourceId } from '@/game/config';
import type { SelectionItem } from '@/game/selection';
import { useEngineVersion } from '@/hooks/use-engine';

interface Props {
  engine: GameEngine;
  mode: 'building' | 'trade' | 'objectives';
  selection: SelectionItem[];
  onClose: () => void;
  onOpenTrade: () => void;
  onArmBuild: (defId: string) => void;
  notify: (msg: string, kind: 'good' | 'bad' | 'info') => void;
}

export default function SidePanel({ engine, mode, selection, onClose, onOpenTrade, onArmBuild, notify }: Props) {
  // the open detail panel mirrors live engine state — re-render on every bump
  useEngineVersion(engine);
  const single = selection.length === 1 ? selection[0] : null;
  const title = mode === 'trade' ? '🛃 Foreign Trade'
    : mode === 'objectives' ? '🎯 Five-Year Plan'
    : selection.length > 1 ? `${selection.length} selected`
    : single?.kind === 'deposit' ? 'Deposit' : 'Building';
  return (
    <div className="absolute right-0 top-24 bottom-0 z-10 flex pointer-events-none">
      <div className="pointer-events-auto flex flex-col w-72 m-2 rounded-lg border-2 border-yellow-600/60 bg-red-950/95 text-yellow-50 shadow-2xl overflow-hidden">
        <div className="flex items-center justify-between px-3 py-2 bg-red-900/60 border-b border-yellow-600/30">
          <span className="text-xs font-black uppercase tracking-widest text-yellow-400">{title}</span>
          <button onClick={onClose} className="text-yellow-200/60 hover:text-yellow-100 font-bold">✕</button>
        </div>
        <div className="flex-1 overflow-y-auto soviet-scroll p-3">
          {mode === 'building' && (
            selection.length > 1
              ? <MultiInfo engine={engine} items={selection} onArmBuild={onArmBuild} />
              : single?.kind === 'deposit'
                ? <DepositInfo engine={engine} x={single.x} y={single.y} onArmBuild={onArmBuild} />
                : <BuildingInfo engine={engine} id={single?.kind === 'building' ? single.id : null} onOpenTrade={onOpenTrade} />
          )}
          {mode === 'trade' && <TradePanel engine={engine} notify={notify} />}
          {mode === 'objectives' && <ObjectivesPanel engine={engine} />}
        </div>
      </div>
    </div>
  );
}

// ------------------------------------------------------------

function MultiInfo({ engine, items, onArmBuild }: { engine: GameEngine; items: SelectionItem[]; onArmBuild: (defId: string) => void }) {
  const buildings = items
    .filter((i): i is Extract<SelectionItem, { kind: 'building' }> => i.kind === 'building')
    .map(i => engine.buildings.get(i.id))
    .filter((b): b is BuildingInst => !!b);

  // deposit tiles grouped by kind; "free" tiles can each host one extractor
  const depositKinds = new Map<DepositType, { total: number; free: number }>();
  for (const i of items) {
    if (i.kind !== 'deposit') continue;
    const t = engine.tiles[i.y]?.[i.x];
    if (!t?.deposit) continue;
    const g = depositKinds.get(t.deposit) ?? { total: 0, free: 0 };
    g.total++;
    if (!t.buildingId && !t.road) g.free++;
    depositKinds.set(t.deposit, g);
  }

  // building group counts + live totals
  const typeCounts = new Map<string, number>();
  let staff = 0, jobs = 0, powerDraw = 0;
  const flowIn: Partial<Record<ResourceId, number>> = {};
  const flowOut: Partial<Record<ResourceId, number>> = {};
  for (const b of buildings) {
    const def = BUILDINGS[b.defId];
    typeCounts.set(b.defId, (typeCounts.get(b.defId) ?? 0) + 1);
    if (b.constructed) {
      staff += b.staff;
      jobs += def.workers;
      powerDraw += def.power;
      const rates = engine.productionRates(b);
      for (const [r, a] of Object.entries(rates.inputs) as [ResourceId, number][]) flowIn[r] = (flowIn[r] ?? 0) + a;
      for (const [r, a] of Object.entries(rates.outputs) as [ResourceId, number][]) flowOut[r] = (flowOut[r] ?? 0) + a;
    }
  }
  const staffed = buildings.filter(b => b.constructed && BUILDINGS[b.defId].workers > 0);
  const allPriority = staffed.length > 0 && staffed.every(b => b.priorityHigh);
  const fmt = (n: number) => (n < 10 ? n.toFixed(1) : String(Math.round(n)));
  const ins = Object.entries(flowIn) as [ResourceId, number][];
  const outs = Object.entries(flowOut) as [ResourceId, number][];

  return (
    <div className="space-y-3">
      {buildings.length > 0 && (
        <div className="space-y-2">
          <div className="text-[10px] font-black uppercase tracking-wider text-yellow-400">Buildings ({buildings.length})</div>
          <div className="space-y-0.5">
            {[...typeCounts.entries()].map(([defId, n]) => (
              <Row key={defId} label={`${BUILDINGS[defId].icon} ${BUILDINGS[defId].name}`} value={`×${n}`} />
            ))}
          </div>
          <div className="grid grid-cols-2 gap-x-2">
            {jobs > 0 && <Row label="👷 Staff" value={`${staff}/${jobs}`} ok={staff >= jobs * 0.8} />}
            {powerDraw > 0 && <Row label="⚡ Draw" value={`${powerDraw.toFixed(1)} MW`} />}
          </div>
          {(ins.length > 0 || outs.length > 0) && (
            <div className="rounded bg-red-900/40 p-2">
              <div className="text-[10px] font-black uppercase tracking-wider text-yellow-400 mb-1">Combined production / day</div>
              <div className="text-xs">
                {ins.map(([r, a]) => `${RESOURCES[r].icon}${fmt(a)}`).join(' + ')}
                {ins.length > 0 && outs.length > 0 && ' → '}
                {outs.map(([r, a]) => `${RESOURCES[r].icon}${fmt(a)}`).join(' + ')}
              </div>
            </div>
          )}
          {staffed.length > 0 && (
            <button
              onClick={() => engine.setStaffPriorityMany(staffed.map(b => b.id), !allPriority)}
              className={`w-full rounded font-bold text-xs py-1.5 ${allPriority ? 'bg-yellow-500 text-red-950' : 'bg-red-900/70 hover:bg-red-800'}`}
              title="High-priority buildings are staffed first when workers are scarce"
            >
              {allPriority ? '⭐ Priority staffing: ON for all' : `☆ Set priority staffing for ${staffed.length} building${staffed.length > 1 ? 's' : ''}`}
            </button>
          )}
        </div>
      )}

      {depositKinds.size > 0 && (
        <div className="space-y-2">
          <div className="text-[10px] font-black uppercase tracking-wider text-yellow-400">Deposits</div>
          {[...depositKinds.entries()].map(([kind, g]) => {
            const res = RESOURCES[kind];
            const miner = Object.values(BUILDINGS).find(d => d.requiresDeposit === kind)!;
            const outputsPerMine = Object.entries(miner.outputs ?? {}) as [ResourceId, number][];
            return (
              <div key={kind} className="rounded bg-red-900/40 p-2 space-y-1">
                <div className="flex justify-between text-xs font-bold">
                  <span>{res.icon} {res.name}</span>
                  <span>{g.total} tile{g.total > 1 ? 's' : ''}{g.free < g.total ? ` · ${g.free} free` : ''}</span>
                </div>
                {g.free > 0 && (
                  <>
                    <div className="text-[11px] text-yellow-200/80">
                      One {miner.name} per free tile: {outputsPerMine.map(([r, a]) => `${RESOURCES[r].icon}${fmt(a * g.free)}/day`).join(' ')}
                      <span className="text-yellow-200/60"> · 👷{miner.workers * g.free} · ₽{(miner.costRubles * g.free).toLocaleString()} total</span>
                    </div>
                    <button
                      onClick={() => onArmBuild(miner.id)}
                      className="w-full rounded bg-yellow-500 text-red-950 font-bold text-xs py-1 hover:bg-yellow-400"
                    >
                      🏗️ Build {miner.name} (₽{miner.costRubles} each)
                    </button>
                  </>
                )}
              </div>
            );
          })}
        </div>
      )}

      {buildings.length === 0 && depositKinds.size === 0 && (
        <div className="text-xs text-yellow-200/60">The selection no longer exists.</div>
      )}
    </div>
  );
}

// ------------------------------------------------------------

const DEPOSIT_NAMES: Record<string, string> = { coal: 'Coal', ironOre: 'Iron Ore', oil: 'Oil', gravel: 'Gravel' };

function DepositInfo({ engine, x, y, onArmBuild }: { engine: GameEngine; x: number; y: number; onArmBuild: (defId: string) => void }) {
  const cluster = engine.depositClusterAt(x, y);
  if (!cluster) return <div className="text-xs text-yellow-200/60">Nothing of value here.</div>;
  const res = RESOURCES[cluster.kind];
  const miner = Object.values(BUILDINGS).find(d => d.requiresDeposit === cluster.kind)!;
  const exploited = cluster.exploitedBy;
  const outputs = Object.entries(miner.outputs ?? {}) as [ResourceId, number][];

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2">
        <span className="text-2xl">{res.icon}</span>
        <div>
          <div className="font-bold text-sm">{DEPOSIT_NAMES[cluster.kind]} Deposit</div>
          <div className="text-[10px] text-yellow-200/50">
            {exploited ? `Worked by a ${BUILDINGS[exploited.defId].name}.` : 'Unexploited mineral wealth of the republic.'}
          </div>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-x-2">
        <Row label={`${res.icon} Resource`} value={res.name} />
        <Row label="🗺️ Cluster" value={`${cluster.tiles.length} tile${cluster.tiles.length > 1 ? 's' : ''}`} />
        <Row label="⛏️ Status" value={exploited ? 'Exploited' : 'Unexploited'} ok={!!exploited} />
      </div>

      <div className="rounded bg-red-900/40 p-2">
        <div className="text-[10px] font-black uppercase tracking-wider text-yellow-400 mb-1">{miner.name}</div>
        <div className="text-xs">
          {outputs.map(([r, a]) => `${RESOURCES[r].icon}${a}/day at full staff`).join(' ')}
          <span className="text-yellow-200/60"> · 👷{miner.workers}{miner.power > 0 ? ` · ⚡${miner.power} MW` : ''}</span>
        </div>
      </div>

      {!exploited && (
        <button
          onClick={() => onArmBuild(miner.id)}
          className="w-full rounded bg-yellow-500 text-red-950 font-bold text-xs py-1.5 hover:bg-yellow-400"
          title={`Arm the build tool — place the ${miner.name} on one of this cluster's tiles`}
        >
          🏗️ Build {miner.name} (₽{miner.costRubles})
        </button>
      )}
    </div>
  );
}

// ------------------------------------------------------------

function Row({ label, value, ok }: { label: string; value: string; ok?: boolean }) {
  return (
    <div className="flex justify-between text-xs py-0.5">
      <span className="text-yellow-200/70">{label}</span>
      <span className={`font-bold ${ok === false ? 'text-red-300' : ok === true ? 'text-green-300' : ''}`}>{value}</span>
    </div>
  );
}

function BuildingInfo({ engine, id, onOpenTrade }: { engine: GameEngine; id: number | null; onOpenTrade: () => void }) {
  const b: BuildingInst | undefined = id ? engine.buildings.get(id) : undefined;
  if (!b) return <div className="text-xs text-yellow-200/60">Select a building on the map to inspect it.</div>;
  const def = BUILDINGS[b.defId];

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2">
        <span className="text-2xl">{def.icon}</span>
        <div>
          <div className="font-bold text-sm">{def.name}</div>
          <div className="text-[10px] text-yellow-200/50">{def.description}</div>
        </div>
      </div>

      {!b.constructed ? (
        <div className="space-y-1.5">
          <div className="text-xs font-bold text-yellow-400">🚧 Under construction — {Math.round((b.progress / def.labor) * 100)}%</div>
          <div className="h-2 rounded bg-red-900 overflow-hidden">
            <div className="h-full bg-yellow-500" style={{ width: `${(b.progress / def.labor) * 100}%` }} />
          </div>
          <div className="text-[11px] text-yellow-200/70">
            Labor: {Math.floor(b.progress)} / {def.labor} worker-days
          </div>
          {Object.entries(def.materials).map(([r, amt]) => {
            const stock = b.stock[r as ResourceId] ?? 0;
            const inc = b.incoming[r as ResourceId] ?? 0;
            return (
              <Row
                key={r}
                label={`${RESOURCES[r as ResourceId].icon} ${RESOURCES[r as ResourceId].name}`}
                value={`${Math.floor(stock)}${inc > 0.05 ? ` (+${Math.floor(inc)}🚚)` : ''} / ${amt}`}
                ok={stock >= (amt)}
              />
            );
          })}
          <div className="text-[10px] text-yellow-200/50">Materials arrive by truck. Builders come from a staffed Construction Office.</div>
        </div>
      ) : (
        <div className="space-y-2">
          <div className="grid grid-cols-2 gap-x-2">
            {def.workers > 0 && <Row label="👷 Staff" value={`${b.staff}/${def.workers}`} ok={b.staff >= def.workers * 0.8} />}
            <Row label="🛣️ Road" value={b.connected ? 'Connected' : 'No road!'} ok={b.connected} />
            {def.power > 0 && <Row label="⚡ Power" value={b.powered ? `${def.power} MW` : 'No power!'} ok={b.powered} />}
            {def.powerOutput !== undefined && <Row label="⚡ Output" value={`${(def.powerOutput * b.eff * b.coalFactor).toFixed(1)} MW`} />}
            {def.heatOutput !== undefined && <Row label="🔥 Output" value={`${(def.heatOutput * b.eff * b.coalFactor).toFixed(1)}`} />}
            {def.heat > 0 && <Row label="🔥 Heat" value={b.heated ? 'Warm' : 'Freezing!'} ok={b.heated} />}
            {def.housingCapacity && <Row label="🛏️ Capacity" value={`${def.housingCapacity} citizens`} />}
            {def.serviceRadius && <Row label="📏 Coverage" value={`${def.serviceRadius} tiles`} />}
            {def.isFarm && <Row label="🌱 Fields" value={`${b.farmFields} plots · season ×${(FARM_SEASON[engine.month] ?? 0).toFixed(2)}`} />}
            {def.workers > 0 && <Row label="⚙️ Efficiency" value={`${Math.round(b.eff * 100)}%`} ok={b.eff > 0.6} />}
          </div>

          {(def.inputs || def.outputs) && (() => {
            // the engine's own numbers — includes season, fields, forest and
            // input starvation, so this always matches what actually happens
            const rates = engine.productionRates(b);
            const fmt = (n: number) => (n < 10 ? n.toFixed(1) : String(Math.round(n)));
            const ins = Object.entries(rates.inputs) as [ResourceId, number][];
            const outs = Object.entries(rates.outputs) as [ResourceId, number][];
            return (
              <div className="rounded bg-red-900/40 p-2">
                <div className="text-[10px] font-black uppercase tracking-wider text-yellow-400 mb-1">Production / day</div>
                <div className="text-xs">
                  {ins.length === 0 && outs.length === 0
                    ? <span className="text-yellow-200/50">idle</span>
                    : <>
                        {ins.map(([r, a]) => `${RESOURCES[r].icon}${fmt(a)}`).join(' + ')}
                        {ins.length > 0 && outs.length > 0 && ' → '}
                        {outs.map(([r, a]) => `${RESOURCES[r].icon}${fmt(a)}`).join(' + ')}
                      </>}
                </div>
              </div>
            );
          })()}

          {Object.keys(def.storage).length > 0 && (
            <div>
              <div className="text-[10px] font-black uppercase tracking-wider text-yellow-400 mb-1">Storage</div>
              <div className="space-y-1">
                {(Object.entries(def.storage) as [ResourceId, number][]).filter(([r]) => (b.stock[r] ?? 0) > 0.05 || (b.incoming[r] ?? 0) > 0.05 || (def.inputs?.[r] ?? 0) > 0 || (def.outputs?.[r] ?? 0) > 0 || def.serviceType === 'shop').map(([r, cap]) => {
                  const v = b.stock[r] ?? 0;
                  const inc = b.incoming[r] ?? 0;
                  return (
                    <div key={r} className="text-[11px]">
                      <div className="flex justify-between">
                        <span>{RESOURCES[r].icon} {RESOURCES[r].name}</span>
                        <span className="font-bold">{v.toFixed(1)}/{cap}{inc > 0.05 ? ` (+${inc.toFixed(0)}🚚)` : ''}</span>
                      </div>
                      <div className="h-1.5 rounded bg-red-900 overflow-hidden">
                        <div className="h-full bg-yellow-500/80" style={{ width: `${Math.min(100, (v / cap) * 100)}%` }} />
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
          )}

          {def.isCustoms && (
            <button onClick={onOpenTrade} className="w-full rounded bg-yellow-500 text-red-950 font-bold text-xs py-1.5 hover:bg-yellow-400">
              🛃 Open Foreign Trade
            </button>
          )}

          {def.workers > 0 && (
            <button
              onClick={() => engine.toggleStaffPriority(b.id)}
              className={`w-full rounded font-bold text-xs py-1.5 ${b.priorityHigh ? 'bg-yellow-500 text-red-950' : 'bg-red-900/70 hover:bg-red-800'}`}
              title="High-priority buildings are staffed first when workers are scarce"
            >
              {b.priorityHigh ? '⭐ Priority staffing: ON' : '☆ Priority staffing: OFF'}
            </button>
          )}
        </div>
      )}
    </div>
  );
}

// ------------------------------------------------------------

function TradePanel({ engine, notify }: { engine: GameEngine; notify: (m: string, k: 'good' | 'bad' | 'info') => void }) {
  const [amount, setAmount] = useState(10);
  const doTrade = (fn: () => { ok: boolean; msg: string }) => {
    const res = fn();
    notify(res.msg, res.ok ? 'good' : 'bad');
  };
  return (
    <div className="space-y-2">
      <div className="text-[10px] text-yellow-200/60 leading-tight">
        Sell surplus to the <b>East (₽)</b> or the <b>West ($)</b>. Goods must be road-connected to a Customs House. Imports arrive at customs storage and are hauled away by truck.
      </div>
      <div className="flex items-center gap-1 text-xs">
        <span className="text-yellow-200/70 mr-1">Amount:</span>
        {[10, 50, 200].map(a => (
          <button key={a} onClick={() => setAmount(a)} className={`px-2 py-0.5 rounded font-bold ${amount === a ? 'bg-yellow-500 text-red-950' : 'bg-red-900/70'}`}>{a}</button>
        ))}
      </div>
      <div className="space-y-1">
        {(() => {
          const customs = [...engine.buildings.values()].find(b => BUILDINGS[b.defId].isCustoms && b.constructed);
          return ALL_RESOURCES.map(r => {
            const def = RESOURCES[r];
            const total = engine.totals[r];
            const sellable = engine.sellableStock(r);
            const customsFree = customs
              ? engine.capOf(customs, r) - engine.stockOf(customs, r) - engine.incomingOf(customs, r)
              : 0;
            const canSell = !!customs && sellable >= 1;
            const canBuy = (cur: 'east' | 'west') => {
              if (!customs || customsFree < 1) return false;
              const funds = cur === 'east' ? engine.rubles : engine.dollars;
              return funds >= engine.importPriceOf(r, cur); // at least one unit
            };
            return (
              <div key={r} className="rounded bg-red-900/40 px-2 py-1">
                <div className="flex items-center justify-between text-xs">
                  <span className="font-bold">{def.icon} {def.name}</span>
                  <span className="text-yellow-200/70" title="sellable = customs-connected stock minus what shops and industry keep for themselves">
                    sellable {Math.floor(sellable)} / {Math.floor(total)}
                  </span>
                </div>
                <div className="flex items-center gap-1 mt-1">
                  <button onClick={() => doTrade(() => engine.sell(r, amount, 'east'))} disabled={!canSell}
                    className="flex-1 rounded bg-red-800 hover:bg-red-700 disabled:opacity-30 text-[10px] font-bold py-0.5" title={`Sell to East at ₽${engine.priceOf(r, 'east').toFixed(1)}`}>
                    ➜₽{engine.priceOf(r, 'east').toFixed(1)}
                  </button>
                  <button onClick={() => doTrade(() => engine.sell(r, amount, 'west'))} disabled={!canSell}
                    className="flex-1 rounded bg-green-900 hover:bg-green-800 disabled:opacity-30 text-[10px] font-bold py-0.5" title={`Sell to West at $${engine.priceOf(r, 'west').toFixed(1)}`}>
                    ➜${engine.priceOf(r, 'west').toFixed(1)}
                  </button>
                  <button onClick={() => doTrade(() => engine.buy(r, amount, 'east'))} disabled={!canBuy('east')}
                    className="flex-1 rounded bg-red-950 hover:bg-red-800 disabled:opacity-30 text-[10px] font-bold py-0.5 border border-yellow-600/30" title="Import from East">
                    ₽{engine.importPriceOf(r, 'east').toFixed(1)}
                  </button>
                  <button onClick={() => doTrade(() => engine.buy(r, amount, 'west'))} disabled={!canBuy('west')}
                    className="flex-1 rounded bg-red-950 hover:bg-red-800 disabled:opacity-30 text-[10px] font-bold py-0.5 border border-green-600/30" title="Import from West">
                    ${engine.importPriceOf(r, 'west').toFixed(1)}
                  </button>
                </div>
              </div>
            );
          });
        })()}
      </div>
    </div>
  );
}

// ------------------------------------------------------------

function ObjectivesPanel({ engine }: { engine: GameEngine }) {
  const current = OBJECTIVES.find(o => !engine.objectivesDone.includes(o.id));
  return (
    <div className="space-y-1.5">
      <div className="text-[10px] text-yellow-200/60">Directives from the Planning Committee. Complete them for rewards.</div>
      {OBJECTIVES.map(o => {
        const done = engine.objectivesDone.includes(o.id);
        const isCurrent = current?.id === o.id;
        return (
          <div key={o.id} className={`rounded px-2 py-1.5 border ${done ? 'border-green-700/50 bg-green-900/20' : isCurrent ? 'border-yellow-500/70 bg-yellow-500/10' : 'border-yellow-600/20 bg-red-900/30'}`}>
            <div className="flex items-center gap-1.5 text-xs font-bold">
              <span>{done ? '✅' : isCurrent ? '⭐' : '▫️'}</span>
              <span className={done ? 'line-through text-yellow-200/50' : ''}>{o.title}</span>
            </div>
            <div className="text-[10px] text-yellow-200/60 ml-5">{o.description}</div>
            <div className="text-[10px] text-green-300/80 ml-5">
              {o.rewardRubles ? `+₽${o.rewardRubles.toLocaleString()} ` : ''}{o.rewardDollars ? `+$${o.rewardDollars.toLocaleString()}` : ''}
            </div>
          </div>
        );
      })}
    </div>
  );
}
