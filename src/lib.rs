extern crate dbsdk_rs;
extern crate byteorder;

use dbsdk_rs::{vdp::{self, Color32}, db};

mod bsp;

fn tick() {
    vdp::clear_color(Color32::new(128, 128, 255, 255));
}

#[no_mangle]
pub fn main(_: i32, _: i32) -> i32 {
    db::register_panic();
    db::log(format!("Hello, DreamBox!").as_str());
    vdp::set_vsync_handler(Some(tick));
    return 0;
}