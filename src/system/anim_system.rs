use dbsdk_rs::math::{Matrix4x4, Quaternion, Vector3};
use hecs::{CommandBuffer, World};

use crate::{component::mesh::{Mesh, MeshAnim, SkeletalPoseState}, dbanim::{AnimationCurveLoopMode, DBAnimationClip}, dbmesh::{DBSkelNode, DBSkeleton}, TimeData};

fn sample_anim_node(node: &DBSkelNode, anim: &DBAnimationClip, time: f32, loopmode: AnimationCurveLoopMode, parent_mat: Matrix4x4, bonepalette: &mut [Matrix4x4]) {
    let mut local_pos = Vector3::zero();
    let mut local_rot = Quaternion::identity();
    let mut local_scale = Vector3::new(1.0, 1.0, 1.0);

    match anim.get_channel_vec3(node.bone_index as u32, 0) {
        Some(channel) => {
            local_pos = match channel.sample(time, loopmode) {
                Ok(v) => { v }
                Err(_) => { Vector3::zero() }
            };
        }
        None => {
        }
    };

    match anim.get_channel_quat(node.bone_index as u32, 1) {
        Some(channel) => {
            local_rot = match channel.sample(time, loopmode) {
                Ok(v) => { v }
                Err(_) => { Quaternion::identity() }
            };
        }
        None => {
        }
    };

    match anim.get_channel_vec3(node.bone_index as u32, 2) {
        Some(channel) => {
            local_scale = match channel.sample(time, loopmode) {
                Ok(v) => { v }
                Err(_) => { Vector3::new(1.0, 1.0, 1.0) }
            };
        }
        None => {
        }
    };

    // compute skinning matrix
    // in order, this matrix:
    // - transforms vertex into bone local space
    // - applies animation transform relative to rest pose
    // - applies rest pose
    // - transforms vertex back into object space (using accumulated parent transform)

    let object_to_bone = node.inv_bind_pose;

    // compute bone to object
    let mut bone_to_object = Matrix4x4::identity();
    Matrix4x4::load_simd(&Matrix4x4::scale(local_scale));
    Matrix4x4::mul_simd(&Matrix4x4::rotation(local_rot));
    Matrix4x4::mul_simd(&Matrix4x4::translation(local_pos));
    Matrix4x4::mul_simd(&node.local_rest_pose);
    Matrix4x4::mul_simd(&parent_mat);
    Matrix4x4::store_simd(&mut bone_to_object);

    // compute skinning matrix
    let mut skin_mat = Matrix4x4::identity();
    Matrix4x4::load_simd(&object_to_bone);
    Matrix4x4::mul_simd(&bone_to_object);
    Matrix4x4::store_simd(&mut skin_mat);

    // write result to bone matrix palette
    bonepalette[node.bone_index as usize] = skin_mat;

    // iterate children
    for child in &node.children {
        sample_anim_node(child, anim, time, loopmode, bone_to_object, bonepalette);
    }
}

fn sample_anim(skeleton: &DBSkeleton, anim: &DBAnimationClip, time: f32, loopmode: AnimationCurveLoopMode, bonepalette: &mut [Matrix4x4]) {
    for root in skeleton.nodes.as_slice() {
        sample_anim_node(root, anim, time, loopmode, Matrix4x4::identity(), bonepalette);
    }
}

// initialize skeletal animation state
fn sk_anim_init(world: &mut World) {
    let mut cmd_buf = CommandBuffer::new();
    for (e, (_mesh_anim, mesh)) in world.query_mut::<(&MeshAnim, &Mesh)>() {
        let bone_palette: Vec<Matrix4x4> = vec![Matrix4x4::identity();mesh.mesh.skeleton.as_ref().unwrap().bone_count as usize];
        cmd_buf.insert_one(e, SkeletalPoseState {
            bone_palette
        });
    }
    cmd_buf.run_on(world);
}

// update skeletal animation
fn sk_anim_update(time: &TimeData, world: &mut World) {
    for (_, (mesh_anim, mesh, pose_state)) in world.query_mut::<(&mut MeshAnim, &Mesh, &mut SkeletalPoseState)>() {
        // sample animation
        sample_anim(mesh.mesh.skeleton.as_ref().unwrap(), &mesh_anim.anim, mesh_anim.time, mesh_anim.loop_mode, &mut pose_state.bone_palette);

        mesh_anim.time += time.delta_time;
    }
}

/// System which performs skeletal animation & computes bone transforms
pub fn sk_anim_system_update(time: &TimeData, world: &mut World) {
    sk_anim_init(world);
    sk_anim_update(time, world);
}