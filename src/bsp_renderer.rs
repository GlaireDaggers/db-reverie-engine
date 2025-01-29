use std::{collections::HashMap, vec};

use dbsdk_rs::{db::log, field_offset::offset_of, math::{Matrix4x4, Vector2, Vector3, Vector4}, vdp::{self, Color32, PackedVertex, Rectangle, Texture}};
use lazy_static::lazy_static;

use crate::{asset_loader::load_texture, bsp_file::{BspFile, Edge, SURF_NODRAW, SURF_NOLM, SURF_SKY, SURF_TRANS33, SURF_TRANS66, SURF_WARP}, common::{self, aabb_aabb_intersects, aabb_frustum}};

pub const NUM_CUSTOM_LIGHT_LAYERS: usize = 30;
pub const CUSTOM_LIGHT_LAYER_START: usize = 32;
pub const CUSTOM_LIGHT_LAYER_END: usize = CUSTOM_LIGHT_LAYER_START + NUM_CUSTOM_LIGHT_LAYERS;

const LM_SIZE: i32 = 512;

// TODO: not sure how viable it is memory-wise to have one lightmap atlas per renderer/camera
// For now probably not worth worrying about until splitscreen support is actually needed

lazy_static! {
    static ref LIGHTSTYLES: [Vec<f32>;12] = [
        make_light_table(b"m"),
        make_light_table(b"mmnmmommommnonmmonqnmmo"),
        make_light_table(b"abcdefghijklmnopqrstuvwxyzyxwvutsrqponmlkjihgfedcba"),
        make_light_table(b"mmmmmaaaaammmmmaaaaaabcdefgabcdefg"),
        make_light_table(b"mamamamamama"),
        make_light_table(b"mamamamamamajklmnopqrstuvwxyzyxwvutsrqponmlkj"),
        make_light_table(b"nmonqnmomnmomomno"),
        make_light_table(b"mmmaaaabcdefgmmmmaaaammmaamm"),
        make_light_table(b"mmmaaammmaaammmabcdefaaaammmmabcdefmmmaaaa"),
        make_light_table(b"aaaaaaaazzzzzzzz"),
        make_light_table(b"mmamammmmammamamaaamammma"),
        make_light_table(b"abcdefghijklmnopqrrqponmlkjihgfedcba"),
    ];
}

// convert Quake-style light animation table to float array ('a' is minimum light, 'z' is maximum light)
fn make_light_table(data: &[u8]) -> Vec<f32> {
    let mut output = vec![0.0;data.len()];

    for i in 0..data.len() {
        output[i] = (data[i] - 97) as f32 / 25.0;
    }

    output
}

struct LmAtlasPacker {
    pub lm: Texture,
    pub cache: HashMap<usize, Rectangle>,
    pub anim_regions: Vec<usize>,
    lm_pack_x: usize,
    lm_pack_y: usize,
    lm_pack_y_max: usize
}

impl LmAtlasPacker {
    pub fn new(size: i32) -> LmAtlasPacker {
        LmAtlasPacker {
            lm: Texture::new(size, size, false, vdp::TextureFormat::RGBA8888).unwrap(),
            anim_regions: Vec::new(),
            cache: HashMap::new(),
            lm_pack_x: 0,
            lm_pack_y: 0,
            lm_pack_y_max: 0
        }
    }

    pub fn pack(self: &mut Self, face_id: usize, width: usize, height: usize, anim: bool) -> Rectangle {
        if self.cache.contains_key(&face_id) {
            return self.cache[&face_id];
        }

        let lm_width = self.lm.width as usize;
        let lm_height = self.lm.height as usize;

        if self.lm_pack_x + width > lm_width {
            self.lm_pack_x = 0;
            self.lm_pack_y += self.lm_pack_y_max;
            self.lm_pack_y_max = 0;
        }

        if self.lm_pack_x + width > lm_width || self.lm_pack_y + height > lm_height {
            panic!("Out of room in lightmap atlas!!");
        }

        let result = Rectangle::new(self.lm_pack_x as i32, self.lm_pack_y as i32, width as i32, height as i32);

        self.lm_pack_x += width;
        self.lm_pack_y_max = self.lm_pack_y_max.max(height);

        self.cache.insert(face_id, result);
        
        if anim {
            self.anim_regions.push(face_id);
        }

        result
    }

