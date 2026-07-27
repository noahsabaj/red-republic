import { useState } from 'react';
import type { KeyboardEvent } from 'react';
import { MenuShell } from './MenuShell';
import { MapPreview } from './MapPreview';
import { OptionCardGroup, primaryBtn, secondaryBtn } from './controls';
import { CLIMATES, DIFFICULTIES, MAP_SIZES } from '@/game/config';
import type { ClimateId, DifficultyId, MapSizeId } from '@/game/config';
import { MAX_SEED, parseSeed, randomRepublicName, randomSeed } from '@/app/session';
import type { NewGameConfig } from '@/app/session';
import { fmtMoney } from '@/game/format';
import { GameIcon } from '@/ui/GameIcon';

interface Props {
  onBack: () => void;
  onStart: (cfg: NewGameConfig) => void;
  escDisabled: boolean;
}

export function NewGameScreen({ onBack, onStart, escDisabled }: Props) {
  const [name, setName] = useState(randomRepublicName);
  // The seed field holds TEXT, not the number: a number state cannot represent
  // an empty field, which is what made the old one impossible to clear.
  const [seedText, setSeedText] = useState(() => String(randomSeed()));
  const [mapSize, setMapSize] = useState<MapSizeId>('medium');
  const [climate, setClimate] = useState<ClimateId>('plains');
  const [difficulty, setDifficulty] = useState<DifficultyId>('normal');
  const seed = parseSeed(seedText);

  const label = (text: string) => (
    <div className="text-[0.625rem] font-black uppercase tracking-wider text-yellow-400 mb-1">{text}</div>
  );
  const diceBtn = (onClick: () => void, what: string) => (
    <button type="button" onClick={onClick} aria-label={`Randomize ${what}`} title={`Randomize ${what}`}
      className="rounded bg-red-900/70 px-2 py-1.5 text-yellow-200 hover:bg-red-800">
      <GameIcon name="dice" size={14} />
    </button>
  );
  // Escape inside a field belongs to the field (dismiss the IME/autocomplete,
  // let go of it) — not to MenuShell's window-level handler, which would throw
  // away every choice on this screen. Blur, swallow, and a second Escape backs
  // out as usual.
  const fieldEscape = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key !== 'Escape') return;
    e.stopPropagation();
    // Hand focus back to the dialog container (MenuShell's tabIndex={-1} card)
    // rather than blurring to <body>, which would drop the focus trap.
    const shell = e.currentTarget.closest<HTMLElement>('[tabindex="-1"]');
    if (shell) shell.focus(); else e.currentTarget.blur();
  };

  return (
    <MenuShell title="Found a New Republic" icon="flag" onBack={onBack} escDisabled={escDisabled} width="max-w-3xl">
      {/* A real form, so Enter in either field founds the republic — the
          primary action of the screen the fields belong to. Every other button
          inside it is type="button" or it would submit too. */}
      <form onSubmit={e => {
        e.preventDefault();
        onStart({ name: name.trim() || 'Red Republic', seed, mapSize, climate, difficulty });
      }}>
        <div className="grid gap-4 md:grid-cols-[3fr_2fr]">
          <div className="space-y-3 min-w-0">
            <div>
              {label('Republic name')}
              <div className="flex gap-1.5">
                <input
                  value={name}
                  maxLength={24}
                  onChange={e => setName(e.target.value)}
                  onKeyDown={fieldEscape}
                  className="flex-1 min-w-0 rounded border border-yellow-600/40 bg-red-950/60 px-2 py-1.5 text-sm text-yellow-50 outline-none focus:border-yellow-500"
                  aria-label="Republic name"
                />
                {diceBtn(() => setName(randomRepublicName()), 'name')}
              </div>
            </div>

            <div>
              {label('Map size')}
              <OptionCardGroup
                label="Map size"
                value={mapSize}
                onChange={setMapSize}
                className="flex gap-1.5"
                options={(Object.keys(MAP_SIZES) as MapSizeId[]).map(id => ({
                  value: id,
                  label: MAP_SIZES[id].label,
                  blurb: `${MAP_SIZES[id].tiles}×${MAP_SIZES[id].tiles} · ${MAP_SIZES[id].blurb}`,
                }))}
              />
            </div>

            <div>
              {label('Climate region')}
              <OptionCardGroup
                label="Climate region"
                value={climate}
                onChange={setClimate}
                className="grid grid-cols-2 gap-1.5"
                options={(Object.keys(CLIMATES) as ClimateId[]).map(id => ({
                  value: id,
                  icon: CLIMATES[id].icon,
                  label: CLIMATES[id].label,
                  blurb: CLIMATES[id].description,
                }))}
              />
            </div>

            <div>
              {label('Difficulty — starting conditions')}
              <OptionCardGroup
                label="Difficulty — starting conditions"
                value={difficulty}
                onChange={setDifficulty}
                className="flex gap-1.5"
                options={(Object.keys(DIFFICULTIES) as DifficultyId[]).map(id => {
                  const d = DIFFICULTIES[id];
                  return {
                    value: id,
                    icon: d.icon,
                    label: d.label,
                    blurb: `${d.blurb} · grant ₽${fmtMoney(d.startRubles)} + $${fmtMoney(d.startDollars)} · imports ×${d.importPriceMult}`,
                  };
                })}
              />
            </div>

            <div>
              {label('Seed')}
              <div className="flex gap-1.5">
                <input
                  value={seedText}
                  inputMode="numeric"
                  onChange={e => {
                    // Clamp, never wrap: a seed past the uint32 ceiling pins to
                    // the ceiling instead of becoming an unrelated valid seed.
                    const digits = e.target.value.replace(/\D/g, '');
                    setSeedText(digits !== '' && Number(digits) > MAX_SEED ? String(MAX_SEED) : digits);
                  }}
                  onKeyDown={fieldEscape}
                  className="w-36 rounded border border-yellow-600/40 bg-red-950/60 px-2 py-1.5 text-sm text-yellow-50 outline-none focus:border-yellow-500 tabular-nums"
                  aria-label="Map seed"
                />
                {diceBtn(() => setSeedText(String(randomSeed())), 'seed')}
                <div className="self-center text-[0.625rem] text-yellow-200/50">Same seed, same map — share it, comrade.</div>
              </div>
            </div>
          </div>

          <div className="flex items-start justify-center pt-4">
            <MapPreview seed={seed} tiles={MAP_SIZES[mapSize].tiles} />
          </div>
        </div>

        <div className="mt-5 flex justify-between">
          <button type="button" className={secondaryBtn} data-sfx="back" onClick={onBack}>Back</button>
          <button type="submit" className={primaryBtn} data-sfx="confirm">
            Found the Republic
          </button>
        </div>
      </form>
    </MenuShell>
  );
}
