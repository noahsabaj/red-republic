import { useState, type ReactNode } from 'react';
import { buildingWorn } from '@/game/engine';
import type { GameEngine, BuildingInst } from '@/game/engine';
import { BALANCE, BUILDINGS, RESOURCES, ALL_RESOURCES, OBJECTIVES, FARM_SEASON } from '@/game/config';
import type { DepositType, ResourceId } from '@/game/config';
import type { Contract } from '@/game/engine';
import { fmtQty, fmtLevel, fmtRate, fmtPct, fmtMoney } from '@/game/format';
import type { SelectionItem } from '@/game/selection';
import type { PanelMode } from './HUD';
import { useEngineVersion } from '@/hooks/use-engine';
import { GameIcon } from '@/ui/GameIcon';
import { buildCostText, buildCostTotalText } from '@/ui/build-cost';
import type { BuildPayMode } from '@/ui/build-cost';
import { ToggleButton } from '@/components/menu/controls';
import type { BuildPolicy } from './GameCanvas';
import { audio } from '@/audio';
import MusicPanel from './MusicPanel';

interface Props {
  engine: GameEngine;
  mode: PanelMode;
  selection: SelectionItem[];
  policy: BuildPolicy;
  onClose: () => void;
  onOpenTrade: () => void;
  onArmBuild: (defId: string) => void;
  notify: (msg: string, kind: 'good' | 'bad' | 'info') => void;
}

export default function SidePanel({ engine, mode, selection, policy, onClose, onOpenTrade, onArmBuild, notify }: Props) {
  const payMode: BuildPayMode = policy.instant ? 'instant' : policy.autoBuy ? 'autoBuy' : 'materials';
  const currency = policy.currency;
  // the open detail panel mirrors live engine state — re-render on every bump
  useEngineVersion(engine);
  const single = selection.length === 1 ? selection[0] : null;
  const title = mode === 'trade' ? 'Foreign Trade'
    : mode === 'objectives' ? 'Five-Year Plan'
    : mode === 'stockpiles' ? 'National Stockpiles'
    : mode === 'music' ? 'State Radio'
    : mode === 'logistics' ? 'Logistics & Fleet'
    : selection.length > 1 ? `${selection.length} selected`
    : single?.kind === 'deposit' ? 'Deposit' : 'Building';
  const titleIcon = mode === 'trade' ? 'trade' : mode === 'objectives' ? 'plan' : mode === 'stockpiles' ? 'stockpiles' : mode === 'music' ? 'music' : mode === 'logistics' ? 'truck' : null;
  return (
    <div className="absolute right-0 top-24 bottom-16 z-10 flex pointer-events-none">
      <div className="pointer-events-auto flex flex-col w-72 m-2 rounded-lg border-2 border-yellow-600/60 bg-red-950/95 text-yellow-50 shadow-2xl overflow-hidden">
        <div className="flex items-center justify-between px-3 py-2 bg-red-900/60 border-b border-yellow-600/30">
          <span className="flex items-center gap-1.5 text-xs font-black uppercase tracking-widest text-yellow-400">
            {titleIcon && <GameIcon name={titleIcon} size={13} />}{title}
          </span>
          <button onClick={onClose} aria-label="Close panel" data-sfx="back" className="text-yellow-200/60 hover:text-yellow-100"><GameIcon name="close" size={14} /></button>
        </div>
        <div className="flex-1 overflow-y-auto soviet-scroll p-3">
          {mode === 'building' && (
            selection.length > 1
              ? <MultiInfo engine={engine} items={selection} payMode={payMode} currency={currency} onArmBuild={onArmBuild} notify={notify} />
              : single?.kind === 'deposit'
                ? <DepositInfo engine={engine} x={single.x} y={single.y} payMode={payMode} currency={currency} onArmBuild={onArmBuild} />
                : <BuildingInfo engine={engine} id={single?.kind === 'building' ? single.id : null} onOpenTrade={onOpenTrade} notify={notify} />
          )}
          {mode === 'trade' && <TradePanel engine={engine} notify={notify} />}
          {mode === 'objectives' && <ObjectivesPanel engine={engine} />}
          {mode === 'stockpiles' && <StockpilesPanel engine={engine} />}
          {mode === 'logistics' && <LogisticsPanel engine={engine} />}
          {mode === 'music' && <MusicPanel />}
        </div>
      </div>
    </div>
  );
}

// ------------------------------------------------------------

function Row({ label, value, ok }: { label: ReactNode; value: ReactNode; ok?: boolean }) {
  return (
    <div className="flex justify-between text-xs py-0.5">
      <span className="text-yellow-200/70 flex items-center gap-1">{label}</span>
      <span className={`font-bold ${ok === false ? 'text-red-300' : ok === true ? 'text-green-300' : ''}`}>{value}</span>
    </div>
  );
}

/** "iron 2.0 + coal 1.0 → steel 1.5" as icon-annotated JSX */
function FlowLine({ ins, outs }: { ins: [ResourceId, number][]; outs: [ResourceId, number][] }) {
  if (ins.length === 0 && outs.length === 0) return <span className="text-yellow-200/50">idle</span>;
  const seg = ([r, a]: [ResourceId, number], i: number, arr: unknown[]) => (
    <span key={r} className="inline-flex items-center gap-0.5">
      <GameIcon name={RESOURCES[r].icon} size={12} />{fmtRate(a)}{i < arr.length - 1 ? <span className="text-yellow-200/50 px-0.5">+</span> : null}
    </span>
  );
  return (
    <span className="inline-flex items-center flex-wrap gap-y-0.5">
      {ins.map(seg)}
      {ins.length > 0 && outs.length > 0 && <span className="text-yellow-200/60 px-1">→</span>}
      {outs.map(seg)}
    </span>
  );
}

/** One storage bin: icon + name, current/cap, incoming-truck note, and a fill bar. */
function StorageBar({ r, v, cap, inc }: { r: ResourceId; v: number; cap: number; inc: number }) {
  return (
    <div className="text-[0.6875rem]">
      <div className="flex justify-between">
        <span className="flex items-center gap-1"><GameIcon name={RESOURCES[r].icon} size={12} /> {RESOURCES[r].name}</span>
        <span className="font-bold">{fmtLevel(v)}/{cap}{inc > 0.05 ? <span className="text-yellow-200/60"> (+{fmtQty(inc)} <GameIcon name="truck" size={11} />)</span> : null}</span>
      </div>
      <div className="h-1.5 rounded bg-red-900 overflow-hidden">
        <div className="h-full bg-yellow-500/80" style={{ width: `${cap > 0 ? Math.min(100, (v / cap) * 100) : 0}%` }} />
      </div>
    </div>
  );
}

// ------------------------------------------------------------