    pub fn reset(self: &mut Self) {
        self.lm_pack_x = 0;
        self.lm_pack_y = 0;
        self.lm_pack_y_max = 0;
        self.cache.clear();
        self.anim_regions.clear();
    }
}

struct Model {
    geometry: Vec<(usize, Vec<PackedVertex>, Vec<Vector2>)>
}

pub struct BspMapTextures {
    loaded_textures: Vec<Option<Texture>>,
    tex_ids: Vec<usize>,
    err_tex: Texture,
    opaque_meshes: Vec<usize>,
    transp_meshes: Vec<usize>,
}

pub struct BspMapModelRenderer {
    models: Vec<Model>,
    lm_atlas: LmAtlasPacker,
    geo_buff: Vec<PackedVertex>,
}

pub struct BspMapRenderer {
    vis: Vec<bool>,
    prev_leaf: i32,
    meshes: Vec<Vec<PackedVertex>>,
    lm_uvs: Vec<Vec<Vector2>>,
    visible_leaves: Vec<bool>,
    lm_atlas: LmAtlasPacker,
    drawn_faces: Vec<bool>,
    geo_buff: Vec<PackedVertex>,
}

fn update_lm_animation(light_layers: &[f32;NUM_CUSTOM_LIGHT_LAYERS], animation_time: f32, lm_atlas: &LmAtlasPacker, bsp: &BspFile) {
    // update animated lightmap regions
    let lightstyle_frame = (animation_time * 10.0) as usize;

    let mut lm_slice_buffer = [Color32::new(0, 0, 0, 255);16*16];
    for face_idx in &lm_atlas.anim_regions {
        let face = &bsp.face_lump.faces[*face_idx];
        let region = lm_atlas.cache[face_idx];

        let slice_len = (region.width * region.height) as usize;

        let lm_target_slice = &mut lm_slice_buffer[0..slice_len];
        lm_target_slice.fill(Color32::new(0, 0, 0, 255));

        for i in 0..face.num_lightmaps {
            let style = face.lightmap_styles[i] as usize;
            let sc = if style < LIGHTSTYLES.len() {
                // preset light style animation
                let table = &LIGHTSTYLES[style];
                table[lightstyle_frame % table.len()]
            }
            else if style >= CUSTOM_LIGHT_LAYER_START && style < CUSTOM_LIGHT_LAYER_END {
                light_layers[style - CUSTOM_LIGHT_LAYER_START]
            }
            else {
                1.0
            };

            let slice_start = (face.lightmap_offset / 3) as usize + (i * slice_len);
            let slice_end = slice_start + slice_len;
            let lm_src_slice = &bsp.lm_lump.lm[slice_start..slice_end];

            for j in 0..slice_len {
                lm_target_slice[j].r = lm_target_slice[j].r.saturating_add((lm_src_slice[j].r as f32 * sc).clamp(0.0, 255.0) as u8);
                lm_target_slice[j].g = lm_target_slice[j].g.saturating_add((lm_src_slice[j].g as f32 * sc).clamp(0.0, 255.0) as u8);
                lm_target_slice[j].b = lm_target_slice[j].b.saturating_add((lm_src_slice[j].b as f32 * sc).clamp(0.0, 255.0) as u8);
            }
        }

        lm_atlas.lm.set_texture_data_region(0, Some(region), lm_target_slice);
    }
}

