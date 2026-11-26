use crate::{app::MusicApp, bridge::Bridge, model::Visual};
use gpui_kit::*;
use std::{
    borrow::Cow,
    cell::RefCell,
    sync::{Arc, Mutex},
};
use wasm_bindgen::prelude::*;
gpui_kit::assets::icon_assets!(
    WebAssets,
    [
        ChevronDown,
        ChevronUp,
        ChevronLeft,
        ChevronRight,
        Check,
        X,
        Search
    ]
);
thread_local! {static APPLICATION:RefCell<Option<ApplicationHandle>>=const {RefCell::new(None)};}
#[wasm_bindgen]
pub fn run(hub: Option<String>) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);
    gpui_kit::platform::web_init();
    let bridge = Bridge::connect(hub.unwrap_or_else(|| "ws://127.0.0.1:9613".into()))
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    let settings = Arc::new(Mutex::new(Visual::default()));
    let app = gpui_kit::platform::single_threaded_web().with_assets(WebAssets);
    let handle=app.run_embedded(move |cx| {
        gpui_kit::init(cx);
        cx.text_system().add_fonts(vec![
            Cow::Borrowed(include_bytes!("../vendor/gpui-nightshade/upstream/gpui-component/crates/story-web/fonts/Inter-Regular.ttf").as_slice()),
            Cow::Borrowed(include_bytes!("../vendor/gpui-nightshade/upstream/gpui-component/crates/story-web/fonts/IBMPlexSans-Regular.ttf").as_slice()),
            Cow::Borrowed(include_bytes!("../vendor/gpui-nightshade/upstream/gpui-component/crates/story-web/fonts/JetBrainsMono-Regular.ttf").as_slice()),
        ]).expect("load bundled browser fonts");
        component::Theme::change(component::ThemeMode::Dark,None,cx);
        component::Theme::update(cx,|theme| {theme.font_family="Inter Variable".into();theme.mono_font_family="JetBrains Mono".into();});
        gpui_kit::open_window(WindowOptions::default(),cx,move |window,cx| {
            let shared=bridge.shared.clone();let visual=settings.clone();
            let factory:gpui_nightshade_components::SceneFactory=Arc::new(move |b,w,h| {
                Box::pin(crate::stage::Stage::open_async(shared.clone(),visual.clone(),b,w,h))
            });
            let viewport=cx.new(|cx|gpui_nightshade_components::NightshadeViewport::from_factory(gpui_nightshade_components::BackendPreference::Auto,factory,cx));
            cx.new(|cx|MusicApp::new(bridge,settings,viewport,window,cx))
        }).expect("open GPUI browser viewer");
        cx.activate(true);
    });
    APPLICATION.with(|a| *a.borrow_mut() = Some(handle));
    Ok(())
}

/// Receives browser-owned audio messages using the same validated music protocol.
#[wasm_bindgen]
pub fn local_message(topic: &str, json: &str) {
    crate::bridge::receive_local(topic, json);
}
