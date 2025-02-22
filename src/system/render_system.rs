use std::sync::Arc;

use dbsdk_rs::{math::{Matrix4x4, Quaternion, Vector2, Vector3, Vector4}, vdp::{self, Color32, PackedVertex, Rectangle, Texture, TextureUnit, VertexSlotFormat}, vu_asm::vu_asm};
use hecs::World;

use crate::{bsp_file::{BspFile, MASK_SOLID}, bsp_renderer::{self, MapVertex}, common::{self, aabb_frustum, coord_space_transform, extract_frustum, transform_aabb}, component::{camera::Camera, light::Light, mapmodel::MapModel, mesh::{Mesh, SkeletalPoseState}, transform3d::Transform3D}, dbmesh::DBMeshPart, sh::SphericalHarmonics, MapData, TimeData};

// VU program which multiplies input vertex positions against a transform matrix, and input normals against a lighting matrix
const VU_TRANSFORM_AND_LIGHT: &[u32] = &vu_asm!{
    ld r0 0     // input position in r0
    ld r1 1     // input normal in r1
    ld r2 2     // input texcoord in r2
    ld r3 3     // input vertex color in r3
    ldc r4 0    // transform matrix column 0 in r4
    ldc r5 1    // transform matrix column 1 in r5
    ldc r6 2    // transform matrix column 2 in r6
    ldc r7 3    // transform matrix column 3 in r7
    ldc r8 4    // lighting matrix column 0 in r8
    ldc r9 5    // lighting matrix column 1 in r9
    ldc r10 6   // lighting matrix column 2 in r10
    ldc r11 7   // lighting matrix column 3 in r11
    ldc r12 8   // ocol in r12

    // transform position with MVP
    mulm r0 r4

    // transform normal with SH lighting matrix & multiply with vertex color
    mulm r1 r8
    mul r1 r3
    
    // output
    st pos r0
    st col r1
    st tex r2
    st ocol r12
};

#[derive(Clone, Copy)]
pub struct ModelVertex {
    pub position: Vector4,
    pub normal: Vector4,
    pub texcoord: Vector2,
    pub color: Color32
}

impl ModelVertex {
    pub fn new(position: Vector4, normal: Vector4, texcoord: Vector2, color: Color32) -> ModelVertex {
        ModelVertex { position, normal, texcoord, color }
    }
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

    vdp::depth_func(vdp::Compare::Always);
    vdp::depth_write(false);
    vdp::blend_func(vdp::BlendFactor::One, vdp::BlendFactor::Zero);
    vdp::bind_texture_slot::<Texture>(TextureUnit::TU0, None);

    let trs = (*camera_view) * common::coord_space_transform() * (*camera_proj);

    for vtx in &mut geo {
        vtx.position = trs * vtx.position;
    }

    vdp::submit_vu(vdp::Topology::LineList, &geo);

    todo!("draw_aabb will break if a non-default VU is loaded")
}