fn unpack_face(bsp: &BspFile, textures: &BspMapTextures, face_idx: usize, edge_buffer: &mut Vec<Edge>, geo: &mut Vec<PackedVertex>, lm_uvs: &mut Vec<Vector2>, lm: &mut LmAtlasPacker) {
    let face = &bsp.face_lump.faces[face_idx];
    let tex_idx = face.texture_info as usize;
    let tex_info = &bsp.tex_info_lump.textures[tex_idx];

    if tex_info.flags & SURF_NODRAW != 0 {
        return;
    }

    if tex_info.flags & SURF_SKY != 0 {
        return;
    }

    let mut col = Color32::new(255, 255, 255, 255);

    if tex_info.flags & SURF_TRANS33 != 0 {
        col.a = 85;
    }
    else if tex_info.flags & SURF_TRANS66 != 0 {
        col.a = 170;
    }

    let start_edge_idx = face.first_edge as usize;
    let end_edge_idx = start_edge_idx + (face.num_edges as usize);

    edge_buffer.clear();
    for face_edge in start_edge_idx..end_edge_idx {
        let edge_idx = bsp.face_edge_lump.edges[face_edge];
        let reverse = edge_idx < 0;

        let edge = bsp.edge_lump.edges[edge_idx.abs() as usize];

        if reverse {
            edge_buffer.push(Edge{ a: edge.b, b: edge.a });
        }
        else {
            edge_buffer.push(edge);
        }
    }

    let mut tex_min = Vector2::new(f32::INFINITY, f32::INFINITY);
    let mut tex_max = Vector2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);

    // calculate lightmap UVs
    for i in 0..edge_buffer.len() {
        let e = &edge_buffer[i];

        let pos_a = bsp.vertex_lump.vertices[e.a as usize];
        let pos_b = bsp.vertex_lump.vertices[e.b as usize];

        let tex_a = Vector2::new(
            Vector3::dot(&pos_a, &tex_info.u_axis) + tex_info.u_offset,
            Vector3::dot(&pos_a, &tex_info.v_axis) + tex_info.v_offset
        );

        let tex_b = Vector2::new(
            Vector3::dot(&pos_b, &tex_info.u_axis) + tex_info.u_offset,
            Vector3::dot(&pos_b, &tex_info.v_axis) + tex_info.v_offset
        );

        tex_min.x = tex_min.x.min(tex_a.x);
        tex_min.y = tex_min.y.min(tex_a.y);
        tex_min.x = tex_min.x.min(tex_b.x);
        tex_min.y = tex_min.y.min(tex_b.y);

        tex_max.x = tex_max.x.max(tex_a.x);
        tex_max.y = tex_max.y.max(tex_a.y);
        tex_max.x = tex_max.x.max(tex_b.x);
        tex_max.y = tex_max.y.max(tex_b.y);
    }

    let lm_size_x = ((tex_max.x / 16.0).ceil() - (tex_min.x / 16.0).floor() + 1.0).trunc() as usize;
    let lm_size_y = ((tex_max.y / 16.0).ceil() - (tex_min.y / 16.0).floor() + 1.0).trunc() as usize;

    let lm_size_x = lm_size_x.clamp(1, 16);
    let lm_size_y = lm_size_y.clamp(1, 16);

    // upload region to lightmap atlas
    let (lm_region_offset, lm_region_scale) = if tex_info.flags & SURF_NOLM == 0 {
        let lm_region = lm.pack(face_idx, lm_size_x, lm_size_y, face.num_lightmaps > 1);
        let slice_start = (face.lightmap_offset / 3) as usize;
        let slice_end = slice_start + (lm_size_x * lm_size_y);
        let lm_slice = &bsp.lm_lump.lm[slice_start..slice_end];

        lm.lm.set_texture_data_region(0, Some(lm_region), lm_slice);

        // hack: scale lightmap UVs inwards to avoid bilinear sampling artifacts on borders
        // todo: should probably be padding these instead
        let lm_region_offset = Vector2::new((lm_region.x as f32 + 0.5) / lm.lm.width as f32, (lm_region.y as f32 + 0.5) / lm.lm.height as f32);
        let lm_region_scale = Vector2::new((lm_region.width as f32 - 1.0) / lm.lm.width as f32, (lm_region.height as f32 - 1.0) / lm.lm.height as f32);

        (lm_region_offset, lm_region_scale)
    }
    else {
        (Vector2::zero(), Vector2::zero())
    };

    // build triangle fan out of edges (note: clockwise winding)
    for i in 1..edge_buffer.len() - 1 {
        let pos_a = edge_buffer[0].a as usize;
        let pos_b = edge_buffer[i].a as usize;
        let pos_c = edge_buffer[i].b as usize;

        let pos_a = bsp.vertex_lump.vertices[pos_a];
        let pos_b = bsp.vertex_lump.vertices[pos_b];
        let pos_c = bsp.vertex_lump.vertices[pos_c];

        let mut tex_a = Vector2::new(
            Vector3::dot(&pos_a, &tex_info.u_axis) + tex_info.u_offset,
            Vector3::dot(&pos_a, &tex_info.v_axis) + tex_info.v_offset
        );

        let mut tex_b = Vector2::new(
            Vector3::dot(&pos_b, &tex_info.u_axis) + tex_info.u_offset,
            Vector3::dot(&pos_b, &tex_info.v_axis) + tex_info.v_offset
        );

        let mut tex_c = Vector2::new(
            Vector3::dot(&pos_c, &tex_info.u_axis) + tex_info.u_offset,
            Vector3::dot(&pos_c, &tex_info.v_axis) + tex_info.v_offset
        );

        let lm_a = (((tex_a - tex_min) / (tex_max - tex_min)) * lm_region_scale) + lm_region_offset;
        let lm_b = (((tex_b - tex_min) / (tex_max - tex_min)) * lm_region_scale) + lm_region_offset;
        let lm_c = (((tex_c - tex_min) / (tex_max - tex_min)) * lm_region_scale) + lm_region_offset;

        let tex_id = textures.tex_ids[tex_idx];
        match &textures.loaded_textures[tex_id] {
            Some(v) => {
                let sc = Vector2::new(1.0 / v.width as f32, 1.0 / v.height as f32);
                tex_a = tex_a * sc;
                tex_b = tex_b * sc;
                tex_c = tex_c * sc;
            }
            None => {
                let sc = Vector2::new(1.0 / 64.0, 1.0 / 64.0);
                tex_a = tex_a * sc;
                tex_b = tex_b * sc;
                tex_c = tex_c * sc;
            }
        };

        let pos_a = Vector4::new(pos_a.x, pos_a.y, pos_a.z, 1.0);
        let pos_b = Vector4::new(pos_b.x, pos_b.y, pos_b.z, 1.0);
        let pos_c = Vector4::new(pos_c.x, pos_c.y, pos_c.z, 1.0);

        let vtx_a = PackedVertex::new(pos_a, tex_a, col, 
            Color32::new(0, 0, 0, 0));
        let vtx_b = PackedVertex::new(pos_b, tex_b, col, 
            Color32::new(0, 0, 0, 0));
        let vtx_c = PackedVertex::new(pos_c, tex_c, col, 
            Color32::new(0, 0, 0, 0));

        geo.push(vtx_a);
        geo.push(vtx_b);
        geo.push(vtx_c);

        lm_uvs.push(lm_a);
        lm_uvs.push(lm_b);
        lm_uvs.push(lm_c);
    }
}

