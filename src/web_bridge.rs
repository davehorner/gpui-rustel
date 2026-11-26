//! Browser transport for the same Hearsay music protocol used by the desktop app.
pub use crate::state::*;
use music_link::{AudioFrame, Command, Response, Snapshot};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Mutex},
    time::Duration,
};
use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{BinaryType, MessageEvent, WebSocket};
use web_time::Instant;

struct Connection {
    socket: WebSocket,
    _open: Closure<dyn FnMut()>,
    _message: Closure<dyn FnMut(MessageEvent)>,
    _close: Closure<dyn FnMut()>,
    _error: Closure<dyn FnMut()>,
}
impl Drop for Connection {
    fn drop(&mut self) {
        self.socket.set_onopen(None);
        self.socket.set_onmessage(None);
        self.socket.set_onclose(None);
        self.socket.set_onerror(None);
        let _ = self.socket.close();
    }
}
fn connect(address: &str, state: Shared) -> std::io::Result<Connection> {
    let socket =
        WebSocket::new(address).map_err(|e| std::io::Error::other(format!("WebSocket: {e:?}")))?;
    socket.set_binary_type(BinaryType::Arraybuffer);
    let opened = socket.clone();
    let opened_state = state.clone();
    let open = Closure::<dyn FnMut()>::new(move || {
        for frame in [
            hearsay::encode_hello("gpui-rustel-web"),
            hearsay::encode_subscribe(music_link::SNAPSHOT_TOPIC),
            hearsay::encode_subscribe(music_link::AUDIO_TOPIC),
            hearsay::encode_subscribe(music_link::RESPONSE_TOPIC),
        ] {
            if !frame.is_ok_and(|frame| opened.send_with_u8_array(&frame).is_ok()) {
                opened_state.lock().unwrap().status = "Music subscription failed".into();
                let _ = opened.close();
                return;
            }
        }
        opened_state.lock().unwrap().status = "Connected · waiting for Rustel".into();
    });
    socket.set_onopen(Some(open.as_ref().unchecked_ref()));
    let incoming = state.clone();
    let message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        let Ok(buffer) = event.data().dyn_into::<js_sys::ArrayBuffer>() else {
            return;
        };
        if buffer.byte_length() > 2 * 1024 * 1024 {
            return;
        }
        let Some(message) = hearsay::decode_message(hearsay::Bytes::from(
            js_sys::Uint8Array::new(&buffer).to_vec(),
        )) else {
            return;
        };
        let Some(text) = message.text() else { return };
        let mut state = incoming.lock().unwrap();
        if message.topic() == music_link::SNAPSHOT_TOPIC {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(text)
                && let Ok(snapshot) = serde_json::from_value::<Snapshot>(value.clone())
            {
                let id = snapshot.session.clone();
                let caps = value
                    .get("capabilities")
                    .cloned()
                    .and_then(|v| serde_json::from_value(v).ok());
                state.receive_snapshot(snapshot);
                if state.sessions.contains_key(&id) {
                    if let Some(caps) = caps {
                        state.capabilities.insert(id, caps);
                    } else {
                        state.capabilities.remove(&id);
                    }
                }
            }
        } else if message.topic() == music_link::AUDIO_TOPIC {
            if let Ok(audio) = serde_json::from_str::<AudioFrame>(text) {
                state.receive_audio(audio);
            }
        } else if message.topic() == music_link::RESPONSE_TOPIC
            && let Ok(response) = serde_json::from_str::<Response>(text)
            && state.pending.remove(&response.request).is_some()
        {
            state.feedback = if response.accepted {
                "Applied".into()
            } else {
                response.message
            };
        }
    });
    socket.set_onmessage(Some(message.as_ref().unchecked_ref()));
    let closed = state.clone();
    let close = Closure::<dyn FnMut()>::new(move || {
        let mut state = closed.lock().unwrap();
        state.sessions.clear();
        state.audio.clear();
        state.pending.clear();
        state.capabilities.clear();
        state.status = "Music hub disconnected · reconnecting".into();
    });
    socket.set_onclose(Some(close.as_ref().unchecked_ref()));
    let error = Closure::<dyn FnMut()>::new(move || {
        state.lock().unwrap().status = "Cannot reach music hub · reconnecting".into();
    });
    socket.set_onerror(Some(error.as_ref().unchecked_ref()));
    Ok(Connection {
        socket,
        _open: open,
        _message: message,
        _close: close,
        _error: error,
    })
}
thread_local! {static LOCAL:RefCell<Option<std::sync::Weak<Mutex<State>>>>=const {RefCell::new(None)};}
pub fn receive_local(topic: &str, text: &str) {
    LOCAL.with(|slot| {
        let Some(shared) = slot.borrow().as_ref().and_then(|s| s.upgrade()) else {
            return;
        };
        let mut state = shared.lock().unwrap();
        match topic {
            "music/status" => state.status = text.chars().take(512).collect(),
            music_link::SNAPSHOT_TOPIC => {
                if text.len() <= 2 * 1024 * 1024
                    && let Ok(value) = serde_json::from_str::<serde_json::Value>(text)
                    && let Ok(snapshot) = serde_json::from_value::<Snapshot>(value.clone())
                {
                    let id = snapshot.session.clone();
                    let caps = value
                        .get("capabilities")
                        .cloned()
                        .and_then(|v| serde_json::from_value(v).ok());
                    state.receive_snapshot(snapshot);
                    if state.sessions.contains_key(&id)
                        && let Some(caps) = caps
                    {
                        state.capabilities.insert(id, caps);
                    }
                }
            }
            music_link::AUDIO_TOPIC => {
                if text.len() <= 512 * 1024
                    && let Ok(frame) = serde_json::from_str::<AudioFrame>(text)
                {
                    state.receive_audio(frame);
                }
            }
            music_link::RESPONSE_TOPIC => {
                if let Ok(response) = serde_json::from_str::<Response>(text)
                    && state.pending.remove(&response.request).is_some()
                {
                    state.feedback = if response.accepted {
                        "Applied".into()
                    } else {
                        response.message
                    };
                }
            }
            _ => {}
        }
    });
}
pub struct Bridge {
    pub shared: Shared,
    address: String,
    connection: Rc<RefCell<Option<Connection>>>,
    attempted: Instant,
    serial: u64,
    prefix: String,
}
impl Bridge {
    pub fn connect(address: String) -> std::io::Result<Self> {
        if address == "browser" {
            let shared = Arc::new(Mutex::new(State {
                status: "Preparing browser-owned Rustel audio".into(),
                ..Default::default()
            }));
            LOCAL.with(|slot| *slot.borrow_mut() = Some(Arc::downgrade(&shared)));
            return Ok(Self {
                shared,
                address,
                connection: Rc::new(RefCell::new(None)),
                attempted: Instant::now(),
                serial: 0,
                prefix: format!("local-{}", js_sys::Date::now()),
            });
        }
        if !address.starts_with("ws://") && !address.starts_with("wss://") {
            return Err(std::io::Error::other("Use a ws:// or wss:// music hub URL"));
        }
        let shared = Arc::new(Mutex::new(State {
            status: format!("Connecting to {address}"),
            ..Default::default()
        }));
        let connection = connect(&address, shared.clone())?;
        Ok(Self {
            shared,
            address,
            connection: Rc::new(RefCell::new(Some(connection))),
            attempted: Instant::now(),
            serial: 0,
            prefix: format!("web-{}-{}", js_sys::Date::now(), js_sys::Math::random()),
        })
    }
    pub fn tick(&mut self) {
        self.shared.lock().unwrap().prune();
        if self.address == "browser" {
            return;
        }
        if self
            .connection
            .borrow()
            .as_ref()
            .is_none_or(|c| c.socket.ready_state() == WebSocket::CLOSED)
            && self.attempted.elapsed() >= Duration::from_secs(1)
        {
            self.attempted = Instant::now();
            match connect(&self.address, self.shared.clone()) {
                Ok(connection) => *self.connection.borrow_mut() = Some(connection),
                Err(error) => self.shared.lock().unwrap().status = error.to_string(),
            }
        }
    }
    pub fn is_hosted(&self) -> bool {
        false
    }
    pub fn restart_runtime(&self) {}
    pub fn send(&mut self, snapshot: &Snapshot, mut command: Command) {
        self.serial += 1;
        command.request = format!("{}-{}", self.prefix, self.serial);
        command.session = snapshot.session.clone();
        command.generation = snapshot.generation;
        let mut state = self.shared.lock().unwrap();
        if state.sessions.get(&snapshot.session).is_none_or(|s| {
            s.at.elapsed() >= Duration::from_secs(3) || s.value.generation != snapshot.generation
        }) {
            state.feedback = "Session is stale".into();
            return;
        }
        if let Err(error) = command.validate(snapshot) {
            state.feedback = error.into();
            return;
        }
        if state.pending.len() >= 64 {
            state.feedback = "Too many pending commands".into();
            return;
        }
        if self.address == "browser" {
            let request = command.request.clone();
            state
                .pending
                .insert(request.clone(), Instant::now() + Duration::from_secs(65));
            state.feedback = "Applying…".into();
            drop(state);
            let result = (|| {
                let text = serde_json::to_string(&command).map_err(|e| e.to_string())?;
                let init = web_sys::CustomEventInit::new();
                init.set_detail(&wasm_bindgen::JsValue::from_str(&text));
                let event = web_sys::CustomEvent::new_with_event_init_dict("rustel-command", &init)
                    .map_err(|e| format!("{e:?}"))?;
                web_sys::window()
                    .ok_or("Browser window unavailable")?
                    .dispatch_event(&event)
                    .map_err(|e| format!("{e:?}"))?;
                Ok::<(), String>(())
            })();
            if let Err(error) = result {
                let mut state = self.shared.lock().unwrap();
                state.pending.remove(&request);
                state.feedback = error;
            }
            return;
        }
        let connection = self.connection.borrow();
        let result = (|| {
            let connection = connection.as_ref().ok_or("Music hub disconnected")?;
            if connection.socket.ready_state() != WebSocket::OPEN
                || connection.socket.buffered_amount() > 256 * 1024
            {
                return Err("Music connection is not ready");
            }
            let text = serde_json::to_string(&command).map_err(|_| "Command encoding failed")?;
            let frame = hearsay::encode_publish_text(music_link::COMMAND_TOPIC, &text, false)
                .map_err(|_| "Command encoding failed")?;
            connection
                .socket
                .send_with_u8_array(&frame)
                .map_err(|_| "Command could not reach hub")
        })();
        match result {
            Ok(()) => {
                state
                    .pending
                    .insert(command.request, Instant::now() + Duration::from_secs(5));
                state.feedback = "Applying…".into();
            }
            Err(error) => state.feedback = error.into(),
        }
    }
}
pub fn command() -> Command {
    Command {
        demo: None,
        tempo: None,
        source_update: None,
        transport: None,
        request: String::new(),
        session: String::new(),
        generation: 0,
        lane: String::new(),
        semitones: 0,
        solo: None,
    }
}
