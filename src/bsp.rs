use std::io::Seek;

use byteorder::{LittleEndian, ReadBytesExt};

const BSP_MAGIC: u32 = 0x50534249;
const BSP_VERSION: u32 = 38;

pub struct Vec3f {
    x: f32,
    y: f32,
    z: f32
}

impl Vec3f {
    pub fn read<R: ReadBytesExt>(reader: &mut R) -> Vec3f {
        let x = reader.read_f32::<LittleEndian>().unwrap();
        let y = reader.read_f32::<LittleEndian>().unwrap();
        let z = reader.read_f32::<LittleEndian>().unwrap();

        Vec3f {
            x, y, z
        }
    }
}

pub struct Vec3i {
    x: i32,
    y: i32,
    z: i32
}

impl Vec3i {
    pub fn read<R: ReadBytesExt>(reader: &mut R) -> Vec3i {
        let x = reader.read_i32::<LittleEndian>().unwrap();
        let y = reader.read_i32::<LittleEndian>().unwrap();
        let z = reader.read_i32::<LittleEndian>().unwrap();

        Vec3i {
            x, y, z
        }
    }
}

struct BspLumpInfo {
    offset: u32,
    length: u32,
}

struct Edge {
    a: u16,
    b: u16
}

struct BspFace {
    plane: u16,
    plane_side: u16,
    first_edge: u32,
    num_edges: u16,
    texture_info: u16,
    lightmap_styles: [u8;4],
    lightmap_offset: u32,
}

struct Plane {
    normal: Vec3f,
    distance: f32,
    plane_type: u32,
}

struct Node {
    plane: u32,
    front_child: i32,
    back_child: i32,
    bbox_min: Vec3i,
    bbox_max: Vec3i,
    first_face: u16,
    num_faces: u16,
}

struct Leaf {
    brush_or: u32,
    cluster: u16,
    area: u16,
    bbox_min: Vec3i,
    bbox_max: Vec3i,
    first_leaf_face: u16,
    num_leaf_faces: u16,
    first_leaf_brush: u16,
    num_leaf_brushes: u16
}

struct TexInfo {
    u_axis: Vec3f,
    u_offset: f32,
    v_axis: Vec3f,
    v_offset: f32,
    flags: u32,
    value: u32,
    texture_name: [u8; 32],
    next_texinfo: u32,
}

struct VertexLump {
    vertices: Vec<Vec3f>
}

struct EdgeLump {
    edges: Vec<Edge>
}

struct FaceLump {
    faces: Vec<BspFace>
}

struct FaceEdgeLump {
    edges: Vec<u32>
}

struct PlaneLump {
    planes: Vec<Plane>
}

struct NodeLump {
    nodes: Vec<Node>
}

struct LeafLump {
    leaves: Vec<Leaf>
}

struct LeafFaceLump {
    faces: Vec<u16>
}

struct TexInfoLump {
    textures: Vec<TexInfo>
}

impl VertexLump {
    pub fn new<R: Seek + ReadBytesExt>(reader: &mut R, info: &BspLumpInfo) -> VertexLump {
        reader.seek(std::io::SeekFrom::Start(info.offset as u64)).unwrap();

        let num_vertices = (info.length / 12) as usize;
        let mut vertices: Vec<Vec3f> = Vec::with_capacity(num_vertices);

        for _ in 0..num_vertices {
            vertices.push(Vec3f::read(reader));
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
            let normal = Vec3f::read(reader);
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
            let bbox_min = Vec3i::read(reader);
            let bbox_max = Vec3i::read(reader);
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
            let bbox_min = Vec3i::read(reader);
            let bbox_max = Vec3i::read(reader);
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

        let num_textures = (info.length / 2) as usize;
        let mut textures: Vec<TexInfo> = Vec::with_capacity(num_textures);

        for _ in 0..num_textures {
            let u_axis = Vec3f::read(reader);
            let u_offset = reader.read_f32::<LittleEndian>().unwrap();

            let v_axis = Vec3f::read(reader);
            let v_offset = reader.read_f32::<LittleEndian>().unwrap();

            let flags = reader.read_u32::<LittleEndian>().unwrap();
            let value = reader.read_u32::<LittleEndian>().unwrap();

            let mut texture_name: [u8; 32] = [0; 32];
            reader.read_exact(&mut texture_name).unwrap();

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

pub struct BspFile {
    vertex_lump: VertexLump,
    edge_lump: EdgeLump,
    face_lump: FaceLump,
    face_edge_lump: FaceEdgeLump,
    plane_lump: PlaneLump,
    node_lump: NodeLump,
    leaf_lump: LeafLump,
    leaf_face_lump: LeafFaceLump,
    tex_info_lump: TexInfoLump
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
        let node_lump = NodeLump::new(reader, &bsp_lumps[4]);
        let tex_info_lump = TexInfoLump::new(reader, &bsp_lumps[5]);
        let face_lump = FaceLump::new(reader, &bsp_lumps[6]);
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
            tex_info_lump
        }
    }
}