fn draw_env_quad(tex: &Texture, rotation: &Quaternion, camera_view: &Matrix4x4, camera_proj: &Matrix4x4) {
    // build view + projection matrix
    let trs = Matrix4x4::scale(Vector3::new(100.0, 100.0, 100.0))
        * Matrix4x4::rotation(*rotation)
        * (*camera_view)
        * common::coord_space_transform()
        * (*camera_proj);

    bsp_renderer::load_cdata_matrix(0, &trs);

    let quad = [
        MapVertex::new(Vector4::new(-1.0, -1.0, -1.0, 1.0), Vector2::new(0.0, 1.0), Vector2::zero(), Color32::new(255, 255, 255, 255)),
        MapVertex::new(Vector4::new(-1.0, -1.0,  1.0, 1.0), Vector2::new(0.0, 0.0), Vector2::zero(), Color32::new(255, 255, 255, 255)),
        MapVertex::new(Vector4::new( 1.0, -1.0, -1.0, 1.0), Vector2::new(1.0, 1.0), Vector2::zero(), Color32::new(255, 255, 255, 255)),

        MapVertex::new(Vector4::new( 1.0, -1.0, -1.0, 1.0), Vector2::new(1.0, 1.0), Vector2::zero(), Color32::new(255, 255, 255, 255)),
        MapVertex::new(Vector4::new(-1.0, -1.0,  1.0, 1.0), Vector2::new(0.0, 0.0), Vector2::zero(), Color32::new(255, 255, 255, 255)),
        MapVertex::new(Vector4::new( 1.0, -1.0,  1.0, 1.0), Vector2::new(1.0, 0.0), Vector2::zero(), Color32::new(255, 255, 255, 255)),
    ];

    vdp::blend_func(vdp::BlendFactor::One, vdp::BlendFactor::Zero);
    vdp::depth_func(vdp::Compare::Always);
    vdp::depth_write(false);
    vdp::bind_texture_slot::<Texture>(TextureUnit::TU0, Some(tex));
    vdp::bind_texture_slot::<Texture>(TextureUnit::TU1, None);
    vdp::set_sample_params_slot(TextureUnit::TU0, vdp::TextureFilter::Linear, vdp::TextureWrap::Clamp, vdp::TextureWrap::Clamp);
    vdp::set_culling(false);

    vdp::submit_vu(vdp::Topology::TriangleList, &quad);
}

fn setup_vu_lit_mesh() {
    // set up VU program
    vdp::upload_vu_program(VU_TRANSFORM_AND_LIGHT);

    // set up VU layout
    vdp::set_vu_stride(44);
    vdp::set_vu_layout(0, 0, VertexSlotFormat::FLOAT4);
    vdp::set_vu_layout(1, 16, VertexSlotFormat::FLOAT4);
    vdp::set_vu_layout(2, 32, VertexSlotFormat::FLOAT2);
    vdp::set_vu_layout(3, 40, VertexSlotFormat::UNORM4);
}

fn draw_static_meshpart(vtx_buffer: &mut Vec<ModelVertex>, meshpart: &DBMeshPart, mvp: &Matrix4x4, normal2world: &Matrix4x4, light: &SphericalHarmonics) {
    vtx_buffer.clear();

    // unpack mesh part vertices into GPU vertices
    for vertex in meshpart.vertices.as_slice() {
        let vtx = Vector4::new(vertex.pos[0].to_f32(), vertex.pos[1].to_f32(), vertex.pos[2].to_f32(), 1.0);
        let nrm = Vector4::new(vertex.nrm[0].to_f32(), vertex.nrm[1].to_f32(), vertex.nrm[2].to_f32(), 1.0);

        vtx_buffer.push(ModelVertex::new(
            vtx,
            nrm,
            Vector2::new(vertex.tex[0].to_f32(), vertex.tex[1].to_f32()),
            Color32::new(vertex.col[0], vertex.col[1], vertex.col[2], vertex.col[3])));
    }

    // load cdata
    let trs = meshpart.transform * (*mvp);
    bsp_renderer::load_cdata_matrix(0, &trs);

    let lightmat = meshpart.transform * (*normal2world) * light.coeff;
    bsp_renderer::load_cdata_matrix(4, &lightmat);

    vdp::set_vu_cdata(8, &Vector4::zero());

    // set render state
    vdp::depth_func(vdp::Compare::LessOrEqual);
    vdp::set_culling(meshpart.material.enable_cull);
    vdp::set_winding(vdp::WindingOrder::CounterClockwise);
    match &meshpart.material.texture {
        Some(v) => {
            vdp::bind_texture_slot::<Texture>(TextureUnit::TU0, Some(v.as_ref()));
        },
        None => {
            vdp::bind_texture_slot::<Texture>(TextureUnit::TU0, None);
        }
    };
    vdp::bind_texture_slot::<Texture>(TextureUnit::TU1, None);

    vdp::blend_equation(vdp::BlendEquation::Add);

    if meshpart.material.blend_enable {
        vdp::blend_func(vdp::BlendFactor::SrcAlpha, vdp::BlendFactor::OneMinusSrcAlpha);
        vdp::depth_write(false);
    } else {
        vdp::blend_func(vdp::BlendFactor::One, vdp::BlendFactor::Zero);
        vdp::depth_write(true);
    }

    // draw
    vdp::submit_vu(vdp::Topology::TriangleList, vtx_buffer.as_slice());
}

