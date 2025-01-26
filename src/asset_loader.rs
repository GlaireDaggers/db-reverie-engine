use dbsdk_rs::{io::{self, IOError}, vdp};
use ktx::KtxInfo;

const GL_RGB: u32 = 0x1907;
const GL_RGBA: u32 = 0x1908;
const GL_UNSIGNED_BYTE: u32 = 0x1401;
const GL_UNSIGNED_SHORT_5_6_5: u32 = 0x8363;
const GL_UNSIGNED_SHORT_4_4_4_4: u32 = 0x8033;
const GL_COMPRESSED_RGB_S3TC_DXT1_EXT: u32 = 0x83F0;
const GL_COMPRESSED_RGBA_S3TC_DXT1_EXT: u32 = 0x83F1;
const GL_COMPRESSED_RGBA_S3TC_DXT3_EXT: u32 = 0x83F2;

pub fn load_texture(path: &str) -> Result<vdp::Texture, IOError> {
    let tex_file = match io::FileStream::open(path, io::FileMode::Read) {
        Ok(v) => v,
        Err(e) => return Err(e)
    };

    // decode KTX texture
    let decoder = ktx::Decoder::new(tex_file).expect("Failed decoding KTX image");

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
        panic!("Failed decoding KTX image: format is unsupported");
    };

    // allocate VDP texture
    let tex = vdp::Texture::new(
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

pub fn load_env(env_name: &str) -> [vdp::Texture;6] {
    let env_ft = load_texture(format!("/cd/content/env/{}1ft.ktx", env_name).as_str()).unwrap();
    let env_bk = load_texture(format!("/cd/content/env/{}1bk.ktx", env_name).as_str()).unwrap();
    let env_lf = load_texture(format!("/cd/content/env/{}1lf.ktx", env_name).as_str()).unwrap();
    let env_rt = load_texture(format!("/cd/content/env/{}1rt.ktx", env_name).as_str()).unwrap();
    let env_up = load_texture(format!("/cd/content/env/{}1up.ktx", env_name).as_str()).unwrap();
    let env_dn = load_texture(format!("/cd/content/env/{}1dn.ktx", env_name).as_str()).unwrap();

    [env_ft, env_bk, env_lf, env_rt, env_up, env_dn]
}