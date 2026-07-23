import { useCallback, useEffect, useReducer, useRef, useState } from 'react';
import GameCanvas, { type BuildPolicy, type SelectionItem, type Tool } from './components/GameCanvas';
import { updateSelection } from './game/selection';
import HUD, { type PanelMode } from './components/HUD';
import BottomBar from './components/BottomBar';
import SidePanel from './components/SidePanel';
import { IntroOverlay, HelpOverlay, ToastStack } from './components/Overlays';
import { MainMenu } from './components/menu/MainMenu';
import { MenuBackdrop } from './components/menu/MenuBackdrop';
import { NewGameScreen } from './components/menu/NewGameScreen';
import { OptionsScreen } from './components/menu/OptionsScreen';
import { PauseMenu } from './components/menu/PauseMenu';
import { SaveLoadScreen } from './components/menu/SaveLoadScreen';
import { useToasts } from './hooks/use-toasts';
import { useAutosave } from './hooks/use-autosave';
import { MENU_ROOT, PLAYING, screenReducer } from './app/screens';
import { bootFromUrl, createSession, sessionFromSave } from './app/session';
import type { GameSession, NewGameConfig } from './app/session';
import { getSettings, subscribeSettings } from './app/settings';
import { SaveError } from './game/save-format';
import type { SaveGameV1 } from './game/save-format';
import { QUICKSAVE_SLOT, readSlot, writeSlot } from './game/save-slots';
import { onStorageFlushError } from './platform/storage';
import { isTauri, onUpdateAvailable, quitApp, setCloseRequestHandler } from './platform/desktop';
import type { PendingUpdate } from './platform/desktop';
import { UpdateBanner } from './components/UpdateBanner';
import { audio, installUiSounds } from './audio';

