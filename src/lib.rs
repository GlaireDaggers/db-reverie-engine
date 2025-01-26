extern crate dbsdk_rs;
extern crate byteorder;
extern crate lazy_static;
extern crate ktx;
extern crate hecs;

use std::sync::Mutex;

use asset_loader::load_env;
use bsp_file::BspFile;
use bsp_renderer::BspMapRenderer;
use component::{camera::Camera, flycam::FlyCam, fpview::FPView, playerinput::PlayerInput, transform3d::Transform3D};
use hecs::World;
use lazy_static::lazy_static;
use dbsdk_rs::{db, gamepad::{self, Gamepad}, io::{FileMode, FileStream}, math::Vector3, vdp::{self, Texture}};
use system::{flycam_system::flycam_system_update, fpview_system::{fpview_input_system_update, fpview_transform_system_update}, render_system::render_system};

pub mod common;
pub mod bsp_file;
pub mod bsp_renderer;
pub mod bsp_collision;
pub mod asset_loader;

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
}

pub struct MapData {
    pub map: BspFile,
    pub map_renderer: BspMapRenderer,
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
        let bsp_renderer = BspMapRenderer::new(&bsp);
        db::log("Map loaded");

        MapData {
            map: bsp,
            map_renderer: bsp_renderer
        }
    }
}

impl GameState {
    pub fn new() -> GameState {
        let mut world = World::new();

        let map_data = MapData::load_map("demo1");
        let env = load_env("sky");

        // spawn entities
        world.spawn((
            Transform3D::default().with_position(Vector3::new(0.0, 0.0, 20.0)),
            Camera::default(),
            FPView::default(),
            PlayerInput::new(),
            FlyCam::default()
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
        };

        // update time
        self.time_data.delta_time = DELTA;
        self.time_data.total_time += DELTA;

        // update & render
        fpview_input_system_update(&input_state, &mut self.world);
        fpview_transform_system_update(&mut self.world);
        match &mut self.map_data {
            Some(v) => {
                flycam_system_update(&input_state, &self.time_data, &v.map, &mut self.world);
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