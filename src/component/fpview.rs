pub struct FPView {
    pub yaw: f32,
    pub pitch: f32,
}

impl FPView {
    pub fn default() -> FPView {
        FPView {
            yaw: 0.0,
            pitch: 0.0
        }
    }
}