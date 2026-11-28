# GPUI Rustel

A music-first GPUI application with shared native and WebGPU Nightshade 3D and a self-contained browser WASM audio host. The renderer integration lives in [gpui-nightshade](https://github.com/davehorner/gpui-nightshade), pinned here as a submodule. No sibling bespoke_rustel checkout is required to build.

## Browser

```powershell
python scripts/prepare.py
rustup toolchain install 1.100.0 --target wasm32-unknown-unknown
rustup toolchain install nightly-2026-10-31 --target wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked --root target/wasm-tools
python web/build.py
python web/serve.py
```

Open http://127.0.0.1:18766/. Bachata loads by default and starts automatically when browser audio permissions allow; otherwise, click **Play** to enable it. The dropdown includes the same JavaScript `.strudel` sources as the native app: Arpeggio, Counterpoint, Many lanes, Bachata, Sample bank and Custom samples. QuickJS, scheduling and actual Rustel DSP run in WASM, with sample-bank fetching, browser decoding and General MIDI soundfont zones. No native engine process is required. See [audio architecture, limitations and tests](wasm-audio/README.md).

Both targets use the same Nightshade stage: named-part colors, time/cycle and pitch/MIDI axes, lane labels, and an embedded skinned Fox for every visible lane. Each fox uses its const color; its Run clip and box-to-box jump arc follow the music clock and freeze on pause. Camera controls are hidden by default; **Show camera** reveals the left panel with Fit/Top/Front/Side views, rotation, pan, deep zoom, Fox focus and 35/65/100% fox opacity. **Fullscreen** toggles the viewer window. Stage labels keep a stable projected size as the camera zooms. Mouse-wheel zoom supports the same extended range. On mobile, one-finger drag zooms, two-finger drag pans, pinching zooms toward the point between the fingers, twisting two fingers rotates around the scene, and three-finger drag adjusts yaw and pitch. The browser uses WebGPU with asynchronous readback; native presentation uses Vulkan sharing where available. Const declarations and references use stable name colors, with note identities recovered from exact runtime source contexts when the producer does not supply them. `?mini=1` starts with the earlier mini-notation synth demos. `?hub=wss%3A%2F%2FYOUR_SERVER` connects a native producer instead; HTTPS pages require a secure WebSocket.

## Rustel Studio

Open the same site with `?studio` to run upstream Rustel Studio entirely in the browser. Its real Ratatui editor and menus render through a WASM cell backend, with browser audio, inline scope/piano roll/spectrum, local drafts and scenes, file import/download, a mobile keyboard button and fullscreen. This route does not require WebGPU or a server. Native plugins, device APIs and filesystem workers still need browser-specific adapters. See [the Studio port, build and tests](wasm-studio/README.md).

The WASM viewer also has a **Code & Console** panel. Edit the current score and press **Run** (Ctrl/Command+Enter) to evaluate it in Rustel's worker, or **Stop** to pause playback. The panel keeps a local draft and shows sample loading, evaluation errors and Rustel logs. Use `console.log('message')` for literal text; double-quoted strings follow Strudel's mini-notation rules. **Load current score** replaces the draft after confirmation. Messages are capped at 200 entries.

## Native

```powershell
python scripts/prepare.py
cargo run --release --locked -- --hub=127.0.0.1:9612
# Or let GPUI own an upstream Rustel CLI process:
python upstream-adapter/build_runtime.py
cargo run --release --locked -- --rustel=FULL/PATH/TO/rustel.exe --score=FULL/PATH/TO/demos/Counterpoint.strudel
```

Hub mode receives the compatible music-link protocol used by [bespoke_rustel](https://github.com/davehorner/bespoke_rustel). Owned mode starts the upstream CLI, reads NDJSON and sends acknowledged controls, with child cleanup at app shutdown. Use `--silent` for validation without a device. Native presentation defaults to Vulkan sharing on supported Windows adapters with readback fallback. The score is never rewritten by the viewer. See [the adapter notes](upstream-adapter/README.md).

## Desktop scene MCP

Desktop Rustel enables the same Nightshade viewer MCP endpoint on **http://127.0.0.1:9620**. Use `--agent-port=PORT` to change it or `--no-agent` to disable it. It binds to loopback only. `tools/list` gives the authoritative scene-tool catalogue; the endpoint uses the viewer's existing POST JSON-RPC transport, not a new Rustel server or a full modern Streamable HTTP transport.

```powershell
Invoke-RestMethod http://127.0.0.1:9620 -Method Post -ContentType application/json -Body '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
```

Run `python scripts/check_mcp.py` with desktop Rustel open to verify initialization, tool discovery, procedural scene discovery and a live transform query.

Scene queries and edits operate on the actual music-stage world. Procedural entities are added to the viewer catalogue and their rows refresh twice per second. Rustel regenerates note bars, fox transforms and the music camera, so edits to those generated values can be overwritten on subsequent updates. Viewer animation tools affect scene animation. The separate `*_music_*` tools below control the real music runtime. The WASM/static Pages app does not run a native HTTP MCP server. Port-binding failures remain viewer notices; the startup line reports the requested address, not confirmation that binding succeeded.

## Music MCP and desktop editor

The same desktop endpoint advertises `get_music_state`, `get_music_score`, `get_music_messages`, `set_music_transport`, `set_music_tempo`, `set_music_lane_transpose`, `set_music_lane_solo`, `select_music_demo`, and `execute_music_score`.

Read state first. Mutations require its exact `session` and `generation`; stale generations, invalid arguments and unsupported pause/seek requests fail before enqueueing. A successful mutation waits for its own runtime acknowledgement (or, for score loads, the new score snapshot). This confirms runtime control handling, not audible output or GPU presentation. An acknowledgement timeout reports an unknown outcome: query state before retrying. Hosted native Play restarts the owned process; pause/seek remain unavailable on that backend. Hub connections use the capabilities their producer advertises. Arbitrary source execution requires an owned runtime.

Desktop **Code & Console** opens a native GPUI multiline editor and the latest 200 runtime messages (newest first). **Run** or **Ctrl+Enter** validates and executes a private sibling `.js` score file, preserving the original and relative asset base directory. Invalid edits leave the previous score playing. Validation has an eight-second deadline; loading a valid edit restarts the owned runtime. The temporary files are removed at normal shutdown. The editor automatically follows new running source from MCP, demo changes and successful Run actions. Playback ticks leave the buffer and cursor alone. If a running-source change replaces unsaved text, **Restore previous draft** recovers the previous buffer. **Load current score** replaces the editor buffer; **Stop** stops playback. Editing is kept in memory while the window is open; use the original source files for permanent saving. This does not add a desktop editor to external hub producers.

`python scripts/check_music_mcp.py` exercises real transport acknowledgements, stale/unsupported controls, score execution, error recovery and messages, then restores the initial demo. Run it against an owned desktop runtime.

## Publishing

`Publish WASM to GitHub Pages` builds the GPUI application and JavaScript-enabled Rustel engine from pinned source and deploys on pushes to main/master or manual dispatch. Project-relative asset URLs work under `/gpui-rustel/`. The single-threaded browser app starts directly over HTTPS without waiting for an isolation service worker. Remote samples require CORS. WebAudio autoplays when the browser permits; otherwise Play unlocks it.

Expected GitHub Pages entry: https://davehorner.github.io/gpui-rustel/ (redirects to the account's custom Pages domain when configured).

## Tests

```powershell
cargo test --locked
# Optional real GPU scene, skinning and pause check (Vulkan required):
cargo test --lib stage::gpu_tests -- --ignored --nocapture
python wasm-audio/build.py --target nodejs --out target/rustel-browser-audio-test
node wasm-audio/test-javascript.cjs
node wasm-audio/test-wasm.cjs
```

Open `/audio-validation.html` for the shipped host/worker/worklet browser integration check. Its master device gain is muted while validating played PCM before gain. Native plugin, MIDI hardware, Csound and filesystem integrations are not available in the browser host.

## Source and licenses

GPUI integration is MIT OR Apache-2.0. Rustel engine and `wasm-audio` are AGPL-3.0-or-later, with source pin, patches, build scripts and license supplied in the downloadable web bundle. QuickJS and WASM libc notices are included. Music protocol and broker dependency attribution is recorded in [vendor/README.md](vendor/README.md).

The embedded Fox asset and its animation/conversion credits are listed in [assets/README.md](assets/README.md).

### Android and constrained GPUs

Phones use an aspect-preserving Nightshade render budget of 960x640 at up to
25 frames/second with two readback slots. Every lane keeps its animated fox.
`?safe=1` selects 640x480 at up to 15 frames/second on any device. Slow GPU
readback applies backpressure instead of accumulating render submissions.
Matching audio device rates reuse the initially prepared score and sample banks.
JavaScript and viewer WASM ship together under a content-versioned directory;
startup panics preserve the original Rust message in the visible error panel.

The browser material texture arrays are bounded to 256-pixel layers and 64 layers
per color space (about 43 MiB total with mipmaps), replacing the desktop-sized
2.7 GiB reservation. Native material texture sizing is unchanged.

For startup failures, open `?diagnostics=1` and use **Copy diagnostics** before
starting the app. It pauses before GPU calls or loading either WASM engine.
`diagnostics.html` reads the saved startup stage without GPU calls.
`gpu-check.html` explicitly tests copy/readback, indirect drawing and simultaneous
GPU devices without a scene, GPUI, WASM or audio. Reports are copied manually.
On narrow screens the stage uses the full width; **Show code** opens the source
panel below it. A desktop test at phone dimensions does not validate a phone GPU.

The browser renderer uses an 8 MiB SDF atlas (desktop: 343 MiB), smaller lookup
buffers and reduced environment-lighting filter budgets. Initial rendering
completion is checked before copying the first frame to isolate device losses.

Mobile browsers use the renderer's compact material profile, preserving the
animated per-lane foxes, const tints and alpha while avoiding the full advanced
PBR fragment shader. Desktop/native retain full shading. Browser overrides are
`?materials=compact` and `?materials=full`; `?safe=1` also selects compact materials
unless full is explicitly requested. See the renderer README for profile limits.

`mesh-check.html` reproduces the captured compact opaque mesh pipeline without
WASM/audio and compares actual vertex/fragment stages, target state and sampler
variants using small zero-filled resources. It preserves per-case GPU errors.

The browser mesh path reconstructs affine UVs and transparency depth from
perspective varyings to work around the reproduced Adreno pipeline failure.
`interpolation-check.html` checks equivalence against hardware-linear interpolation.