fn apply_warp(warp_time: f32, geo_buff: &mut Vec<PackedVertex>) {
    for vtx in geo_buff {
        let os = vtx.position.x * 0.05;
        let ot = vtx.position.y * 0.05;

        vtx.texcoord.x += (warp_time + ot).sin() * 0.1;
        vtx.texcoord.y += (warp_time + os).cos() * 0.1;
    }
}

fn draw_opaque_geom_setup(model: &Matrix4x4, camera_view: &Matrix4x4, camera_proj: &Matrix4x4) {
    // build view + projection matrix
    Matrix4x4::load_identity_simd();
    Matrix4x4::mul_simd(model);
    Matrix4x4::mul_simd(camera_view);
    Matrix4x4::mul_simd(&common::coord_space_transform());
    Matrix4x4::mul_simd(camera_proj);

    // set up render state
    vdp::set_winding(vdp::WindingOrder::Clockwise);
    vdp::set_culling(true);
    vdp::blend_equation(vdp::BlendEquation::Add);
}

fn draw_opaque_geom(bsp: &BspFile, animation_time: f32, textures: &BspMapTextures, texture_index: usize, geo_buff: &mut Vec<PackedVertex>, m: &Vec<PackedVertex>, lm_uvs: &Vec<Vector2>, lm: &LmAtlasPacker) {
    let tex_id = textures.tex_ids[texture_index];
    match &textures.loaded_textures[tex_id] {
        Some(v) => {
            vdp::bind_texture(Some(v));
            vdp::set_sample_params(vdp::TextureFilter::Linear, vdp::TextureWrap::Repeat, vdp::TextureWrap::Repeat);
        }
        None => {
            vdp::bind_texture(Some(&textures.err_tex));
            vdp::set_sample_params(vdp::TextureFilter::Nearest, vdp::TextureWrap::Repeat, vdp::TextureWrap::Repeat);
        }
    };

    if m.len() > 0 {
        geo_buff.clear();
        geo_buff.extend_from_slice(m);

        if bsp.tex_info_lump.textures[texture_index].flags & SURF_WARP != 0 {
            apply_warp(animation_time, geo_buff);
        }

        // transform vertices & draw
        vdp::depth_func(vdp::Compare::LessOrEqual);
        vdp::depth_write(true);
        vdp::blend_func(vdp::BlendFactor::One, vdp::BlendFactor::Zero);

        Matrix4x4::transform_vertex_simd(geo_buff, offset_of!(PackedVertex => position));
        vdp::draw_geometry_packed(vdp::Topology::TriangleList, &geo_buff);

        if bsp.tex_info_lump.textures[texture_index].flags & SURF_NOLM == 0 {
            // copy lightmap UVs & draw again (we don't need to re-transform)
            for i in 0..geo_buff.len() {
                geo_buff[i].texcoord = lm_uvs[i];
            }

            vdp::depth_func(vdp::Compare::Equal);
            vdp::depth_write(false);
            vdp::blend_func(vdp::BlendFactor::Zero, vdp::BlendFactor::SrcColor);

            vdp::bind_texture(Some(&lm.lm));
            vdp::set_sample_params(vdp::TextureFilter::Linear, vdp::TextureWrap::Repeat, vdp::TextureWrap::Repeat);
            vdp::draw_geometry_packed(vdp::Topology::TriangleList, &geo_buff);
        }
    }
}

