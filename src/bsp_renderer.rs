use std::{collections::HashMap, sync::Arc, vec};

use dbsdk_rs::{math::{Matrix4x4, Vector2, Vector3, Vector4}, vdp::{self, Color32, Rectangle, Texture, TextureUnit, VertexSlotFormat}, vu_asm::vu_asm};
use lazy_static::lazy_static;

use crate::{asset_loader::load_texture, bsp_file::{BspFile, Edge, SURF_NODRAW, SURF_NOLM, SURF_SKY, SURF_TRANS33, SURF_TRANS66, SURF_WARP}, common::{self, aabb_aabb_intersects, aabb_frustum}};

pub const NUM_CUSTOM_LIGHT_LAYERS: usize = 30;
pub const CUSTOM_LIGHT_LAYER_START: usize = 32;
pub const CUSTOM_LIGHT_LAYER_END: usize = CUSTOM_LIGHT_LAYER_START + NUM_CUSTOM_LIGHT_LAYERS;

const LM_SIZE: i32 = 512;

// basic VU program which multiplies input vertex positions against a transform matrix
const VU_BASIC_TRANSFORM: &[u32] = &vu_asm!{
    ld r0 0     // input position in r0
    ld r1 1     // input texcoord in r1
    ld r2 2     // input vertex color in r2
    ldc r3 0    // transform matrix column 0 in r3
    ldc r4 1    // transform matrix column 1 in r4
    ldc r5 2    // transform matrix column 2 in r5
    ldc r6 3    // transform matrix column 3 in r6
    ldc r7 4    // ocol in r7

    // transform position with MVP matrix in r3..r6
    mulm r0 r3
    
    // output
    st pos r0
    st tex r1
    st col r2
    st ocol r7
};

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

#[derive(Clone, Copy)]
pub struct MapVertex {
    pub position: Vector4,
    pub texcoord0: Vector2,
    pub texcoord1: Vector2,
    pub color: Color32,
}

impl MapVertex {
    pub fn new(position: Vector4, texcoord0: Vector2, texcoord1: Vector2, color: Color32) -> MapVertex {
        MapVertex {
            position,
            texcoord0,
            texcoord1,
            color
        }
    }
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

