import { useState } from 'react';
import { MenuShell } from './MenuShell';
import { RangeSlider, Segmented, SettingRow, Toggle, TwoStepButton, secondaryBtn } from './controls';
import { resetSettings, updateSettings } from '@/app/settings';
import { useSettings } from '@/hooks/use-settings';
import { GameIcon } from '@/ui/GameIcon';

type Tab = 'gameplay' | 'display' | 'audio' | 'access';

const TABS: { id: Tab; label: string; icon: string }[] = [
  { id: 'gameplay', label: 'Gameplay', icon: 'plan' },
  { id: 'display', label: 'Display', icon: 'map' },
  { id: 'audio', label: 'Audio', icon: 'pub' },
  { id: 'access', label: 'Access', icon: 'users' },
];

interface Props {
  onBack: () => void;
  escDisabled: boolean;
}

/** Every control live-applies through the settings store — no OK/Apply. */
export function OptionsScreen({ onBack, escDisabled }: Props) {
  const s = useSettings();
  const [tab, setTab] = useState<Tab>('gameplay');
  const pct = (v: number) => `${Math.round(v * 100)}%`;

  return (
    <MenuShell
      title="Options" icon="settings" onBack={onBack} escDisabled={escDisabled} width="max-w-2xl"
      footer={
        <>
          <button className={secondaryBtn} onClick={onBack}>Back</button>
          <TwoStepButton label="Reset to defaults" confirmLabel="Reset everything?" onConfirm={resetSettings} />
        </>
      }
    >
      <div className="flex gap-4">
        <nav className="flex w-28 shrink-0 flex-col gap-1" aria-label="Options sections">
          {TABS.map(t => (
            <button
              key={t.id}
              aria-pressed={tab === t.id}
              onClick={() => setTab(t.id)}
              className={`flex items-center gap-2 rounded px-2 py-1.5 text-[0.6875rem] font-black uppercase tracking-wider ${
                tab === t.id ? 'bg-yellow-500 text-red-950' : 'bg-red-950/60 text-yellow-100/70 hover:bg-red-900'}`}
            >
              <GameIcon name={t.icon} size={13} />{t.label}
            </button>
          ))}
        </nav>

        <div className="min-w-0 flex-1">
          {tab === 'gameplay' && (
            <>
              <SettingRow label="Autosave" description="Automatic save every N in-game days, rotating three slots">
                <Segmented
                  label="Autosave interval"
                  options={[{ value: 0, label: 'Off' }, { value: 10, label: '10d' }, { value: 30, label: '30d' }, { value: 90, label: '90d' }]}
                  value={s.autosaveIntervalDays}
                  onChange={v => updateSettings({ autosaveIntervalDays: v })}
                />
              </SettingRow>
              <SettingRow label="Commissar's briefing" description="Show the welcome briefing when founding a republic">
                <Toggle label="Commissar's briefing" checked={s.showBriefing} onChange={v => updateSettings({ showBriefing: v })} />
              </SettingRow>
              <SettingRow label="Camera pan speed" description="WASD and edge panning speed">
                <RangeSlider label="Camera pan speed" min={0.5} max={2} step={0.05} value={s.panSpeed} format={pct} onChange={v => updateSettings({ panSpeed: v })} />
              </SettingRow>
              <SettingRow label="Edge panning" description="Pan when the mouse touches the viewport edge">
                <Toggle label="Edge panning" checked={s.edgePan} onChange={v => updateSettings({ edgePan: v })} />
              </SettingRow>
              <SettingRow label="Invert zoom" description="Flip the mouse-wheel zoom direction">
                <Toggle label="Invert zoom" checked={s.invertZoom} onChange={v => updateSettings({ invertZoom: v })} />
              </SettingRow>
              <SettingRow label="Notification time" description="How long toasts stay on screen">
                <RangeSlider label="Notification time" min={2} max={10} step={0.5} value={s.toastSeconds} format={v => `${v.toFixed(1)}s`} onChange={v => updateSettings({ toastSeconds: v })} />
              </SettingRow>
            </>
          )}

          {tab === 'display' && (
            <>
              <SettingRow label="Interface scale" description="Size of the HUD and panels (the map is unaffected)">
                <RangeSlider label="Interface scale" min={0.85} max={1.3} step={0.05} value={s.uiScale} format={pct} onChange={v => updateSettings({ uiScale: v })} />
              </SettingRow>
              <SettingRow label="Render sharpness" description="Pixel-density cap — lower is faster on 4K displays">
                <Segmented
                  label="Render sharpness"
                  options={[{ value: 1, label: 'Eco' }, { value: 1.5, label: 'Balanced' }, { value: 2, label: 'Sharp' }]}
                  value={s.dprCap}
                  onChange={v => updateSettings({ dprCap: v })}
                />
              </SettingRow>
              <SettingRow label="Tile grid" description="Overlay the tile lattice on terrain">
                <Toggle label="Tile grid" checked={s.showGrid} onChange={v => updateSettings({ showGrid: v })} />
              </SettingRow>
            </>
          )}

          {tab === 'audio' && (
            <>
              <SettingRow label="Master mute" description="Silence everything">
                <Toggle label="Master mute" checked={s.muted} onChange={v => updateSettings({ muted: v })} />
              </SettingRow>
              <SettingRow label="Music volume" description="The State Radio Orchestra's ambient programme">
                <RangeSlider label="Music volume" min={0} max={1} step={0.05} value={s.musicVolume} format={pct} onChange={v => updateSettings({ musicVolume: v })} />
              </SettingRow>
              <SettingRow label="Effects volume" description="Construction, trade and world sounds">
                <RangeSlider label="Effects volume" min={0} max={1} step={0.05} value={s.sfxVolume} format={pct} onChange={v => updateSettings({ sfxVolume: v })} />
              </SettingRow>
              <SettingRow label="Interface volume" description="Menus, clicks, toggles and tabs">
                <RangeSlider label="Interface volume" min={0} max={1} step={0.05} value={s.interfaceVolume} format={pct} onChange={v => updateSettings({ interfaceVolume: v })} />
              </SettingRow>
              <SettingRow label="Hover ticks" description="A whisper-quiet tick when the pointer crosses a control">
                <Toggle label="Hover ticks" checked={s.hoverSounds} onChange={v => updateSettings({ hoverSounds: v })} />
              </SettingRow>
              <SettingRow label="Mute when hidden" description="Silence the game while the tab is in the background">
                <Toggle label="Mute when hidden" checked={s.muteWhenHidden} onChange={v => updateSettings({ muteWhenHidden: v })} />
              </SettingRow>
            </>
          )}

          {tab === 'access' && (
            <>
              <SettingRow label="Colorblind palette" description="Blue/orange placement and status colors with shape cues">
                <Toggle label="Colorblind palette" checked={s.colorblind} onChange={v => updateSettings({ colorblind: v })} />
              </SettingRow>
              <SettingRow label="Reduced motion" description="No decorative animation — weather particles, water shimmer, menu drift">
                <Toggle label="Reduced motion" checked={s.reducedMotion} onChange={v => updateSettings({ reducedMotion: v })} />
              </SettingRow>
            </>
          )}
        </div>
      </div>
    </MenuShell>
  );
}
