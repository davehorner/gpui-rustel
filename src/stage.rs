use crate::{
    bridge::Shared,
    model::{COLORS, Frame, Settings, event_color},
};
use gpui_nightshade::{
    host::{CameraPose, SceneHost, ViewportHost, WgpuRenderer},
    wgpu,
};
use nightshade::prelude::nalgebra_glm as glm;
use nightshade_render::{components::render_world::*, data::material::Material};

struct Slot {
    entity: nightshade::ecs::Entity,
    material: u32,
}
pub struct Stage {
    host: SceneHost,
    shared: Shared,
    settings: Settings,
    slots: Vec<Slot>,
    used: usize,
    started: std::time::Instant,
    camera: CameraPose,
}
impl Stage {
    pub fn open(
        shared: Shared,
        settings: Settings,
        backends: wgpu::Backends,
        w: u32,
        h: u32,
    ) -> Result<Box<dyn ViewportHost>, Box<dyn std::error::Error>> {
        let mut host = pollster::block_on(SceneHost::cube_with_backends(w, h, backends))?;
        let entities: Vec<_> = host
            .scene
            .scene_world
            .query_ref::<&MeshName>()
            .iter()
            .map(|(e, _)| e)
            .collect();
        for entity in entities {
            host.scene
                .scene_world
                .storage
                .get_mut::<Visible>(entity)
                .unwrap()
                .0 = false;
        }
        Ok(Box::new(Self {
            host,
            shared,
            settings,
            slots: vec![],
            used: 0,
            started: std::time::Instant::now(),
            camera: CameraPose {
                yaw: 0.6,
                pitch: 0.35,
                distance: 4.,
                target: [0.; 3],
            },
        }))
    }
    fn box_at(&mut self, position: [f32; 3], scale: [f32; 3], color: u32) {
        if self.used >= 2177 {
            return;
        }
        if self.used == self.slots.len() {
            let material = self
                .host
                .scene
                .state
                .render_materials
                .add(Material::default());
            let entity = self.host.scene.scene_world.spawn((
                MeshName("Cube".into()),
                MeshKey(nightshade_render::data::generational_registry::registry_name_hash("Cube")),
                MorphWeights::default(),
                CullingMask::default(),
                MaterialId(material),
                Transform::default(),
                Visible(true),
                Layer(nightshade_render::data::render_layer::RenderLayer::WORLD),
            ));
            self.slots.push(Slot { entity, material });
        }
        let slot = &self.slots[self.used];
        self.used += 1;
        let world = &mut self.host.scene.scene_world.storage;
        world.get_mut::<Visible>(slot.entity).unwrap().0 = true;
        world.get_mut::<Transform>(slot.entity).unwrap().0 =
            glm::translation(&glm::Vec3::from(position)) * glm::scaling(&glm::Vec3::from(scale));
        let rgba = [
            ((color >> 16) & 255) as f32 / 255.,
            ((color >> 8) & 255) as f32 / 255.,
            (color & 255) as f32 / 255.,
            1.,
        ];
        let materials = &mut self.host.scene.state.render_materials;
        let m = &mut materials.entries[slot.material as usize - 1].material;
        if m.base_color != rgba {
            m.base_color = rgba;
            materials.generation += 1;
        }
    }
    fn update(&mut self) {
        self.used = 0;
        let frame = Frame::read(&self.shared, &self.settings);
        if let Some((s, cycle)) = frame.projected() {
            let mut pose = self.camera;
            pose.distance *= 6.;
            pose.target[1] += 0.5;
            pose.target[2] += s.lanes.len().saturating_sub(1) as f32 * 0.8;
            self.host.set_camera(pose);
            for (i, _) in s.lanes.iter().enumerate() {
                self.box_at([0., 0., i as f32 * 1.6], [18., 0.035, 0.7], 0x203449);
                self.box_at(
                    [-6., 0.05, i as f32 * 1.6],
                    [0.04, 0.05, 0.8],
                    COLORS[i % 8],
                );
            }
            for e in &s.events {
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
                self.box_at(
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
                );
            }
            self.box_at(
                [-6., 1., (s.lanes.len().saturating_sub(1)) as f32 * 0.8],
                [0.04, 2., (s.lanes.len() as f32 * 1.6).max(1.)],
                0xffffff,
            );
        }
        for slot in &self.slots[self.used..] {
            self.host
                .scene
                .scene_world
                .storage
                .get_mut::<Visible>(slot.entity)
                .unwrap()
                .0 = false;
        }
        if frame.visual.orbit {
            self.host.yaw = self.started.elapsed().as_secs_f32() * 0.12;
        }
    }
}
impl ViewportHost for Stage {
    fn renderer(&self) -> &WgpuRenderer {
        &self.host.renderer
    }
    fn renderer_mut(&mut self) -> &mut WgpuRenderer {
        &mut self.host.renderer
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
}