    pub fn pack(self: &mut Self, face_id: usize, width: usize, height: usize, anim: bool) -> (bool, Rectangle) {
        if self.cache.contains_key(&face_id) {
            return (true, self.cache[&face_id]);
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

        (false, result)
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
    geometry: Vec<(usize, Vec<MapVertex>, Vec<u16>)>
}

pub struct BspMapTextures {
    loaded_textures: Vec<Option<Arc<Texture>>>,
    err_tex: Texture,
    opaque_meshes: Vec<usize>,
    transp_meshes: Vec<usize>,
}

pub struct BspMapModelRenderer {
    models: Vec<Model>,
    lm_atlas: LmAtlasPacker,
    geo_buff: Vec<MapVertex>,
    geo_buff2: Vec<MapVertex>,
}

pub struct BspMapRenderer {
    vis: Vec<bool>,
    prev_leaf: i32,
    mesh_vertices: Vec<Vec<MapVertex>>,
    mesh_indices: Vec<Vec<u16>>,
    visible_leaves: Vec<bool>,
    lm_atlas: LmAtlasPacker,
    drawn_faces: Vec<bool>,
    geo_buff: Vec<MapVertex>,
    geo_buff2: Vec<MapVertex>,
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

fn unpack_face(bsp: &BspFile, textures: &BspMapTextures, face_idx: usize, edge_buffer: &mut Vec<Edge>, geo: &mut Vec<MapVertex>, index: &mut Vec<u16>, lm: &mut LmAtlasPacker) {
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
        let (in_cache, lm_region) = lm.pack(face_idx, lm_size_x, lm_size_y, face.num_lightmaps > 1);

        if !in_cache {
            let slice_start = (face.lightmap_offset / 3) as usize;
            let slice_end = slice_start + (lm_size_x * lm_size_y);
            let lm_slice = &bsp.lm_lump.lm[slice_start..slice_end];
    
            lm.lm.set_texture_data_region(0, Some(lm_region), lm_slice);
        }

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
    let idx_start = geo.len();

    for i in 0..edge_buffer.len() {
        let pos = edge_buffer[i].a as usize;
        let pos = bsp.vertex_lump.vertices[pos];

        let mut tex = Vector2::new(
            Vector3::dot(&pos, &tex_info.u_axis) + tex_info.u_offset,
            Vector3::dot(&pos, &tex_info.v_axis) + tex_info.v_offset
        );

        let lm = (((tex - tex_min) / (tex_max - tex_min)) * lm_region_scale) + lm_region_offset;

        match &textures.loaded_textures[tex_idx] {
            Some(v) => {
                let sc = Vector2::new(1.0 / v.width as f32, 1.0 / v.height as f32);
                tex = tex * sc;
            }
            None => {
                let sc = Vector2::new(1.0 / 64.0, 1.0 / 64.0);
                tex = tex * sc;
            }
        };

        let pos = Vector4::new(pos.x, pos.y, pos.z, 1.0);

        let vtx = MapVertex::new(pos, tex, lm, col);

        geo.push(vtx);
    }

    for i in 1..edge_buffer.len() - 1 {
        let idx0 = idx_start;
        let idx1 = idx_start + i;
        let idx2 = idx_start + i + 1;

        index.push(idx0 as u16);
        index.push(idx1 as u16);
        index.push(idx2 as u16);
    }
}

fn apply_warp(warp_time: f32, geo_buff: &mut Vec<MapVertex>) {
    for vtx in geo_buff {
        let os = vtx.position.x * 0.05;
        let ot = vtx.position.y * 0.05;

        vtx.texcoord0.x += (warp_time + ot).sin() * 0.1;
        vtx.texcoord0.y += (warp_time + os).cos() * 0.1;
    }
}

pub fn setup_vu() {
    // set up VU program
    vdp::upload_vu_program(VU_BASIC_TRANSFORM);

    // set up VU layout
    vdp::set_vu_stride(36);
    vdp::set_vu_layout(0, 0, VertexSlotFormat::FLOAT4);
    vdp::set_vu_layout(1, 16, VertexSlotFormat::FLOAT4);
    vdp::set_vu_layout(2, 32, VertexSlotFormat::UNORM4);
}

pub fn load_cdata_matrix(slot: usize, trs: &Matrix4x4) {
    vdp::set_vu_cdata(slot + 0, &trs.get_column(0));
    vdp::set_vu_cdata(slot + 1, &trs.get_column(1));
    vdp::set_vu_cdata(slot + 2, &trs.get_column(2));
    vdp::set_vu_cdata(slot + 3, &trs.get_column(3));
}

fn draw_opaque_geom_setup(model: &Matrix4x4, camera_view: &Matrix4x4, camera_proj: &Matrix4x4) {
    // build view + projection matrix
    let trs = (*model) * (*camera_view) * common::coord_space_transform() * (*camera_proj);

    // set up render state
    vdp::set_winding(vdp::WindingOrder::Clockwise);
    vdp::set_culling(true);
    vdp::depth_func(vdp::Compare::LessOrEqual);
    vdp::depth_write(true);
    vdp::blend_equation(vdp::BlendEquation::Add);
    vdp::blend_func(vdp::BlendFactor::One, vdp::BlendFactor::Zero);

    vdp::set_tex_combine(vdp::TexCombine::Mul, vdp::TexCombine::Mul);

    // load cdata
    load_cdata_matrix(0, &trs);
    vdp::set_vu_cdata(4, &Vector4::zero());
}

fn unpack_indexed(src: &[MapVertex], dst: &mut [MapVertex], idx: &[u16]) {
    for (i, v) in idx.iter().enumerate() {
        dst[i] = src[*v as usize];
    }
}

fn draw_geom(bsp: &BspFile, animation_time: f32, textures: &BspMapTextures, texture_index: usize, geo_buff: &mut Vec<MapVertex>, geo_buff2: &mut Vec<MapVertex>, m: &Vec<MapVertex>, idx: &Vec<u16>, lm: &LmAtlasPacker) {
    match &textures.loaded_textures[texture_index] {
        Some(v) => {
            vdp::bind_texture_slot::<Texture>(TextureUnit::TU0, Some(v));
            vdp::set_sample_params_slot(TextureUnit::TU0, vdp::TextureFilter::Linear, vdp::TextureWrap::Repeat, vdp::TextureWrap::Repeat);
        }
        None => {
            vdp::bind_texture_slot::<Texture>(TextureUnit::TU0, Some(&textures.err_tex));
            vdp::set_sample_params_slot(TextureUnit::TU0, vdp::TextureFilter::Nearest, vdp::TextureWrap::Repeat, vdp::TextureWrap::Repeat);
        }
    };

    if m.len() > 0 {
        geo_buff.clear();
        geo_buff.extend_from_slice(m);

        geo_buff2.clear();
        geo_buff2.reserve(idx.len());
        unsafe { geo_buff2.set_len(idx.len()) };

        if bsp.tex_info_lump.textures[texture_index].flags & SURF_WARP != 0 {
            apply_warp(animation_time, geo_buff);
        }

        if bsp.tex_info_lump.textures[texture_index].flags & SURF_NOLM == 0 {
            vdp::bind_texture_slot::<Texture>(TextureUnit::TU1, Some(&lm.lm));
        }
        else {
            vdp::bind_texture_slot::<Texture>(TextureUnit::TU1, None);
        }

        unpack_indexed(geo_buff, geo_buff2, idx);
        vdp::submit_vu(vdp::Topology::TriangleList, &geo_buff2);
    }
}

fn draw_transparent_geom_setup(model: &Matrix4x4, camera_view: &Matrix4x4, camera_proj: &Matrix4x4) {
    // build view + projection matrix
    let trs = (*model) * (*camera_view) * common::coord_space_transform() * (*camera_proj);

    // set up render state
    vdp::set_winding(vdp::WindingOrder::Clockwise);
    vdp::set_culling(true);
    vdp::blend_equation(vdp::BlendEquation::Add);

    vdp::depth_func(vdp::Compare::LessOrEqual);
    vdp::depth_write(false);
    vdp::blend_func(vdp::BlendFactor::SrcAlpha, vdp::BlendFactor::OneMinusSrcAlpha);

    // load cdata
    load_cdata_matrix(0, &trs);
    vdp::set_vu_cdata(4, &Vector4::zero());
}

impl BspMapTextures {
    pub fn new(bsp_file: &BspFile) -> BspMapTextures {
        // load unique textures
        let mut loaded_textures: Vec<Option<Arc<Texture>>> = Vec::new();

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

            let tex = match load_texture(format!("/cd/content/textures/{}.ktx", &tex_info.texture_name).as_str()) {
                Ok(v) => Some(v),
                Err(_) => None
            };

            loaded_textures.push(tex);
        }

        BspMapTextures {
            loaded_textures,
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
                let mut idx = Vec::new();

                let face = &bsp_file.face_lump.faces[face_idx];
                let tex_idx = face.texture_info as usize;

                unpack_face(bsp_file, textures, face_idx, &mut edges, &mut geom, &mut idx, &mut lm_atlas);

                model_geom.push((tex_idx, geom, idx));
            }

            models.push(Model {
                geometry: model_geom
            });
        }

        BspMapModelRenderer { models, lm_atlas, geo_buff: Vec::with_capacity(1024), geo_buff2: Vec::with_capacity(1024) }
    }

    /// Call each frame before rendering. Updates lightmap animation
    pub fn update(self: &BspMapModelRenderer, light_layers: &[f32;NUM_CUSTOM_LIGHT_LAYERS], bsp: &BspFile, animation_time: f32) {
        update_lm_animation(light_layers, animation_time, &self.lm_atlas, bsp);
    }

    /// Draw the opaque parts of a given map model
    pub fn draw_model_opaque(self: &mut Self, bsp: &BspFile, animation_time: f32, textures: &BspMapTextures, model_idx: usize, model_transform: &Matrix4x4, camera_view: &Matrix4x4, camera_proj: &Matrix4x4) {
        let model = &self.models[model_idx];

        draw_opaque_geom_setup(model_transform, camera_view, camera_proj);

        for (i, m, idx) in &model.geometry {
            let tex_info = &bsp.tex_info_lump.textures[*i];

            if tex_info.flags & SURF_TRANS33 == 0 && tex_info.flags & SURF_TRANS66 == 0 {
                draw_geom(bsp, animation_time, textures, *i, &mut self.geo_buff, &mut self.geo_buff2, m, idx, &self.lm_atlas);
            }
        }
    }

    /// Draw the transparent parts of a given map model
    pub fn draw_model_transparent(self: &mut Self, bsp: &BspFile, animation_time: f32, textures: &BspMapTextures, model_idx: usize, model_transform: &Matrix4x4, camera_view: &Matrix4x4, camera_proj: &Matrix4x4) {
        let model = &self.models[model_idx];

        draw_transparent_geom_setup(model_transform, camera_view, camera_proj);

        for (i, m, idx) in &model.geometry {
            let tex_info = &bsp.tex_info_lump.textures[*i];

            if tex_info.flags & SURF_TRANS33 != 0 || tex_info.flags & SURF_TRANS66 != 0 {
                draw_geom(bsp, animation_time, textures, *i, &mut self.geo_buff, &mut self.geo_buff2, m, idx, &self.lm_atlas);
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
            mesh_vertices: vec![Vec::new();num_textures],
            mesh_indices: vec![Vec::new();num_textures],
            drawn_faces: vec![false;num_faces],
            prev_leaf: -1,
            lm_atlas,
            geo_buff: Vec::with_capacity(1024),
            geo_buff2: Vec::with_capacity(1024),
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
        for m in &mut self.mesh_vertices {
            m.clear();
        }

        for idx in &mut self.mesh_indices {
            idx.clear();
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
                    unpack_face(bsp, textures, face_idx, &mut edges, &mut self.mesh_vertices[tex_idx], &mut self.mesh_indices[tex_idx], &mut self.lm_atlas);
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
        let corners = Self::get_bounds_corners(center, extents);
        return self.check_vis_recursive(bsp, 0, center, extents, &corners);
    }

    pub fn is_leaf_visible(self: &Self, leaf_index: usize) -> bool {
        return self.visible_leaves[leaf_index];
    }

    /// After updating a map, call this to render opaque geometry
    pub fn draw_opaque(self: &mut Self, bsp: &BspFile, textures: &BspMapTextures, animation_time: f32, camera_view: &Matrix4x4, camera_proj: &Matrix4x4) {
        draw_opaque_geom_setup(&Matrix4x4::identity(), camera_view, camera_proj);

        // bind lightmap texture
        vdp::bind_texture_slot(TextureUnit::TU1, Some(&self.lm_atlas.lm));

        for i in &textures.opaque_meshes {
            let m = &self.mesh_vertices[*i];
            let idx = &self.mesh_indices[*i];

            draw_geom(bsp, animation_time, textures, *i, &mut self.geo_buff, &mut self.geo_buff2, &m, &idx, &self.lm_atlas);
        }
    }

    /// After updating a map, call this to render transparent geometry
    pub fn draw_transparent(self: &mut Self, bsp: &BspFile, textures: &BspMapTextures, animation_time: f32, camera_view: &Matrix4x4, camera_proj: &Matrix4x4) {
        draw_transparent_geom_setup(&Matrix4x4::identity(), camera_view, camera_proj);

        for i in &textures.transp_meshes {
            let m = &self.mesh_vertices[*i];
            let idx = &self.mesh_indices[*i];

            draw_geom(bsp, animation_time, textures, *i, &mut self.geo_buff, &mut self.geo_buff2, &m, &idx, &self.lm_atlas);
        }
    }
}