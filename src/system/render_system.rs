use dbsdk_rs::{field_offset::offset_of, math::{Matrix4x4, Quaternion, Vector2, Vector3, Vector4}, vdp::{self, Color32, PackedVertex, Rectangle, Texture}};
use hecs::World;

use crate::{common, component::{camera::Camera, mapmodel::MapModel, transform3d::Transform3D}, MapData, TimeData};

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

fn _draw_aabb(center: Vector3, extents: Vector3, camera_view: &Matrix4x4, camera_proj: &Matrix4x4, col: Color32) {
    let c0 = center + Vector3::new(-extents.x, -extents.y, -extents.z);
    let c1 = center + Vector3::new( extents.x, -extents.y, -extents.z);
    let c2 = center + Vector3::new(-extents.x,  extents.y, -extents.z);
    let c3 = center + Vector3::new( extents.x,  extents.y, -extents.z);
    let c4 = center + Vector3::new(-extents.x, -extents.y,  extents.z);
    let c5 = center + Vector3::new( extents.x, -extents.y,  extents.z);
    let c6 = center + Vector3::new(-extents.x,  extents.y,  extents.z);
    let c7 = center + Vector3::new( extents.x,  extents.y,  extents.z);

    let c0 = Vector4::new(c0.x, c0.y, c0.z, 1.0);
    let c1 = Vector4::new(c1.x, c1.y, c1.z, 1.0);
    let c2 = Vector4::new(c2.x, c2.y, c2.z, 1.0);
    let c3 = Vector4::new(c3.x, c3.y, c3.z, 1.0);
    let c4 = Vector4::new(c4.x, c4.y, c4.z, 1.0);
    let c5 = Vector4::new(c5.x, c5.y, c5.z, 1.0);
    let c6 = Vector4::new(c6.x, c6.y, c6.z, 1.0);
    let c7 = Vector4::new(c7.x, c7.y, c7.z, 1.0);

    let ocol = Color32::new(0, 0, 0, 0);

    let mut geo = vec![
        PackedVertex::new(c0, Vector2::zero(), col, ocol),
        PackedVertex::new(c1, Vector2::zero(), col, ocol),
        PackedVertex::new(c2, Vector2::zero(), col, ocol),
        PackedVertex::new(c3, Vector2::zero(), col, ocol),
        PackedVertex::new(c0, Vector2::zero(), col, ocol),
        PackedVertex::new(c2, Vector2::zero(), col, ocol),
        PackedVertex::new(c1, Vector2::zero(), col, ocol),
        PackedVertex::new(c3, Vector2::zero(), col, ocol),

        PackedVertex::new(c4, Vector2::zero(), col, ocol),
        PackedVertex::new(c5, Vector2::zero(), col, ocol),
        PackedVertex::new(c6, Vector2::zero(), col, ocol),
        PackedVertex::new(c7, Vector2::zero(), col, ocol),
        PackedVertex::new(c4, Vector2::zero(), col, ocol),
        PackedVertex::new(c6, Vector2::zero(), col, ocol),
        PackedVertex::new(c5, Vector2::zero(), col, ocol),
        PackedVertex::new(c7, Vector2::zero(), col, ocol),

        PackedVertex::new(c0, Vector2::zero(), col, ocol),
        PackedVertex::new(c4, Vector2::zero(), col, ocol),
        PackedVertex::new(c1, Vector2::zero(), col, ocol),
        PackedVertex::new(c5, Vector2::zero(), col, ocol),
        PackedVertex::new(c2, Vector2::zero(), col, ocol),
        PackedVertex::new(c6, Vector2::zero(), col, ocol),
        PackedVertex::new(c3, Vector2::zero(), col, ocol),
        PackedVertex::new(c7, Vector2::zero(), col, ocol),
    ];

    Matrix4x4::load_identity_simd();
    Matrix4x4::mul_simd(camera_view);
    Matrix4x4::mul_simd(&common::coord_space_transform());
    Matrix4x4::mul_simd(camera_proj);

    Matrix4x4::transform_vertex_simd(&mut geo, offset_of!(PackedVertex => position));
    vdp::draw_geometry_packed(vdp::Topology::LineList, &geo);
}

/// System which performs all rendering (world + entities)
pub fn render_system(time: &TimeData, map_data: &mut MapData, env_data: &Option<[Texture;6]>, world: &mut World) {
    // gather map models
    let mut mapmodel_iter = world.query::<(&MapModel, &Transform3D)>();
    let mapmodels = mapmodel_iter
        .iter()
        .map(|(e, c)| (e, c))
        .collect::<Vec<_>>();

    // gather cameras
    let mut camera_iter = world.query::<(&Transform3D, &Camera)>();
    let cameras = camera_iter
        .iter()
        .map(|(e, c)| (e, c))
        .collect::<Vec<_>>();

    let mut camera_index = 0;
    for (_, (transform, camera)) in cameras {
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

        // retrieve map renderer for camera
        map_data.update_renderer_cache(camera_index);
        let renderer = &mut map_data.map_renderers[camera_index];

        // update with new camera position
        renderer.update(time.total_time, &map_data.light_layers, &map_data.map, &map_data.map_textures, &transform.position);

        // draw opaque geometry
        renderer.draw_opaque(&map_data.map, &map_data.map_textures, time.total_time, &cam_view, &cam_proj);

        vdp::depth_func(vdp::Compare::Always);
        vdp::bind_texture(None);
        vdp::blend_func(vdp::BlendFactor::One, vdp::BlendFactor::Zero);

        // draw models
        let mut model_mat: Matrix4x4 = Matrix4x4::identity();
        for (_, (model_info, model_transform)) in &mapmodels {
            let submodel = &map_data.map.submodel_lump.submodels[model_info.model_idx + 1];
            let bounds_extents = (submodel.maxs - submodel.mins) * 0.5;
            let bounds_center = model_transform.position + ((submodel.maxs + submodel.mins) * 0.5);

            let vis = renderer.check_vis(&map_data.map, bounds_center, bounds_extents);

            if vis {
                // build model matrix
                Matrix4x4::load_identity_simd();
                Matrix4x4::mul_simd(&Matrix4x4::scale(model_transform.scale));
                Matrix4x4::mul_simd(&Matrix4x4::rotation(model_transform.rotation));
                Matrix4x4::mul_simd(&Matrix4x4::translation(model_transform.position));
                Matrix4x4::store_simd(&mut model_mat);

                map_data.map_models.draw_model(&map_data.map_textures, model_info.model_idx, &model_mat, &cam_view, &cam_proj);
            }
        }

        // draw transparent geometry
        renderer.draw_transparent(&map_data.map, &map_data.map_textures, time.total_time, &cam_view, &cam_proj);
        
        camera_index += 1;
    }
}