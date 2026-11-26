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

Both targets use the same Nightshade stage: named-part colors, time/cycle and pitch/MIDI axes, lane labels, and an embedded skinned Fox for every visible lane. Each fox uses its const color; its Run clip and box-to-box jump arc follow the music clock and freeze on pause. The left camera panel provides Fit/Top/Front/Side views, rotation, pan, deep zoom, Fox focus and 35/65/100% fox opacity. Mouse-wheel zoom supports the same extended range. The browser uses WebGPU with asynchronous readback; native presentation uses Vulkan sharing where available. Const declarations and references use stable name colors, with note identities recovered from exact runtime source contexts when the producer does not supply them. `?mini=1` starts with the earlier mini-notation synth demos. `?hub=wss%3A%2F%2FYOUR_SERVER` connects a native producer instead; HTTPS pages require a secure WebSocket.

## Native

```powershell
python scripts/prepare.py
cargo run --release --locked -- --hub=127.0.0.1:9612
# Or let GPUI own an upstream Rustel CLI process:
python upstream-adapter/build_runtime.py
cargo run --release --locked -- --rustel=FULL/PATH/TO/rustel.exe --score=FULL/PATH/TO/demos/Counterpoint.strudel
```

Hub mode receives the compatible music-link protocol used by [bespoke_rustel](https://github.com/davehorner/bespoke_rustel). Owned mode starts the upstream CLI, reads NDJSON and sends acknowledged controls, with child cleanup at app shutdown. Use `--silent` for validation without a device. Native presentation defaults to Vulkan sharing on supported Windows adapters with readback fallback. The score is never rewritten by the viewer. See [the adapter notes](upstream-adapter/README.md).

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

GPUI integration is MIT OR Apache-2.0. Rustel engine and `wasm-audio` are AGPL-3.0-or-later, with source pin, patches, build scripts and license supplied in the downloadable web bundle. QuickJS and WASM libc notices are included. Vendored protocol and broker attribution is recorded in [vendor/README.md](vendor/README.md).

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
