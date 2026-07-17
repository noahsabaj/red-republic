import { useState } from 'react';
import type { GameEngine, BuildingInst } from '@/game/engine';
import { BUILDINGS, RESOURCES, ALL_RESOURCES, OBJECTIVES, FARM_SEASON } from '@/game/config';
import type { ResourceId } from '@/game/config';

interface Props {
  engine: GameEngine;
  mode: 'building' | 'trade' | 'objectives';
  selectedId: number | null;
  onClose: () => void;
  onOpenTrade: () => void;
  notify: (msg: string, kind: 'good' | 'bad' | 'info') => void;
}

export default function SidePanel({ engine, mode, selectedId, onClose, onOpenTrade, notify }: Props) {
  return (
    <div className="absolute right-0 top-24 bottom-0 z-10 flex pointer-events-none">
      <div className="pointer-events-auto flex flex-col w-72 m-2 rounded-lg border-2 border-yellow-600/60 bg-red-950/95 text-yellow-50 shadow-2xl overflow-hidden">
        <div className="flex items-center justify-between px-3 py-2 bg-red-900/60 border-b border-yellow-600/30">
          <span className="text-xs font-black uppercase tracking-widest text-yellow-400">
            {mode === 'building' ? 'Building' : mode === 'trade' ? '🛃 Foreign Trade' : '🎯 Five-Year Plan'}
          </span>
          <button onClick={onClose} className="text-yellow-200/60 hover:text-yellow-100 font-bold">✕</button>
        </div>
        <div className="flex-1 overflow-y-auto soviet-scroll p-3">
          {mode === 'building' && <BuildingInfo engine={engine} id={selectedId} onOpenTrade={onOpenTrade} />}
          {mode === 'trade' && <TradePanel engine={engine} notify={notify} />}
          {mode === 'objectives' && <ObjectivesPanel engine={engine} />}
        </div>
      </div>
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
            const have = (b.stock[r as ResourceId] ?? 0) + (b.incoming[r as ResourceId] ?? 0);
            return (
              <Row key={r} label={`${RESOURCES[r as ResourceId].icon} ${RESOURCES[r as ResourceId].name}`} value={`${Math.floor(have)} / ${amt}`} ok={have >= (amt as number)} />
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

          {(def.inputs || def.outputs) && (
            <div className="rounded bg-red-900/40 p-2">
              <div className="text-[10px] font-black uppercase tracking-wider text-yellow-400 mb-1">Production / day</div>
              <div className="text-xs">
                {def.inputs && Object.entries(def.inputs).map(([r, a]) => `${RESOURCES[r as ResourceId].icon}${(a as number) * b.eff < 10 ? ((a as number) * b.eff).toFixed(1) : Math.round((a as number) * b.eff)}`).join(' + ')}
                {def.inputs && def.outputs && ' → '}
                {def.outputs && Object.entries(def.outputs).map(([r, a]) => `${RESOURCES[r as ResourceId].icon}${((a as number) * b.eff).toFixed(1)}`).join(' + ')}
              </div>
            </div>
          )}

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
              onClick={() => { b.priorityHigh = !b.priorityHigh; engine.setSpeed(engine.speed); }}
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
        {ALL_RESOURCES.map(r => {
          const def = RESOURCES[r];
          const stock = engine.totals[r];
          return (
            <div key={r} className="rounded bg-red-900/40 px-2 py-1">
              <div className="flex items-center justify-between text-xs">
                <span className="font-bold">{def.icon} {def.name}</span>
                <span className="text-yellow-200/70">stock {Math.floor(stock)}</span>
              </div>
              <div className="flex items-center gap-1 mt-1">
                <button onClick={() => doTrade(() => engine.sell(r, amount, 'east'))} disabled={stock < 1}
                  className="flex-1 rounded bg-red-800 hover:bg-red-700 disabled:opacity-30 text-[10px] font-bold py-0.5" title={`Sell to East at ₽${engine.priceOf(r, 'east').toFixed(1)}`}>
                  ➜₽{engine.priceOf(r, 'east').toFixed(1)}
                </button>
                <button onClick={() => doTrade(() => engine.sell(r, amount, 'west'))} disabled={stock < 1}
                  className="flex-1 rounded bg-green-900 hover:bg-green-800 disabled:opacity-30 text-[10px] font-bold py-0.5" title={`Sell to West at $${engine.priceOf(r, 'west').toFixed(1)}`}>
                  ➜${engine.priceOf(r, 'west').toFixed(1)}
                </button>
                <button onClick={() => doTrade(() => engine.buy(r, amount, 'east'))}
                  className="flex-1 rounded bg-red-950 hover:bg-red-800 text-[10px] font-bold py-0.5 border border-yellow-600/30" title={`Import from East`}>
                  ₽{(engine.priceOf(r, 'east') * 1.6).toFixed(1)}
                </button>
                <button onClick={() => doTrade(() => engine.buy(r, amount, 'west'))}
                  className="flex-1 rounded bg-red-950 hover:bg-red-800 text-[10px] font-bold py-0.5 border border-green-600/30" title={`Import from West`}>
                  ${(engine.priceOf(r, 'west') * 1.6).toFixed(1)}
                </button>
              </div>
            </div>
          );
        })}
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
