// ============================================================
// One second of white noise, cached per AudioContext — the raw material for
// every percussive voice in the system: the score's kick/snare/hat and the
// filtered-noise component of most SFX recipes.
//
// Seeded, not Math.random(). The score must be bit-identical every play, and
// a shared buffer is only shareable if the deterministic consumer can accept
// it. SFX get their variation from the random START OFFSET into the buffer,
// not from the samples, so nothing is lost by fixing the contents.
// ============================================================
import { mulberry32 } from '@/lib/rng';

const NOISE_SEED = 0x51ed;

const cache = new WeakMap<BaseAudioContext, AudioBuffer>();

export function noiseBuffer(ctx: BaseAudioContext): AudioBuffer {
  let buf = cache.get(ctx);
  if (!buf) {
    buf = ctx.createBuffer(1, ctx.sampleRate, ctx.sampleRate);
    const data = buf.getChannelData(0);
    const rng = mulberry32(NOISE_SEED);
    for (let i = 0; i < data.length; i++) data[i] = rng() * 2 - 1;
    cache.set(ctx, buf);
  }
  return buf;
}
