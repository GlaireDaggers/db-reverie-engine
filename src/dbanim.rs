use std::io::{Read, ErrorKind, Seek};

use byteorder::{ReadBytesExt, LittleEndian};
use dbsdk_rs::{math::{Vector2, Vector3, Vector4, Quaternion}, db::log};

const DBA_VER: u32 = 1;

#[derive(Clone, Copy)]
pub enum AnimationCurveLoopMode {
    _Clamp,
    Repeat,
}

pub trait Lerp<T> where T : Clone + Copy {
    fn lerp(lhs: T, rhs: T, time: f32) -> T;
}

impl Lerp<f32> for f32 {
    fn lerp(lhs: f32, rhs: f32, time: f32) -> f32 {
        return lhs + ((rhs - lhs) * time.clamp(0.0, 1.0));
    }
}

impl Lerp<Vector2> for Vector2 {
    fn lerp(lhs: Vector2, rhs: Vector2, time: f32) -> Vector2 {
        return lhs + ((rhs - lhs) * time.clamp(0.0, 1.0));
    }
}

impl Lerp<Vector3> for Vector3 {
    fn lerp(lhs: Vector3, rhs: Vector3, time: f32) -> Vector3 {
        return lhs + ((rhs - lhs) * time.clamp(0.0, 1.0));
    }
}

impl Lerp<Vector4> for Vector4 {
    fn lerp(lhs: Vector4, rhs: Vector4, time: f32) -> Vector4 {
        return lhs + ((rhs - lhs) * time.clamp(0.0, 1.0));
    }
}

impl Lerp<Quaternion> for Quaternion {
    fn lerp(lhs: Quaternion, rhs: Quaternion, time: f32) -> Quaternion {
        let num = time;
        let num2: f32;
        let num3: f32;

        let mut num4 =
            (lhs.x * rhs.x) +
            (lhs.y * rhs.y) +
            (lhs.z * rhs.z) +
            (lhs.w * rhs.w);

        let mut flag: f32 = 1.0;
        
        if num4 < 0.0 {
            flag = -1.0;
            num4 = -num4;
        }

        if num4 > 0.999999 {
            num3 = 1.0 - num;
            num2 = num * flag;
        } else {
            let num5 = num4.acos();
            let num6 = 1.0 / num5.sin();
            num3 = ((1.0 - num) * num5).sin() * num6;
            num2 = flag * (num * num5).sin() * num6;
        }

        return Quaternion::new(
            (num3 * lhs.x) + (num2 * rhs.x),
            (num3 * lhs.y) + (num2 * rhs.y),
            (num3 * lhs.z) + (num2 * rhs.z),
            (num3 * lhs.w) + (num2 * rhs.w));
    }
}

/// Represents a keyframe of animation
pub struct AnimationKeyframe<T> where T : Clone + Copy {
    pub value: T,
    pub time: f32,
}

/// Represents a collection of animation keyframes which can be sampled at a given point in time
pub struct AnimationCurve<T> where T : Clone + Copy + Lerp<T> {
    pub keyframes: Vec<AnimationKeyframe<T>>,
    duration: f32,
}

impl<T> AnimationCurve<T> where T : Clone + Copy + Lerp<T> {
    pub fn new() -> AnimationCurve<T> {
        AnimationCurve { keyframes: Vec::new(), duration: 0.0 }
    }

    pub fn insert_keyframe(&mut self, value: T, time: f32) {
        for i in 0..self.keyframes.len() {
            if self.keyframes[i].time > time {
                self.keyframes.insert(i, AnimationKeyframe { value: value, time: time });
                return;
            }
        }
        self.keyframes.push(AnimationKeyframe { value: value, time: time });
        self.duration = time;
    }

    /// Get the duration of this animation curve
    pub fn duration(&self) -> f32 {
        return self.duration;
    }

