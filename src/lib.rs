extern crate dbsdk_rs;
extern crate byteorder;
extern crate lazy_static;
extern crate ktx;
extern crate hecs;
extern crate regex;

use std::sync::Mutex;

use asset_loader::load_env;
use bsp_file::BspFile;
use bsp_renderer::{BspMapRenderer, BspMapTextures};
use component::{camera::{Camera, FPCamera}, charactercontroller::CharacterController, fpview::FPView, playerinput::PlayerInput, transform3d::Transform3D};
use hecs::World;
use lazy_static::lazy_static;
use dbsdk_rs::{db::{self, log}, gamepad::{self, Gamepad}, io::{FileMode, FileStream}, math::{Quaternion, Vector3}, vdp::{self, Texture}};
use system::{character_system::{character_apply_input_update, character_init, character_input_update, character_rotation_update, character_update}, flycam_system::flycam_system_update, fpcam_system::fpcam_update, fpview_system::{fpview_eye_update, fpview_input_system_update}, render_system::render_system};

pub mod common;
pub mod bsp_file;
pub mod bsp_renderer;
pub mod bsp_collision;
pub mod asset_loader;
pub mod parse_utils;

pub mod component;
pub mod system;

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
    pub map_renderers: Vec<BspMapRenderer>,
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
    env: Option<[Texture;6]>,
}

impl MapData {
    pub fn load_map(map_name: &str) -> MapData {
        db::log(format!("Loading map: {}", map_name).as_str());
        let mut bsp_file = FileStream::open(format!("/cd/content/maps/{}.bsp", map_name).as_str(), FileMode::Read).unwrap();
        let bsp = BspFile::new(&mut bsp_file);
        let bsp_textures = BspMapTextures::new(&bsp);
        db::log("Map loaded");

        MapData {
            map: bsp,
            map_textures: bsp_textures,
            map_renderers: Vec::new()
        }
    }

    pub fn update_renderer_cache(self: &mut Self, index: usize) {
        while self.map_renderers.len() <= index {
            log(format!("Allocating map renderer for camera {}", index).as_str());
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

        map_data.map.entity_lump.parse(|entity_data| {
            match entity_data["classname"] {
                "info_player_start" => {
                    let pos = entity_data["origin"];
                    let angle = entity_data["angle"];

                    player_start_pos = parse_utils::parse_vec3(pos);
                    player_start_rot = angle.parse::<f32>().unwrap() + 180.0;
                }
                "worldspawn" => {
                    for (key, val) in entity_data {
                        log(format!("worldspawn: {} = {}", key, val).as_str());
                    }
                }
                _ => {
                }
            }
        });

        // spawn entities
        let player_entity = world.spawn((
            Transform3D::default().with_position(player_start_pos),
            FPView::new(-player_start_rot, 0.0, 40.0),
            CharacterController::default(),
            PlayerInput::new()
        ));

        world.spawn((
            Transform3D::default(),
            Camera::default(),
            FPCamera::new(player_entity)
        ));

        GameState {
            gamepad: Gamepad::new(gamepad::GamepadSlot::SlotA),
            world,
            time_data: TimeData::default(),
            map_data: Some(map_data),
            env: Some(env)
        }
    }

    pub fn tick(self: &mut Self) {
        const DELTA: f32 = 1.0 / 60.0;

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
        fpview_input_system_update(&input_state, &self.time_data, &mut self.world);
        character_init(&mut self.world);
        character_rotation_update(&mut self.world);
        character_input_update(&input_state, &mut self.world);
        fpview_eye_update(&self.time_data, &mut self.world);
        match &mut self.map_data {
            Some(v) => {
                character_apply_input_update(&self.time_data, v, &mut self.world);
                character_update(&self.time_data, v, &mut self.world);
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
    db::log(format!("Hello, DreamBox!").as_str());
    vdp::set_vsync_handler(Some(tick));
    return 0;
}