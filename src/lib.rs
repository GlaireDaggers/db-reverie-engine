extern crate dbsdk_rs;
extern crate byteorder;

use bsp_file::BspFile;
use map::BspMap;
use dbsdk_rs::{vdp::{self, Color32}, db, io::{FileStream, FileMode}};

mod bsp_file;
mod map;

fn tick() {
    vdp::clear_color(Color32::new(128, 128, 255, 255));
}

#[no_mangle]
pub fn main(_: i32, _: i32) -> i32 {
    db::register_panic();
    db::log(format!("Hello, DreamBox!").as_str());
    let mut bsp_file = FileStream::open("/cd/content/maps/demo1.bsp", FileMode::Read).unwrap();
    let bsp = BspFile::new(&mut bsp_file);
    let _bsp_map = BspMap::new(bsp);
    db::log("BSP loaded");
    vdp::set_vsync_handler(Some(tick));
    return 0;
}