    /// Sample this animation curve at the given point in time
    pub fn sample(&self, time: f32, loop_mode: AnimationCurveLoopMode) -> Result<T, ()> {
        // cannot sample a curve with no keyframes
        if self.keyframes.len() == 0 {
            return Err(());
        }

        if self.keyframes.len() == 1 {
            return Ok(self.keyframes[0].value);
        }

        if time < self.keyframes[0].time {
            return Ok(self.keyframes[0].value);
        }

        let mut sample_time = time;

        if sample_time >= self.duration {
            match loop_mode {
                AnimationCurveLoopMode::_Clamp => {
                    return Ok(self.keyframes.last().unwrap().value);
                },
                AnimationCurveLoopMode::Repeat => {
                    // loop within bounds
                    while sample_time >= self.duration {
                        sample_time -= self.duration;
                    }
                }
            };
        }

        for i in 0..self.keyframes.len() {
            if self.keyframes[i].time > sample_time {
                let lhs = &self.keyframes[i - 1];
                let rhs = &self.keyframes[i];
                let lerp_t = (sample_time - lhs.time) / (rhs.time - lhs.time);
                return Ok(T::lerp(lhs.value, rhs.value, lerp_t));
            }
        }

        return Err(());
    }
}

/// Represents a single channel of an animation file
pub struct DBAnimationChannel<T> where T : Clone + Copy + Lerp<T> {
    pub channelid : u32,
    pub bindingid : u32,
    pub curve: AnimationCurve<T>,
}

/// Represents an animation clip loaded from a DBA file
pub struct DBAnimationClip {
    pub channels_f32: Vec<DBAnimationChannel<f32>>,
    pub channels_vec2: Vec<DBAnimationChannel<Vector2>>,
    pub channels_vec3: Vec<DBAnimationChannel<Vector3>>,
    pub channels_vec4: Vec<DBAnimationChannel<Vector4>>,
    pub channels_quat: Vec<DBAnimationChannel<Quaternion>>,
    _duration: f32,
}

/// Enumeration of errors which can result from parsing a DBA animation file
#[derive(Debug)]
pub enum DBAnimationError {
    ParseError,
    VersionError,
    IOError(std::io::Error)
}

