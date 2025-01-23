extern crate dbsdk_rs;
extern crate byteorder;
extern crate lazy_static;
extern crate ktx;

use std::{ops::{Add, Mul}, sync::Mutex};

use asset_loader::load_texture;
use bsp_file::{BspFile, MASK_SOLID};
use lazy_static::lazy_static;
use map::{BspMap, TraceResult};
use dbsdk_rs::{db::{self, log}, field_offset::offset_of, gamepad::{self, Gamepad}, io::{FileMode, FileStream}, math::{Matrix4x4, Quaternion, Vector2, Vector3, Vector4}, vdp::{self, Color32, PackedVertex, Texture}};

mod common;
mod bsp_file;
mod map;
mod asset_loader;

lazy_static! {
    static ref GAME_STATE: Mutex<GameState> = Mutex::new(GameState::new());
}

struct GameState {
    gamepad: Gamepad,
    cam_x: f32,
    cam_y: f32,
    camera_position: Vector3,
    map: BspMap,
    env: Option<[Texture;6]>,
}

impl GameState {
    pub fn new() -> GameState {
        db::log("Loading BSP...");
        let mut bsp_file = FileStream::open("/cd/content/maps/demo1.bsp", FileMode::Read).unwrap();
        let bsp = BspFile::new(&mut bsp_file);
        let map = BspMap::new(bsp);
        db::log("BSP loaded");

        let env_ft = load_texture("/cd/content/env/sky1ft.ktx").unwrap();
        let env_bk = load_texture("/cd/content/env/sky1bk.ktx").unwrap();
        let env_lf = load_texture("/cd/content/env/sky1lf.ktx").unwrap();
        let env_rt = load_texture("/cd/content/env/sky1rt.ktx").unwrap();
        let env_up = load_texture("/cd/content/env/sky1up.ktx").unwrap();
        let env_dn = load_texture("/cd/content/env/sky1dn.ktx").unwrap();

        GameState {
            gamepad: Gamepad::new(gamepad::GamepadSlot::SlotA),
            cam_x: 180.0,
            cam_y: 0.0,
            camera_position: Vector3::zero(),
            map,
            env: Some([env_ft, env_bk, env_lf, env_rt, env_up, env_dn])
        }
    }

    fn draw_env_quad(tex: &Texture, rotation: &Quaternion, camera_rotation: &Quaternion, camera_proj: &Matrix4x4) {
        // build view + projection matrix
        Matrix4x4::load_identity_simd();
        Matrix4x4::mul_simd(&Matrix4x4::scale(Vector3::new(100.0, 100.0, 100.0)));
        Matrix4x4::mul_simd(&Matrix4x4::rotation(*rotation));
        Matrix4x4::mul_simd(&Matrix4x4::rotation({let mut r = *camera_rotation; r.invert(); r}));
        Matrix4x4::mul_simd(&common::coord_space_transform());
        Matrix4x4::mul_simd(camera_proj);

        let mut quad = [
            PackedVertex::new(Vector4::new(-1.0, -1.0, -1.0, 1.0), Vector2::new(0.0, 1.0), Color32::new(255, 255, 255, 255), Color32::new(0, 0, 0, 0)),
            PackedVertex::new(Vector4::new(-1.0, -1.0,  1.0, 1.0), Vector2::new(0.0, 0.0), Color32::new(255, 255, 255, 255), Color32::new(0, 0, 0, 0)),
            PackedVertex::new(Vector4::new( 1.0, -1.0, -1.0, 1.0), Vector2::new(1.0, 1.0), Color32::new(255, 255, 255, 255), Color32::new(0, 0, 0, 0)),

            PackedVertex::new(Vector4::new( 1.0, -1.0, -1.0, 1.0), Vector2::new(1.0, 1.0), Color32::new(255, 255, 255, 255), Color32::new(0, 0, 0, 0)),
            PackedVertex::new(Vector4::new(-1.0, -1.0,  1.0, 1.0), Vector2::new(0.0, 0.0), Color32::new(255, 255, 255, 255), Color32::new(0, 0, 0, 0)),
            PackedVertex::new(Vector4::new( 1.0, -1.0,  1.0, 1.0), Vector2::new(1.0, 0.0), Color32::new(255, 255, 255, 255), Color32::new(0, 0, 0, 0)),
        ];

        Matrix4x4::transform_vertex_simd(&mut quad, offset_of!(PackedVertex => position));

        vdp::blend_func(vdp::BlendFactor::One, vdp::BlendFactor::Zero);
        vdp::depth_func(vdp::Compare::Always);
        vdp::depth_write(false);
        vdp::bind_texture(Some(tex));
        vdp::set_sample_params(vdp::TextureFilter::Linear, vdp::TextureWrap::Clamp, vdp::TextureWrap::Clamp);
        vdp::set_culling(false);

        vdp::draw_geometry_packed(vdp::Topology::TriangleList, &quad);
    }

