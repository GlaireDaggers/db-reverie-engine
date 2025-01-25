use std::{collections::HashSet, ops::Mul, vec};

use dbsdk_rs::{db::log, field_offset::offset_of, math::{Matrix4x4, Quaternion, Vector2, Vector3, Vector4}, vdp::{self, Color32, PackedVertex, Rectangle, Texture}};

use crate::{asset_loader::load_texture, bsp_file::{BspFile, Edge, SURF_NODRAW, SURF_SKY, SURF_TRANS33, SURF_TRANS66, SURF_WARP}, common};

const DEBUG_DRAW_LM: bool = false;
const DIST_EPSILON: f32 = 0.01;

pub struct BspMap {
    pub file: BspFile,
    vis: Vec<bool>,
    prev_leaf: i32,
    meshes: Vec<Vec<PackedVertex>>,
    lm_uvs: Vec<Vec<Vector2>>,
    visible_leaves: Vec<bool>,
    loaded_textures: Vec<Option<Texture>>,
    tex_ids: Vec<usize>,
    err_tex: Texture,
    lm_atlas: Texture,
    drawn_faces: Vec<bool>,
    opaque_meshes: Vec<usize>,
    transp_meshes: Vec<usize>,
    warp_time: f32,
}

pub struct Trace {
    pub all_solid: bool,
    pub start_solid: bool,
    pub fraction: f32,
    pub end_pos: Vector3,
    pub plane: i32
}

impl BspMap {
    pub fn new(bsp_file: BspFile) -> BspMap {
        let num_clusters = bsp_file.vis_lump.clusters.len();
        let num_leaves = bsp_file.leaf_lump.leaves.len();
        let num_textures = bsp_file.tex_info_lump.textures.len();
        let num_faces = bsp_file.face_lump.faces.len();

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

        let lm_atlas = Texture::new(512, 512, false, vdp::TextureFormat::RGB565).unwrap();

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

        BspMap {
            file: bsp_file,
            vis: vec![false;num_clusters],
            visible_leaves: vec![false;num_leaves],
            meshes: vec![Vec::new();num_textures],
            lm_uvs: vec![Vec::new();num_textures],
            drawn_faces: vec![false;num_faces],
            prev_leaf: -1,
            tex_ids,
            loaded_textures,
            err_tex,
            lm_atlas,
            opaque_meshes,
            transp_meshes,
            warp_time: 0.0,
        }
    }

