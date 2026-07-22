import { useRef, useState } from 'react';
import { BUILDINGS, CATEGORIES, RESOURCES, SUBCATEGORIES } from '@/game/config';
import type { Category, ResourceId } from '@/game/config';
import type { GameEngine } from '@/game/engine';
import { fmtMoney } from '@/game/format';
import { useEngineSignature } from '@/hooks/use-engine';
import { GameIcon } from '@/ui/GameIcon';
import { ToggleButton } from '@/components/menu/controls';
import { buildCostText, canAffordBuild, materialsShort } from '@/ui/build-cost';
import type { BuildPayMode } from '@/ui/build-cost';
import type { BuildPolicy, Tool } from './GameCanvas';

interface Props {
  engine: GameEngine;
  tool: Tool;
  setTool: (t: Tool) => void;
  policy: BuildPolicy;
  setPolicy: (patch: Partial<BuildPolicy>) => void;
  push: (text: string, kind?: 'good' | 'bad' | 'info', icon?: string) => void;
}

/** The mode-aware hover card: cost in the active funding mode, then the specs. */
function InfoCard({ engine, defId, mode, currency }: { engine: GameEngine; defId: string; mode: BuildPayMode; currency: 'east' | 'west' }) {
  const def = BUILDINGS[defId];
  const mats = Object.entries(def.materials) as [ResourceId, number][];
  const io = (rec?: Partial<Record<ResourceId, number>>) =>
    rec ? (Object.entries(rec) as [ResourceId, number][]) : [];
  const inputs = io(def.inputs), outputs = io(def.outputs);
  const powerLine = def.powerOutput ? `+${def.powerOutput} MW` : def.power ? `−${def.power} MW` : null;
  return (
    <div className="pointer-events-none absolute bottom-full left-1/2 z-40 mb-2 w-56 -translate-x-1/2 rounded-lg border border-yellow-600/60 bg-red-950/95 p-2.5 text-yellow-50 shadow-2xl animate-in fade-in duration-150">
      <div className="flex items-center gap-1.5 text-xs font-black uppercase tracking-wide text-yellow-300">
        <GameIcon name={def.icon} size={14} /> {def.name}
      </div>
      <div className="mt-1 text-sm font-bold text-yellow-400 tabular-nums">{buildCostText(engine, defId, mode, currency)}</div>
      {mode === 'materials' && (
        <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[0.625rem] text-yellow-200/70">
          <span className="inline-flex items-center gap-0.5"><GameIcon name="builders" size={10} />{def.labor}</span>
          {mats.map(([r, a]) => (
            <span key={r} className="inline-flex items-center gap-0.5">{a}<GameIcon name={RESOURCES[r].icon} size={10} /></span>
          ))}
        </div>
      )}
      <div className="mt-1.5 space-y-0.5 border-t border-yellow-600/20 pt-1.5 text-[0.625rem] text-yellow-200/70">
        {def.workers > 0 && <div className="inline-flex items-center gap-1"><GameIcon name="staff" size={10} /> {def.workers} workers</div>}
        {def.housingCapacity ? <div className="inline-flex items-center gap-1"><GameIcon name="beds" size={10} /> houses {def.housingCapacity}</div> : null}
        {(inputs.length > 0 || outputs.length > 0) && (
          <div className="flex flex-wrap items-center gap-1">
            {inputs.map(([r, a]) => <span key={r} className="inline-flex items-center gap-0.5">{a}<GameIcon name={RESOURCES[r].icon} size={10} /></span>)}
            {inputs.length > 0 && outputs.length > 0 && <span className="text-yellow-500">→</span>}
            {outputs.map(([r, a]) => <span key={r} className="inline-flex items-center gap-0.5">{a}<GameIcon name={RESOURCES[r].icon} size={10} /></span>)}
          </div>
        )}
        {powerLine && <div className="inline-flex items-center gap-1"><GameIcon name="power" size={10} /> {powerLine}</div>}
      </div>
      <div className="mt-1.5 text-[0.625rem] leading-snug text-yellow-200/50">{def.description}</div>
    </div>
  );
}

