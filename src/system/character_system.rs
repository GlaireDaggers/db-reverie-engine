use dbsdk_rs::math::{Matrix4x4, Quaternion, Vector3, Vector4};
use hecs::{CommandBuffer, World};
use lazy_static::lazy_static;

use crate::{bsp_file::MASK_SOLID, component::{charactercontroller::{CharacterController, CharacterInputState, CharacterState}, fpview::FPView, playerinput::PlayerInput, transform3d::Transform3D}, InputState, MapData, TimeData};

const GROUND_SLOPE_ANGLE: f32 = 45.0;
const STEP_HEIGHT: f32 = 20.0;
const GRAVITY: f32 = 300.0;
const FRICTION: f32 = 0.2;
const MAX_ACCEL: f32 = 10.0;
const AIR_ACCEL: f32 = 1.0;

lazy_static! {
    static ref GROUND_SLOPE_COS_ANGLE: f32 = GROUND_SLOPE_ANGLE.to_radians().cos();
}
/// System which initializes characters
pub fn character_init(world: &mut World) {
    // initialize character state
    let mut cmd_buffer = CommandBuffer::new();
    for (eid, cc) in world.query_mut::<&CharacterController>().without::<&CharacterState>() {
        cmd_buffer.insert_one(eid, CharacterState::new(cc.main_height));
        cmd_buffer.insert_one(eid, CharacterInputState::default());
    }
    cmd_buffer.run_on(world);
}

/// System which rotates characters according to an attached FPView
pub fn character_rotation_update(world: &mut World) {
    for (_, (_, transform, fpview)) in world.query_mut::<(&CharacterController, &mut Transform3D, &FPView)>() {
        transform.rotation = Quaternion::from_euler(Vector3::new(0.0, 0.0, fpview.yaw.to_radians()));
    }
}

/// System which allows characters with a PlayerInput component to receive input
pub fn character_input_update(input: &InputState, world: &mut World) {
    for (_, (state, transform, _)) in world.query_mut::<(&mut CharacterInputState, &Transform3D, &PlayerInput)>() {
        let rot_matrix = Matrix4x4::rotation(transform.rotation);

        let fwd = rot_matrix * Vector4::new(0.0, 1.0, 0.0, 0.0);
        let right = rot_matrix * Vector4::new(1.0, 0.0, 0.0, 0.0);

        let fwd = Vector3::new(fwd.x, fwd.y, fwd.z);
        let right = Vector3::new(right.x, right.y, right.z);

        let input_velocity = (fwd * input.move_y)
            + (right * input.move_x);

        state.input_move_dir = input_velocity;
        state.input_crouch = input.crouch;
        state.input_jump = input.jump;
    }
}

/// System which applies input to characters
pub fn character_apply_input_update(time: &TimeData, map_data: &MapData, world: &mut World) {
    for (_, (state, cc, input, transform)) in world.query_mut::<(&mut CharacterState, &mut CharacterController, &CharacterInputState, &Transform3D)>() {
        if state.grounded {
            // apply friction
            state.velocity = state.velocity - (state.velocity * FRICTION);
        }

        let wish_dir = Vector3::new(input.input_move_dir.x, input.input_move_dir.y, 0.0);
        let accel = if state.grounded { MAX_ACCEL } else { AIR_ACCEL };
        
        if wish_dir.length_sq() > 0.0 {
            let wish_dir = wish_dir.normalized();
            let current_speed = Vector3::dot(&wish_dir, &state.velocity);
            let add_speed = (cc.move_speed - current_speed).clamp(0.0, accel * cc.move_speed * time.delta_time);
            
            state.velocity = state.velocity + (wish_dir * add_speed);
        }

        if state.crouched && !input.input_crouch {
            // make sure we have enough room to uncrouch before doing so
            let box_extents = Vector3::new(cc.radius, cc.radius, cc.main_height * 0.5);
            let box_offset = Vector3::unit_z() * (cc.main_height * 0.5);
            let box_pos = transform.position + box_offset;

            if !map_data.map.box_check(MASK_SOLID, &box_pos, box_extents) {
                state.crouched = false;
            }
        }
        else {
            state.crouched = input.input_crouch;
        }

        if state.grounded && input.input_jump {
            state.grounded = false;
            state.velocity.z = cc.jump_force;
        }

        state.height = if state.crouched { cc.crouch_height } else { cc.main_height };
        cc.height_offset = state.height * 0.5;
    }
}

