extern crate dbsdk_rs;
extern crate byteorder;
extern crate lazy_static;
extern crate ktx;
extern crate hecs;
extern crate regex;
extern crate half;
extern crate qoaudio;

use std::{collections::HashMap, sync::{Arc, Mutex}};

use asset_loader::{load_env, load_mesh, load_mesh_anim};
use bsp_file::BspFile;
use bsp_renderer::{BspMapModelRenderer, BspMapRenderer, BspMapTextures, NUM_CUSTOM_LIGHT_LAYERS};
use common::aabb_aabb_intersects;
use component::{camera::{Camera, FPCamera}, charactercontroller::CharacterController, collider::ColliderBounds, door::{Door, DoorLink, DoorOpener}, fpview::FPView, light::Light, mapmodel::MapModel, mesh::{Mesh, MeshAnim}, playerinput::PlayerInput, rotator::Rotator, transform3d::Transform3D, triggerable::{TriggerLink, TriggerState}};
use dbanim::AnimationCurveLoopMode;
use hecs::{CommandBuffer, World};
use lazy_static::lazy_static;
use dbsdk_rs::{db::{self, log}, gamepad::{self, Gamepad}, io::{FileMode, FileStream}, logfmt, math::{Quaternion, Vector3}, vdp::{self, Texture}};
use music_player::MusicPlayer;
use system::{anim_system::sk_anim_system_update, character_system::{character_apply_input_update, character_init, character_input_update, character_rotation_update, character_update}, door_system::door_system_update, flycam_system::flycam_system_update, fpcam_system::fpcam_update, fpview_system::{fpview_eye_update, fpview_input_system_update}, render_system::render_system, rotator_system::rotator_system_update, triggerable_system::trigger_link_system_update};

use crate::component::mesh::FPMesh;

pub mod common;
pub mod dbanim;
pub mod dbmesh;
pub mod sh;
pub mod bsp_file;
pub mod bsp_renderer;
pub mod bsp_collision;
pub mod asset_loader;
pub mod parse_utils;

pub mod component;
pub mod system;
pub mod music_player;

lazy_static! {
    static ref GAME_STATE: Mutex<GameState> = Mutex::new(GameState::new());
}

#[derive(Default)]
pub struct InputState {
    pub move_x: f32,
    pub move_y: f32,
    pub look_x: f32,
    pub look_y: f32,
    pub crouch: bool,
    pub jump: bool,
}

pub struct MapData {
    pub map: BspFile,
    pub map_textures: BspMapTextures,
    pub map_models: BspMapModelRenderer,
    pub map_renderers: Vec<BspMapRenderer>,
    pub light_layers: [f32;NUM_CUSTOM_LIGHT_LAYERS],
}

#[derive(Default)]
pub struct TimeData {
    pub delta_time: f32,
    pub total_time: f32
}

struct GameState {
    gamepad: Gamepad,
    world: World,
    time_data: TimeData,
    map_data: Option<MapData>,
    env: Option<[Arc<Texture>;6]>,
    music_player: Option<MusicPlayer>,
}

impl MapData {
    pub fn load_map(map_name: &str) -> MapData {
        logfmt!("Loading map: {}", map_name);
        let mut bsp_file = FileStream::open(format!("/cd/content/maps/{}.bsp", map_name).as_str(), FileMode::Read).unwrap();
        let bsp = BspFile::new(&mut bsp_file);
        let bsp_textures = BspMapTextures::new(&bsp);
        let bsp_models = BspMapModelRenderer::new(&bsp, &bsp_textures);
        logfmt!("Map loaded");

        MapData {
            map: bsp,
            map_textures: bsp_textures,
            map_models: bsp_models,
            map_renderers: Vec::new(),
            light_layers: [0.0;NUM_CUSTOM_LIGHT_LAYERS]
        }
    }

    pub fn update_renderer_cache(self: &mut Self, index: usize) {
        while self.map_renderers.len() <= index {
            logfmt!("Allocating map renderer for camera {}", index);
            self.map_renderers.push(BspMapRenderer::new(&self.map));
        }
    }
}

