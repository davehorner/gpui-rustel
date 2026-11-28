//! The real Studio UI, painted by GPUI and controlled by the existing runtime.
use crate::{
    bridge::{Bridge, command},
    model::Frame,
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton as TerminalButton, MouseEvent,
    MouseEventKind,
};
use gpui_kit::*;
use ratatui::{
    buffer::Buffer,
    style::{Color, Modifier},
};
use rustel_studio::app::BrowserStudio as StudioUi;
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

struct Evaluation {
    remote: Option<(crate::studio_mcp::Reply, u64)>,
    session: String,
    request: u64,
    revision: u64,
    source: String,
    reply: mpsc::Receiver<music_link::Response>,
    started: Instant,
}
pub struct DesktopStudio {
    draft_id: String,
    draft_revision: u64,
    observed_draft: Option<(u64, u64, String)>,
    ui: StudioUi,
    focus: FocusHandle,
    bounds: Bounds<Pixels>,
    cell_width: f32,
    session: String,
    generation: u64,
    running_source: String,
    audio_sequence: Option<(u64, u64)>,
    pending: Option<Evaluation>,
    marked: Option<std::ops::Range<usize>>,
    pub status: String,
}
impl DesktopStudio {
    pub fn new(source: &str, cx: &mut App) -> Result<Self, String> {
        static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let draft_id = format!(
            "studio-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        Ok(Self {
            draft_id,
            draft_revision: 0,
            observed_draft: None,
            ui: StudioUi::new(source, None)?,
            focus: cx.focus_handle(),
            bounds: Bounds::default(),
            cell_width: 8.4,
            session: String::new(),
            generation: 0,
            running_source: source.into(),
            audio_sequence: None,
            pending: None,
            marked: None,
            status: "Studio · existing Rustel runtime".into(),
        })
    }
    pub fn foxy_surface(&self) -> Option<(rustel_studio::visuals::FoxyRegion, f32)> {
        self.ui
            .host_foxy_regions()
            .into_iter()
            .find(|r| r.width >= 10 && r.height >= 4)
            .map(|r| (r, self.cell_width))
    }
    pub fn play(&mut self, cx: &mut Context<Self>) {
        let result = self.ui.play();
        self.event_result(result, cx);
    }
    fn draft_id(&self) -> String {
        self.draft_id.clone()
    }
    fn capture_draft(&mut self, frame: &Frame, open: bool) -> serde_json::Value {
        let (scene, editor_revision, name) = self.ui.host_document();
        let source = self.ui.source();
        let observed = (scene, editor_revision, source.clone());
        if self.observed_draft.as_ref() != Some(&observed) {
            self.draft_revision = self.draft_revision.saturating_add(1);
            self.observed_draft = Some(observed);
        }
        serde_json::json!({"draft_id":self.draft_id(),"revision":self.draft_revision,"source":source,"scene":name,"scene_id":scene,"editor_revision":editor_revision,"dirty":source!=self.running_source,"open":open,"execution_pending":self.pending.is_some(),"running_session":frame.snapshot.as_ref().map(|s|&s.value.session),"running_generation":frame.snapshot.as_ref().map(|s|s.value.generation)})
    }
    pub fn mcp_request(
        &mut self,
        request: crate::studio_mcp::Request,
        frame: &Frame,
        bridge: &mut Bridge,
        open: bool,
        cx: &mut Context<Self>,
    ) {
        let crate::studio_mcp::Request { name, args, reply } = request;
        if !reply.active() {
            return;
        }
        let draft = self.capture_draft(frame, open);
        if name != "get_studio_draft"
            && (args["draft_id"] != draft["draft_id"]
                || args["expected_revision"].as_u64() != Some(self.draft_revision))
        {
            reply.send(Err("Stale Studio draft; read get_studio_draft and review the current revision before editing or executing".into()));
            return;
        }
        match name.as_str() {
            "get_studio_draft" | "validate_studio_draft" => reply.send(Ok(draft)),
            "set_studio_draft" => {
                if self.marked.is_some() {
                    reply.send(Err(
                        "Finish text composition before remotely replacing the Studio draft".into(),
                    ));
                    return;
                }
                if self.pending.is_some() {
                    reply.send(Err("Studio execution is pending; wait for its result before replacing the draft".into()));
                    return;
                }
                let Some(source) = args["source"].as_str() else {
                    reply.send(Err("source must be a string".into()));
                    return;
                };
                if !reply.active() {
                    return;
                }
                match self.ui.host_set_draft(source) {
                    Ok(()) => {
                        self.marked = None;
                        self.status = "Studio draft updated; playback unchanged".into();
                        let mut changed = self.capture_draft(frame, open);
                        changed["boundary"] = serde_json::json!("studio_draft_revision");
                        changed["playback_changed"] = serde_json::json!(false);
                        reply.send(Ok(changed));
                        cx.notify();
                    }
                    Err(error) => reply.send(Err(error)),
                }
            }
            "execute_studio_draft" => {
                if self.marked.is_some() {
                    reply.send(Err(
                        "Finish text composition before executing the Studio draft".into(),
                    ));
                    return;
                }
                if !bridge.is_hosted() {
                    reply.send(Err("Studio execution requires an owned runtime".into()));
                    return;
                }
                if self.pending.is_some() {
                    reply.send(Err("A Studio execution is already pending".into()));
                    return;
                }
                let Some(snapshot) = &frame.snapshot else {
                    reply.send(Err("No current Rustel session".into()));
                    return;
                };
                let s = &snapshot.value;
                if args["session"].as_str() != Some(s.session.as_str())
                    || args["generation"].as_u64() != Some(s.generation)
                    || snapshot.at.elapsed() > Duration::from_secs(3)
                {
                    reply.send(Err(
                        "Stale music session/generation; read get_music_state before executing"
                            .into(),
                    ));
                    return;
                }
                let source = self.ui.source();
                if let Err(error) = self.ui.host_set_draft(&source) {
                    reply.send(Err(error));
                    return;
                }
                if !reply.active() {
                    return;
                }
                let mut c = command();
                c.source_update = Some(source.clone());
                match bridge.send_with_reply(s, c) {
                    Some(receiver) => {
                        self.pending = Some(Evaluation {
                            remote: Some((reply, self.draft_revision)),
                            session: s.session.clone(),
                            request: 0,
                            revision: self.ui.host_document().1,
                            source,
                            reply: receiver,
                            started: Instant::now(),
                        });
                        self.status = "Executing the revision-checked Studio draft…".into();
                        cx.notify();
                    }
                    None => {
                        reply.send(Err("Runtime command was refused; playback unchanged".into()))
                    }
                }
            }
            _ => reply.send(Err("Unknown Studio tool".into())),
        }
    }
    pub fn stop(&mut self, cx: &mut Context<Self>) {
        let result = self.ui.stop();
        self.event_result(result, cx);
    }
    fn event_result(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        if let Err(error) = result {
            self.status = error;
        }
        cx.notify();
    }
    fn event(&mut self, event: Event, cx: &mut Context<Self>) {
        let result = self.ui.event(event);
        self.event_result(result, cx);
    }
    pub fn tick(&mut self, frame: &Frame, bridge: &mut Bridge, cx: &mut Context<Self>) {
        let Some(received) = &frame.snapshot else {
            self.ui.transport(false, self.generation, 0., 0.5);
            self.status = "Waiting for a live Rustel session…".into();
            let commands: Vec<serde_json::Value> =
                serde_json::from_str(&self.ui.commands()).unwrap_or_default();
            for c in commands {
                if c["type"] == "evaluate" {
                    self.ui.evaluated(
                        c["request"].as_u64().unwrap_or(0),
                        c["revision"].as_u64().unwrap_or(0),
                        c["source"].as_str().unwrap_or(""),
                        self.generation,
                        Some("No live Rustel session; reconnect before evaluating".into()),
                    );
                }
            }
            return;
        };
        let s = &received.value;
        if let Some(pending) = self.pending.take() {
            match pending.reply.try_recv() {
                Ok(reply) => {
                    let error = if pending.session != s.session
                        || (reply.accepted && pending.source != s.source)
                    {
                        Some(
                            "The selected running score changed before evaluation completed".into(),
                        )
                    } else {
                        (!reply.accepted).then_some(reply.message)
                    };
                    self.ui.evaluated(
                        pending.request,
                        pending.revision,
                        &pending.source,
                        s.generation,
                        error.clone(),
                    );
                    self.status = error
                        .clone()
                        .unwrap_or_else(|| "Score accepted by the existing runtime".into());
                    if let Some((remote, revision)) = pending.remote {
                        remote.send(match error {Some(error)=>Err(error),None=>Ok(serde_json::json!({"draft_id":self.draft_id(),"revision":revision,"session":s.session,"generation":s.generation,"request":reply.request,"acknowledged":true,"boundary":"runtime_score_snapshot"}))});
                    }
                }
                Err(mpsc::TryRecvError::Empty)
                    if pending.started.elapsed() < Duration::from_secs(16) =>
                {
                    self.pending = Some(pending)
                }
                _ => {
                    let error="Evaluation acknowledgement unavailable; check the running score before retrying".to_owned();
                    self.ui.evaluated(
                        pending.request,
                        pending.revision,
                        &pending.source,
                        s.generation,
                        Some(error.clone()),
                    );
                    self.status = error;
                    if let Some((remote, _)) = pending.remote {
                        remote.send(Err(self.status.clone()));
                    }
                }
            }
        }
        if self.session != s.session
            || self.generation != s.generation
            || self.running_source != s.source
        {
            // An external demo/MCP edit follows into a clean editor; preserve an unsent draft.
            let clean = self.ui.source() == self.running_source;
            if let Err(error) = self.ui.host_source(&s.source, s.generation, clean) {
                self.status = error;
            } else if !clean && self.ui.source() != s.source {
                self.status = "Running score changed; Studio draft preserved".into();
            }
            self.session = s.session.clone();
            self.generation = s.generation;
            self.running_source = s.source.clone();
            self.audio_sequence = None;
        }
        let cycle = s.display_cycle(received.at.elapsed().as_secs_f64());
        self.ui.transport(
            !s.transport.as_ref().is_some_and(|t| t.paused),
            s.generation,
            cycle,
            s.cps,
        );
        let events=s.events.iter().filter(|e|!e.rest).map(|e|serde_json::json!({"id":e.id,"cycle":e.cycle,"duration":e.duration,"pitch":e.pitch,"sound":e.sound,"gain":e.gain,"lane":e.lane,"context":e.context,"ui_visuals":e.ui_visuals,"label":e.part,"color":format!("#{:06x}",crate::model::event_color(e,0))})).collect::<Vec<_>>();
        if let Err(error) = self.ui.events(
            &serde_json::to_string(&events).unwrap(),
            s.generation,
            cycle / s.cps.max(0.0001),
            s.cps,
        ) {
            self.status = error;
        }
        if let Some(audio) = &frame.audio {
            let a = &audio.value;
            if self.audio_sequence != Some((a.generation, a.sequence)) {
                if let Err(error) = self
                    .ui
                    .host_audio(&serde_json::to_string(a.as_ref()).unwrap())
                {
                    self.status = error;
                }
                self.audio_sequence = Some((a.generation, a.sequence));
            }
        }
        let commands: Vec<serde_json::Value> =
            serde_json::from_str(&self.ui.commands()).unwrap_or_default();
        for value in commands {
            match value["type"].as_str().unwrap_or("") {
                "evaluate" => {
                    let request = value["request"].as_u64().unwrap_or(0);
                    let revision = value["revision"].as_u64().unwrap_or(0);
                    let source = value["source"].as_str().unwrap_or("").to_owned();
                    let refused = if self.pending.is_some() {
                        Some("An evaluation is still awaiting acknowledgement")
                    } else if !bridge.is_hosted() {
                        Some("Score execution requires an owned Rustel runtime")
                    } else if value["mini"].as_bool() == Some(true) {
                        Some("The current runtime accepts JavaScript scores; turn off mini mode")
                    } else {
                        None
                    };
                    if let Some(error) = refused {
                        self.ui.evaluated(
                            request,
                            revision,
                            &source,
                            s.generation,
                            Some(error.into()),
                        );
                        self.status = error.into();
                        continue;
                    }
                    let mut c = command();
                    c.source_update = Some(source.clone());
                    if let Some(reply) = bridge.send_with_reply(s, c) {
                        self.pending = Some(Evaluation {
                            remote: None,
                            session: s.session.clone(),
                            request,
                            revision,
                            source,
                            reply,
                            started: Instant::now(),
                        });
                        self.status = "Evaluating in the existing runtime…".into();
                    } else {
                        self.ui.evaluated(
                            request,
                            revision,
                            &source,
                            s.generation,
                            Some("Runtime command was refused".into()),
                        );
                    }
                }
                "stop" => {
                    let mut c = command();
                    c.transport = Some(music_link::TransportCommand::Stop);
                    bridge.send(s, c);
                }
                // These controls have no command in the current runtime protocol.
                kind => {
                    self.status = format!("{kind} is not supported by the connected runtime");
                    self.ui.clipboard_status(&self.status);
                }
            }
        }
        cx.notify();
    }
    fn mouse(
        &mut self,
        position: Point<Pixels>,
        kind: MouseEventKind,
        modifiers: Modifiers,
        cx: &mut Context<Self>,
    ) {
        let x = (f32::from(position.x - self.bounds.origin.x) / self.cell_width).max(0.) as u16;
        let y = (f32::from(position.y - self.bounds.origin.y) / 18.).max(0.) as u16;
        self.event(
            Event::Mouse(MouseEvent {
                kind,
                column: x,
                row: y,
                modifiers: mods(modifiers),
            }),
            cx,
        );
    }
    fn key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke;
        let code = match key.key.as_str() {
            "enter" => KeyCode::Enter,
            "escape" => KeyCode::Esc,
            "backspace" => KeyCode::Backspace,
            "delete" => KeyCode::Delete,
            "tab" if key.modifiers.shift => KeyCode::BackTab,
            "tab" => KeyCode::Tab,
            "insert" => KeyCode::Insert,
            "space" if key.modifiers.control || key.modifiers.platform || key.modifiers.alt => {
                KeyCode::Char(' ')
            }
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "pageup" => KeyCode::PageUp,
            "pagedown" => KeyCode::PageDown,
            other if other.starts_with('f') && other[1..].parse::<u8>().is_ok() => {
                KeyCode::F(other[1..].parse().unwrap())
            }
            other
                if other.chars().count() == 1
                    && (key.modifiers.control || key.modifiers.platform || key.modifiers.alt) =>
            {
                let Some(c) = other.chars().next() else {
                    return;
                };
                KeyCode::Char(c)
            }
            // Printable text is committed through GPUI's text-input/IME handler.
            _ => return,
        };
        self.event(Event::Key(KeyEvent::new(code, mods(key.modifiers))), cx);
        cx.stop_propagation();
    }
}
fn mods(m: Modifiers) -> KeyModifiers {
    let mut value = KeyModifiers::empty();
    if m.shift {
        value |= KeyModifiers::SHIFT
    }
    if m.control || m.platform {
        value |= KeyModifiers::CONTROL
    }
    if m.alt {
        value |= KeyModifiers::ALT
    }
    value
}
impl Focusable for DesktopStudio {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
fn byte_offset(text: &str, utf16: usize) -> usize {
    let mut count = 0;
    for (byte, c) in text.char_indices() {
        if count + c.len_utf16() > utf16 {
            return byte;
        }
        count += c.len_utf16();
    }
    text.len()
}
impl EntityInputHandler for DesktopStudio {
    fn text_for_range(
        &mut self,
        range: std::ops::Range<usize>,
        actual: &mut Option<std::ops::Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let source = self.ui.source();
        let a = byte_offset(&source, range.start);
        let b = byte_offset(&source, range.end);
        *actual = Some(source[..a].encode_utf16().count()..source[..b].encode_utf16().count());
        Some(source[a..b].into())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let text = self.ui.source();
        let (a, b) = self.ui.host_selection();
        Some(UTF16Selection {
            range: text[..a.min(b)].encode_utf16().count()..text[..a.max(b)].encode_utf16().count(),
            reversed: a > b,
        })
    }
    fn set_selected_text_range(
        &mut self,
        range: std::ops::Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let source = self.ui.source();
        let result = self.ui.host_select(
            byte_offset(&source, range.start),
            byte_offset(&source, range.end),
        );
        self.event_result(result, cx);
    }
    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.ui.source().encode_utf16().count())
    }
    fn marked_text_range(
        &self,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<std::ops::Range<usize>> {
        self.marked.clone()
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }
    fn replace_text_in_range(
        &mut self,
        range: Option<std::ops::Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(range) = range.or(self.marked.take()) {
            self.set_selected_text_range(range, window, cx)
        }
        self.event(Event::Paste(text.into()), cx);
        window.notify_text_input_changed(&self.focus);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<std::ops::Range<usize>>,
        text: &str,
        selected: Option<std::ops::Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let start = range
            .as_ref()
            .or(self.marked.as_ref())
            .map(|r| r.start)
            .unwrap_or_else(|| {
                self.selected_text_range(false, window, cx)
                    .unwrap()
                    .range
                    .start
            });
        self.replace_text_in_range(range, text, window, cx);
        self.marked = Some(start..start + text.encode_utf16().count());
        if let Some(selected) = selected {
            self.set_selected_text_range(start + selected.start..start + selected.end, window, cx)
        }
    }
    fn bounds_for_range(
        &mut self,
        _: std::ops::Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let (x, y) = self.ui.cursor()?;
        Some(Bounds::new(
            self.bounds.origin + point(px(f32::from(x) * self.cell_width), px(f32::from(y) * 18.)),
            size(px(self.cell_width), px(18.)),
        ))
    }
    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}
