import type { GameEngine } from '@/game/engine';
import { BALANCE, WEATHER } from '@/game/config';
import { fmtMoney } from '@/game/format';
import { useEngineSignature } from '@/hooks/use-engine';
import { GameIcon } from '@/ui/GameIcon';

export type PanelMode = 'building' | 'trade' | 'objectives' | 'stockpiles' | 'music' | 'logistics';

interface Props {
  engine: GameEngine;
  activePanel: PanelMode | null;
  helpOpen: boolean;
  onOpenStockpiles: () => void;
  onOpenLogistics: () => void;
  onOpenObjectives: () => void;
  onOpenTrade: () => void;
  onOpenMusic: () => void;
  onOpenHelp: () => void;
  onOpenMenu: () => void;
}

function HappinessCard({ engine }: { engine: GameEngine }) {
  const breakdown = engine.happinessBreakdown();
  return (
    <div className="pointer-events-none absolute left-1/2 top-full z-50 mt-2 w-72 -translate-x-1/2 rounded-lg border border-yellow-600/60 bg-red-950/95 p-3 text-yellow-50 shadow-2xl backdrop-blur-md">
      <div className="flex items-center justify-between border-b border-yellow-600/30 pb-1.5">
        <div className="flex items-center gap-1.5 font-black uppercase text-xs tracking-wider text-yellow-300">
          <GameIcon name="happy" size={14} /> Happiness Breakdown
        </div>
        <div className={`font-black text-sm tabular-nums ${breakdown.overall < 40 ? 'text-red-400' : breakdown.overall > 65 ? 'text-green-400' : 'text-yellow-300'}`}>
          {breakdown.overall}%
        </div>
      </div>

      <div className="mt-2 space-y-1.5 text-[0.6875rem]">
        {breakdown.factors.map(f => (
          <div key={f.id} className="flex flex-col gap-0.5">
            <div className="flex items-center justify-between text-yellow-100/90 font-medium">
              <span className="inline-flex items-center gap-1">
                <GameIcon name={f.icon} size={11} className="text-yellow-300/80" />
                {f.label}
                <span className="text-[0.5625rem] text-yellow-200/40">({f.weightPct}% weight)</span>
              </span>
              <span className={`font-bold tabular-nums ${f.satPct < 40 ? 'text-red-300' : f.satPct > 70 ? 'text-green-300' : 'text-yellow-200'}`}>
                {f.satPct}%
              </span>
            </div>
            <div className="h-1.5 w-full rounded-full bg-red-900/60 overflow-hidden border border-yellow-600/20">
              <div
                className={`h-full transition-all duration-300 ${
                  f.satPct < 40 ? 'bg-red-500' : f.satPct > 70 ? 'bg-emerald-400' : 'bg-amber-400'
                }`}
                style={{ width: `${Math.min(100, Math.max(0, f.satPct))}%` }}
              />
            </div>
          </div>
        ))}
      </div>

      {(breakdown.modifiers.pollutionPenaltyPct > 0 || breakdown.modifiers.weatherMoralePct !== 0) && (
        <div className="mt-2.5 border-t border-yellow-600/20 pt-2 space-y-1 text-[0.625rem] text-yellow-200/80">
          <div className="font-bold uppercase tracking-wider text-[0.5625rem] text-yellow-400/80">Active Modifiers</div>
          {breakdown.modifiers.pollutionPenaltyPct > 0 && (
            <div className="flex items-center justify-between text-red-300">
              <span className="inline-flex items-center gap-1"><GameIcon name="smoke" size={10} /> Industrial Pollution</span>
              <span className="font-bold tabular-nums">−{breakdown.modifiers.pollutionPenaltyPct}%</span>
            </div>
          )}
          {breakdown.modifiers.weatherMoralePct !== 0 && (
            <div className={`flex items-center justify-between ${breakdown.modifiers.weatherMoralePct > 0 ? 'text-green-300' : 'text-amber-300'}`}>
              <span className="inline-flex items-center gap-1">
                <GameIcon name={breakdown.modifiers.weatherMoralePct > 0 ? 'sun' : 'cloud'} size={10} /> Weather Morale
              </span>
              <span className="font-bold tabular-nums">{breakdown.modifiers.weatherMoralePct > 0 ? `+${breakdown.modifiers.weatherMoralePct}%` : `${breakdown.modifiers.weatherMoralePct}%`}</span>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export default function HUD({ engine, activePanel, helpOpen, onOpenStockpiles, onOpenLogistics, onOpenObjectives, onOpenTrade, onOpenMusic, onOpenHelp, onOpenMenu }: Props) {
  // re-render only when something the HUD actually displays changes
  useEngineSignature(engine, (e) => [
    e.day, e.month, e.year, e.speed,
    Math.floor(e.rubles), Math.floor(e.dollars),
    e.pop, e.capacity, e.workers, e.employed,
    Math.round(e.happiness),
    e.sat.food, e.sat.clothes, e.sat.power, e.sat.heat, e.sat.employment, e.sat.culture, e.sat.health, e.sat.pollution, e.happinessBreakdown().modifiers.weatherMoralePct,
    e.powerProduced.toFixed(1), e.powerDemand.toFixed(1),
    e.heatingRequired(), e.heatProduced.toFixed(1), e.heatDemand.toFixed(1),
    e.weather.condition, Math.round(e.weather.tempC), e.weather.riverFrozen,
    e.forecast().map(d => d.condition + Math.round(d.tempC)).join('|'),
    e.alerts.map(a => a.id + a.text).join('|'),
    (() => { const f = e.fleetStatus(); return `${f.active}/${f.max}/${f.driverTrucks}/${Math.round(f.gasFuel)}`; })(),
    e.autoTrade.enabled,
    Math.round(e.tradeLedger.yesterday.rubles), Math.round(e.tradeLedger.yesterday.dollars),
    Math.round(e.tradeLedger.yesterday.foreignLaborRubles ?? e.tradeLedger.yesterday.foreignLabor),
    Math.round(e.tradeLedger.yesterday.foreignLaborDollars),
    e.foreignLaborCurrency,
    e.contracts.filter(c => c.state === 'offer').length,
  ]);
  const offersPending = engine.contracts.filter(c => c.state === 'offer').length;
  const tradeNote = (v: number, sym: string) =>
    Math.round(v) !== 0 ? ` · auto-trade yesterday ${v > 0 ? '+' : '−'}${sym}${fmtMoney(Math.abs(v))}` : '';
  const laborNote = (v: number, sym: string) =>
    Math.round(v) !== 0 ? ` · foreign labor −${sym}${fmtMoney(Math.abs(v))}/day` : '';

  const season = engine.season();
  const speedBtn = (s: 0 | 1 | 2 | 4, label: React.ReactNode, name: string) => (
    <button
      key={name}
      onClick={() => engine.setSpeed(s)}
      data-sfx="speed"
      aria-label={name}
      aria-pressed={engine.speed === s}
      className={`px-2 py-0.5 rounded text-xs font-bold ${engine.speed === s ? 'bg-yellow-500 text-red-950' : 'bg-red-950/60 text-yellow-100/70 hover:bg-red-900'}`}
    >
      {label}
    </button>
  );

  const panelBtn = (label: string, icon: string, active: boolean, onClick: () => void, dot = false) => (
    <button
      onClick={onClick}
      data-sfx="panel"
      aria-label={label}
      aria-pressed={active}
      title={label}
      className={`relative rounded px-2 py-1 text-xs font-bold ${active ? 'bg-yellow-500 text-red-950' : 'bg-red-950/60 hover:bg-red-800'}`}
    >
      <GameIcon name={icon} size={14} />
      {dot && <span className="absolute -top-0.5 -right-0.5 h-2 w-2 rounded-full bg-yellow-400 border border-red-950" />}
    </button>
  );

  return (
    <div className="pointer-events-none absolute inset-x-0 top-0 z-20 flex flex-col">
      <div className="pointer-events-auto flex items-center gap-3 bg-gradient-to-b from-red-950 to-red-900/95 px-3 py-1.5 text-yellow-50 shadow-lg border-b-2 border-yellow-600/60">
        <div className="flex items-center gap-1.5 shrink-0" title={engine.name}>
          <span className="text-yellow-400 text-lg">★</span>
          <span className="font-black tracking-widest text-sm uppercase hidden md:inline max-w-44 truncate">{engine.name}</span>
        </div>

        <div className="flex items-center gap-1 bg-red-950/60 rounded px-2 py-0.5 text-xs">
          <GameIcon name={season} size={12} className="text-yellow-300" />
          <span className="font-bold">{BALANCE.months[engine.month - 1]} {engine.year}</span>
          <span className="text-yellow-200/60">day {engine.day}</span>
        </div>

        <div
          className="flex items-center gap-1 bg-red-950/60 rounded px-2 py-0.5 text-xs"
          title={`${WEATHER[engine.weather.condition].label}, ${Math.round(engine.weather.tempC)} °C${engine.weather.riverFrozen ? ' · river frozen' : ''}`}
        >
          <GameIcon name={WEATHER[engine.weather.condition].icon} size={12} className="text-yellow-300" />
          <span className="font-bold">{Math.round(engine.weather.tempC)}°C</span>
          {engine.weather.riverFrozen && <GameIcon name="freeze" size={11} className="text-sky-300" />}
        </div>

        <div
          className="hidden lg:flex items-center gap-1.5 bg-red-950/40 rounded px-2 py-0.5"
          title="State Hydrometeorological Service — 5-day forecast"
        >
          {engine.forecast().map((d, i) => (
            <span key={i} className="flex flex-col items-center leading-none gap-0.5">
              <GameIcon name={WEATHER[d.condition].icon} size={10} className="text-yellow-200/80" />
              <span className="text-[0.5625rem] text-yellow-200/60">{Math.round(d.tempC)}°</span>
            </span>
          ))}
        </div>

        <div className="flex items-center gap-1">
          {speedBtn(0, <GameIcon name="pause" size={12} />, 'Pause')}
          {speedBtn(1, '1×', 'Normal speed')}
          {speedBtn(2, '2×', 'Double speed')}
          {speedBtn(4, '4×', 'Quadruple speed')}
        </div>

        <div className="h-5 w-px bg-yellow-600/40" />

        <div className="flex items-center gap-3 text-xs font-bold">
          <span title={`Rubles — foreign currency earned from trade with the East; buys imports and machinery${tradeNote(engine.tradeLedger.yesterday.rubles, '₽')}${laborNote(engine.tradeLedger.yesterday.foreignLaborRubles ?? engine.tradeLedger.yesterday.foreignLabor, '₽')}`}>₽ {fmtMoney(engine.rubles)}</span>
          <span title={`Dollars — hard currency from the West${tradeNote(engine.tradeLedger.yesterday.dollars, '$')}${laborNote(engine.tradeLedger.yesterday.foreignLaborDollars, '$')}`} className="text-green-300">$ {fmtMoney(engine.dollars)}</span>
          <span title={`Citizens: ${engine.pop} / housing ${engine.capacity} · workers ${engine.workers} · employed ${engine.employed}`} className="flex items-center gap-1">
            <GameIcon name="users" size={12} /> {engine.pop}<span className="text-yellow-200/50">/{engine.capacity}</span>
          </span>
          <div className="group relative flex items-center">
            <span className={`flex items-center gap-1 cursor-help ${engine.happiness < 40 ? 'text-red-300' : engine.happiness > 65 ? 'text-green-300' : ''}`}>
              <GameIcon name="happy" size={12} /> {Math.round(engine.happiness)}%
            </span>
            <div className="hidden group-hover:block">
              <HappinessCard engine={engine} />
            </div>
          </div>
          <span title={`Power: ${engine.powerProduced.toFixed(1)} / ${engine.powerDemand.toFixed(1)} MW`} className={`flex items-center gap-1 ${engine.powerDemand > engine.powerProduced + 0.01 ? 'text-red-300' : ''}`}>
            <GameIcon name="power" size={12} /> {engine.powerProduced.toFixed(0)}/{engine.powerDemand.toFixed(0)}
          </span>
          {engine.heatingRequired() && (
            <span title={`Heat: ${engine.heatProduced.toFixed(1)} / ${engine.heatDemand.toFixed(1)}`} className={`flex items-center gap-1 ${engine.heatDemand > engine.heatProduced + 0.01 ? 'text-red-300' : ''}`}>
              <GameIcon name="heat" size={12} /> {engine.heatProduced.toFixed(0)}/{engine.heatDemand.toFixed(0)}
            </span>
          )}
          {(() => { const f = engine.fleetStatus(); return (
            <span title={`Trucks in use ${f.active}/${f.max} — offices ${f.officeTrucks} + depots ${f.depotTrucks}${f.driverTrucks > f.depotTrucks ? ` (${f.driverTrucks - f.depotTrucks} idle: low fuel)` : ''}`} className={`flex items-center gap-1 ${f.max > 0 && f.active >= f.max ? 'text-red-300' : ''}`}>
              <GameIcon name="truck" size={12} /> {f.active}/{f.max}
            </span>
          ); })()}
        </div>

        <div className="ml-auto flex items-center gap-1.5">
          {panelBtn('Stockpiles', 'stockpiles', activePanel === 'stockpiles', onOpenStockpiles)}
          {panelBtn('Logistics & Fleet', 'truck', activePanel === 'logistics', onOpenLogistics)}
          {panelBtn('Five-Year Plan', 'plan', activePanel === 'objectives', onOpenObjectives)}
          {panelBtn(
            `Foreign Trade${offersPending ? ` — ${offersPending} contract offer${offersPending > 1 ? 's' : ''} pending` : ''}${engine.autoTrade.enabled ? ' — auto-trade ON' : ''}`,
            'trade', activePanel === 'trade', onOpenTrade,
            engine.autoTrade.enabled || offersPending > 0,
          )}
          {panelBtn('State Radio (music)', 'music', activePanel === 'music', onOpenMusic)}
          {panelBtn('Help', 'help', helpOpen, onOpenHelp)}
          {panelBtn('Menu (Esc)', 'menu', false, onOpenMenu)}
        </div>
      </div>

      {engine.alerts.length > 0 && (
        // the strip itself stays click-through so the gaps between chips
        // don't block panning/zooming the canvas underneath
        <div className="pointer-events-none flex flex-wrap gap-1.5 px-3 py-1.5">
          {engine.alerts.map(a => (
            <span key={a.id} className={`pointer-events-auto flex items-center gap-1 rounded px-2 py-0.5 text-[0.6875rem] font-bold shadow ${a.level === 'bad' ? 'bg-red-600/90 text-white' : 'bg-amber-500/90 text-amber-950'}`}>
              <GameIcon name={a.icon} size={11} /> {a.text}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
