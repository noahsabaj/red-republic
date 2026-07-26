// A fake AudioContext for AudioSystem tests: RecordingContext (which already
// implements everything MusicEngine touches, and logs the note stream) plus
// the handful of members the SYSTEM layer reaches for — the compressor, the
// destination, and the suspend/resume state machine.
//
// Injected through AudioSystem's AudioContextFactory seam, so the transport
// runs for real: a genuine MusicEngine on a fake clock the test advances.
import { RecordingContext } from './recording-context';

class FakeCompressor {
  threshold = { value: 0 };
  ratio = { value: 0 };
  connect() { return this; }
  disconnect() { /* no-op */ }
}

export class FakeAudioContext extends RecordingContext {
  state: 'running' | 'suspended' | 'closed' = 'running';
  destination = { connect() { /* no-op */ }, disconnect() { /* no-op */ } };
  createDynamicsCompressor() { return new FakeCompressor(); }
  resume() { this.state = 'running'; return Promise.resolve(); }
  suspend() { this.state = 'suspended'; return Promise.resolve(); }
}

/** Drive the audio clock past the end of whatever is playing and let the
 *  scheduler's interval fire, so the song reaches its end and the transport
 *  advances. Requires vi.useFakeTimers(). */
export function runSongToEnd(ctx: FakeAudioContext, advanceTimers: (ms: number) => void) {
  ctx.currentTime += 10_000; // past any track's duration
  advanceTimers(200);        // two ticks of the 100 ms pump
}
