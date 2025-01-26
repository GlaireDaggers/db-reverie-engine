use dbsdk_rs::{field_offset::offset_of, math::{Matrix4x4, Quaternion, Vector2, Vector3, Vector4}, vdp::{self, Color32, PackedVertex, Rectangle, Texture}};
use hecs::World;

use crate::{common, component::{camera::Camera, transform3d::Transform3D}, MapData, TimeData};

fn draw_env_quad(tex: &Texture, rotation: &Quaternion, camera_view: &Matrix4x4, camera_proj: &Matrix4x4) {
    // build view + projection matrix
    Matrix4x4::load_identity_simd();
    Matrix4x4::mul_simd(&Matrix4x4::scale(Vector3::new(100.0, 100.0, 100.0)));
    Matrix4x4::mul_simd(&Matrix4x4::rotation(*rotation));
    Matrix4x4::mul_simd(camera_view);
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

pub fn render_system(time: &TimeData, map_data: &mut MapData, env_data: &Option<[Texture;6]>, world: &mut World) {
    for (_, (transform, camera)) in world.query_mut::<(&Transform3D, &Camera)>() {
        // build view & projection matrices
        let mut cam_rot_inv = transform.rotation;
        cam_rot_inv.invert();

        let mut cam_view = Matrix4x4::identity();
        Matrix4x4::load_identity_simd();
        Matrix4x4::mul_simd(&Matrix4x4::translation(transform.position * -1.0));
        Matrix4x4::mul_simd(&Matrix4x4::rotation(cam_rot_inv));
        Matrix4x4::store_simd(&mut cam_view);

        let mut cam_env_view = Matrix4x4::identity();
        Matrix4x4::load_identity_simd();
        Matrix4x4::mul_simd(&Matrix4x4::rotation(cam_rot_inv));
        Matrix4x4::store_simd(&mut cam_env_view);

        let cam_proj = Matrix4x4::projection_perspective(640.0 / 480.0, camera.fov.to_radians(), camera.near, camera.far);

        match camera.viewport_rect {
            Some(v) => vdp::viewport(v),
            None => vdp::viewport(Rectangle::new(0, 0, 640, 480))
        };
        
        vdp::clear_color(Color32::new(0, 0, 0, 255));
        vdp::clear_depth(1.0);

        // draw skybox
        match env_data {
            Some(v) => {
                draw_env_quad(&v[0], &Quaternion::identity(), &cam_env_view, &cam_proj);
                draw_env_quad(&v[1], &Quaternion::from_euler(Vector3::new(0.0, 0.0, 180.0_f32.to_radians())), &cam_env_view, &cam_proj);
                draw_env_quad(&v[2], &Quaternion::from_euler(Vector3::new(0.0, 0.0, 90.0_f32.to_radians())), &cam_env_view, &cam_proj);
                draw_env_quad(&v[3], &Quaternion::from_euler(Vector3::new(0.0, 0.0, -90.0_f32.to_radians())), &cam_env_view, &cam_proj);
                draw_env_quad(&v[4], &Quaternion::from_euler(Vector3::new(-90.0_f32.to_radians(), 0.0, -90.0_f32.to_radians())), &cam_env_view, &cam_proj);
                draw_env_quad(&v[5], &Quaternion::from_euler(Vector3::new(90.0_f32.to_radians(), 0.0, -90.0_f32.to_radians())), &cam_env_view, &cam_proj);
            }
            _ => {
            }
        };

        // update with new camera position
        map_data.map_renderer.update(&map_data.map, &transform.position);

        // draw opaque geometry
        map_data.map_renderer.draw_opaque(&map_data.map, time.total_time, &cam_view, &cam_proj);

        // draw transparent geometry
        map_data.map_renderer.draw_transparent(&map_data.map, time.total_time, &cam_view, &cam_proj);
    }
}