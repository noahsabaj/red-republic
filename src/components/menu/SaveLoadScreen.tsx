import { useCallback, useRef, useState } from 'react';
import { MenuShell } from './MenuShell';
import { TwoStepButton, primaryBtn, rowBtnMuted, rowBtnPrimary, secondaryBtn } from './controls';
import { BALANCE, CLIMATES } from '@/game/config';
import type { GameEngine } from '@/game/engine';
import { SaveError } from '@/game/save-format';
import type { SaveGameV1 } from '@/game/save-format';
import {
  deleteSlot, exportSave, importSave, importSaveViaDialog, listSlots, newSlotId, readSlot, writeSlot,
} from '@/game/save-slots';
import { isTauri } from '@/platform/storage';
import type { SlotMeta } from '@/game/save-slots';
import { GameIcon } from '@/ui/GameIcon';

interface Props {
  mode: 'save' | 'load';
  /** null on the main menu (no live game). */
  engine: GameEngine | null;
  /** in-game days since the last save of this session; 0 when nothing to lose. */
  unsavedDays: number;
  onBack: () => void;
  onLoad: (save: SaveGameV1) => void;
  onSaved: () => void; // caller updates its unsaved-progress baseline
  notify: (text: string, kind?: 'good' | 'bad' | 'info', icon?: string) => void;
  escDisabled: boolean;
}

const KIND_ORDER = { quick: 0, auto: 1, manual: 2 } as const;

