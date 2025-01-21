use std::io::Seek;

use byteorder::{LittleEndian, ReadBytesExt};
use dbsdk_rs::math::Vector3;

const BSP_MAGIC: u32 = 0x50534249;
const BSP_VERSION: u32 = 38;

fn read_vec3f<R: ReadBytesExt>(reader: &mut R) -> Vector3 {
    let x = reader.read_f32::<LittleEndian>().unwrap();
    let y = reader.read_f32::<LittleEndian>().unwrap();
    let z = reader.read_f32::<LittleEndian>().unwrap();

    Vector3::new(x, y, z)
}

fn read_vec3i<R: ReadBytesExt>(reader: &mut R) -> Vector3 {
    let x = reader.read_i32::<LittleEndian>().unwrap() as f32;
    let y = reader.read_i32::<LittleEndian>().unwrap() as f32;
    let z = reader.read_i32::<LittleEndian>().unwrap() as f32;

    Vector3::new(x, y, z)
}

pub struct Color32 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8
}

impl Color32 {
    pub fn read24<R: ReadBytesExt>(reader: &mut R) -> Color32 {
        let r = reader.read_u8().unwrap();
        let g = reader.read_u8().unwrap();
        let b = reader.read_u8().unwrap();

        Color32 {
            r,
            g,
            b,
            a: 255
        }
    }
}

pub struct BspLumpInfo {
    offset: u32,
    length: u32,
}

pub struct Edge {
    pub a: u16,
    pub b: u16
}

pub struct BspFace {
    pub plane: u16,
    pub plane_side: u16,
    pub first_edge: u32,
    pub num_edges: u16,
    pub texture_info: u16,
    pub lightmap_styles: [u8;4],
    pub lightmap_offset: u32,
}

pub struct Plane {
    pub normal: Vector3,
    pub distance: f32,
    pub plane_type: u32,
}

pub struct Node {
    pub plane: u32,
    pub front_child: i32,
    pub back_child: i32,
    pub bbox_min: Vector3,
    pub bbox_max: Vector3,
    pub first_face: u16,
    pub num_faces: u16,
}

pub struct Leaf {
    pub brush_or: u32,
    pub cluster: u16,
    pub area: u16,
    pub bbox_min: Vector3,
    pub bbox_max: Vector3,
    pub first_leaf_face: u16,
    pub num_leaf_faces: u16,
    pub first_leaf_brush: u16,
    pub num_leaf_brushes: u16
}

pub struct TexInfo {
    pub u_axis: Vector3,
    pub u_offset: f32,
    pub v_axis: Vector3,
    pub v_offset: f32,
    pub flags: u32,
    pub value: u32,
    pub texture_name: String,
    pub next_texinfo: u32,
}

pub struct VisCluster {
    pub vis_offset: usize
}

pub struct VertexLump {
    pub vertices: Vec<Vector3>
}

pub struct EdgeLump {
    pub edges: Vec<Edge>
}

pub struct FaceLump {
    pub faces: Vec<BspFace>
}

pub struct FaceEdgeLump {
    pub edges: Vec<u32>
}

pub struct PlaneLump {
    pub planes: Vec<Plane>
}

pub struct NodeLump {
    pub nodes: Vec<Node>
}

pub struct LeafLump {
    pub leaves: Vec<Leaf>
}

pub struct LeafFaceLump {
    pub faces: Vec<u16>
}

pub struct TexInfoLump {
    pub textures: Vec<TexInfo>
}

pub struct VisLump {
    pub clusters: Vec<VisCluster>,
    pub vis_buffer: Vec<u8>,
}

pub struct LightmapLump {
    pub lm: Vec<Color32>
}

impl VertexLump {
    pub fn new<R: Seek + ReadBytesExt>(reader: &mut R, info: &BspLumpInfo) -> VertexLump {
        reader.seek(std::io::SeekFrom::Start(info.offset as u64)).unwrap();

        let num_vertices = (info.length / 12) as usize;
        let mut vertices: Vec<Vector3> = Vec::with_capacity(num_vertices);

        for _ in 0..num_vertices {
            vertices.push(read_vec3f(reader));
        }

        VertexLump {
            vertices
        }
    }
}

