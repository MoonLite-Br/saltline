//! Người chơi: yaw/pitch, trượt theo tường AABB, nhảy.
//! A = trái, D = phải khi nhìn theo `look`. Có test khoá hướng này.

use macroquad::prelude::*;
use smallvec::SmallVec;

use crate::level::Aabb;

pub const EYE: f32 = 1.55;
pub const RADIUS: f32 = 0.32;
const HEIGHT: f32 = 1.70;
const SPEED: f32 = 5.0;
const JUMP: f32 = 7.4;
const GRAVITY: f32 = 24.0;

pub struct Player {
    pub pos: Vec3,
    pub prev: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub hp: f32,
    pub on_ground: bool,
    step_acc: f32,
}

impl Player {
    pub fn at_spawn(pos: Vec3, yaw: f32) -> Self {
        Self {
            pos,
            prev: pos,
            vel: Vec3::ZERO,
            yaw,
            pitch: 0.0,
            hp: 100.0,
            on_ground: true,
            step_acc: 0.0,
        }
    }

    /// `dx`/`dy` là pixel. Chuột/ngón sang phải làm tăng yaw (quay phải).
    pub fn apply_look(&mut self, dx: f32, dy: f32, sens: f32) {
        self.yaw += dx * sens;
        self.pitch = (self.pitch - dy * sens).clamp(-1.15, 1.15);
    }

    #[inline]
    pub fn look_dir(&self) -> Vec3 {
        look_dir(self.yaw, self.pitch)
    }

    #[inline]
    pub fn eye(&self, alpha: f32) -> Vec3 {
        let feet = self.prev.lerp(self.pos, alpha);
        feet + vec3(0.0, EYE, 0.0)
    }

    /// Một tick vật lý. Trả về true khi cần tiếng bước chân.
    pub fn simulate(
        &mut self,
        move_x: f32,
        move_y: f32,
        jump: bool,
        dt: f32,
        walls: &[Aabb],
        world: f32,
    ) -> bool {
        self.prev = self.pos;
        let wish = wish_dir(self.yaw, move_x, move_y);
        let accel = if self.on_ground { 30.0 } else { 8.0 };
        self.vel.x = approach(self.vel.x, wish.x * SPEED, accel * dt);
        self.vel.z = approach(self.vel.z, wish.z * SPEED, accel * dt);
        if move_x == 0.0 && move_y == 0.0 && self.on_ground {
            self.vel.x = approach(self.vel.x, 0.0, 18.0 * dt);
            self.vel.z = approach(self.vel.z, 0.0, 18.0 * dt);
        }
        if jump && self.on_ground {
            self.vel.y = JUMP;
            self.on_ground = false;
        }
        self.vel.y -= GRAVITY * dt;

        let before = vec2(self.pos.x, self.pos.z);
        self.pos.x += self.vel.x * dt;
        self.solve_axis(0, walls);
        self.pos.z += self.vel.z * dt;
        self.solve_axis(2, walls);
        // X lần hai: đẩy Z có thể ấn người vào tường bên.
        self.solve_axis(0, walls);
        self.pos.y += self.vel.y * dt;
        if self.pos.y <= 0.0 {
            self.pos.y = 0.0;
            if self.vel.y < 0.0 {
                self.vel.y = 0.0;
            }
            self.on_ground = true;
        } else {
            self.on_ground = false;
        }
        self.solve_axis(1, walls);

        let margin = RADIUS + 0.02;
        self.pos.x = self.pos.x.clamp(margin, world - margin);
        self.pos.z = self.pos.z.clamp(margin, world - margin);

        let walked = (vec2(self.pos.x, self.pos.z) - before).length();
        if self.on_ground && walked > 0.0 {
            self.step_acc += walked;
            if self.step_acc > 1.65 {
                self.step_acc = 0.0;
                return true;
            }
        }
        false
    }

