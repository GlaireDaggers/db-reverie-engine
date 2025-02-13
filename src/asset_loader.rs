use std::{collections::HashMap, marker::PhantomData, path::Path, sync::{Arc, RwLock, Weak}};

use dbsdk_rs::{db::log, io::{self, IOError}, logfmt, vdp::{self, Texture}};
use ktx::KtxInfo;
use lazy_static::lazy_static;

use crate::{dbanim::DBAnimationClip, dbmesh::DBMesh};

const GL_RGB: u32 = 0x1907;
const GL_RGBA: u32 = 0x1908;
const GL_UNSIGNED_BYTE: u32 = 0x1401;
const GL_UNSIGNED_SHORT_5_6_5: u32 = 0x8363;
const GL_UNSIGNED_SHORT_4_4_4_4: u32 = 0x8033;
const GL_COMPRESSED_RGB_S3TC_DXT1_EXT: u32 = 0x83F0;
const GL_COMPRESSED_RGBA_S3TC_DXT1_EXT: u32 = 0x83F1;
const GL_COMPRESSED_RGBA_S3TC_DXT3_EXT: u32 = 0x83F2;

lazy_static! {
    static ref TEXTURE_CACHE: RwLock<TextureCache> = RwLock::new(TextureCache::new());
    static ref MESH_CACHE: RwLock<MeshCache> = RwLock::new(MeshCache::new());
    static ref MESH_ANIM_CACHE: RwLock<MeshAnimCache> = RwLock::new(MeshAnimCache::new());
}

pub fn load_texture(path: &str) -> Result<Arc<Texture>, ResourceError> {
    let tex_cache = &mut TEXTURE_CACHE.write().unwrap();
    return tex_cache.load(path);
}

pub fn load_mesh(path: &str) -> Result<Arc<DBMesh>, ResourceError> {
    let mesh_cache = &mut MESH_CACHE.write().unwrap();
    return mesh_cache.load(path);
}

pub fn load_mesh_anim(path: &str) -> Result<Arc<DBAnimationClip>, ResourceError> {
    let anim_cache = &mut MESH_ANIM_CACHE.write().unwrap();
    return anim_cache.load(path);
}

pub fn load_env(env_name: &str) -> [Arc<Texture>;6] {
    let env_ft = load_texture(format!("/cd/content/env/{}1ft.ktx", env_name).as_str()).unwrap();
    let env_bk = load_texture(format!("/cd/content/env/{}1bk.ktx", env_name).as_str()).unwrap();
    let env_lf = load_texture(format!("/cd/content/env/{}1lf.ktx", env_name).as_str()).unwrap();
    let env_rt = load_texture(format!("/cd/content/env/{}1rt.ktx", env_name).as_str()).unwrap();
    let env_up = load_texture(format!("/cd/content/env/{}1up.ktx", env_name).as_str()).unwrap();
    let env_dn = load_texture(format!("/cd/content/env/{}1dn.ktx", env_name).as_str()).unwrap();

    [env_ft, env_bk, env_lf, env_rt, env_up, env_dn]
}

#[derive(Debug)]
pub enum ResourceError {
    ParseError,
    IOError(IOError)
}

pub trait ResourceLoader<TResource> {
    fn load_resource(path: &str) -> Result<TResource, ResourceError>;
}

pub struct TextureLoader {
}

impl ResourceLoader<Texture> for TextureLoader {
    fn load_resource(path: &str) -> Result<Texture, ResourceError> {    
        let tex_file = match io::FileStream::open(path, io::FileMode::Read) {
            Ok(v) => v,
            Err(e) => return Err(ResourceError::IOError(e))
        };

        // decode KTX texture
        let decoder = match ktx::Decoder::new(tex_file) {
            Ok(v) => v,
            Err(_) => return Err(ResourceError::ParseError)
        };

        // find appropriate VDP format
        let tex_fmt = if decoder.gl_type() == GL_UNSIGNED_BYTE && decoder.gl_format() == GL_RGBA {
            vdp::TextureFormat::RGBA8888
        } else if decoder.gl_type() == GL_UNSIGNED_SHORT_5_6_5 && decoder.gl_format() == GL_RGB {
            vdp::TextureFormat::RGB565
        } else if decoder.gl_type() == GL_UNSIGNED_SHORT_4_4_4_4 && decoder.gl_format() == GL_RGBA {
            vdp::TextureFormat::RGBA4444
        } else if decoder.gl_internal_format() == GL_COMPRESSED_RGB_S3TC_DXT1_EXT || decoder.gl_internal_format() == GL_COMPRESSED_RGBA_S3TC_DXT1_EXT {
            vdp::TextureFormat::DXT1
        } else if decoder.gl_internal_format() == GL_COMPRESSED_RGBA_S3TC_DXT3_EXT {
            vdp::TextureFormat::DXT3
        } else {
            logfmt!("Failed decoding KTX image: unsupported pixel format");
            return Err(ResourceError::ParseError);
        };

        // allocate VDP texture
        let tex = Texture::new(
            decoder.pixel_width() as i32,
            decoder.pixel_height() as i32,
            decoder.mipmap_levels() > 1, tex_fmt)
            .expect("Failed allocating VDP texture");

        // upload each mip slice
        let mut level: i32 = 0;
        for tex_level in decoder.read_textures() {
            tex.set_texture_data(level, &tex_level);
            level += 1;
        }

        Ok(tex)
    }
}

