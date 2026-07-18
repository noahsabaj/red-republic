import { useEffect } from 'react';
import type { Toast } from '@/hooks/use-toasts';
import { useFocusTrap } from '@/hooks/use-focus-trap';
import { GameIcon } from '@/ui/GameIcon';

// ------------------------------------------------------------
// Toasts
// ------------------------------------------------------------

export function ToastStack({ toasts }: { toasts: Toast[] }) {
  return (
    <div className="absolute top-16 left-1/2 -translate-x-1/2 z-30 flex flex-col items-center gap-1.5 pointer-events-none">
      {toasts.map(t => (
        <div key={t.id} className={`flex items-center gap-1.5 rounded px-3 py-1.5 text-xs font-bold shadow-2xl border animate-[fadein_.2s_ease-out] ${
          t.kind === 'good' ? 'bg-green-800/95 border-green-500 text-green-50'
          : t.kind === 'bad' ? 'bg-red-700/95 border-red-400 text-red-50'
          : 'bg-slate-700/95 border-slate-400 text-slate-50'}`}>
          {t.icon && <GameIcon name={t.icon} size={12} />}{t.text}
        </div>
      ))}
    </div>
  );
}

// ------------------------------------------------------------
// Intro
// ------------------------------------------------------------

export function IntroOverlay({ onStart }: { onStart: () => void }) {
  const trapRef = useFocusTrap<HTMLDivElement>();
  return (
    <div role="dialog" aria-modal="true" aria-label="Welcome to Red Republic" className="absolute inset-0 z-40 flex items-center justify-center bg-black/80 backdrop-blur-sm">
      <div ref={trapRef} tabIndex={-1} className="max-w-lg w-full mx-4 rounded-xl border-4 border-yellow-600 bg-gradient-to-b from-red-900 to-red-950 p-8 text-center shadow-2xl outline-none">
        <div className="text-6xl text-yellow-400 mb-2">★</div>
        <h1 className="text-3xl font-black uppercase tracking-[0.3em] text-yellow-100">Red Republic</h1>
        <div className="text-[11px] uppercase tracking-widest text-yellow-400/80 mt-1 mb-5">A planned-economy city builder · inspired by Workers &amp; Resources: Soviet Republic</div>
        <p className="text-sm text-yellow-100/85 leading-relaxed">
          Comrade, the Politburo has entrusted you with this land. There is no free market here —
          <b> you</b> plan everything: mines, factories, farms, housing, power, and every single truck.
          Feed your citizens, keep them warm through winter, and earn hard currency through foreign trade.
        </p>
        <div className="mt-4 grid grid-cols-2 gap-2 text-left text-[11px] text-yellow-100/75">
          <div><GameIcon name="cat-industry" size={12} className="text-yellow-400" /> Build production chains</div>
          <div><GameIcon name="truck" size={12} className="text-yellow-400" /> Trucks haul goods by road</div>
          <div><GameIcon name="users" size={12} className="text-yellow-400" /> Citizens need food &amp; warmth</div>
          <div><GameIcon name="winter" size={12} className="text-yellow-400" /> Survive the winter</div>
          <div><GameIcon name="trade" size={12} className="text-yellow-400" /> Trade with East (₽) &amp; West ($)</div>
          <div><GameIcon name="plan" size={12} className="text-yellow-400" /> Fulfill the Five-Year Plan</div>
        </div>
        <button onClick={onStart}
          className="mt-6 rounded-lg bg-yellow-500 px-8 py-3 text-lg font-black uppercase tracking-widest text-red-950 hover:bg-yellow-400 shadow-lg">
          Begin, Comrade
        </button>
      </div>
    </div>
  );
}

// ------------------------------------------------------------
// Help
// ------------------------------------------------------------

