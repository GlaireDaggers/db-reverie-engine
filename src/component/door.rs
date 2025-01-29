use dbsdk_rs::math::Vector3;
use hecs::Entity;

pub struct Door {
    pub auto_open: bool,
    pub close_pos: Vector3,
    pub open_pos: Vector3,
    pub move_speed: f32,
}

pub struct DoorLink {
    pub links: Vec<Entity>
}

pub struct DoorOpener {
}