fn draw_transparent_geom_setup(model: &Matrix4x4, camera_view: &Matrix4x4, camera_proj: &Matrix4x4) {
    // build view + projection matrix
    Matrix4x4::load_identity_simd();
    Matrix4x4::mul_simd(model);
    Matrix4x4::mul_simd(camera_view);
    Matrix4x4::mul_simd(&common::coord_space_transform());
    Matrix4x4::mul_simd(camera_proj);

    // set up render state
    vdp::set_winding(vdp::WindingOrder::Clockwise);
    vdp::set_culling(true);
    vdp::blend_equation(vdp::BlendEquation::Add);

    vdp::depth_func(vdp::Compare::LessOrEqual);
    vdp::depth_write(false);
    vdp::blend_func(vdp::BlendFactor::SrcAlpha, vdp::BlendFactor::OneMinusSrcAlpha);
}

fn draw_transparent_geom(bsp: &BspFile, animation_time: f32, textures: &BspMapTextures, texture_index: usize, geo_buff: &mut Vec<PackedVertex>, m: &Vec<PackedVertex>) {
    let tex_id = textures.tex_ids[texture_index];
    match &textures.loaded_textures[tex_id] {
        Some(v) => {
            vdp::bind_texture(Some(v));
            vdp::set_sample_params(vdp::TextureFilter::Linear, vdp::TextureWrap::Repeat, vdp::TextureWrap::Repeat);
        }
        None => {
            vdp::bind_texture(Some(&textures.err_tex));
            vdp::set_sample_params(vdp::TextureFilter::Nearest, vdp::TextureWrap::Repeat, vdp::TextureWrap::Repeat);
        }
    };

    if m.len() > 0 {
        geo_buff.clear();
        geo_buff.extend_from_slice(m);

        if bsp.tex_info_lump.textures[texture_index].flags & SURF_WARP != 0 {
            apply_warp(animation_time, geo_buff);
        }

        // transform vertices & draw
        Matrix4x4::transform_vertex_simd(geo_buff, offset_of!(PackedVertex => position));
        vdp::draw_geometry_packed(vdp::Topology::TriangleList, &geo_buff);
    }
}