export default function App() {
  // ?demo / ?seed=N boot straight into gameplay; otherwise start at the menu.
  // The initializer must stay side-effect-free re: globals (StrictMode runs it twice).
  const [session, setSession] = useState<GameSession | null>(bootFromUrl);
  const [screen, dispatch] = useReducer(screenReducer, undefined, () => (session ? PLAYING : MENU_ROOT));
  const [tool, setTool] = useState<Tool>({ kind: 'select' });
  const [selection, setSelection] = useState<SelectionItem[]>([]);
  // placement defaults stamped onto each new site (foreign-labor default is engine state)
  const [policy, setPolicyState] = useState<BuildPolicy>({ autoBuy: false, currency: 'east', instant: false, plan: false });
  // instant ($) completes now, so it's exclusive with the deferred/paid modes;
  // auto-buy is exclusive with instant; plan may combine with auto-buy.
  const setPolicy = useCallback((patch: Partial<BuildPolicy>) => {
    setPolicyState(p => {
      const next = { ...p, ...patch };
      if (patch.instant) { next.autoBuy = false; next.plan = false; }
      if (patch.autoBuy) next.instant = false;
      if (patch.plan) next.instant = false;
      return next;
    });
  }, []);
  const [panel, setPanel] = useState<PanelMode | null>(null);
  const [briefingVisible, setBriefingVisible] = useState(false);
  const [showHelp, setShowHelp] = useState(false);
  const [pendingUpdate, setPendingUpdate] = useState<PendingUpdate | null>(null);
  const { toasts, push, clear } = useToasts();
  const nextSessionId = useRef((session?.id ?? 0) + 1);
  const speedBeforePause = useRef<0 | 1 | 2 | 4>(1);
  // last save of the CURRENT session (any kind) — drives "unsaved progress" confirms
  const [lastSave, setLastSave] = useState<{ sessionId: number; dayIndex: number } | null>(
    () => (session ? { sessionId: session.id, dayIndex: session.engine.dayIndex() } : null),
  );

  // interface scale: rem-based Tailwind means the whole DOM HUD follows the root font-size
  useEffect(() => {
    const apply = () => { document.documentElement.style.fontSize = `${getSettings().uiScale * 100}%`; };
    apply();
    return subscribeSettings(apply);
  }, []);

  // desktop: async disk writes fail out-of-band — surface them, never swallow
  useEffect(() => onStorageFlushError(f => push(`Could not write to disk: ${f.message}`, 'bad')), [push]);

  // audio: the context can only start from a user gesture (autoplay policy)
  useEffect(() => {
    const unlock = () => audio.unlock();
    window.addEventListener('pointerdown', unlock, { capture: true, once: true });
    window.addEventListener('keydown', unlock, { capture: true, once: true });
    return () => {
      window.removeEventListener('pointerdown', unlock, { capture: true });
      window.removeEventListener('keydown', unlock, { capture: true });
    };
  }, []);
  // one delegated capture-phase listener voices every button click + hover;
  // the single owner of press-driven UI sound (keyboard nav calls uiSound()).
  useEffect(() => installUiSounds(), []);
  useEffect(() => {
    audio.setScene(screen.phase === 'menu' ? 'menu' : 'game');
  }, [screen.phase]);
  useEffect(() => {
    if (!session) { audio.setEngineProbe(null); return; }
    const engine = session.engine;
    audio.setEngineProbe(() => ({
      season: engine.season(),
      tempC: engine.weather.tempC,
      condition: engine.weather.condition,
    }));
    return () => audio.setEngineProbe(null);
  }, [session]);

  // drain engine events into toasts whenever the engine changes. App itself
  // holds no global engine subscription — HUD/BuildMenu/SidePanel subscribe
  // to their own slices via the use-engine hooks.
  useEffect(() => {
    if (!session) return;
    const engine = session.engine;
    // ONE drain, fanned out — drainEvents() is destructive, single-consumer
    const drain = () => {
      for (const e of engine.drainEvents()) {
        push(e.text, e.kind, e.icon);
        audio.onGameEvent(e);
      }
    };
    drain();
    return engine.subscribe(drain);
  }, [session, push]);

  const markSaved = useCallback(() => {
    if (!session) return;
    setLastSave({ sessionId: session.id, dayIndex: session.engine.dayIndex() });
  }, [session]);

  useAutosave(session, markSaved, push);

  const unsavedDays = session
    ? session.engine.dayIndex() - (lastSave?.sessionId === session.id ? lastSave.dayIndex : 0)
    : 0;

  // ---------- session lifecycle ----------

  /** Common swap: fresh world, fresh UI state, keyed GameCanvas remount. */
  const startGame = useCallback((s: GameSession) => {
    setSession(s);
    setPolicyState(p => ({ ...p, currency: s.engine.foreignLaborCurrency }));
    setTool({ kind: 'select' });
    setSelection([]);
    setPanel(null);
    setBriefingVisible(false);
    clear();
    setLastSave({ sessionId: s.id, dayIndex: s.engine.dayIndex() });
    dispatch({ type: 'START_GAME' });
  }, [clear]);

  const newGame = useCallback((cfg: NewGameConfig) => {
    const s = createSession(cfg, nextSessionId.current++);
    if (getSettings().showBriefing) {
      s.engine.setSpeed(0);
      startGame(s);
      setBriefingVisible(true);
    } else {
      startGame(s);
    }
  }, [startGame]);

  const loadGame = useCallback((save: SaveGameV1) => {
    try {
      const s = sessionFromSave(save, nextSessionId.current++); // arrives paused
      startGame(s);
      push('Game loaded — press Space to resume', 'info', 'save');
    } catch (e) {
      push(e instanceof SaveError ? e.message : 'Could not load the save', 'bad');
    }
  }, [startGame, push]);

  const restart = useCallback(() => {
    if (!session) return;
    const s = createSession(session.config, nextSessionId.current++);
    s.isNew = false;
    startGame(s);
  }, [session, startGame]);

  const exitToMenu = useCallback(() => {
    setSession(null);
    setSelection([]);
    setPanel(null);
    setBriefingVisible(false);
    clear();
    dispatch({ type: 'EXIT_TO_MENU' });
  }, [clear]);

  // ---------- pause bookkeeping ----------

  const openPause = useCallback(() => {
    if (!session) return;
    speedBeforePause.current = session.engine.speed;
    session.engine.setSpeed(0);
    dispatch({ type: 'OPEN_PAUSE' });
  }, [session]);

  /** RESUME must restore 0 for a manually-paused game — hence no togglePause(). */
  const resume = useCallback(() => {
    session?.engine.setSpeed(speedBeforePause.current);
    dispatch({ type: 'RESUME' });
  }, [session]);

  /** The one Escape/back semantic: closing the pause root resumes time. */
  const back = useCallback(() => {
    if (screen.phase === 'playing' && screen.overlay === 'root') resume();
    else dispatch({ type: 'BACK' });
  }, [screen, resume]);

  // desktop: titlebar X / Alt+F4 → confirm when progress is unsaved. Days are
  // computed from the engine AT CALL TIME — App renders lazily, so captured
  // values could be a day stale.
  useEffect(() => setCloseRequestHandler(() => {
    const days = session
      ? session.engine.dayIndex() - (lastSave?.sessionId === session.id ? lastSave.dayIndex : 0)
      : 0;
    if (!session || days <= 0) return 'quit';
    if (screen.phase === 'playing') {
      if (screen.overlay === null) openPause(); // pauses the sim, overlay → 'root'
      dispatch({ type: 'PAUSE_GOTO', sub: 'confirm-quit' });
    }
    return 'confirm';
  }), [session, lastSave, screen, openPause]);

  // desktop: update prompt (no-op subscription in the browser)
  useEffect(() => onUpdateAvailable(setPendingUpdate), []);

  // ---------- quicksave / quickload ----------

  const quickSave = useCallback(() => {
    if (!session) return;
    const res = writeSlot(QUICKSAVE_SLOT, session.engine.serialize());
    if (res.ok) {
      markSaved();
      audio.sfx('quicksave');
      push('Quicksaved', 'good', 'save');
    } else {
      push(res.message, 'bad');
    }
  }, [session, markSaved, push]);

  const quickLoad = useCallback(() => {
    try {
      loadGame(readSlot(QUICKSAVE_SLOT));
    } catch (e) {
      push(e instanceof SaveError && e.code === 'missing' ? 'No quicksave yet — F5 makes one' : 'Could not quickload', 'bad');
    }
  }, [loadGame, push]);

  useEffect(() => {
    if (!session || screen.phase !== 'playing') return;
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)) return;
      if (e.key === 'F5') { e.preventDefault(); quickSave(); }
      else if (e.key === 'F9') { e.preventDefault(); quickLoad(); }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [session, screen.phase, quickSave, quickLoad]);

  // ---------- selection / panels ----------

  const handleSelect = (item: SelectionItem | null, additive: boolean) => {
    setSelection(cur => {
      const next = updateSelection(cur, item, additive);
      if (next.length) setPanel('building');
      else setPanel(p => (p === 'building' ? null : p));
      return next;
    });
  };

  // the HUD panel buttons (data-sfx="panel") voice open/close via the click
  // classifier — no explicit sound here keeps one owner per press.
  const togglePanel = (m: PanelMode) => {
    setPanel(p => (p === m ? null : m));
  };

  /** Wrap setTool so arming/dropping a build tool plays a world cue — the one
   *  owner of tool sound, covering menu clicks AND the Escape-cancel path. */
  const setToolSfx = useCallback((next: Tool) => {
    setTool(prev => {
      const key = (t: Tool) => (t.kind === 'build' ? `build:${t.defId}` : t.kind);
      if (key(next) !== key(prev)) audio.ui(next.kind === 'select' ? 'toolCancel' : 'toolArm');
      return next;
    });
  }, []);

  const hotkeysEnabled =
    screen.phase === 'playing' && screen.overlay === null && !showHelp && !briefingVisible;
  const canQuit = isTauri(); // desktop: main-menu Exit Game + pause Exit-to-Desktop; web has neither

  const saveLoadShared = {
    unsavedDays,
    onLoad: loadGame,
    onSaved: markSaved,
    notify: push,
    escDisabled: showHelp,
  };

  return (
    <div className="relative h-screen w-screen overflow-hidden bg-[#1a2028] select-none">
      {session && (
        <GameCanvas
          key={session.id}
          engine={session.engine}
          tool={tool}
          setTool={setToolSfx}
          selection={selection}
          onSelect={handleSelect}
          policy={policy}
          hotkeysEnabled={hotkeysEnabled}
          onError={(msg) => push(msg, 'bad')}
          onOpenMenu={openPause}
        />
      )}

      {session && screen.phase === 'playing' && (
        <>
          <HUD
            engine={session.engine}
            activePanel={panel}
            helpOpen={showHelp}
            onOpenStockpiles={() => togglePanel('stockpiles')}
            onOpenLogistics={() => togglePanel('logistics')}
            onOpenObjectives={() => togglePanel('objectives')}
            onOpenTrade={() => togglePanel('trade')}
            onOpenMusic={() => togglePanel('music')}
            onOpenHelp={() => setShowHelp(true)}
            onOpenMenu={openPause}
          />
          <BottomBar
            engine={session.engine}
            tool={tool}
            setTool={setToolSfx}
            policy={policy}
            setPolicy={setPolicy}
            push={push}
          />
          {panel && (
            <SidePanel
              engine={session.engine}
              mode={panel}
              selection={selection}
              policy={policy}
              onClose={() => setPanel(null)}
              onOpenTrade={() => setPanel('trade')}
              onArmBuild={(defId) => setToolSfx({ kind: 'build', defId })}
              notify={push}
            />
          )}
        </>
      )}

      {screen.phase === 'menu' && (
        <>
          <MenuBackdrop />
          {screen.sub === 'root' && (
            <MainMenu
              onContinue={(latest) => {
                try {
                  loadGame(readSlot(latest.slotId));
                } catch {
                  push('Could not load the latest save', 'bad');
                }
              }}
              onNewGame={() => dispatch({ type: 'MENU_GOTO', sub: 'new-game' })}
              onLoad={() => dispatch({ type: 'MENU_GOTO', sub: 'load' })}
              onOptions={() => dispatch({ type: 'MENU_GOTO', sub: 'options' })}
              onManual={() => setShowHelp(true)}
              onExit={canQuit ? () => void quitApp() : undefined}
            />
          )}
          {screen.sub === 'new-game' && (
            <NewGameScreen onBack={back} onStart={newGame} escDisabled={showHelp} />
          )}
          {screen.sub === 'load' && (
            <SaveLoadScreen mode="load" engine={null} onBack={back} {...saveLoadShared} />
          )}
          {screen.sub === 'options' && <OptionsScreen onBack={back} escDisabled={showHelp} />}
        </>
      )}

      {session && screen.phase === 'playing' && screen.overlay !== null && (
        screen.overlay === 'save' ? (
          <SaveLoadScreen mode="save" engine={session.engine} onBack={back} {...saveLoadShared} />
        ) : screen.overlay === 'load' ? (
          <SaveLoadScreen mode="load" engine={session.engine} onBack={back} {...saveLoadShared} />
        ) : screen.overlay === 'options' ? (
          <OptionsScreen onBack={back} escDisabled={showHelp} />
        ) : (
          <PauseMenu
            overlay={screen.overlay}
            republicName={session.engine.name}
            unsavedDays={unsavedDays}
            escDisabled={showHelp}
            onResume={resume}
            onSave={() => dispatch({ type: 'PAUSE_GOTO', sub: 'save' })}
            onLoad={() => dispatch({ type: 'PAUSE_GOTO', sub: 'load' })}
            onOptions={() => dispatch({ type: 'PAUSE_GOTO', sub: 'options' })}
            onManual={() => setShowHelp(true)}
            onRestartRequest={() => dispatch({ type: 'PAUSE_GOTO', sub: 'confirm-restart' })}
            onRestartConfirm={restart}
            canQuit={canQuit}
            onExitChooser={() => dispatch({ type: 'PAUSE_GOTO', sub: 'exit' })}
            onExitRequest={() => {
              if (unsavedDays > 0) dispatch({ type: 'PAUSE_GOTO', sub: 'confirm-exit' });
              else exitToMenu();
            }}
            onExitConfirm={exitToMenu}
            onQuitRequest={() => {
              if (unsavedDays > 0) dispatch({ type: 'PAUSE_GOTO', sub: 'confirm-quit' });
              else void quitApp();
            }}
            onQuitConfirm={() => void quitApp()}
            onBack={back}
          />
        )
      )}

      <ToastStack toasts={toasts} />
      {pendingUpdate && (
        <UpdateBanner
          version={pendingUpdate.version}
          install={pendingUpdate.install}
          onDismiss={() => setPendingUpdate(null)}
          notify={push}
        />
      )}
      {briefingVisible && session && (
        <IntroOverlay onStart={() => { setBriefingVisible(false); session.engine.setSpeed(1); }} />
      )}
      {showHelp && <HelpOverlay onClose={() => setShowHelp(false)} />}
    </div>
  );
}
