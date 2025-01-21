use dbsdk_rs::math::Vector3;

use crate::bsp_file::BspFile;

pub struct BspMap {
    file: BspFile
}

impl BspMap {
    pub fn new(bsp_file: BspFile) -> BspMap {
        BspMap {
            file: bsp_file
        }
    }

    pub fn calc_leaf_index(self: &Self, position: Vector3) -> usize {
        let mut cur_node: i32 = 0;

        while cur_node >= 0 {
            let node = &self.file.node_lump.nodes[cur_node as usize];
            let plane = &self.file.plane_lump.planes[node.plane as usize];

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
        let leaf_index = -cur_node - 1;
        return leaf_index as usize;
    }
}