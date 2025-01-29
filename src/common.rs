use dbsdk_rs::math::{Matrix4x4, Vector3, Vector4};

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

pub fn extract_frustum(viewproj: &Matrix4x4) -> [Vector4;6] {
    let row1 = Vector4::new(viewproj.m[0][0], viewproj.m[1][0], viewproj.m[2][0], viewproj.m[3][0]);
    let row2 = Vector4::new(viewproj.m[0][1], viewproj.m[1][1], viewproj.m[2][1], viewproj.m[3][1]);
    let row3 = Vector4::new(viewproj.m[0][2], viewproj.m[1][2], viewproj.m[2][2], viewproj.m[3][2]);
    let row4 = Vector4::new(viewproj.m[0][3], viewproj.m[1][3], viewproj.m[2][3], viewproj.m[3][3]);

    [
        row4 + row1,
        row4 - row1,
        row4 + row2,
        row4 - row2,
        row4 + row3,
        row4 - row3,
    ]
}

pub fn aabb_frustum(min: Vector3, max: Vector3, frustum: &[Vector4]) -> bool {
    for plane in frustum {
        if Vector4::dot(&plane, &Vector4::new(min.x, min.y, min.z, 1.0)) <= 0.0 &&
            Vector4::dot(&plane, &Vector4::new(max.x, min.y, min.z, 1.0)) <= 0.0 &&
            Vector4::dot(&plane, &Vector4::new(min.x, max.y, min.z, 1.0)) <= 0.0 &&
            Vector4::dot(&plane, &Vector4::new(max.x, max.y, min.z, 1.0)) <= 0.0 &&
            Vector4::dot(&plane, &Vector4::new(min.x, min.y, max.z, 1.0)) <= 0.0 &&
            Vector4::dot(&plane, &Vector4::new(max.x, min.y, max.z, 1.0)) <= 0.0 &&
            Vector4::dot(&plane, &Vector4::new(min.x, max.y, max.z, 1.0)) <= 0.0 &&
            Vector4::dot(&plane, &Vector4::new(max.x, max.y, max.z, 1.0)) <= 0.0 {
            return false;
        }
    }

    return true;
}