impl EdgeLump {
    pub fn new<R: Seek + ReadBytesExt>(reader: &mut R, info: &BspLumpInfo) -> EdgeLump {
        reader.seek(std::io::SeekFrom::Start(info.offset as u64)).unwrap();

        let num_edges = (info.length / 4) as usize;
        let mut edges: Vec<Edge> = Vec::with_capacity(num_edges);

        for _ in 0..num_edges {
            let a = reader.read_u16::<LittleEndian>().unwrap();
            let b = reader.read_u16::<LittleEndian>().unwrap();
            edges.push(Edge {a, b});
        }

        EdgeLump {
            edges
        }
    }
}

impl FaceLump {
    pub fn new<R: Seek + ReadBytesExt>(reader: &mut R, info: &BspLumpInfo) -> FaceLump {
        reader.seek(std::io::SeekFrom::Start(info.offset as u64)).unwrap();

        let num_faces = (info.length / 20) as usize;
        let mut faces: Vec<BspFace> = Vec::with_capacity(num_faces);

        for _ in 0..num_faces {
            let plane = reader.read_u16::<LittleEndian>().unwrap();
            let plane_side = reader.read_u16::<LittleEndian>().unwrap();
            let first_edge = reader.read_u32::<LittleEndian>().unwrap();
            let num_edges = reader.read_u16::<LittleEndian>().unwrap();
            let texture_info = reader.read_u16::<LittleEndian>().unwrap();
            let lightmap_styles = [
                reader.read_u8().unwrap(),
                reader.read_u8().unwrap(),
                reader.read_u8().unwrap(),
                reader.read_u8().unwrap()
            ];
            let lightmap_offset = reader.read_u32::<LittleEndian>().unwrap();

            faces.push(BspFace {
                plane, plane_side, first_edge, num_edges, texture_info, lightmap_styles, lightmap_offset
            });
        }

        FaceLump {
            faces
        }
    }
}

impl FaceEdgeLump {
    pub fn new<R: Seek + ReadBytesExt>(reader: &mut R, info: &BspLumpInfo) -> FaceEdgeLump {
        reader.seek(std::io::SeekFrom::Start(info.offset as u64)).unwrap();

        let num_edges = (info.length / 4) as usize;
        let mut edges: Vec<u32> = Vec::with_capacity(num_edges);

        for _ in 0..num_edges {
            edges.push(reader.read_u32::<LittleEndian>().unwrap());
        }

        FaceEdgeLump {
            edges
        }
    }
}

impl PlaneLump {
    pub fn new<R: Seek + ReadBytesExt>(reader: &mut R, info: &BspLumpInfo) -> PlaneLump {
        reader.seek(std::io::SeekFrom::Start(info.offset as u64)).unwrap();

        let num_planes = (info.length / 20) as usize;
        let mut planes: Vec<Plane> = Vec::with_capacity(num_planes);

        for _ in 0..num_planes {
            let normal = read_vec3f(reader);
            let distance = reader.read_f32::<LittleEndian>().unwrap();
            let plane_type = reader.read_u32::<LittleEndian>().unwrap();
            planes.push(Plane { normal, distance, plane_type });
        }

        PlaneLump {
            planes
        }
    }
}

impl NodeLump {
    pub fn new<R: Seek + ReadBytesExt>(reader: &mut R, info: &BspLumpInfo) -> NodeLump {
        reader.seek(std::io::SeekFrom::Start(info.offset as u64)).unwrap();

        let num_nodes = (info.length / 28) as usize;
        let mut nodes: Vec<Node> = Vec::with_capacity(num_nodes);

        for _ in 0..num_nodes {
            let plane = reader.read_u32::<LittleEndian>().unwrap();
            let front_child = reader.read_i32::<LittleEndian>().unwrap();
            let back_child = reader.read_i32::<LittleEndian>().unwrap();
            let bbox_min = read_vec3i(reader);
            let bbox_max = read_vec3i(reader);
            let first_face = reader.read_u16::<LittleEndian>().unwrap();
            let num_faces = reader.read_u16::<LittleEndian>().unwrap();

            nodes.push(Node {
                plane,
                front_child,
                back_child,
                bbox_min,
                bbox_max,
                first_face,
                num_faces
            });
        }

        NodeLump {
            nodes
        }
    }
}

