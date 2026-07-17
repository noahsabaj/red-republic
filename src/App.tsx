import { useEffect, useMemo, useState, useSyncExternalStore } from 'react';
import { GameEngine } from './game/engine';
import { seedDemoTown } from './game/demo';
import GameCanvas, { type Tool } from './components/GameCanvas';
import HUD from './components/HUD';
import BuildMenu from './components/BuildMenu';
import SidePanel from './components/SidePanel';
import { IntroOverlay, HelpOverlay, ToastStack } from './components/Overlays';
import { useToasts } from './hooks/use-toasts';

type PanelMode = 'building' | 'trade' | 'objectives';

export default function App() {
  const engine = useMemo(() => {
    const params = new URLSearchParams(window.location.search);
    const demo = params.has('demo');
    const seedParam = params.get('seed');
    const eng = new GameEngine({
      // ?demo pins the classic map; ?seed=N reproduces a specific run
      seed: demo ? 1961 : seedParam !== null ? Number(seedParam) >>> 0 : undefined,
    });
    if (demo) seedDemoTown(eng);
    return eng;
  }, []);
  useSyncExternalStore(
    (cb) => engine.subscribe(cb),
    () => engine.getVersion(),
  );

  const [tool, setTool] = useState<Tool>({ kind: 'select' });
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [instantBuild, setInstantBuild] = useState(false);
  const [panel, setPanel] = useState<PanelMode | null>(null);
  const [showIntro, setShowIntro] = useState(true);
  const [showHelp, setShowHelp] = useState(false);
  const { toasts, push } = useToasts();

  // pause while intro is up
  useEffect(() => { engine.setSpeed(0); }, [engine]);

  // drain engine events into toasts on every state bump
  useEffect(() => {
    for (const e of engine.drainEvents()) push(e.text, e.kind);
  });

  const handleSelect = (id: number | null) => {
    setSelectedId(id);
    if (id) setPanel('building');
    else setPanel(p => (p === 'building' ? null : p));
  };

  const togglePanel = (m: PanelMode) => setPanel(p => (p === m ? null : m));

  return (
    <div className="relative h-screen w-screen overflow-hidden bg-[#1a2028] select-none">
      <GameCanvas
        engine={engine}
        tool={tool}
        setTool={setTool}
        selectedId={selectedId}
        setSelectedId={handleSelect}
        instantBuild={instantBuild}
        hotkeysEnabled={!showIntro && !showHelp}
        onError={(msg) => push(msg, 'bad')}
      />

      <HUD
        engine={engine}
        onOpenObjectives={() => togglePanel('objectives')}
        onOpenTrade={() => togglePanel('trade')}
        onOpenHelp={() => setShowHelp(true)}
      />

      <BuildMenu
        engine={engine}
        tool={tool}
        setTool={setTool}
        instantBuild={instantBuild}
        setInstantBuild={setInstantBuild}
      />

      {panel && (
        <SidePanel
          engine={engine}
          mode={panel}
          selectedId={selectedId}
          onClose={() => setPanel(null)}
          onOpenTrade={() => setPanel('trade')}
          notify={push}
        />
      )}

      <ToastStack toasts={toasts} />
      {showIntro && (
        <IntroOverlay onStart={() => { setShowIntro(false); engine.setSpeed(1); }} />
      )}
      {showHelp && <HelpOverlay onClose={() => setShowHelp(false)} />}
    </div>
  );
}
