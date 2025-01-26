use std::collections::HashSet;
use dbsdk_rs::{db::log, math::Vector3};
use crate::bsp_file::{BspFile, MASK_SOLID};

const DIST_EPSILON: f32 = 0.01;

pub struct Trace {
    pub all_solid: bool,
    pub start_solid: bool,
    pub fraction: f32,
    pub end_pos: Vector3,
    pub plane: i32
}

impl BspFile {
    fn trace_brush(self: &Self, brush_idx: usize, start: &Vector3, end: &Vector3, frac_adj: f32, box_extents: Option<&Vector3>, trace: &mut Trace) {
        let brush = &self.brush_lump.brushes[brush_idx];

        if brush.num_brush_sides == 0 {
            return;
        }

        let mut hitplane = -1;
        let mut enterfrac = f32::MIN;
        let mut exitfrac = 1.0;
        let mut startout = false;
        let mut getout = false;

        for i in 0..brush.num_brush_sides {
            let side = &self.brush_side_lump.brush_sides[(brush.first_brush_side + i) as usize];
            let plane = &self.plane_lump.planes[side.plane as usize];

            let dist = match box_extents {
                Some(v) => {
                    let offs = Vector3::new(
                        if plane.normal.x < 0.0 { v.x } else { -v.x },
                        if plane.normal.y < 0.0 { v.y } else { -v.y },
                        if plane.normal.z < 0.0 { v.z } else { -v.z }
                    );

                    plane.distance - Vector3::dot(&offs, &plane.normal)
                }
                None => {
                    plane.distance
                }
            };

            let d1 = Vector3::dot(start, &plane.normal) - dist;
            let d2 = Vector3::dot(end, &plane.normal) - dist;

            if d2 > 0.0 {
                getout = true;
            }

            if d1 > 0.0 {
                startout = true;
            }

            if d1 > 0.0 && d2 >= d1 {
                return;
            }

            if d1 <= 0.0 && d2 <= 0.0 {
                continue;
            }

            if d1 > d2 {
                let f = (d1 - DIST_EPSILON) / (d1 - d2);
                if f > enterfrac {
                    enterfrac = f;
                    hitplane = side.plane as i32;
                }
            }
            else {
                let f = (d1 + DIST_EPSILON) / (d1 - d2);
                if f < exitfrac {
                    exitfrac = f;
                }
            }
        }
        
        if !startout {
            trace.start_solid = true;
            if !getout {
                trace.all_solid = true;
            }

            return;
        }

        if enterfrac < exitfrac {
            if enterfrac > f32::MIN && enterfrac < trace.fraction {
                if enterfrac < 0.0 {
                    enterfrac = 0.0;
                }

                trace.fraction = enterfrac + frac_adj;
                trace.plane = hitplane;
            }
        }
    }

    fn trace_leaf(self: &Self, leaf_index: usize, checked_brush: &mut HashSet<u16>, content_mask: u32, start: &Vector3, end: &Vector3, frac_adj: f32, box_extents: Option<&Vector3>, trace: &mut Trace) {
        let leaf = &self.leaf_lump.leaves[leaf_index];

        if leaf.contents & content_mask == 0 {
            return;
        }

        // linetrace all brushes in leaf
        for i in 0..leaf.num_leaf_brushes {
            let brush_idx = self.leaf_brush_lump.brushes[(leaf.first_leaf_brush + i) as usize];
            
            // ensure we don't process the same brush more than once during a trace
            if checked_brush.contains(&brush_idx) {
                continue;
            }
            checked_brush.insert(brush_idx);

            let brush = &self.brush_lump.brushes[brush_idx as usize];

            if brush.contents & content_mask == 0 {
                return;
            }

            self.trace_brush(brush_idx as usize, start, end, frac_adj, box_extents, trace);

            if trace.fraction <= 0.0 {
                return;
            }
        }
    }

