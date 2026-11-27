//! One Nightshade simulation, skeletal animation and stage for desktop and WebGPU.
use crate::{
    bridge::Shared,
    model::{COLORS, Frame, Settings, event_color},
};
use gpui_nightshade::{
    host::{CameraPose, ViewportHost, WgpuRenderer},
    wgpu,
};
use nightshade::{ecs::Entity, prelude::nalgebra_glm as glm};
use nightshade_animation::contract::AnimationPlayer;
use nightshade_render::{
    components::{BoundingVolume, MaterialRef, RenderMesh},
    data::{asset_id::MaterialId, material::Material},
};
use nightshade_scene::contract::{
    GlobalTransform, LocalTransform,
    primitives::{Name, Visibility},
};
use nightshade_text::contract::{Text, TextProperties, TextState};
use nightshade_viewer_core::{ViewerArguments, ViewerConnection, ViewerCorePlugin, ViewerRuntime};
use std::collections::BTreeMap;

struct Fox {
    root: Entity,
    meshes: Vec<Entity>,
    material: MaterialId,
    tint: (u32, u8),
}
struct FoxAsset {
    prefab: nightshade_prefab::contract::Prefab,
    animations: Vec<nightshade_animation::contract::AnimationClip>,
    skins: Vec<nightshade_prefab::data::ModelSkin>,
}
struct Label {
    entity: Entity,
    text: String,
}
pub struct Stage {
    host: ViewerRuntime,
    shared: Shared,
    settings: Settings,
    boxes: Vec<Entity>,
    labels: Vec<Label>,
    used: usize,
    labels_used: usize,
    materials: BTreeMap<u32, MaterialId>,
    fox_asset: FoxAsset,
    foxes: Vec<Fox>,
    fox_scale: f32,
    fox_floor: f32,
    camera: CameraPose,
    label_scale: f32,
}
impl Stage {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open(
        shared: Shared,
        settings: Settings,
        backends: wgpu::Backends,
        w: u32,
        h: u32,
    ) -> Result<Box<dyn ViewportHost>, Box<dyn std::error::Error>> {
        pollster::block_on(Self::open_async(shared, settings, backends, w, h))
    }
    pub async fn open_async(
        shared: Shared,
        settings: Settings,
        backends: wgpu::Backends,
        w: u32,
        h: u32,
    ) -> Result<Box<dyn ViewportHost>, Box<dyn std::error::Error>> {
        Ok(Box::new(
            Self::build(shared, settings, backends, w, h).await?,
        ))
    }
    async fn build(
        shared: Shared,
        settings: Settings,
        backends: wgpu::Backends,
        w: u32,
        h: u32,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let connection = ViewerConnection::default();
        let mut host = ViewerRuntime::with_options(
            ViewerArguments {
                no_agent: true,
                preview: true,
                ..Default::default()
            },
            backends,
            w,
            h,
            connection.clone(),
            ViewerCorePlugin {
                persist_settings: false,
                remote_catalogs: false,
            },
        )
        .await?;
        for action in [
            nightshade_viewer_core::ViewerAction::SetGrid(false),
            nightshade_viewer_core::ViewerAction::SetShowSky(false),
            nightshade_viewer_core::ViewerAction::SetAtmosphere(0),
            nightshade_viewer_core::ViewerAction::SetBloom(false),
            nightshade_viewer_core::ViewerAction::SetSsao(false),
        ] {
            let _ = connection.submit(action);
        }
        host.world
            .res_mut::<nightshade_render::data::config::RenderSettings>()
            .clear_color = [0.003, 0.006, 0.012, 1.];
        // Embedded bytes give native and WASM the same model without a remote fetch.
        let mut import =
            nightshade_gltf::commands::import_gltf_from_bytes(include_bytes!("../assets/fox.glb"))?;
        let prefab = import
            .prefabs
            .first()
            .cloned()
            .ok_or("Fox asset has no scene")?;
        let (_middle, reach) = nightshade_prefab::queries::prefab_reach(&prefab, &import.meshes);
        let fox_scale = 0.65 / reach.max(0.001);
        let fox_floor = -nightshade_prefab::queries::prefab_bounds(&prefab, &import.meshes)
            .map(|(low, _)| low.y)
            .unwrap_or(0.)
            * fox_scale;
        let animations = import.animations.clone();
        let skins = import.skins.clone();
        {
            let parts = nightshade_prefab::resources::prefab_parts(&mut host.world.resources);
            let mut spawn = nightshade_prefab::PrefabSpawn {
                storage: &mut host.world.storage,
                parts,
            };
            nightshade_prefab::queue_model_load(&mut spawn, &mut import);
        }
        // Register a shared primitive once. Pools below never grow beyond the bounded event window.
        nightshade_render::commands::mesh_cache::mesh_cache_insert(
            host.world.res_mut(),
            "Cube".into(),
            nightshade_render::data::procedural_meshes::create_cube_mesh(),
        );
        Ok(Self {
            host,
            shared,
            settings,
            boxes: vec![],
            labels: vec![],
            used: 0,
            labels_used: 0,
            materials: BTreeMap::new(),
            fox_asset: FoxAsset {
                prefab,
                animations,
                skins,
            },
            foxes: vec![],
            fox_scale,
            fox_floor,
            label_scale: 2.,
            camera: CameraPose {
                yaw: 0.6,
                pitch: 0.35,
                distance: 4.,
                target: [0.; 3],
            },
        })
    }
    fn ensure_fox(&mut self, index: usize) {
        if index < self.foxes.len() {
            return;
        }
        let parts = nightshade_prefab::resources::prefab_parts(&mut self.host.world.resources);
        let mut spawn = nightshade_prefab::PrefabSpawn {
            storage: &mut self.host.world.storage,
            parts,
        };
        let root = nightshade_prefab::commands::spawn_prefab_with_skins(
            &mut spawn,
            &self.fox_asset.prefab,
            &self.fox_asset.animations,
            &self.fox_asset.skins,
            glm::Vec3::zeros(),
        );
        let player = spawn
            .storage
            .get_mut::<AnimationPlayer>(root)
            .expect("embedded fox animation");
        player.current_clip = Some(
            player
                .clips
                .iter()
                .position(|c| c.name.to_lowercase().contains("run"))
                .unwrap_or(0),
        );
        player.playing = true;
        player.looping = true;
        player.speed = 0.;
        let nodes: Vec<_> = player.node_index_to_entity.values().copied().collect();
        let meshes = nodes
            .into_iter()
            .filter(|e| spawn.storage.has::<RenderMesh>(*e))
            .collect();
        let material = nightshade_render::commands::material_registry::material_registry_insert(
            self.host.world.res_mut(),
            format!("fox-{index}"),
            fox_material(0xffffff, 0.65),
        );
        self.foxes.push(Fox {
            root,
            meshes,
            material,
            tint: (0, 0),
        });
    }
    fn hide_fox(&mut self, index: usize) {
        let fox = &self.foxes[index];
        self.host
            .world
            .storage
            .get_mut::<AnimationPlayer>(fox.root)
            .unwrap()
            .playing = false;
        for entity in std::iter::once(&fox.root).chain(fox.meshes.iter()) {
            self.host
                .world
                .storage
                .set(*entity, Visibility { visible: false });
        }
    }
    fn update_fox(&mut self, index: usize, hop: Hop, cycle: f64, color: u32, opacity: f32) {
        self.ensure_fox(index);
        let root = self.foxes[index].root;
        let opacity = (opacity.clamp(0., 1.) * 100.).round() as u8;
        if self.foxes[index].tint != (color, opacity) {
            let material = nightshade_render::commands::material_registry::material_registry_insert(
                self.host.world.res_mut(),
                format!("fox-{index}"),
                fox_material(color, opacity as f32 / 100.),
            );
            self.foxes[index].material = material;
            self.foxes[index].tint = (color, opacity);
        }
        let mut position = hop.position;
        position[1] += self.fox_floor;
        self.pose(root, position, [self.fox_scale; 3]);
        // Face along the lane even when an event is held or the next box is behind.
        self.host
            .world
            .storage
            .get_mut::<LocalTransform>(root)
            .unwrap()
            .rotation = glm::quat_angle_axis(std::f32::consts::FRAC_PI_2, &glm::vec3(0., 1., 0.));
        let player = self
            .host
            .world
            .storage
            .get_mut::<AnimationPlayer>(root)
            .unwrap();
        player.playing = true;
        if let Some(clip) = player.current_clip.and_then(|i| player.clips.get(i)) {
            player.time = (cycle as f32 * 2.).rem_euclid(clip.duration.max(0.001));
        }
        let fox = &self.foxes[index];
        for entity in &fox.meshes {
            self.host
                .world
                .storage
                .set(*entity, Visibility { visible: true });
            self.host
                .world
                .storage
                .set(*entity, MaterialRef::new(fox.material));
        }
    }
    fn material(&mut self, color: u32) -> MaterialId {
        if let Some(id) = self.materials.get(&color) {
            return *id;
        }
        let [r, g, b] = crate::model::rgb_channels(color);
        let id = nightshade_render::commands::material_registry::material_registry_insert(
            self.host.world.res_mut(),
            format!("music-{color:06x}"),
            Material {
                base_color: [r, g, b, 1.],
                unlit: true,
                ..Default::default()
            },
        );
        self.materials.insert(color, id);
        id
    }
    fn pose(&mut self, entity: Entity, position: [f32; 3], scale: [f32; 3]) {
        self.host.world.storage.set(
            entity,
            LocalTransform {
                translation: glm::Vec3::from(position),
                scale: glm::Vec3::from(scale),
                ..Default::default()
            },
        );
        self.host
            .world
            .storage
            .set(entity, Visibility { visible: true });
    }
    fn box_at(&mut self, position: [f32; 3], scale: [f32; 3], color: u32) {
        if self.used >= 2304 {
            return;
        }
        let material = self.material(color);
        if self.used == self.boxes.len() {
            let entity = self.host.world.storage.spawn((
                Name("Music box".into()),
                RenderMesh::new("Cube"),
                MaterialRef::new(material),
                BoundingVolume::from_mesh_type("Cube"),
                LocalTransform::default(),
                GlobalTransform::default(),
                Visibility { visible: true },
            ));
            nightshade_render::commands::mesh_state::mesh_state_mark_entity_added(
                self.host.world.res_mut(),
                entity,
            );
            self.boxes.push(entity);
        }
        let entity = self.boxes[self.used];
        self.used += 1;
        self.pose(entity, position, scale);
        self.host
            .world
            .storage
            .set(entity, MaterialRef::new(material));
    }
    fn label(&mut self, text: String, position: [f32; 3], color: u32) {
        if self.labels_used >= 192 {
            return;
        }
        let [r, g, b] = crate::model::rgb_channels(color);
        if self.labels_used == self.labels.len() {
            let index = self.host.world.res_mut::<TextState>().cache.add_text(&text);
            let entity = self.host.world.storage.spawn((
                Name("Music axis label".into()),
                Text {
                    billboard: true,
                    ..Text::new(index)
                },
                LocalTransform::default(),
                GlobalTransform::default(),
                Visibility { visible: true },
            ));
            self.labels.push(Label {
                entity,
                text: String::new(),
            });
        }
        let label = &mut self.labels[self.labels_used];
        self.labels_used += 1;
        let entity = label.entity;
        let component = self.host.world.storage.get_mut::<Text>(entity).unwrap();
        let index = component.text_index;
        component.properties = TextProperties {
            font_size: 20.,
            alignment: nightshade_text::contract::TextAlignment::Center,
            color: glm::vec4(r, g, b, 1.),
            ..Default::default()
        };
        if label.text != text {
            component.dirty = true;
            label.text = text.clone();
            self.host
                .world
                .res_mut::<TextState>()
                .cache
                .set_text(index, text);
        }
        self.pose(entity, position, [self.label_scale; 3]);
    }
    fn update(&mut self) {
        self.used = 0;
        self.labels_used = 0;
        let frame = Frame::read(&self.shared, &self.settings);
        if let Some((s, cycle)) = frame.projected() {
            let depth = s.lanes.len().saturating_sub(1) as f32 * 1.6;
            let mut pose = self.camera;
            // Fit determines the initial view, not a minimum zoom distance.
            pose.distance *= (depth * 1.2 + 15.).max(19.2) / 4.;
            self.label_scale = pose.distance * 2. * (std::f32::consts::FRAC_PI_8).tan()
                / self.host.renderer().surface_config.height.max(1) as f32
                * 70.;
            pose.pitch = (pose.pitch + 0.4).clamp(0.15, 1.4);
            pose.target[1] += 1.7;
            pose.target[2] += depth * 0.5;
            if frame.visual.orbit {
                pose.yaw += (cycle as f32) * 0.18;
            }
            // The viewer host accepts relative camera changes; this music stage owns
            // an absolute framing that also accounts for expanded named-part lanes.
            use nightshade_camera_controller::contract::PanOrbitCamera;
            let camera = self
                .host
                .world
                .storage
                .query_ref::<&PanOrbitCamera>()
                .iter()
                .next()
                .map(|(e, _)| e);
            if let Some(camera) = camera {
                let orbit = self
                    .host
                    .world
                    .storage
                    .get_mut::<PanOrbitCamera>(camera)
                    .unwrap();
                orbit.radius = pose.distance;
                orbit.target_radius = pose.distance;
                orbit.yaw = pose.yaw;
                orbit.target_yaw = pose.yaw;
                orbit.pitch = pose.pitch;
                orbit.target_pitch = pose.pitch;
                orbit.focus = glm::Vec3::from(pose.target);
                orbit.target_focus = orbit.focus;
                let (translation, rotation) =
                    nightshade_camera_controller::contract::compute_pan_orbit_transform(
                        orbit.focus,
                        orbit.yaw,
                        orbit.pitch,
                        orbit.radius,
                    );
                self.host.world.storage.set(
                    camera,
                    LocalTransform {
                        translation,
                        rotation,
                        ..Default::default()
                    },
                );
            }
            for (i, lane) in s.lanes.iter().enumerate() {
                let color = lane
                    .parts
                    .first()
                    .map(|p| crate::model::rgb_color(p.color))
                    .unwrap_or(COLORS[i % 8]);
                self.box_at([0., 0., i as f32 * 1.6], [18., 0.035, 0.7], 0x203449);
                self.label(
                    lane.display_name().chars().take(24).collect(),
                    [-9.7, 0.1, i as f32 * 1.6],
                    color,
                );
            }
            // X = time relative to the sounding playhead; Y = MIDI pitch; Z = named part.
            self.box_at([0., -0.1, -1.1], [18., 0.025, 0.025], 0x7f9dbb);
            self.box_at([-9., 2., -1.1], [0.025, 4.2, 0.025], 0x7f9dbb);
            self.label("Time / cycles".into(), [0., -0.55, -1.1], 0xc0d4e8);
            self.label("Pitch / MIDI".into(), [-9., 4.6, -1.1], 0xc0d4e8);
            for note in (36..=108).step_by(24) {
                let y = pitch_height(Some(note as f64));
                self.box_at([-9., y, -1.1], [0.18, 0.018, 0.025], 0x7f9dbb);
                self.label(
                    format!("C{} / {note}", note / 12 - 1),
                    [-9.8, y, -1.1],
                    0x91aac4,
                );
            }
            for tick in (cycle.floor() as i64 - 2)..=(cycle.floor() as i64 + 6) {
                let x = (tick as f64 - cycle) as f32 * 3. - 6.;
                if !(-9.0..=9.0).contains(&x) {
                    continue;
                }
                self.box_at([x, -0.1, -1.1], [0.025, 0.15, 0.025], 0x7f9dbb);
                self.label(tick.to_string(), [x, -0.35, -1.1], 0x91aac4);
            }
            for e in &s.events {
                let Some(lane) = s.lanes.iter().position(|l| l.id == e.lane) else {
                    continue;
                };
                let age = e.cycle - cycle;
                if !(-2.0..=6.0).contains(&age) {
                    continue;
                }
                let length = (e.duration as f32 * 3.).clamp(0.08, 18.);
                let active = age <= 0. && age + e.duration > 0.;
                self.box_at(
                    [
                        (age as f32) * 3. - 6. + length / 2.,
                        pitch_height(e.pitch),
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
                );
            }
            self.box_at([-6., 2., depth * 0.5], [0.025, 4.4, 0.025], 0xffffff);
            self.box_at(
                [-6., 0.02, depth * 0.5],
                [0.025, 0.025, depth + 0.9],
                0xffffff,
            );
            // Reuse skeletons when a filter or a new score changes the visible lanes.
            for (index, lane) in s.lanes.iter().enumerate() {
                if let Some((hop, color)) = fox_hop(&s, cycle, index) {
                    self.update_fox(index, hop, cycle, color, frame.visual.fox_opacity);
                } else {
                    let color = lane
                        .parts
                        .first()
                        .map(|p| crate::model::rgb_color(p.color))
                        .unwrap_or(COLORS[index % COLORS.len()]);
                    self.update_fox(
                        index,
                        Hop {
                            position: [-6., 0.12, index as f32 * 1.6],
                        },
                        cycle,
                        color,
                        frame.visual.fox_opacity,
                    );
                }
            }
            for index in s.lanes.len()..self.foxes.len() {
                self.hide_fox(index);
            }
        } else {
            for index in 0..self.foxes.len() {
                self.hide_fox(index);
            }
        }
        for entity in &self.boxes[self.used..] {
            self.host
                .world
                .storage
                .set(*entity, Visibility { visible: false });
        }
        for label in &self.labels[self.labels_used..] {
            self.host
                .world
                .storage
                .set(label.entity, Visibility { visible: false });
        }
    }
}
fn pitch_height(pitch: Option<f64>) -> f32 {
    0.2 + ((pitch.unwrap_or(48.) - 36.) / 12.).clamp(0., 8.) as f32 * 0.6
}

#[derive(Debug)]
struct Hop {
    position: [f32; 3],
}
fn fox_hop(s: &music_link::Snapshot, cycle: f64, lane_index: usize) -> Option<(Hop, u32)> {
    let lane = s.lanes.get(lane_index)?;
    let mut events: Vec<_> = s
        .events
        .iter()
        .filter(|e| e.lane == lane.id && !e.rest)
        .collect();
    events.sort_by(|a, b| a.cycle.total_cmp(&b.cycle).then(a.id.cmp(&b.id)));
    events.dedup_by(|a, b| (a.cycle - b.cycle).abs() < 0.00001);
    let index = events.iter().rposition(|e| e.cycle <= cycle)?;
    let a = events[index];
    let b = events.get(index + 1).copied().unwrap_or(a);
    let phase = if b.cycle > a.cycle {
        // Jump just before the next onset: live producers supply a short
        // audio lookahead, so do not require an entire future phrase.
        let flight = (b.cycle - a.cycle).min(0.125);
        ((cycle - (b.cycle - flight)) / flight).clamp(0., 1.) as f32
    } else {
        0.
    };
    let lane_index_f = lane_index as f32;
    let point = |e: &music_link::Event| {
        [
            (e.cycle - cycle) as f32 * 3. - 6. + (e.duration as f32 * 3.).clamp(0.08, 18.) / 2.,
            pitch_height(e.pitch) + 0.17,
            lane_index_f * 1.6,
        ]
    };
    let from = point(a);
    let to = point(b);
    let mut position = std::array::from_fn(|i| from[i] + (to[i] - from[i]) * phase);
    position[1] += 4. * phase * (1. - phase) * 0.85;
    Some((Hop { position }, event_color(a, lane_index)))
}
pub(crate) fn fox_focus_target(s: &music_link::Snapshot, cycle: f64) -> Option<[f32; 3]> {
    let hop = s
        .lanes
        .iter()
        .enumerate()
        .find_map(|(index, _)| fox_hop(s, cycle, index))?
        .0;
    Some([
        hop.position[0],
        hop.position[1] - 1.7,
        hop.position[2] - s.lanes.len().saturating_sub(1) as f32 * 0.8,
    ])
}
fn fox_material(color: u32, opacity: f32) -> Material {
    let [r, g, b] = crate::model::rgb_channels(color);
    Material {
        base_color: [r, g, b, opacity],
        alpha_mode: if opacity < 1. {
            nightshade_render::data::material::AlphaMode::Blend
        } else {
            nightshade_render::data::material::AlphaMode::Opaque
        },
        unlit: true,
        double_sided: true,
        ..Default::default()
    }
}
impl ViewportHost for Stage {
    fn renderer(&self) -> &WgpuRenderer {
        self.host.renderer()
    }
    fn renderer_mut(&mut self) -> &mut WgpuRenderer {
        self.host.renderer_mut()
    }
    fn render(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.update();
        self.host.render()
    }
    fn set_camera(&mut self, pose: CameraPose) {
        self.camera = pose;
    }
    fn request_pick(&mut self, x: u32, y: u32) {
        self.host.request_pick(x, y);
    }
    fn poll_pick(&mut self) -> Option<Option<u32>> {
        self.host.poll_pick()
    }
    fn continuous(&self) -> bool {
        true
    }
    fn resize(&mut self, width: u32, height: u32) -> Result<(), Box<dyn std::error::Error>> {
        self.host.resize(width, height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fox_lands_on_boxes_and_hops_on_the_audio_clock() {
        let s = music_link::Snapshot {
            lanes: vec![music_link::Lane {
                id: "#0".into(),
                ..Default::default()
            }],
            events: vec![
                music_link::Event {
                    id: 1,
                    lane: "#0".into(),
                    cycle: 0.,
                    duration: 0.25,
                    pitch: Some(60.),
                    ..crate::model::test_event()
                },
                music_link::Event {
                    id: 2,
                    lane: "#0".into(),
                    cycle: 1.,
                    duration: 0.25,
                    pitch: Some(72.),
                    ..crate::model::test_event()
                },
            ],
            ..Default::default()
        };
        let a = fox_hop(&s, 0., 0).unwrap().0;
        let mid = fox_hop(&s, 0.9375, 0).unwrap().0;
        let b = fox_hop(&s, 1., 0).unwrap().0;
        assert!((mid.position[1] - (a.position[1] + b.position[1]) / 2. - 0.85).abs() < 0.0001);
        assert!((b.position[1] - pitch_height(Some(72.)) - 0.17).abs() < 0.0001);
        assert_eq!(
            fox_hop(&s, 0.9375, 0).unwrap().0.position,
            mid.position,
            "paused clock freezes the jump"
        );
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod gpu_tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    #[test]
    #[ignore = "Requires a Vulkan adapter; writes target/stage-smoke.png"]
    fn nightshade_fox_skin_and_axes_render_and_freeze_on_pause() {
        let source =
            "const melody=note(\"c4 e4 g4\"); const bass=note(\"c2 g2\"); $: stack(melody,bass);";
        let start = source.find("c4").unwrap();
        let mut snapshot = music_link::Snapshot {
            version: 1,
            session: "stage-test".into(),
            source: source.into(),
            cps: 0.5,
            cycle: 0.1,
            transport: Some(music_link::TransportState {
                paused: true,
                view_cycle: Some(0.1),
            }),
            lanes: vec![music_link::Lane {
                id: "#0".into(),
                label: "Melody".into(),
                ..Default::default()
            }],
            events: (0..8)
                .map(|i| music_link::Event {
                    id: i,
                    lane: "#0".into(),
                    cycle: i as f64 * 0.5,
                    duration: 0.4,
                    pitch: Some(60. + (i % 3) as f64 * 4.),
                    context: vec![(start, start + 2)],
                    ..crate::model::test_event()
                })
                .collect(),
            ..Default::default()
        };
        snapshot.lanes.push(music_link::Lane {
            id: "#1".into(),
            label: "Bass".into(),
            ..Default::default()
        });
        let bass_start = source.find("c2").unwrap();
        let bass: Vec<_> = snapshot
            .events
            .iter()
            .cloned()
            .map(|mut event| {
                event.id += 100;
                event.lane = "#1".into();
                event.cycle += 0.125;
                event.pitch = event.pitch.map(|p| p - 24.);
                event.context = vec![(bass_start, bass_start + 2)];
                event
            })
            .collect();
        snapshot.events.extend(bass);
        let shared: Shared = Arc::new(Mutex::new(crate::state::State::default()));
        shared.lock().unwrap().receive_snapshot(snapshot.clone());
        let settings = Arc::new(Mutex::new(crate::model::Visual::default()));
        settings.lock().unwrap().session = "stage-test".into();
        let mut stage = pollster::block_on(Stage::build(
            shared.clone(),
            settings,
            wgpu::Backends::VULKAN,
            960,
            640,
        ))
        .unwrap();
        shared.lock().unwrap().receive_snapshot(snapshot.clone());
        for _ in 0..10 {
            stage.render().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        // Nightshade evaluates skinned joints on the GPU and freezes CPU joint
        // transforms. Verify the extracted GPU pose clock, not those bind poses.
        let pose = |stage: &Stage| {
            stage
                .host
                .world
                .res::<nightshade_render::data::config::RendererState>()
                .render_animation
                .pose_runtime
                .clone()
        };
        assert_eq!(stage.foxes.len(), 2);
        assert_ne!(stage.foxes[0].material, stage.foxes[1].material);
        assert_eq!(
            stage.foxes[0].tint,
            (
                crate::model::rgb_color(music_link::named_color("melody")),
                65
            )
        );
        assert_eq!(
            stage.foxes[1].tint,
            (crate::model::rgb_color(music_link::named_color("bass")), 65)
        );
        for fox in &stage.foxes {
            assert!(fox.meshes.iter().all(|entity| {
                stage
                    .host
                    .world
                    .storage
                    .get::<MaterialRef>(*entity)
                    .unwrap()
                    .id
                    == fox.material
            }));
        }
        assert!(
            stage
                .host
                .world
                .storage
                .query_ref::<&nightshade_render::components::Skin>()
                .iter()
                .count()
                > 0,
            "Fox must be skinned"
        );
        let before = pose(&stage);
        let mut later = snapshot.clone();
        later.cycle = 0.35;
        later.transport.as_mut().unwrap().view_cycle = Some(0.35);
        shared.lock().unwrap().receive_snapshot(later);
        stage.render().unwrap();
        let after = pose(&stage);
        assert!(
            !before.is_empty(),
            "GPU skeletal animation must be extracted"
        );
        assert_ne!(before, after, "skeletal pose must advance with the score");
        stage.render().unwrap();
        assert_eq!(
            after,
            pose(&stage),
            "pause must freeze skeleton and root motion"
        );
        assert!(stage.labels.iter().any(|l| l.text == "Time / cycles"));
        assert!(stage.labels.iter().any(|l| l.text == "Pitch / MIDI"));
        let renderer = stage.renderer();
        let mut ring = gpui_nightshade::presentation::readback::ReadbackRing::new(
            &renderer.device,
            960,
            640,
            0,
            renderer.surface_format,
            gpui_nightshade::presentation::AlphaMode::Opaque,
            2,
        )
        .unwrap();
        ring.enqueue(
            &renderer.device,
            &renderer.queue,
            renderer.external_output.as_ref().unwrap(),
            0,
        )
        .unwrap();
        renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        let rgba = ring.take_latest().unwrap().rgba;
        stage.camera.distance = 0.04;
        stage.render().unwrap();
        let radius = stage
            .host
            .world
            .storage
            .query_ref::<&nightshade_camera_controller::contract::PanOrbitCamera>()
            .iter()
            .next()
            .unwrap()
            .1
            .radius;
        assert!(
            radius < 0.5,
            "camera must zoom closer than the former limit"
        );
        image::save_buffer(
            "target/stage-smoke.png",
            &rgba,
            960,
            640,
            image::ColorType::Rgba8,
        )
        .unwrap();
        assert!(
            rgba.chunks_exact(4)
                .filter(|p| p[0] > 100 || p[1] > 100 || p[2] > 100)
                .count()
                > 500,
            "stage must contain visible geometry and text"
        );
    }
}
