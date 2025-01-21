extern crate dbsdk_rs;
extern crate byteorder;
extern crate lazy_static;

use std::sync::Mutex;

use bsp_file::BspFile;
use lazy_static::lazy_static;
use map::BspMap;
use dbsdk_rs::{db, io::{FileMode, FileStream}, math::{Matrix4x4, Quaternion, Vector3}, vdp::{self, Color32}};

mod bsp_file;
mod map;

lazy_static! {
    static ref GAME_STATE: Mutex<GameState> = Mutex::new(GameState::new());
}

struct GameState {
    camera_position: Vector3,
    camera_rotation: Quaternion,
    map: BspMap
}

impl GameState {
    pub fn new() -> GameState {
        db::log("Loading BSP...");
        let mut bsp_file = FileStream::open("/cd/content/maps/demo1.bsp", FileMode::Read).unwrap();
        let bsp = BspFile::new(&mut bsp_file);
        let map = BspMap::new(bsp);
        db::log("BSP loaded");

        GameState {
            camera_position: Vector3::zero(),
            camera_rotation: Quaternion::identity(),
            map
        }
    }

    pub fn tick(self: &mut Self) {
        vdp::clear_color(Color32::new(128, 128, 255, 255));

        let cam_proj = Matrix4x4::projection_perspective(640.0 / 480.0, (60.0_f32).to_radians(), 0.01, 1000.0);
        self.map.draw_map(&self.camera_position, &self.camera_rotation, &cam_proj);
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