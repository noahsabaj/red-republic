import { useCallback, useEffect, useReducer, useRef, useState } from 'react';
import GameCanvas, { type SelectionItem, type Tool } from './components/GameCanvas';
import { updateSelection } from './game/selection';
import HUD, { type PanelMode } from './components/HUD';
import BuildMenu from './components/BuildMenu';
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
import { audio } from './audio';

export default function App() {
  // ?demo / ?seed=N boot straight into gameplay; otherwise start at the menu.
  // The initializer must stay side-effect-free re: globals (StrictMode runs it twice).
  const [session, setSession] = useState<GameSession | null>(bootFromUrl);
  const [screen, dispatch] = useReducer(screenReducer, undefined, () => (session ? PLAYING : MENU_ROOT));
  const [tool, setTool] = useState<Tool>({ kind: 'select' });
  const [selection, setSelection] = useState<SelectionItem[]>([]);
  const [instantBuild, setInstantBuild] = useState(false);
  const [panel, setPanel] = useState<PanelMode | null>(null);
  const [briefingVisible, setBriefingVisible] = useState(false);
  const [showHelp, setShowHelp] = useState(false);
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
    audio.sfx('panelOpen');
    dispatch({ type: 'OPEN_PAUSE' });
  }, [session]);

  /** RESUME must restore 0 for a manually-paused game — hence no togglePause(). */
  const resume = useCallback(() => {
    session?.engine.setSpeed(speedBeforePause.current);
    audio.sfx('panelClose');
    dispatch({ type: 'RESUME' });
  }, [session]);

  /** The one Escape/back semantic: closing the pause root resumes time. */
  const back = useCallback(() => {
    if (screen.phase === 'playing' && screen.overlay === 'root') resume();
    else dispatch({ type: 'BACK' });
  }, [screen, resume]);

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

  const togglePanel = (m: PanelMode) => {
    audio.sfx(panel === m ? 'panelClose' : 'panelOpen'); // side effects stay out of the updater
    setPanel(p => (p === m ? null : m));
  };

  const hotkeysEnabled =
    screen.phase === 'playing' && screen.overlay === null && !showHelp && !briefingVisible;

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
          setTool={setTool}
          selection={selection}
          onSelect={handleSelect}
          instantBuild={instantBuild}
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
            onOpenObjectives={() => togglePanel('objectives')}
            onOpenTrade={() => togglePanel('trade')}
            onOpenHelp={() => setShowHelp(true)}
            onOpenMenu={openPause}
          />
          <BuildMenu
            engine={session.engine}
            tool={tool}
            setTool={setTool}
            instantBuild={instantBuild}
            setInstantBuild={setInstantBuild}
          />
          {panel && (
            <SidePanel
              engine={session.engine}
              mode={panel}
              selection={selection}
              instantBuild={instantBuild}
              onClose={() => setPanel(null)}
              onOpenTrade={() => setPanel('trade')}
              onArmBuild={(defId) => setTool({ kind: 'build', defId })}
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
            onExitRequest={() => {
              if (unsavedDays > 0) dispatch({ type: 'PAUSE_GOTO', sub: 'confirm-exit' });
              else exitToMenu();
            }}
            onExitConfirm={exitToMenu}
            onBack={back}
          />
        )
      )}

      <ToastStack toasts={toasts} />
      {briefingVisible && session && (
        <IntroOverlay onStart={() => { setBriefingVisible(false); session.engine.setSpeed(1); }} />
      )}
      {showHelp && <HelpOverlay onClose={() => setShowHelp(false)} />}
    </div>
  );
}
