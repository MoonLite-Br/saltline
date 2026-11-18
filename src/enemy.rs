//! Địch: pool 8 slot sống/chết. AI Idle → Chase → Attack.
//! Gull bay trên tường, Hound đi bộ và trượt AABB.

use arrayvec::ArrayVec;
use macroquad::prelude::*;

use crate::level::{line_clear, Aabb, Marker};

const SLOTS: usize = 8;
const SIGHT: f32 = 18.0;
const ATTACK: f32 = 7.5;
const LEASH: f32 = 22.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Ai {
    Idle,
    Chase,
    Attack,
}

#[derive(Clone, Copy)]
pub struct Enemy {
    pub alive: bool,
    pub gull: bool,
    pub ai: Ai,
    pub pos: Vec3,
    pub prev: Vec3,
    pub hp: f32,
    pub cool: f32,
    pub yaw: f32,
    pub home: Vec3,
    pub down: f32,
}

impl Enemy {
    fn dead() -> Self {
        Self {
            alive: false,
            gull: false,
            ai: Ai::Idle,
            pos: Vec3::ZERO,
            prev: Vec3::ZERO,
            hp: 0.0,
            cool: 0.0,
            yaw: 0.0,
            home: Vec3::ZERO,
            down: 0.0,
        }
    }

    #[inline]
    pub fn draw_pos(&self, alpha: f32) -> Vec3 {
        self.prev.lerp(self.pos, alpha)
    }

    pub fn bounds_at(&self, pos: Vec3) -> Aabb {
        if self.gull {
            Aabb {
                min: pos - vec3(0.45, 0.18, 0.45),
                max: pos + vec3(0.45, 0.18, 0.45),
            }
        } else {
            Aabb {
                min: pos - vec3(0.28, 0.0, 0.28),
                max: pos + vec3(0.28, 1.55, 0.28),
            }
        }
    }

    #[inline]
    fn eye(self) -> Vec3 {
        if self.gull {
            self.pos
        } else {
            self.pos + vec3(0.0, 1.25, 0.0)
        }
    }
}

#[derive(Clone, Copy)]
pub struct Attack {
    pub from: Vec3,
    pub to: Vec3,
    pub dmg: f32,
}

pub struct Flock {
    slots: [Enemy; SLOTS],
    rng: u32,
}

impl Flock {
    pub fn spawn(markers: &[Marker]) -> Self {
        let mut slots = [Enemy::dead(); SLOTS];
        for (i, marker) in markers.iter().take(SLOTS).enumerate() {
            let pos = if marker.gull {
                vec3(marker.pos.x, 3.15, marker.pos.z)
            } else {
                marker.pos
            };
            slots[i] = Enemy {
                alive: true,
                gull: marker.gull,
                ai: Ai::Idle,
                pos,
                prev: pos,
                hp: 42.0,
                cool: 0.4 + i as f32 * 0.15,
                yaw: 0.0,
                home: pos,
                down: 0.0,
            };
        }
        Self {
            slots,
            rng: 0x51F15EED,
        }
    }

    pub fn damage(&mut self, id: usize, amount: f32) -> bool {
        let Some(enemy) = self.slots.get_mut(id) else {
            return false;
        };
        if !enemy.alive {
            return false;
        }
        enemy.hp -= amount;
        if enemy.hp <= 0.0 {
            enemy.alive = false;
            enemy.down = 0.0;
            true
        } else {
            false
        }
    }

    pub fn targets_at(&self, alpha: f32, out: &mut ArrayVec<crate::weapon::Target, 8>) {
        out.clear();
        for (id, enemy) in self.slots.iter().enumerate() {
            if !enemy.alive {
                continue;
            }
            let _ = out.try_push(crate::weapon::Target {
                id,
                aabb: enemy.bounds_at(enemy.draw_pos(alpha)),
            });
        }
    }

    pub fn for_each_alive(&self, mut f: impl FnMut(&Enemy)) {
        for enemy in &self.slots {
            if enemy.alive {
                f(enemy);
            }
        }
    }

