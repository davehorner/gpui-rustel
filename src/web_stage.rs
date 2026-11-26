//! Perspective stage painted with GPUI paths on the browser graphics backend.
use crate::model::{COLORS, Frame, event_color};
use gpui_kit::*;
struct Face {
    points: [[f32; 2]; 4],
    depth: f32,
    color: u32,
}
fn box_faces(
    faces: &mut Vec<Face>,
    center: [f32; 3],
    size: [f32; 3],
    color: u32,
    project: &impl Fn([f32; 3]) -> ([f32; 2], f32),
) {
    let corners: Vec<_> = (0..8)
        .map(|i| {
            project(std::array::from_fn(|a| {
                center[a]
                    + if i & (1 << a) == 0 {
                        -size[a] / 2.
                    } else {
                        size[a] / 2.
                    }
            }))
        })
        .collect();
    for (indices, shade) in [
        ([0, 1, 3, 2], 0.55),
        ([4, 5, 7, 6], 0.8),
        ([0, 1, 5, 4], 0.5),
        ([2, 3, 7, 6], 1.0),
        ([0, 2, 6, 4], 0.65),
        ([1, 3, 7, 5], 0.9),
    ] {
        if indices.iter().any(|&i| corners[i].1 < 0.5) {
            continue;
        }
        let shade_color =
            |shift: u32| ((((color >> shift) & 255u32) as f32 * shade).min(255.) as u32) << shift;
        faces.push(Face {
            points: indices.map(|i| corners[i].0),
            depth: indices.iter().map(|&i| corners[i].1).sum::<f32>() / 4.,
            color: shade_color(16) | shade_color(8) | shade_color(0),
        });
    }
}
pub fn view(frame: Frame) -> AnyElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let Some((s, cycle)) = frame.projected() else {
                return;
            };
            let w = f32::from(bounds.size.width);
            let h = f32::from(bounds.size.height);
            let yaw = 0.6
                + frame.visual.pan[0] * 3.
                + if frame.visual.orbit {
                    cycle as f32 * 0.12
                } else {
                    0.
                };
            let pitch = (0.45 + frame.visual.pan[1] * 2.).clamp(0.1, 1.3);
            let distance = 24. / frame.visual.zoom.clamp(0.25, 4.);
            let middle = s.lanes.len().saturating_sub(1) as f32 * 0.8;
            let project = |p: [f32; 3]| {
                let x = p[0];
                let y = p[1] - 0.6;
                let z = p[2] - middle;
                let rx = x * yaw.cos() - z * yaw.sin();
                let rz = x * yaw.sin() + z * yaw.cos();
                let ry = y * pitch.cos() - rz * pitch.sin();
                let depth = distance - y * pitch.sin() - rz * pitch.cos();
                let scale = w.min(h) * 1.1 / depth.max(0.5);
                ([w / 2. + rx * scale, h / 2. - ry * scale], depth)
            };
            let mut faces = Vec::new();
            for (i, _) in s.lanes.iter().enumerate() {
                box_faces(
                    &mut faces,
                    [0., 0., i as f32 * 1.6],
                    [18., 0.035, 0.7],
                    0x203449,
                    &project,
                );
                box_faces(
                    &mut faces,
                    [-6., 0.05, i as f32 * 1.6],
                    [0.04, 0.05, 0.8],
                    COLORS[i % 8],
                    &project,
                );
            }
            for e in s.events.iter().take(1024) {
                let Some(lane) = s.lanes.iter().position(|l| l.id == e.lane) else {
                    continue;
                };
                let age = e.cycle - cycle;
                if !(-2.0..=6.).contains(&age) {
                    continue;
                }
                let length = (e.duration as f32 * 3.).clamp(0.08, 18.);
                let active = age <= 0. && age + e.duration > 0.;
                let pitch = ((e.pitch.unwrap_or(48.) - 36.) / 48.).clamp(0., 2.) as f32;
                box_faces(
                    &mut faces,
                    [
                        age as f32 * 3. - 6. + length / 2.,
                        0.16 + pitch * 1.3,
                        lane as f32 * 1.6,
                    ],
                    [
                        length,
                        if e.rest {
                            0.025
                        } else if active {
                            0.3
                        } else {
                            0.14
                        },
                        0.6,
                    ],
                    event_color(e, lane),
                    &project,
                );
            }
            box_faces(
                &mut faces,
                [-6., 1., middle],
                [0.04, 2., (s.lanes.len() as f32 * 1.6).max(1.)],
                0xffffff,
                &project,
            );
            faces.sort_by(|a, b| b.depth.total_cmp(&a.depth));
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                for face in faces {
                    let mut path = PathBuilder::fill();
                    path.move_to(
                        bounds.origin + point(px(face.points[0][0]), px(face.points[0][1])),
                    );
                    for p in &face.points[1..] {
                        path.line_to(bounds.origin + point(px(p[0]), px(p[1])));
                    }
                    path.close();
                    if let Ok(path) = path.build() {
                        window.paint_path(path, rgb(face.color));
                    }
                }
            });
        },
    )
    .size_full()
    .into_any_element()
}
