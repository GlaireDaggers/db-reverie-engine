use dbsdk_rs::math::Vector3;

pub fn parse_vec3(src: &str) -> Vector3 {
    let mut split = src.split_whitespace();
    let x = split.nth(0).unwrap().parse::<f32>().unwrap();
    let y = split.nth(0).unwrap().parse::<f32>().unwrap();
    let z = split.nth(0).unwrap().parse::<f32>().unwrap();

    return Vector3::new(x, y, z);
}