fn draw_skinned_meshpart(vtx_buffer: &mut Vec<ModelVertex>, meshpart: &DBMeshPart, mvp: &Matrix4x4, normal2world: &Matrix4x4, bonepalette: &[Matrix4x4], light: &SphericalHarmonics) {
    vtx_buffer.clear();
    
    // unpack mesh part vertices into GPU vertices
    for vertex in meshpart.vertices.as_slice() {
        let vtx = Vector4::new(vertex.pos[0].to_f32(), vertex.pos[1].to_f32(), vertex.pos[2].to_f32(), 1.0);
        let nrm = Vector4::new(vertex.nrm[0].to_f32(), vertex.nrm[1].to_f32(), vertex.nrm[2].to_f32(), 1.0);
        
        let mut sk0 = vtx;
        let mut sk1 = vtx;
        let mut nrm0 = nrm;
        let mut nrm1 = nrm;

        if vertex.bweight[0] > 0 {
            sk0 = bonepalette[vertex.bidx[0] as usize] * sk0;
            nrm0 = bonepalette[vertex.bidx[0] as usize] * nrm0;
        }

        if vertex.bweight[1] > 0 {
            sk1 = bonepalette[vertex.bidx[1] as usize] * sk1;
            nrm1 = bonepalette[vertex.bidx[1] as usize] * nrm1;
        }

        let weight0 = (vertex.bweight[0] as f32) / 255.0;
        let weight1 = (vertex.bweight[1] as f32) / 255.0;

        let vtx = (sk0 * weight0) + (sk1 * weight1);
        let nrm = (nrm0 * weight0) + (nrm1 * weight1);

        vtx_buffer.push(ModelVertex::new(
            vtx,
            nrm,
            Vector2::new(vertex.tex[0].to_f32(), vertex.tex[1].to_f32()),
            Color32::new(vertex.col[0], vertex.col[1], vertex.col[2], vertex.col[3])));
    }

    // load cdata
    let trs = meshpart.transform * (*mvp);
    bsp_renderer::load_cdata_matrix(0, &trs);

    let lightmat = meshpart.transform * (*normal2world) * light.coeff;
    bsp_renderer::load_cdata_matrix(4, &lightmat);

    vdp::set_vu_cdata(8, &Vector4::zero());

    // set render state
    vdp::depth_func(vdp::Compare::LessOrEqual);
    vdp::set_culling(meshpart.material.enable_cull);
    vdp::set_winding(vdp::WindingOrder::CounterClockwise);
    match &meshpart.material.texture {
        Some(v) => {
            vdp::bind_texture_slot::<Texture>(TextureUnit::TU0, Some(v.as_ref()));
        },
        None => {
            vdp::bind_texture_slot::<Texture>(TextureUnit::TU0, None);
        }
    };
    vdp::bind_texture_slot::<Texture>(TextureUnit::TU1, None);

    vdp::blend_equation(vdp::BlendEquation::Add);

    if meshpart.material.blend_enable {
        vdp::blend_func(vdp::BlendFactor::SrcAlpha, vdp::BlendFactor::OneMinusSrcAlpha);
        vdp::depth_write(false);
    } else {
        vdp::blend_func(vdp::BlendFactor::One, vdp::BlendFactor::Zero);
        vdp::depth_write(true);
    }

    // draw
    vdp::submit_vu(vdp::Topology::TriangleList, vtx_buffer.as_slice());
}

fn gather_lighting(light: &mut SphericalHarmonics, pos: &Vector3, lights: &[(Vector3, Vector3, f32)], bsp: &BspFile) {
    for (light_pos, light_color, light_radius) in lights {
        let dir = *light_pos - *pos;
        let dist = dir.length();

        if dist > 0.0 && dist < *light_radius {
            if bsp.linetrace(0, MASK_SOLID, pos, light_pos).fraction == 1.0 {
                let dir = dir / dist;
                let falloff = 1.0 - (dist / *light_radius);
                light.add_directional_light(dir, *light_color * falloff);
            }
        }
    }
}