    pub fn update(
        &mut self,
        dt: f32,
        player_eye: Vec3,
        player_feet: Vec3,
        walls: &[Aabb],
        out: &mut ArrayVec<Attack, 8>,
    ) {
        for i in 0..SLOTS {
            if !self.slots[i].alive {
                self.slots[i].down += dt;
                let home = self.slots[i].home;
                let away = vec2(home.x - player_feet.x, home.z - player_feet.z).length();
                // Về chỗ cũ sau 14s, nhưng không hiện ngay dưới chân người chơi.
                if self.slots[i].down >= 14.0 && away > 8.0 {
                    let enemy = &mut self.slots[i];
                    enemy.alive = true;
                    enemy.pos = enemy.home;
                    enemy.prev = enemy.home;
                    enemy.hp = 42.0;
                    enemy.ai = Ai::Idle;
                    enemy.cool = 0.8;
                    enemy.down = 0.0;
                }
                continue;
            }
            let gull = self.slots[i].gull;
            let pos = self.slots[i].pos;
            let to_feet = player_feet - pos;
            let dist = vec2(to_feet.x, to_feet.z).length();
            let eye = self.slots[i].eye();
            let seen = dist < SIGHT && line_clear(walls, eye, player_eye);
            let ai = self.slots[i].ai;
            let next = match ai {
                Ai::Idle if seen => Ai::Chase,
                Ai::Chase if dist < ATTACK && seen => Ai::Attack,
                Ai::Chase if dist > LEASH || !seen => Ai::Idle,
                Ai::Attack if dist > ATTACK + 1.5 || !seen => Ai::Chase,
                other => other,
            };
            self.slots[i].ai = next;
            self.slots[i].prev = pos;
            if dist > 0.05 {
                self.slots[i].yaw = to_feet.x.atan2(to_feet.z);
            }

            if matches!(next, Ai::Chase | Ai::Attack) && dist > 0.9 {
                let speed = if gull { 4.2 } else { 3.1 };
                let scale = if next == Ai::Attack { 0.35 } else { 1.0 };
                let dir = vec3(to_feet.x / dist, 0.0, to_feet.z / dist);
                let step = dir * speed * scale * dt;
                if gull {
                    self.slots[i].pos.x += step.x;
                    self.slots[i].pos.z += step.z;
                } else {
                    let mut nxt = self.slots[i].pos;
                    nxt.x += step.x;
                    if !hound_blocked(nxt, walls) {
                        self.slots[i].pos.x = nxt.x;
                    }
                    nxt = self.slots[i].pos;
                    nxt.z += step.z;
                    if !hound_blocked(nxt, walls) {
                        self.slots[i].pos.z = nxt.z;
                    }
                }
            }

            if self.slots[i].cool > 0.0 {
                self.slots[i].cool -= dt;
            }
            if next == Ai::Attack && self.slots[i].cool <= 0.0 && out.len() < 8 {
                self.slots[i].cool = 1.05;
                let hit = next_unit(&mut self.rng) < 0.55;
                let side = vec3(-to_feet.z, 0.0, to_feet.x).normalize_or_zero();
                let to = if hit {
                    player_eye
                } else {
                    player_eye + side * 1.3 + vec3(0.0, 0.4, 0.0)
                };
                out.push(Attack {
                    from: eye,
                    to,
                    dmg: if hit { 9.0 } else { 0.0 },
                });
            }
        }
        separate_hounds(&mut self.slots);
    }
}

fn hound_blocked(feet: Vec3, walls: &[Aabb]) -> bool {
    let bmin = feet - vec3(0.28, 0.0, 0.28);
    let bmax = feet + vec3(0.28, 1.55, 0.28);
    walls.iter().any(|wall| wall.overlaps(bmin, bmax))
}

fn separate_hounds(slots: &mut [Enemy; SLOTS]) {
    for i in 0..SLOTS {
        if !slots[i].alive || slots[i].gull {
            continue;
        }
        for j in (i + 1)..SLOTS {
            if !slots[j].alive || slots[j].gull {
                continue;
            }
            let d = slots[i].pos - slots[j].pos;
            let len = vec2(d.x, d.z).length();
            if len < 0.85 && len > 0.001 {
                let push = (0.85 - len) * 0.5 / len;
                slots[i].pos.x += d.x * push;
                slots[i].pos.z += d.z * push;
                slots[j].pos.x -= d.x * push;
                slots[j].pos.z -= d.z * push;
            }
        }
    }
}

fn next_unit(rng: &mut u32) -> f32 {
    *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
    (*rng >> 8) as f32 / 16_777_216.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::Marker;

    #[test]
    fn dead_enemy_returns_only_when_the_player_is_far() {
        let mut flock = Flock::spawn(&[Marker {
            pos: vec3(4.0, 0.0, 4.0),
            gull: false,
        }]);
        flock.slots[0].alive = false;
        flock.slots[0].down = 20.0;
        let mut attacks = ArrayVec::new();
        flock.update(
            0.1,
            vec3(4.0, 1.6, 4.0),
            vec3(4.0, 0.0, 4.0),
            &[],
            &mut attacks,
        );
        assert!(!flock.slots[0].alive, "khong hoi sinh sat nguoi choi");
        flock.update(
            0.1,
            vec3(30.0, 1.6, 30.0),
            vec3(30.0, 0.0, 30.0),
            &[],
            &mut attacks,
        );
        assert!(flock.slots[0].alive);
        assert!((flock.slots[0].pos.x - 4.0).abs() < 0.01);
    }
}
