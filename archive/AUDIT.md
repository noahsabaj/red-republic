# Audit — red-republic @ 86cf66f

**Scope:** the entire audio system — `src/audio/` (12 modules, 1,672 lines) plus its consumers: `src/components/MusicPanel.tsx`, `src/hooks/use-music.ts`, the `src/App.tsx` wiring, and the `musicVolume`/`musicShuffle`/`musicRepeat`/`musicTrackId`/`muteWhenHidden`/`hoverSounds` surface in `src/app/settings.ts`. Includes the requested idiomatic-fit pass (WebAudio, React 19, and this codebase's own stated conventions).
**Date:** 2026-07-26

## Summary

The audio system is in good structural health and its architecture is genuinely well-chosen: the pure/impure split is real (six of twelve modules import no WebAudio and are node-testable), the lookahead scheduler is the correct WebAudio idiom, the `useSyncExternalStore` integration satisfies React's snapshot-stability contract exactly, and CLAUDE.md's "audio listens, never participates" rule holds — nothing in `src/audio/` mutates engine state.

Every defect found traces to one shape: **`MusicEngine.playTrack()` is the only entry point, and every transport verb is expressed as a call to it.** Seek is a restart. Pause-resume is a restart. Track change is a restart. That works when a transport action is rare and deliberate, and it breaks down at both ends of the frequency range — at high frequency the scrubber fires ~176 full track restarts per drag (F1), and at the boundary condition a finished song resumes into its own final block instead of its beginning (F2).

Fix F1 first: it is on the panel's primary control, it is audible on every drag, and the fix is a two-line change in the panel rather than in the engine.

The one thing this audit could not assess is whether any of it *sounds good* — that is unverifiable by machine and is stated as such throughout the codebase's own comments.

## Findings

### F1 — The scrubber issues a full track restart per input event, garbling audio during any drag

- **Severity:** Medium
- **Status:** Confirmed
- **Location:** `src/components/MusicPanel.tsx:52` (`onChange` → `audio.seek`), `src/audio/audio-system.ts:264` (`seek`), `src/audio/music.ts:147-154` (`seek` → `playTrack`)
- **Failure scenario:** A player drags the seek bar in the State Radio panel. React maps `onChange` on `<input type="range">` to the native `input` event, so it fires once per value change across the entire drag — not once on release. Each event calls `audio.seek(v)` → `MusicEngine.seek()` → `playTrack()`, which allocates a new `GainNode`, ramps the outgoing node down over 60 ms, registers a 560 ms tail timer, rebuilds the song plan, and resets the scheduler. The player hears a stutter/garble for the duration of the drag rather than a single clean jump.
- **Evidence:** Repro harness driving the real `MusicEngine` against a recording context, simulating one drag across the bar at the panel's own `step={durationS/200}`:

  ```
  step size (s)  : 1.00  => onChange events per full drag: 176
  playTrack() calls : 176
  GainNodes created : 506
  pending tail timers : 176 (each setTimeout ~560ms)
  notes scheduled : 354
  ```

  176 restarts, 506 gain nodes and 354 scheduled voices for a single user gesture. The end position is correct, so the defect is entirely in the transient — which is why it reads as an audio-quality problem rather than a logic bug.
- **Root cause:** See RC1. Aggravated by using `onChange` (continuous) where a scrubber wants `onChange` for the *thumb* and a commit on release for the *seek*.

### F2 — After the playlist ends with repeat off, pressing Play replays only the final 4 seconds

- **Severity:** Medium
- **Status:** Confirmed
- **Location:** `src/audio/audio-system.ts:303-305` (`onSongEnded` → `setMusicPlaying(false)`), `src/audio/music.ts:158-178` (`setPlaying`), `src/audio/music.ts:169` (`elapsedAtPause = this.elapsedS()`)
- **Failure scenario:** The player sets repeat to **off** and lets the last track finish. `onSongEnded` → `autoAdvance` returns `stop: true` → `setMusicPlaying(false)` → `MusicEngine.setPlaying(false)`, which records `elapsedAtPause = elapsedS()`. At end-of-song that value is exactly `durationS`. When the player presses Play, `setPlaying(true)` resolves `blockIndexAtTime(plan, durationS)` to the *last* block and re-enters there — so playback resumes 4.3 seconds before the end, hits `songEndTime`, fires `onEnded` again, and stops. The button appears to do nothing but emit a short chord, repeatably.
- **Evidence:** Repro harness, March of the Vanguard (175.7 s):

  ```
  autoAdvance(off,end): {"pos":5,"reshuffle":false,"stop":true,"sameTrack":false}
  elapsed after stop  : 175.7 (of 175.7)
  resume enters block : 27 of 27
  resume startElapsed : 171.4
  => audio remaining  : 4.3 seconds
  re-ended after      : 4.5 seconds of playback
  ```
- **Root cause:** See RC1. `setPlaying(true)` has no notion of "the song is finished" distinct from "the song is paused near its end" — both are just an elapsed value handed to `blockIndexAtTime`. Reachable only with the non-default `repeat: 'off'` (default is `'all'`), which is why this is Medium rather than High.

### F3 — The score is not "note for note" identical across the menu/game boundary, contradicting three stated claims

- **Severity:** Low
- **Status:** Confirmed
- **Location:** `src/audio/music.ts:322` (arp density × intensity), `src/audio/music.ts:343` (lead chance × intensity), `src/audio/audio-system.ts:132,196` (`setIntensity(0.35 | 0.55)`), claim sites: `src/audio/music.ts:9-12`, `src/audio/__tests__/music-determinism.test.ts:10-14`, `src/components/MusicPanel.tsx:106`
- **Failure scenario:** `intensity` is 0.35 on the menu and 0.55 in game. Two layers roll their per-step probability *against* intensity — the arp drops a step when `vrng() >= density × min(1, intensity × 1.8)`, and the lead skips its whole phrase when `vrng() >= chance × intensity × 2`. So the same track plays a materially different note stream depending on which screen you are on. Nothing breaks; but the panel tells the player "six fixed works, synthesized live from a seeded score: **the same performance, note for note, every broadcast**," and that is false for any track with an arp or lead layer — which is four of the six.
- **Evidence:** Repro harness capturing the full note stream of March of the Vanguard at both intensities:

  ```
  notes at menu intensity 0.35: 1113
  notes at game intensity 0.55: 1341
  identical note stream       : false
  ```

  The determinism test suite does not catch this because every case constructs the engine at its default `intensity = 0.5` and never varies it.
- **Root cause:** `intensity` conflates two separable concerns — *loudness/brightness* (level, filter cutoff), which should legitimately vary by scene, and *note selection*, which the module docs claim is fixed. Only the second contradicts the stated invariant.

### F4 — The 100 ms scheduler interval runs for the process lifetime and is never cleared

- **Severity:** Low
- **Status:** Confirmed
- **Location:** `src/audio/music.ts:142` (`setInterval`, started in `playTrack`), `src/audio/music.ts:182` (cleared only in `dispose()`)
- **Failure scenario:** `playTrack` starts the pump interval and only `dispose()` clears it. `dispose()` has **zero production callers** — grep across `src/` finds it referenced only from `music-determinism.test.ts`. So once music has started, a 100 ms timer runs for the entire lifetime of the page (or the desktop app), including while music is paused, while the playlist has stopped, and while the tab is hidden. `pump()` early-returns on `!this.playing`, so the wasted work is small — but it is unconditional and unbounded.
- **Evidence:** Directly observed: the audit's repro harness **hung until an explicit `process.exit(0)` was added**, because the live interval kept bun's event loop from draining. That is the same timer, in a context where its persistence is visible.
- **Root cause:** No teardown path exists for `MusicEngine` because `AudioSystem` is an app-lifetime singleton that never disposes it. The interval's lifetime is tied to nothing.

## Root causes

**RC1 — Every transport verb is implemented as "restart the track."** F1, F2, and F4 all descend from `playTrack()` being `MusicEngine`'s sole entry point. `seek()` calls it, `setPlaying(true)` calls it, `selectTrack`/`nextTrack`/`prevTrack` call it via `startCurrent`, and `onSongEnded` calls it. Each call does full re-initialisation: new gain node, crossfade of the old one, tail timer, `buildSongPlan`, block-index reset, interval start.

This is a reasonable design for deliberate, infrequent actions, and for those it works correctly. It fails where the frequency or the boundary conditions differ from that assumption — a dragged scrubber (hundreds of calls per second of gesture, F1), and a resume that must distinguish "paused mid-song" from "finished" (F2). F4 is the same shape seen from the other side: because there is one entry point and no exit point, the interval it starts has no owner.

F3 stands alone and does not share this cause.

## Opportunities

Not defects — places where the intent itself could be better.

### O1 — `AudioSystem` — the module holding both confirmed bugs — has no tests at all

- **Kind:** absent
- **Confidence:** Grounded
- **Rationale:** Seven test files cover `arrange`, `event-sounds`, `format`, `music-determinism`, `music-theory`, `playlist`, and `ui-catalog` — every pure module plus the WebAudio scheduler. `audio-system.ts` (366 lines) has none, and it is precisely where F1's and F2's transport logic lives. The obstacle is that it reaches `globalThis.AudioContext` directly in `unlock()`, so it cannot be constructed in the node harness. The fix is small and the tooling already exists — `__tests__/recording-context.ts` is a working fake context; making the context factory injectable would put the whole transport state machine (shuffle/repeat/advance/pause/seek/persistence) under test. Note that `playlist.ts` is thoroughly tested but is only the *decision* layer; the bugs are both in the *application* of those decisions.
- **Rough cost:** contained

### O2 — Seek is the heaviest operation in the system and is bound to the most frequent interaction

- **Kind:** wrong shape
- **Confidence:** Grounded
- **Rationale:** RC1 stated as a design question rather than a defect. A scrubber's natural contract is "move the thumb freely, commit on release," and the platform already distinguishes these (`input` for live movement, `pointerup`/`keyup`/`change`-on-commit for the commit). The panel currently treats every intermediate value as a commit. Fixing F1 at the panel is a two-line change; the deeper version is giving `MusicEngine` a cheap `repositionTo(block)` that reuses the current gain node rather than crossfading a new one — which would also make F2's resume path expressible without a restart.
- **Rough cost:** trivial at the panel, contained at the engine

### O3 — `src/audio/` imports the simulation's RNG out of the map generator

- **Kind:** wrong shape
- **Confidence:** Grounded
- **Rationale:** `arrange.ts:15` and `music.ts:23` both `import { mulberry32 } from '../game/mapgen'`. CLAUDE.md states audio "must never import engine internals or touch sim RNG." No sim RNG *stream* is perturbed — the seeds are the audio module's own, so the invariant is upheld in substance — but the audio layer now depends on the map-generation module to obtain a hash function, which is the dependency direction the rule exists to prevent. Extracting `mulberry32` to a shared util both layers import would satisfy the rule in letter as well as spirit and cost nothing behaviourally.
- **Rough cost:** trivial

### O4 — Nothing ties `event-sounds.ts` to the engine events it claims to map

- **Kind:** absent
- **Confidence:** Grounded
- **Rationale:** `soundForEvent` keys a map on the string `` `${kind}:${icon}` ``. If a config change alters an event's `icon`, the lookup silently returns `null` and that event goes quiet — no error, no test failure. `event-sounds.test.ts` does check that every mapped sound has a synth recipe, but it iterates a **hardcoded list of icon names** written in the test, not the engine's actual event vocabulary, so it cannot detect drift. This is exactly the failure class `ui-guards.test.ts` already guards elsewhere in this repo (unknown icon names fail the build), which makes it a gap in an otherwise-applied convention rather than a novel idea.
- **Rough cost:** contained

### O5 — Two independent white-noise buffer generators

- **Kind:** excess
- **Confidence:** Grounded
- **Rationale:** `sfx.ts:29-39` caches one second of noise per context in a `WeakMap`; `music.ts:400-408` keeps its own `noiseCache` instance field. The duplication is *principled* — music's must be seeded (`mulberry32(0x51ed)`) for determinism while SFX may use `Math.random()` — so this is not a defect. But it is two allocations of an identical-sized buffer and two cache strategies for one concept, and a single seeded generator would serve both without weakening anything.
- **Rough cost:** trivial

### O6 — `intensity` could stop being a single scalar

- **Kind:** wrong shape
- **Confidence:** Speculative
- **Rationale:** F3's root is that one number drives both mix (level, cutoff) and composition (note probability). Splitting it — a `mix` scalar that scene changes may move freely, and a `density` scalar that stays fixed per track — would make the "note for note" claim true by construction rather than by convention, and would remove the single largest obstacle noted in the parked Phase 2 work (authored notes must not be probabilistically dropped). Speculative because I have not established that the scene-dependent density is unwanted; it may have been a deliberate choice to keep menu music sparser, in which case the honest fix is to correct the claim in `MusicPanel.tsx:106` instead.
- **Rough cost:** contained

### O7 — Skipping or seeking while paused silently starts playback

- **Kind:** wrong shape
- **Confidence:** Grounded
- **Rationale:** `nextTrack`/`prevTrack`/`selectTrack` all route through `startCurrent` → `playTrack`, which unconditionally sets `playing = true`; `seek` does the same. The now-playing snapshot stays consistent (the UI correctly shows "playing"), so nothing is broken and `audio-system.ts:262` documents the seek case deliberately. But most media players preserve paused state across a track change, and a player who paused the radio to hear something else will find that browsing the programme list resumes it. Worth a decision rather than a fix.
- **Rough cost:** trivial

### O8 — `MusicEngine.dispose()` is production-dead

- **Kind:** excess
- **Confidence:** Grounded
- **Rationale:** Zero production callers (see F4). It is real, working teardown code maintained for tests only. Either it should acquire a caller — which is the F4 fix — or its test-only status should be stated, so a future reader does not assume the app has a teardown path it does not have.
- **Rough cost:** trivial

## Coverage

- **Examined:** All 12 modules in `src/audio/` read in full (`arrange`, `audio-system`, `event-sounds`, `format`, `index`, `music`, `music-theory`, `playlist`, `sfx`, `tracks`, `ui-catalog`, `ui-sounds`); all 7 test files plus `recording-context.ts`; `MusicPanel.tsx`, `use-music.ts`, the audio wiring in `App.tsx` (lines 29, 77, 87-112, 228, 277), and the audio-related fields of `settings.ts`. Axes: WebAudio node/timer lifecycle, scheduler correctness under the lookahead pattern, determinism against the module's own stated invariant, gesture-gating and autoplay policy, tab-visibility interaction, React 19 external-store contract and effect cleanup, event-drain single-consumer discipline, rate limiting under burst, persistence round-trip, and conformance to CLAUDE.md's audio rules.
- **Not examined:** Whether any of it sounds good. Musical quality is not machine-assessable and the codebase says so; no judgment about the score's aesthetics appears above. Also not examined: the Tauri/desktop audio path beyond noting it shares the same web bundle, and `src/components/ui/` (vendored shadcn, excluded from linting by project convention).
- **Left unverified:** Nothing reported is Unverified — F1–F4 are all Confirmed by the repro harness. Two candidates were investigated and **dismissed** rather than reported: (a) background-tab `setInterval` throttling starving the 2 s lookahead — Chrome exempts audibly-playing pages from intensive throttling, and `muteWhenHidden` defaults to `true` (suspending the context, which freezes `ctx.currentTime`) so the elapsed bookkeeping stays coherent either way; (b) oscillator/filter nodes never being explicitly `disconnect()`ed in `sfx.ts` — this is the standard WebAudio idiom, as stopped source nodes and their exclusive downstream graph are collectable, and no growth path was traced.
- **Touched by tooling (audit pass):** No tracked file was modified; the working tree was clean apart from this report. `AUDIT.md` created at the repo root. The repro harness and its output were written **outside the repo** to the session scratchpad (`…/scratchpad/repro.ts`, `…/scratchpad/out.txt`) specifically so they never reach `tsc -b` or `eslint`. A `bun` process left running by the first repro attempt (before the explicit-exit fix) was stopped. No test suite was run — `bun run check` was deliberately not invoked, so nothing in `node_modules/.vite` or any cache was written by this audit.

---

## Resolution

**Date:** 2026-07-26
**Branch:** `fix/audio-audit` (10 commits off `86cf66f`)
**Gate:** `bun run check` green — 572 tests, 65 files, tsc and eslint clean.

| ID | Disposition | Commit | Verification |
|----|-------------|--------|--------------|
| F1 | Fixed (root) | `4b3aa68` | Live browser, `?demo`: a full drag fires **200 input events → 0 seeks held, 1 on release**. Thumb held at 76 s while the playhead ran to 11.4 s (poll suspension), then landed at 86.3 s block-snapped with the chord changing to `[8,12,15]`. No console errors. |
| F2 | Fixed (root) | `d2b039b` | `music-transport.test.ts` + `audio-system.test.ts`. Proven to bite: reverting fails `expected 171.42857142857142 to be less than 1` (engine) and `expected 200 to be less than 1` (system). |
| F3 | Fixed (root) + claims corrected | `4e13420` | `music-determinism.test.ts` › "scene intensity changes the mix, never the notes", over all 6 tracks. Proven to bite: `march menu vs field: expected [...1129] to deeply equal [...1331]`. |
| F4 | Fixed (root) | `2c05262` | `music-transport.test.ts` › "scheduler lifetime". Proven to bite: `expected 1 to be +0`. |
| O1 | Accepted → done | `ce14403` | New `audio-system.test.ts`, 12 tests over the transport state machine. |
| O2 | Accepted → done | `d5530d8` | Behaviour-neutral; 86 audio tests unchanged. |
| O3 | Accepted → done | `8c891b3` | `mapgen-snapshot` + `save-roundtrip` unchanged. |
| O4 | Accepted → done | `244f87a` | Proven to bite: renaming `good:star` → `good:medal` in `objectives.ts` now fails two guards; previously silent. |
| O5 | Accepted → done | `e691aba` | Determinism suite unchanged (same seed, same fill). |
| O6 | Folded into F3 | `4e13420` | Was the same change. |
| O7 | Accepted → done | `c530e34` | Unit test + live browser check. |
| O8 | Resolved by F4 | `2c05262` | `dispose()` is still test-only, but the interval it existed to clear is now owned by the play/pause lifecycle, so nothing depends on a caller appearing. |

### Deviations and discoveries

Three things during implementation departed from the plan as approved, each stated in its commit:

1. **O2 kept the gain-node swap.** The approved sketch had `repositionTo` reuse the current gain node. It can't: retiring the outgoing node is exactly what silences voices already scheduled ahead of the playhead (2 s lookahead, pad releases to 4 s). Reusing one node would require holding a reference to every source ever started in order to stop each one — more machinery, thousands of retained references, identical audible result. The plan reuse was the part worth having.

2. **F3's approved fix was necessary but not sufficient.** `intensity` was one of *two* inputs to note selection; mood is the other (`dropThirdWhenCold` drops the pad's third, and cold halves the lead's chance). Mood was kept — it is an authored per-track feature and removing it was not what was approved — so the real guarantee is *same track + same weather ⇒ same notes*, not unconditional identity. The three claim sites now say that. Correcting the copy is part of the fix, not a substitute for it.

3. **A second guard had silently narrowed.** `ui-guards.test.ts` scans for event icons in `engine.ts` only. v1.9.1 moved event emission into `systems/`, so `port`, `rain`, `star` and `summer` had fallen outside every check while the guard kept passing. Widened to the whole sim as part of O4. Found during triage, not in the original audit.

O3 touched 10 files, above the five-file gate, because the approved item inherently means updating every caller — the standing no-backwards-compatibility rule rules out a re-export shim.

### Coverage of this pass

- **Not fixed:** nothing. All four findings and all seven raised opportunities are closed or folded.
- **Still unverifiable:** whether any of it *sounds* good. Every fix here is about what plays and when, never about whether the result is musical. F3 in particular makes menu music audibly denser — that was the accepted trade and it wants a listen.
- **Touched by tooling (fix pass):** `bun run check` wrote its usual caches under `node_modules/.vite`. A dev server was started on port 3000 for F1's browser verification and **stopped** afterwards; the demo session's `localStorage` was left as found (`musicTrackId: radio`, shuffle off, repeat all). Scratch repro files stayed outside the repo in the session scratchpad and the temporary `/tmp` backups used for the bite-checks were deleted.