    pub fn draw_map(self: &mut Self, delta: f32, position: &Vector3, rotation: &Quaternion, camera_proj: &Matrix4x4) {
        self.warp_time += delta;

        let leaf_index = self.calc_leaf_index(position);

        // if camera enters a new cluster, unpack new cluster's visibility info & build geometry
        if leaf_index != self.prev_leaf {
            self.prev_leaf = leaf_index;
            let leaf = &self.file.leaf_lump.leaves[leaf_index as usize];
            
            self.vis.fill(false);
            if leaf.cluster != u16::MAX {
                self.file.vis_lump.unpack_vis(leaf.cluster as usize, &mut self.vis);
            }

            // mark visible leaves
            for i in 0..self.visible_leaves.len() {
                let leaf = &self.file.leaf_lump.leaves[i];
                self.visible_leaves[i] = leaf.cluster != u16::MAX && self.vis[leaf.cluster as usize];
            }

            // build geometry for visible leaves
            for m in &mut self.meshes {
                m.clear();
            }

            for m in &mut self.lm_uvs {
                m.clear();
            }

            let mut edges: Vec<Edge> = Vec::new();

            let mut lm_pack_x = 0;
            let mut lm_pack_y = 0;
            let mut lm_pack_y_max = 0;

            let lm_width = self.lm_atlas.width as usize;
            let lm_height = self.lm_atlas.height as usize;

            // faces might be shared by multiple leaves. keep track of them so we don't draw them more than once
            self.drawn_faces.fill(false);

            for i in 0..self.visible_leaves.len() {
                if self.visible_leaves[i] {
                    let leaf = &self.file.leaf_lump.leaves[i];
                    let start_face_idx = leaf.first_leaf_face as usize;
                    let end_face_idx: usize = start_face_idx + (leaf.num_leaf_faces as usize);

                    for leaf_face in start_face_idx..end_face_idx {
                        let face_idx = self.file.leaf_face_lump.faces[leaf_face] as usize;

                        if self.drawn_faces[face_idx] {
                            continue;
                        }

                        self.drawn_faces[face_idx] = true;

                        let face = &self.file.face_lump.faces[face_idx];
                        let tex_idx = face.texture_info as usize;
                        let tex_info = &self.file.tex_info_lump.textures[tex_idx];

                        if tex_info.flags & SURF_NODRAW != 0 {
                            continue;
                        }
                        if tex_info.flags & SURF_SKY != 0 {
                            continue;
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

                        edges.clear();
                        for face_edge in start_edge_idx..end_edge_idx {
                            let edge_idx = self.file.face_edge_lump.edges[face_edge];
                            let reverse = edge_idx < 0;

                            let edge = self.file.edge_lump.edges[edge_idx.abs() as usize];

                            if reverse {
                                edges.push(Edge{ a: edge.b, b: edge.a });
                            }
                            else {
                                edges.push(edge);
                            }
                        }

                        let mut tex_min = Vector2::new(f32::INFINITY, f32::INFINITY);
                        let mut tex_max = Vector2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);

                        // calculate lightmap UVs
                        for e in &edges {
                            let pos_a = self.file.vertex_lump.vertices[e.a as usize];
                            let pos_b = self.file.vertex_lump.vertices[e.b as usize];

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

                        if lm_pack_x + lm_size_x > lm_width {
                            lm_pack_x = 0;
                            lm_pack_y += lm_pack_y_max;
                            lm_pack_y_max = 0;
                        }

                        if lm_pack_x + lm_size_x > lm_width || lm_pack_y + lm_size_y > lm_height {
                            panic!("Out of room in lightmap atlas!!");
                        }

                        // upload region to lightmap atlas
                        let slice_start = (face.lightmap_offset / 3) as usize;
                        let slice_end = slice_start + (lm_size_x * lm_size_y);
                        let lm_slice = &self.file.lm_lump.lm[slice_start..slice_end];

                        self.lm_atlas.set_texture_data_region(0, Some(Rectangle::new(lm_pack_x as i32, lm_pack_y as i32, lm_size_x as i32, lm_size_y as i32)), lm_slice);

                        // hack: scale lightmap UVs inwards to avoid bilinear sampling artifacts on borders
                        // todo: should probably be padding these instead
                        let lm_region_offset = Vector2::new((lm_pack_x + 1) as f32 / lm_width as f32, (lm_pack_y + 1) as f32 / lm_height as f32);
                        let lm_region_scale = Vector2::new((lm_size_x.saturating_sub(2)) as f32 / lm_width as f32, (lm_size_y.saturating_sub(2)) as f32 / lm_height as f32);

                        lm_pack_x += lm_size_x;
                        lm_pack_y_max = lm_pack_y_max.max(lm_size_y);

                        // build triangle fan out of edges (note: clockwise winding)
                        for i in 1..edges.len() {
                            let pos_a = edges[0].a as usize;
                            let pos_b = edges[i].a as usize;
                            let pos_c = edges[i].b as usize;

                            let pos_a = self.file.vertex_lump.vertices[pos_a];
                            let pos_b = self.file.vertex_lump.vertices[pos_b];
                            let pos_c = self.file.vertex_lump.vertices[pos_c];

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

                            let tex_id = self.tex_ids[tex_idx];
                            match &self.loaded_textures[tex_id] {
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

                            self.meshes[tex_idx].push(vtx_a);
                            self.meshes[tex_idx].push(vtx_b);
                            self.meshes[tex_idx].push(vtx_c);

                            self.lm_uvs[tex_idx].push(lm_a);
                            self.lm_uvs[tex_idx].push(lm_b);
                            self.lm_uvs[tex_idx].push(lm_c);
                        }
                    }
                }
            }
        }

        // build view + projection matrix
        Matrix4x4::load_identity_simd();
        Matrix4x4::mul_simd(&Matrix4x4::translation((*position).mul(-1.0)));
        Matrix4x4::mul_simd(&Matrix4x4::rotation({let mut r = *rotation; r.invert(); r}));
        Matrix4x4::mul_simd(&common::coord_space_transform());
        Matrix4x4::mul_simd(camera_proj);

        let mut geo_buff: Vec<PackedVertex> = Vec::with_capacity(1024);

        // set up render state
        vdp::set_winding(vdp::WindingOrder::CounterClockwise);
        vdp::set_culling(true);
        vdp::blend_equation(vdp::BlendEquation::Add);

        for i in &self.opaque_meshes {
            let m = &self.meshes[*i];
            let lm_uvs = &self.lm_uvs[*i];

            let tex_id = self.tex_ids[*i];
            match &self.loaded_textures[tex_id] {
                Some(v) => {
                    vdp::bind_texture(Some(v));
                    vdp::set_sample_params(vdp::TextureFilter::Linear, vdp::TextureWrap::Repeat, vdp::TextureWrap::Repeat);
                }
                None => {
                    vdp::bind_texture(Some(&self.err_tex));
                    vdp::set_sample_params(vdp::TextureFilter::Nearest, vdp::TextureWrap::Repeat, vdp::TextureWrap::Repeat);
                }
            };

            if m.len() > 0 {
                geo_buff.clear();
                geo_buff.extend_from_slice(m);

                if self.file.tex_info_lump.textures[*i].flags & SURF_WARP != 0 {
                    // warp UVs
                    for vtx in &mut geo_buff {
                        let os = vtx.position.x * 0.05;
                        let ot = vtx.position.y * 0.05;

                        vtx.texcoord.x += (self.warp_time + ot).sin() * 0.1;
                        vtx.texcoord.y += (self.warp_time + os).cos() * 0.1;
                    }
                }

                // transform vertices & draw
                vdp::depth_func(vdp::Compare::LessOrEqual);
                vdp::depth_write(true);
                vdp::blend_func(vdp::BlendFactor::One, vdp::BlendFactor::Zero);

                Matrix4x4::transform_vertex_simd(&mut geo_buff, offset_of!(PackedVertex => position));
                vdp::draw_geometry_packed(vdp::Topology::TriangleList, &geo_buff);

                // copy lightmap UVs & draw again (we don't need to re-transform)
                for i in 0..geo_buff.len() {
                    geo_buff[i].texcoord = lm_uvs[i];
                }

                vdp::depth_func(vdp::Compare::Equal);
                vdp::depth_write(false);
                vdp::blend_func(vdp::BlendFactor::Zero, vdp::BlendFactor::SrcColor);

                vdp::bind_texture(Some(&self.lm_atlas));
                vdp::set_sample_params(vdp::TextureFilter::Linear, vdp::TextureWrap::Repeat, vdp::TextureWrap::Repeat);
                vdp::draw_geometry_packed(vdp::Topology::TriangleList, &geo_buff);
            }
        }

        vdp::depth_func(vdp::Compare::LessOrEqual);
        vdp::depth_write(false);
        vdp::blend_func(vdp::BlendFactor::SrcAlpha, vdp::BlendFactor::OneMinusSrcAlpha);

        for i in &self.transp_meshes {
            let m = &self.meshes[*i];

            let tex_id = self.tex_ids[*i];
            match &self.loaded_textures[tex_id] {
                Some(v) => {
                    vdp::bind_texture(Some(v));
                    vdp::set_sample_params(vdp::TextureFilter::Linear, vdp::TextureWrap::Repeat, vdp::TextureWrap::Repeat);
                }
                None => {
                    vdp::bind_texture(Some(&self.err_tex));
                    vdp::set_sample_params(vdp::TextureFilter::Nearest, vdp::TextureWrap::Repeat, vdp::TextureWrap::Repeat);
                }
            };

            if m.len() > 0 {
                geo_buff.clear();
                geo_buff.extend_from_slice(m);

                if self.file.tex_info_lump.textures[*i].flags & SURF_WARP != 0 {
                    // warp UVs
                    for vtx in &mut geo_buff {
                        let os = vtx.position.x * 0.05;
                        let ot = vtx.position.y * 0.05;

                        vtx.texcoord.x += (self.warp_time + ot).sin() * 0.1;
                        vtx.texcoord.y += (self.warp_time + os).cos() * 0.1;
                    }
                }

                // transform vertices & draw
                vdp::depth_func(vdp::Compare::LessOrEqual);
                Matrix4x4::transform_vertex_simd(&mut geo_buff, offset_of!(PackedVertex => position));
                vdp::draw_geometry_packed(vdp::Topology::TriangleList, &geo_buff);
            }
        }

        if DEBUG_DRAW_LM {
            let quad = [
                PackedVertex::new(Vector4::new(-1.0, -1.0, 0.0, 1.0), Vector2::new(0.0, 0.0), Color32::new(255, 255, 255, 255), Color32::new(0, 0, 0, 0)),
                PackedVertex::new(Vector4::new(-1.0, 0.0, 0.0, 1.0), Vector2::new(0.0, 1.0), Color32::new(255, 255, 255, 255), Color32::new(0, 0, 0, 0)),
                PackedVertex::new(Vector4::new(0.0, -1.0, 0.0, 1.0), Vector2::new(1.0, 0.0), Color32::new(255, 255, 255, 255), Color32::new(0, 0, 0, 0)),

                PackedVertex::new(Vector4::new(0.0, -1.0, 0.0, 1.0), Vector2::new(1.0, 0.0), Color32::new(255, 255, 255, 255), Color32::new(0, 0, 0, 0)),
                PackedVertex::new(Vector4::new(-1.0, 0.0, 0.0, 1.0), Vector2::new(0.0, 1.0), Color32::new(255, 255, 255, 255), Color32::new(0, 0, 0, 0)),
                PackedVertex::new(Vector4::new(0.0, 0.0, 0.0, 1.0), Vector2::new(1.0, 1.0), Color32::new(255, 255, 255, 255), Color32::new(0, 0, 0, 0)),
            ];

            vdp::depth_func(vdp::Compare::Always);
            vdp::depth_write(false);
            vdp::bind_texture(Some(&self.lm_atlas));
            vdp::blend_func(vdp::BlendFactor::One, vdp::BlendFactor::Zero);
            vdp::set_culling(false);
            vdp::draw_geometry_packed(vdp::Topology::TriangleList, &quad);
        }
    }

    fn trace_brush(self: &Self, brush_idx: usize, start: &Vector3, end: &Vector3, frac_adj: f32, box_extents: Option<&Vector3>, trace: &mut Trace) {
        let brush = &self.file.brush_lump.brushes[brush_idx];

        if brush.num_brush_sides == 0 {
            return;
        }

        let mut hitplane = -1;
        let mut enterfrac = f32::MIN;
        let mut exitfrac = 1.0;
        let mut startout = false;
        let mut getout = false;

        for i in 0..brush.num_brush_sides {
            let side = &self.file.brush_side_lump.brush_sides[(brush.first_brush_side + i) as usize];
            let plane = &self.file.plane_lump.planes[side.plane as usize];

            let dist = match box_extents {
                Some(v) => {
                    let offs = Vector3::new(
                        if plane.normal.x < 0.0 { v.x } else { -v.x },
                        if plane.normal.y < 0.0 { v.y } else { -v.y },
                        if plane.normal.z < 0.0 { v.z } else { -v.z }
                    );

                    plane.distance - Vector3::dot(&offs, &plane.normal)
                }
                None => {
                    plane.distance
                }
            };

            let d1 = Vector3::dot(start, &plane.normal) - dist;
            let d2 = Vector3::dot(end, &plane.normal) - dist;

            if d2 > 0.0 {
                getout = true;
            }

            if d1 > 0.0 {
                startout = true;
            }

            if d1 > 0.0 && d2 >= d1 {
                return;
            }

            if d1 <= 0.0 && d2 <= 0.0 {
                continue;
            }

            if d1 > d2 {
                let f = (d1 - DIST_EPSILON) / (d1 - d2);
                if f > enterfrac {
                    enterfrac = f;
                    hitplane = side.plane as i32;
                }
            }
            else {
                let f = (d1 + DIST_EPSILON) / (d1 - d2);
                if f < exitfrac {
                    exitfrac = f;
                }
            }
        }
        
        if !startout {
            trace.start_solid = true;
            if !getout {
                trace.all_solid = true;
            }

            return;
        }

        if enterfrac < exitfrac {
            if enterfrac > f32::MIN && enterfrac < trace.fraction {
                if enterfrac < 0.0 {
                    enterfrac = 0.0;
                }

                trace.fraction = enterfrac + frac_adj;
                trace.plane = hitplane;
            }
        }
    }

    fn trace_leaf(self: &Self, leaf_index: usize, checked_brush: &mut HashSet<u16>, content_mask: u32, start: &Vector3, end: &Vector3, frac_adj: f32, box_extents: Option<&Vector3>, trace: &mut Trace) {
        let leaf = &self.file.leaf_lump.leaves[leaf_index];

        if leaf.contents & content_mask == 0 {
            return;
        }

        // linetrace all brushes in leaf
        for i in 0..leaf.num_leaf_brushes {
            let brush_idx = self.file.leaf_brush_lump.brushes[(leaf.first_leaf_brush + i) as usize];
            
            // ensure we don't process the same brush more than once during a trace
            if checked_brush.contains(&brush_idx) {
                continue;
            }
            checked_brush.insert(brush_idx);

            let brush = &self.file.brush_lump.brushes[brush_idx as usize];

            if brush.contents & content_mask == 0 {
                return;
            }

            self.trace_brush(brush_idx as usize, start, end, frac_adj, box_extents, trace);

            if trace.fraction <= 0.0 {
                return;
            }
        }
    }

    fn recursive_trace(self: &Self, node_idx: i32, checked_brush: &mut HashSet<u16>, content_mask: u32, p1f: f32, p2f: f32, start: &Vector3, end: &Vector3, frac_adj: f32, box_extents: Option<&Vector3>, trace: &mut Trace) {
        if trace.fraction <= p1f {
            return;
        }
        
        if node_idx < 0 {
            self.trace_leaf((-node_idx - 1) as usize, checked_brush, content_mask, start, end, frac_adj, box_extents, trace);
            return;
        }

        let node = &self.file.node_lump.nodes[node_idx as usize];
        let plane = &self.file.plane_lump.planes[node.plane as usize];

        let (t1, t2, offset) = if plane.plane_type == 0 {
            let t1 = start.x - plane.distance;
            let t2 = end.x - plane.distance;
            let offset = match box_extents {
                Some(v) => {
                    v.x
                }
                None => {
                    0.0
                }
            };

            (t1, t2, offset)
        }
        else if plane.plane_type == 1 {
            let t1 = start.y - plane.distance;
            let t2 = end.y - plane.distance;
            let offset = match box_extents {
                Some(v) => {
                    v.y
                }
                None => {
                    0.0
                }
            };

            (t1, t2, offset)
        }
        else if plane.plane_type == 2 {
            let t1 = start.z - plane.distance;
            let t2 = end.z - plane.distance;
            let offset = match box_extents {
                Some(v) => {
                    v.z
                }
                None => {
                    0.0
                }
            };

            (t1, t2, offset)
        }
        else {
            let t1 = Vector3::dot(&plane.normal, start) - plane.distance;
            let t2 = Vector3::dot(&plane.normal, end) - plane.distance;
            let offset = match box_extents {
                Some(v) => {
                    (v.x * plane.normal.x).abs() +
                    (v.y * plane.normal.y).abs() +
                    (v.z * plane.normal.z).abs()
                },
                None => {
                    0.0
                }
            };

            (t1, t2, offset)
        };

        if t1 >= offset && t2 >= offset {
            self.recursive_trace(node.front_child, checked_brush, content_mask, p1f, p2f, start, end, frac_adj, box_extents, trace);
            return;
        }

        if t1 < -offset && t2 < -offset {
            self.recursive_trace(node.back_child, checked_brush, content_mask, p1f, p2f, start, end, frac_adj, box_extents, trace);
            return;
        }

        self.recursive_trace(node.front_child, checked_brush, content_mask, p1f, p2f, start, end, frac_adj, box_extents, trace);
        self.recursive_trace(node.back_child, checked_brush, content_mask, p1f, p2f, start, end, frac_adj, box_extents, trace);

        /*let (side, frac2, frac) = if t1 < t2 {
            let idist = 1.0 / (t1 - t2);
            (
                true,
                (t1 + offset + DIST_EPSILON)*idist,
                (t1 - offset - DIST_EPSILON)*idist
            )
        }
        else if t1 > t2 {
            let idist = 1.0 / (t1 - t2);
            (
                false,
                (t1 - offset - DIST_EPSILON)*idist,
                (t1 + offset + DIST_EPSILON)*idist
            )
        }
        else {
            (
                false,
                0.0,
                1.0
            )
        };

        // move up to the node
        let frac = frac.clamp(0.0, 1.0);

        let midf = p1f + ((p2f - p1f) * frac);
        let mid = *start + ((*end - *start) * frac);

        self.recursive_trace(if side { node.back_child } else { node.front_child }, checked_brush, content_mask, p1f, midf, start, &mid, frac_adj, box_extents, trace);

        // go past the node
        let frac2 = frac2.clamp(0.0, 1.0);

        let midf = p1f + ((p2f - p1f) * frac2);
        let mid = *start + ((*end - *start) * frac2);

        self.recursive_trace(if side { node.front_child } else { node.back_child }, checked_brush, content_mask, midf, p2f, &mid, end, frac_adj + frac2, box_extents, trace);*/
    }

    pub fn boxtrace(self: &Self, content_mask: u32, start: &Vector3, end: &Vector3, box_extents: Vector3) -> Trace {
        let head_node = self.file.submodel_lump.submodels[0].headnode as i32;

        let mut trace_trace = Trace {
            all_solid: false,
            start_solid: false,
            fraction: 1.0,
            end_pos: Vector3::zero(),
            plane: -1
        };

        self.recursive_trace(head_node, &mut HashSet::<u16>::new(), content_mask, 0.0, 1.0, start, end, 0.0, Some(&box_extents), &mut trace_trace);

        if trace_trace.fraction == 1.0 {
            trace_trace.end_pos = *end;
        }
        else {
            trace_trace.end_pos = *start + ((*end - *start) * trace_trace.fraction);
        }

        trace_trace
    }

    pub fn calc_leaf_index(self: &Self, position: &Vector3) -> i32 {
        let mut cur_node: i32 = 0;

        while cur_node >= 0 {
            let node = &self.file.node_lump.nodes[cur_node as usize];
            let plane = &self.file.plane_lump.planes[node.plane as usize];

            // what side of the plane is this point on
            let side = Vector3::dot(&position, &plane.normal) - plane.distance;
            if side >= 0.0 {
                cur_node = node.front_child;
            }
            else {
                cur_node = node.back_child;
            }
        }

        // leaf indices are encoded as negative numbers: -(leaf_idx + 1)
        return -cur_node - 1;
    }
}