    fn recursive_trace(self: &Self, node_idx: i32, checked_brush: &mut HashSet<u16>, content_mask: u32, p1f: f32, p2f: f32, start: &Vector3, end: &Vector3, frac_adj: f32, box_extents: Option<&Vector3>, trace: &mut Trace) {
        if trace.fraction <= p1f {
            return;
        }
        
        if node_idx < 0 {
            self.trace_leaf((-node_idx - 1) as usize, checked_brush, content_mask, start, end, frac_adj, box_extents, trace);
            return;
        }

        let node = &self.node_lump.nodes[node_idx as usize];
        let plane = &self.plane_lump.planes[node.plane as usize];

        let (t1, t2, offset) = if plane.plane_type == 0 {
            let t1 = start.x - plane.distance;
            let t2 = end.x - plane.distance;
            let offset = match box_extents {
                Some(v) => {
                    v.x
                }
                None => {
                    0.0
                }
            };

            (t1, t2, offset)
        }
        else if plane.plane_type == 1 {
            let t1 = start.y - plane.distance;
            let t2 = end.y - plane.distance;
            let offset = match box_extents {
                Some(v) => {
                    v.y
                }
                None => {
                    0.0
                }
            };

            (t1, t2, offset)
        }
        else if plane.plane_type == 2 {
            let t1 = start.z - plane.distance;
            let t2 = end.z - plane.distance;
            let offset = match box_extents {
                Some(v) => {
                    v.z
                }
                None => {
                    0.0
                }
            };

            (t1, t2, offset)
        }
        else {
            let t1 = Vector3::dot(&plane.normal, start) - plane.distance;
            let t2 = Vector3::dot(&plane.normal, end) - plane.distance;
            let offset = match box_extents {
                Some(v) => {
                    (v.x * plane.normal.x).abs() +
                    (v.y * plane.normal.y).abs() +
                    (v.z * plane.normal.z).abs()
                },
                None => {
                    0.0
                }
            };

            (t1, t2, offset)
        };

        if t1 >= offset && t2 >= offset {
            self.recursive_trace(node.front_child, checked_brush, content_mask, p1f, p2f, start, end, frac_adj, box_extents, trace);
            return;
        }

        if t1 < -offset && t2 < -offset {
            self.recursive_trace(node.back_child, checked_brush, content_mask, p1f, p2f, start, end, frac_adj, box_extents, trace);
            return;
        }

        self.recursive_trace(node.front_child, checked_brush, content_mask, p1f, p2f, start, end, frac_adj, box_extents, trace);
        self.recursive_trace(node.back_child, checked_brush, content_mask, p1f, p2f, start, end, frac_adj, box_extents, trace);

        /*let (side, frac2, frac) = if t1 < t2 {
            let idist = 1.0 / (t1 - t2);
            (
                true,
                (t1 + offset + DIST_EPSILON)*idist,
                (t1 - offset - DIST_EPSILON)*idist
            )
        }
        else if t1 > t2 {
            let idist = 1.0 / (t1 - t2);
            (
                false,
                (t1 - offset - DIST_EPSILON)*idist,
                (t1 + offset + DIST_EPSILON)*idist
            )
        }
        else {
            (
                false,
                0.0,
                1.0
            )
        };

        // move up to the node
        let frac = frac.clamp(0.0, 1.0);

        let midf = p1f + ((p2f - p1f) * frac);
        let mid = *start + ((*end - *start) * frac);

        self.recursive_trace(if side { node.back_child } else { node.front_child }, checked_brush, content_mask, p1f, midf, start, &mid, frac_adj, box_extents, trace);

        // go past the node
        let frac2 = frac2.clamp(0.0, 1.0);

        let midf = p1f + ((p2f - p1f) * frac2);
        let mid = *start + ((*end - *start) * frac2);

        self.recursive_trace(if side { node.front_child } else { node.back_child }, checked_brush, content_mask, midf, p2f, &mid, end, frac_adj + frac2, box_extents, trace);*/
    }

    /// Sweeps a box shape through the world & returns information about what was hit and where, if any
    pub fn boxtrace(self: &Self, content_mask: u32, start: &Vector3, end: &Vector3, box_extents: Vector3) -> Trace {
        let head_node = self.submodel_lump.submodels[0].headnode as i32;

        let mut trace_trace = Trace {
            all_solid: false,
            start_solid: false,
            fraction: 1.0,
            end_pos: Vector3::zero(),
            plane: -1
        };

        self.recursive_trace(head_node, &mut HashSet::<u16>::new(), content_mask, 0.0, 1.0, start, end, 0.0, Some(&box_extents), &mut trace_trace);

        if trace_trace.fraction == 1.0 {
            trace_trace.end_pos = *end;
        }
        else {
            trace_trace.end_pos = *start + ((*end - *start) * trace_trace.fraction);
        }

        trace_trace
    }

