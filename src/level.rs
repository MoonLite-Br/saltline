//! Map arena sinh từ chuỗi trong code. Không đọc file ảnh/model.
//! Tường là AABB trục thẳng. Mesh tĩnh dựng ở `renderer`.

use arrayvec::ArrayVec;
use macroquad::prelude::*;

pub const CELL: f32 = 4.0;
pub const WALL_H: f32 = 2.6;
const MAP: &str = "\
########\n\
#S.E.HG#\n\
#.##.R.#\n\
#E.A.#.#\n\
#.R.#H.#\n\
##.#.E.#\n\
#GA.R..#\n\
########\n\
";

#[derive(Clone, Copy)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    #[inline]
    pub fn overlaps(&self, bmin: Vec3, bmax: Vec3) -> bool {
        self.min.x < bmax.x
            && self.max.x > bmin.x
            && self.min.y < bmax.y
            && self.max.y > bmin.y
            && self.min.z < bmax.z
            && self.max.z > bmin.z
    }

    #[inline]
    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }
}

#[derive(Clone, Copy)]
pub struct Relay {
    pub pos: Vec3,
    pub cut: bool,
}

#[derive(Clone, Copy)]
pub struct Marker {
    pub pos: Vec3,
    pub gull: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Loot {
    Med,
    Ammo,
}

#[derive(Clone, Copy)]
pub struct Pickup {
    pub pos: Vec3,
    pub med: bool,
    pub alive: bool,
    pub cool: f32,
}

pub struct Level {
    pub walls: ArrayVec<Aabb, 64>,
    pub relays: ArrayVec<Relay, 4>,
    pub markers: ArrayVec<Marker, 12>,
    pub pickups: ArrayVec<Pickup, 8>,
    pub spawn: Vec3,
    pub spawn_yaw: f32,
    pub world: f32,
}

impl Level {
    pub fn from_map() -> Self {
        let mut walls = ArrayVec::new();
        let mut relays = ArrayVec::new();
        let mut markers = ArrayVec::new();
        let mut pickups = ArrayVec::new();
        let mut spawn = vec3(CELL * 1.5, 0.0, CELL * 1.5);
        let mut cols = 0usize;

        for (z, line) in MAP.lines().filter(|l| !l.is_empty()).enumerate() {
            cols = cols.max(line.chars().count());
            for (x, ch) in line.chars().enumerate() {
                let center = vec3((x as f32 + 0.5) * CELL, 0.0, (z as f32 + 0.5) * CELL);
                match ch {
                    '#' => {
                        let _ = walls.try_push(Aabb {
                            min: vec3(x as f32 * CELL, 0.0, z as f32 * CELL),
                            max: vec3((x as f32 + 1.0) * CELL, WALL_H, (z as f32 + 1.0) * CELL),
                        });
                    }
                    'S' => spawn = center,
                    'R' => {
                        let _ = relays.try_push(Relay {
                            pos: center,
                            cut: false,
                        });
                    }
                    'E' => {
                        let _ = markers.try_push(Marker {
                            pos: center,
                            gull: false,
                        });
                    }
                    'G' => {
                        let _ = markers.try_push(Marker {
                            pos: center,
                            gull: true,
                        });
                    }
                    'H' => {
                        let _ = pickups.try_push(Pickup {
                            pos: center,
                            med: true,
                            alive: true,
                            cool: 0.0,
                        });
                    }
                    'A' => {
                        let _ = pickups.try_push(Pickup {
                            pos: center,
                            med: false,
                            alive: true,
                            cool: 0.0,
                        });
                    }
                    _ => {}
                }
            }
        }

        Self {
            walls,
            relays,
            markers,
            pickups,
            spawn,
            spawn_yaw: 0.0,
            world: cols as f32 * CELL,
        }
    }

    pub fn reset(&mut self) {
        for relay in &mut self.relays {
            relay.cut = false;
        }
        for pickup in &mut self.pickups {
            pickup.alive = true;
            pickup.cool = 0.0;
        }
    }

    pub fn tick_pickups(&mut self, dt: f32) {
        for pickup in &mut self.pickups {
            if pickup.alive {
                continue;
            }
            pickup.cool -= dt;
            if pickup.cool <= 0.0 {
                pickup.alive = true;
                pickup.cool = 0.0;
            }
        }
    }

    /// Nhặt một hộp nếu đứng đủ gần và còn chỗ (máu < 100 hoặc đạn dự trữ < 96).
    pub fn take(&mut self, feet: Vec3, hp: f32, reserve: u16) -> Option<Loot> {
        for pickup in &mut self.pickups {
            if !pickup.alive {
                continue;
            }
            let d = vec2(pickup.pos.x - feet.x, pickup.pos.z - feet.z).length();
            if d > 1.05 {
                continue;
            }
            if pickup.med && hp >= 100.0 {
                continue;
            }
            if !pickup.med && reserve >= 96 {
                continue;
            }
            pickup.alive = false;
            pickup.cool = 18.0;
            return Some(if pickup.med { Loot::Med } else { Loot::Ammo });
        }
        None
    }

