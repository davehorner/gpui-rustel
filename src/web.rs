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
thread_local! {static FOXY_SETTINGS:RefCell<Option<crate::model::Settings>>=const {RefCell::new(None)};}
thread_local! {static APPLICATION:RefCell<Option<ApplicationHandle>>=const {RefCell::new(None)};}
#[wasm_bindgen(
    inline_js = "export function reportRustelPanic(message) { console.error(message); queueMicrotask(() => window.dispatchEvent(new CustomEvent('rustel-panic', {detail: message}))); }"
)]
extern "C" {
    #[wasm_bindgen(js_name=reportRustelPanic)]
    fn report_panic(message: &str);
}
#[wasm_bindgen]
pub fn run(hub: Option<String>) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    std::panic::set_hook(Box::new(|info| report_panic(&info.to_string())));
    let _ = console_log::init_with_level(log::Level::Info);
    gpui_kit::platform::web_init();
    let bridge = Bridge::connect(hub.unwrap_or_else(|| "ws://127.0.0.1:9613".into()))
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    let settings = Arc::new(Mutex::new(Visual::default()));
    let embedded = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .is_some_and(|s| s.contains("inlinefoxy=1"));
    settings.lock().unwrap().embedded = embedded;
    settings.lock().unwrap().foxy_active = embedded;
    FOXY_SETTINGS.with(|s| *s.borrow_mut() = Some(settings.clone()));
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

/// Configure the one audio-free stage embedded by Studio.
#[wasm_bindgen]
pub fn foxy_filter(slot: Option<u8>) {
    FOXY_SETTINGS.with(|s| {
        if let Some(settings) = s.borrow().as_ref() {
            settings.lock().unwrap().foxy_slot = slot.filter(|s| *s < 64);
        }
    });
}

#[wasm_bindgen]
pub fn foxy_visible(visible: bool) {
    FOXY_SETTINGS.with(|s| {
        if let Some(settings) = s.borrow().as_ref() {
            settings.lock().unwrap().embedded_visible = visible;
        }
    });
}

/// Receives browser-owned audio messages using the same validated music protocol.
#[wasm_bindgen]
pub fn local_message(topic: &str, json: &str) {
    crate::bridge::receive_local(topic, json);
}