    fn body(&self) -> (Vec3, Vec3) {
        (
            vec3(self.pos.x - RADIUS, self.pos.y, self.pos.z - RADIUS),
            vec3(
                self.pos.x + RADIUS,
                self.pos.y + HEIGHT,
                self.pos.z + RADIUS,
            ),
        )
    }

    fn solve_axis(&mut self, axis: usize, walls: &[Aabb]) {
        for _ in 0..3 {
            if !self.solve_once(axis, walls) {
                break;
            }
        }
    }

    fn solve_once(&mut self, axis: usize, walls: &[Aabb]) -> bool {
        let (bmin, bmax) = self.body();
        let mut hits: SmallVec<[usize; 8]> = SmallVec::new();
        for (i, wall) in walls.iter().enumerate() {
            if wall.overlaps(bmin, bmax) && hits.len() < 8 {
                hits.push(i);
            }
        }
        let Some(&index) = hits.first() else {
            return false;
        };
        let wall = walls[index];
        match axis {
            0 => {
                if self.pos.x < (wall.min.x + wall.max.x) * 0.5 {
                    self.pos.x = wall.min.x - RADIUS - 0.001;
                } else {
                    self.pos.x = wall.max.x + RADIUS + 0.001;
                }
                self.vel.x = 0.0;
            }
            2 => {
                if self.pos.z < (wall.min.z + wall.max.z) * 0.5 {
                    self.pos.z = wall.min.z - RADIUS - 0.001;
                } else {
                    self.pos.z = wall.max.z + RADIUS + 0.001;
                }
                self.vel.z = 0.0;
            }
            _ => {
                if self.vel.y > 0.0 {
                    self.pos.y = wall.min.y - HEIGHT - 0.001;
                } else {
                    self.pos.y = wall.max.y + 0.001;
                    self.on_ground = true;
                }
                self.vel.y = 0.0;
            }
        }
        true
    }
}

#[inline]
pub fn look_dir(yaw: f32, pitch: f32) -> Vec3 {
    let cp = pitch.cos();
    vec3(yaw.sin() * cp, pitch.sin(), yaw.cos() * cp)
}

#[inline]
pub fn basis(yaw: f32) -> (Vec3, Vec3) {
    // yaw = 0 nhìn +Z. yaw tăng thì nhìn ngả về +X (bên phải).
    let forward = vec3(yaw.sin(), 0.0, yaw.cos());
    let right = vec3(yaw.cos(), 0.0, -yaw.sin());
    (forward, right)
}

#[inline]
pub fn wish_dir(yaw: f32, move_x: f32, move_y: f32) -> Vec3 {
    let (forward, right) = basis(yaw);
    let wish = right * move_x + forward * move_y;
    let len2 = wish.length_squared();
    if len2 > 1.0 {
        wish / len2.sqrt()
    } else {
        wish
    }
}

#[inline]
fn approach(cur: f32, target: f32, delta: f32) -> f32 {
    if cur < target {
        (cur + delta).min(target)
    } else {
        (cur - delta).max(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strafe_and_look_are_not_inverted() {
        let (fwd, right) = basis(0.0);
        assert!(fwd.z > 0.9 && fwd.x.abs() < 0.01, "yaw 0 phải nhìn +Z");
        assert!(right.x > 0.9, "bên phải thế giới là +X");

        let left = wish_dir(0.0, -1.0, 0.0);
        assert!(left.x < -0.9, "phím A phải đi sang trái (-X)");
        let right_move = wish_dir(0.0, 1.0, 0.0);
        assert!(right_move.x > 0.9, "phím D phải đi sang phải (+X)");
        let forward = wish_dir(0.0, 0.0, 1.0);
        assert!(forward.z > 0.9, "phím W phải đi về phía đang nhìn");

        let mut player = Player::at_spawn(Vec3::ZERO, 0.0);
        player.apply_look(10.0, 0.0, 0.01);
        assert!(player.yaw > 0.0, "chuột sang phải phải tăng yaw");
        let (turned, _) = basis(player.yaw);
        assert!(turned.x > 0.0, "yaw dương ngả về +X, tức bên phải");
    }
}
