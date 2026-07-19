import { useState } from 'react';
import { MenuShell } from './MenuShell';
import { MapPreview } from './MapPreview';
import { OptionCard, primaryBtn, secondaryBtn } from './controls';
import { CLIMATES, DIFFICULTIES, MAP_SIZES } from '@/game/config';
import type { ClimateId, DifficultyId, MapSizeId } from '@/game/config';
import { randomRepublicName, randomSeed } from '@/app/session';
import type { NewGameConfig } from '@/app/session';
import { GameIcon } from '@/ui/GameIcon';

interface Props {
  onBack: () => void;
  onStart: (cfg: NewGameConfig) => void;
  escDisabled: boolean;
}

export function NewGameScreen({ onBack, onStart, escDisabled }: Props) {
  const [name, setName] = useState(randomRepublicName);
  const [seed, setSeed] = useState(randomSeed);
  const [mapSize, setMapSize] = useState<MapSizeId>('medium');
  const [climate, setClimate] = useState<ClimateId>('plains');
  const [difficulty, setDifficulty] = useState<DifficultyId>('normal');

  const label = (text: string) => (
    <div className="text-[0.625rem] font-black uppercase tracking-wider text-yellow-400 mb-1">{text}</div>
  );
  const diceBtn = (onClick: () => void, what: string) => (
    <button onClick={onClick} aria-label={`Randomize ${what}`} title={`Randomize ${what}`}
      className="rounded bg-red-900/70 px-2 py-1.5 text-yellow-200 hover:bg-red-800">
      <GameIcon name="dice" size={14} />
    </button>
  );

  return (
    <MenuShell title="Found a New Republic" icon="flag" onBack={onBack} escDisabled={escDisabled} width="max-w-3xl">
      <div className="grid gap-4 md:grid-cols-[3fr_2fr]">
        <div className="space-y-3 min-w-0">
          <div>
            {label('Republic name')}
            <div className="flex gap-1.5">
              <input
                value={name}
                maxLength={24}
                onChange={e => setName(e.target.value)}
                className="flex-1 min-w-0 rounded border border-yellow-600/40 bg-red-950/60 px-2 py-1.5 text-sm text-yellow-50 outline-none focus:border-yellow-500"
                aria-label="Republic name"
              />
              {diceBtn(() => setName(randomRepublicName()), 'name')}
            </div>
          </div>

          <div>
            {label('Map size')}
            <div className="flex gap-1.5">
              {(Object.keys(MAP_SIZES) as MapSizeId[]).map(id => (
                <OptionCard key={id} selected={mapSize === id} label={MAP_SIZES[id].label}
                  blurb={`${MAP_SIZES[id].tiles}×${MAP_SIZES[id].tiles} · ${MAP_SIZES[id].blurb}`}
                  onClick={() => setMapSize(id)} />
              ))}
            </div>
          </div>

          <div>
            {label('Climate region')}
            <div className="grid grid-cols-2 gap-1.5">
              {(Object.keys(CLIMATES) as ClimateId[]).map(id => (
                <OptionCard key={id} selected={climate === id} icon={CLIMATES[id].icon}
                  label={CLIMATES[id].label} blurb={CLIMATES[id].description}
                  onClick={() => setClimate(id)} />
              ))}
            </div>
          </div>

          <div>
            {label('Difficulty — starting conditions')}
            <div className="flex gap-1.5">
              {(Object.keys(DIFFICULTIES) as DifficultyId[]).map(id => {
                const d = DIFFICULTIES[id];
                return (
                  <OptionCard key={id} selected={difficulty === id} icon={d.icon} label={d.label}
                    blurb={`${d.blurb} · grant ₽${d.startRubles.toLocaleString()} + $${d.startDollars.toLocaleString()} · imports ×${d.importPriceMult}`}
                    onClick={() => setDifficulty(id)} />
                );
              })}
            </div>
          </div>

          <div>
            {label('Seed')}
            <div className="flex gap-1.5">
              <input
                value={seed}
                inputMode="numeric"
                onChange={e => {
                  const digits = e.target.value.replace(/\D/g, '');
                  setSeed(digits === '' ? 0 : Number(digits.slice(0, 10)) >>> 0);
                }}
                className="w-36 rounded border border-yellow-600/40 bg-red-950/60 px-2 py-1.5 text-sm text-yellow-50 outline-none focus:border-yellow-500 tabular-nums"
                aria-label="Map seed"
              />
              {diceBtn(() => setSeed(randomSeed()), 'seed')}
              <div className="self-center text-[0.625rem] text-yellow-200/50">Same seed, same map — share it, comrade.</div>
            </div>
          </div>
        </div>

        <div className="flex items-start justify-center pt-4">
          <MapPreview seed={seed} tiles={MAP_SIZES[mapSize].tiles} />
        </div>
      </div>

      <div className="mt-5 flex justify-between">
        <button className={secondaryBtn} onClick={onBack}>Back</button>
        <button
          className={primaryBtn}
          onClick={() => onStart({ name: name.trim() || 'Red Republic', seed, mapSize, climate, difficulty })}
        >
          Found the Republic
        </button>
      </div>
    </MenuShell>
  );
}
