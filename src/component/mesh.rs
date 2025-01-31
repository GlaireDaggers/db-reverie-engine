use std::sync::Arc;

use dbsdk_rs::math::Matrix4x4;

use crate::{dbanim::{AnimationCurveLoopMode, DBAnimationClip}, dbmesh::DBMesh};

pub struct Mesh {
    pub mesh: Arc<DBMesh>
}

pub struct MeshAnim {
    pub anim: Arc<DBAnimationClip>,
    pub loop_mode: AnimationCurveLoopMode,
    pub time: f32,
}

pub struct SkeletalPoseState {
    pub bone_palette: Vec<Matrix4x4>
}