export function SaveLoadScreen({ mode, engine, unsavedDays, onBack, onLoad, onSaved, notify, escDisabled }: Props) {
  const [refresh, setRefresh] = useState(0);
  const [newName, setNewName] = useState('');
  const fileRef = useRef<HTMLInputElement>(null);

  // manual slots by recency; quick/auto pinned on top in load mode
  const slots = listSlots().sort((a, b) =>
    KIND_ORDER[a.kind] - KIND_ORDER[b.kind] || b.header.savedAt - a.header.savedAt);
  const visible = mode === 'save' ? slots.filter(s => s.kind === 'manual') : slots;
  void refresh;

  const defaultLabel = engine
    ? `${engine.name} — ${BALANCE.months[engine.month - 1]} ${engine.year}`
    : '';

  const doSave = useCallback((slotId: string, label: string) => {
    if (!engine) return;
    const blob = engine.serialize();
    blob.header.label = label;
    const res = writeSlot(slotId, blob);
    if (res.ok) {
      onSaved();
      notify(`Saved “${label}”`, 'good', 'save');
      setNewName('');
      setRefresh(n => n + 1);
    } else {
      notify(res.message, 'bad');
    }
  }, [engine, onSaved, notify]);

  const doLoad = useCallback((slotId: string) => {
    try {
      onLoad(readSlot(slotId));
    } catch (e) {
      notify(e instanceof SaveError ? e.message : 'Could not load this save', 'bad');
    }
  }, [onLoad, notify]);

  const doExport = useCallback(async (slotId: string) => {
    try {
      if (await exportSave(readSlot(slotId))) notify('Save exported', 'info', 'download');
    } catch (e) {
      notify(e instanceof SaveError ? e.message : 'Export failed', 'bad');
    }
  }, [notify]);

  const addImported = useCallback((save: SaveGameV1) => {
    const res = writeSlot(newSlotId(), save);
    if (res.ok) {
      notify(`Imported “${save.header.label ?? save.header.name}”`, 'good', 'upload');
      setRefresh(n => n + 1);
    } else {
      notify(res.message, 'bad');
    }
  }, [notify]);

  const doImport = useCallback(async (file: File) => {
    try {
      addImported(await importSave(file));
    } catch (e) {
      notify(e instanceof SaveError ? e.message : 'Not a valid save file', 'bad');
    }
  }, [addImported, notify]);

  const doImportDesktop = useCallback(async () => {
    try {
      const save = await importSaveViaDialog();
      if (save) addImported(save); // null = user cancelled, no toast
    } catch (e) {
      notify(e instanceof SaveError ? e.message : 'Not a valid save file', 'bad');
    }
  }, [addImported, notify]);

  const meta = (m: SlotMeta) => {
    const h = m.header;
    return `${h.name} · ${BALANCE.months[h.month - 1]} ${h.year} · pop ${h.pop} · ${h.mapW}×${h.mapH} · ${CLIMATES[h.climate].label}`;
  };

  const row = (m: SlotMeta) => (
    <div key={m.slotId} className="rounded border border-yellow-600/20 bg-red-900/30 p-2">
      <div className="flex items-center justify-between gap-2">
        <div className="min-w-0">
          <div className="flex items-center gap-1.5 text-xs font-bold text-yellow-100 truncate">
            {m.kind === 'quick' && <span className="rounded bg-amber-500/90 px-1 text-[0.5625rem] font-black text-amber-950">QUICK</span>}
            {m.kind === 'auto' && <span className="rounded bg-amber-500/90 px-1 text-[0.5625rem] font-black text-amber-950">AUTO</span>}
            <span className="truncate">{m.header.label ?? `${m.header.name} — ${BALANCE.months[m.header.month - 1]} ${m.header.year}`}</span>
          </div>
          <div className="text-[0.625rem] text-yellow-200/60 truncate">{meta(m)}</div>
          <div className="text-[0.625rem] text-yellow-200/40">
            {new Date(m.header.savedAt).toLocaleString()} · {(m.sizeBytes / 1024).toFixed(0)} KB
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {mode === 'load' && (
            engine && unsavedDays > 0 ? (
              <TwoStepButton
                label="Load" confirmLabel={`Lose ${unsavedDays}d?`}
                className={rowBtnPrimary}
                onConfirm={() => doLoad(m.slotId)}
              />
            ) : (
              <button onClick={() => doLoad(m.slotId)} data-sfx="confirm" className={rowBtnPrimary}>
                Load
              </button>
            )
          )}
          {mode === 'save' && m.kind === 'manual' && (
            <TwoStepButton
              label="Overwrite" confirmLabel="Overwrite?"
              onConfirm={() => doSave(m.slotId, m.header.label ?? defaultLabel)}
            />
          )}
          <button
            onClick={() => void doExport(m.slotId)}
            aria-label="Export this save" title="Export as .json"
            className={rowBtnMuted}
          >
            <GameIcon name="download" size={12} />
          </button>
          <TwoStepButton
            label={<GameIcon name="trash" size={12} />} confirmLabel="Delete?"
            title="Delete this save"
            onConfirm={() => { deleteSlot(m.slotId); notify('Save deleted', 'info', 'trash'); setRefresh(n => n + 1); }}
          />
        </div>
      </div>
    </div>
  );

  return (
    <MenuShell
      title={mode === 'save' ? 'Save Game' : 'Load Game'}
      icon={mode === 'save' ? 'save' : 'load'}
      onBack={onBack}
      escDisabled={escDisabled}
      width="max-w-2xl"
      footer={
        <>
          <button className={secondaryBtn} data-sfx="back" onClick={onBack}>Back</button>
          <button
            className="flex items-center gap-1.5 rounded bg-red-900/70 px-3 py-1.5 text-[0.6875rem] font-bold text-yellow-100 hover:bg-red-800"
            onClick={() => { if (isTauri()) void doImportDesktop(); else fileRef.current?.click(); }}
          >
            <GameIcon name="upload" size={12} /> Import .json
          </button>
          <input
            ref={fileRef} type="file" accept="application/json,.json" className="hidden"
            onChange={e => {
              const f = e.target.files?.[0];
              if (f) void doImport(f);
              e.target.value = '';
            }}
          />
        </>
      }
    >
      {mode === 'save' && engine && (
        <div className="mb-3 rounded border border-yellow-500/50 bg-yellow-500/10 p-2">
          <div className="text-[0.625rem] font-black uppercase tracking-wider text-yellow-400 mb-1">New save</div>
          <div className="flex gap-1.5">
            <input
              value={newName}
              placeholder={defaultLabel}
              maxLength={40}
              onChange={e => setNewName(e.target.value)}
              className="flex-1 min-w-0 rounded border border-yellow-600/40 bg-red-950/60 px-2 py-1.5 text-xs text-yellow-50 outline-none focus:border-yellow-500"
              aria-label="Save name"
            />
            <button className={primaryBtn} data-sfx="confirm" onClick={() => doSave(newSlotId(), newName.trim() || defaultLabel)}>
              Save
            </button>
          </div>
        </div>
      )}

      {visible.length === 0 ? (
        <div className="py-8 text-center text-xs text-yellow-200/50">
          No saves yet{mode === 'load' ? ' — found a republic first, comrade.' : '.'}
        </div>
      ) : (
        <div className="space-y-1.5">{visible.map(row)}</div>
      )}
    </MenuShell>
  );
}
