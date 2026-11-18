//! Súng hitscan. Pool hiệu ứng là mảng cố định, ghi đè vòng (không alloc).

use macroquad::prelude::*;

use crate::level::{closest_wall, ray_aabb, Aabb};

const MAG: u8 = 24;
const RANGE: f32 = 40.0;
pub const FX_CAP: usize = 24;

#[derive(Clone, Copy)]
pub struct Fx {
    pub a: Vec3,
    pub b: Vec3,
    pub life: f32,
    pub spark: bool,
}

impl Fx {
    const fn blank() -> Self {
        Self {
            a: Vec3::ZERO,
            b: Vec3::ZERO,
            life: 0.0,
            spark: false,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Target {
    pub id: usize,
    pub aabb: Aabb,
}

#[derive(Clone, Copy)]
pub struct Hit {
    pub point: Vec3,
    pub enemy: Option<usize>,
    /// Mét dọc theo tia. Giữ để HUD/debug đọc; vòng chơi dùng `point`.
    #[allow(dead_code)]
    pub t: f32,
    pub wall: bool,
}

pub struct Weapon {
    pub mag: u8,
    pub reserve: u16,
    pub cool: f32,
    pub reload: f32,
    pub fx: [Fx; FX_CAP],
    pub flash: f32,
    pub dry: bool,
    cursor: usize,
}

impl Weapon {
    pub fn new() -> Self {
        Self {
            mag: MAG,
            reserve: 72,
            cool: 0.0,
            reload: 0.0,
            fx: [Fx::blank(); FX_CAP],
            flash: 0.0,
            dry: false,
            cursor: 0,
        }
    }

    pub fn tick(&mut self, dt: f32) {
        if self.cool > 0.0 {
            self.cool -= dt;
        }
        if self.flash > 0.0 {
            self.flash = (self.flash - dt).max(0.0);
        }
        for fx in &mut self.fx {
            if fx.life > 0.0 {
                fx.life = (fx.life - dt).max(0.0);
            }
        }
        if self.reload > 0.0 {
            self.reload -= dt;
            if self.reload <= 0.0 {
                self.reload = 0.0;
                let need = MAG.saturating_sub(self.mag);
                let take = (need as u16).min(self.reserve) as u8;
                self.mag += take;
                self.reserve -= take as u16;
            }
        }
    }

    pub fn queue_reload(&mut self) {
        if self.reload > 0.0 || self.mag >= MAG || self.reserve == 0 {
            return;
        }
        self.reload = 1.25;
    }

    pub fn spawn_beam(&mut self, a: Vec3, b: Vec3) {
        self.push_fx(a, b, 0.07, false);
    }

    pub fn spawn_spark(&mut self, p: Vec3) {
        self.push_fx(p, p, 0.12, true);
    }

    fn push_fx(&mut self, a: Vec3, b: Vec3, life: f32, spark: bool) {
        self.fx[self.cursor] = Fx { a, b, life, spark };
        self.cursor = (self.cursor + 1) % FX_CAP;
    }

    /// Bắn một phát nếu hết hồi chiêu và còn đạn. None = không bắn.
    pub fn fire(
        &mut self,
        eye: Vec3,
        dir: Vec3,
        walls: &[Aabb],
        targets: &[Target],
    ) -> Option<Hit> {
        if self.reload > 0.0 || self.cool > 0.0 {
            return None;
        }
        if self.mag == 0 {
            self.queue_reload();
            if self.reserve == 0 {
                self.cool = 0.28;
                self.dry = true;
            }
            return None;
        }
        let Some(dir) = normalize(dir) else {
            return None;
        };
        self.mag -= 1;
        self.cool = 0.18;
        self.flash = 0.10;
        let hit = trace(eye, dir, walls, targets);
        let muzzle = eye + dir * 0.35 + vec3(0.0, -0.12, 0.0);
        self.spawn_beam(muzzle, hit.point);
        if hit.enemy.is_some() || hit.wall {
            self.spawn_spark(hit.point);
        }
        if self.mag == 0 {
            self.queue_reload();
        }
        Some(hit)
    }
}

pub fn trace(eye: Vec3, dir: Vec3, walls: &[Aabb], targets: &[Target]) -> Hit {
    let dir = normalize(dir).unwrap_or(vec3(0.0, 0.0, 1.0));
    let wall_t = closest_wall(walls, eye, dir, RANGE);
    let mut best = wall_t.unwrap_or(RANGE);
    let mut enemy = None;
    for target in targets {
        if let Some(t) = ray_aabb(eye, dir, target.aabb) {
            if t >= 0.02 && t < best {
                best = t;
                enemy = Some(target.id);
            }
        }
    }
    let wall = enemy.is_none() && wall_t.is_some() && best < RANGE - 0.05;
    Hit {
        point: eye + dir * best,
        enemy,
        t: best,
        wall,
    }
}

#[inline]
fn normalize(v: Vec3) -> Option<Vec3> {
    let len2 = v.length_squared();
    if len2 < 1e-8 {
        None
    } else {
        Some(v / len2.sqrt())
    }
}
