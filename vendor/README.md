# Source dependencies

`music-link` is the shared music protocol copied from https://github.com/davehorner/bespoke_rustel (music-link folder). Only source, tests and manifests are copied; runtime recordings, configuration and build outputs are excluded.

Hearsay is a Git dependency pinned to upstream commit `033cf8ba3ef87614f45ac397d5cb940898433a5d` (package version 0.3.0): https://github.com/matthewjberger/hearsay/tree/033cf8ba3ef87614f45ac397d5cb940898433a5d. The former vendored copy's 30 files matched that revision after line-ending normalization; no local changes were required. The application, music-link and upstream-adapter use the same pin. MIT and Apache notices are retained in `../licenses/hearsay` and copied into the web distribution.

`gpui-nightshade` is a pinned submodule of https://github.com/davehorner/gpui-nightshade. Run `python scripts/prepare.py` to initialize its pinned renderer/component sources and integration patches.