impl DBAnimationClip {
    pub fn new<R>(reader: &mut R) -> Result<DBAnimationClip,DBAnimationError> where R : Read + Seek {
        // read header
        let mut id: [u8;4] = [0;4];
        match reader.read_exact(&mut id) {
            Ok(_) => {
            },
            Err(e) => {
                return Err(DBAnimationError::IOError(e));
            }
        };

        match std::str::from_utf8(&id) {
            Ok("DBA\0") => {
            },
            _ => {
                log("Incorrect ID");
                return Err(DBAnimationError::ParseError);
            }
        }

        let ver = match reader.read_u32::<LittleEndian>() {
            Ok(v) => { v },
            Err(e) => {
                return Err(DBAnimationError::IOError(e));
            }
        };

        if ver != DBA_VER {
            return Err(DBAnimationError::VersionError);
        }

        let mut channels_f32: Vec<DBAnimationChannel<f32>> = Vec::new();
        let mut channels_vec2: Vec<DBAnimationChannel<Vector2>> = Vec::new();
        let mut channels_vec3: Vec<DBAnimationChannel<Vector3>> = Vec::new();
        let mut channels_vec4: Vec<DBAnimationChannel<Vector4>> = Vec::new();
        let mut channels_quat: Vec<DBAnimationChannel<Quaternion>> = Vec::new();

        let mut duration: f32 = 0.0;

        // scan chunks
        loop {
            let mut chunk_id: [u8;4] = [0;4];
            match reader.read_exact(&mut chunk_id) {
                Ok(_) => {
                },
                Err(e) => {
                    // EOF, no more chunks in stream
                    if e.kind() == ErrorKind::UnexpectedEof {
                        break;
                    }
                    return Err(DBAnimationError::IOError(e));
                }
            };

            let chunk_size = match reader.read_u32::<LittleEndian>() {
                Ok(v) => { v },
                Err(e) => {
                    return Err(DBAnimationError::IOError(e));
                }
            };

            match std::str::from_utf8(&chunk_id) {
                Ok("F32\0") => {
                    let channel_id = match reader.read_u32::<LittleEndian>() {
                        Ok(v) => { v },
                        Err(e) => {
                            return Err(DBAnimationError::IOError(e));
                        }
                    };

                    let binding_id = match reader.read_u32::<LittleEndian>() {
                        Ok(v) => { v },
                        Err(e) => {
                            return Err(DBAnimationError::IOError(e));
                        }
                    };

                    let key_cnt = match reader.read_u32::<LittleEndian>() {
                        Ok(v) => { v },
                        Err(e) => {
                            return Err(DBAnimationError::IOError(e));
                        }
                    } as usize;

                    let mut anim_curve: AnimationCurve<f32> = AnimationCurve::new();

                    for _ in 0..key_cnt {
                        let time = match reader.read_f32::<LittleEndian>() {
                            Ok(v) => { v },
                            Err(e) => {
                                return Err(DBAnimationError::IOError(e));
                            }
                        };

                        let val = match reader.read_f32::<LittleEndian>() {
                            Ok(v) => { v },
                            Err(e) => {
                                return Err(DBAnimationError::IOError(e));
                            }
                        };

                        anim_curve.insert_keyframe(val, time);
                    }

                    duration = duration.max(anim_curve.duration());
                    channels_f32.push(DBAnimationChannel {
                        channelid: channel_id,
                        bindingid: binding_id,
                        curve: anim_curve
                    });
                },
                Ok("VEC2") => {
                    let channel_id = match reader.read_u32::<LittleEndian>() {
                        Ok(v) => { v },
                        Err(e) => {
                            return Err(DBAnimationError::IOError(e));
                        }
                    };

                    let binding_id = match reader.read_u32::<LittleEndian>() {
                        Ok(v) => { v },
                        Err(e) => {
                            return Err(DBAnimationError::IOError(e));
                        }
                    };

                    let key_cnt = match reader.read_u32::<LittleEndian>() {
                        Ok(v) => { v },
                        Err(e) => {
                            return Err(DBAnimationError::IOError(e));
                        }
                    } as usize;

                    let mut anim_curve: AnimationCurve<Vector2> = AnimationCurve::new();

                    for _ in 0..key_cnt {
                        let time = match reader.read_f32::<LittleEndian>() {
                            Ok(v) => { v },
                            Err(e) => {
                                return Err(DBAnimationError::IOError(e));
                            }
                        };

                        let vx = match reader.read_f32::<LittleEndian>() {
                            Ok(v) => { v },
                            Err(e) => {
                                return Err(DBAnimationError::IOError(e));
                            }
                        };

                        let vy = match reader.read_f32::<LittleEndian>() {
                            Ok(v) => { v },
                            Err(e) => {
                                return Err(DBAnimationError::IOError(e));
                            }
                        };

                        anim_curve.insert_keyframe(Vector2::new(vx, vy), time);
                    }
                    
                    duration = duration.max(anim_curve.duration());
                    channels_vec2.push(DBAnimationChannel {
                        channelid: channel_id,
                        bindingid: binding_id,
                        curve: anim_curve
                    });
                },
                Ok("VEC3") => {
                    let channel_id = match reader.read_u32::<LittleEndian>() {
                        Ok(v) => { v },
                        Err(e) => {
                            return Err(DBAnimationError::IOError(e));
                        }
                    };

                    let binding_id = match reader.read_u32::<LittleEndian>() {
                        Ok(v) => { v },
                        Err(e) => {
                            return Err(DBAnimationError::IOError(e));
                        }
                    };

                    let key_cnt = match reader.read_u32::<LittleEndian>() {
                        Ok(v) => { v },
                        Err(e) => {
                            return Err(DBAnimationError::IOError(e));
                        }
                    } as usize;

                    let mut anim_curve: AnimationCurve<Vector3> = AnimationCurve::new();

                    for _ in 0..key_cnt {
                        let time = match reader.read_f32::<LittleEndian>() {
                            Ok(v) => { v },
                            Err(e) => {
                                return Err(DBAnimationError::IOError(e));
                            }
                        };

                        let vx = match reader.read_f32::<LittleEndian>() {
                            Ok(v) => { v },
                            Err(e) => {
                                return Err(DBAnimationError::IOError(e));
                            }
                        };

                        let vy = match reader.read_f32::<LittleEndian>() {
                            Ok(v) => { v },
                            Err(e) => {
                                return Err(DBAnimationError::IOError(e));
                            }
                        };

                        let vz = match reader.read_f32::<LittleEndian>() {
                            Ok(v) => { v },
                            Err(e) => {
                                return Err(DBAnimationError::IOError(e));
                            }
                        };

                        anim_curve.insert_keyframe(Vector3::new(vx, vy, vz), time);
                    }
                    
                    duration = duration.max(anim_curve.duration());
                    channels_vec3.push(DBAnimationChannel {
                        channelid: channel_id,
                        bindingid: binding_id,
                        curve: anim_curve
                    });
                },
                Ok("VEC4") => {
                    let channel_id = match reader.read_u32::<LittleEndian>() {
                        Ok(v) => { v },
                        Err(e) => {
                            return Err(DBAnimationError::IOError(e));
                        }
                    };

                    let binding_id = match reader.read_u32::<LittleEndian>() {
                        Ok(v) => { v },
                        Err(e) => {
                            return Err(DBAnimationError::IOError(e));
                        }
                    };

                    let key_cnt = match reader.read_u32::<LittleEndian>() {
                        Ok(v) => { v },
                        Err(e) => {
                            return Err(DBAnimationError::IOError(e));
                        }
                    } as usize;

                    let mut anim_curve: AnimationCurve<Vector4> = AnimationCurve::new();

                    for _ in 0..key_cnt {
                        let time = match reader.read_f32::<LittleEndian>() {
                            Ok(v) => { v },
                            Err(e) => {
                                return Err(DBAnimationError::IOError(e));
                            }
                        };

                        let vx = match reader.read_f32::<LittleEndian>() {
                            Ok(v) => { v },
                            Err(e) => {
                                return Err(DBAnimationError::IOError(e));
                            }
                        };

                        let vy = match reader.read_f32::<LittleEndian>() {
                            Ok(v) => { v },
                            Err(e) => {
                                return Err(DBAnimationError::IOError(e));
                            }
                        };

                        let vz = match reader.read_f32::<LittleEndian>() {
                            Ok(v) => { v },
                            Err(e) => {
                                return Err(DBAnimationError::IOError(e));
                            }
                        };

                        let vw = match reader.read_f32::<LittleEndian>() {
                            Ok(v) => { v },
                            Err(e) => {
                                return Err(DBAnimationError::IOError(e));
                            }
                        };

                        anim_curve.insert_keyframe(Vector4::new(vx, vy, vz, vw), time);
                    }
                    
                    duration = duration.max(anim_curve.duration());
                    channels_vec4.push(DBAnimationChannel {
                        channelid: channel_id,
                        bindingid: binding_id,
                        curve: anim_curve
                    });
                },
                Ok("QUAT") => {
                    let channel_id = match reader.read_u32::<LittleEndian>() {
                        Ok(v) => { v },
                        Err(e) => {
                            return Err(DBAnimationError::IOError(e));
                        }
                    };

                    let binding_id = match reader.read_u32::<LittleEndian>() {
                        Ok(v) => { v },
                        Err(e) => {
                            return Err(DBAnimationError::IOError(e));
                        }
                    };

                    let key_cnt = match reader.read_u32::<LittleEndian>() {
                        Ok(v) => { v },
                        Err(e) => {
                            return Err(DBAnimationError::IOError(e));
                        }
                    } as usize;

                    let mut anim_curve: AnimationCurve<Quaternion> = AnimationCurve::new();

                    for _ in 0..key_cnt {
                        let time = match reader.read_f32::<LittleEndian>() {
                            Ok(v) => { v },
                            Err(e) => {
                                return Err(DBAnimationError::IOError(e));
                            }
                        };

                        let vx = match reader.read_f32::<LittleEndian>() {
                            Ok(v) => { v },
                            Err(e) => {
                                return Err(DBAnimationError::IOError(e));
                            }
                        };

                        let vy = match reader.read_f32::<LittleEndian>() {
                            Ok(v) => { v },
                            Err(e) => {
                                return Err(DBAnimationError::IOError(e));
                            }
                        };

                        let vz = match reader.read_f32::<LittleEndian>() {
                            Ok(v) => { v },
                            Err(e) => {
                                return Err(DBAnimationError::IOError(e));
                            }
                        };

                        let vw = match reader.read_f32::<LittleEndian>() {
                            Ok(v) => { v },
                            Err(e) => {
                                return Err(DBAnimationError::IOError(e));
                            }
                        };

                        anim_curve.insert_keyframe(Quaternion::new(vx, vy, vz, vw), time);
                    }
                    
                    duration = duration.max(anim_curve.duration());
                    channels_quat.push(DBAnimationChannel {
                        channelid: channel_id,
                        bindingid: binding_id,
                        curve: anim_curve
                    });
                },
                _ => {
                    // unknown chunk ID, skip
                    match reader.seek(std::io::SeekFrom::Current(chunk_size as i64)) {
                        Ok(_) => {
                        },
                        Err(e) => {
                            return Err(DBAnimationError::IOError(e));
                        }
                    }
                }
            }
        }

        return Ok(DBAnimationClip {
            _duration: duration,
            channels_f32: channels_f32,
            channels_vec2: channels_vec2,
            channels_vec3: channels_vec3,
            channels_vec4: channels_vec4,
            channels_quat: channels_quat,
        });
    }

