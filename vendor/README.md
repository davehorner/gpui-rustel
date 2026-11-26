# Source dependencies

`music-link` is the shared music protocol from https://github.com/davehorner/bespoke_rustel (music-link folder).
`hearsay` is the compatible broker checkout used by that app, from https://github.com/matthewjberger/hearsay; its MIT and Apache licenses are included.
Only source, tests, and manifests are copied. Runtime recordings, scores, configuration and build outputs are excluded.

`gpui-nightshade` is a pinned submodule of https://github.com/davehorner/gpui-nightshade. Run `python scripts/prepare.py` to initialize its pinned renderer/component sources and integration patches.
