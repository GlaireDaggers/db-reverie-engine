use std::{io::{Read, Seek, ErrorKind}, ffi::CStr, str::FromStr, sync::Arc};

use byteorder::{ReadBytesExt, LittleEndian};
use dbsdk_rs::{io::IOError, math::{Matrix4x4, Quaternion, Vector3, Vector4}, vdp::Texture};
use half::f16;

use crate::asset_loader::ResourceError;

const DBM_VER: u32 = 1;

/// Represents a skeleton loaded from DBM mesh file
pub struct DBSkeleton {
    pub bone_count: u32,
    pub nodes: Vec<DBSkelNode>
}

/// Represents a single node in a skeleton
pub struct DBSkelNode {
    pub bone_index: u8,
    pub inv_bind_pose: Matrix4x4,
    pub local_rest_pose: Matrix4x4,
    pub children: Vec<DBSkelNode>,
}

/// Represents a vertex loaded from DBM mesh file
#[derive(Clone, Copy)]
pub struct DBMeshVertex {
    pub pos: [f16;3],
    pub nrm: [f16;3],
    pub col: [u8;4],
    pub tex: [f16;2],
    pub bweight: [u8;2],
    pub bidx: [u8;2],
}

/// Represents a material loaded from DBM mesh file
pub struct DBMaterialInfo {
    pub name: String,
    pub texture: Option<Arc<Texture>>,
    pub blend_enable: bool,
    pub enable_cull: bool,
    pub diffuse_color: Vector4,
    pub spec_color: Vector3,
    pub roughness: f32,
}

/// Represents a mesh part loaded from DBM mesh file
pub struct DBMeshPart {
    pub name: String,
    pub transform: Matrix4x4,
    pub material: DBMaterialInfo,
    pub vertices: Vec<DBMeshVertex>,
}

/// A mesh loaded from DBM mesh file
pub struct DBMesh {
    pub mesh_parts: Vec<DBMeshPart>,
    pub skeleton: Option<DBSkeleton>,
}

/// Enumeration of errors which can result from parsing a DBM mesh file
#[derive(Debug)]
pub enum DBMeshError {
    ParseError,
    VersionError,
    IOError(IOError)
}

fn str_from_null_terminated_utf8_safe(s: &[u8]) -> &str {
    if s.iter().any(|&x| x == 0) {
        unsafe { str_from_null_terminated_utf8(s) }
    } else {
        std::str::from_utf8(s).unwrap()
    }
}

// unsafe: s must contain a null byte
unsafe fn str_from_null_terminated_utf8(s: &[u8]) -> &str {
    CStr::from_ptr(s.as_ptr() as *const _).to_str().unwrap()
}

fn read_skel_node<R>(reader: &mut R) -> Result<Option<DBSkelNode>,DBMeshError> where R : Read {
    // read inverse bind mat
    let mut inv_bind_mat = Matrix4x4::identity();

    for j in 0..4 {
        for i in 0..4 {
            inv_bind_mat.m[i][j] = match reader.read_f32::<LittleEndian>() {
                Ok(v) => { v },
                Err(e) => {
                    if e.kind() == ErrorKind::UnexpectedEof {
                        return Ok(None);
                    }
                    return Err(DBMeshError::ParseError)
                }
            };
        }
    }

    // read local rest mat
    let mut local_rest_mat = Matrix4x4::identity();

    for j in 0..4 {
        for i in 0..4 {
            local_rest_mat.m[i][j] = match reader.read_f32::<LittleEndian>() {
                Ok(v) => { v },
                Err(e) => {
                    if e.kind() == ErrorKind::UnexpectedEof {
                        return Ok(None);
                    }
                    return Err(DBMeshError::ParseError)
                }
            };
        }
    }

    // read bone index
    let bone_index = match reader.read_u8() {
        Ok(v) => { v },
        Err(_) => {
            return Err(DBMeshError::ParseError);
        }
    };

    // read child count
    let child_count = match reader.read_u8() {
        Ok(v) => { v },
        Err(_) => {
            return Err(DBMeshError::ParseError);
        }
    } as usize;

    let mut children: Vec<DBSkelNode> = Vec::new();

    for _ in 0..child_count {
        match read_skel_node(reader)? {
            Some(v) => {
                children.push(v);
            }
            None => {
                return Err(DBMeshError::ParseError);
            }
        };
    }

    return Ok(Some(DBSkelNode { bone_index: bone_index, inv_bind_pose: inv_bind_mat, local_rest_pose: local_rest_mat, children: children }));
}