/** One height + no-shrink for every control in the bar. A single source of truth
 *  keeps the whole row aligned, and shrink-0 makes the bar scroll on narrow
 *  screens instead of squashing a label (e.g. "Instant $") onto two lines. */
const BAR_CTL = 'h-10 shrink-0';

export default function BottomBar({ engine, tool, setTool, policy, setPolicy, push }: Props) {
  const [cat, setCat] = useState<Category | null>(null);
  const [sub, setSub] = useState<string | null>(null);
  const [hidden, setHidden] = useState(false);
  const [peeking, setPeeking] = useState(false);

  const [hoveredDefId, setHoveredDefId] = useState<string | null>(null);
  const hoverTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearHoverTimer = () => {
    if (hoverTimerRef.current) {
      clearTimeout(hoverTimerRef.current);
      hoverTimerRef.current = null;
    }
  };

  const handleMouseEnterCard = (id: string) => {
    clearHoverTimer();
    hoverTimerRef.current = setTimeout(() => {
      setHoveredDefId(id);
    }, 500);
  };

  const handleMouseLeaveCard = () => {
    clearHoverTimer();
    setHoveredDefId(null);
  };

  const mode: BuildPayMode = policy.instant ? 'instant' : policy.autoBuy ? 'autoBuy' : 'materials';

  // re-render when affordability, the foreign-labor default, or the planned-site count changes
  useEngineSignature(engine, (e) => [e.rubles, e.dollars, e.foreignLaborEnabled, e.plannedCount()]);

  const plannedN = engine.plannedCount();
  const commenceCost = plannedN > 0 ? engine.plannedCommenceCost() : null;
  const commenceCostText = commenceCost
    ? [commenceCost.rubles ? `₽${fmtMoney(commenceCost.rubles)}` : null,
       commenceCost.dollars ? `$${fmtMoney(commenceCost.dollars)}` : null].filter(Boolean).join(' / ') || 'free'
    : '';

  const openCat = (c: Category) => {
    clearHoverTimer();
    setHoveredDefId(null);
    setCat(prev => (prev === c ? null : c));
    setSub(null);
  };
  const hide = () => { clearHoverTimer(); setHoveredDefId(null); setHidden(true); setPeeking(false); setCat(null); };
  const retracted = hidden && !peeking;

  const activeCat = cat ? CATEGORIES.find(c => c.id === cat)! : null;
  const subs = cat ? SUBCATEGORIES[cat] : [];
  const activeSub = sub ? subs.find(s => s.id === sub) : null;

  return (
    <div
      className="pointer-events-none absolute inset-x-0 bottom-0 z-10 flex flex-col justify-end transition-transform duration-200"
      style={{ transform: retracted ? 'translateY(calc(100% - 0.95rem))' : undefined }}
      onMouseEnter={() => hidden && setPeeking(true)}
      onMouseLeave={() => setPeeking(false)}
    >
      {/* ---- drill-down flyout (tiers 2 & 3), only while pinned ---- */}
      {!hidden && activeCat && (
        <div className="pointer-events-auto mx-2 mb-1.5 self-center rounded-lg border-2 border-yellow-600/60 bg-red-950/95 p-2 shadow-2xl">
          <div className="mb-1.5 flex items-center gap-1.5 px-1 text-[0.6875rem] font-black uppercase tracking-widest" style={{ color: activeCat.accent }}>
            <GameIcon name={activeCat.icon} size={13} /> {activeCat.name}
          </div>
          {/* tier 2: sub-category strip */}
          <div className="flex flex-wrap gap-1.5">
            {subs.map(s => (
              <button
                key={s.id}
                aria-pressed={sub === s.id}
                onClick={() => {
                  clearHoverTimer();
                  setHoveredDefId(null);
                  setSub(prev => (prev === s.id ? null : s.id));
                }}
                className={`rounded px-2.5 py-1 text-[0.6875rem] font-bold ${sub === s.id ? 'bg-yellow-500 text-red-950' : 'bg-red-900/60 text-yellow-100/80 hover:bg-red-800'}`}
              >
                {s.name} <span className="opacity-60 tabular-nums">{s.ids.length}</span>
              </button>
            ))}
          </div>
          {/* tier 3: building grid for the chosen sub-category */}
          {activeSub && (
            <div className="mt-2 grid grid-cols-[repeat(auto-fill,minmax(4.75rem,1fr))] gap-1.5" style={{ minWidth: 'min(34rem, 78vw)' }}>
              {activeSub.ids.map(id => {
                const def = BUILDINGS[id];
                const active = tool.kind === 'build' && tool.defId === id;
                const afford = canAffordBuild(engine, id, mode, policy.currency);
                const short = mode === 'materials' ? materialsShort(engine, id) : [];
                return (
                  <button
                    key={id}
                    data-sfx="none" // the setTool funnel voices toolArm/toolCancel
                    disabled={!afford && !active}
                    onMouseEnter={() => handleMouseEnterCard(id)}
                    onMouseLeave={handleMouseLeaveCard}
                    onClick={() => {
                      clearHoverTimer();
                      setHoveredDefId(null);
                      setTool(active ? { kind: 'select' } : { kind: 'build', defId: id });
                    }}
                    className={`group relative flex flex-col items-center gap-1 rounded-md border p-1.5 text-center transition-colors disabled:opacity-40 ${
                      active ? 'border-yellow-400 bg-yellow-500/20' : 'border-yellow-600/25 bg-red-900/40 hover:bg-red-800/70'
                    }`}
                    style={{ '--c': activeCat.accent } as React.CSSProperties}
                  >
                    <GameIcon name={def.icon} size={26} className="text-yellow-200" />
                    <span className="text-[0.625rem] font-bold leading-tight text-yellow-100">{def.name}</span>
                    <span className={`text-[0.5625rem] tabular-nums ${short.length ? 'text-amber-400' : 'text-yellow-200/55'}`}>
                      {buildCostText(engine, id, mode, policy.currency)}
                    </span>
                    {hoveredDefId === id && <InfoCard engine={engine} defId={id} mode={mode} currency={policy.currency} />}
                  </button>
                );
              })}
            </div>
          )}
        </div>
      )}

      {/* ---- the bar ---- */}
      <div className="pointer-events-auto flex items-stretch gap-2 overflow-x-auto border-t-2 border-yellow-600/60 bg-gradient-to-b from-red-900 to-red-950 px-2 py-1.5 shadow-[0_-10px_26px_rgba(0,0,0,0.34)]">
        {/* left: build configurations (the BuildPolicy stamped onto each new site) */}
        <div className="flex flex-1 items-center justify-start gap-1.5">
          {/* Import + its currency read as one control: tighter internal gap groups
              them, the segmented border keeps ₽/$ associated, no bulky frame. */}
          <div className="flex items-center gap-1">
            <ToggleButton on={policy.autoBuy} onChange={v => setPolicy({ autoBuy: v })} className={BAR_CTL}
              icon="truck" label="Auto-buy" title="Auto-buy new sites' construction materials at the border (₽ East / $ West) instead of hauling them from your own stockpiles" />
            <div className={`${BAR_CTL} flex overflow-hidden rounded-md border border-yellow-600/30`} role="group" aria-label="Import currency">
              {(['east', 'west'] as const).map(cur => (
                <button
                  key={cur}
                  aria-pressed={policy.currency === cur}
                  onClick={() => setPolicy({ currency: cur })}
                  className={`flex items-center justify-center px-2 text-[0.6875rem] font-black ${policy.currency === cur ? 'bg-yellow-500 text-red-950' : 'bg-red-950/50 text-yellow-100/60 hover:bg-red-900'}`}
                >{cur === 'east' ? '₽' : '$'}</button>
              ))}
            </div>
          </div>
          <ToggleButton on={policy.instant} onChange={v => setPolicy({ instant: v })} className={BAR_CTL}
            icon="download" label="Instant $" title="Import a finished Western prefab — completes immediately for dollars" />
          <ToggleButton on={engine.foreignLaborEnabled} onChange={v => engine.setForeignLaborEnabled(v)} className={BAR_CTL}
            icon="users" label="Foreign" title="New sites may hire paid foreign builders before you have citizens" />
          <ToggleButton on={policy.plan} onChange={v => setPolicy({ plan: v })} className={BAR_CTL}
            icon="contract" label="Plan" title="Place sites without commencing — begin construction later" />
          {plannedN > 0 && (
            <button
              title={`Commence every planned site the treasury can afford, highest priority first${commenceCostText ? ` — ${commenceCostText}` : ''}`}
              data-sfx="confirm"
              className={`${BAR_CTL} flex items-center gap-1.5 whitespace-nowrap rounded-md px-2.5 text-[0.6875rem] font-bold bg-yellow-500 text-red-950 hover:bg-yellow-400`}
              onClick={() => {
                const started = engine.commenceAllPlanned();
                push(started < plannedN
                  ? `Commenced ${started} of ${plannedN} — the rest are unaffordable`
                  : `Commenced ${started} planned site${started === 1 ? '' : 's'}`,
                  started < plannedN ? 'bad' : 'good', 'builders');
              }}
            >
              <GameIcon name="builders" size={14} /> Commence All ({plannedN})
            </button>
          )}
        </div>

        {/* middle: build options — the category drill-down, centered between the
            config zone (left) and the tools zone (right) via equal flex-1 sides */}
        <div className="flex shrink-0 items-center justify-center gap-1.5">
          {CATEGORIES.map(c => (
            <button
              key={c.id}
              aria-pressed={cat === c.id}
              title={c.name}
              onClick={() => openCat(c.id)}
              className={`${BAR_CTL} flex min-w-[3.25rem] flex-col items-center justify-center gap-0.5 rounded-md border-b-[3px] px-2 transition-colors ${
                cat === c.id ? 'bg-red-800/80 text-yellow-50' : 'bg-red-950/40 text-yellow-100/70 hover:bg-red-900/70'
              }`}
              style={{ borderBottomColor: cat === c.id ? c.accent : `${c.accent}66` }}
            >
              <span style={{ color: c.accent }}><GameIcon name={c.icon} size={20} /></span>
              <span className="text-[0.5625rem] font-bold uppercase tracking-wide leading-none">{c.name.split(' ')[0]}</span>
            </button>
          ))}
        </div>

        {/* right: destructive + bar visibility tools */}
        <div className="flex flex-1 items-center justify-end gap-1.5">
          <button
            data-sfx="none" // the setTool funnel voices toolArm/toolCancel
            aria-pressed={tool.kind === 'bulldoze'}
            title="Demolish (X)"
            onClick={() => setTool(tool.kind === 'bulldoze' ? { kind: 'select' } : { kind: 'bulldoze' })}
            className={`${BAR_CTL} flex items-center gap-1.5 whitespace-nowrap rounded-md px-2.5 text-[0.6875rem] font-bold ${
              tool.kind === 'bulldoze' ? 'bg-red-500 text-white' : 'border border-red-400/30 bg-red-900/50 text-red-100 hover:bg-red-800'
            }`}
          >
            <GameIcon name="bulldoze" size={16} /> Demolish
          </button>
          <button
            data-sfx="panel"
            aria-pressed={!hidden}
            title={hidden ? 'Pin the build bar open' : 'Hide the build bar'}
            onClick={() => (hidden ? (setHidden(false), setPeeking(false)) : hide())}
            className={`${BAR_CTL} flex items-center justify-center rounded-md border border-yellow-600/30 bg-red-950/40 px-2 text-yellow-100/80 hover:bg-red-900/70`}
          >
            <GameIcon name={hidden ? 'pin' : 'chevronDown'} size={16} />
          </button>
        </div>
      </div>
    </div>
  );
}
