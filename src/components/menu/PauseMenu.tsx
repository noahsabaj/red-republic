import { MenuShell } from './MenuShell';
import { primaryBtn, secondaryBtn } from './controls';
import { GameIcon } from '@/ui/GameIcon';

interface Props {
  overlay: 'root' | 'confirm-exit' | 'confirm-restart';
  republicName: string;
  unsavedDays: number;
  escDisabled: boolean;
  onResume: () => void;
  onSave: () => void;
  onLoad: () => void;
  onOptions: () => void;
  onManual: () => void;
  onRestartRequest: () => void; // -> confirm-restart
  onRestartConfirm: () => void;
  onExitRequest: () => void;    // -> confirm-exit (or straight exit when nothing is unsaved)
  onExitConfirm: () => void;
  onBack: () => void;
}

export function PauseMenu(p: Props) {
  if (p.overlay === 'confirm-restart') {
    return (
      <MenuShell title="Restart the Republic?" icon="restart" onBack={p.onBack} escDisabled={p.escDisabled} width="max-w-sm">
        <p className="text-xs leading-relaxed text-yellow-100/85">
          Rebuild <b>{p.republicName}</b> from day one — same map, same seed, same climate.
          {p.unsavedDays > 0 && <> The <b>{p.unsavedDays} day{p.unsavedDays > 1 ? 's' : ''}</b> since your last save will be lost.</>}
        </p>
        <div className="mt-4 flex justify-end gap-2">
          <button className={secondaryBtn} onClick={p.onBack}>Back</button>
          <button className={primaryBtn} onClick={p.onRestartConfirm}>Restart</button>
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
          <button className={secondaryBtn} onClick={p.onBack}>Back</button>
          <button className={secondaryBtn} onClick={p.onSave}>Save Game</button>
          <button className={primaryBtn} onClick={p.onExitConfirm}>Exit Anyway</button>
        </div>
      </MenuShell>
    );
  }

  const item = (label: string, icon: string, onClick: () => void, primary = false) => (
    <button
      onClick={onClick}
      className={`flex items-center gap-2.5 rounded px-3 py-2 text-xs font-black uppercase tracking-widest ${primary
        ? 'bg-yellow-500 text-red-950 hover:bg-yellow-400'
        : 'bg-red-900/60 text-yellow-100 hover:bg-red-800'}`}
    >
      <GameIcon name={icon} size={14} />{label}
    </button>
  );

  return (
    <MenuShell title="— Paused —" icon="pause" onBack={p.onBack} escDisabled={p.escDisabled} width="max-w-xs">
      <div className="flex flex-col gap-1.5">
        {item('Resume', 'play', p.onResume, true)}
        {item('Save Game', 'save', p.onSave)}
        {item('Load Game', 'load', p.onLoad)}
        {item('Options', 'settings', p.onOptions)}
        {item('Manual', 'help', p.onManual)}
        {item('Restart', 'restart', p.onRestartRequest)}
        {item('Exit to Main Menu', 'exit', p.onExitRequest)}
      </div>
    </MenuShell>
  );
}