impl BspMapTextures {
    pub fn new(bsp_file: &BspFile) -> BspMapTextures {
        // load unique textures
        let mut loaded_tex_names: Vec<&str> = Vec::new();
        let mut loaded_textures: Vec<Option<Texture>> = Vec::new();
        let mut tex_ids: Vec<usize> = Vec::new();

        let mut opaque_meshes: Vec<usize> = Vec::new();
        let mut transp_meshes: Vec<usize> = Vec::new();

        let err_tex = Texture::new(2, 2, false, vdp::TextureFormat::RGBA8888).unwrap();
        err_tex.set_texture_data(0, &[
            Color32::new(255, 0, 255, 255), Color32::new(0, 0, 0, 255),
            Color32::new(0, 0, 0, 255), Color32::new(255, 0, 255, 255)
        ]);

        for (i, tex_info) in bsp_file.tex_info_lump.textures.iter().enumerate() {
            if tex_info.flags & SURF_TRANS33 != 0 || tex_info.flags & SURF_TRANS66 != 0 {
                transp_meshes.push(i);
            }
            else {
                opaque_meshes.push(i);
            }

            match loaded_tex_names.iter().position(|&r| r == &tex_info.texture_name) {
                Some(i) => {
                    tex_ids.push(i);
                }
                None => {
                    // new texture
                    log(format!("Loading: {}", &tex_info.texture_name).as_str());

                    let tex = match load_texture(format!("/cd/content/textures/{}.ktx", &tex_info.texture_name).as_str()) {
                        Err(_) => {
                            log(format!("Failed loading {}", &tex_info.texture_name).as_str());
                            None
                        },
                        Ok(v) => Some(v)
                    };
                    let i = loaded_textures.len();
                    loaded_tex_names.push(&tex_info.texture_name);
                    loaded_textures.push(tex);
                    tex_ids.push(i);
                }
            }
        }

        BspMapTextures {
            loaded_textures,
            tex_ids,
            err_tex,
            opaque_meshes,
            transp_meshes
        }
    }
}

impl BspMapModelRenderer {
    pub fn new(bsp_file: &BspFile, textures: &BspMapTextures) -> BspMapModelRenderer {
        let mut lm_atlas = LmAtlasPacker::new(LM_SIZE);

        // build models
        let mut models = Vec::new();
        let mut edges = Vec::new();
        for i in 1..bsp_file.submodel_lump.submodels.len() {
            let model = &bsp_file.submodel_lump.submodels[i];
            let mut model_geom = Vec::new();

            let start_face_idx = model.first_face as usize;
            let end_face_idx: usize = start_face_idx + (model.num_faces as usize);

            for face_idx in start_face_idx..end_face_idx {
                let mut geom = Vec::new();
                let mut lm_uv = Vec::new();

                let face = &bsp_file.face_lump.faces[face_idx];
                let tex_idx = face.texture_info as usize;

                unpack_face(bsp_file, textures, face_idx, &mut edges, &mut geom, &mut lm_uv, &mut lm_atlas);

                model_geom.push((tex_idx, geom, lm_uv));
            }

            models.push(Model {
                geometry: model_geom
            });
        }

        BspMapModelRenderer { models, lm_atlas, geo_buff: Vec::with_capacity(1024) }
    }

    /// Call each frame before rendering. Updates lightmap animation
    pub fn update(self: &BspMapModelRenderer, light_layers: &[f32;NUM_CUSTOM_LIGHT_LAYERS], bsp: &BspFile, animation_time: f32) {
        update_lm_animation(light_layers, animation_time, &self.lm_atlas, bsp);
    }

    /// Draw the opaque parts of a given map model
    pub fn draw_model_opaque(self: &mut Self, bsp: &BspFile, animation_time: f32, textures: &BspMapTextures, model_idx: usize, model_transform: &Matrix4x4, camera_view: &Matrix4x4, camera_proj: &Matrix4x4) {
        let model = &self.models[model_idx];

        draw_opaque_geom_setup(model_transform, camera_view, camera_proj);

        for (i, m, lm_uvs) in &model.geometry {
            let tex_info = &bsp.tex_info_lump.textures[*i];

            if tex_info.flags & SURF_TRANS33 == 0 && tex_info.flags & SURF_TRANS66 == 0 {
                draw_opaque_geom(bsp, animation_time, textures, *i, &mut self.geo_buff, m, lm_uvs, &self.lm_atlas);
            }
        }
    }

    /// Draw the transparent parts of a given map model
    pub fn draw_model_transparent(self: &mut Self, bsp: &BspFile, animation_time: f32, textures: &BspMapTextures, model_idx: usize, model_transform: &Matrix4x4, camera_view: &Matrix4x4, camera_proj: &Matrix4x4) {
        let model = &self.models[model_idx];

        draw_transparent_geom_setup(model_transform, camera_view, camera_proj);

        for (i, m, _) in &model.geometry {
            let tex_info = &bsp.tex_info_lump.textures[*i];

            if tex_info.flags & SURF_TRANS33 != 0 || tex_info.flags & SURF_TRANS66 != 0 {
                draw_transparent_geom(bsp, animation_time, textures, *i, &mut self.geo_buff, m);
            }
        }
    }
}

