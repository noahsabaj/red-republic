import { useEffect } from 'react';
import type { ReactNode } from 'react';
import { useFocusTrap } from '@/hooks/use-focus-trap';
import { GameIcon } from '@/ui/GameIcon';
import { uiSound } from '@/audio';

interface Props {
  title: string;
  icon?: string;
  onBack: () => void;
  /** Suppress the Escape handler while another layer (Help) owns Escape. */
  escDisabled?: boolean;
  /** Tailwind max-width class for the dialog card. */
  width?: string;
  children: ReactNode;
  footer?: ReactNode;
}

/**
 * Shared chrome for every menu dialog: Soviet panel styling, focus trap,
 * and the single Escape-goes-back handler. The screen state machine
 * guarantees exactly one MenuShell is mounted at a time, so there is
 * never a second competing Escape listener.
 */
export function MenuShell({ title, icon, onBack, escDisabled, width = 'max-w-md', children, footer }: Props) {
  const trapRef = useFocusTrap<HTMLDivElement>();
  useEffect(() => {
    if (escDisabled) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') { e.preventDefault(); uiSound('back'); onBack(); }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onBack, escDisabled]);

  return (
    <div role="dialog" aria-modal="true" aria-label={title} className="absolute inset-0 z-40 flex items-center justify-center bg-black/70 backdrop-blur-sm">
      <div ref={trapRef} tabIndex={-1} className={`${width} w-full mx-4 max-h-[88vh] flex flex-col rounded-lg border-2 border-yellow-600/60 bg-red-950/95 text-yellow-50 shadow-2xl outline-none overflow-hidden`}>
        <header className="flex items-center justify-between px-4 py-2.5 bg-red-900/60 border-b border-yellow-600/30 shrink-0">
          <span className="flex items-center gap-2 text-yellow-400 font-black uppercase tracking-widest text-xs">
            {icon && <GameIcon name={icon} size={14} />}{title}
          </span>
          <button onClick={onBack} aria-label="Back" data-sfx="back" className="flex items-center justify-center text-yellow-200/60 hover:text-yellow-100">
            <GameIcon name="close" size={15} />
          </button>
        </header>
        <div className="flex-1 overflow-y-auto soviet-scroll p-4">
          {children}
        </div>
        {footer && (
          <footer className="flex items-center justify-between gap-2 px-4 py-2.5 bg-red-900/40 border-t border-yellow-600/30 shrink-0">
            {footer}
          </footer>
        )}
      </div>
    </div>
  );
}
