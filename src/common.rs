use dbsdk_rs::math::{Matrix4x4, Vector3};

pub fn coord_space_transform() -> Matrix4x4 {
    // Quake coordinate system:
    // +X is right
    // +Y is forwards
    // +Z is up

    // DreamBox coordinate system:
    // +X is right
    // +Y is up
    // -Z is forwards

    Matrix4x4 {m: [
        [ 1.0,  0.0,  0.0, 0.0],
        [ 0.0,  0.0, -1.0, 0.0],
        [ 0.0,  1.0,  0.0, 0.0],
        [ 0.0,  0.0,  0.0, 1.0]
    ]}
}

pub fn aabb_aabb_intersects(min_a: Vector3, max_a: Vector3, min_b: Vector3, max_b: Vector3) -> bool {
    return min_a.x <= max_b.x && max_a.x >= min_b.x &&
            min_a.y <= max_b.y && max_a.y >= min_b.y &&
            min_a.z <= max_b.z && max_a.z >= min_b.z;
}