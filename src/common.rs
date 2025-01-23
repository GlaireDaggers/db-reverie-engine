use dbsdk_rs::math::Matrix4x4;

pub fn coord_space_transform() -> Matrix4x4 {
    // Quake coordinate system:
    // +X is right
    // +Y is towards viewer
    // +Z is up

    // DreamBox coordinate system:
    // +X is right
    // +Y is up
    // +Z is towards viewer

    Matrix4x4 {m: [
        [ 1.0,  0.0, 0.0, 0.0],
        [ 0.0,  0.0, 1.0, 0.0],
        [ 0.0,  1.0, 0.0, 0.0],
        [ 0.0,  0.0, 0.0, 1.0]
    ]}
}