//! Synth PCM 16-bit ngay trong RAM rồi đưa cho quad-snd (feature `audio` của macroquad).
//! Không dùng cpal/rodio: cả hai kéo backend AAudio riêng, dễ đụng NativeActivity
//! và phình binary. quad-snd trộn vài voice ngắn là đủ tiếng súng / bước chân.

use macroquad::audio::{load_sound_from_bytes, play_sound, PlaySoundParams, Sound};

const RATE: u32 = 22_050;

pub struct Audio {
    shot: Option<Sound>,
    step: Option<Sound>,
    hit: Option<Sound>,
    hurt: Option<Sound>,
    dry: Option<Sound>,
    master: f32,
}

impl Audio {
    pub async fn load() -> Self {
        let shot = synth(0.09, |t, rng| {
            let n = noise(rng);
            let sq = if (t * 160.0) % 1.0 < 0.5 { 0.35 } else { -0.35 };
            (n * 0.75 + sq) * (-t * 28.0).exp()
        });
        let step = synth(0.05, |t, rng| noise(rng) * (-t * 55.0).exp() * 0.7);
        let hit = synth(0.07, |t, _| {
            let f = 880.0 - t * 2400.0;
            (t * f * std::f32::consts::TAU).sin() * (-t * 18.0).exp() * 0.6
        });
        let hurt = synth(0.11, |t, _| {
            let sq = if (t * 90.0) % 1.0 < 0.5 { 0.5 } else { -0.5 };
            sq * (-t * 14.0).exp()
        });
        let dry = synth(0.03, |t, _| {
            let sq = if (t * 1400.0) % 1.0 < 0.5 { 0.3 } else { -0.3 };
            sq * (-t * 90.0).exp()
        });
        Self {
            shot: load_one(&shot).await,
            step: load_one(&step).await,
            hit: load_one(&hit).await,
            hurt: load_one(&hurt).await,
            dry: load_one(&dry).await,
            master: 0.45,
        }
    }

    pub fn play_shot(&self) {
        voice(&self.shot, self.master);
    }
    pub fn play_step(&self) {
        voice(&self.step, self.master * 0.55);
    }
    pub fn play_hit(&self) {
        voice(&self.hit, self.master * 0.8);
    }
    pub fn play_hurt(&self) {
        voice(&self.hurt, self.master);
    }
    pub fn play_dry(&self) {
        voice(&self.dry, self.master * 0.7);
    }
    pub fn play_pickup(&self) {
        voice(&self.hit, self.master * 0.65);
    }
}

async fn load_one(wav: &[u8]) -> Option<Sound> {
    load_sound_from_bytes(wav).await.ok()
}

fn voice(sound: &Option<Sound>, volume: f32) {
    if let Some(sound) = sound {
        play_sound(
            sound,
            PlaySoundParams {
                looped: false,
                volume,
            },
        );
    }
}

fn synth(dur: f32, wave: impl Fn(f32, &mut u32) -> f32) -> Vec<u8> {
    let n = (dur * RATE as f32) as usize;
    let mut samples = Vec::with_capacity(n);
    let mut rng = 0xA341_316Cu32;
    for i in 0..n {
        let t = i as f32 / RATE as f32;
        samples.push(wave(t, &mut rng).clamp(-1.0, 1.0));
    }
    wav_pcm16(&samples, RATE)
}

fn noise(rng: &mut u32) -> f32 {
    *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
    (*rng >> 8) as f32 / 8_388_608.0 - 1.0
}

fn wav_pcm16(samples: &[f32], rate: u32) -> Vec<u8> {
    let data_bytes = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_bytes as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_bytes.to_le_bytes());
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}
