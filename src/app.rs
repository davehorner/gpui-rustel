use crate::bridge::{Bridge, command};
use crate::model::{Frame, MODES, Settings};
use crate::{charts, model};
use gpui_kit::*;
use gpui_kit::{Animation, AnimationExt};
use gpui_kit::{
    base::Disableable,
    component::{
        Sizable,
        button::{Button, ButtonVariants},
        input::{Input, InputEvent, InputState},
        menu::{DropdownMenu, PopupMenuItem},
    },
    prelude::FluentBuilder,
};
use gpui_nightshade_components::NightshadeViewport;
use music_link::{Snapshot, TempoSettings, TransportCommand};
use std::{sync::Arc, time::Duration};

#[derive(Clone)]
enum Choice {
    Mode(usize),
    Session(String),
    Demo(String),
    Lane(String),
    Gain(f32),
}
#[derive(Clone, Copy)]
enum CameraControl {
    Home,
    Top,
    Front,
    Side,
    Orbit(f32, f32),
    Pan(f32, f32),
    Zoom(f32),
    Fox,
}
pub(crate) struct MusicApp {
    pub(crate) bridge: Bridge,
    settings: Settings,
    viewport: Entity<NightshadeViewport>,
    focused: String,
    source_open: bool,
    camera_controls_open: bool,
    source_scroll: ScrollHandle,
    drag: Option<Point<Pixels>>,
    tempo: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
    _poll: Task<()>,
}
impl MusicApp {
    fn camera_button(
        &self,
        id: &'static str,
        label: &'static str,
        action: CameraControl,
        cx: &mut Context<Self>,
    ) -> Button {
        Button::new(id)
            .label(label)
            .xsmall()
            .on_click(cx.listener(move |app, _, _, cx| app.adjust_camera(action, cx)))
    }
    fn adjust_camera(&mut self, action: CameraControl, cx: &mut Context<Self>) {
        self.settings.lock().unwrap().orbit = false;
        if matches!(action, CameraControl::Home | CameraControl::Fox) {
            let mut settings = self.settings.lock().unwrap();
            settings.camera_reframe = settings.camera_reframe.wrapping_add(1);
        }
        let frame = Frame::read(&self.bridge.shared, &self.settings);
        let projected = frame.projected();
        let fit = projected.as_ref().map_or(19.2, |(s, _)| {
            ((s.lanes.len().saturating_sub(1) as f32 * 1.6) * 1.2 + 15.).max(19.2)
        });
        self.viewport.update(cx, |viewport, cx| {
            let mut pose = viewport.camera_pose();
            match action {
                CameraControl::Home => {
                    pose = gpui_nightshade::host::CameraPose {
                        yaw: 0.6,
                        pitch: 0.35,
                        distance: 4.,
                        target: [0.; 3],
                    }
                }
                CameraControl::Top => {
                    pose.yaw = 0.;
                    pose.pitch = 1.08;
                }
                CameraControl::Front => {
                    pose.yaw = 0.;
                    pose.pitch = -0.25;
                }
                CameraControl::Side => {
                    pose.yaw = std::f32::consts::FRAC_PI_2;
                    pose.pitch = 0.;
                }
                CameraControl::Orbit(yaw, pitch) => {
                    pose.yaw += yaw;
                    pose.pitch = (pose.pitch + pitch).clamp(-0.25, 1.08);
                }
                CameraControl::Zoom(factor) => pose.distance *= factor,
                CameraControl::Pan(x, y) => {
                    let step = pose.distance * fit / 4. * 0.08;
                    let pitch = (pose.pitch + 0.4).clamp(0.15, 1.4);
                    pose.target[0] +=
                        step * (x * pose.yaw.cos() - y * pose.yaw.sin() * pitch.sin());
                    pose.target[1] += step * y * pitch.cos();
                    pose.target[2] +=
                        step * (-x * pose.yaw.sin() - y * pose.yaw.cos() * pitch.sin());
                }
                CameraControl::Fox => {
                    if let Some((s, cycle)) = &projected {
                        if let Some(target) = crate::stage::fox_focus_target(s, *cycle) {
                            pose.target = target;
                            pose.distance = 3.5 / fit * 4.;
                        }
                    }
                }
            }
            viewport.set_camera_pose(pose, cx);
        });
        cx.notify();
    }
    fn camera_controls(&self, opacity: f32, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id("stage-camera-controls")
            .absolute()
            .left(px(8.))
            .top(px(8.))
            .w(px(176.))
            .p_2()
            .gap_1()
            .flex()
            .flex_col()
            .rounded_md()
            .bg(rgba(0x101c2ae8))
            .border_1()
            .border_color(rgb(0x294054))
            .text_xs()
            .child("CAMERA")
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(self.camera_button("camera-home", "Fit", CameraControl::Home, cx))
                    .child(self.camera_button("camera-top", "Top", CameraControl::Top, cx))
                    .child(self.camera_button("camera-front", "Front", CameraControl::Front, cx))
                    .child(self.camera_button("camera-side", "Side", CameraControl::Side, cx)),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(self.camera_button(
                        "camera-left",
                        "Left",
                        CameraControl::Orbit(-0.2, 0.),
                        cx,
                    ))
                    .child(self.camera_button(
                        "camera-right",
                        "Right",
                        CameraControl::Orbit(0.2, 0.),
                        cx,
                    ))
                    .child(self.camera_button(
                        "camera-up",
                        "Up",
                        CameraControl::Orbit(0., 0.15),
                        cx,
                    ))
                    .child(self.camera_button(
                        "camera-down",
                        "Down",
                        CameraControl::Orbit(0., -0.15),
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(self.camera_button(
                        "camera-pan-left",
                        "Pan <",
                        CameraControl::Pan(-1., 0.),
                        cx,
                    ))
                    .child(self.camera_button(
                        "camera-pan-right",
                        ">",
                        CameraControl::Pan(1., 0.),
                        cx,
                    ))
                    .child(self.camera_button("camera-pan-up", "^", CameraControl::Pan(0., 1.), cx))
                    .child(self.camera_button(
                        "camera-pan-down",
                        "v",
                        CameraControl::Pan(0., -1.),
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(self.camera_button("camera-zoom-in", "+", CameraControl::Zoom(0.7), cx))
                    .child(self.camera_button("camera-zoom-out", "-", CameraControl::Zoom(1.4), cx))
                    .child(self.camera_button("camera-focus-fox", "Fox", CameraControl::Fox, cx))
                    .child(
                        Button::new("fox-opacity")
                            .xsmall()
                            .label(format!("{}%", (opacity * 100.).round() as u32))
                            .on_click(cx.listener(|app, _, _, cx| {
                                let mut visual = app.settings.lock().unwrap();
                                visual.fox_opacity = if visual.fox_opacity > 0.9 {
                                    0.35
                                } else if visual.fox_opacity < 0.5 {
                                    0.65
                                } else {
                                    1.
                                };
                                cx.notify();
                            })),
                    ),
            )
    }
    fn snapshot(&self) -> Option<Arc<Snapshot>> {
        Frame::read(&self.bridge.shared, &self.settings)
            .snapshot
            .map(|s| s.value)
    }
    fn send(&mut self, mut edit: impl FnMut(&mut music_link::Command), cx: &mut Context<Self>) {
        if let Some(s) = self.snapshot() {
            let mut c = command();
            edit(&mut c);
            self.bridge.send(&s, c);
            cx.notify();
        }
    }
    fn choose(&mut self, choice: Choice, cx: &mut Context<Self>) {
        match choice {
            Choice::Mode(mode) => self.settings.lock().unwrap().mode = mode,
            Choice::Gain(gain) => self.settings.lock().unwrap().gain = gain,
            Choice::Session(session) => {
                if !self
                    .bridge
                    .shared
                    .lock()
                    .unwrap()
                    .sessions
                    .contains_key(&session)
                {
                    return;
                }
                let mut v = self.settings.lock().unwrap();
                v.session = session;
                v.lane.clear();
                v.expanded.clear();
                self.focused.clear();
            }
            Choice::Lane(lane) => {
                self.focused = lane.clone();
                self.settings.lock().unwrap().lane = lane;
            }
            Choice::Demo(demo) => {
                if self.snapshot().is_some_and(|s| s.demos.contains(&demo)) {
                    self.send(|c| c.demo = Some(demo.clone()), cx);
                }
            }
        }
        cx.notify();
    }
    fn menu(
        &self,
        id: &'static str,
        label: String,
        items: Vec<(String, Choice)>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let app = cx.entity().downgrade();
        Button::new(id)
            .label(label)
            .disabled(items.is_empty())
            .dropdown_menu(move |mut menu, _, _| {
                for (label, choice) in &items {
                    let app = app.clone();
                    let choice = choice.clone();
                    menu =
                        menu.item(PopupMenuItem::new(label.clone()).on_click(move |_, _, cx| {
                            let _ = app.update(cx, |app, cx| app.choose(choice.clone(), cx));
                        }));
                }
                menu
            })
            .into_any_element()
    }
    fn transport(
        &self,
        id: &'static str,
        label: &'static str,
        action: TransportCommand,
        ready: bool,
        cx: &mut Context<Self>,
    ) -> Button {
        Button::new(id)
            .label(label)
            .when(id == "play", |button| {
                button.primary().bg(rgb(0x2563eb)).text_color(rgb(0xffffff))
            })
            .disabled(!ready)
            .on_click(cx.listener(move |app, _, _, cx| {
                app.send(|c| c.transport = Some(action.clone()), cx)
            }))
    }
    pub(crate) fn new(
        bridge: Bridge,
        settings: Settings,
        viewport: Entity<NightshadeViewport>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let tempo = cx.new(|cx| InputState::new(window, cx).placeholder("BPM, beats, denominator"));
        let subscription = cx.subscribe_in(&tempo, window, |app, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                let text = app.tempo.read(cx).value().to_string();
                let values: Vec<_> = text.split(',').map(str::trim).collect();
                let tempo = (|| {
                    Some(TempoSettings {
                        bpm: values.first()?.parse().ok()?,
                        top: values.get(1)?.parse().ok()?,
                        bottom: values.get(2)?.parse().ok()?,
                    })
                })();
                if let Some(tempo) = tempo.filter(|t| t.validate().is_ok()) {
                    app.send(|c| c.tempo = Some(tempo.clone()), cx);
                } else {
                    app.bridge.shared.lock().unwrap().feedback =
                        "Enter BPM, numerator, denominator (for example 120, 4, 4)".into();
                    cx.notify();
                }
            }
        });
        let poll = cx.spawn(async |app, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(33))
                    .await;
                if app
                    .update(cx, |app, cx| {
                        app.bridge.tick();
                        let sessions: Vec<_> = app
                            .bridge
                            .shared
                            .lock()
                            .unwrap()
                            .sessions
                            .keys()
                            .cloned()
                            .collect();
                        let mut v = app.settings.lock().unwrap();
                        if !sessions.contains(&v.session)
                            && let Some(first) = sessions.first()
                        {
                            v.session = first.clone();
                            v.lane.clear();
                            v.expanded.clear();
                            app.focused.clear();
                        }
                        let snapshot = app
                            .bridge
                            .shared
                            .lock()
                            .unwrap()
                            .sessions
                            .get(&v.session)
                            .map(|s| s.value.clone());
                        if let Some(snapshot) = snapshot
                            && v.sync_score(&snapshot)
                        {
                            app.focused.clear();
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            bridge,
            settings,
            viewport,
            focused: String::new(),
            source_open: false,
            camera_controls_open: false,
            source_scroll: ScrollHandle::new(),
            drag: None,
            tempo,
            _subscriptions: vec![subscription],
            _poll: poll,
        }
    }
}
impl Render for MusicApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let narrow = f32::from(window.viewport_size().width) < 760.;
        let frame = Frame::read(&self.bridge.shared, &self.settings);
        let v = frame.visual.clone();
        self.viewport
            .update(cx, |viewport, cx| viewport.set_visible(v.mode == 0, cx));
        let s = frame.snapshot.as_ref().map(|s| s.value.clone());
        let sessions = self
            .bridge
            .shared
            .lock()
            .unwrap()
            .sessions
            .keys()
            .map(|s| (s.clone(), Choice::Session(s.clone())))
            .collect();
        let capabilities = frame.capabilities.as_ref();
        let browser_javascript = capabilities.is_some_and(|c| c.backend == "browser-js-wasm");
        let browser_audio = capabilities.is_some_and(|c| c.backend.starts_with("browser-"));
        let has_named_parts = capabilities.is_none_or(|c| c.named_parts)
            || frame
                .snapshot
                .as_ref()
                .is_some_and(|s| s.value.lanes.iter().any(|l| !l.parts.is_empty()));
        let can_pause = capabilities.is_none_or(|c| c.pause);
        let can_seek = capabilities.is_none_or(|c| c.seek);
        let native_restart = capabilities.is_some_and(|c| c.restart);
        let status = if s.is_some() {
            if self.bridge.is_hosted()
                || capabilities.is_some_and(|c| c.backend.starts_with("browser-"))
            {
                self.bridge.shared.lock().unwrap().status.clone()
            } else {
                "Connected · live Rustel session".to_owned()
            }
        } else {
            self.bridge.shared.lock().unwrap().status.clone()
        };
        let feedback = self.bridge.shared.lock().unwrap().feedback.clone();
        let ready = s.as_ref().is_some_and(|s| s.transport.is_some());
        let cycle = frame
            .snapshot
            .as_ref()
            .map_or(0., |s| s.value.display_cycle(s.at.elapsed().as_secs_f64()));
        let demos = s.as_ref().map_or(vec![], |s| {
            s.demos
                .iter()
                .map(|s| (s.clone(), Choice::Demo(s.clone())))
                .collect()
        });
        let mut lanes = vec![("All lanes".into(), Choice::Lane(String::new()))];
        if let Some(s) = &s {
            lanes.extend(
                s.lanes
                    .iter()
                    .map(|l| (l.display_name(), Choice::Lane(l.id.clone()))),
            );
        }
        let toolbar = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .p_3()
            .child(
                Button::new("open-studio")
                    .label("Studio")
                    .on_click(cx.listener(|_, _, _, cx| {
                        #[cfg(target_arch = "wasm32")]
                        if let Some(window) = web_sys::window() {
                            let _ = window.location().set_search("?studio");
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        cx.open_url("https://davehorner.github.io/gpui-rustel/?studio");
                        let _ = cx;
                    })),
            )
            .child(self.menu(
                "session",
                if v.session.is_empty() {
                    "No live session".into()
                } else {
                    v.session.clone()
                },
                sessions,
                cx,
            ))
            .child(
                self.menu(
                    "demo",
                    s.as_ref()
                        .filter(|s| !s.demo.is_empty())
                        .map_or("Select demo".into(), |s| s.demo.clone()),
                    demos,
                    cx,
                ),
            )
            .child(
                self.menu(
                    "mode",
                    MODES[v.mode].into(),
                    MODES
                        .iter()
                        .enumerate()
                        .map(|(i, s)| (s.to_string(), Choice::Mode(i)))
                        .collect(),
                    cx,
                ),
            )
            .child(self.menu(
                "lane",
                if v.lane.is_empty() {
                    "All lanes".into()
                } else {
                    v.lane.clone()
                },
                lanes,
                cx,
            ))
            .child(self.transport(
                "play",
                if native_restart { "Restart" } else { "Play" },
                TransportCommand::Play,
                ready,
                cx,
            ))
            .child(self.transport(
                "pause",
                "Pause",
                TransportCommand::Pause,
                ready && can_pause,
                cx,
            ))
            .child(
                self.transport(
                    "stop",
                    "Stop",
                    TransportCommand::Stop,
                    ready
                        && (!native_restart
                            || s.as_ref()
                                .and_then(|s| s.transport.as_ref())
                                .is_some_and(|t| !t.paused)),
                    cx,
                ),
            )
            .child(self.transport(
                "back",
                "−4",
                TransportCommand::Seek {
                    cycle: (cycle - 4.).max(0.),
                },
                ready && can_seek,
                cx,
            ))
            .child(self.transport(
                "forward",
                "+4",
                TransportCommand::Seek {
                    cycle: (cycle + 4.).min(1_000_000.),
                },
                ready && can_seek,
                cx,
            ))
            .when(self.bridge.is_hosted(), |toolbar| {
                toolbar.child(
                    Button::new("restart-runtime")
                        .label("Restart runtime")
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.bridge.restart_runtime();
                            cx.notify();
                        })),
                )
            })
            .child(
                Button::new("rests")
                    .label(format!("Rests: {}", if v.rests { "on" } else { "off" }))
                    .on_click(cx.listener(|app, _, _, cx| {
                        let mut v = app.settings.lock().unwrap();
                        v.rests = !v.rests;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("orbit")
                    .label(format!("Orbit: {}", if v.orbit { "on" } else { "off" }))
                    .on_click(cx.listener(|app, _, _, cx| {
                        let mut v = app.settings.lock().unwrap();
                        v.orbit = !v.orbit;
                        cx.notify();
                    })),
            )
            .when(v.mode == 0, |toolbar| {
                toolbar.child(
                    Button::new("camera-toggle")
                        .label(if self.camera_controls_open {
                            "Hide camera"
                        } else {
                            "Show camera"
                        })
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.camera_controls_open = !app.camera_controls_open;
                            cx.notify();
                        })),
                )
            })
            .child(
                Button::new("fullscreen")
                    .label(if window.is_fullscreen() {
                        "Exit fullscreen"
                    } else {
                        "Fullscreen"
                    })
                    .on_click(cx.listener(|_, _, window, cx| {
                        window.toggle_fullscreen();
                        cx.notify();
                    })),
            )
            .child(
                Button::new("reset-view")
                    .label("Reset flat view")
                    .on_click(cx.listener(|app, _, _, cx| {
                        let mut v = app.settings.lock().unwrap();
                        v.zoom = 1.;
                        v.pan = [0.; 2];
                        cx.notify();
                    })),
            )
            .when(matches!(v.mode, 5..=7), |toolbar| {
                toolbar.child(
                    self.menu(
                        "display-gain",
                        format!("Display gain {}×", v.gain),
                        [1., 2., 4., 8., 16.]
                            .into_iter()
                            .map(|g| (format!("{g}×"), Choice::Gain(g)))
                            .collect(),
                        cx,
                    ),
                )
            });
        let content = if v.mode == 0 {
            self.viewport.clone().into_any_element()
        } else {
            charts::view(frame)
        };
        let scene = div()
            .id("music-field")
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .relative()
            .child(content)
            .when(v.mode == 0 && self.camera_controls_open, |scene| {
                scene.child(self.camera_controls(v.fox_opacity, cx).with_animation(
                    "camera-reveal",
                    Animation::new(Duration::from_millis(180)).with_max_fps(30.),
                    |panel, progress| panel.opacity(progress),
                ))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|app, event: &MouseDownEvent, _, _| {
                    if app.settings.lock().unwrap().mode != 0 {
                        app.drag = Some(event.position);
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|app, _, _, _| app.drag = None),
            )
            .on_mouse_move(cx.listener(|app, event: &MouseMoveEvent, _, cx| {
                if let Some(previous) = app.drag {
                    if !event.pressed_button.is_some_and(|b| b == MouseButton::Left) {
                        app.drag = None;
                        return;
                    }
                    let delta = event.position - previous;
                    app.drag = Some(event.position);
                    let mut v = app.settings.lock().unwrap();
                    v.pan[0] += f32::from(delta.x) / 800.;
                    v.pan[1] += f32::from(delta.y) / 500.;
                    cx.notify();
                }
            }))
            .on_scroll(cx.listener(|app, event: &ScrollEvent, _, cx| {
                let mut v = app.settings.lock().unwrap();
                if v.mode != 0 {
                    v.zoom = (v.zoom
                        * (1. + f32::from(event.delta.pixel_delta(px(16.)).y) * 0.001))
                        .clamp(0.1, 20.);
                    cx.notify();
                }
            }));
        let mut source = div()
            .id("source-lanes")
            .w(px(360.))
            .flex_shrink_0()
            .h_full()
            .overflow_y_scroll()
            .track_scroll(&self.source_scroll)
            .p_3()
            .gap_3()
            .flex()
            .flex_col()
            .bg(rgb(0x0c1622));
        if let Some(s) = &s {
            for (index, lane) in s.lanes.iter().enumerate() {
                let id = lane.id.clone();
                let focus = id.clone();
                let solo = id.clone();
                let minus = id.clone();
                let plus = id.clone();
                let expand = id.clone();
                let mut card = div()
                    .flex_shrink_0()
                    .p_2()
                    .border_1()
                    .border_color(rgb(model::COLORS[index % 8]))
                    .rounded_md()
                    .gap_2()
                    .flex()
                    .flex_col()
                    .child(
                        Button::new(("focus-lane", index))
                            .label(lane.display_name())
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.focused = focus.clone();
                                cx.notify();
                            })),
                    )
                    .child(format!("Line {} · {:+} st", lane.line, lane.transpose))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new(("solo", index))
                                    .label(if s.solo_lane.as_ref() == Some(&id) {
                                        "Unsolo"
                                    } else {
                                        "Solo"
                                    })
                                    .on_click(cx.listener(move |app, _, _, cx| {
                                        let enabled = app
                                            .snapshot()
                                            .is_some_and(|s| s.solo_lane.as_ref() != Some(&solo));
                                        app.send(
                                            |c| {
                                                c.lane = solo.clone();
                                                c.solo = Some(enabled);
                                            },
                                            cx,
                                        );
                                    })),
                            )
                            .child(
                                Button::new(("down", index))
                                    .label("−1 st")
                                    .disabled(lane.transpose <= -48)
                                    .on_click(cx.listener(move |app, _, _, cx| {
                                        let t = app.snapshot().and_then(|s| {
                                            s.lanes
                                                .iter()
                                                .find(|l| l.id == minus)
                                                .map(|l| l.transpose)
                                        });
                                        if let Some(t) = t {
                                            app.send(
                                                |c| {
                                                    c.lane = minus.clone();
                                                    c.semitones = (t - 1).max(-48);
                                                },
                                                cx,
                                            );
                                        }
                                    })),
                            )
                            .child(
                                Button::new(("up", index))
                                    .label("+1 st")
                                    .disabled(lane.transpose >= 48)
                                    .on_click(cx.listener(move |app, _, _, cx| {
                                        let t = app.snapshot().and_then(|s| {
                                            s.lanes
                                                .iter()
                                                .find(|l| l.id == plus)
                                                .map(|l| l.transpose)
                                        });
                                        if let Some(t) = t {
                                            app.send(
                                                |c| {
                                                    c.lane = plus.clone();
                                                    c.semitones = (t + 1).min(48);
                                                },
                                                cx,
                                            );
                                        }
                                    })),
                            ),
                    );
                if !lane.parts.is_empty() {
                    card = card.child(
                        Button::new(("parts", index))
                            .label(if v.expanded.contains(&expand) {
                                "Collapse parts"
                            } else {
                                "Expand parts"
                            })
                            .on_click(cx.listener(move |app, _, _, cx| {
                                let mut v = app.settings.lock().unwrap();
                                if v.expanded.contains(&expand) {
                                    v.expanded.retain(|id| id != &expand);
                                } else {
                                    v.expanded.push(expand.clone());
                                }
                                cx.notify();
                            })),
                    );
                    for part in &lane.parts {
                        card =
                            card.child(div().text_color(rgb(model::rgb_color(part.color))).child(
                                format!(
                                    "{}{}",
                                    if part.helper { "↳ helper " } else { "↳ " },
                                    part.name
                                ),
                            ));
                    }
                }
                if self.focused == lane.id {
                    card = card.child(
                        gpui_kit::base::SelectableText::new(
                            ("lane-source", index),
                            s.source.clone(),
                        )
                        .highlights(model::source_highlights(s, cycle, &lane.id)),
                    );
                }
                source = source.child(card);
            }
            source = source.child(if browser_audio {
                if browser_javascript {
                    "JAVASCRIPT SCORE · Rustel WASM"
                } else {
                    "BROWSER SYNTH SCORE · mini notation"
                }
            } else if !has_named_parts {
                "SOURCE · verified revision"
            } else {
                "SHARED DEFINITIONS"
            });
            for (i, d) in s.shared_definitions().into_iter().take(128).enumerate() {
                source = source.child(div().text_color(rgb(model::rgb_color(d.color))).child(
                    gpui_kit::base::SelectableText::new(
                        ("definition", i),
                        format!(
                            "{}{}",
                            d.name,
                            d.value.map_or(String::new(), |v| format!(" = {v:.3}"))
                        ),
                    ),
                ));
            }
            if self.focused.is_empty() {
                source = source.child(
                    gpui_kit::base::SelectableText::new("score-source", s.source.clone())
                        .highlights(model::source_highlights(s, cycle, "")),
                );
            }
        } else {
            source = source.child(if self.bridge.is_hosted() {
                "Waiting for the owned runtime. Restart runtime after stopping playback."
            } else {
                "Waiting for live lanes. Start your existing music hub and Rustel producer."
            });
        }
        let clock = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_3()
            .p_3()
            .child(format!(
                "CYCLE {cycle:.3} · {}",
                s.as_ref().and_then(|s| s.transport.as_ref()).map_or(
                    "TRANSPORT UNAVAILABLE",
                    |t| if t.paused { "PAUSED" } else { "PLAYING" }
                )
            ))
            .child(div().w(px(250.)).child(Input::new(&self.tempo)))
            .child(
                s.as_ref()
                    .and_then(|s| s.tempo.as_ref())
                    .map_or(String::new(), |t| {
                        format!("{:.1} BPM · {}/{}", t.bpm, t.top, t.bottom)
                    }),
            );
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x08111c))
            .text_color(rgb(0xdce9f7))
            .child(
                div()
                    .p_3()
                    .child("GPUI RUSTEL · LIVE PATTERN FIELD")
                    .child(format!("{status} · {feedback}")),
            )
            .child(
                toolbar
                    .when(narrow, |toolbar| {
                        toolbar.child(
                            Button::new("mobile-source")
                                .label(if self.source_open {
                                    "Hide code"
                                } else {
                                    "Show code"
                                })
                                .on_click(cx.listener(|app, _, _, cx| {
                                    app.source_open = !app.source_open;
                                    cx.notify();
                                })),
                        )
                    })
                    .with_animation(
                        "toolbar-reveal",
                        Animation::new(Duration::from_millis(240)).with_max_fps(30.),
                        |toolbar, progress| toolbar.opacity(progress),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .when(narrow, |d| d.flex_col())
                    .child(scene)
                    .when(!narrow || self.source_open, |d| {
                        d.child(source.when(narrow, |s| s.w_full().h(px(220.))))
                    }),
            )
            .child(clock)
            .when_some(s.as_ref().and_then(|s| s.file_error.clone()), |d, error| {
                d.child(error)
            })
    }
}