export function HelpOverlay({ onClose }: { onClose: () => void }) {
  const trapRef = useFocusTrap<HTMLDivElement>();
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose(); };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);
  return (
    <div role="dialog" aria-modal="true" aria-label="Commissar's Manual" className="absolute inset-0 z-40 flex items-center justify-center bg-black/70" onClick={onClose}>
      <div ref={trapRef} tabIndex={-1} className="max-w-xl w-full mx-4 max-h-[80vh] overflow-y-auto soviet-scroll rounded-xl border-2 border-yellow-600 bg-red-950 p-6 text-yellow-50 shadow-2xl outline-none" onClick={e => e.stopPropagation()}>
        <div className="flex items-center justify-between mb-3">
          <h2 className="text-lg font-black uppercase tracking-widest text-yellow-400">Commissar's Manual</h2>
          <button onClick={onClose} aria-label="Close help" className="text-yellow-200/60 hover:text-yellow-100"><GameIcon name="close" size={15} /></button>
        </div>
        <div className="space-y-3 text-xs leading-relaxed">
          <section>
            <div className="font-bold text-yellow-300 mb-1"><GameIcon name="plan" size={12} /> Goal</div>
            Grow a self-sufficient republic. Follow the Five-Year Plan objectives (<GameIcon name="plan" size={11} /> button) — they walk you through the economy step by step.
          </section>
          <section>
            <div className="font-bold text-yellow-300 mb-1"><GameIcon name="builders" size={12} /> Construction</div>
            Buildings cost rubles up-front, then need <b>materials</b> (planks, bricks, steel) delivered by truck plus <b>builders</b> from a staffed Construction Office. Your starting depot holds some materials. Or tick <b>Instant build</b> to pay Western dollars instead.
          </section>
          <section>
            <div className="font-bold text-yellow-300 mb-1"><GameIcon name="road" size={12} /> Roads &amp; Trucks</div>
            Every building must touch a road. Trucks (from Construction Offices) automatically haul goods between buildings — coal to power plants, food to stores, materials to construction sites. No road, no deliveries, no workers.
          </section>
          <section>
            <div className="font-bold text-yellow-300 mb-1"><GameIcon name="port" size={12} /> Water: bridges, ports &amp; barges</div>
            Roads placed over water become <b>bridges</b> at ₽{90}/tile — short crossings are cheap, long ones are megaprojects. For wide water, build <b>River Ports</b> on each shore: barges automatically ferry goods between ports when a delivery can't be made by road, hauling four truckloads per trip.
          </section>
          <section>
            <div className="font-bold text-yellow-300 mb-1"><GameIcon name="factory" size={12} /> Production chains</div>
            Wood → Planks · Gravel → Bricks · Iron ore + Coal → Steel · Oil → Fuel · Crops → Food &amp; Clothes · Coal → Power &amp; Heat. Mines must sit on their deposit (look for COAL/IRON/OIL/GRAVEL labels).
          </section>
          <section>
            <div className="font-bold text-yellow-300 mb-1"><GameIcon name="users" size={12} /> Citizens</div>
            Build houses and migrants arrive. Citizens need <b>food &amp; clothes</b> (a stocked State Store within 8 tiles), <b>power</b>, <b>heat in winter</b>, jobs, healthcare and culture. Unhappy citizens leave. Industry pollutes — keep it away from homes.
          </section>
          <section>
            <div className="font-bold text-yellow-300 mb-1"><GameIcon name="crops" size={12} /> Seasons</div>
            Farms sow in spring and harvest late summer to autumn — stockpile crops for winter. Heating follows the actual temperature: the colder it gets, the more heat homes demand and the more coal your Heating Plant burns.
          </section>
          <section>
            <div className="font-bold text-yellow-300 mb-1"><GameIcon name="overcast" size={12} /> Weather</div>
            Every day has real weather — the top bar shows today plus the State Hydrometeorological Service's exact 5-day forecast. Rain slows trucks but feeds the crops; storms and blizzards slow everything and ground barges; frost stops crop growth and summer droughts wither it. Sustained cold <b>freezes the river</b>, ice-locking barge traffic until the thaw — stockpile across the water before winter, or pay for a bridge.
          </section>
          <section>
            <div className="font-bold text-yellow-300 mb-1"><GameIcon name="flag" size={12} /> The border &amp; trade</div>
            One map edge is the <b>national border</b> — the striped line with the red-white posts. Foreign soil beyond it is untouchable, and every Customs House must stand at the border. Sell surplus there: East pays rubles (₽), the West pays hard dollars ($) — best for fuel and steel. Import goods you lack at 1.6× price. Workers' wages are paid daily in rubles.
          </section>
          <section>
            <div className="font-bold text-yellow-300 mb-1"><GameIcon name="trade" size={12} /> Auto-trade</div>
            Tick <b>Auto-trade</b> in the Foreign Trade panel and set per-good rules: <b>Imp</b> keeps the town stocked to a level (imports the deficit daily), <b>Exp</b> sells everything above a keep-level — trucks stage the surplus to customs, and the customs house sells what reached it. A staffed, powered Customs House clears more tonnage per day; the <b>reserve floor</b> stops automation from ever spending wages money. The daily ledger in the panel shows what moved.
          </section>
          <section>
            <div className="font-bold text-yellow-300 mb-1"><GameIcon name="contract" size={12} /> Contracts</div>
            Every couple of months a bloc tenders a bulk order at a <b>premium price</b>, locked when offered. Accept it and every export of that good to that bloc counts toward it — deliver in time for the premium, miss the deadline and you pay a fine while that bloc's prices sour on you for a couple of months.
          </section>
          <section>
            <div className="font-bold text-yellow-300 mb-1"><GameIcon name="keyboard" size={12} /> Controls</div>
            Left-click place/select · <b>Shift/Ctrl</b>+click multi-select buildings &amp; deposits · drag to paint roads · right-drag, left-drag or <b>WASD</b> to pan · mouse wheel to zoom · <b>Esc</b> cancel tool · <b>Space</b> pause · <b>1/2/3</b> game speed.
          </section>
        </div>
      </div>
    </div>
  );
}