impl BspMapRenderer {
    pub fn new(bsp_file: &BspFile) -> BspMapRenderer {
        let num_clusters = bsp_file.vis_lump.clusters.len();
        let num_leaves = bsp_file.leaf_lump.leaves.len();
        let num_textures = bsp_file.tex_info_lump.textures.len();
        let num_faces = bsp_file.face_lump.faces.len();

        let lm_atlas = LmAtlasPacker::new(LM_SIZE);

        BspMapRenderer {
            vis: vec![false;num_clusters],
            visible_leaves: vec![false;num_leaves],
            meshes: vec![Vec::new();num_textures],
            lm_uvs: vec![Vec::new();num_textures],
            drawn_faces: vec![false;num_faces],
            prev_leaf: -1,
            lm_atlas,
            geo_buff: Vec::with_capacity(1024)
        }
    }

    fn update_leaf(bsp: &BspFile, leaf_index: usize, visible_clusters: &[bool], visible_leaves: &mut [bool]) {
        let leaf = &bsp.leaf_lump.leaves[leaf_index];
        if leaf.cluster == u16::MAX {
            return;
        }

        if visible_clusters[leaf.cluster as usize] {
            visible_leaves[leaf_index] = true;
        }
    }

    fn update_recursive(bsp: &BspFile, cur_node: i32, frustum: &[Vector4], visible_clusters: &[bool], visible_leaves: &mut [bool]) {
        if cur_node < 0 {
            Self::update_leaf(bsp, (-cur_node - 1) as usize, visible_clusters, visible_leaves);
            return;
        }

        let node = &bsp.node_lump.nodes[cur_node as usize];

        if !aabb_frustum(node._bbox_min, node._bbox_max, frustum) {
            return;
        }

        Self::update_recursive(bsp, node.front_child, frustum, visible_clusters, visible_leaves);
        Self::update_recursive(bsp, node.back_child, frustum, visible_clusters, visible_leaves);
    }

    /// Call each frame before rendering. Recalculates visible leaves, rebuilds geometry and lightmap atlas, & updates lightmap animation
    pub fn update(self: &mut Self, frustum: &[Vector4], anim_time: f32, light_layers: &[f32;NUM_CUSTOM_LIGHT_LAYERS], bsp: &BspFile, textures: &BspMapTextures, position: &Vector3) {
        let leaf_index = bsp.calc_leaf_index(position);
        let leaf = &bsp.leaf_lump.leaves[leaf_index as usize];

        // if camera enters a new cluster, unpack new cluster's visibility info
        if leaf_index != self.prev_leaf {
            self.prev_leaf = leaf_index;
            
            self.vis.fill(false);
            if leaf.cluster != u16::MAX {
                bsp.vis_lump.unpack_vis(leaf.cluster as usize, &mut self.vis);
            }

            // clear lightmap cache
            self.lm_atlas.reset();
        }

        self.visible_leaves.fill(false);
        Self::update_recursive(bsp, 0, frustum, &self.vis, &mut self.visible_leaves);

        // build geometry for visible leaves
        for m in &mut self.meshes {
            m.clear();
        }

        for m in &mut self.lm_uvs {
            m.clear();
        }

        let mut edges: Vec<Edge> = Vec::new();

        // faces might be shared by multiple leaves. keep track of them so we don't draw them more than once
        self.drawn_faces.fill(false);

        for i in 0..self.visible_leaves.len() {
            if self.visible_leaves[i] {
                let leaf = &bsp.leaf_lump.leaves[i];
                let start_face_idx = leaf.first_leaf_face as usize;
                let end_face_idx: usize = start_face_idx + (leaf.num_leaf_faces as usize);

                for leaf_face in start_face_idx..end_face_idx {
                    let face_idx = bsp.leaf_face_lump.faces[leaf_face] as usize;

                    if self.drawn_faces[face_idx] {
                        continue;
                    }

                    self.drawn_faces[face_idx] = true;

                    let face = &bsp.face_lump.faces[face_idx];
                    let tex_idx = face.texture_info as usize;
                    unpack_face(bsp, textures, face_idx, &mut edges, &mut self.meshes[tex_idx], &mut self.lm_uvs[tex_idx], &mut self.lm_atlas);
                }
            }
        }

        update_lm_animation(light_layers, anim_time, &self.lm_atlas, bsp);
    }

