import { useState } from 'react';
import { GameIcon } from '@/ui/GameIcon';
import { primaryBtn, secondaryBtn } from './menu/controls';

interface Props {
  version: string;
  install: () => Promise<void>;
  onDismiss: () => void;
  notify: (text: string, kind?: 'good' | 'bad' | 'info', icon?: string) => void;
}

/**
 * Desktop-only: bottom-right card offering to install an available update.
 * Deliberately non-modal and outside the one-dialog screen machine — it may
 * appear over the main menu or mid-game without fighting anything.
 */
export function UpdateBanner({ version, install, onDismiss, notify }: Props) {
  const [busy, setBusy] = useState(false);
  return (
    <div className="absolute bottom-4 right-4 z-30 w-72 rounded-lg border-2 border-yellow-600/60 bg-red-950/95 p-3 text-yellow-50 shadow-2xl">
      <div className="flex items-center gap-2 text-xs font-black uppercase tracking-widest text-yellow-400">
        <GameIcon name="download" size={14} />Update {version} available
      </div>
      <p className="mt-1.5 text-[0.6875rem] leading-snug text-yellow-100/75">
        {busy
          ? 'Downloading — the app restarts when the update is ready.'
          : 'Restart now to install, or keep playing — the offer returns next launch.'}
      </p>
      <div className="mt-2.5 flex justify-end gap-2">
        <button className={secondaryBtn} data-sfx="back" disabled={busy} onClick={onDismiss}>Later</button>
        <button
          className={primaryBtn}
          data-sfx="confirm"
          disabled={busy}
          onClick={() => {
            setBusy(true);
            install().catch(() => {
              setBusy(false);
              notify('Update failed — it will retry next launch', 'bad');
            });
          }}
        >
          Restart &amp; Update
        </button>
      </div>
    </div>
  );
}
