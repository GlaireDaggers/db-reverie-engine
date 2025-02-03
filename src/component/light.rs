use dbsdk_rs::math::Vector3;

#[derive(Clone, Copy)]
pub struct Light {
    pub color: Vector3,
    pub max_radius: f32,
}