    /// Get the total duration of this animation clip
    pub fn _duration(&self) -> f32 {
        return self._duration;
    }

    /// Get the f32 animation channel with the given channel & binding id, or none
    pub fn _get_channel_f32(&self, channel_id: u32, binding_id: u32) -> Option<&AnimationCurve<f32>> {
        for channel in &self.channels_f32 {
            if channel.channelid == channel_id && channel.bindingid == binding_id {
                return Some(&channel.curve);
            }
        }
        return None;
    }

    /// Get the Vector2 animation channel with the given channel & binding id, or none
    pub fn _get_channel_vec2(&self, channel_id: u32, binding_id: u32) -> Option<&AnimationCurve<Vector2>> {
        for channel in &self.channels_vec2 {
            if channel.channelid == channel_id && channel.bindingid == binding_id {
                return Some(&channel.curve);
            }
        }
        return None;
    }

    /// Get the Vector3 animation channel with the given channel & binding id, or none
    pub fn get_channel_vec3(&self, channel_id: u32, binding_id: u32) -> Option<&AnimationCurve<Vector3>> {
        for channel in &self.channels_vec3 {
            if channel.channelid == channel_id && channel.bindingid == binding_id {
                return Some(&channel.curve);
            }
        }
        return None;
    }

    /// Get the Vector4 animation channel with the given channel & binding id, or none
    pub fn _get_channel_vec4(&self, channel_id: u32, binding_id: u32) -> Option<&AnimationCurve<Vector4>> {
        for channel in &self.channels_vec4 {
            if channel.channelid == channel_id && channel.bindingid == binding_id {
                return Some(&channel.curve);
            }
        }
        return None;
    }

    /// Get the Quaternion animation channel with the given channel & binding id, or none
    pub fn get_channel_quat(&self, channel_id: u32, binding_id: u32) -> Option<&AnimationCurve<Quaternion>> {
        for channel in &self.channels_quat {
            if channel.channelid == channel_id && channel.bindingid == binding_id {
                return Some(&channel.curve);
            }
        }
        return None;
    }
}