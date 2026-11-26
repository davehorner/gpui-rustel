//! GPUI-native equivalents of the old Canvas2D music views.
use crate::model::{COLORS, Frame, event_color, spiral, wheel};
use gpui_kit::*;

fn line(points: &[[f32; 2]], color: u32, b: Bounds<Pixels>, window: &mut Window) {
    if points.len() < 2 {
        return;
    }
    let mut p = PathBuilder::stroke(px(1.5));
    p.move_to(b.origin + point(px(points[0][0]), px(points[0][1])));
    for v in &points[1..] {
        p.line_to(b.origin + point(px(v[0]), px(v[1])));
    }
    if let Ok(p) = p.build() {
        window.paint_path(p, rgb(color));
    }
}
fn rect(x: f32, y: f32, w: f32, h: f32, color: u32, b: Bounds<Pixels>, window: &mut Window) {
    window.paint_quad(fill(
        Bounds::new(
            b.origin + point(px(x), px(y)),
            size(px(w.max(1.)), px(h.max(1.))),
        ),
        rgb(color),
    ));
}
fn label(
    text: String,
    x: f32,
    y: f32,
    color: u32,
    b: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let text: SharedString = text.into();
    let run = TextRun {
        len: text.len(),
        font: font("JetBrains Mono"),
        color: rgb(color).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let shaped = window.text_system().shape_line(text, px(12.), &[run], None);
    let _ = shaped.paint(
        b.origin + point(px(x), px(y)),
        px(16.),
        TextAlign::Left,
        None,
        window,
        cx,
    );
}
pub fn view(frame: Frame) -> AnyElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| {
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                draw(&frame, bounds, window, cx)
            });
        },
    )
    .size_full()
    .into_any_element()
}
fn draw(frame: &Frame, b: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    let w = f32::from(b.size.width);
    let h = f32::from(b.size.height);
    let mode = frame.visual.mode;
    if (mode == 7 && frame.capabilities.as_ref().is_some_and(|c| !c.stereo))
        || (mode == 8 && frame.capabilities.as_ref().is_some_and(|c| !c.control_maps))
    {
        label(
            "This upstream runtime does not publish the data for this view".into(),
            24.,
            24.,
            0x91a8bd,
            b,
            window,
            cx,
        );
        return;
    }
    if (5..=7).contains(&mode) {
        let Some(a) = frame.audio.as_ref() else {
            label(
                "Waiting for fresh master audio analysis".into(),
                24.,
                24.,
                0x91a8bd,
                b,
                window,
                cx,
            );
            return;
        };
        let a = &a.value;
        let gain = frame.visual.gain;
        if mode == 5 {
            let trigger = (1..256)
                .find(|&i| a.scope[i - 1] <= 0. && a.scope[i] > 0.)
                .unwrap_or(0);
            let points: Vec<_> = a
                .scope
                .iter()
                .skip(trigger)
                .take(256)
                .enumerate()
                .map(|(i, v)| {
                    [
                        24. + i as f32 / 255. * (w - 48.),
                        h / 2. - v * gain * h * 0.4,
                    ]
                })
                .collect();
            line(&points, COLORS[0], b, window);
        } else if mode == 6 {
            let nyquist = a.sample_rate as f32 / 2.;
            for i in 0..64 {
                let lo = 20. * (nyquist / 20.).powf(i as f32 / 64.);
                let hi = 20. * (nyquist / 20.).powf((i + 1) as f32 / 64.);
                let first = ((lo / nyquist * 512.) as usize).min(511);
                let last = ((hi / nyquist * 512.) as usize).max(first + 1).min(512);
                let db = a.spectrum[first..last]
                    .iter()
                    .copied()
                    .fold(-120., f32::max);
                let v = ((db + gain.log10() * 20. + 90.) / 90.).clamp(0., 1.);
                rect(
                    24. + i as f32 * (w - 48.) / 64.,
                    h - 24. - v * (h - 48.),
                    (w - 48.) / 64. - 2.,
                    v * (h - 48.),
                    COLORS[i / 8],
                    b,
                    window,
                );
            }
        } else {
            let radius = w.min(h) * 0.4;
            line(
                &[
                    [w / 2. - radius, h / 2. - radius],
                    [w / 2. + radius, h / 2. + radius],
                ],
                0x24364f,
                b,
                window,
            );
            line(
                &[
                    [w / 2. - radius, h / 2. + radius],
                    [w / 2. + radius, h / 2. - radius],
                ],
                0x24364f,
                b,
                window,
            );
            let mut cross = 0.;
            let mut l2 = 0.;
            let mut r2 = 0.;
            for [l, r] in &a.sides {
                rect(
                    w / 2. + (l - r) * 0.5 * gain * radius,
                    h / 2. - (l + r) * 0.5 * gain * radius,
                    2.,
                    2.,
                    COLORS[0],
                    b,
                    window,
                );
                cross += l * r;
                l2 += l * l;
                r2 += r * r;
            }
            label(
                format!(
                    "Stereo correlation {:.2}",
                    cross / (l2 * r2).sqrt().max(1e-12)
                ),
                24.,
                24.,
                0x91a8bd,
                b,
                window,
                cx,
            );
        }
        return;
    }
    let Some((s, cycle)) = frame.projected() else {
        label(
            "Start Rustel and the music hub to see live patterns".into(),
            24.,
            24.,
            0x91a8bd,
            b,
            window,
            cx,
        );
        return;
    };
    let zoom = frame.visual.zoom;
    let x_at =
        |t: f64| 24. + ((t - cycle + 1.) as f32 / 4. * zoom + frame.visual.pan[0]) * (w - 48.);
    for i in -4..=16 {
        let x = x_at(cycle.floor() + i as f64 / 4.);
        line(
            &[[x, 24.], [x, h - 24.]],
            if i % 4 == 0 { 0x31455b } else { 0x172839 },
            b,
            window,
        );
    }
    if mode == 8 {
        for (k, key) in ["gain", "pan", "cutoff", "resonance"].iter().enumerate() {
            let transform = |v: f64| if k == 2 { v.max(1.).log10() } else { v };
            let values: Vec<_> = s
                .events
                .iter()
                .filter_map(|e| e.controls.get(*key))
                .filter(|v| v.is_finite())
                .map(|v| transform(*v))
                .collect();
            let lo = values.iter().copied().fold(0., f64::min);
            let hi = values.iter().copied().fold(1., f64::max);
            let top = k as f32 * h / 4.;
            let row = h / 4.;
            label(
                format!("{key}{}", if k == 2 { " (log Hz)" } else { "" }),
                24.,
                top,
                0x91a8bd,
                b,
                window,
                cx,
            );
            for (i, lane) in s.lanes.iter().enumerate() {
                let mut events: Vec<_> = s
                    .events
                    .iter()
                    .filter(|e| {
                        e.lane == lane.id && e.controls.get(*key).is_some_and(|v| v.is_finite())
                    })
                    .collect();
                events.sort_by(|a, b| a.cycle.total_cmp(&b.cycle));
                let points: Vec<_> = events
                    .iter()
                    .map(|e| {
                        [
                            x_at(e.cycle),
                            top + row
                                - 12.
                                - ((transform(e.controls[*key]) - lo) / (hi - lo)) as f32
                                    * (row - 32.),
                        ]
                    })
                    .collect();
                line(&points, COLORS[i % 8], b, window);
                for [x, y] in points {
                    rect(x, y, 3., 3., COLORS[i % 8], b, window);
                }
            }
        }
        return;
    }
    for (i, lane) in s.lanes.iter().enumerate() {
        let row = h / (s.lanes.len().max(1) as f32);
        if mode == 1 || mode == 2 {
            label(
                lane.display_name().chars().take(48).collect(),
                8.,
                i as f32 * row + 4.,
                COLORS[i % 8],
                b,
                window,
                cx,
            );
        }
        for e in s.events.iter().filter(|e| e.lane == lane.id) {
            let age = e.cycle - cycle;
            if !(-3.0..=8.).contains(&age) {
                continue;
            }
            let pitch = e.pitch.unwrap_or(48.);
            let color = event_color(e, i);
            let (x, y, ew, eh) = if mode == 1 {
                (
                    x_at(e.cycle),
                    i as f32 * row + row * 0.8
                        - ((pitch - 36.) / 48.).clamp(0., 1.) as f32 * row * 0.65,
                    ((e.duration as f32 / 4.) * zoom * (w - 48.)).max(3.),
                    8.,
                )
            } else if mode == 2 {
                (
                    24. + ((pitch - 24.) / 72.).clamp(0., 1.) as f32 * (w - 80.),
                    h * 0.7 - age as f32 * h / 4. * zoom + frame.visual.pan[1] * h,
                    48.,
                    18.,
                )
            } else {
                let (x, y) = if mode == 3 {
                    spiral(age, pitch)
                } else {
                    wheel(pitch)
                };
                (
                    w / 2. + x * w.min(h) * 0.35 * zoom + frame.visual.pan[0] * w,
                    h / 2. + y * w.min(h) * 0.35 * zoom + frame.visual.pan[1] * h,
                    if age <= 0. && age + e.duration > 0. {
                        9.
                    } else {
                        4.
                    },
                    6.,
                )
            };
            rect(
                x,
                y,
                ew,
                eh,
                if e.rest { 0x415367 } else { color },
                b,
                window,
            );
            if mode == 2 {
                label(
                    e.face_label().chars().take(18).collect(),
                    x,
                    y,
                    color,
                    b,
                    window,
                    cx,
                );
            }
        }
    }
    line(&[[x_at(cycle), 0.], [x_at(cycle), h]], 0xffffff, b, window);
}