impl LeafLump {
    pub fn new<R: Seek + ReadBytesExt>(reader: &mut R, info: &BspLumpInfo) -> LeafLump {
        reader.seek(std::io::SeekFrom::Start(info.offset as u64)).unwrap();

        let num_leaves = (info.length / 28) as usize;
        let mut leaves: Vec<Leaf> = Vec::with_capacity(num_leaves);

        for _ in 0..num_leaves {
            let brush_or = reader.read_u32::<LittleEndian>().unwrap();
            let cluster = reader.read_u16::<LittleEndian>().unwrap();
            let area = reader.read_u16::<LittleEndian>().unwrap();
            let bbox_min = read_vec3i(reader);
            let bbox_max = read_vec3i(reader);
            let first_leaf_face = reader.read_u16::<LittleEndian>().unwrap();
            let num_leaf_faces = reader.read_u16::<LittleEndian>().unwrap();
            let first_leaf_brush = reader.read_u16::<LittleEndian>().unwrap();
            let num_leaf_brushes = reader.read_u16::<LittleEndian>().unwrap();

            leaves.push(Leaf {
                brush_or,
                cluster,
                area,
                bbox_min,
                bbox_max,
                first_leaf_face,
                num_leaf_faces,
                first_leaf_brush,
                num_leaf_brushes
            });
        }

        LeafLump {
            leaves
        }
    }
}

impl LeafFaceLump {
    pub fn new<R: Seek + ReadBytesExt>(reader: &mut R, info: &BspLumpInfo) -> LeafFaceLump {
        reader.seek(std::io::SeekFrom::Start(info.offset as u64)).unwrap();

        let num_faces = (info.length / 2) as usize;
        let mut faces: Vec<u16> = Vec::with_capacity(num_faces);

        for _ in 0..num_faces {
            let a = reader.read_u16::<LittleEndian>().unwrap();
            faces.push(a);
        }

        LeafFaceLump {
            faces
        }
    }
}

impl TexInfoLump {
    pub fn new<R: Seek + ReadBytesExt>(reader: &mut R, info: &BspLumpInfo) -> TexInfoLump {
        reader.seek(std::io::SeekFrom::Start(info.offset as u64)).unwrap();

        let num_textures = (info.length / 76) as usize;
        let mut textures: Vec<TexInfo> = Vec::with_capacity(num_textures);

        for _ in 0..num_textures {
            let u_axis = read_vec3f(reader);
            let u_offset = reader.read_f32::<LittleEndian>().unwrap();

            let v_axis = read_vec3f(reader);
            let v_offset = reader.read_f32::<LittleEndian>().unwrap();

            let flags = reader.read_u32::<LittleEndian>().unwrap();
            let value = reader.read_u32::<LittleEndian>().unwrap();

            let mut texture_name: [u8; 32] = [0; 32];
            reader.read_exact(&mut texture_name).unwrap();

            let mut name_len = 32;
            for i in 0..32 {
                if texture_name[i] == 0 {
                    name_len = i;
                    break;
                }
            }

            let texture_name = unsafe { std::str::from_utf8_unchecked(&texture_name[0..name_len]) }.to_owned();
            let next_texinfo = reader.read_u32::<LittleEndian>().unwrap();

            textures.push(TexInfo {
                u_axis,
                u_offset,
                v_axis,
                v_offset,
                flags,
                value,
                texture_name,
                next_texinfo
            });
        }

        TexInfoLump {
            textures
        }
    }
}

impl VisLump {
    pub fn new<R: Seek + ReadBytesExt>(reader: &mut R, info: &BspLumpInfo) -> VisLump {
        reader.seek(std::io::SeekFrom::Start(info.offset as u64)).unwrap();

        let num_clusters = reader.read_u32::<LittleEndian>().unwrap() as usize;
        let hdr_size = 4 + (num_clusters * 8);

        let mut clusters: Vec<VisCluster> = Vec::with_capacity(num_clusters);

        for _ in 0..num_clusters {
            let pvs = reader.read_u32::<LittleEndian>().unwrap();
            let _phs = reader.read_u32::<LittleEndian>().unwrap();

            let offs = (pvs as usize) - hdr_size;

            clusters.push(VisCluster {
                vis_offset: offs
            });
        }

        // read remainder of lump as byte array
        let buf_len = (info.length as usize) - hdr_size;
        let mut vis_buffer: Vec<u8> = vec![0;buf_len];
        reader.read_exact(&mut vis_buffer).unwrap();

        VisLump {
            clusters,
            vis_buffer
        }
    }