    /// Trace a line through the world & returns information about what was hit and where, if any
    pub fn linetrace(self: &Self, content_mask: u32, start: &Vector3, end: &Vector3) -> Trace {
        let head_node = self.submodel_lump.submodels[0].headnode as i32;

        let mut trace_trace = Trace {
            all_solid: false,
            start_solid: false,
            fraction: 1.0,
            end_pos: Vector3::zero(),
            plane: -1
        };

        self.recursive_trace(head_node, &mut HashSet::<u16>::new(), content_mask, 0.0, 1.0, start, end, 0.0, None, &mut trace_trace);

        if trace_trace.fraction == 1.0 {
            trace_trace.end_pos = *end;
        }
        else {
            trace_trace.end_pos = *start + ((*end - *start) * trace_trace.fraction);
        }

        trace_trace
    }

    /// Calculate the index of the leaf node which contains the given point
    pub fn calc_leaf_index(self: &Self, position: &Vector3) -> i32 {
        let mut cur_node: i32 = 0;

        while cur_node >= 0 {
            let node = &self.node_lump.nodes[cur_node as usize];
            let plane = &self.plane_lump.planes[node.plane as usize];

            // what side of the plane is this point on
            let side = Vector3::dot(&position, &plane.normal) - plane.distance;
            if side >= 0.0 {
                cur_node = node.front_child;
            }
            else {
                cur_node = node.back_child;
            }
        }

        // leaf indices are encoded as negative numbers: -(leaf_idx + 1)
        return -cur_node - 1;
    }

    /// Attempts to sweep a box through the world, sliding along any surfaces it hits and returning a new position and velocity
    /// 
    /// # Arguments
    /// 
    /// * 'start_pos' - The current center point of the box shape
    /// * 'velocity' - The velocity of the box shape
    /// * 'delta' - The timestep of the movement (final sweep length is velocity times delta)
    /// * 'box_extents' - The extents of the box on each axis (half the box's total size)
    pub fn trace_move(self: &Self, start_pos: &Vector3, velocity: &Vector3, delta: f32, box_extents: Vector3) -> (Vector3, Vector3) {
        const NUM_ITERATIONS: usize = 8;

        let mut cur_pos = *start_pos;
        let mut cur_velocity = *velocity;
        let mut remaining_delta = delta;

        let mut planes: [Vector3; NUM_ITERATIONS] = [Vector3::zero(); NUM_ITERATIONS];
        let mut num_planes: usize = 0;

        for _iter in 0..NUM_ITERATIONS {
            let end = cur_pos + (cur_velocity * remaining_delta);
            let trace = self.boxtrace(MASK_SOLID, &cur_pos, &end, box_extents);

            if trace.all_solid {
                log(format!("STUCK AT {}, {}, {}", cur_pos.x, cur_pos.y, cur_pos.z).as_str());
                return (cur_pos, Vector3::zero());
            }

            if trace.fraction > 0.0 {
                num_planes = 0;
                cur_pos = trace.end_pos;
                remaining_delta -= remaining_delta * trace.fraction;
            }

            if trace.fraction == 1.0 {
                break;
            }

            let plane = &self.plane_lump.planes[trace.plane as usize];
            planes[num_planes] = plane.normal;
            num_planes += 1;

            let mut broke_i: bool = false;
            for i in 0..num_planes {
                // clip velocity to plane
                let backoff = Vector3::dot(&cur_velocity, &planes[i]) * 1.01;
                cur_velocity = cur_velocity - (planes[i] * backoff);

                let mut broke_j = false;
                for j in 0..num_planes {
                    if j != i {
                        if Vector3::dot(&cur_velocity, &planes[j]) < 0.0 {
                            broke_j = true;
                            break;
                        }
                    }
                }

                if !broke_j {
                    broke_i = true;
                    break;
                }
            }

            if broke_i {
                // go along this plane
            }
            else {
                // go along the crease
                if num_planes != 2 {
                    break;
                }

                let dir = Vector3::cross(&planes[0], &planes[1]);
                let d = Vector3::dot(&dir, &cur_velocity);
                cur_velocity = dir * d;
            }
        }

        (cur_pos, cur_velocity)
    }
}