/// System which performs all rendering (world + entities)
pub fn render_system(time: &TimeData, map_data: &mut MapData, env_data: &Option<[Arc<Texture>;6]>, world: &mut World) {
    // gather map models
    let mut mapmodel_iter = world.query::<(&MapModel, &Transform3D)>();
    let mapmodels = mapmodel_iter
        .iter()
        .collect::<Vec<_>>();

    // gather static meshes
    let mut mesh_iter = world.query::<(&Mesh, &Transform3D)>().without::<&SkeletalPoseState>();
    let meshes = mesh_iter
        .iter()
        .collect::<Vec<_>>();

    // gather skinned meshes
    let mut sk_mesh_iter = world.query::<(&Mesh, &Transform3D, &SkeletalPoseState)>();
    let sk_meshes = sk_mesh_iter
        .iter()
        .collect::<Vec<_>>();

    // gather lights
    let mut light_iter = world.query::<(&Transform3D, &Light)>();
    let lights = light_iter
        .iter()
        .collect::<Vec<_>>();

    // gather cameras
    let mut camera_iter = world.query::<(&Transform3D, &Camera)>();
    let cameras = camera_iter
        .iter()
        .collect::<Vec<_>>();

    let mut light_data = Vec::with_capacity(lights.len());

    let mut camera_index = 0;
    for (_, (transform, camera)) in cameras {
        // build view & projection matrices
        let mut cam_rot_inv = transform.rotation;
        cam_rot_inv.invert();

        let cam_view = Matrix4x4::translation(transform.position * -1.0)
            * Matrix4x4::rotation(cam_rot_inv);

        let cam_env_view = Matrix4x4::rotation(cam_rot_inv);

        let cam_proj = Matrix4x4::projection_perspective(640.0 / 480.0, camera.fov.to_radians(), camera.near, camera.far);

        // calculate camera frustum planes
        let viewproj = cam_view * common::coord_space_transform() * cam_proj;

        let frustum = extract_frustum(&viewproj);

        match camera.viewport_rect {
            Some(v) => vdp::viewport(v),
            None => vdp::viewport(Rectangle::new(0, 0, 640, 480))
        };
        
        vdp::clear_color(Color32::new(0, 0, 0, 255));
        vdp::clear_depth(1.0);

        // retrieve map renderer for camera
        map_data.update_renderer_cache(camera_index);
        let renderer = &mut map_data.map_renderers[camera_index];

        // update with new camera position
        renderer.update(&frustum, time.total_time, &map_data.light_layers, &map_data.map, &map_data.map_textures, &transform.position);

        // set up map VU layout & program
        bsp_renderer::setup_vu();

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

        // draw opaque geometry
        renderer.draw_opaque(&map_data.map, &map_data.map_textures, time.total_time, &cam_view, &cam_proj);

        // cull light sources
        light_data.clear();
        for (_, (light_transform, light)) in &lights {
            let light_bounds_extents = Vector3::new(light.max_radius, light.max_radius, light.max_radius);

            if renderer.check_vis(&map_data.map, light_transform.position, light_bounds_extents) {
                light_data.push((light_transform.position, light.color, light.max_radius));
            }
        }

        // gather visible models
        let mut visible_models = Vec::new();
        for (_, (model_info, model_transform)) in &mapmodels {
            let submodel = &map_data.map.submodel_lump.submodels[model_info.model_idx + 1];
            let bounds_extents = (submodel.maxs - submodel.mins) * 0.5;
            let bounds_center = model_transform.position + ((submodel.maxs + submodel.mins) * 0.5);

            let vis = aabb_frustum(bounds_center - bounds_extents, bounds_center + bounds_extents, &frustum) && renderer.check_vis(&map_data.map, bounds_center, bounds_extents);

            if vis {
                let model_mat = Matrix4x4::scale(model_transform.scale)
                    * Matrix4x4::rotation(model_transform.rotation)
                    * Matrix4x4::translation(model_transform.position);

                visible_models.push((model_mat, model_info.model_idx));
            }
        }

        // gather visible meshes
        let mut visible_meshes = Vec::new();
        for (_, (mesh, mesh_transform)) in &meshes {
            let model_mat = Matrix4x4::scale(mesh_transform.scale)
                * Matrix4x4::rotation(mesh_transform.rotation)
                * Matrix4x4::translation(mesh_transform.position);

            let (bounds_center, bounds_extents) = transform_aabb(mesh.bounds_offset, mesh.bounds_extents, &model_mat);

            // calculate lighting
            let mut light = SphericalHarmonics::new();
            light.add_ambient_light(Vector3::new(0.25, 0.1, 0.0));
            gather_lighting(&mut light, &bounds_center, &light_data, &map_data.map);

            let vis = aabb_frustum(bounds_center - bounds_extents, bounds_center + bounds_extents, &frustum) && renderer.check_vis(&map_data.map, bounds_center, bounds_extents);

            if vis {
                let normal2world = Matrix4x4::rotation(mesh_transform.rotation);
                visible_meshes.push((model_mat, light, normal2world, &mesh.mesh));
            }
        }

        // gather visible skinned meshes
        let mut visible_skinned_meshes = Vec::new();
        for (_, (mesh, mesh_transform, pose_state)) in &sk_meshes {
            let model_mat = Matrix4x4::scale(mesh_transform.scale)
                * Matrix4x4::rotation(mesh_transform.rotation)
                * Matrix4x4::translation(mesh_transform.position);

            let (bounds_center, bounds_extents) = transform_aabb(mesh.bounds_offset, mesh.bounds_extents, &model_mat);

            // calculate lighting
            let mut light = SphericalHarmonics::new();
            light.add_ambient_light(Vector3::new(0.25, 0.1, 0.0));
            gather_lighting(&mut light, &bounds_center, &light_data, &map_data.map);

            let vis = aabb_frustum(bounds_center - bounds_extents, bounds_center + bounds_extents, &frustum) && renderer.check_vis(&map_data.map, bounds_center, bounds_extents);

            if vis {
                let normal2world = Matrix4x4::rotation(mesh_transform.rotation);
                visible_skinned_meshes.push((model_mat, light, normal2world, &mesh.mesh, &pose_state.bone_palette));
            }
        }

        // draw models (opaque)
        for (transform, id) in &visible_models {
            map_data.map_models.draw_model_opaque(&map_data.map, time.total_time, &map_data.map_textures, *id, transform, &cam_view, &cam_proj);
        }

        let mut vtx_buffer = Vec::with_capacity(1024);

        // setup VU for drawing lit meshes
        setup_vu_lit_mesh();

        // draw static meshes
        for (local2world, light, normal2world, mesh) in &visible_meshes {
            let mvp = (*local2world) * cam_view * coord_space_transform() * cam_proj;

            for part in &mesh.mesh_parts {
                draw_static_meshpart(&mut vtx_buffer, part, &mvp, &normal2world, &light);
            }
        }

        // draw skinned meshes
        for (local2world, light, normal2world, mesh, pose_state) in &visible_skinned_meshes {
            let mvp = (*local2world) * cam_view * coord_space_transform() * cam_proj;

            for part in &mesh.mesh_parts {
                draw_skinned_meshpart(&mut vtx_buffer, part, &mvp, &normal2world, &pose_state, &light);
            }
        }

        // setup VU for map rendering
        bsp_renderer::setup_vu();

        // draw transparent geometry
        renderer.draw_transparent(&map_data.map, &map_data.map_textures, time.total_time, &cam_view, &cam_proj);

        // draw models (transparent)
        for (transform, id) in &visible_models {
            map_data.map_models.draw_model_transparent(&map_data.map, time.total_time, &map_data.map_textures, *id, transform, &cam_view, &cam_proj);
        }

        camera_index += 1;
    }
}