impl GameState {
    pub fn new() -> GameState {
        let mut world = World::new();

        let map_data = MapData::load_map("demo1");
        let env = load_env("sky");

        let mut player_start_pos = Vector3::zero();
        let mut player_start_rot = 0.0;

        let mut targetmap = HashMap::new();
        let mut pending_resolve_targets = Vec::new();

        let mut doors = Vec::new();

        // spawn entities
        map_data.map.entity_lump.parse(|entity_data| {
            match entity_data["classname"] {
                "info_player_start" => {
                    player_start_pos = parse_utils::parse_prop_vec3(&entity_data, "origin", Vector3::zero());
                    player_start_rot = parse_utils::parse_prop::<f32>(&entity_data, "angle", 0.0) + 180.0;
                }
                "worldspawn" => {
                    for (key, val) in entity_data {
                        logfmt!("worldspawn: {} = {}", key, val);
                    }
                }
                "light" => {
                    let light_pos = parse_utils::parse_prop_vec3(&entity_data, "origin", Vector3::zero());
                    let light_intensity = parse_utils::parse_prop::<f32>(&entity_data, "light", 300.0);
                    let light_color = parse_utils::parse_prop_vec3(&entity_data, "_color", Vector3::new(1.0, 1.0, 1.0));

                    world.spawn((
                        Transform3D::default().with_position(light_pos),
                        Light { color: light_color, max_radius: light_intensity }
                    ));
                }
                "func_door" => {
                    let model_idx = parse_utils::parse_prop_modelindex(&entity_data, "model", usize::MAX);
                    let submodel = &map_data.map.submodel_lump.submodels[model_idx + 1];
                    let pos = submodel.origin;
                    let size = submodel.maxs - submodel.mins;

                    let target_name = parse_utils::get_prop_str(&entity_data, "targetname", "");
                    let target = parse_utils::get_prop_str(&entity_data, "target", "");

                    let auto_open = target_name == "";

                    let angle = parse_utils::parse_prop::<i32>(&entity_data, "angle", 0);
                    let speed = parse_utils::parse_prop::<f32>(&entity_data, "speed", 100.0);
                    let lip = parse_utils::parse_prop::<f32>(&entity_data, "lip", 0.0);

                    let spawn_flags = parse_utils::parse_prop::<u32>(&entity_data, "spawnflags", 0);

                    let move_dir = if angle == -1 {
                        Vector3::new(0.0, 0.0, 1.0)
                    }
                    else if angle == -2 {
                        Vector3::new(0.0, 0.0, -1.0)
                    }
                    else {
                        let r = (angle as f32).to_radians();
                        let sx = r.cos();
                        let sy = r.sin();

                        Vector3::new(sx, sy, 0.0)
                    };

                    // calculate move distance along direction
                    let move_dist = (move_dir.x.abs() * size.x +
                        move_dir.y.abs() * size.y +
                        move_dir.z.abs() * size.z) - lip;

                    let open_pos = pos + (move_dir * move_dist);

                    let e = world.spawn((
                        Transform3D::default().with_position(pos),
                        Door { auto_open, open_pos, close_pos: pos, move_speed: speed },
                        TriggerState { triggered: false },
                        MapModel { model_idx }
                    ));

                    if target != "" {
                        pending_resolve_targets.push((e, target.to_owned()));
                    }

                    if target_name != "" {
                        targetmap.insert(target_name.to_owned(), e);
                    }

                    // don't link doors if they have the "don't link" spawn flag set
                    if spawn_flags & 4 == 0 {
                        doors.push((e, submodel));
                    }
                }
                "func_explosive" => {
                    let model_idx = parse_utils::parse_prop_modelindex(&entity_data, "model", usize::MAX);
                    let submodel = &map_data.map.submodel_lump.submodels[model_idx + 1];
                    let pos = submodel.origin;
                    
                    world.spawn((
                        Transform3D::default().with_position(pos),
                        MapModel { model_idx }
                    ));
                }
                "func_wall" => {
                    let model_idx = parse_utils::parse_prop_modelindex(&entity_data, "model", usize::MAX);
                    let submodel = &map_data.map.submodel_lump.submodels[model_idx + 1];
                    let pos = submodel.origin;
                    
                    world.spawn((
                        Transform3D::default().with_position(pos),
                        MapModel { model_idx }
                    ));
                }
                "func_object" => {
                    let model_idx = parse_utils::parse_prop_modelindex(&entity_data, "model", usize::MAX);
                    let submodel = &map_data.map.submodel_lump.submodels[model_idx + 1];
                    let pos = submodel.origin;
                    
                    world.spawn((
                        Transform3D::default().with_position(pos),
                        MapModel { model_idx }
                    ));
                }
                "func_plat" => {
                    let model_idx = parse_utils::parse_prop_modelindex(&entity_data, "model", usize::MAX);
                    let submodel = &map_data.map.submodel_lump.submodels[model_idx + 1];
                    let pos = submodel.origin;
                    
                    world.spawn((
                        Transform3D::default().with_position(pos),
                        MapModel { model_idx }
                    ));
                }
                "func_rotating" => {
                    let model_idx = parse_utils::parse_prop_modelindex(&entity_data, "model", usize::MAX);
                    let submodel = &map_data.map.submodel_lump.submodels[model_idx + 1];
                    let spawn_flags = parse_utils::parse_prop::<u32>(&entity_data, "spawnflags", 0);
                    let pos = parse_utils::parse_prop_vec3(&entity_data, "origin", submodel.origin);
                    let speed = parse_utils::parse_prop::<f32>(&entity_data, "speed", 0.0);

                    let axis = if spawn_flags & 4 != 0 {
                        Vector3::unit_x()
                    }
                    else if spawn_flags & 8 != 0 {
                        Vector3::unit_y()
                    }
                    else {
                        Vector3::unit_z()
                    };
                    
                    world.spawn((
                        Transform3D::default().with_position(pos),
                        Rotator { rot_axis: axis, rot_speed: speed },
                        MapModel { model_idx }
                    ));
                }
                "func_train" => {
                    let model_idx = parse_utils::parse_prop_modelindex(&entity_data, "model", usize::MAX);
                    let submodel = &map_data.map.submodel_lump.submodels[model_idx + 1];
                    let pos = submodel.origin;
                    
                    world.spawn((
                        Transform3D::default().with_position(pos),
                        MapModel { model_idx }
                    ));
                }
                _ => {
                }
            }
        });

        // resolve triggerable entity targets
        let mut cmd_buf = CommandBuffer::new();
        for (e, targetname) in pending_resolve_targets {
            if !targetmap.contains_key(&targetname) {
                logfmt!("Couldn't find trigger target: {}", &targetname);
            }
            else {
                let target_ent = targetmap[&targetname];
                cmd_buf.insert_one(e, TriggerLink {
                    target: target_ent
                });
            }
        }
        cmd_buf.run_on(&mut world);

        // link doors together if they are touching
        let mut pending_door_links = Vec::new();
        for (e, doormodel) in &doors {
            let mut links = Vec::new();
            for (e2, doormodel2) in &doors {
                if e2 != e && aabb_aabb_intersects(doormodel.mins, doormodel.maxs, doormodel2.mins, doormodel2.maxs) {
                    links.push(*e2);
                }
            }
            pending_door_links.push((e, links));
        }

        for (e, links) in pending_door_links {
            cmd_buf.insert_one(*e, DoorLink {
                links
            });
        }

        cmd_buf.run_on(&mut world);

        // player & camera
        let player_entity = world.spawn((
            Transform3D::default().with_position(player_start_pos),
            FPView::new(-player_start_rot, 0.0, 40.0),
            CharacterController::default(),
            PlayerInput::new(),
            DoorOpener {},
            // Light { max_radius: 200.0, color: Vector3::new(1.0, 1.0, 1.0) }
        ));

        world.spawn((
            Transform3D::default(),
            Camera::default(),
            FPCamera::new(player_entity)
        ));

        // test mesh
        world.spawn((
            Transform3D::default().with_scale(Vector3::new(20.0, 20.0, 20.0)).with_rotation(Quaternion::from_euler(Vector3::new(90.0_f32.to_radians(), 0.0, 0.0))),
            Mesh {
                mesh: load_mesh("/cd/content/model/leigh/leigh.dbm").unwrap(),
                bounds_offset: Vector3::new(0.0, 1.25, 0.0),
                bounds_extents: Vector3::new(1.0, 1.5, 1.0),
            },
            MeshAnim { anim: load_mesh_anim("/cd/content/model/leigh/leigh_idle.dba").unwrap(), loop_mode: AnimationCurveLoopMode::Repeat, time: 0.0 },
            // CharacterController::default(),
            ColliderBounds { bounds_offset: Vector3::new(0.0, 0.5, 0.0), bounds_extents: Vector3::new(1.0, 2.0, 1.0) }
        ));

        // test mesh 2
        world.spawn((
            Transform3D::default().with_scale(Vector3::new(5.0, 5.0, 5.0)).with_position(Vector3::new(10.0, 20.0, -10.0)),
            FPMesh {
                mesh: load_mesh("/cd/content/model/wpn/pistol/pistol.dbm").unwrap(),
            }
        ));

        // let music_player = MusicPlayer::new("/cd/content/mus/b8d_toys.qoa", false).unwrap();

        GameState {
            gamepad: Gamepad::new(gamepad::GamepadSlot::SlotA),
            world,
            time_data: TimeData::default(),
            map_data: Some(map_data),
            env: Some(env),
            music_player: None, //Some(music_player),
        }
    }

