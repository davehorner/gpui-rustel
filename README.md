# GPUI Rustel

A music-first GPUI application with native Nightshade 3D and a self-contained browser WASM audio host. The renderer integration lives in [gpui-nightshade](https://github.com/davehorner/gpui-nightshade), pinned here as a submodule. No sibling bespoke_rustel checkout is required to build.

## Browser

```powershell
python scripts/prepare.py
rustup toolchain install 1.100.0 --target wasm32-unknown-unknown
rustup toolchain install nightly-2026-10-31 --target wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked --root target/wasm-tools
python web/build.py
python web/serve.py
```

Open http://127.0.0.1:18766/ and click **Play**. The dropdown includes the same JavaScript `.strudel` sources as the native app: Arpeggio, Counterpoint, Many lanes, Bachata, Sample bank and Custom samples. QuickJS, scheduling and actual Rustel DSP run in WASM, with sample-bank fetching, browser decoding and General MIDI soundfont zones. No native engine process is required. See [audio architecture, limitations and tests](wasm-audio/README.md).

The browser visualizes note geometry with GPUI paths; the native app uses Nightshade's real 3D renderer. The complete GPUI Nightshade editor's WASM build is in the separate renderer repository. `?mini=1` starts with the earlier mini-notation synth demos. `?hub=wss%3A%2F%2FYOUR_SERVER` connects a native producer instead; HTTPS pages require a secure WebSocket.

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

`Publish WASM to GitHub Pages` builds the GPUI application and JavaScript-enabled Rustel engine from pinned source and deploys on pushes to main/master or manual dispatch. Project-relative asset URLs work under `/gpui-rustel/`. A same-origin service worker supplies isolation headers for graphics on static Pages hosting. Remote samples require CORS. WebAudio starts only after a user gesture.

Expected GitHub Pages entry: https://davehorner.github.io/gpui-rustel/ (redirects to the account's custom Pages domain when configured).

## Tests

```powershell
cargo test --locked
python wasm-audio/build.py --target nodejs --out target/rustel-browser-audio-test
node wasm-audio/test-javascript.cjs
node wasm-audio/test-wasm.cjs
```

Open `/audio-validation.html` for the shipped host/worker/worklet browser integration check. Its master device gain is muted while validating played PCM before gain. Native plugin, MIDI hardware, Csound and filesystem integrations are not available in the browser host.

## Source and licenses

GPUI integration is MIT OR Apache-2.0. Rustel engine and `wasm-audio` are AGPL-3.0-or-later, with source pin, patches, build scripts and license supplied in the downloadable web bundle. QuickJS and WASM libc notices are included. Vendored protocol and broker attribution is recorded in [vendor/README.md](vendor/README.md).