/// System which controls movement of characters
pub fn character_update(time: &TimeData, map_data: &MapData, world: &mut World) {
    // update character physics
    for (_, (cc, cstate, transform)) in world.query_mut::<(&CharacterController, &mut CharacterState, &mut Transform3D)>() {
        let box_extents = Vector3::new(cc.radius, cc.radius, cstate.height * 0.5);
        let box_offset = Vector3::unit_z() * cc.height_offset;
        
        let box_pos = transform.position + box_offset;

        // sweep character sideways
        let move_vec_xy = Vector3::new(cstate.velocity.x, cstate.velocity.y, 0.0);

        let (box_pos, move_vec_xy) = if cstate.grounded && move_vec_xy.length_sq() > f32::EPSILON {
            let original_pos = box_pos;
            let original_move_vec_xy = move_vec_xy;

            // while on the ground, sweep up by step height, sweep sideways, then sweep back down by step height.
            let (box_pos, _, _) = map_data.map.trace_move(&box_pos, &Vector3::new(0.0, 0.0, STEP_HEIGHT), 1.0, false, box_extents);
            let (box_pos, move_vec_xy, _) = map_data.map.trace_move(&box_pos, &move_vec_xy, time.delta_time, true, box_extents);
            let (box_pos, _, trace) = map_data.map.trace_move(&box_pos, &Vector3::new(0.0, 0.0, -STEP_HEIGHT), 1.0, false, box_extents);

            // if we leave the ground, see if the ground is still close enough to step down
            let (box_pos, move_vec_xy) = if trace.fraction == 1.0 {
                let (new_pos, _, trace) = map_data.map.trace_move(&box_pos, &Vector3::new(0.0, 0.0, -STEP_HEIGHT), 1.0, false, box_extents);

                if trace.fraction < 1.0 {
                    (new_pos, move_vec_xy)
                }
                else {
                    (box_pos, move_vec_xy)
                }
            }
            else {
                // if we stepped onto ground that's too steep, reset back to original pos and just do a normal sweep instead
                let hit_normal = map_data.map.plane_lump.planes[trace.plane as usize].normal;
                if hit_normal.z < *GROUND_SLOPE_COS_ANGLE {
                    let (box_pos, move_vec_xy, _) = map_data.map.trace_move(&original_pos, &original_move_vec_xy, time.delta_time, true, box_extents);
                    (box_pos, move_vec_xy)
                }
                else {
                    (box_pos, move_vec_xy)
                }
            };

            (box_pos, Vector3::new(move_vec_xy.x, move_vec_xy.y, f32::min(move_vec_xy.z, 0.0)))
        }
        else {
            let (box_pos, move_vec_xy, _) = map_data.map.trace_move(&box_pos, &move_vec_xy, time.delta_time, true, box_extents);
            (box_pos, move_vec_xy)
        };

        // sweep character down
        let move_vec_z = Vector3::unit_z() * cstate.velocity.z;
        let (box_pos, mut move_vec_z, trace) = map_data.map.trace_move(&box_pos, &move_vec_z, time.delta_time, !cstate.grounded, box_extents);

        // if we hit something while moving down, & slope is within threshold, set character to grounded state
        if cstate.velocity.z < 0.0 && trace.fraction < 1.0 {
            let hit_normal = map_data.map.plane_lump.planes[trace.plane as usize].normal;
            if hit_normal.z >= *GROUND_SLOPE_COS_ANGLE {
                cstate.grounded = true;
            }
            else {
                cstate.grounded = false;
            }
        }
        else if cstate.velocity.z > 0.0 && trace.fraction < 1.0 {
            // clamp velocity if we hit our head
            move_vec_z.z = 0.0;
        }
        else {
            cstate.grounded = false;
        }

        // update transform & character state
        transform.position = box_pos - box_offset;

        let prev_velocity = cstate.velocity;
        cstate.velocity = move_vec_xy + move_vec_z;

        cstate.velocity.z = f32::min(cstate.velocity.z, prev_velocity.z);
        
        // apply gravity
        if !cstate.grounded {
            cstate.velocity.z -= GRAVITY * time.delta_time;
        }
        else {
            cstate.velocity.z = -1.0;
        }
    }
}