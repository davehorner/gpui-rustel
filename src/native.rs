use crate::{app::MusicApp, bridge::Bridge, model::Visual, stage};
use gpui_kit::*;
use gpui_nightshade_components::NightshadeViewport;
use std::sync::{Arc, Mutex};
pub fn run() {
    let mut hub = "127.0.0.1:9612".to_owned();
    let mut executable = None;
    let mut score = None;
    let mut demo_dir = Some(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("demos"));
    let mut silent = false;
    let mut agent_port = Some(9620u16);
    let mut no_agent = false;
    let mut backend = gpui_nightshade::presentation::BackendPreference::Auto;
    for arg in std::env::args().skip(1) {
        if let Some(value) = arg.strip_prefix("--hub=") {
            hub = value.to_owned();
        } else if let Some(value) = arg.strip_prefix("--presentation=") {
            backend = value.parse().expect("valid presentation backend");
        } else if let Some(value) = arg.strip_prefix("--rustel=") {
            executable = Some(std::path::PathBuf::from(value));
        } else if let Some(value) = arg.strip_prefix("--score=") {
            score = Some(std::path::PathBuf::from(value));
        } else if let Some(value) = arg.strip_prefix("--demos=") {
            demo_dir = Some(value.into());
        } else if let Some(value) = arg.strip_prefix("--agent-port=") {
            let port = value
                .parse::<u16>()
                .expect("agent port must be an integer from 1 to 65535");
            assert!(port != 0, "agent port must be from 1 to 65535");
            agent_port = Some(port);
        } else if arg == "--no-agent" {
            no_agent = true;
        } else if arg == "--silent" {
            silent = true;
        } else if arg == "--help" {
            println!(
                "gpui-rustel [--hub=127.0.0.1:9612] [--presentation=auto|vulkan-shared|readback] [--rustel=EXE --score=FILE [--silent]] [--agent-port=9620 | --no-agent]"
            );
            return;
        } else {
            panic!("unknown argument: {arg}");
        }
    }
    let bridge = match (executable, score) {
        (Some(executable), Some(score)) => Bridge::hosted(rustel_ui_adapter::Config {
            executable,
            score,
            silent,
            demo_dir,
        })
        .expect("start owned runtime worker"),
        (None, None) if !silent => Bridge::connect(hub).expect("start music client"),
        _ => panic!(
            "owned mode requires both --rustel=EXE and --score=FILE; --silent applies only to owned mode"
        ),
    };
    let mut visual = Visual::default();
    visual.agent_port = if no_agent { None } else { agent_port };
    let settings = Arc::new(Mutex::new(visual));
    if !no_agent {
        eprintln!(
            "Nightshade scene MCP requested on http://127.0.0.1:{} (POST JSON-RPC)",
            agent_port.unwrap()
        );
    }
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, None, cx);
            let (_, app) = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        point(px(70.), px(60.)),
                        size(px(1280.), px(740.)),
                    ))),
                    ..Default::default()
                },
                cx,
                move |window, cx| {
                    window.set_window_title(if bridge.is_hosted() {
                        "GPUI Rustel — Hosted Upstream"
                    } else {
                        "GPUI Rustel — Live Pattern Field"
                    });
                    let shared = bridge.shared.clone();
                    let visual = settings.clone();
                    let factory: gpui_nightshade_components::SceneFactory =
                        Arc::new(move |b, w, h| {
                            stage::Stage::open(shared.clone(), visual.clone(), b, w, h)
                        });
                    let viewport =
                        cx.new(|cx| NightshadeViewport::from_factory(backend, factory, cx));
                    cx.new(|cx| MusicApp::new(bridge, settings, viewport, window, cx))
                },
            )
            .expect("open native music app");
            // Retain the app through last-window removal so quit can await child cleanup.
            cx.on_app_quit(move |cx| {
                let worker = app.update(cx, |app, _| app.bridge.shutdown());
                cx.background_executor().spawn(async move {
                    if let Some(worker) = worker {
                        let _ = worker.join();
                    }
                })
            })
            .detach();
            cx.activate(true);
        });
}
