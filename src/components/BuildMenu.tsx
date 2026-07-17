import { useState } from 'react';
import { BUILDINGS, BUILD_LIST, CATEGORY_NAMES, RESOURCES, type Category, type ResourceId } from '@/game/config';
import type { GameEngine } from '@/game/engine';
import { useEngineSignature } from '@/hooks/use-engine';
import type { Tool } from './GameCanvas';

interface Props {
  engine: GameEngine;
  tool: Tool;
  setTool: (t: Tool) => void;
  instantBuild: boolean;
  setInstantBuild: (v: boolean) => void;
}

const CATS: { id: Category; icon: string }[] = [
  { id: 'infra', icon: '🛣️' },
  { id: 'housing', icon: '🏠' },
  { id: 'industry', icon: '🏭' },
  { id: 'services', icon: '🏪' },
  { id: 'trade', icon: '📦' },
];

export default function BuildMenu({ engine, tool, setTool, instantBuild, setInstantBuild }: Props) {
  const [cat, setCat] = useState<Category>('infra');

  // only affordability flags matter here — re-render when one flips
  useEngineSignature(engine, (e) => [
    BUILD_LIST.map(id => (instantBuild ? e.dollars >= e.instantCost(id) : e.rubles >= BUILDINGS[id].costRubles) ? 1 : 0).join(''),
  ]);

  const items = BUILD_LIST.filter(id => BUILDINGS[id].category === cat);

  return (
    <div className="absolute left-0 top-24 bottom-0 z-10 flex pointer-events-none">
      <div className="pointer-events-auto flex flex-col w-56 m-2 rounded-lg border-2 border-yellow-600/60 bg-red-950/95 text-yellow-50 shadow-2xl overflow-hidden">
        <div className="px-2 py-1.5 text-[11px] font-black uppercase tracking-widest text-yellow-400 bg-red-900/60">Construction</div>

        <div className="flex border-b border-yellow-600/30">
          {CATS.map(c => (
            <button
              key={c.id}
              onClick={() => setCat(c.id)}
              title={CATEGORY_NAMES[c.id]}
              className={`flex-1 py-1.5 text-sm ${cat === c.id ? 'bg-yellow-500/20 border-b-2 border-yellow-400' : 'hover:bg-red-900/60'}`}
            >
              {c.icon}
            </button>
          ))}
        </div>

        <div className="flex-1 overflow-y-auto soviet-scroll">
          {items.map(id => {
            const def = BUILDINGS[id];
            const active = tool.kind === 'build' && tool.defId === id;
            const afford = instantBuild ? engine.dollars >= engine.instantCost(id) : engine.rubles >= def.costRubles;
            return (
              <button
                key={id}
                onClick={() => setTool(active ? { kind: 'select' } : { kind: 'build', defId: id })}
                disabled={!afford && !active}
                aria-disabled={!afford && !active}
                title={!afford ? `Cannot afford (${instantBuild ? `$${engine.instantCost(id)}` : `₽${def.costRubles}`})` : undefined}
                className={`w-full text-left px-2 py-1.5 flex items-start gap-2 border-b border-yellow-600/10 group ${active ? 'bg-yellow-500/25' : 'hover:bg-red-900/50'} ${!afford ? 'opacity-50 cursor-not-allowed' : ''}`}
              >
                <span className="text-lg leading-5">{def.icon}</span>
                <span className="flex-1 min-w-0">
                  <span className="block text-xs font-bold truncate">{def.name}</span>
                  <span className="block text-[10px] text-yellow-200/60">
                    {instantBuild ? `$${engine.instantCost(id)}` : `₽${def.costRubles}`}
                    {def.workers > 0 && ` · 👷${def.workers}`}
                    {Object.keys(def.materials).length > 0 && !instantBuild && (
                      <span> · {Object.entries(def.materials).map(([r, a]) => `${a}${RESOURCES[r as ResourceId].icon}`).join(' ')}</span>
                    )}
                  </span>
                  <span className="hidden group-hover:block text-[10px] text-yellow-100/80 leading-tight mt-0.5">{def.description}</span>
                </span>
              </button>
            );
          })}
        </div>

        <div className="border-t border-yellow-600/30 p-2 space-y-1.5">
          <label className="flex items-center gap-2 text-[11px] cursor-pointer select-none" title="Pay dollars to finish construction instantly, no materials or builders needed">
            <input type="checkbox" checked={instantBuild} onChange={e => setInstantBuild(e.target.checked)} className="accent-yellow-500" />
            <span>Instant build (Western $)</span>
          </label>
          <button
            onClick={() => setTool(tool.kind === 'bulldoze' ? { kind: 'select' } : { kind: 'bulldoze' })}
            className={`w-full rounded px-2 py-1 text-xs font-bold ${tool.kind === 'bulldoze' ? 'bg-red-500 text-white' : 'bg-red-900/70 hover:bg-red-800'}`}
          >
            🚜 Bulldoze
          </button>
          <div className="text-[9px] text-yellow-200/40 leading-tight">
            Left-click: place · drag: paint roads · right-drag: pan · wheel: zoom · Esc: cancel · Space: pause
          </div>
        </div>
      </div>
    </div>
  );
}