function StockpilesPanel({ engine }: { engine: GameEngine }) {
  // net production flow across the republic, from the engine's live rates
  const flow: Partial<Record<ResourceId, number>> = {};
  for (const b of engine.buildings.values()) {
    if (!b.constructed) continue;
    const rates = engine.productionRates(b);
    for (const [r, a] of Object.entries(rates.outputs) as [ResourceId, number][]) flow[r] = (flow[r] ?? 0) + a;
    for (const [r, a] of Object.entries(rates.inputs) as [ResourceId, number][]) flow[r] = (flow[r] ?? 0) - a;
  }
  return (
    <div className="space-y-0.5">
      <div className="grid grid-cols-[1fr_auto_auto] gap-x-3 px-1 pb-1 text-[0.625rem] font-black uppercase tracking-wider text-yellow-400">
        <span>Resource</span><span className="text-right">Stock</span><span className="text-right">Net / day</span>
      </div>
      {ALL_RESOURCES.map(r => {
        const def = RESOURCES[r];
        const stock = fmtQty(engine.totals[r]);
        const sellable = fmtQty(engine.sellableStock(r));
        const net = (flow[r] ?? 0) - engine.citizenDemandOf(r);
        const idle = Math.abs(net) < 0.05;
        return (
          <div
            key={r}
            className="grid grid-cols-[1fr_auto_auto] gap-x-3 items-center rounded px-1 py-0.5 text-xs hover:bg-red-900/40"
            title={`${def.name}: production net ${(flow[r] ?? 0) >= 0 ? '+' : ''}${fmtRate(flow[r] ?? 0)}/day · citizen demand −${fmtRate(engine.citizenDemandOf(r))}/day`}
          >
            <span className="flex items-center gap-1.5"><GameIcon name={def.icon} size={13} /> {def.name}</span>
            <span className="text-right font-bold">{stock}<span className="text-yellow-200/40 font-normal"> ({sellable})</span></span>
            <span className={`text-right font-bold ${idle ? 'text-yellow-200/40' : net > 0 ? 'text-green-300' : 'text-red-300'}`}>
              {idle ? '—' : `${net > 0 ? '+' : ''}${fmtRate(net)}`}
            </span>
          </div>
        );
      })}
      <p className="pt-2 text-[0.625rem] leading-snug text-yellow-200/50">
        Stock (sellable): total inventory, with the customs-connected share industry can spare in parentheses.
        Net/day is live production minus factory inputs and citizen demand.
      </p>
    </div>
  );
}

// ------------------------------------------------------------

