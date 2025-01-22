use std::{fmt::format, ops::Mul, vec};

use dbsdk_rs::{db::{self, log}, field_offset::offset_of, math::{Matrix4x4, Quaternion, Vector2, Vector3, Vector4}, vdp::{self, Color32, PackedVertex}};

use crate::bsp_file::{BspFile, Edge, TextureType};

pub struct BspMap {
    file: BspFile,
    vis: Vec<bool>,
    prev_leaf: i32,
    meshes: Vec<Vec<PackedVertex>>,
    visible_leaves: Vec<bool>,
}

fn coord_space_transform() -> Matrix4x4 {
    // Quake coordinate system:
    // +X is right
    // +Y is towards viewer
    // +Z is up

    // DreamBox coordinate system:
    // +X is right
    // +Y is up
    // +Z is towards viewer

    Matrix4x4 {m: [
        [ 1.0,  0.0, 0.0, 0.0],
        [ 0.0,  0.0, 1.0, 0.0],
        [ 0.0,  1.0, 0.0, 0.0],
        [ 0.0,  0.0, 0.0, 1.0]
    ]}
}

impl BspMap {
    pub fn new(bsp_file: BspFile) -> BspMap {
        let num_clusters = bsp_file.vis_lump.clusters.len();
        let num_leaves = bsp_file.leaf_lump.leaves.len();
        let num_textures = bsp_file.tex_info_lump.textures.len();

        BspMap {
            file: bsp_file,
            vis: vec![false;num_clusters],
            visible_leaves: vec![false;num_leaves],
            meshes: vec![Vec::new();num_textures],
            prev_leaf: -1,
        }
    }

    pub fn draw_map(self: &mut Self, position: &Vector3, rotation: &Quaternion, camera_proj: &Matrix4x4) {
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

            let mut edges: Vec<Edge> = Vec::new();

            for i in 0..self.visible_leaves.len() {
                if self.visible_leaves[i] {
                    let leaf = &self.file.leaf_lump.leaves[i];
                    let start_face_idx = leaf.first_leaf_face as usize;
                    let end_face_idx: usize = start_face_idx + (leaf.num_leaf_faces as usize);

                    for leaf_face in start_face_idx..end_face_idx {
                        let face_idx = self.file.leaf_face_lump.faces[leaf_face] as usize;
                        let face = &self.file.face_lump.faces[face_idx];
                        let tex_idx = face.texture_info as usize;
                        let tex_info = &self.file.tex_info_lump.textures[tex_idx];
                        let plane = &self.file.plane_lump.planes[face.plane as usize];

                        let skip = match tex_info.tex_type {
                            TextureType::Sky => true,
                            TextureType::Skip => true,
                            // TextureType::Clip => true, // ???
                            _ => false
                        };

                        if skip {
                            continue;
                        }

                        let normal = plane.normal;
                        let col = Color32::new(
                            ((normal.x * 0.5 + 0.5) * 255.0) as u8,
                            ((normal.y * 0.5 + 0.5) * 255.0) as u8,
                            ((normal.z * 0.5 + 0.5) * 255.0) as u8,
                            128);

                        //let col = Color32::new(255, 255, 255, 255);

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

                        // build triangle fan out of edges (note: clockwise winding)
                        for i in 1..edges.len() {
                            let pos_a = edges[0].a as usize;
                            let pos_b = edges[i].a as usize;
                            let pos_c = edges[i].b as usize;

                            let pos_a = self.file.vertex_lump.vertices[pos_a];
                            let pos_b = self.file.vertex_lump.vertices[pos_b];
                            let pos_c = self.file.vertex_lump.vertices[pos_c];

                            let pos_a = Vector4::new(pos_a.x, pos_a.y, pos_a.z, 1.0);
                            let pos_b = Vector4::new(pos_b.x, pos_b.y, pos_b.z, 1.0);
                            let pos_c = Vector4::new(pos_c.x, pos_c.y, pos_c.z, 1.0);

                            let vtx_a = PackedVertex::new(pos_a, Vector2::zero(), col, 
                                Color32::new(0, 0, 0, 0));
                            let vtx_b = PackedVertex::new(pos_b, Vector2::zero(), col, 
                                Color32::new(0, 0, 0, 0));
                            let vtx_c = PackedVertex::new(pos_c, Vector2::zero(), col, 
                                Color32::new(0, 0, 0, 0));

                            self.meshes[tex_idx].push(vtx_a);
                            self.meshes[tex_idx].push(vtx_b);
                            self.meshes[tex_idx].push(vtx_c);
                        }
                    }
                }
            }
        }

        // build view + projection matrix
        Matrix4x4::load_identity_simd();
        Matrix4x4::mul_simd(&Matrix4x4::translation((*position).mul(-1.0)));
        Matrix4x4::mul_simd(&Matrix4x4::rotation({let mut r = *rotation; r.invert(); r}));
        Matrix4x4::mul_simd(&coord_space_transform());
        Matrix4x4::mul_simd(camera_proj);

        let mut geo_buff: Vec<PackedVertex> = Vec::with_capacity(1024);

        // set up render state
        vdp::depth_func(vdp::Compare::LessOrEqual);
        vdp::set_winding(vdp::WindingOrder::CounterClockwise);
        vdp::set_culling(true);
        vdp::bind_texture(None);

        for (i, m) in self.meshes.iter().enumerate() {
            let tex_info = &self.file.tex_info_lump.textures[i];

            match tex_info.tex_type {
                TextureType::Liquid => {
                    vdp::blend_equation(vdp::BlendEquation::Add);
                    vdp::blend_func(vdp::BlendFactor::SrcAlpha, vdp::BlendFactor::OneMinusSrcAlpha);
                    vdp::depth_write(false);
                }
                _ => {
                    vdp::blend_equation(vdp::BlendEquation::Add);
                    vdp::blend_func(vdp::BlendFactor::One, vdp::BlendFactor::Zero);
                    vdp::depth_write(true);
                }
            }

            if m.len() > 0 {
                geo_buff.clear();
                geo_buff.extend_from_slice(m);

                // transform vertices & draw
                Matrix4x4::transform_vertex_simd(&mut geo_buff, offset_of!(PackedVertex => position));
                vdp::draw_geometry_packed(vdp::Topology::TriangleList, &geo_buff);
            }
        }
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