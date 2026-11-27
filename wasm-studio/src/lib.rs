use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::style::Color;
use rustel_transpiler::on_caller_stack;
use serde_json::json;
use wasm_bindgen::prelude::*;
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn log_panic(message: &str);
}
fn error(e: String) -> JsValue {
    JsValue::from_str(&e)
}
fn modifiers(bits: u8) -> KeyModifiers {
    KeyModifiers::from_bits_truncate(bits)
}
#[wasm_bindgen]
pub struct Studio {
    inner: rustel_studio::app::BrowserStudio,
}
#[wasm_bindgen]
impl Studio {
    #[wasm_bindgen(constructor)]
    pub fn new(source: &str, state: Option<String>) -> Result<Studio, JsValue> {
        std::panic::set_hook(Box::new(|info| log_panic(&info.to_string())));
        Ok(Self {
            inner: on_caller_stack(|| {
                rustel_studio::app::BrowserStudio::new(source, state.as_deref())
            })
            .map_err(error)?,
        })
    }
    pub fn play(&mut self) -> Result<(), JsValue> {
        on_caller_stack(|| self.inner.play()).map_err(error)
    }
    pub fn stop(&mut self) -> Result<(), JsValue> {
        on_caller_stack(|| self.inner.stop()).map_err(error)
    }
    pub fn state(&self) -> String {
        self.inner.state()
    }
    pub fn clipboard(&mut self) -> String {
        self.inner.clipboard()
    }
    pub fn clipboard_status(&mut self, message: &str) {
        self.inner.clipboard_status(message);
    }
    pub fn gain(&self) -> f32 {
        self.inner.gain()
    }
    pub fn source(&self) -> String {
        self.inner.source()
    }
    pub fn paste(&mut self, text: &str) -> Result<(), JsValue> {
        on_caller_stack(|| self.inner.event(Event::Paste(text.to_owned()))).map_err(error)
    }
    pub fn key(&mut self, key: &str, bits: u8) -> Result<(), JsValue> {
        let code = match key {
            "Enter" => KeyCode::Enter,
            "Escape" => KeyCode::Esc,
            "Backspace" => KeyCode::Backspace,
            "Tab" if bits & 1 != 0 => KeyCode::BackTab,
            "Tab" => KeyCode::Tab,
            "Delete" => KeyCode::Delete,
            "Insert" => KeyCode::Insert,
            "ArrowLeft" => KeyCode::Left,
            "ArrowRight" => KeyCode::Right,
            "ArrowUp" => KeyCode::Up,
            "ArrowDown" => KeyCode::Down,
            "Home" => KeyCode::Home,
            "End" => KeyCode::End,
            "PageUp" => KeyCode::PageUp,
            "PageDown" => KeyCode::PageDown,
            _ => {
                if key.starts_with('F') && key[1..].parse::<u8>().is_ok() {
                    KeyCode::F(key[1..].parse().unwrap())
                } else if key.chars().count() == 1 {
                    KeyCode::Char(key.chars().next().unwrap())
                } else {
                    return Ok(());
                }
            }
        };
        on_caller_stack(|| {
            self.inner
                .event(Event::Key(KeyEvent::new(code, modifiers(bits))))
        })
        .map_err(error)
    }
    pub fn mouse(&mut self, kind: &str, x: u16, y: u16, bits: u8) -> Result<(), JsValue> {
        let kind = match kind {
            "down" => MouseEventKind::Down(MouseButton::Left),
            "up" => MouseEventKind::Up(MouseButton::Left),
            "drag" => MouseEventKind::Drag(MouseButton::Left),
            "scroll-up" => MouseEventKind::ScrollUp,
            "scroll-down" => MouseEventKind::ScrollDown,
            _ => MouseEventKind::Moved,
        };
        on_caller_stack(|| {
            self.inner.event(Event::Mouse(MouseEvent {
                kind,
                column: x,
                row: y,
                modifiers: modifiers(bits),
            }))
        })
        .map_err(error)
    }
    pub fn commands(&mut self) -> String {
        self.inner.commands()
    }
    pub fn evaluated(
        &mut self,
        request: f64,
        revision: f64,
        source: &str,
        generation: f64,
        message: Option<String>,
    ) {
        on_caller_stack(|| {
            self.inner.evaluated(
                request as u64,
                revision as u64,
                source,
                generation as u64,
                message,
            )
        });
    }
    pub fn transport(&mut self, playing: bool, generation: f64, cycle: f64, cps: f64) {
        self.inner.transport(playing, generation as u64, cycle, cps);
    }
    pub fn events(
        &mut self,
        json: &str,
        generation: f64,
        time: f64,
        cps: f64,
    ) -> Result<(), JsValue> {
        self.inner
            .events(json, generation as u64, time, cps)
            .map_err(error)
    }
    pub fn audio(
        &mut self,
        json: &str,
        generation: f64,
        sequence: f64,
        time: f64,
        rate: u32,
    ) -> Result<(), JsValue> {
        self.inner
            .audio(json, generation as u64, sequence as u64, time, rate)
            .map_err(error)
    }
    pub fn frame(&mut self, width: u16, height: u16) -> Result<String, JsValue> {
        let buffer = on_caller_stack(|| {
            self.inner
                .render(width.clamp(20, 300), height.clamp(10, 120))
        })
        .map_err(error)?;
        let cells = buffer
            .content
            .iter()
            .map(|c| json!([c.symbol(), color(c.fg), color(c.bg), c.modifier.bits()]))
            .collect::<Vec<_>>();
        Ok(json!({"width":buffer.area.width,"height":buffer.area.height,"cells":cells,"cursor":self.inner.cursor()}).to_string())
    }
}
fn color(value: Color) -> String {
    match value {
        Color::Reset => String::new(),
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Indexed(n) => {
            if n < 16 {
                palette(n as usize).into()
            } else if n >= 232 {
                let v = 8 + (n - 232) * 10;
                format!("#{v:02x}{v:02x}{v:02x}")
            } else {
                let n = n - 16;
                let v = |x: u8| if x == 0 { 0 } else { 55 + x * 40 };
                format!("#{:02x}{:02x}{:02x}", v(n / 36), v(n / 6 % 6), v(n % 6))
            }
        }
        _ => palette(match value {
            Color::Black => 0,
            Color::Red => 1,
            Color::Green => 2,
            Color::Yellow => 3,
            Color::Blue => 4,
            Color::Magenta => 5,
            Color::Cyan => 6,
            Color::Gray => 7,
            Color::DarkGray => 8,
            Color::LightRed => 9,
            Color::LightGreen => 10,
            Color::LightYellow => 11,
            Color::LightBlue => 12,
            Color::LightMagenta => 13,
            Color::LightCyan => 14,
            _ => 15,
        })
        .into(),
    }
}
fn palette(i: usize) -> &'static str {
    [
        "#101216", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#abb2bf",
        "#5c6370", "#ff7a85", "#b3e08d", "#ffda93", "#8ac7ff", "#dfa0ff", "#82e0e8", "#ffffff",
    ][i]
}

// QuickJS uses the browser clock rather than an unresolved native OS import.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn __rquickjs_host_now_us() -> f64 {
    js_sys::Date::now() * 1000.
}

#[wasm_bindgen]
pub fn memory_bytes() -> u32 {
    let memory: js_sys::WebAssembly::Memory = wasm_bindgen::memory().unchecked_into();
    let buffer: js_sys::ArrayBuffer = memory.buffer().unchecked_into();
    buffer.byte_length()
}
