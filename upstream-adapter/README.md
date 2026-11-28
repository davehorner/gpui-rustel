# Upstream Rustel adapter

This optional backend integrates upstream issue [#318](https://github.com/tzfm/rustel/issues/318) and merged [PR #323](https://github.com/tzfm/rustel/pull/323). It runs the pinned Rustel CLI and translates its NDJSON UI protocol into the existing music-link messages. The build helper now uses the same maintained checkout as browser audio and Studio, including the `_foxy()`/`_all_foxy()` visual declarations. Ordinary upstream executables remain compatible with scores using upstream methods.

## Build and use

From the gpui-nightshade root (adapter is staged here pending transfer to bespoke_rustel):

```powershell
python upstream-adapter/build_runtime.py
# Start music-hub separately for the optional hub bridge:
cargo run --manifest-path upstream-adapter/Cargo.toml --features hub --locked -- --rustel=.build-sources/rustel-browser-engine/target/release/rustel.exe --score=../bespoke_rustel/music-link/demo.strudel
```

Add `--silent` to validate without opening an audio device. The runtime watches the selected score. Hosted editor execution validates a private sibling `.js` copy and restarts playback from that copy; it does not rewrite the original. Native sample loading retains upstream behavior; this minimal build does not include every upstream default feature.

Pinned upstream: `79fd1fb3fd64c0a637f475ad24de675fd5b2ac59` (0.1.9). Build features are `device-audio,midi`: `device-audio` alone currently fails with unresolved `midi_clock` references. Upstream remains AGPL-3.0-or-later; its source/license stay in the maintained checkout. Changes are captured in `wasm-audio/patches/browser-portability.patch`. No binary is checked in.

For the native GPUI app, use its `--rustel=EXE --score=FILE` mode. It uses this crate directly, owns the child runtime, and needs no hub, browser or separately launched Rustel. This is subprocess hosting; the engine is not yet linked into GPUI's process.

## What the new output supports

| Data/control | Upstream adapter |
| --- | --- |
| Lane labels, IDs and source lines | Supported |
| Accepted onsets | Actual frequency, gain, sound, lane, rational times, source ranges |
| Source highlighting | Only when exact UTF-8 source SHA256 matches source_revision |
| Solo, transpose, tempo | Acknowledged; revision/generation gated |
| Play | Maps to native restart |
| Stop | Acknowledged; CLI exits (observed exit code 1) |
| Pause / seek | Unsupported; explicitly rejected locally |
| Master scope / spectrum | Supported when score requests an audio visual, e.g. `.scope()` |
| Stereo pairs / vectorscope | Unavailable in upstream ui_audio |
| Named const provenance / helper graph | Unavailable in upstream output |
| Full cutoff/pan/resonance control maps | Unavailable; value labels are not parsed into fake data |
| Demo switching / edited source | The owned supervisor switches catalogued demos and validates private edited score copies; the bare stdio encoder does not rewrite source |
| Lane mute | Upstream supports it; existing music-link Command has no mute field yet |

Capabilities are attached as an additional snapshot JSON field for compatible clients. Existing music-link structs remain unchanged. Process epochs prevent command/audio generation collisions after a child restart. Queues and line sizes are bounded; acknowledgements report acceptance, rather than assuming a queued command applied. Normal Hosted drop kills and waits for the owned child; applications must drain shutdown before exiting. Crash recovery/OS process-tree containment is further work.

## Evidence and checks

`probe-report.json` records a real silent upstream run on Windows on 2026-11-25. Scope arrays had 512 samples. Solo, transpose, tempo, restart and stop acknowledged; an unsupported pause produced no acknowledgement. The adapter rejects pause before sending it.

```powershell
cargo test --manifest-path upstream-adapter/Cargo.toml --locked
cargo clippy --manifest-path upstream-adapter/Cargo.toml --all-targets --features hub --locked -- -D warnings
$env:RUSTEL_UPSTREAM_EXE = 'FULL/PATH/TO/rustel.exe'
cargo test --manifest-path upstream-adapter/Cargo.toml --locked --test protocol -- --ignored --test-threads=1
```

The upstream in-process Session callback example also ran successfully: 4,800 stereo frames, peak 0.060 (no audio device).

The opt-in native test launches the real CLI, receives events and audio, transposes a live lane and checks the acknowledgement, then drops the owned runtime.

## Next step: link the engine in-process

Upstream `docs/embedding.md` and `crates/engine/examples/session_callback.rs` provide the foundation: put `Session` and QuickJS on a dedicated worker with `QUERY_WORKER_STACK_BYTES`; schedule bounded owned audio events into `Ring`; consume those events in a host-owned `LiveScalarBackend` audio callback. GPUI and Nightshade must not evaluate JavaScript or wait on the audio callback. Keep one audio owner and use its sample clock. The CLI currently supplies continuous scheduling, file watching, controls and diagnostics; replacing those responsibilities is the remaining in-process hosting work.
