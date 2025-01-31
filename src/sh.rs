use dbsdk_rs::math::{Matrix4x4, Vector3};

const BASIS_WEIGHT_CONSTANT: f32 = 0.282095;
const BASIS_WEIGHT_LINEAR: f32 = 0.325735;

pub struct SphericalHarmonics {
    pub coeff: Matrix4x4
}

impl SphericalHarmonics {
    pub const fn new() -> SphericalHarmonics {
        let coeff = Matrix4x4 {
            m: [
                [0.0, 0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ]
        };

        return SphericalHarmonics {
            coeff: coeff
        };
    }

    pub fn add_ambient_light(&mut self, color: Vector3) {
        // when storing L1 coefficients in a matrix, the fourth column of the matrix stores constant terms for R, G, and B channels.
        // we can just add ambient color to this column.

        self.coeff.m[3][0] += color.x;
        self.coeff.m[3][1] += color.y;
        self.coeff.m[3][2] += color.z;
    }

    pub fn add_directional_light(&mut self, dir: Vector3, color: Vector3) {
        let mut direction = dir;
        direction.normalize();

        // when storing L1 coefficients in a matrix, the first three columns store the directional coefficients of RGB channels for X, Y, and Z axes respectively.
        // you can think of this as storing average directional intensity of each R, G, and B.
        // this WILL of course result in ringing artifacts the more intense the directional component, but that's a known limitation of L1 spherical harmonics
        
        self.coeff.m[0][0] += direction.x * color.x * BASIS_WEIGHT_LINEAR;
        self.coeff.m[0][1] += direction.x * color.y * BASIS_WEIGHT_LINEAR;
        self.coeff.m[0][2] += direction.x * color.z * BASIS_WEIGHT_LINEAR;
        
        self.coeff.m[1][0] += direction.y * color.x * BASIS_WEIGHT_LINEAR;
        self.coeff.m[1][1] += direction.y * color.y * BASIS_WEIGHT_LINEAR;
        self.coeff.m[1][2] += direction.y * color.z * BASIS_WEIGHT_LINEAR;
        
        self.coeff.m[2][0] += direction.z * color.x * BASIS_WEIGHT_LINEAR;
        self.coeff.m[2][1] += direction.z * color.y * BASIS_WEIGHT_LINEAR;
        self.coeff.m[2][2] += direction.z * color.z * BASIS_WEIGHT_LINEAR;
        
        self.coeff.m[3][0] += color.x * BASIS_WEIGHT_CONSTANT;
        self.coeff.m[3][1] += color.y * BASIS_WEIGHT_CONSTANT;
        self.coeff.m[3][2] += color.z * BASIS_WEIGHT_CONSTANT;
    }
}