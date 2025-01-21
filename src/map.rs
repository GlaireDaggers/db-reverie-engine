use std::ops::Mul;

use dbsdk_rs::{db, field_offset::offset_of, math::{Matrix4x4, Quaternion, Vector2, Vector3, Vector4}, vdp::{self, Color32, PackedVertex}};

use crate::bsp_file::{BspFile, Edge};

pub struct BspMap {
    file: BspFile,
    vis: Vec<bool>,
    prev_leaf: i32,
    meshes: Vec<Vec<PackedVertex>>,
    clusters: Vec<usize>,
}

impl BspMap {
    pub fn new(bsp_file: BspFile) -> BspMap {
        let num_clusters = bsp_file.vis_lump.clusters.len();
        let num_textures = bsp_file.tex_info_lump.textures.len();

        let mut clusters: Vec<usize> = vec![0;num_clusters];
        for i in 0..bsp_file.leaf_lump.leaves.len() {
            let leaf = &bsp_file.leaf_lump.leaves[i];
            if leaf.cluster != u16::MAX {
                let cluster_idx = leaf.cluster as usize;
                if cluster_idx < num_clusters {
                    clusters[cluster_idx] = i;
                }
                else {
                    db::log(format!("WARNING: Leaf cluster out of range (index: {}, num clusters: {})", cluster_idx, num_clusters).as_str());
                }
            }
        }

        BspMap {
            file: bsp_file,
            vis: vec![false;num_clusters],
            meshes: vec![Vec::new();num_textures],
            clusters: clusters,
            prev_leaf: -1,
        }
    }

    pub fn draw_map(self: &mut Self, position: &Vector3, rotation: &Quaternion, camera_proj: &Matrix4x4) {
        let leaf_index = self.calc_leaf_index(position);

        // if camera enters a new cluster, unpack new cluster's visibility info & build geometry
        if leaf_index != self.prev_leaf {
            db::log(format!("Entered leaf: {}", leaf_index).as_str());
            
            self.prev_leaf = leaf_index;
            let leaf = &self.file.leaf_lump.leaves[leaf_index as usize];
            
            if leaf.cluster != u16::MAX {
                self.file.vis_lump.unpack_vis(leaf.cluster as usize, &mut self.vis);
            }
            else {
                self.vis.fill(false);
            }

            // build geometry for visible clusters
            for m in &mut self.meshes {
                m.clear();
            }

            let mut edges: Vec<Edge> = Vec::new();

            for i in 0..self.vis.len() {
                if self.vis[i] {
                    let leaf = &self.file.leaf_lump.leaves[self.clusters[i]];
                    let start_face_idx = leaf.first_leaf_face as usize;
                    let end_face_idx = start_face_idx + (leaf.num_leaf_faces as usize);

                    for leaf_face in start_face_idx..end_face_idx {
                        let face_idx = self.file.leaf_face_lump.faces[leaf_face] as usize;
                        let face = &self.file.face_lump.faces[face_idx];
                        let tex_idx = face.texture_info as usize;
                        let tex_info = &self.file.tex_info_lump.textures[tex_idx];

                        let start_edge_idx = face.first_edge as usize;
                        let end_edge_idx = start_edge_idx + (face.num_edges as usize);

                        edges.clear();
                        for face_edge in start_edge_idx..end_edge_idx {
                            let edge_idx = self.file.face_edge_lump.edges[face_edge];

                            if edge_idx >= 0 {
                                let edge = self.file.edge_lump.edges[edge_idx as usize];
                                edges.push(edge);
                            }
                            else {
                                let edge_idx = -edge_idx;
                                let edge = self.file.edge_lump.edges[edge_idx as usize];
                                edges.push(Edge{ a: edge.b, b: edge.a });
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

                            let vtx_a = PackedVertex::new(pos_a, Vector2::zero(), Color32::new(255, 255, 255, 255), 
                                Color32::new(0, 0, 0, 0));
                            let vtx_b = PackedVertex::new(pos_b, Vector2::zero(), Color32::new(255, 255, 255, 255), 
                                Color32::new(0, 0, 0, 0));
                            let vtx_c = PackedVertex::new(pos_c, Vector2::zero(), Color32::new(255, 255, 255, 255), 
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
        let m = Matrix4x4::translation((*position).mul(-1.0));
        Matrix4x4::load_simd(&m);
        let mut r = *rotation; r.invert();
        let m = Matrix4x4::rotation(r);
        Matrix4x4::mul_simd(&m);
        Matrix4x4::mul_simd(camera_proj);

        let mut geo_buff: Vec<PackedVertex> = Vec::with_capacity(1024);

        // set up render state
        vdp::blend_equation(vdp::BlendEquation::Add);
        vdp::blend_func(vdp::BlendFactor::One, vdp::BlendFactor::Zero);
        vdp::depth_write(true);
        vdp::depth_func(vdp::Compare::LessOrEqual);
        vdp::set_winding(vdp::WindingOrder::Clockwise);
        vdp::set_culling(true);
        vdp::bind_texture(None);

        for m in &self.meshes {
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