    pub fn tick(self: &mut Self) {
        const DELTA: f32 = 1.0 / 60.0;

        // music playback
        if let Some(music_player) = &mut self.music_player {
            music_player.update();
        }

        // update input state
        let gp_state = self.gamepad.read_state();
        let input_state = InputState {
            move_x: gp_state.left_stick_x as f32 / i16::MAX as f32,
            move_y: gp_state.left_stick_y as f32 / i16::MAX as f32,
            look_x: gp_state.right_stick_x as f32 / i16::MAX as f32,
            look_y: gp_state.right_stick_y as f32 / i16::MAX as f32,
            crouch: gp_state.is_pressed(gamepad::GamepadButton::B),
            jump: gp_state.is_pressed(gamepad::GamepadButton::A)
        };

        // update time
        self.time_data.delta_time = DELTA;
        self.time_data.total_time += DELTA;

        // update & render
        match &mut self.map_data {
            Some(v) => {
                rotator_system_update(&self.time_data, &mut self.world);
                door_system_update(&self.time_data, v, &mut self.world);
                trigger_link_system_update(&mut self.world);
                fpview_input_system_update(&input_state, &self.time_data, &mut self.world);
                character_init(&mut self.world);
                character_rotation_update(&mut self.world);
                character_input_update(&input_state, &mut self.world);
                fpview_eye_update(&self.time_data, &mut self.world);
                character_apply_input_update(&self.time_data, v, &mut self.world);
                character_update(&self.time_data, v, &mut self.world);
                sk_anim_system_update(&self.time_data, &mut self.world);
                flycam_system_update(&input_state, &self.time_data, &v.map, &mut self.world);
                fpcam_update(&mut self.world);
                render_system(&self.time_data, v, &self.env, &mut self.world);
            }
            _ => {
            }
        };

    }
}

fn tick() {
    GAME_STATE.lock().unwrap().tick();
}

#[no_mangle]
pub fn main(_: i32, _: i32) -> i32 {
    db::register_panic();
    vdp::set_vsync_handler(Some(tick));
    return 0;
}