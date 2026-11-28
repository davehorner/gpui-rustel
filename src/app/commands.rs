//! Searchable commands use the same handlers as Rustel's toolbar.
use super::*;
use gpui_kit::component::{
    WindowExt,
    command::{Command as CommandList, CommandItem, CommandState},
};
use std::{cell::Cell, rc::Rc};
actions!(rustel, [OpenCommands]);
#[derive(Clone, PartialEq)]
enum Action {
    Play,
    Pause,
    Stop,
    Mode(usize),
    Demo(String),
    Camera(u8),
    CameraControls,
    Orbit,
    Rests,
    Fullscreen,
    ResetFlat,
    Studio,
    #[cfg(not(target_arch = "wasm32"))]
    Console,
    #[cfg(not(target_arch = "wasm32"))]
    RunEditor,
    #[cfg(not(target_arch = "wasm32"))]
    LoadEditor,
    Restart,
}
#[derive(Clone)]
struct Entry {
    label: String,
    action: Action,
    enabled: bool,
}
impl MusicApp {
    fn command_entries(&self) -> Vec<Entry> {
        let frame = Frame::read(&self.bridge.shared, &self.settings);
        let snapshot = frame.snapshot.as_ref().map(|s| &s.value);
        let ready = snapshot.is_some_and(|s| s.transport.is_some());
        let mut entries = vec![];
        let mut add = |label: String, action, enabled| {
            entries.push(Entry {
                label,
                action,
                enabled,
            })
        };
        add(
            if frame.capabilities.as_ref().is_some_and(|c| c.restart) {
                "Restart music"
            } else {
                "Play music"
            }
            .into(),
            Action::Play,
            ready,
        );
        add(
            "Pause music".into(),
            Action::Pause,
            ready && frame.capabilities.as_ref().is_none_or(|c| c.pause),
        );
        add("Stop music".into(), Action::Stop, ready);
        add(
            "Restart owned runtime".into(),
            Action::Restart,
            self.bridge.is_hosted(),
        );
        #[cfg(not(target_arch = "wasm32"))]
        {
            add(
                if self.console_open {
                    "Hide code & console"
                } else {
                    "Show code & console"
                }
                .into(),
                Action::Console,
                true,
            );
            add(
                "Run editor score".into(),
                Action::RunEditor,
                self.bridge.is_hosted() && ready && self.editor_loaded,
            );
            add(
                "Load running score into editor".into(),
                Action::LoadEditor,
                snapshot.is_some(),
            );
        }
        add("Open Rustel Studio".into(), Action::Studio, true);
        add("Toggle fullscreen".into(), Action::Fullscreen, true);
        add("Reset flat view".into(), Action::ResetFlat, true);
        add(
            if self.camera_controls_open {
                "Hide camera controls"
            } else {
                "Show camera controls"
            }
            .into(),
            Action::CameraControls,
            true,
        );
        add("Toggle camera orbit".into(), Action::Orbit, true);
        add("Toggle rests".into(), Action::Rests, true);
        for (index, label) in [
            "Fit stage camera",
            "Focus fox dancer",
            "Top camera",
            "Front camera",
            "Side camera",
        ]
        .into_iter()
        .enumerate()
        {
            add(label.into(), Action::Camera(index as u8), true);
        }
        for (index, label) in MODES.iter().enumerate() {
            add(format!("View: {label}"), Action::Mode(index), true);
        }
        if let Some(s) = snapshot {
            for name in &s.demos {
                add(format!("Demo: {name}"), Action::Demo(name.clone()), true);
            }
        }
        entries
    }
    fn execute_command(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        // Recheck availability against the current session at confirmation.
        if !self
            .command_entries()
            .iter()
            .any(|e| e.action == action && e.enabled)
        {
            return;
        }
        match action {
            Action::Play => self.send(|c| c.transport = Some(TransportCommand::Play), cx),
            Action::Pause => self.send(|c| c.transport = Some(TransportCommand::Pause), cx),
            Action::Stop => self.send(|c| c.transport = Some(TransportCommand::Stop), cx),
            Action::Demo(name) => self.choose(Choice::Demo(name), cx),
            Action::Mode(index) => self.choose(Choice::Mode(index), cx),
            Action::Restart => self.bridge.restart_runtime(),
            Action::Fullscreen => window.toggle_fullscreen(),
            Action::CameraControls => {
                self.camera_controls_open = !self.camera_controls_open;
                if self.camera_controls_open {
                    self.settings.lock().unwrap().mode = 0;
                }
            }
            Action::Orbit => {
                let mut settings = self.settings.lock().unwrap();
                settings.orbit = !settings.orbit;
            }
            Action::Rests => {
                let mut settings = self.settings.lock().unwrap();
                settings.rests = !settings.rests;
            }
            Action::ResetFlat => {
                let mut settings = self.settings.lock().unwrap();
                settings.zoom = 1.;
                settings.pan = [0.; 2];
            }
            Action::Camera(index) => {
                self.settings.lock().unwrap().mode = 0;
                self.adjust_camera(
                    match index {
                        0 => CameraControl::Home,
                        1 => CameraControl::Fox,
                        2 => CameraControl::Top,
                        3 => CameraControl::Front,
                        _ => CameraControl::Side,
                    },
                    cx,
                );
            }
            #[cfg(not(target_arch = "wasm32"))]
            Action::Console => {
                self.console_open = !self.console_open;
                if self.console_open && !self.editor_loaded {
                    self.load_editor(window, cx);
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            Action::RunEditor => {
                self.console_open = true;
                self.run_editor(cx);
            }
            #[cfg(not(target_arch = "wasm32"))]
            Action::LoadEditor => {
                self.console_open = true;
                self.load_editor(window, cx);
            }
            Action::Studio => {
                #[cfg(target_arch = "wasm32")]
                if let Some(browser) = web_sys::window() {
                    let _ = browser.location().set_search("?studio");
                }
                #[cfg(not(target_arch = "wasm32"))]
                cx.open_url("https://davehorner.github.io/gpui-rustel/?studio");
            }
        }
        cx.notify();
    }
    pub(super) fn open_commands(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        let state = cx.new(|cx| CommandState::new(window, cx));
        let owner = cx.weak_entity();
        let focus = Rc::new(Cell::new(true));
        window.open_dialog(cx, move |dialog, _, _| {
            let state = state.clone();
            let owner = owner.clone();
            let focus = focus.clone();
            dialog
                .title("Rustel commands")
                .width(px(560.))
                .content(move |content, window, cx| {
                    if focus.replace(false) {
                        let state = state.clone();
                        window.defer(cx, move |window, cx| {
                            state.read(cx).focus_handle(cx).focus(window, cx)
                        });
                    }
                    let entries = owner
                        .read_with(cx, |view, _| view.command_entries())
                        .unwrap_or_default();
                    let confirm_owner = owner.clone();
                    content.child(
                        CommandList::new(&state)
                            .placeholder("Search commands or demos…")
                            .highlight_matches(true)
                            .max_h(px(360.))
                            .items(
                                entries
                                    .iter()
                                    .map(|entry| {
                                        CommandItem::new()
                                            .label(entry.label.clone())
                                            .disabled(!entry.enabled)
                                    })
                                    .collect::<Vec<_>>(),
                            )
                            .on_confirm(move |index, window, cx| {
                                if let Some(entry) =
                                    entries.get(index.row).filter(|entry| entry.enabled)
                                {
                                    let action = entry.action.clone();
                                    let _ = confirm_owner.update(cx, |view, cx| {
                                        window.close_dialog(cx);
                                        window.focus(&view.viewport.focus_handle(cx), cx);
                                        view.execute_command(action, window, cx);
                                    });
                                }
                            }),
                    )
                })
        });
    }
}
