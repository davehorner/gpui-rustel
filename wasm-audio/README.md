# Rustel audio in WebAssembly

The browser runs the actual Rustel QuickJS score evaluator, transpiler, scheduler, voice resolver and scalar DSP. The default demo dropdown contains the same `.strudel` files as the native app; the build copies `../demos` and generates a SHA-256 catalog. No native Rustel process or music hub is required.

## Build and play

```powershell
python web/build.py
python web/serve.py
```

Open http://127.0.0.1:18766/ and click **Play** to enable the browser audio device. Choose Arpeggio, Counterpoint, Many lanes, Bachata, Sample bank or Custom samples. The original JavaScript appears in the source panel. Tempo, solo and transpose changes evaluate a replacement score before committing its generation. Pause retains the consumed-sample position; Stop resets the score. A failed replacement retains the previous engine. Stop cancels a pending sample load.

Use `?mini=1` to start with the earlier mini-notation synth examples. `?hub=ws%3A%2F%2F127.0.0.1%3A19613` retains native-producer viewer mode.

## JavaScript and samples

QuickJS executes score functions, callbacks, variables, `stack`, arrangement, transformations and `samples()` / `preload()` effects. The host loads Rustel's pinned default bank manifests, inline Dirt banks, custom HTTP(S) maps and manifests, GitHub shorthand and General MIDI WebAudioFont zones. Array variants, note-mapped roots, drum-bank prefixes/aliases, soundfont key ranges, tuning and loops are preserved. `gm_electric_piano_1/2` are accepted aliases for the pinned `gm_epiano1/2` names. Custom banks override defaults. Unsupported/missing sounds produce errors or loading notices, never an oscillator substitute.

A worker fetches encoded samples; the browser decodes compressed files through `decodeAudioData`. Raw WebAudioFont PCM is converted directly. The decoded PCM is installed in Rustel's real sample voices. Fetches require CORS permission from the source server. Manifests are hash-verified against the pinned native catalog. No downloaded soundfont JavaScript is evaluated: its zone data is parsed.

The host probes the first 64 cycles before replacement to preload required sounds and variants. Later, newly encountered assets load asynchronously; their first triggering onset may be silent. `preload()` can request them beforehand. Encoded/decoded individual samples are limited to 32 MiB, the decoded host cache to 128 MiB, installed samples per engine to 256 MiB, and score loading to 60 seconds. Initial score evaluation runs in the audio worker with a 10-second interrupt deadline; scheduled queries retain their own interrupt deadlines. Score source is limited to 512 KiB and 64 lanes.

This is browser score/audio parity for the supplied demos, not every native integration. Native plugins, Csound, MIDI/OSC/serial devices, local filesystem sample paths, speech synthesis, wavetable-bank installation, seeking and named-const provenance are not supported. JavaScript score semantics remain those of the pinned Rustel version, including its mini-notation treatment of double-quoted strings.

## Audio ownership

A dedicated Web Worker owns the WASM engine and renders 4,096-frame stereo chunks. A direct MessageChannel connects it to an AudioWorklet; GPUI drawing and the browser main thread do not relay PCM. The worklet keeps at most four chunks, starts after prefill, discards replaced generations and advances the displayed clock from consumed samples. Pause/play acknowledgements come back from the worklet. Scope, spectrum and stereo plots use real rendered audio. Master gain starts at 0.25. Localhost or HTTPS is required for AudioWorklet.

## Portability and source

Rustel is pinned to [79fd1fb3fd64c0a637f475ad24de675fd5b2ac59](https://github.com/tzfm/rustel/tree/79fd1fb3fd64c0a637f475ad24de675fd5b2ac59). `prepare_engine.py` creates an isolated checkout in `.build-sources/rustel-browser-engine`, outside the cached Cargo target directory and applies the tracked portability patch: browser clocks, portable QuickJS value checks, and literal preservation inside nested sample maps. It refuses an unrecognized dirty checkout. Native app dependencies are unchanged.

`build.py` compiles QuickJS with its vendored WASM libc, supplies the host clock and a 64 MiB stack, and generates matching wasm-bindgen bindings. On Windows it needs Clang/llvm-ar on PATH and copies vendored headers to avoid canonical Windows include-path problems. The full web build uses pinned GPUI nightly-2026-10-31 and project-local wasm-bindgen 0.2.130. The downloadable bundle includes this wrapper, patches and build scripts; running its source build retrieves the exact upstream sources.

Rustel and this wrapper use AGPL-3.0-or-later. The bundle includes the license and upstream source reference. QuickJS/rquickjs and WASM libc retain their upstream licenses.

## Validation

```powershell
python wasm-audio/build.py --target nodejs --out target/rustel-browser-audio-test
node wasm-audio/test-javascript.cjs
node wasm-audio/test-wasm.cjs
cargo test --manifest-path wasm-audio/Cargo.toml --locked
```

The JavaScript test evaluates unchanged demo sources, checks actual scheduling/DSP, sample declarations and installed PCM, and refuses runaway JavaScript. The mini test runs the real WASM engine through the shipped worklet consumer, checking output, queue bounds, pause and stale generations.

Open `/audio-validation.html` and press **Run validation** for an integration check of the shipped host, worker, QuickJS WASM, remote samples, Chrome decoding and worklet. It checks each native demo's source, played PCM and events, command acknowledgements, pause and Stop. Its device gain is zero; analysis occurs before master gain. This check does not make a subjective listening assessment.

The GPUI host prepares the first Nightshade frame before evaluating the default
score. A refused initial score leaves the viewer and demo catalog usable; Play
retries the selected score. Worker, worklet, JS glue and WASM requests share a
content version so an engine update does not reuse an older cached deadline.
