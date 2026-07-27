// ============================================================
// GameEvent → sound-effect mapping. Pure data: the engine's events carry
// (kind, icon) and the icon is a de facto event-type tag. Anything not
// mapped stays silent — every toast making noise would be fatiguing.
// ============================================================
import type { SfxName } from './sfx';

const MAP: Record<string, SfxName> = {
  'good:star': 'objective',       // objective complete
  'good:check': 'complete',       // building finished
  'good:contract': 'contractDone',
  'info:contract': 'contractOffer', // tender arrives / withdrawn
  'bad:contract': 'alertBad',     // contract failed
  'bad:winter': 'alertBad',
  'bad:freeze': 'alertBad',
  'bad:summer': 'alertBad',       // drought
  'bad:users': 'alertBad',        // citizens leave
  'good:users': 'complete',       // settlers arrive
  'good:rain': 'complete',        // drought breaks
  'good:port': 'complete',        // thaw
};

export function soundForEvent(kind: string, icon?: string): SfxName | null {
  return MAP[`${kind}:${icon ?? ''}`] ?? null;
}
