import { useEffect, useState } from 'react';
import type { ReactNode } from 'react';
import { GameIcon } from '@/ui/GameIcon';

/** Label + description on the left, control on the right. */
export function SettingRow({ label, description, children }: { label: string; description?: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4 py-2 border-b border-yellow-600/15 last:border-b-0">
      <div className="min-w-0">
        <div className="text-xs font-bold text-yellow-100">{label}</div>
        {description && <div className="text-[0.6875rem] text-yellow-200/60 leading-snug">{description}</div>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

export function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={`relative h-5 w-10 rounded-full border transition-colors ${checked ? 'bg-yellow-500 border-yellow-400' : 'bg-red-950/60 border-yellow-600/40'}`}
    >
      <span className={`absolute top-0.5 h-3.5 w-3.5 rounded-full transition-all ${checked ? 'left-[22px] bg-red-950' : 'left-1 bg-yellow-200/70'}`} />
    </button>
  );
}

export function RangeSlider({ value, min, max, step, onChange, label, format }: {
  value: number; min: number; max: number; step: number;
  onChange: (v: number) => void; label: string; format?: (v: number) => string;
}) {
  return (
    <div className="flex items-center gap-2">
      <input
        type="range"
        className="soviet-range w-32"
        aria-label={label}
        min={min} max={max} step={step} value={value}
        onChange={e => onChange(Number(e.target.value))}
      />
      <span className="w-12 text-right text-[0.6875rem] font-bold text-yellow-200/80 tabular-nums">
        {format ? format(value) : value}
      </span>
    </div>
  );
}

export function Segmented<T extends string | number>({ options, value, onChange, label }: {
  options: { value: T; label: string }[]; value: T; onChange: (v: T) => void; label: string;
}) {
  return (
    <div role="group" aria-label={label} className="flex gap-1">
      {options.map(o => (
        <button
          key={String(o.value)}
          aria-pressed={o.value === value}
          onClick={() => onChange(o.value)}
          className={`px-2 py-0.5 rounded text-[0.6875rem] font-bold ${o.value === value ? 'bg-yellow-500 text-red-950' : 'bg-red-950/60 text-yellow-100/70 hover:bg-red-900'}`}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

/** Selectable card for map size / climate / difficulty choices. */
export function OptionCard({ selected, icon, label, blurb, onClick }: {
  selected: boolean; icon?: string; label: string; blurb: string; onClick: () => void;
}) {
  return (
    <button
      aria-pressed={selected}
      onClick={onClick}
      className={`flex-1 min-w-0 rounded border-2 p-2 text-left transition-colors ${
        selected ? 'border-yellow-500 bg-red-900/80' : 'border-yellow-600/30 bg-red-950/60 hover:bg-red-900/50'}`}
    >
      <div className={`flex items-center gap-1.5 text-xs font-black uppercase tracking-wider ${selected ? 'text-yellow-300' : 'text-yellow-100/90'}`}>
        {icon && <GameIcon name={icon} size={13} />}{label}
      </div>
      <div className="mt-0.5 text-[0.625rem] leading-snug text-yellow-200/60">{blurb}</div>
    </button>
  );
}

/**
 * Inline two-step confirm: the first click arms the button (label swaps,
 * danger styling) for 3 s; the second click commits. No nested modals.
 */
export function TwoStepButton({ label, confirmLabel, onConfirm, className, armedClassName, disabled, title }: {
  label: ReactNode; confirmLabel: ReactNode; onConfirm: () => void;
  /** Geometry + palette for both states; arming may only change palette, never size. */
  className?: string; armedClassName?: string; disabled?: boolean; title?: string;
}) {
  const [armed, setArmed] = useState(false);
  useEffect(() => {
    if (!armed) return;
    const t = setTimeout(() => setArmed(false), 3000);
    return () => clearTimeout(t);
  }, [armed]);
  return (
    <button
      disabled={disabled}
      title={title}
      onClick={() => {
        if (armed) { setArmed(false); onConfirm(); }
        else setArmed(true);
      }}
      className={armed ? armedClassName ?? rowBtnDanger : className ?? rowBtnMuted}
    >
      {armed ? confirmLabel : label}
    </button>
  );
}

/** Standard primary/secondary menu buttons. */
export const primaryBtn = 'rounded-lg bg-yellow-500 px-4 py-2 text-sm font-black uppercase tracking-widest text-red-950 hover:bg-yellow-400 shadow-lg disabled:opacity-40';
export const secondaryBtn = 'rounded-lg bg-red-900/70 border border-yellow-600/40 px-4 py-2 text-sm font-bold uppercase tracking-wider text-yellow-100 hover:bg-red-800';

/**
 * Compact list-row action buttons (save slots and similar): one fixed height
 * with flex centering so text and icon-only buttons align. Without this, an
 * icon-only button's height is set by the line box of the parent's inherited
 * font — text that is never rendered — and drifts from its text siblings.
 */
export const rowBtn = 'flex h-6 shrink-0 items-center justify-center rounded px-2 text-[0.6875rem] font-bold';
export const rowBtnPrimary = `${rowBtn} bg-yellow-500 text-red-950 hover:bg-yellow-400`;
export const rowBtnMuted = `${rowBtn} bg-red-900/70 text-yellow-100 hover:bg-red-800 disabled:opacity-40`;
export const rowBtnDanger = `${rowBtn} bg-red-600 text-white hover:bg-red-500`;
