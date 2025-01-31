use dbsdk_rs::math::Vector3;

pub struct ColliderBounds {
    pub bounds_offset: Vector3,
    pub bounds_extents: Vector3,
}