pub struct MeshLoader {
}

impl ResourceLoader<DBMesh> for MeshLoader {
    fn load_resource(path: &str) -> Result<DBMesh, ResourceError> {
        let mut mesh_file = match io::FileStream::open(path, io::FileMode::Read) {
            Ok(v) => v,
            Err(e) => return Err(ResourceError::IOError(e))
        };

        let rootpath = Path::new(path).parent().unwrap().to_str().unwrap();

        let tex_loader = |name: &str| {
            let tex_path = format!("{}/{}.ktx", rootpath, name);
            let tex_cache = &mut TEXTURE_CACHE.write().unwrap();
            tex_cache.load(&tex_path)
        };

        match DBMesh::new(&mut mesh_file, tex_loader) {
            Ok(v) => Ok(v),
            Err(e) => {
                match e {
                    crate::dbmesh::DBMeshError::IOError(io_err) => {
                        Err(ResourceError::IOError(io_err))
                    }
                    _ => {
                        Err(ResourceError::ParseError)
                    }
                }
            }
        }
    }
}

pub struct MeshAnimLoader {
}

impl ResourceLoader<DBAnimationClip> for MeshAnimLoader {
    fn load_resource(path: &str) -> Result<DBAnimationClip, ResourceError> {
        let mut anim_file = match io::FileStream::open(path, io::FileMode::Read) {
            Ok(v) => v,
            Err(e) => return Err(ResourceError::IOError(e))
        };

        match DBAnimationClip::new(&mut anim_file) {
            Ok(v) => Ok(v),
            Err(_) => Err(ResourceError::ParseError)
        }
    }
}

/// Implementation of a smart cache with ref counted resources
/// Attempts to load the same resource path more than once will return a reference to the same resource
/// If all references to the resource are dropped, the resource will be unloaded
pub struct ResourceCache<TResource, TResourceLoader>
    where TResourceLoader: ResourceLoader<TResource>
{
    cache: HashMap<String, Weak<TResource>>,
    phantom: PhantomData<TResourceLoader>
}

impl<TResource, TResourceLoader> ResourceCache<TResource, TResourceLoader> 
    where TResourceLoader: ResourceLoader<TResource>
{
    pub fn new() -> ResourceCache<TResource, TResourceLoader> {
        ResourceCache::<TResource, TResourceLoader> {
            cache: HashMap::new(),
            phantom: PhantomData::default()
        }
    }

    pub fn load(self: &mut Self, path: &str) -> Result<Arc<TResource>, ResourceError> {
        if self.cache.contains_key(path) {
            // try and get a reference to the resource, upgraded to a new Rc
            // if that fails, the resource has been unloaded (we'll just load a new one)
            let res = self.cache[path].clone().upgrade();
            match res {
                Some(v) => {
                    return Ok(v);
                }
                None => {
                    self.cache.remove(path);
                }
            };
        }

        logfmt!("Loading {}: {}", std::any::type_name::<TResource>(), path);

        let tex = match TResourceLoader::load_resource(path) {
            Ok(v) => v,
            Err(e) => {
                logfmt!("\t FAILED: {:?}", e);
                return Err(e);
            }
        };

        let res = Arc::new(tex);
        let store = Arc::downgrade(&res.clone());

        self.cache.insert(path.to_owned(), store);
        return Ok(res);
    }
}

pub type TextureCache = ResourceCache<Texture, TextureLoader>;
pub type MeshCache = ResourceCache<DBMesh, MeshLoader>;
pub type MeshAnimCache = ResourceCache<DBAnimationClip, MeshAnimLoader>;