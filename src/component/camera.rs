use dbsdk_rs::vdp::Rectangle;

pub struct Camera {
    pub fov: f32,
    pub near: f32,
    pub far: f32,
    pub viewport_rect: Option<Rectangle>
}

impl Camera {
    pub fn default() -> Camera {
        Camera {
            fov: 60.0,
            near: 10.0,
            far: 10000.0,
            viewport_rect: None
        }
    }
}