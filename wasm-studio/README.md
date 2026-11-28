# Rustel Studio in the browser

`?studio` loads the actual upstream Studio application in WebAssembly. Its editor and 2D visuals run without GPUI, Nightshade, WebGPU, a server, or a remote terminal. An explicit `_foxy({mode: "3d"})` or `_all_foxy({mode: "3d"})` lazily loads one audio-free GPUI/Nightshade viewer, sharing Studio's runtime snapshots. The default `_foxy()` and `_all_foxy()` draw light 2D fox tracks.

The source is Rustel `79fd1fb3fd64c0a637f475ad24de675fd5b2ac59`. `BrowserStudio` calls the original `App` input handler and `paint_into`; Ratatui cells, colors and cursor are painted on Canvas2D. The test-only Studio harness is not enabled. The original editor, menus, scenes, selection, undo, syntax, inline widgets and settings remain Rust code.

New workspaces default to **Bachata bruma de canela.strudel**. Saved drafts and explicit demo links take precedence. **Reset Studio** stays in the browser toolbar even when native controls are hidden. After confirmation it restores that default score, clears saved Studio scenes and layout preferences, removes the old demo query, and reloads with factory defaults. Other applications’ saved settings are preserved.

The Zen toolbar button remains reachable when native chrome is hidden. Leaving Zen re-enables the menu, header, footer, line numbers, minimap and scrollbars, and restores visualizer docks that were open before entering Zen.

The browser adapter replaces the terminal loop and native worker boundary. Score evaluation and synthesis use the existing Rustel WASM worker and AudioWorklet. Each visual receiver has its own scalar-audio capture; scope/spectrum are analyzed with upstream Studio's FFT reduction. Master and orbit faders and score sliders go to browser audio. Draft scenes and preferences use local storage; Open/Download handle local files. Browser clipboard, IME input, a mobile keyboard button and fullscreen are handled by the host.

Native plugins, Csound shared libraries, native audio/MIDI device selection, Ableton Link, native recording/export workers and native filesystem browsing are not browser capabilities. The native UI remains present; these host operations require additional browser adapters. Unsupported recording/prebake requests receive failure replies instead of waiting indefinitely. Animated native theme effects are disabled on WASM. This is a browser port of Studio, not a claim of complete native platform parity.

The maintained upstream changes are in `../wasm-audio/patches/browser-portability.patch`. Crossterm is a local MIT-licensed copy of the upstream pinned fork because it needs WASM platform guards; see `vendor/crossterm/BROWSER-PORT.md`. Native terminal behavior is preserved.

Build and check:

```powershell
python wasm-studio/check.py
python wasm-studio/build.py
python wasm-studio/build.py --target nodejs --out target/rustel-browser-studio-test
node wasm-studio/test-studio.cjs
python web/build.py --out target/gpui-rustel-web
```

The whole-site builder publishes versioned Studio assets and the corresponding adapter source alongside the existing engine source and AGPL license. GitHub Pages needs no server-side changes.

The toolbar’s **Demo score** picker uses the same published catalogue as the GPUI/WASM viewer. **Load demo** verifies the score hash and opens it as a new scene, preserving existing scenes and edits. Press **Play** to evaluate it. All scores in the repository’s `demos/` folder are included automatically.

Use `?studio&demo=Bachata%20bruma%20de%20canela.strudel` to open a shared score directly. The extension is optional, and `?sudio` also works. Links load drafts without autoplay or overwriting existing scenes. An unchanged copy is reused on subsequent visits. The demo picker updates the address bar when loading a score.