    pub fn tick(self: &mut Self) {
        const DELTA: f32 = 1.0 / 60.0;

        vdp::clear_color(Color32::new(0, 0, 0, 255));
        vdp::clear_depth(1.0);

        let gp_state = self.gamepad.read_state();
        let lx = (gp_state.left_stick_x as f32) / (i16::MAX as f32);
        let ly = (gp_state.left_stick_y as f32) / (i16::MAX as f32);
        let rx = (gp_state.right_stick_x as f32) / (i16::MAX as f32);
        let ry = (gp_state.right_stick_y as f32) / (i16::MAX as f32);

        self.cam_x += rx * 45.0 * (1.0 / 60.0);
        self.cam_y -= ry * 45.0 * (1.0 / 60.0);

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

        let cam_rot = Quaternion::from_euler(Vector3::new(self.cam_y.to_radians(), 0.0, self.cam_x.to_radians()));
        let rot_mat = Matrix4x4::rotation(cam_rot);
        let fwd = rot_mat.mul(Vector4::new(0.0, -1.0, 0.0, 0.0));
        let right = rot_mat.mul(Vector4::new(1.0, 0.0, 0.0, 0.0));

        let fwd = Vector3::new(fwd.x, fwd.y, fwd.z);
        let right = Vector3::new(right.x, right.y, right.z);

        let new_pos = self.camera_position + (ly * fwd * 100.0 * DELTA) + (lx * right * 100.0 * DELTA);

        match self.map.recursive_linetrace(0, MASK_SOLID, &self.camera_position, &new_pos) {
            TraceResult::InSolid { .. } => {
                self.camera_position = new_pos;
            },
            TraceResult::Hit { t, .. } => {
                let delta = new_pos - self.camera_position;

                if delta.length_sq() > 0.0 {
                    let delta_mag = (delta.length() * t) - 0.1;
                    let delta = delta.normalized() * delta_mag;
                    self.camera_position = self.camera_position + delta;
                }
            },
            TraceResult::None => {
                self.camera_position = new_pos;
            }
        };

        let cam_proj = Matrix4x4::projection_perspective(640.0 / 480.0, (60.0_f32).to_radians(), 10.0, 10000.0);

        // draw skybox
        match &self.env {
            Some(v) => {
                GameState::draw_env_quad(&v[0], &Quaternion::identity(), &cam_rot, &cam_proj);
                GameState::draw_env_quad(&v[1], &Quaternion::from_euler(Vector3::new(0.0, 0.0, 180.0_f32.to_radians())), &cam_rot, &cam_proj);
                GameState::draw_env_quad(&v[2], &Quaternion::from_euler(Vector3::new(0.0, 0.0, 90.0_f32.to_radians())), &cam_rot, &cam_proj);
                GameState::draw_env_quad(&v[3], &Quaternion::from_euler(Vector3::new(0.0, 0.0, -90.0_f32.to_radians())), &cam_rot, &cam_proj);
                GameState::draw_env_quad(&v[4], &Quaternion::from_euler(Vector3::new(-90.0_f32.to_radians(), 0.0, -90.0_f32.to_radians())), &cam_rot, &cam_proj);
                GameState::draw_env_quad(&v[5], &Quaternion::from_euler(Vector3::new(90.0_f32.to_radians(), 0.0, -90.0_f32.to_radians())), &cam_rot, &cam_proj);
            }
            _ => {
            }
        };

        // draw map
        self.map.draw_map(DELTA, &self.camera_position, &cam_rot, &cam_proj);
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