    // Unpack vis info for a given cluster index
    pub fn unpack_vis(self: &VisLump, cluster_index: usize, vis_info: &mut [bool]) {
        let mut v = self.clusters[cluster_index].vis_offset;
        let mut c = 0;

        while c < self.clusters.len() {
            if self.vis_buffer[v] == 0 {
                v += 1;
                c += 8 * (self.vis_buffer[v] as usize);
            }
            else {
                for bit in 0..8 {
                    let m = 1 << bit;
                    if (self.vis_buffer[v] & m) != 0 {
                        vis_info[c] = true;
                    }
                    c += 1;
                }
            }

            v += 1;
        }
    }
}

impl LightmapLump {
    pub fn new<R: Seek + ReadBytesExt>(reader: &mut R, info: &BspLumpInfo) -> LightmapLump {
        reader.seek(std::io::SeekFrom::Start(info.offset as u64)).unwrap();

        let num_px = (info.length / 3) as usize;
        let mut lm: Vec<Color32> = Vec::with_capacity(num_px);

        for _ in 0..num_px {
            lm.push(Color32::read24(reader));
        }

        LightmapLump {
            lm
        }
    }
}

pub struct BspFile {
    pub vertex_lump: VertexLump,
    pub edge_lump: EdgeLump,
    pub face_lump: FaceLump,
    pub face_edge_lump: FaceEdgeLump,
    pub plane_lump: PlaneLump,
    pub node_lump: NodeLump,
    pub leaf_lump: LeafLump,
    pub leaf_face_lump: LeafFaceLump,
    pub tex_info_lump: TexInfoLump,
    pub vis_lump: VisLump,
    pub lm_lump: LightmapLump,
}

impl BspFile {
    pub fn new<R: Seek + ReadBytesExt>(reader: &mut R) -> BspFile {
        let magic = reader.read_u32::<LittleEndian>().unwrap();
        if magic != BSP_MAGIC {
            panic!("Failed loading BSP: input is not valid IBSP data");
        }

        let version = reader.read_u32::<LittleEndian>().unwrap();
        if version != BSP_VERSION {
            panic!("Failed loading BSP: wrong IBSP file version");
        }

        // read BSP lump info
        let mut bsp_lumps: Vec<BspLumpInfo> = Vec::with_capacity(19);

        for _ in 0..19 {
            let offset = reader.read_u32::<LittleEndian>().unwrap();
            let length = reader.read_u32::<LittleEndian>().unwrap();

            bsp_lumps.push(BspLumpInfo { offset, length });
        }

        // read lumps
        let plane_lump = PlaneLump::new(reader, &bsp_lumps[1]);
        let vertex_lump = VertexLump::new(reader, &bsp_lumps[2]);
        let vis_lump = VisLump::new(reader, &bsp_lumps[3]);
        let node_lump = NodeLump::new(reader, &bsp_lumps[4]);
        let tex_info_lump = TexInfoLump::new(reader, &bsp_lumps[5]);
        let face_lump = FaceLump::new(reader, &bsp_lumps[6]);
        let lm_lump = LightmapLump::new(reader, &bsp_lumps[7]);
        let leaf_lump = LeafLump::new(reader, &bsp_lumps[8]);
        let leaf_face_lump = LeafFaceLump::new(reader, &bsp_lumps[9]);
        let edge_lump = EdgeLump::new(reader, &bsp_lumps[11]);
        let face_edge_lump = FaceEdgeLump::new(reader, &bsp_lumps[12]);

        BspFile {
            vertex_lump,
            edge_lump,
            face_lump,
            face_edge_lump,
            plane_lump,
            node_lump,
            leaf_lump,
            leaf_face_lump,
            tex_info_lump,
            vis_lump,
            lm_lump
        }
    }
}