import { useEffect, useState } from 'react';
import { GameIcon } from '@/ui/GameIcon';
import { RangeSlider, ToggleButton } from '@/components/menu/controls';
import { audio, PLAYLIST } from '@/audio';
import { fmtClock } from '@/audio/format';
import { useMusicState } from '@/hooks/use-music';
import { useSettings } from '@/hooks/use-settings';
import { updateSettings } from '@/app/settings';

/**
 * The music player — a non-modal PanelMode. Now-playing readout, transport,
 * shuffle/repeat, a pickable programme list, and the music volume (the same
 * setting the Options Audio tab writes). All state comes from the audio
 * singleton via useMusicState(); this panel only issues commands.
 */
export default function MusicPanel() {
  const m = useMusicState();
  const s = useSettings();
  const cycleRepeat = () => audio.setRepeat(m.repeat === 'off' ? 'all' : m.repeat === 'all' ? 'one' : 'off');
  const repeatLabel = m.repeat === 'one' ? 'Repeat one' : m.repeat === 'all' ? 'Repeat all' : 'Repeat off';

  // The playhead ticks off the audio clock — polled here (not in MusicState, which
  // stays event-only). 4 Hz while playing for a smooth thumb glide, 1 Hz when paused
  // or under reducedMotion (the M:SS text only changes per second anyway). A no-op
  // setState (same value while paused) is cheaply skipped by React.
  const [elapsed, setElapsed] = useState(() => audio.musicProgress().elapsedS);
  useEffect(() => {
    const ms = m.playing && !s.reducedMotion ? 250 : 1000;
    const id = setInterval(() => setElapsed(audio.musicProgress().elapsedS), ms);
    return () => clearInterval(id);
  }, [m.playing, s.reducedMotion]);
  const fillPct = m.durationS > 0 ? Math.min(100, (elapsed / m.durationS) * 100) : 0;

  return (
    <div className="space-y-3">
      <div className="rounded bg-red-900/40 p-2 text-center">
        <div className="text-[0.625rem] font-black uppercase tracking-widest text-yellow-400/80">Now Broadcasting</div>
        <div className="mt-0.5 text-sm font-bold text-yellow-100">{m.trackName}</div>
        <div className="text-[0.625rem] text-yellow-200/50">
          {m.index + 1} / {m.total}{m.playing ? '' : ' · paused'}
        </div>
      </div>

      {/* scrubber — elapsed / total, drag to seek (snaps to the nearest chord block) */}
      <div className="space-y-1">
        <input
          type="range"
          className="soviet-range w-full"
          aria-label="Seek"
          min={0} max={Math.max(1, m.durationS)} step={Math.max(1, m.durationS / 200)}
          value={Math.min(elapsed, m.durationS)}
          onChange={e => { const v = Number(e.target.value); setElapsed(v); audio.seek(v); }}
          style={{ background: `linear-gradient(to right, #eab308 0 ${fillPct}%, rgba(10,5,4,0.7) ${fillPct}% 100%)` }}
        />
        <div className="flex justify-between text-[0.625rem] font-bold tabular-nums text-yellow-200/60">
          <span>{fmtClock(elapsed)}</span>
          <span>{fmtClock(m.durationS)}</span>
        </div>
      </div>

      <div className="flex items-center justify-center gap-2">
        <button onClick={() => audio.prevTrack()} aria-label="Previous track" title="Previous track"
          className="rounded bg-red-950/60 p-2 hover:bg-red-800"><GameIcon name="skipBack" size={16} /></button>
        <button onClick={() => audio.setMusicPlaying(!m.playing)}
          aria-label={m.playing ? 'Pause music' : 'Play music'} title={m.playing ? 'Pause music' : 'Play music'}
          className="rounded bg-yellow-500 p-2.5 text-red-950 hover:bg-yellow-400"><GameIcon name={m.playing ? 'pause' : 'play'} size={18} /></button>
        <button onClick={() => audio.nextTrack()} aria-label="Next track" title="Next track"
          className="rounded bg-red-950/60 p-2 hover:bg-red-800"><GameIcon name="skipForward" size={16} /></button>
      </div>

      <div className="flex items-center justify-center gap-2">
        <ToggleButton on={m.shuffle} onChange={v => audio.setShuffle(v)} icon="shuffle" label="Shuffle" title="Shuffle the programme order" />
        <button onClick={cycleRepeat} aria-pressed={m.repeat !== 'off'} data-sfx="toggle" title={repeatLabel}
          className={`flex items-center gap-1.5 rounded px-2 py-1 text-[0.6875rem] font-bold ${m.repeat !== 'off' ? 'bg-yellow-500 text-red-950' : 'border border-yellow-600/30 bg-red-950/50 text-yellow-100/70 hover:bg-red-900/70'}`}>
          <GameIcon name={m.repeat === 'one' ? 'repeatOne' : 'repeat'} size={13} />
          {m.repeat === 'one' ? 'One' : m.repeat === 'all' ? 'All' : 'Off'}
        </button>
      </div>

      <div className="space-y-1">
        <div className="text-[0.625rem] font-black uppercase tracking-wider text-yellow-400">Programme</div>
        {PLAYLIST.map(t => {
          const active = t.id === m.trackId;
          return (
            <button
              key={t.id}
              onClick={() => audio.selectTrack(t.id)}
              aria-pressed={active}
              className={`flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs font-bold ${active ? 'bg-yellow-500 text-red-950' : 'bg-red-900/40 text-yellow-100/80 hover:bg-red-800/60'}`}
            >
              <GameIcon name={active && m.playing ? 'play' : 'music'} size={12} />
              <span className="flex-1 truncate">{t.name}</span>
              <span className={`text-[0.5625rem] ${active ? 'text-red-950/70' : 'text-yellow-200/40'}`}>{t.mode}</span>
            </button>
          );
        })}
      </div>

      <div className="flex items-center justify-between gap-2 border-t border-yellow-600/20 pt-2">
        <span className="flex items-center gap-1.5 text-xs font-bold text-yellow-100"><GameIcon name="volume" size={13} /> Volume</span>
        <RangeSlider label="Music volume" min={0} max={1} step={0.05} value={s.musicVolume}
          format={v => `${Math.round(v * 100)}%`} onChange={v => updateSettings({ musicVolume: v })} />
      </div>

      <p className="text-[0.625rem] leading-snug text-yellow-200/50">
        The People's approved repertoire — six fixed works, synthesized live from a seeded score: the same performance, note for note, every broadcast.
      </p>
    </div>
  );
}
