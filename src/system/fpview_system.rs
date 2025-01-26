use dbsdk_rs::math::{Quaternion, Vector3};
use hecs::World;

use crate::{component::{fpview::FPView, playerinput::PlayerInput, transform3d::Transform3D}, InputState};

/// System which allows player to control yaw/pitch of FPView
pub fn fpview_input_system_update(input: &InputState, world: &mut World) {
    for (_, (fpview, _)) in world.query_mut::<(&mut FPView, &PlayerInput)>() {
        fpview.yaw += input.look_x * 45.0 * (1.0 / 60.0);
        fpview.pitch -= input.look_y * 45.0 * (1.0 / 60.0);

        if fpview.yaw < 0.0 {
            fpview.yaw += 360.0;
        }
        else if fpview.yaw > 360.0 {
            fpview.yaw -= 360.0;
        }

        fpview.pitch = fpview.pitch.clamp(-90.0, 90.0);
    }
}

/// System which translates yaw/pitch of FPView to a quaternion rotation on a Transform3D
pub fn fpview_transform_system_update(world: &mut World) {
    for (_, (fpview, transform)) in world.query_mut::<(&FPView, &mut Transform3D)>() {
        transform.rotation = Quaternion::from_euler(Vector3::new(fpview.pitch.to_radians(), 0.0, fpview.yaw.to_radians()))
    }
}