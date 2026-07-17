import { useState } from 'react';
import type { GameEngine } from '@/game/engine';
import { ALL_RESOURCES, RESOURCES, BALANCE } from '@/game/config';

interface Props {
  engine: GameEngine;
  onOpenObjectives: () => void;
  onOpenTrade: () => void;
  onOpenHelp: () => void;
}

const SEASON_ICON: Record<string, string> = { winter: '❄️', spring: '🌱', summer: '☀️', autumn: '🍂' };

export default function HUD({ engine, onOpenObjectives, onOpenTrade, onOpenHelp }: Props) {
  const [resOpen, setResOpen] = useState(false);
  const season = engine.season();
  const speedBtn = (s: 0 | 1 | 2 | 4, label: string) => (
    <button
      key={label}
      onClick={() => engine.setSpeed(s)}
      className={`px-2 py-0.5 rounded text-xs font-bold ${engine.speed === s ? 'bg-yellow-500 text-red-950' : 'bg-red-950/60 text-yellow-100/70 hover:bg-red-900'}`}
    >
      {label}
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
          <span>{SEASON_ICON[season]}</span>
          <span className="font-bold">{BALANCE.months[engine.month - 1]} {engine.year}</span>
          <span className="text-yellow-200/60">day {engine.day}</span>
        </div>

        <div className="flex items-center gap-1">
          {speedBtn(0, '⏸')}
          {speedBtn(1, '▶')}
          {speedBtn(2, '▶▶')}
          {speedBtn(4, '▶▶▶')}
        </div>

        <div className="h-5 w-px bg-yellow-600/40" />

        <div className="flex items-center gap-3 text-xs font-bold">
          <span title="Rubles — earned from wages of trade with the East" className={engine.rubles < 0 ? 'text-red-300' : ''}>₽ {Math.floor(engine.rubles).toLocaleString()}</span>
          <span title="Dollars — hard currency from the West" className="text-green-300">$ {Math.floor(engine.dollars).toLocaleString()}</span>
          <span title={`Citizens: ${engine.pop} / housing ${engine.capacity} · workers ${engine.workers} · employed ${engine.employed}`}>
            👥 {engine.pop}<span className="text-yellow-200/50">/{engine.capacity}</span>
          </span>
          <span title="Happiness" className={engine.happiness < 40 ? 'text-red-300' : engine.happiness > 65 ? 'text-green-300' : ''}>
            😊 {Math.round(engine.happiness)}%
          </span>
          <span title={`Power: ${engine.powerProduced.toFixed(1)} / ${engine.powerDemand.toFixed(1)} MW`} className={engine.powerDemand > engine.powerProduced ? 'text-red-300' : ''}>
            ⚡ {engine.powerProduced.toFixed(0)}/{engine.powerDemand.toFixed(0)}
          </span>
          {engine.isHeatingSeason() && (
            <span title={`Heat: ${engine.heatProduced.toFixed(1)} / ${engine.heatDemand.toFixed(1)}`} className={engine.heatDemand > engine.heatProduced ? 'text-red-300' : ''}>
              🔥 {engine.heatProduced.toFixed(0)}/{engine.heatDemand.toFixed(0)}
            </span>
          )}
        </div>

        <div className="ml-auto flex items-center gap-1.5 relative">
          <button onClick={() => setResOpen(!resOpen)} className="bg-red-950/60 hover:bg-red-800 rounded px-2 py-1 text-xs font-bold" title="Stockpiles">📦</button>
          <button onClick={onOpenObjectives} className="bg-red-950/60 hover:bg-red-800 rounded px-2 py-1 text-xs font-bold" title="Five-Year Plan">🎯</button>
          <button onClick={onOpenTrade} className="bg-red-950/60 hover:bg-red-800 rounded px-2 py-1 text-xs font-bold" title="Foreign Trade">🛃</button>
          <button onClick={onOpenHelp} className="bg-red-950/60 hover:bg-red-800 rounded px-2 py-1 text-xs font-bold" title="Help">❓</button>

          {resOpen && (
            <div className="absolute right-0 top-9 w-64 rounded-lg border-2 border-yellow-600/60 bg-red-950/97 p-3 shadow-2xl">
              <div className="text-xs font-black uppercase tracking-wider text-yellow-400 mb-2">National Stockpiles</div>
              <div className="grid grid-cols-2 gap-x-3 gap-y-1">
                {ALL_RESOURCES.map(r => (
                  <div key={r} className="flex items-center justify-between text-xs">
                    <span>{RESOURCES[r].icon} {RESOURCES[r].name}</span>
                    <span className="font-bold">{Math.floor(engine.totals[r])}</span>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>

      {engine.alerts.length > 0 && (
        <div className="pointer-events-auto flex flex-wrap gap-1.5 px-3 py-1.5">
          {engine.alerts.map(a => (
            <span key={a.id} className={`rounded px-2 py-0.5 text-[11px] font-bold shadow ${a.level === 'bad' ? 'bg-red-600/90 text-white' : 'bg-amber-500/90 text-amber-950'}`}>
              {a.icon} {a.text}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
