import { MenuShell } from './MenuShell';
import { primaryBtn, secondaryBtn } from './controls';
import { GameIcon } from '@/ui/GameIcon';

interface Props {
  overlay: 'root' | 'exit' | 'confirm-exit' | 'confirm-restart' | 'confirm-quit';
  republicName: string;
  unsavedDays: number;
  escDisabled: boolean;
  canQuit: boolean;             // desktop: offer "Exit to Desktop"
  onResume: () => void;
  onSave: () => void;
  onLoad: () => void;
  onOptions: () => void;
  onManual: () => void;
  onRestartRequest: () => void; // -> confirm-restart
  onRestartConfirm: () => void;
  onExitChooser: () => void;    // desktop: -> the exit chooser (Main Menu / Desktop)
  onExitRequest: () => void;    // -> confirm-exit (or straight exit when nothing is unsaved)
  onExitConfirm: () => void;
  onQuitRequest: () => void;    // desktop: -> confirm-quit (or straight quit when nothing is unsaved)
  onQuitConfirm: () => void;    // desktop only: close the application
  onBack: () => void;
}

export function PauseMenu(p: Props) {
  const item = (label: string, icon: string, onClick: () => void, primary = false, sfx?: string) => (
    <button
      onClick={onClick}
      data-sfx={sfx}
      className={`flex items-center gap-2.5 rounded px-3 py-2 text-xs font-black uppercase tracking-widest ${primary
        ? 'bg-yellow-500 text-red-950 hover:bg-yellow-400'
        : 'bg-red-900/60 text-yellow-100 hover:bg-red-800'}`}
    >
      <GameIcon name={icon} size={14} />{label}
    </button>
  );

  if (p.overlay === 'confirm-restart') {
    return (
      <MenuShell title="Restart the Republic?" icon="restart" onBack={p.onBack} escDisabled={p.escDisabled} width="max-w-sm">
        <p className="text-xs leading-relaxed text-yellow-100/85">
          Rebuild <b>{p.republicName}</b> from day one — same map, same seed, same climate.
          {p.unsavedDays > 0 && <> The <b>{p.unsavedDays} day{p.unsavedDays > 1 ? 's' : ''}</b> since your last save will be lost.</>}
        </p>
        <div className="mt-4 flex justify-end gap-2">
          <button className={secondaryBtn} data-sfx="back" onClick={p.onBack}>Back</button>
          <button className={primaryBtn} data-sfx="commit" onClick={p.onRestartConfirm}>Restart</button>
        </div>
      </MenuShell>
    );
  }

  if (p.overlay === 'confirm-quit') {
    return (
      <MenuShell title="Quit Red Republic?" icon="exit" onBack={p.onBack} escDisabled={p.escDisabled} width="max-w-sm">
        <p className="text-xs leading-relaxed text-yellow-100/85">
          <b>{p.unsavedDays} day{p.unsavedDays > 1 ? 's' : ''}</b> of progress since your last save will be lost.
          Save first, comrade?
        </p>
        <div className="mt-4 flex justify-end gap-2">
          <button className={secondaryBtn} data-sfx="back" onClick={p.onBack}>Back</button>
          <button className={secondaryBtn} onClick={p.onSave}>Save Game</button>
          <button className={primaryBtn} data-sfx="commit" onClick={p.onQuitConfirm}>Quit Anyway</button>
        </div>
      </MenuShell>
    );
  }

  if (p.overlay === 'confirm-exit') {
    return (
      <MenuShell title="Exit to Main Menu?" icon="exit" onBack={p.onBack} escDisabled={p.escDisabled} width="max-w-sm">
        <p className="text-xs leading-relaxed text-yellow-100/85">
          <b>{p.unsavedDays} day{p.unsavedDays > 1 ? 's' : ''}</b> of progress since your last save will be lost.
          Save first, comrade?
        </p>
        <div className="mt-4 flex justify-end gap-2">
          <button className={secondaryBtn} data-sfx="back" onClick={p.onBack}>Back</button>
          <button className={secondaryBtn} onClick={p.onSave}>Save Game</button>
          <button className={primaryBtn} data-sfx="commit" onClick={p.onExitConfirm}>Exit Anyway</button>
        </div>
      </MenuShell>
    );
  }

  // desktop-only chooser: leave to the main menu, or all the way to the desktop
  if (p.overlay === 'exit') {
    return (
      <MenuShell title="Exit" icon="exit" onBack={p.onBack} escDisabled={p.escDisabled} width="max-w-xs">
        <div className="flex flex-col gap-1.5">
          {item('Exit to Main Menu', 'exit', p.onExitRequest, true)}
          {item('Exit to Desktop', 'close', p.onQuitRequest, false, 'commit')}
        </div>
      </MenuShell>
    );
  }

  return (
    <MenuShell title="— Paused —" icon="pause" onBack={p.onBack} escDisabled={p.escDisabled} width="max-w-xs">
      <div className="flex flex-col gap-1.5">
        {item('Resume', 'play', p.onResume, true, 'back')}
        {item('Save Game', 'save', p.onSave)}
        {item('Load Game', 'load', p.onLoad)}
        {item('Options', 'settings', p.onOptions)}
        {item('Manual', 'help', p.onManual)}
        {item('Restart', 'restart', p.onRestartRequest)}
        {item('Exit', 'exit', p.canQuit ? p.onExitChooser : p.onExitRequest)}
      </div>
    </MenuShell>
  );
}
