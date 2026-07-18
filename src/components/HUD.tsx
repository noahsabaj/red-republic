import type { GameEngine } from '@/game/engine';
import { BALANCE } from '@/game/config';
import { useEngineSignature } from '@/hooks/use-engine';
import { GameIcon } from '@/ui/GameIcon';

export type PanelMode = 'building' | 'trade' | 'objectives' | 'stockpiles';

interface Props {
  engine: GameEngine;
  activePanel: PanelMode | null;
  helpOpen: boolean;
  onOpenStockpiles: () => void;
  onOpenObjectives: () => void;
  onOpenTrade: () => void;
  onOpenHelp: () => void;
}

export default function HUD({ engine, activePanel, helpOpen, onOpenStockpiles, onOpenObjectives, onOpenTrade, onOpenHelp }: Props) {
  // re-render only when something the HUD actually displays changes
  useEngineSignature(engine, (e) => [
    e.day, e.month, e.year, e.speed,
    Math.floor(e.rubles), Math.floor(e.dollars), e.wagesUnpaid,
    e.pop, e.capacity, e.workers, e.employed,
    Math.round(e.happiness),
    e.powerProduced.toFixed(1), e.powerDemand.toFixed(1),
    e.isHeatingSeason(), e.heatProduced.toFixed(1), e.heatDemand.toFixed(1),
    e.alerts.map(a => a.id + a.text).join('|'),
  ]);

  const season = engine.season();
  const speedBtn = (s: 0 | 1 | 2 | 4, label: React.ReactNode, name: string) => (
    <button
      key={name}
      onClick={() => engine.setSpeed(s)}
      aria-label={name}
      aria-pressed={engine.speed === s}
      className={`px-2 py-0.5 rounded text-xs font-bold ${engine.speed === s ? 'bg-yellow-500 text-red-950' : 'bg-red-950/60 text-yellow-100/70 hover:bg-red-900'}`}
    >
      {label}
    </button>
  );

  const panelBtn = (label: string, icon: string, active: boolean, onClick: () => void) => (
    <button
      onClick={onClick}
      aria-label={label}
      aria-pressed={active}
      title={label}
      className={`rounded px-2 py-1 text-xs font-bold ${active ? 'bg-yellow-500 text-red-950' : 'bg-red-950/60 hover:bg-red-800'}`}
    >
      <GameIcon name={icon} size={14} />
    </button>
  );

  return (
    <div className="pointer-events-none absolute inset-x-0 top-0 z-20 flex flex-col">
      <div className="pointer-events-auto flex items-center gap-3 bg-gradient-to-b from-red-950 to-red-900/95 px-3 py-1.5 text-yellow-50 shadow-lg border-b-2 border-yellow-600/60">
        <div className="flex items-center gap-1.5 shrink-0">
          <span className="text-yellow-400 text-lg">★</span>
          <span className="font-black tracking-widest text-sm uppercase hidden md:inline">Red Republic</span>
        </div>

        <div className="flex items-center gap-1 bg-red-950/60 rounded px-2 py-0.5 text-xs">
          <GameIcon name={season} size={12} className="text-yellow-300" />
          <span className="font-bold">{BALANCE.months[engine.month - 1]} {engine.year}</span>
          <span className="text-yellow-200/60">day {engine.day}</span>
        </div>

        <div className="flex items-center gap-1">
          {speedBtn(0, <GameIcon name="pause" size={12} />, 'Pause')}
          {speedBtn(1, '1×', 'Normal speed')}
          {speedBtn(2, '2×', 'Double speed')}
          {speedBtn(4, '4×', 'Quadruple speed')}
        </div>

        <div className="h-5 w-px bg-yellow-600/40" />

        <div className="flex items-center gap-3 text-xs font-bold">
          <span title="Rubles — earned from trade with the East; pays wages and construction" className={engine.wagesUnpaid ? 'text-red-300' : ''}>₽ {Math.floor(engine.rubles).toLocaleString()}</span>
          <span title="Dollars — hard currency from the West" className="text-green-300">$ {Math.floor(engine.dollars).toLocaleString()}</span>
          <span title={`Citizens: ${engine.pop} / housing ${engine.capacity} · workers ${engine.workers} · employed ${engine.employed}`} className="flex items-center gap-1">
            <GameIcon name="users" size={12} /> {engine.pop}<span className="text-yellow-200/50">/{engine.capacity}</span>
          </span>
          <span title="Happiness" className={`flex items-center gap-1 ${engine.happiness < 40 ? 'text-red-300' : engine.happiness > 65 ? 'text-green-300' : ''}`}>
            <GameIcon name="happy" size={12} /> {Math.round(engine.happiness)}%
          </span>
          <span title={`Power: ${engine.powerProduced.toFixed(1)} / ${engine.powerDemand.toFixed(1)} MW`} className={`flex items-center gap-1 ${engine.powerDemand > engine.powerProduced + 0.01 ? 'text-red-300' : ''}`}>
            <GameIcon name="power" size={12} /> {engine.powerProduced.toFixed(0)}/{engine.powerDemand.toFixed(0)}
          </span>
          {engine.isHeatingSeason() && (
            <span title={`Heat: ${engine.heatProduced.toFixed(1)} / ${engine.heatDemand.toFixed(1)}`} className={`flex items-center gap-1 ${engine.heatDemand > engine.heatProduced + 0.01 ? 'text-red-300' : ''}`}>
              <GameIcon name="heat" size={12} /> {engine.heatProduced.toFixed(0)}/{engine.heatDemand.toFixed(0)}
            </span>
          )}
        </div>

        <div className="ml-auto flex items-center gap-1.5">
          {panelBtn('Stockpiles', 'stockpiles', activePanel === 'stockpiles', onOpenStockpiles)}
          {panelBtn('Five-Year Plan', 'plan', activePanel === 'objectives', onOpenObjectives)}
          {panelBtn('Foreign Trade', 'trade', activePanel === 'trade', onOpenTrade)}
          {panelBtn('Help', 'help', helpOpen, onOpenHelp)}
        </div>
      </div>

      {engine.alerts.length > 0 && (
        // the strip itself stays click-through so the gaps between chips
        // don't block panning/zooming the canvas underneath
        <div className="pointer-events-none flex flex-wrap gap-1.5 px-3 py-1.5">
          {engine.alerts.map(a => (
            <span key={a.id} className={`pointer-events-auto flex items-center gap-1 rounded px-2 py-0.5 text-[11px] font-bold shadow ${a.level === 'bad' ? 'bg-red-600/90 text-white' : 'bg-amber-500/90 text-amber-950'}`}>
              <GameIcon name={a.icon} size={11} /> {a.text}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