    fn get_bounds_corners(center: Vector3, extents: Vector3) -> [Vector3;8] {
        [
            center + Vector3::new(-extents.x, -extents.y, -extents.z),
            center + Vector3::new( extents.x, -extents.y, -extents.z),
            center + Vector3::new(-extents.x,  extents.y, -extents.z),
            center + Vector3::new( extents.x,  extents.y, -extents.z),
            center + Vector3::new(-extents.x, -extents.y,  extents.z),
            center + Vector3::new( extents.x, -extents.y,  extents.z),
            center + Vector3::new(-extents.x,  extents.y,  extents.z),
            center + Vector3::new( extents.x,  extents.y,  extents.z),
        ]
    }

    fn check_vis_leaf(self: &Self, bsp: &BspFile, leaf_index: usize, center: Vector3, extents: Vector3) -> bool {
        if !self.visible_leaves[leaf_index] {
            return false;
        }

        let min = center - extents;
        let max = center + extents;

        let leaf = &bsp.leaf_lump.leaves[leaf_index];

        return aabb_aabb_intersects(min, max, leaf.bbox_min, leaf.bbox_max);
    }

    fn check_vis_recursive(self: &Self, bsp: &BspFile, node_index: i32, center: Vector3, extents: Vector3, corners: &[Vector3;8]) -> bool {
        if node_index < 0 {
            return self.check_vis_leaf(bsp, (-node_index - 1) as usize, center, extents);
        }

        let node = &bsp.node_lump.nodes[node_index as usize];
        let split_plane = &bsp.plane_lump.planes[node.plane as usize];

        let mut dmin = f32::MAX;
        let mut dmax = f32::MIN;

        for i in 0..8 {
            let d = Vector3::dot(&corners[i], &split_plane.normal) - split_plane.distance;

            if d < dmin {
                dmin = d;
            }

            if d > dmax {
                dmax = d;
            }
        }

        if dmax >= 0.0 {
            if self.check_vis_recursive(bsp, node.front_child, center, extents, corners) {
                return true;
            }
        }

        if dmin <= 0.0 {
            if self.check_vis_recursive(bsp, node.back_child, center, extents, corners) {
                return true;
            }
        }

        return false;
    }

    pub fn check_vis(self: &Self, bsp: &BspFile, center: Vector3, extents: Vector3) -> bool {
        /*for leaf_idx in 0..self.visible_leaves.len() {
            if self.check_vis_leaf(bsp, leaf_idx, center, extents) {
                return true;
            }
        }
        return false;*/
        let corners = Self::get_bounds_corners(center, extents);
        return self.check_vis_recursive(bsp, 0, center, extents, &corners);
    }

    pub fn is_leaf_visible(self: &Self, leaf_index: usize) -> bool {
        return self.visible_leaves[leaf_index];
    }

    /// After updating a map, call this to render opaque geometry
    pub fn draw_opaque(self: &mut Self, bsp: &BspFile, textures: &BspMapTextures, animation_time: f32, camera_view: &Matrix4x4, camera_proj: &Matrix4x4) {
        draw_opaque_geom_setup(&Matrix4x4::identity(), camera_view, camera_proj);

        for i in &textures.opaque_meshes {
            let m = &self.meshes[*i];
            let lm_uvs = &self.lm_uvs[*i];

            draw_opaque_geom(bsp, animation_time, textures, *i, &mut self.geo_buff, &m, &lm_uvs, &self.lm_atlas);
        }
    }

    /// After updating a map, call this to render transparent geometry
    pub fn draw_transparent(self: &mut Self, bsp: &BspFile, textures: &BspMapTextures, animation_time: f32, camera_view: &Matrix4x4, camera_proj: &Matrix4x4) {
        draw_transparent_geom_setup(&Matrix4x4::identity(), camera_view, camera_proj);

        for i in &textures.transp_meshes {
            let m = &self.meshes[*i];

            draw_transparent_geom(bsp, animation_time, textures, *i, &mut self.geo_buff, &m);
        }
    }
}