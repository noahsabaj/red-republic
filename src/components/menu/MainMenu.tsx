import { useMemo } from 'react';
import { BALANCE } from '@/game/config';
import { listSlots } from '@/game/save-slots';
import type { SlotMeta } from '@/game/save-slots';
import { useFocusTrap } from '@/hooks/use-focus-trap';
import { GameIcon } from '@/ui/GameIcon';

interface Props {
  onContinue: (latest: SlotMeta) => void;
  onNewGame: () => void;
  onLoad: () => void;
  onOptions: () => void;
  onManual: () => void;
}

/** The landing screen. Lives over the MenuBackdrop attract canvas. */
export function MainMenu({ onContinue, onNewGame, onLoad, onOptions, onManual }: Props) {
  const trapRef = useFocusTrap<HTMLDivElement>();
  // remounts on every return to the menu root, so this stays fresh
  const latest = useMemo(() => listSlots()[0] ?? null, []);
  const hasSaves = latest !== null;

  const item = (label: string, icon: string, onClick: () => void, primary = false, sub?: string) => (
    <button
      onClick={onClick}
      className={`flex flex-col items-center rounded-lg px-4 py-2.5 shadow-lg ${primary
        ? 'bg-yellow-500 text-red-950 hover:bg-yellow-400'
        : 'bg-red-900/80 border border-yellow-600/40 text-yellow-100 hover:bg-red-800'}`}
    >
      <span className="flex items-center gap-2 text-sm font-black uppercase tracking-widest">
        <GameIcon name={icon} size={15} />{label}
      </span>
      {sub && <span className={`text-[0.625rem] mt-0.5 ${primary ? 'text-red-950/70' : 'text-yellow-200/60'}`}>{sub}</span>}
    </button>
  );

  return (
    // pointer-events-none: this is a full-screen LAYOUT container, not a modal —
    // it must never swallow clicks meant for non-modal overlays beneath it
    // (the desktop UpdateBanner sits at z-30 under this z-40 layer)
    <div className="pointer-events-none absolute inset-0 z-40 flex flex-col items-center justify-center">
      <div className="text-7xl text-yellow-400 drop-shadow-lg">★</div>
      <h1 className="mt-1 text-5xl font-black uppercase tracking-[0.3em] text-yellow-100 drop-shadow-lg">Red Republic</h1>
      <div className="mt-2 text-[0.6875rem] uppercase tracking-widest text-yellow-400/80">A planned-economy city builder</div>

      <div ref={trapRef} tabIndex={-1} className="pointer-events-auto mt-8 flex w-72 flex-col gap-2 outline-none">
        {hasSaves && item(
          'Continue', 'play', () => onContinue(latest), true,
          `${latest.header.name} · ${BALANCE.months[latest.header.month - 1]} ${latest.header.year}`,
        )}
        {item('New Game', 'flag', onNewGame, !hasSaves)}
        {item('Load Game', 'load', onLoad)}
        {item('Options', 'settings', onOptions)}
        {item('Manual', 'help', onManual)}
      </div>

      <div className="absolute bottom-4 text-[0.625rem] uppercase tracking-widest text-yellow-200/40">
        v{__APP_VERSION__} · Inspired by Workers &amp; Resources: Soviet Republic
      </div>
    </div>
  );
}