impl Render for DesktopStudio {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let paint_entity = entity.clone();
        div()
            .size_full()
            .track_focus(&self.focus)
            .key_context("RustelStudio")
            .overflow_hidden()
            .on_key_down(cx.listener(Self::key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, event: &MouseDownEvent, window, cx| {
                    window.focus(&view.focus, cx);
                    view.mouse(
                        event.position,
                        MouseEventKind::Down(TerminalButton::Left),
                        event.modifiers,
                        cx,
                    )
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, event: &MouseUpEvent, _, cx| {
                    view.mouse(
                        event.position,
                        MouseEventKind::Up(TerminalButton::Left),
                        event.modifiers,
                        cx,
                    )
                }),
            )
            .on_mouse_move(cx.listener(|view, event: &MouseMoveEvent, _, cx| {
                view.mouse(
                    event.position,
                    if event.dragging() {
                        MouseEventKind::Drag(TerminalButton::Left)
                    } else {
                        MouseEventKind::Moved
                    },
                    event.modifiers,
                    cx,
                )
            }))
            .on_scroll(cx.listener(|view, event: &ScrollEvent, _, cx| {
                let dy = event.delta.pixel_delta(px(18.)).y;
                view.mouse(
                    event.position,
                    if dy > px(0.) {
                        MouseEventKind::ScrollUp
                    } else {
                        MouseEventKind::ScrollDown
                    },
                    event.modifiers,
                    cx,
                )
            }))
            .child(
                canvas(
                    move |bounds, window, cx| {
                        entity.update(cx, |view, _| {
                            view.bounds = bounds;
                            let measure = window.text_system().shape_line(
                                "M".into(),
                                px(14.),
                                &[TextRun {
                                    len: 1,
                                    font: font("Consolas"),
                                    color: rgb(0xabb2bf).into(),
                                    background_color: None,
                                    underline: None,
                                    strikethrough: None,
                                }],
                                None,
                            );
                            view.cell_width = f32::from(measure.width).max(1.);
                            view.ui
                                .render(
                                    (f32::from(bounds.size.width) / view.cell_width)
                                        .clamp(20., 300.)
                                        as u16,
                                    (f32::from(bounds.size.height) / 18.).clamp(10., 120.) as u16,
                                )
                                .ok()
                        })
                    },
                    move |bounds, buffer, window, cx| {
                        let view = paint_entity.read(cx);
                        let width = view.cell_width;
                        let focus = view.focus.clone();
                        let cursor = view.ui.cursor();
                        window.handle_input(
                            &focus,
                            ElementInputHandler::new(bounds, paint_entity.clone()),
                            cx,
                        );
                        if let Some(buffer) = buffer {
                            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                                paint(&buffer, cursor, width, bounds, window, cx)
                            });
                        }
                    },
                )
                .size_full(),
            )
    }
}
fn color(c: Color, default: u32) -> Rgba {
    rgb(match c {
        Color::Rgb(r, g, b) => (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b),
        Color::Indexed(n) => {
            let palette = [
                0x101216, 0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xabb2bf,
                0x5c6370, 0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xffffff,
            ];
            if n < 16 {
                palette[n as usize]
            } else if n >= 232 {
                let g = 8 + 10 * u32::from(n - 232);
                g * 0x010101
            } else {
                let n = u32::from(n - 16);
                let cv = |v| if v == 0 { 0 } else { 55 + v * 40 };
                (cv(n / 36) << 16) | (cv(n / 6 % 6) << 8) | cv(n % 6)
            }
        }
        Color::Reset => default,
        Color::Black => 0x101216,
        Color::Red | Color::LightRed => 0xe06c75,
        Color::Green | Color::LightGreen => 0x98c379,
        Color::Yellow | Color::LightYellow => 0xe5c07b,
        Color::Blue | Color::LightBlue => 0x61afef,
        Color::Magenta | Color::LightMagenta => 0xc678dd,
        Color::Cyan | Color::LightCyan => 0x56b6c2,
        Color::DarkGray => 0x5c6370,
        _ => 0xabb2bf,
    })
}
fn paint(
    buffer: &Buffer,
    cursor: Option<(u16, u16)>,
    cw: f32,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    // Paint background runs first, then text runs. The usual score line needs
    // tens of shaped spans rather than a separate text operation per cell.
    for (y, row) in buffer
        .content
        .chunks(buffer.area.width as usize)
        .enumerate()
    {
        let background = |cell: &ratatui::buffer::Cell| {
            if cell.modifier.contains(Modifier::REVERSED) {
                color(cell.fg, 0xabb2bf)
            } else {
                color(cell.bg, 0x101216)
            }
        };
        let mut x = 0;
        while x < row.len() {
            let bg = background(&row[x]);
            let start = x;
            x += 1;
            while x < row.len() && background(&row[x]) == bg {
                x += 1;
            }
            window.paint_quad(fill(
                Bounds::new(
                    bounds.origin + point(px(start as f32 * cw), px(y as f32 * 18.)),
                    size(px((x - start) as f32 * cw), px(18.)),
                ),
                bg,
            ));
        }
    }
    for (y, row) in buffer
        .content
        .chunks(buffer.area.width as usize)
        .enumerate()
    {
        let mut x = 0;
        while x < row.len() {
            let cell = &row[x];
            let start = x;
            x += 1;
            let mut text = cell.symbol().to_owned();
            if text.is_ascii() && text.len() == 1 {
                while x < row.len()
                    && row[x].fg == cell.fg
                    && row[x].bg == cell.bg
                    && row[x].modifier == cell.modifier
                    && row[x].symbol().is_ascii()
                    && row[x].symbol().len() == 1
                {
                    text.push_str(row[x].symbol());
                    x += 1;
                }
            }
            if text.trim().is_empty() || cell.modifier.contains(Modifier::HIDDEN) {
                continue;
            }
            let mut f = font("Consolas");
            if cell.modifier.contains(Modifier::BOLD) {
                f.weight = FontWeight::BOLD
            }
            if cell.modifier.contains(Modifier::ITALIC) {
                f.style = FontStyle::Italic
            }
            let mut fg = if cell.modifier.contains(Modifier::REVERSED) {
                color(cell.bg, 0x101216)
            } else {
                color(cell.fg, 0xabb2bf)
            };
            if cell.modifier.contains(Modifier::DIM) {
                fg.a *= 0.65;
            }
            let text: SharedString = text.into();
            let run = TextRun {
                len: text.len(),
                font: f,
                color: fg.into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let line = window.text_system().shape_line(text, px(14.), &[run], None);
            let pos = bounds.origin + point(px(start as f32 * cw), px(y as f32 * 18. + 1.));
            let _ = line.paint(pos, px(18.), TextAlign::Left, None, window, cx);
            if cell.modifier.contains(Modifier::UNDERLINED) {
                window.paint_quad(fill(
                    Bounds::new(
                        pos + point(px(0.), px(16.)),
                        size(px((x - start) as f32 * cw), px(1.)),
                    ),
                    fg,
                ));
            }
            if cell.modifier.contains(Modifier::CROSSED_OUT) {
                window.paint_quad(fill(
                    Bounds::new(
                        pos + point(px(0.), px(8.)),
                        size(px((x - start) as f32 * cw), px(1.)),
                    ),
                    fg,
                ));
            }
        }
    }
    if let Some((x, y)) = cursor {
        window.paint_quad(fill(
            Bounds::new(
                bounds.origin + point(px(f32::from(x) * cw), px(f32::from(y) * 18.)),
                size(px(2.), px(18.)),
            ),
            rgb(0xe7eef9),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::{StudioUi, byte_offset};
    #[test]
    fn foxes_run_on_bars_and_jump_to_the_next_highest_chord_note() {
        use rustel_studio::visuals::foxy_motion;
        let notes = [(0., 0.4, 48.), (0.8, 1.4, 60.), (0.8, 1.4, 72.)];
        let start = foxy_motion(&notes, 0.1);
        let run = foxy_motion(&notes, 0.3);
        assert!(start.running && run.running);
        assert!(run.time > start.time);
        assert_ne!(start.stride, run.stride);
        assert_eq!(run.pitch, 48.);
        assert_eq!(run.lift, 0.);
        let jump = foxy_motion(&notes, 0.6);
        assert!(!jump.running && jump.lift > 6.);
        assert!(jump.pitch > 48. && jump.pitch < 72.);
        let landed = foxy_motion(&notes, 0.8);
        assert_eq!(landed.pitch, 72.);
        assert_eq!(landed.lift, 0.);
        assert!(landed.running);
        let adjacent = [(0., 1., 60.), (1., 2., 67.)];
        assert!(foxy_motion(&adjacent, 0.9).lift > 0.);
        assert_eq!(foxy_motion(&adjacent, 1.).pitch, 67.);
        assert_eq!(foxy_motion(&adjacent, 1.).lift, 0.);
    }
    #[test]
    fn remote_draft_edits_are_undoable_and_do_not_queue_playback() {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(|| {
                let source = "$: note(\"c3\").s(\"sine\")";
                let mut ui = StudioUi::new(source, None).unwrap();
                ui.host_source(source, 1, false).unwrap();
                ui.transport(true, 1, 0., 0.5);
                let _ = ui.commands();
                let before = ui.host_document();
                ui.host_set_draft("const broken = (").unwrap();
                assert_ne!(ui.host_document().1, before.1);
                for _ in 0..3 {
                    ui.render(80, 25).unwrap();
                    let commands: Vec<serde_json::Value> =
                        serde_json::from_str(&ui.commands()).unwrap();
                    assert!(
                        !commands
                            .iter()
                            .any(|c| c["type"] == "evaluate" || c["type"] == "stop")
                    );
                }
                ui.event(crossterm::event::Event::Key(
                    crossterm::event::KeyEvent::new(
                        crossterm::event::KeyCode::Char('z'),
                        crossterm::event::KeyModifiers::CONTROL,
                    ),
                ))
                .unwrap();
                assert_eq!(ui.source(), source);
                assert_ne!(ui.host_document().1, before.1);
            })
            .unwrap()
            .join()
            .unwrap();
    }
    #[test]
    fn real_studio_renders_colors_and_queues_evaluation_for_the_host() {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(|| {
                let source = "$: note(\"c4 e4 g4\").s(\"sine\")._pianoroll()";
                let mut studio = StudioUi::new(source, None).unwrap();
                studio.host_source(source, 7, false).unwrap();
                studio.transport(true, 7, 1., 0.5);
                let frame = studio.render(100, 35).unwrap();
                assert!(frame.content.iter().any(|c| c.symbol() == "$"));
                let colors = frame
                    .content
                    .iter()
                    .filter(|c| !c.symbol().trim().is_empty())
                    .map(|c| format!("{:?}", c.fg))
                    .collect::<std::collections::BTreeSet<_>>();
                assert!(
                    colors.len() >= 3,
                    "real syntax/menu colors must survive cell rendering"
                );
                // Clear construction-time settings requests; no engine is attached.
                let _ = studio.commands();
                let edited = source.replace("c4", "d4");
                studio.host_select(0, source.len()).unwrap();
                studio
                    .event(crossterm::event::Event::Paste(edited.clone()))
                    .unwrap();
                studio.play().unwrap();
                let mut commands = Vec::<serde_json::Value>::new();
                for _ in 0..40 {
                    studio.render(100, 35).unwrap();
                    commands.extend(
                        serde_json::from_str::<Vec<serde_json::Value>>(&studio.commands()).unwrap(),
                    );
                    if commands.iter().any(|c| c["type"] == "evaluate") {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                assert!(
                    commands
                        .iter()
                        .any(|c| c["type"] == "evaluate" && c["source"] == edited),
                    "{commands:?}"
                );
                assert_eq!(studio.source(), edited);
            })
            .unwrap()
            .join()
            .unwrap();
    }
    #[test]
    fn native_text_offsets_handle_surrogate_pairs() {
        let text = "a🦊é";
        assert_eq!(byte_offset(text, 1), 1);
        assert_eq!(byte_offset(text, 2), 1);
        assert_eq!(byte_offset(text, 3), 5);
        assert_eq!(byte_offset(text, 4), 7);
    }
}