impl DBMesh {
    pub fn new<R,TL>(reader: &mut R, tex_load_fn: TL) -> Result<DBMesh, DBMeshError>
        where R : Read + Seek,
        TL : Fn(&str) -> Result<Arc<Texture>, ResourceError>
    {
        // read header
        let mut id: [u8;4] = [0;4];
        match reader.read_exact(&mut id) {
            Ok(_) => {
            },
            Err(_) => {
                return Err(DBMeshError::ParseError);
            }
        };

        match std::str::from_utf8(&id) {
            Ok("DBM\0") => {
            },
            _ => {
                return Err(DBMeshError::ParseError);
            }
        }

        let ver = match reader.read_u32::<LittleEndian>() {
            Ok(v) => { v },
            Err(_) => {
                return Err(DBMeshError::ParseError);
            }
        };

        if ver != DBM_VER {
            return Err(DBMeshError::VersionError);
        }

        let mut mesh = DBMesh {
            mesh_parts: Vec::new(),
            skeleton: None,
        };

        // scan chunks
        loop {
            let mut chunk_id: [u8;4] = [0;4];
            match reader.read_exact(&mut chunk_id) {
                Ok(_) => {
                },
                Err(e) => {
                    // EOF, no more chunks in stream
                    if e.kind() == ErrorKind::UnexpectedEof {
                        break;
                    }
                    return Err(DBMeshError::ParseError);
                }
            };

            let chunk_size = match reader.read_u32::<LittleEndian>() {
                Ok(v) => { v },
                Err(_) => {
                    return Err(DBMeshError::ParseError);
                }
            };

            match std::str::from_utf8(&chunk_id) {
                Ok("SKEL") => {
                    let mut skeleton = DBSkeleton {
                        bone_count: 0,
                        nodes: Vec::new()
                    };

                    let mut chunk_data: Vec<u8> = vec![0;chunk_size as usize];
                    match reader.read_exact(&mut chunk_data) {
                        Ok(_) => {},
                        Err(_) => { return Err(DBMeshError::ParseError); }
                    };

                    skeleton.bone_count = chunk_size / 130;

                    // read skeleton from chunk & assign to mesh
                    let mut reader = chunk_data.as_slice();
                    loop {
                        match read_skel_node(&mut reader)? {
                            Some(node) => {
                                skeleton.nodes.push(node);
                            },
                            None => {
                                break;
                            }
                        };
                    }

                    mesh.skeleton = Some(skeleton);
                },
                Ok("MESH") => {
                    // append a new mesh part from chunk
                    let mut mesh_name: [u8;32] = [0;32];
                    match reader.read_exact(&mut mesh_name) {
                        Ok(_) => {
                        },
                        Err(_) => {
                            return Err(DBMeshError::ParseError);
                        }
                    };

                    // translation + rotation + scale
                    let tx = match reader.read_f32::<LittleEndian>() {
                        Ok(v) => { v },
                        Err(_) => {
                            return Err(DBMeshError::ParseError);
                        }
                    };
                    let ty = match reader.read_f32::<LittleEndian>() {
                        Ok(v) => { v },
                        Err(_) => {
                            return Err(DBMeshError::ParseError);
                        }
                    };
                    let tz = match reader.read_f32::<LittleEndian>() {
                        Ok(v) => { v },
                        Err(_) => {
                            return Err(DBMeshError::ParseError);
                        }
                    };

                    let rx = match reader.read_f32::<LittleEndian>() {
                        Ok(v) => { v },
                        Err(_) => {
                            return Err(DBMeshError::ParseError);
                        }
                    };
                    let ry = match reader.read_f32::<LittleEndian>() {
                        Ok(v) => { v },
                        Err(_) => {
                            return Err(DBMeshError::ParseError);
                        }
                    };
                    let rz = match reader.read_f32::<LittleEndian>() {
                        Ok(v) => { v },
                        Err(_) => {
                            return Err(DBMeshError::ParseError);
                        }
                    };
                    let rw = match reader.read_f32::<LittleEndian>() {
                        Ok(v) => { v },
                        Err(_) => {
                            return Err(DBMeshError::ParseError);
                        }
                    };
                    
                    let sx = match reader.read_f32::<LittleEndian>() {
                        Ok(v) => { v },
                        Err(_) => {
                            return Err(DBMeshError::ParseError);
                        }
                    };
                    let sy = match reader.read_f32::<LittleEndian>() {
                        Ok(v) => { v },
                        Err(_) => {
                            return Err(DBMeshError::ParseError);
                        }
                    };
                    let sz = match reader.read_f32::<LittleEndian>() {
                        Ok(v) => { v },
                        Err(_) => {
                            return Err(DBMeshError::ParseError);
                        }
                    };

                    let translate = Matrix4x4::translation(Vector3::new(tx, ty, tz));
                    let rotate = Matrix4x4::rotation(Quaternion::new(rx, ry, rz, rw));
                    let scale = Matrix4x4::scale(Vector3::new(sx, sy, sz));

                    let transform = scale * rotate * translate;

                    // material info
                    let mut mat_name: [u8;32] = [0;32];
                    match reader.read_exact(&mut mat_name) {
                        Ok(_) => {
                        },
                        Err(_) => {
                            return Err(DBMeshError::ParseError);
                        }
                    };

                    let mat_has_texture = match reader.read_u8() {
                        Ok(v) => { v != 0 }
                        Err(_) => {
                            return Err(DBMeshError::ParseError);
                        }
                    };

                    let mat_blend_enable = match reader.read_u8() {
                        Ok(v) => { v != 0 }
                        Err(_) => {
                            return Err(DBMeshError::ParseError);
                        }
                    };

                    let mat_enable_culling = match reader.read_u8() {
                        Ok(v) => { v != 0 }
                        Err(_) => {
                            return Err(DBMeshError::ParseError);
                        }
                    };

                    let mut diffuse_color: [u8;4] = [0;4];
                    match reader.read_exact(&mut diffuse_color) {
                        Ok(_) => {
                        },
                        Err(_) => {
                            return Err(DBMeshError::ParseError);
                        }
                    };

                    let mut spec_color: [u8;3] = [0;3];
                    match reader.read_exact(&mut spec_color) {
                        Ok(_) => {
                        },
                        Err(_) => {
                            return Err(DBMeshError::ParseError);
                        }
                    };

                    let roughness = match reader.read_u8() {
                        Ok(v) => { v }
                        Err(_) => {
                            return Err(DBMeshError::ParseError);
                        }
                    };

                    let mat_name = String::from_str(str_from_null_terminated_utf8_safe(&mat_name)).unwrap();

                    let texture: Option<Arc<Texture>> = if mat_has_texture {
                        // load texture
                        match tex_load_fn(mat_name.as_str()) {
                            Ok(v) => {
                                Some(v)
                            },
                            Err(e) => {
                                match e {
                                    ResourceError::IOError(io_err) => {
                                        return Err(DBMeshError::IOError(io_err));
                                    }
                                    _ => {
                                        return Err(DBMeshError::ParseError)
                                    }
                                }
                            }
                        }
                    } else {
                        None
                    };

                    let mat_info = DBMaterialInfo {
                        name: mat_name,
                        texture: texture,
                        blend_enable: mat_blend_enable,
                        enable_cull: mat_enable_culling,
                        diffuse_color: Vector4::new((diffuse_color[0] as f32) / 255.0, (diffuse_color[1] as f32) / 255.0, (diffuse_color[2] as f32) / 255.0, (diffuse_color[3] as f32) / 255.0),
                        spec_color: Vector3::new((spec_color[0] as f32) / 255.0, (spec_color[1] as f32) / 255.0, (spec_color[2] as f32) / 255.0),
                        roughness: (roughness as f32) / 255.0,
                    };

                    let mut mesh_vertices: Vec<DBMeshVertex> = Vec::new();

                    let tri_count = match reader.read_u16::<LittleEndian>() {
                        Ok(v) => { v },
                        Err(_) => {
                            return Err(DBMeshError::ParseError);
                        }
                    };

                    let vtx_count = (tri_count as usize) * 3;

                    for _ in 0..vtx_count {
                        let px = match reader.read_u16::<LittleEndian>() {
                            Ok(v) => { f16::from_bits(v) },
                            Err(_) => {
                                return Err(DBMeshError::ParseError);
                            }
                        };
                        let py = match reader.read_u16::<LittleEndian>() {
                            Ok(v) => { f16::from_bits(v) },
                            Err(_) => {
                                return Err(DBMeshError::ParseError);
                            }
                        };
                        let pz = match reader.read_u16::<LittleEndian>() {
                            Ok(v) => { f16::from_bits(v) },
                            Err(_) => {
                                return Err(DBMeshError::ParseError);
                            }
                        };
                        let nx = match reader.read_u16::<LittleEndian>() {
                            Ok(v) => { f16::from_bits(v) },
                            Err(_) => {
                                return Err(DBMeshError::ParseError);
                            }
                        };
                        let ny = match reader.read_u16::<LittleEndian>() {
                            Ok(v) => { f16::from_bits(v) },
                            Err(_) => {
                                return Err(DBMeshError::ParseError);
                            }
                        };
                        let nz = match reader.read_u16::<LittleEndian>() {
                            Ok(v) => { f16::from_bits(v) },
                            Err(_) => {
                                return Err(DBMeshError::ParseError);
                            }
                        };
                        let mut vcol: [u8;4] = [0;4];
                        match reader.read_exact(&mut vcol) {
                            Ok(_) => {},
                            Err(_) => {
                                return Err(DBMeshError::ParseError);
                            }
                        };
                        let tx = match reader.read_u16::<LittleEndian>() {
                            Ok(v) => { f16::from_bits(v) },
                            Err(_) => {
                                return Err(DBMeshError::ParseError);
                            }
                        };
                        let ty = match reader.read_u16::<LittleEndian>() {
                            Ok(v) => { f16::from_bits(v) },
                            Err(_) => {
                                return Err(DBMeshError::ParseError);
                            }
                        };
                        let mut bw: [u8;2] = [0;2];
                        match reader.read_exact(&mut bw) {
                            Ok(_) => {},
                            Err(_) => {
                                return Err(DBMeshError::ParseError);
                            }
                        };
                        let mut bi: [u8;2] = [0;2];
                        match reader.read_exact(&mut bi) {
                            Ok(_) => {},
                            Err(_) => {
                                return Err(DBMeshError::ParseError);
                            }
                        };

                        mesh_vertices.push(DBMeshVertex {
                            pos: [px, py, pz],
                            nrm: [nx, ny, nz],
                            tex: [tx, ty],
                            col: vcol,
                            bweight: bw,
                            bidx: bi
                        });
                    }

                    let mesh_part = DBMeshPart {
                        name: String::from_str(str_from_null_terminated_utf8_safe(&mesh_name)).unwrap(),
                        transform: transform,
                        material: mat_info,
                        vertices: mesh_vertices
                    };

                    mesh.mesh_parts.push(mesh_part);
                },
                _ => {
                    // unknown chunk ID, skip
                    match reader.seek(std::io::SeekFrom::Current(chunk_size as i64)) {
                        Ok(_) => {
                        },
                        Err(_) => {
                            return Err(DBMeshError::ParseError);
                        }
                    }
                }
            };
        }

        return Ok(mesh);
    }
}