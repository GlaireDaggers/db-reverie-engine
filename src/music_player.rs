use std::{cell::Cell, io::Seek};

use dbsdk_rs::{audio::{self, AudioSample}, db, io::{FileMode, FileStream}};
use qoaudio::{QoaDecoder, QoaItem};

// NOTE: currently hardcoded to assume music tracks are stereo 44100 Hz

const AUDIO_BUFFER_SIZE: usize = 1280;
const AUDIO_SAMPLERATE: usize = 44100;
const AUDIO_LOOKAHEAD_TIME: f64 = 0.1;

// technically sounds will be buffered up to (AUDIO_LOOKAHEAD_TIME * 2) seconds in advance
// at a lookahead of 0.1s, w/ a buffer size of 1280 samples @ 44100 Hz,
// this is enough time to contain just shy of 7 buffers worth of audio (0.1 / (1280.0/44100.0)) * 2 = 6.890625
// so we round up and keep refs to the previous 7 buffers of audio to prevent them from being deallocated before they play
const AUDIO_NUM_BUFFERS: usize = 7;

pub struct MusicPlayer {
    decoder: Cell<Option<QoaDecoder<FileStream>>>,
    audio_buf: [[Option<AudioSample>;AUDIO_NUM_BUFFERS];2],
    audio_queue: [Option<Vec<i16>>;2],
    audio_schedule_time: f64,
    next_buf: usize,
    playing: bool,
    looping: bool,
}

impl MusicPlayer {
    pub fn new(path: &str, looping: bool) -> Result<MusicPlayer, ()> {
        let music_track = match FileStream::open(path, FileMode::Read) {
            Ok(v) => v,
            Err(_) => {
                return Err(());
            }
        };

        let music_decoder = match QoaDecoder::new(music_track) {
            Ok(v) => v,
            Err(_) => {
                return Err(());
            }
        };

        Ok(MusicPlayer {
            decoder: Cell::new(Some(music_decoder)),
            audio_buf: [[const {None};AUDIO_NUM_BUFFERS], [const {None};AUDIO_NUM_BUFFERS]],
            audio_queue: [const {None};2],
            audio_schedule_time: -1.0,
            next_buf: 0,
            playing: true,
            looping,
        })
    }

    fn schedule_voice(handle: i32, slot: i32, pan: f32, t: f64) {
        audio::queue_set_voice_param_i(slot, audio::AudioVoiceParam::SampleData, handle, t);
        audio::queue_set_voice_param_i(slot, audio::AudioVoiceParam::Samplerate, AUDIO_SAMPLERATE as i32, t);
        audio::queue_set_voice_param_i(slot, audio::AudioVoiceParam::LoopEnabled, 0, t);
        audio::queue_set_voice_param_i(slot, audio::AudioVoiceParam::Reverb, 0, t);
        audio::queue_set_voice_param_f(slot, audio::AudioVoiceParam::Volume, 1.0, t);
        audio::queue_set_voice_param_f(slot, audio::AudioVoiceParam::Pitch, 1.0, t);
        audio::queue_set_voice_param_f(slot, audio::AudioVoiceParam::Detune, 0.0, t);
        audio::queue_set_voice_param_f(slot, audio::AudioVoiceParam::Pan, pan, t);
        audio::queue_set_voice_param_f(slot, audio::AudioVoiceParam::FadeInDuration, 0.0, t);
        audio::queue_set_voice_param_f(slot, audio::AudioVoiceParam::FadeOutDuration, 0.0, t);

        audio::queue_stop_voice(slot, t);
        audio::queue_start_voice(slot, t);
    }

    fn process_audio(&mut self) {
        let t = self.audio_schedule_time + AUDIO_LOOKAHEAD_TIME;
        let maybe_dec = self.decoder.get_mut();

        // we need to "unzip" interleaved LR audio into two mono buffers
        let mut data_l = vec![0;AUDIO_BUFFER_SIZE];
        let mut data_r = vec![0;AUDIO_BUFFER_SIZE];

        if let Some(dec) = maybe_dec {
            // decode audio
            let mut out_idx_l = 0;
            let mut out_idx_r = 0;

            let mut sel = false;

            while out_idx_l < AUDIO_BUFFER_SIZE || out_idx_r < AUDIO_BUFFER_SIZE {
                match dec.next() {
                    Some(Ok(QoaItem::Sample(v))) => {
                        if sel {
                            data_r[out_idx_r] = v;
                            out_idx_r += 1;   
                        }
                        else {
                            data_l[out_idx_l] = v;
                            out_idx_l += 1;
                        }
    
                        sel = !sel;
                    }
                    None => {
                        self.playing = false;
                        return;
                    }
                    _ => {
                    }
                }
            }
        }
        else {
            return;
        }

        // we have a rotating buffer of audio samples we use to upload audio data
        // NOTE: this will automatically deallocate the previous buffers here

        // this is a little tricky:
        // basically, instead of queueing audio chunks right away, we actually stuff them into a buffer and wait
        // then, when we get the next buffer, we actually take its first sample and append it to the start of the LAST buffer and submit that
        // this is all to make DreamBox's 2-tap sampling play nicely - b/c at the end of one of our submitted samples, DreamBox doesn't take the next sample we queue up into account,
        // so there's a single sample of aliasing in between every single buffer we submit and it ends up sounding scratchy
        // this fixes that by basically making each buffer end with the next buffer's starting sample

        match &mut self.audio_queue[0] {
            Some(v1) => {
                // had a previous buffer, append the first sample of this new buffer to the end and queue that
                v1.push(data_l[0]);
                let newbuf_l = AudioSample::create_s16(v1, AUDIO_SAMPLERATE as i32).expect("Failed creating audio sample");
                let handle_l = newbuf_l.handle;
                self.audio_buf[0][self.next_buf % AUDIO_NUM_BUFFERS] = Some(newbuf_l);
                Self::schedule_voice(handle_l, 0, -1.0, t);
            }
            None => {
            }
        }

        match &mut self.audio_queue[1] {
            Some(v2) => {
                // had a previous buffer, append the first sample of this new buffer to the end and queue that
                v2.push(data_r[0]);
                let newbuf_r = AudioSample::create_s16(v2, AUDIO_SAMPLERATE as i32).expect("Failed creating audio sample");
                let handle_r = newbuf_r.handle;
                self.audio_buf[1][self.next_buf % AUDIO_NUM_BUFFERS] = Some(newbuf_r);
                Self::schedule_voice(handle_r, 1, 1.0, t);
            }
            None => {
            }
        }

        // replace audio in the queue with new chunk
        self.audio_queue[0] = Some(data_l);
        self.audio_queue[1] = Some(data_r);

        self.next_buf += 1;
    }

    pub fn update(&mut self) {
        // goofy as heck tbh
        if !self.playing && self.looping {
            let dec = self.decoder.replace(None).unwrap();
            let mut dec_file = dec.into_inner();
            dec_file.seek(std::io::SeekFrom::Start(0)).unwrap();
            let dec = QoaDecoder::new(dec_file).unwrap();
            self.decoder.replace(Some(dec));

            self.playing = true;
        }

        if self.audio_schedule_time < audio::get_time() {
            db::log(format!("Audio schedule time fell behind real time, recovering...").as_str());
            self.audio_schedule_time = audio::get_time();
        }

        if audio::get_time() >= self.audio_schedule_time - AUDIO_LOOKAHEAD_TIME {
            self.process_audio();
            self.audio_schedule_time += AUDIO_BUFFER_SIZE as f64 / AUDIO_SAMPLERATE as f64;
        }
    }
}