    pub fn relays_cut(&self) -> u32 {
        self.relays.iter().filter(|r| r.cut).count() as u32
    }

    pub fn all_cut(&self) -> bool {
        !self.relays.is_empty() && self.relays.iter().all(|r| r.cut)
    }
}

/// `dir` phải đã chuẩn hoá. `t` trả về là mét.
#[inline]
pub fn ray_aabb(origin: Vec3, dir: Vec3, aabb: Aabb) -> Option<f32> {
    let mut t_min = 0.0_f32;
    let mut t_max = f32::MAX;
    for axis in 0..3 {
        let o = origin[axis];
        let d = dir[axis];
        let mn = aabb.min[axis];
        let mx = aabb.max[axis];
        if d.abs() < 1e-8 {
            if o < mn || o > mx {
                return None;
            }
        } else {
            let inv = 1.0 / d;
            let mut t0 = (mn - o) * inv;
            let mut t1 = (mx - o) * inv;
            if t0 > t1 {
                std::mem::swap(&mut t0, &mut t1);
            }
            t_min = t_min.max(t0);
            t_max = t_max.min(t1);
            if t_max < t_min {
                return None;
            }
        }
    }
    if t_max < 0.0 {
        return None;
    }
    let t = if t_min >= 0.0 { t_min } else { t_max };
    if t >= 0.0 {
        Some(t)
    } else {
        None
    }
}

#[inline]
pub fn closest_wall(walls: &[Aabb], origin: Vec3, dir: Vec3, max_t: f32) -> Option<f32> {
    let mut best = max_t;
    let mut hit = false;
    for wall in walls {
        if let Some(t) = ray_aabb(origin, dir, *wall) {
            if t >= 0.0 && t < best {
                best = t;
                hit = true;
            }
        }
    }
    if hit {
        Some(best)
    } else {
        None
    }
}

/// Tia có bị tường chặn trước `to` hay không.
#[inline]
pub fn line_clear(walls: &[Aabb], from: Vec3, to: Vec3) -> bool {
    let delta = to - from;
    let dist = delta.length();
    if dist < 0.05 {
        return true;
    }
    let dir = delta / dist;
    match closest_wall(walls, from, dir, dist) {
        Some(t) => t > dist - 0.3,
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_hits_box_in_front() {
        let box_ = Aabb {
            min: vec3(-1.0, -1.0, -1.0),
            max: vec3(1.0, 1.0, 1.0),
        };
        let t = ray_aabb(vec3(0.0, 0.0, -5.0), vec3(0.0, 0.0, 1.0), box_).unwrap();
        assert!((t - 4.0).abs() < 1e-3);
    }

    #[test]
    fn ray_misses_box_to_the_side() {
        let box_ = Aabb {
            min: vec3(-1.0, -1.0, -1.0),
            max: vec3(1.0, 1.0, 1.0),
        };
        assert!(ray_aabb(vec3(5.0, 0.0, -5.0), vec3(0.0, 0.0, 1.0), box_).is_none());
    }

    #[test]
    fn map_is_playable() {
        let level = Level::from_map();
        assert_eq!(level.relays.len(), 3);
        assert!(level.markers.len() >= 4);
        assert!(level.pickups.iter().any(|p| p.med));
        assert!(level.pickups.iter().any(|p| !p.med));
        let spawn_box = Aabb {
            min: level.spawn - vec3(0.3, 0.0, 0.3),
            max: level.spawn + vec3(0.3, 1.7, 0.3),
        };
        for wall in &level.walls {
            assert!(
                !wall.overlaps(spawn_box.min, spawn_box.max),
                "điểm spawn nằm trong tường"
            );
        }
    }

    #[test]
    fn medkit_heals_only_when_hurt_and_close() {
        let mut level = Level::from_map();
        let med = level.pickups.iter().find(|p| p.med).unwrap().pos;
        assert!(level.take(med + vec3(8.0, 0.0, 0.0), 40.0, 10).is_none());
        assert_eq!(level.take(med, 40.0, 10), Some(Loot::Med));
        assert!(level.take(med, 40.0, 10).is_none(), "hop da bien mat");
        let ammo = level.pickups.iter().find(|p| !p.med).unwrap().pos;
        assert!(
            level.take(ammo, 100.0, 96).is_none(),
            "day dan thi khong nhat"
        );
        assert_eq!(level.take(ammo, 100.0, 10), Some(Loot::Ammo));
    }
}