function MultiInfo({ engine, items, payMode, currency, onArmBuild, notify }: { engine: GameEngine; items: SelectionItem[]; payMode: BuildPayMode; currency: 'east' | 'west'; onArmBuild: (defId: string) => void; notify: (msg: string, kind: 'good' | 'bad' | 'info') => void }) {
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
  const sites = buildings.filter(b => !b.constructed);

  return (
    <div className="space-y-3">
      {buildings.length > 0 && (
        <div className="space-y-2">
          <div className="text-[0.625rem] font-black uppercase tracking-wider text-yellow-400">Buildings ({buildings.length})</div>
          <div className="space-y-0.5">
            {[...typeCounts.entries()].map(([defId, n]) => (
              <Row key={defId} label={<><GameIcon name={BUILDINGS[defId].icon} size={12} /> {BUILDINGS[defId].name}</>} value={`×${n}`} />
            ))}
          </div>
          <div className="grid grid-cols-2 gap-x-2">
            {jobs > 0 && <Row label={<><GameIcon name="staff" size={12} /> Staff</>} value={`${staff}/${jobs}`} ok={staff >= jobs * 0.8} />}
            {powerDraw > 0 && <Row label={<><GameIcon name="power" size={12} /> Draw</>} value={`${powerDraw.toFixed(1)} MW`} />}
          </div>
          {(Object.keys(flowIn).length > 0 || Object.keys(flowOut).length > 0) && (
            <div className="rounded bg-red-900/40 p-2">
              <div className="text-[0.625rem] font-black uppercase tracking-wider text-yellow-400 mb-1">Combined production / day</div>
              <div className="text-xs">
                <FlowLine ins={Object.entries(flowIn) as [ResourceId, number][]} outs={Object.entries(flowOut) as [ResourceId, number][]} />
              </div>
            </div>
          )}
          {staffed.length > 0 && (
            <button
              onClick={() => engine.setStaffPriorityMany(staffed.map(b => b.id), !allPriority)}
              className={`w-full rounded font-bold text-xs py-1.5 ${allPriority ? 'bg-yellow-500 text-red-950' : 'bg-red-900/70 hover:bg-red-800'}`}
              title="High-priority buildings are staffed first when workers are scarce"
            >
              <GameIcon name="star" size={12} /> {allPriority ? 'Priority staffing: ON for all' : `Set priority staffing for ${staffed.length} building${staffed.length > 1 ? 's' : ''}`}
            </button>
          )}
          {sites.length > 0 && (
            <div>
              <div className="mb-1 text-[0.625rem] font-black uppercase tracking-wider text-yellow-400/80">Construction priority ({sites.length} site{sites.length > 1 ? 's' : ''})</div>
              <div className="flex gap-1.5" role="group" aria-label="Construction priority">
                {([[1, 'High'], [0, 'Normal'], [-1, 'Low']] as const).map(([tier, label]) => {
                  const all = sites.every(b => (b.buildPriority ?? 0) === tier);
                  return (
                    <button
                      key={tier}
                      aria-pressed={all}
                      data-sfx="toggle"
                      onClick={() => engine.setSitePriorityMany(sites.map(b => b.id), tier)}
                      className={`flex-1 rounded px-2 py-1.5 text-xs font-bold ${all ? 'bg-yellow-500 text-red-950' : 'border border-yellow-600/30 bg-red-900/50 text-yellow-100/80 hover:bg-red-800'}`}
                    >
                      {label}
                    </button>
                  );
                })}
              </div>
            </div>
          )}
          {sites.length > 0 && (() => {
            const rubleCost = sites.reduce((sum, b) => sum + engine.autoBuyRemainingCost(b.id, 'east'), 0);
            const dollarCost = sites.reduce((sum, b) => sum + engine.autoBuyRemainingCost(b.id, 'west'), 0);
            const allEast = sites.every(b => !!b.autoBought && (b.importCurrency ?? 'east') === 'east');
            const allWest = sites.every(b => !!b.autoBought && b.importCurrency === 'west');
            const handleBulkImport = (cur: 'east' | 'west' | null) => {
              const res = engine.setSiteImportMany(sites.map(b => b.id), cur);
              if (res.succeeded > 0) {
                if (cur === 'east') notify(`Auto-buying materials for ${res.succeeded} site${res.succeeded > 1 ? 's' : ''} (₽${fmtMoney(res.totalCost)})`, 'good');
                else if (cur === 'west') notify(`Auto-buying materials for ${res.succeeded} site${res.succeeded > 1 ? 's' : ''} ($${fmtMoney(res.totalCost)})`, 'good');
                else notify(`Disabled material auto-buy for ${res.succeeded} site${res.succeeded > 1 ? 's' : ''}`, 'info');
              } else if (res.reason) {
                notify(res.reason, 'bad');
              }
            };
            return (
              <div className="space-y-2 border-t border-yellow-600/20 pt-2">
                <div>
                  <div className="mb-1 text-[0.625rem] font-black uppercase tracking-wider text-yellow-400/80">
                    Import remaining materials ({sites.length} site{sites.length > 1 ? 's' : ''})
                  </div>
                  <div className="flex gap-1.5">
                    <button
                      aria-pressed={allEast}
                      data-sfx="toggle"
                      onClick={() => handleBulkImport(allEast ? null : 'east')}
                      className={`flex-1 rounded px-2 py-1.5 text-xs font-bold tabular-nums ${allEast ? 'bg-yellow-500 text-red-950' : 'border border-yellow-600/30 bg-red-900/50 text-yellow-100/80 hover:bg-red-800'}`}
                      title="Enable auto-buy for all selected sites using Rubles (₽)"
                    >
                      {allEast ? 'Auto-buying (₽)' : `₽ Auto-buy (₽${fmtMoney(rubleCost)})`}
                    </button>
                    <button
                      aria-pressed={allWest}
                      data-sfx="toggle"
                      onClick={() => handleBulkImport(allWest ? null : 'west')}
                      className={`flex-1 rounded px-2 py-1.5 text-xs font-bold tabular-nums ${allWest ? 'bg-yellow-500 text-red-950' : 'border border-yellow-600/30 bg-red-900/50 text-yellow-100/80 hover:bg-red-800'}`}
                      title="Enable auto-buy for all selected sites using Dollars ($)"
                    >
                      {allWest ? 'Auto-buying ($)' : `$ Auto-buy ($${fmtMoney(dollarCost)})`}
                    </button>
                  </div>
                </div>
              </div>
            );
          })()}
        </div>
      )}

      {depositKinds.size > 0 && (
        <div className="space-y-2">
          <div className="text-[0.625rem] font-black uppercase tracking-wider text-yellow-400">Deposits</div>
          {[...depositKinds.entries()].map(([kind, g]) => {
            const res = RESOURCES[kind];
            const miner = Object.values(BUILDINGS).find(d => d.requiresDeposit === kind)!;
            const outputsPerMine = Object.entries(miner.outputs ?? {}) as [ResourceId, number][];
            return (
              <div key={kind} className="rounded bg-red-900/40 p-2 space-y-1">
                <div className="flex justify-between text-xs font-bold">
                  <span className="flex items-center gap-1"><GameIcon name={res.icon} size={12} /> {res.name}</span>
                  <span>{g.total} tile{g.total > 1 ? 's' : ''}{g.free < g.total ? ` · ${g.free} free` : ''}</span>
                </div>
                {g.free > 0 && (
                  <>
                    <div className="text-[0.6875rem] text-yellow-200/80">
                      One {miner.name} per free tile:{' '}
                      {outputsPerMine.map(([r, a]) => (
                        <span key={r} className="inline-flex items-center gap-0.5"><GameIcon name={RESOURCES[r].icon} size={11} />{fmtRate(a * g.free)}/day</span>
                      ))}
                      <span className="text-yellow-200/60"> · {miner.workers * g.free} workers · {buildCostTotalText(engine, miner.id, g.free, payMode, currency)} total</span>
                    </div>
                    <button
                      onClick={() => onArmBuild(miner.id)}
                      data-sfx="none" // the setTool funnel voices toolArm
                      className="w-full rounded bg-yellow-500 text-red-950 font-bold text-xs py-1 hover:bg-yellow-400"
                    >
                      <GameIcon name="builders" size={12} /> Build {miner.name} ({buildCostText(engine, miner.id, payMode, currency)} each)
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

function DepositInfo({ engine, x, y, payMode, currency, onArmBuild }: { engine: GameEngine; x: number; y: number; payMode: BuildPayMode; currency: 'east' | 'west'; onArmBuild: (defId: string) => void }) {
  const cluster = engine.depositClusterAt(x, y);
  if (!cluster) return <div className="text-xs text-yellow-200/60">Nothing of value here.</div>;
  const res = RESOURCES[cluster.kind];
  const miner = Object.values(BUILDINGS).find(d => d.requiresDeposit === cluster.kind)!;
  const exploited = cluster.exploitedBy;
  const outputs = Object.entries(miner.outputs ?? {}) as [ResourceId, number][];

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2">
        <GameIcon name={res.icon} size={26} className="text-yellow-300" />
        <div>
          <div className="font-bold text-sm">{DEPOSIT_NAMES[cluster.kind]} Deposit</div>
          <div className="text-[0.625rem] text-yellow-200/50">
            {exploited ? `Worked by a ${BUILDINGS[exploited.defId].name}.` : 'Unexploited mineral wealth of the republic.'}
          </div>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-x-2">
        <Row label={<><GameIcon name={res.icon} size={12} /> Resource</>} value={res.name} />
        <Row label={<><GameIcon name="map" size={12} /> Cluster</>} value={`${cluster.tiles.length} tile${cluster.tiles.length > 1 ? 's' : ''}`} />
        <Row label={<><GameIcon name="pick" size={12} /> Status</>} value={exploited ? 'Exploited' : 'Unexploited'} ok={!!exploited} />
      </div>

      <div className="rounded bg-red-900/40 p-2">
        <div className="text-[0.625rem] font-black uppercase tracking-wider text-yellow-400 mb-1">{miner.name}</div>
        <div className="text-xs">
          {outputs.map(([r, a]) => (
            <span key={r} className="inline-flex items-center gap-0.5"><GameIcon name={RESOURCES[r].icon} size={12} />{a}/day at full staff</span>
          ))}
          <span className="text-yellow-200/60"> · {miner.workers} workers{miner.power > 0 ? ` · ${miner.power} MW` : ''}</span>
        </div>
      </div>

      {!exploited && (
        <button
          onClick={() => onArmBuild(miner.id)}
          data-sfx="none" // the setTool funnel voices toolArm
          className="w-full rounded bg-yellow-500 text-red-950 font-bold text-xs py-1.5 hover:bg-yellow-400"
          title={`Arm the build tool — place the ${miner.name} on one of this cluster's tiles`}
        >
          <GameIcon name="builders" size={12} /> Build {miner.name} ({buildCostText(engine, miner.id, payMode, currency)})
        </button>
      )}
    </div>
  );
}

// ------------------------------------------------------------

function BuildingInfo({ engine, id, onOpenTrade, notify }: { engine: GameEngine; id: number | null; onOpenTrade: () => void; notify: (msg: string, kind: 'good' | 'bad' | 'info') => void }) {
  const b: BuildingInst | undefined = id ? engine.buildings.get(id) : undefined;
  if (!b) return <div className="text-xs text-yellow-200/60">Select a building on the map to inspect it.</div>;
  const def = BUILDINGS[b.defId];

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2">
        <GameIcon name={def.icon} size={26} className="text-yellow-300" />
        <div>
          <div className="font-bold text-sm">{def.name}</div>
          <div className="text-[0.625rem] text-yellow-200/50">{def.description}</div>
        </div>
      </div>

      {!b.constructed ? (
        <div className="space-y-1.5">
          <div className="text-xs font-bold text-yellow-400 flex items-center gap-1">
            <GameIcon name={b.paused ? 'contract' : 'builders'} size={12} />
            {b.paused ? ' Planned — not started' : ` Under construction — ${fmtPct(b.progress / def.labor)}%`}
          </div>
          {!b.paused && (
            <div className="h-2 rounded bg-red-900 overflow-hidden">
              <div className="h-full bg-yellow-500" style={{ width: `${(b.progress / def.labor) * 100}%` }} />
            </div>
          )}
          <div className="text-[0.6875rem] text-yellow-200/70">
            Labor: {fmtQty(b.progress)} / {def.labor} worker-days
          </div>
          {Object.entries(def.materials).map(([r, amt]) => {
            const stock = b.stock[r as ResourceId] ?? 0;
            const inc = b.incoming[r as ResourceId] ?? 0;
            return (
              <Row
                key={r}
                label={<><GameIcon name={RESOURCES[r as ResourceId].icon} size={12} /> {RESOURCES[r as ResourceId].name}</>}
                value={<>{fmtQty(stock)}{inc > 0.05 ? <span className="text-yellow-200/60"> (+{fmtQty(inc)} <GameIcon name="truck" size={11} />)</span> : null} / {amt}</>}
                ok={stock >= amt}
              />
            );
          })}
          <Row
            label={<><GameIcon name="road" size={12} /> Road</>}
            value={b.roadConnected ? 'Connected' : b.connected ? 'Off-road — slow' : 'Isolated!'}
            ok={b.roadConnected}
          />

          {/* ---- this site's construction policy ---- */}
          <div className="space-y-1.5 border-t border-yellow-600/20 pt-2">
            {b.paused && (
              <button
                onClick={() => { const r = engine.commenceSite(b.id); if (!r.ok && r.reason) notify(r.reason, 'bad'); }}
                data-sfx="confirm"
                className="w-full rounded bg-yellow-500 text-red-950 font-bold text-xs py-1.5 hover:bg-yellow-400"
              >
                <GameIcon name="builders" size={12} /> Begin construction
              </button>
            )}

            <div>
              <div className="mb-1 text-[0.625rem] font-black uppercase tracking-wider text-yellow-400/80">Construction priority</div>
              <div className="flex gap-1.5" role="group" aria-label="Construction priority">
                {([[1, 'High'], [0, 'Normal'], [-1, 'Low']] as const).map(([tier, label]) => {
                  const active = (b.buildPriority ?? 0) === tier;
                  return (
                    <button
                      key={tier}
                      aria-pressed={active}
                      data-sfx="toggle"
                      onClick={() => engine.setSitePriority(b.id, tier)}
                      className={`flex-1 rounded px-2 py-1.5 text-xs font-bold ${active ? 'bg-yellow-500 text-red-950' : 'border border-yellow-600/30 bg-red-900/50 text-yellow-100/80 hover:bg-red-800'}`}
                      title={tier === 1 ? 'Build this site before Normal and Low sites — its crews and materials come first' : tier === -1 ? 'Build this site only after higher-priority sites are crewed' : 'Default — shares builders evenly with other Normal sites'}
                    >
                      {label}
                    </button>
                  );
                })}
              </div>
            </div>

            <div>
              <div className="mb-1 text-[0.625rem] font-black uppercase tracking-wider text-yellow-400/80">Import remaining materials</div>
              <div className="flex gap-1.5">
                {(['east', 'west'] as const).map(cur => {
                  const active = !!b.autoBought && (b.importCurrency ?? 'east') === cur;
                  const cost = engine.autoBuyRemainingCost(b.id, cur);
                  return (
                    <button
                      key={cur}
                      aria-pressed={active}
                      data-sfx="toggle"
                      disabled={!!b.autoBought && !active}
                      onClick={() => { const r = engine.setSiteImport(b.id, active ? null : cur); if (!r.ok && r.reason) notify(r.reason, 'bad'); }}
                      className={`flex-1 rounded px-2 py-1.5 text-xs font-bold tabular-nums disabled:opacity-30 ${active ? 'bg-yellow-500 text-red-950' : 'border border-yellow-600/30 bg-red-900/50 text-yellow-100/80 hover:bg-red-800'}`}
                      title={active ? 'Stop importing — already-paid cargo still arrives' : `Pay to import the remaining bill from the ${cur === 'east' ? 'East (₽)' : 'West ($)'}`}
                    >
                      {cur === 'east' ? '₽' : '$'}{cost.toLocaleString()}
                    </button>
                  );
                })}
              </div>
            </div>

            <button
              aria-pressed={b.foreignLabor !== false}
              data-sfx="toggle"
              onClick={() => engine.setSiteForeignLabor(b.id, b.foreignLabor === false)}
              className={`w-full rounded font-bold text-xs py-1.5 ${b.foreignLabor !== false ? 'bg-yellow-500 text-red-950' : 'border border-yellow-600/30 bg-red-900/50 text-yellow-100/80 hover:bg-red-800'}`}
              title="Whether this site may hire paid foreign builders beyond your citizens"
            >
              <GameIcon name="users" size={12} /> Foreign builders: {b.foreignLabor !== false ? 'ON' : 'OFF'}
            </button>

            <button
              onClick={() => { const r = engine.finishSiteInstant(b.id); if (!r.ok && r.reason) notify(r.reason, 'bad'); }}
              data-sfx="confirm"
              className="w-full rounded border border-green-600/40 bg-green-900/40 text-green-100 font-bold text-xs py-1.5 hover:bg-green-800/60"
              title="Complete this site immediately with an imported Western prefab, paid in dollars"
            >
              <GameIcon name="download" size={12} /> Instant finish (${engine.instantFinishCost(b.id).toLocaleString()})
            </button>
          </div>

          <div className="text-[0.625rem] text-yellow-200/50">Materials arrive by truck. Builders come from a staffed Construction Office.</div>
        </div>
      ) : (
        <div className="space-y-2">
          <div className="grid grid-cols-2 gap-x-2">
            {def.workers > 0 && <Row label={<><GameIcon name="staff" size={12} /> Staff</>} value={`${b.staff}/${def.workers}`} ok={b.staff >= def.workers * 0.8} />}
            <Row
              label={<><GameIcon name="road" size={12} /> Road</>}
              value={b.roadConnected ? 'Connected' : b.connected ? 'Off-road — slow' : 'Isolated!'}
              ok={b.roadConnected}
            />
            {def.power > 0 && <Row label={<><GameIcon name="power" size={12} /> Power</>} value={b.powered ? `${def.power} MW` : 'No power!'} ok={b.powered} />}
            {def.powerOutput !== undefined && <Row label={<><GameIcon name="power" size={12} /> Output</>} value={`${(def.powerOutput * b.eff * b.coalFactor).toFixed(1)} MW`} />}
            {def.heatOutput !== undefined && <Row label={<><GameIcon name="heat" size={12} /> Output</>} value={(def.heatOutput * b.eff * b.coalFactor).toFixed(1)} />}
            {def.heat > 0 && <Row label={<><GameIcon name="heat" size={12} /> Heat</>} value={b.heated ? 'Warm' : 'Freezing!'} ok={b.heated} />}
            {def.housingCapacity && <Row label={<><GameIcon name="beds" size={12} /> Capacity</>} value={`${def.housingCapacity} citizens`} />}
            {def.serviceRadius && <Row label={<><GameIcon name="coverage" size={12} /> Coverage</>} value={`${def.serviceRadius} tiles`} />}
            {def.isFarm && <Row label={<><GameIcon name="fields" size={12} /> Fields</>} value={`${b.farmFields} plots · season ×${(FARM_SEASON[engine.month] ?? 0).toFixed(2)}`} />}
            {def.workers > 0 && <Row label={<><GameIcon name="eff" size={12} /> Efficiency</>} value={`${fmtPct(b.eff)}%`} ok={b.eff > 0.6} />}
            {def.isCustoms && <Row label={<><GameIcon name="trade" size={12} /> Clears</>} value={`${Math.floor(BALANCE.customsThroughputPerDay * b.eff)}/day`} ok={b.eff > 0} />}
            {def.wear && <Row label={<><GameIcon name="machinery" size={12} /> Machines</>} value={buildingWorn(b) ? 'Worn — deliver machinery!' : 'Maintained'} ok={!buildingWorn(b)} />}
            {def.isMotorDepot && <Row label={<><GameIcon name="truck" size={12} /> Fleet</>} value={`${engine.trucksFrom(b)} trucks`} ok={b.connected && b.staff > 0} />}
            {def.isGasStation && <Row label={<><GameIcon name="fuel" size={12} /> Fuels fleet</>} value={(b.stock.fuel ?? 0) < 1 ? 'empty — refill!' : `${fmtQty(b.stock.fuel ?? 0)} on hand`} ok={(b.stock.fuel ?? 0) >= 1} />}
            {def.isPort && (() => {
              const s = engine.portStatus(b);
              return <Row label={<><GameIcon name="port" size={12} /> Barges</>} value={s.label} ok={s.state === 'relaying' || s.state === 'ready'} />;
            })()}
          </div>

          {(def.inputs || def.outputs) && (() => {
            // the engine's own numbers — includes season, fields, forest and
            // input starvation, so this always matches what actually happens
            const rates = engine.productionRates(b);
            return (
              <div className="rounded bg-red-900/40 p-2">
                <div className="text-[0.625rem] font-black uppercase tracking-wider text-yellow-400 mb-1">Production / day</div>
                <div className="text-xs">
                  <FlowLine ins={Object.entries(rates.inputs) as [ResourceId, number][]} outs={Object.entries(rates.outputs) as [ResourceId, number][]} />
                </div>
              </div>
            );
          })()}

          {(() => {
            // Group storage so inputs, output and machinery upkeep read apart
            // instead of as one undifferentiated pile. Classify each bin once by
            // precedence output > input > upkeep > goods; a machinery (wear) bin
            // is ALWAYS shown, even at 0, so a worn building visibly reads
            // 'Machinery 0/6' — the crux of the old confusion.
            const groups: { key: string; label: string; rs: ResourceId[] }[] = [
              { key: 'in', label: 'Inputs', rs: [] },
              { key: 'out', label: 'Output', rs: [] },
              { key: 'up', label: 'Upkeep', rs: [] },
              { key: 'goods', label: def.serviceType === 'shop' ? 'Goods' : 'Storage', rs: [] },
            ];
            const keys = new Set<ResourceId>(Object.keys(def.storage) as ResourceId[]);
            for (const r of Object.keys(def.wear ?? {}) as ResourceId[]) keys.add(r);
            for (const r of keys) {
              if ((def.outputs?.[r] ?? 0) > 0) groups[1].rs.push(r);
              else if ((def.inputs?.[r] ?? 0) > 0) groups[0].rs.push(r);
              else if ((def.wear?.[r] ?? 0) > 0) groups[2].rs.push(r);
              else if ((b.stock[r] ?? 0) > 0.05 || (b.incoming[r] ?? 0) > 0.05 || def.serviceType === 'shop') groups[3].rs.push(r);
            }
            const shown = groups.filter(g => g.rs.length > 0);
            if (shown.length === 0) return null;
            return (
              <div className="space-y-2">
                {shown.map(g => (
                  <div key={g.key}>
                    <div className="text-[0.625rem] font-black uppercase tracking-wider text-yellow-400 mb-1">{g.label}</div>
                    <div className="space-y-1">
                      {g.rs.map(r => <StorageBar key={r} r={r} v={b.stock[r] ?? 0} cap={def.storage[r] ?? 0} inc={b.incoming[r] ?? 0} />)}
                    </div>
                  </div>
                ))}
              </div>
            );
          })()}

          {def.isCustoms && (
            <button onClick={onOpenTrade} className="w-full rounded bg-yellow-500 text-red-950 font-bold text-xs py-1.5 hover:bg-yellow-400">
              <GameIcon name="trade" size={12} /> Open Foreign Trade
            </button>
          )}

          {def.workers > 0 && (
            <button
              onClick={() => engine.toggleStaffPriority(b.id)}
              className={`w-full rounded font-bold text-xs py-1.5 ${b.priorityHigh ? 'bg-yellow-500 text-red-950' : 'bg-red-900/70 hover:bg-red-800'}`}
              title="High-priority buildings are staffed first when workers are scarce"
            >
              <GameIcon name="star" size={12} /> Priority staffing: {b.priorityHigh ? 'ON' : 'OFF'}
            </button>
          )}
        </div>
      )}
    </div>
  );
}

// ------------------------------------------------------------

/** Compact dark number input; commits every valid keystroke via the engine mutator. */
function NumInput({ value, onValue, step = 10, w = 'w-16' }: { value: number; onValue: (v: number) => void; step?: number; w?: string }) {
  return (
    <input
      type="number" min={0} step={step} value={value}
      onChange={ev => { const v = ev.target.valueAsNumber; if (Number.isFinite(v)) onValue(v); }}
      className={`${w} rounded bg-red-950/60 border border-yellow-600/30 px-1 py-0.5 text-[0.6875rem] font-bold text-yellow-50`}
    />
  );
}

function ContractCard({ engine, c }: { engine: GameEngine; c: Contract }) {
  const cur = c.bloc === 'east' ? '₽' : '$';
  const blocName = c.bloc === 'east' ? 'East' : 'West';
  const daysLeft = engine.contractDaysLeft(c);
  return (
    <div className="rounded bg-red-900/40 p-2 text-[0.6875rem] space-y-1">
      <div className="flex items-center justify-between">
        <span className="font-bold flex items-center gap-1">
          <GameIcon name={RESOURCES[c.r].icon} size={12} /> {c.amount} {RESOURCES[c.r].name} → {blocName}
        </span>
        <span className="font-bold" title="Locked at offer time — a premium over the market price">{cur}{c.pricePerUnit.toFixed(1)}/u</span>
      </div>
      {c.state === 'offer' && (
        <>
          <div className="text-[0.625rem] text-yellow-200/60">
            Deliver within {daysLeft} days · offer withdrawn in {engine.offerDaysLeft(c)} days
          </div>
          <div className="flex gap-1">
            <button onClick={() => engine.acceptContract(c.id)} data-sfx="confirm" className="flex-1 rounded bg-yellow-500 text-red-950 font-bold py-0.5 hover:bg-yellow-400">Accept</button>
            <button onClick={() => engine.declineContract(c.id)} data-sfx="back" className="flex-1 rounded bg-red-900/70 font-bold py-0.5 hover:bg-red-800">Decline</button>
          </div>
        </>
      )}
      {c.state === 'active' && (
        <>
          <div className="h-1.5 rounded bg-red-900 overflow-hidden">
            <div className="h-full bg-yellow-500/80" style={{ width: `${Math.min(100, (c.delivered / c.amount) * 100)}%` }} />
          </div>
          <div className="flex justify-between text-[0.625rem] text-yellow-200/60">
            <span>{fmtQty(c.delivered)}/{c.amount} delivered</span>
            <span className={daysLeft <= 15 ? 'text-red-300 font-bold' : ''}>{Math.max(0, daysLeft)} days left</span>
          </div>
        </>
      )}
    </div>
  );
}

function LogisticsPanel({ engine }: { engine: GameEngine }) {
  useEngineVersion(engine);
  const f = engine.fleetStatus();
  const bs = [...engine.buildings.values()];
  const offices = bs.filter(b => b.constructed && BUILDINGS[b.defId].isConstructionOffice);
  const depots = bs.filter(b => b.constructed && BUILDINGS[b.defId].isMotorDepot);
  const stations = bs.filter(b => b.constructed && BUILDINGS[b.defId].isGasStation);
  const util = f.max > 0 ? Math.min(100, (f.active / f.max) * 100) : 0;
  const fuelLimited = f.driverTrucks > f.depotTrucks;
  const idle = f.driverTrucks - f.depotTrucks;
  const line = (key: number, name: string, icon: string, value: string, sub: string, bad = false) => (
    <div key={key} className="flex items-center justify-between text-[0.6875rem] rounded bg-red-900/30 px-2 py-1">
      <span className="flex items-center gap-1"><GameIcon name={icon} size={12} /> {name} <span className="text-yellow-200/40">· {sub}</span></span>
      <span className={`font-bold ${bad ? 'text-red-300' : ''}`}>{value}</span>
    </div>
  );
  return (
    <div className="space-y-3">
      <section className="rounded bg-red-900/40 p-2 space-y-1.5">
        <div className="flex items-center justify-between">
          <span className="text-[0.625rem] font-black uppercase tracking-wider text-yellow-400">Fleet in use</span>
          <span className="font-bold flex items-center gap-1"><GameIcon name="truck" size={12} /> {f.active}/{f.max}</span>
        </div>
        <div className="h-2 rounded bg-red-900 overflow-hidden">
          <div className={`h-full ${f.max > 0 && f.active >= f.max ? 'bg-red-500/80' : 'bg-yellow-500/80'}`} style={{ width: `${util}%` }} />
        </div>
        <div className="text-[0.625rem] text-yellow-200/70 leading-relaxed">
          Concurrent hauling trucks. Construction Offices give a fuel-free base; Motor Depots add a truck per driver, capped by the fuel your Gas Stations hold. Grow the fleet by building depots and keeping them fuelled.
        </div>
        <Row label="Office base (fuel-free)" value={f.officeTrucks} />
        <Row label="Depot drivers" value={f.driverTrucks} />
        <Row label="Depot trucks running" value={f.depotTrucks} ok={fuelLimited ? false : undefined} />
      </section>

      <section className="rounded bg-red-900/40 p-2 space-y-1">
        <div className="flex items-center justify-between">
          <span className="text-[0.625rem] font-black uppercase tracking-wider text-yellow-400">Fuel</span>
          <span className="font-bold flex items-center gap-1"><GameIcon name="fuel" size={12} /> {fmtQty(f.gasFuel)}</span>
        </div>
        <div className="text-[0.625rem] text-yellow-200/70 leading-relaxed">
          {stations.length === 0
            ? 'No Gas Station — Motor Depot trucks cannot run. Build one and feed it fuel from a refinery or imports.'
            : fuelLimited
              ? `Low on fuel — ${idle} depot truck${idle === 1 ? '' : 's'} idle. Route more fuel to your stations.`
              : Number.isFinite(f.fuelDaysLeft)
                ? `About ${Math.floor(f.fuelDaysLeft)} day${Math.floor(f.fuelDaysLeft) === 1 ? '' : 's'} of fuel left at the current pace.`
                : 'Well supplied — the fleet is idle or fuel is plentiful.'}
        </div>
      </section>

      {(offices.length + depots.length + stations.length) > 0 && (
        <section className="space-y-1">
          <div className="text-[0.625rem] font-black uppercase tracking-wider text-yellow-400">Fleet buildings</div>
          {depots.map(b => line(b.id, 'Motor Depot', 'truck', `${engine.trucksFrom(b)} trucks`, `${b.staff}/${BUILDINGS[b.defId].workers} drivers`, !b.connected))}
          {stations.map(b => line(b.id, 'Gas Station', 'fuel', `${fmtQty(b.stock.fuel ?? 0)}/${BUILDINGS[b.defId].storage.fuel}`, !b.connected ? 'isolated' : (b.stock.fuel ?? 0) < 1 ? 'empty' : 'fuelling', !b.connected || (b.stock.fuel ?? 0) < 1))}
          {offices.map(b => line(b.id, 'Construction Office', 'constructionOffice', `${engine.trucksFrom(b)} trucks`, `${b.staff}/${BUILDINGS[b.defId].workers} staff`, !b.connected))}
        </section>
      )}
    </div>
  );
}

function TradePanel({ engine, notify }: { engine: GameEngine; notify: (m: string, k: 'good' | 'bad' | 'info') => void }) {
  const [amount, setAmount] = useState(10);
  const doTrade = (fn: () => { ok: boolean; msg: string }) => {
    const res = fn();
    audio.sfx(res.ok ? 'coin' : 'error');
    notify(res.msg, res.ok ? 'good' : 'bad');
  };
  const at = engine.autoTrade;
  const led = engine.tradeLedger.yesterday;
  const today = engine.tradeLedger.today;
  const offers = engine.contracts.filter(c => c.state === 'offer');
  const active = engine.contracts.filter(c => c.state === 'active');
  const closed = engine.contracts.filter(c => c.state === 'done' || c.state === 'failed');
  const ledImports = Object.entries(led.imports) as [ResourceId, number][];
  const ledExports = Object.entries(led.exports) as [ResourceId, number][];
  const money = (v: number, sym: string) =>
    `${v >= 0 ? '+' : '−'}${sym}${Math.abs(v) < 10 ? Math.abs(v).toFixed(1) : Math.abs(Math.round(v)).toLocaleString()}`;
  return (
    <div className="space-y-3">
      <section className="rounded bg-red-900/40 p-2 space-y-1.5">
        <ToggleButton
          on={at.enabled}
          onChange={v => engine.setAutoTradeEnabled(v)}
          icon="trade"
          label="Auto-trade"
          className="uppercase tracking-wider"
        />
        <div className="text-[0.625rem] text-yellow-200/60 leading-tight">
          Standing orders of the Foreign Trade Directorate. Each staffed Customs House clears up to {BALANCE.customsThroughputPerDay} units a day; set per-good rules below.
        </div>
        {at.enabled && (
          <>
            <div className="flex items-center justify-between text-[0.6875rem]">
              <span className="text-yellow-200/70">Customs capacity today</span>
              <span className="font-bold">{today.used}/{today.capacity}</span>
            </div>
            <div className="flex items-center gap-2 text-[0.6875rem]" title="Auto-imports never spend the treasury below these floors — emergency machinery money stays safe">
              <span className="text-yellow-200/70 shrink-0">Reserve</span>
              <label className="flex items-center gap-1">₽ <NumInput value={at.reserveRubles} onValue={v => engine.setAutoTradeReserve('east', v)} step={500} /></label>
              <label className="flex items-center gap-1 text-green-300">$ <NumInput value={at.reserveDollars} onValue={v => engine.setAutoTradeReserve('west', v)} step={100} /></label>
            </div>
            <div className="text-[0.625rem] text-yellow-200/70 border-t border-yellow-600/20 pt-1 leading-relaxed">
              <span className="font-black uppercase tracking-wider text-yellow-400/80 mr-1">Yesterday</span>
              {ledImports.length === 0 && ledExports.length === 0
                ? 'no automated trade'
                : (
                  <>
                    {ledImports.length > 0 && <>in{' '}{ledImports.map(([r, n]) => <span key={r} className="inline-flex items-center gap-0.5 mr-1"><GameIcon name={RESOURCES[r].icon} size={11} />{Math.round(n)}</span>)}</>}
                    {ledExports.length > 0 && <>out{' '}{ledExports.map(([r, n]) => <span key={r} className="inline-flex items-center gap-0.5 mr-1"><GameIcon name={RESOURCES[r].icon} size={11} />{Math.round(n)}</span>)}</>}
                    <span className="font-bold">{money(led.rubles, '₽')}</span>
                    {led.dollars !== 0 && <span className="font-bold text-green-300 ml-1">{money(led.dollars, '$')}</span>}
                  </>
                )}
            </div>
            {today.blocked.length > 0 && (
              <div className="text-[0.625rem] text-red-300 font-bold">Stalled: {today.blocked.join('; ')}</div>
            )}
          </>
        )}
      </section>

      <section className="rounded bg-red-900/40 p-2 space-y-1.5">
        <ToggleButton
          on={engine.repairImportsEnabled}
          onChange={v => engine.setRepairImports(v)}
          icon="machinery"
          label="Repair imports"
          title="When a worn building can't get machinery from your own industry, buy replacement machines at the border to keep it running"
          className="uppercase tracking-wider"
        />
        <div className="text-[0.625rem] text-yellow-200/60 leading-tight">
          A worn factory with no domestic machinery in reach buys replacement machines at the border. Needs a Customs House; never spends below the auto-trade reserve floor.
        </div>
        {engine.repairImportsEnabled && (
          <div className="flex items-center gap-2 text-[0.6875rem]">
            <span className="text-yellow-200/70 shrink-0">Pay with</span>
            {(['east', 'west'] as const).map(cur => (
              <button
                key={cur}
                onClick={() => engine.setRepairImports(true, cur)}
                className={`px-2 py-0.5 rounded font-bold ${
                  engine.repairImportCurrency === cur ? 'bg-yellow-500 text-red-950'
                  : cur === 'west' ? 'bg-red-950/60 hover:bg-red-800 text-green-300'
                  : 'bg-red-950/60 hover:bg-red-800'}`}
                title={cur === 'east' ? 'Buy machinery from the East (₽)' : 'Buy machinery from the West ($)'}
              >
                {cur === 'east' ? '₽ East' : '$ West'}
              </button>
            ))}
            {led.repairImports !== 0 && (
              <span className="ml-auto font-bold" title="Machinery imported for repairs yesterday">{money(led.repairImports, engine.repairImportCurrency === 'east' ? '₽' : '$')}</span>
            )}
          </div>
        )}
      </section>

      {(offers.length > 0 || active.length > 0 || closed.length > 0 || engine.relationsPenalty.east > 0.001 || engine.relationsPenalty.west > 0.001) && (
        <section className="space-y-1">
          <div className="text-[0.625rem] font-black uppercase tracking-wider text-yellow-400 flex items-center gap-1">
            <GameIcon name="contract" size={11} /> Contracts
          </div>
          {(['east', 'west'] as const).map(bloc => engine.relationsPenalty[bloc] > 0.001 && (
            <div key={bloc} className="text-[0.625rem] text-red-300">
              Relations soured with the {bloc === 'east' ? 'East' : 'West'} — prices {fmtPct(engine.relationsPenalty[bloc])}% worse until it blows over.
            </div>
          ))}
          {[...offers, ...active].map(c => <ContractCard key={c.id} engine={engine} c={c} />)}
          {closed.map(c => (
            <div key={c.id} className={`text-[0.625rem] flex justify-between px-1 ${c.state === 'done' ? 'text-yellow-200/40' : 'text-red-300/60'}`}>
              <span>{c.amount} {RESOURCES[c.r].name} → {c.bloc === 'east' ? 'East' : 'West'}</span>
              <span>{c.state === 'done' ? 'fulfilled' : 'failed'}</span>
            </div>
          ))}
        </section>
      )}

      <div className="text-[0.625rem] text-yellow-200/60 leading-tight">
        Sell surplus to the <b>East (₽)</b> or the <b>West ($)</b>. Goods must be road-connected to the border Customs House. Imports arrive at customs storage and are hauled away by truck.
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
                  <span className="font-bold flex items-center gap-1"><GameIcon name={def.icon} size={12} /> {def.name}</span>
                  <span className="text-yellow-200/70" title="sellable = customs-connected stock minus what shops and industry keep for themselves">
                    sellable {fmtQty(sellable)} / {fmtQty(total)}
                  </span>
                </div>
                <div className="flex items-center gap-1 mt-1">
                  <button onClick={() => doTrade(() => engine.sell(r, amount, 'east'))} disabled={!canSell} data-sfx="none"
                    className="flex-1 rounded bg-red-800 hover:bg-red-700 disabled:opacity-30 text-[0.625rem] font-bold py-0.5" title={`Sell to East at ₽${engine.priceOf(r, 'east').toFixed(1)}`}>
                    +₽{engine.priceOf(r, 'east').toFixed(1)}
                  </button>
                  <button onClick={() => doTrade(() => engine.sell(r, amount, 'west'))} disabled={!canSell} data-sfx="none"
                    className="flex-1 rounded bg-green-900 hover:bg-green-800 disabled:opacity-30 text-[0.625rem] font-bold py-0.5" title={`Sell to West at $${engine.priceOf(r, 'west').toFixed(1)}`}>
                    +${engine.priceOf(r, 'west').toFixed(1)}
                  </button>
                  <button onClick={() => doTrade(() => engine.buy(r, amount, 'east'))} disabled={!canBuy('east')} data-sfx="none"
                    className="flex-1 rounded bg-red-950 hover:bg-red-800 disabled:opacity-30 text-[0.625rem] font-bold py-0.5 border border-yellow-600/30" title="Import from East">
                    −₽{engine.importPriceOf(r, 'east').toFixed(1)}
                  </button>
                  <button onClick={() => doTrade(() => engine.buy(r, amount, 'west'))} disabled={!canBuy('west')} data-sfx="none"
                    className="flex-1 rounded bg-red-950 hover:bg-red-800 disabled:opacity-30 text-[0.625rem] font-bold py-0.5 border border-green-600/30" title="Import from West">
                    −${engine.importPriceOf(r, 'west').toFixed(1)}
                  </button>
                </div>
                {(() => {
                  const rule = engine.autoTrade.rules[r];
                  return (
                    <div className={`flex items-center gap-1 mt-1 text-[0.625rem] ${engine.autoTrade.enabled ? '' : 'opacity-50'}`}>
                      <span className="text-yellow-200/50 mr-0.5">auto</span>
                      {(['import', 'export'] as const).map(m => (
                        <button
                          key={m}
                          onClick={() => engine.setAutoTradeRule(r, rule?.mode === m ? null : { mode: m, level: rule?.level ?? 20, currency: rule?.currency ?? 'east' })}
                          className={`px-1.5 py-0.5 rounded font-bold ${rule?.mode === m ? 'bg-yellow-500 text-red-950' : 'bg-red-950/60 hover:bg-red-800'}`}
                          title={m === 'import'
                            ? 'Auto-import: keep the town stocked to the level'
                            : 'Auto-export: trucks stage everything above the level to customs, which sells it'}
                        >
                          {m === 'import' ? 'Imp' : 'Exp'}
                        </button>
                      ))}
                      {rule && (
                        <>
                          <span className="text-yellow-200/50">{rule.mode === 'import' ? 'keep ≥' : 'above'}</span>
                          <NumInput value={rule.level} onValue={v => engine.setAutoTradeRule(r, { ...rule, level: v })} step={5} w="w-12" />
                          <button
                            onClick={() => engine.setAutoTradeRule(r, { ...rule, currency: rule.currency === 'east' ? 'west' : 'east' })}
                            className={`px-1.5 py-0.5 rounded font-bold ${rule.currency === 'east' ? 'bg-red-800' : 'bg-green-900 text-green-100'}`}
                            title="Trade bloc: East trades in rubles, West in dollars"
                          >
                            {rule.currency === 'east' ? '₽' : '$'}
                          </button>
                        </>
                      )}
                    </div>
                  );
                })()}
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
      <div className="text-[0.625rem] text-yellow-200/60">Directives from the Planning Committee. Complete them for rewards.</div>
      {OBJECTIVES.map(o => {
        const done = engine.objectivesDone.includes(o.id);
        const isCurrent = current?.id === o.id;
        return (
          <div key={o.id} className={`rounded px-2 py-1.5 border ${done ? 'border-green-700/50 bg-green-900/20' : isCurrent ? 'border-yellow-500/70 bg-yellow-500/10' : 'border-yellow-600/20 bg-red-900/30'}`}>
            <div className="flex items-center gap-1.5 text-xs font-bold">
              <GameIcon
                name={done ? 'check' : isCurrent ? 'star' : 'square'}
                size={13}
                className={done ? 'text-green-400' : isCurrent ? 'text-yellow-400' : 'text-yellow-200/40'}
              />
              <span className={done ? 'line-through text-yellow-200/50' : ''}>{o.title}</span>
            </div>
            <div className="text-[0.625rem] text-yellow-200/60 ml-5">{o.description}</div>
            <div className="text-[0.625rem] text-green-300/80 ml-5">
              {o.rewardRubles ? `+₽${o.rewardRubles.toLocaleString()} ` : ''}{o.rewardDollars ? `+$${o.rewardDollars.toLocaleString()}` : ''}
            </div>
          </div>
        );
      })}
    </div>
  );
}
