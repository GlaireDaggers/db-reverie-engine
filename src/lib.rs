extern crate dbsdk_rs;
extern crate byteorder;
extern crate lazy_static;

use std::sync::Mutex;

use bsp_file::BspFile;
use lazy_static::lazy_static;
use map::BspMap;
use dbsdk_rs::{db, gamepad::{self, Gamepad}, io::{FileMode, FileStream}, math::{Matrix4x4, Quaternion, Vector3}, vdp::{self, Color32}};

mod bsp_file;
mod map;

lazy_static! {
    static ref GAME_STATE: Mutex<GameState> = Mutex::new(GameState::new());
}

struct GameState {
    gamepad: Gamepad,
    cam_x: f32,
    cam_y: f32,
    camera_position: Vector3,
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
            gamepad: Gamepad::new(gamepad::GamepadSlot::SlotA),
            cam_x: 180.0,
            cam_y: 0.0,
            camera_position: Vector3::zero(),
            map
        }
    }

    pub fn tick(self: &mut Self) {
        vdp::clear_color(Color32::new(0, 0, 0, 255));
        vdp::clear_depth(1.0);

        let gp_state = self.gamepad.read_state();
        let lx = (gp_state.left_stick_x as f32) / (i16::MAX as f32);
        let ly = (gp_state.left_stick_y as f32) / (i16::MAX as f32);
        let rx = (gp_state.right_stick_x as f32) / (i16::MAX as f32);
        let ry = (gp_state.right_stick_y as f32) / (i16::MAX as f32);

        self.camera_position.x += lx * 100.0 * (1.0 / 60.0);
        self.camera_position.z += ly * 100.0 * (1.0 / 60.0);

        self.cam_x += rx * 45.0 * (1.0 / 60.0);
        self.cam_y += ry * 45.0 * (1.0 / 60.0);

        if self.cam_x < 0.0 {
            self.cam_x += 360.0;
        }
        else if self.cam_x > 360.0 {
            self.cam_x -= 360.0;
        }

        if self.cam_y > 90.0 {
            self.cam_y = 90.0;
        }
        else if self.cam_y < -90.0 {
            self.cam_y = -90.0;
        }

        let cam_rot = Quaternion::from_euler(Vector3::new(self.cam_y.to_radians(), self.cam_x.to_radians(), 0.0));

        let cam_proj = Matrix4x4::projection_perspective(640.0 / 480.0, (60.0_f32).to_radians(), 10.0, 1000.0);
        self.map.draw_map(&self.camera_position, &cam_rot, &cam_proj);
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