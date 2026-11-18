//! Trời + map là mesh tĩnh. Mọi thứ động (địch, súng, mưa, tia) là một batch.
//! Macroquad không cull mặt sau, có depth test và alpha blend. Mưa / vệt nắng
//! dùng alpha của vertex — không shader riêng, không bloom, không shadow map.

use macroquad::camera::Camera;
use macroquad::prelude::*;

use crate::enemy::{Ai, Enemy, Flock};
use crate::level::{Level, CELL};
use crate::weapon::Weapon;

/// Culling gameplay. Trời nằm xa hơn, camera dùng z_far riêng.
const SEE: f32 = 40.0;
const UV_WHITE: [f32; 2] = [0.97, 0.04];
const RAIN_MAX: usize = 40;

const CANS: [[u8; 4]; 6] = [
    [186, 84, 56, 255],
    [64, 112, 150, 255],
    [72, 108, 78, 255],
    [168, 132, 84, 255],
    [108, 112, 118, 255],
    [150, 72, 64, 255],
];

pub struct Batch {
    vertices: Vec<Vertex>,
    indices: Vec<u16>,
    texture: Texture2D,
}

impl Batch {
    pub fn new(texture: Texture2D) -> Self {
        Self {
            vertices: Vec::with_capacity(2048),
            indices: Vec::with_capacity(3072),
            texture,
        }
    }

    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
    }

    pub fn draw(&mut self) {
        if self.indices.is_empty() {
            return;
        }
        let vertices = std::mem::take(&mut self.vertices);
        let indices = std::mem::take(&mut self.indices);
        let mesh = Mesh {
            vertices,
            indices,
            texture: Some(self.texture.clone()),
        };
        draw_mesh(&mesh);
        self.vertices = mesh.vertices;
        self.indices = mesh.indices;
    }

    fn push_face(&mut self, pts: &[[f32; 3]; 4], uvs: [[f32; 2]; 4], color: [u8; 4]) {
        if self.vertices.len() + 4 >= 60_000 {
            return;
        }
        let base = self.vertices.len() as u16;
        for i in 0..4 {
            self.vertices.push(Vertex {
                position: vec3(pts[i][0], pts[i][1], pts[i][2]),
                uv: vec2(uvs[i][0], uvs[i][1]),
                color,
                normal: Vec4::ZERO,
            });
        }
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn push_face_vc(&mut self, pts: &[[f32; 3]; 4], colors: [[u8; 4]; 4]) {
        if self.vertices.len() + 4 >= 60_000 {
            return;
        }
        let base = self.vertices.len() as u16;
        for i in 0..4 {
            self.vertices.push(Vertex {
                position: vec3(pts[i][0], pts[i][1], pts[i][2]),
                uv: vec2(UV_WHITE[0], UV_WHITE[1]),
                color: colors[i],
                normal: Vec4::ZERO,
            });
        }
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn push_tri(&mut self, pts: [[f32; 3]; 3], color: [u8; 4]) {
        if self.vertices.len() + 3 >= 60_000 {
            return;
        }
        let base = self.vertices.len() as u16;
        for p in pts {
            self.vertices.push(Vertex {
                position: vec3(p[0], p[1], p[2]),
                uv: vec2(UV_WHITE[0], UV_WHITE[1]),
                color,
                normal: Vec4::ZERO,
            });
        }
        self.indices.extend_from_slice(&[base, base + 1, base + 2]);
    }

    pub fn push_box(&mut self, min: Vec3, max: Vec3, color: [u8; 4]) {
        self.box_faces(min, max, color, false);
    }

    pub fn push_box_rib(&mut self, min: Vec3, max: Vec3, color: [u8; 4]) {
        self.box_faces(min, max, color, true);
    }

    fn box_faces(&mut self, min: Vec3, max: Vec3, color: [u8; 4], rib: bool) {
        let (x0, y0, z0) = (min.x, min.y, min.z);
        let (x1, y1, z1) = (max.x, max.y, max.z);
        let faces: [[[f32; 3]; 4]; 6] = [
            [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
            [[x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]],
            [[x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0]],
            [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
            [[x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]],
            [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]],
        ];
        let uvs = if rib {
            [[0.02, 0.08], [0.46, 0.08], [0.46, 0.96], [0.02, 0.96]]
        } else {
            [UV_WHITE, UV_WHITE, UV_WHITE, UV_WHITE]
        };
        for face in faces {
            self.push_face(&face, uvs, color);
        }
    }

    /// Hộp theo 3 cạnh tuỳ hướng. `ax/ay/az` là cạnh đầy đủ, không phải half-extent.
    pub fn push_prism(&mut self, origin: Vec3, ax: Vec3, ay: Vec3, az: Vec3, color: [u8; 4]) {
        let p = |ix: f32, iy: f32, iz: f32| {
            let v = origin + ax * ix + ay * iy + az * iz;
            [v.x, v.y, v.z]
        };
        let uvs = [UV_WHITE, UV_WHITE, UV_WHITE, UV_WHITE];
        let faces = [
            [
                p(0.0, 0.0, 1.0),
                p(1.0, 0.0, 1.0),
                p(1.0, 1.0, 1.0),
                p(0.0, 1.0, 1.0),
            ],
            [
                p(1.0, 0.0, 0.0),
                p(0.0, 0.0, 0.0),
                p(0.0, 1.0, 0.0),
                p(1.0, 1.0, 0.0),
            ],
            [
                p(0.0, 1.0, 1.0),
                p(1.0, 1.0, 1.0),
                p(1.0, 1.0, 0.0),
                p(0.0, 1.0, 0.0),
            ],
            [
                p(0.0, 0.0, 0.0),
                p(1.0, 0.0, 0.0),
                p(1.0, 0.0, 1.0),
                p(0.0, 0.0, 1.0),
            ],
            [
                p(1.0, 0.0, 1.0),
                p(1.0, 0.0, 0.0),
                p(1.0, 1.0, 0.0),
                p(1.0, 1.0, 1.0),
            ],
            [
                p(0.0, 0.0, 0.0),
                p(0.0, 0.0, 1.0),
                p(0.0, 1.0, 1.0),
                p(0.0, 1.0, 0.0),
            ],
        ];
        for face in faces {
            self.push_face(&face, uvs, color);
        }
    }

    pub fn push_ground_quad(&mut self, x: f32, z: f32, half: f32, y: f32, color: [u8; 4]) {
        let x0 = x - half;
        let x1 = x + half;
        let z0 = z - half;
        let z1 = z + half;
        let pts = [[x0, y, z0], [x0, y, z1], [x1, y, z1], [x1, y, z0]];
        let uvs = [
            asphalt_uv(x0, z0),
            asphalt_uv(x0, z1),
            asphalt_uv(x1, z1),
            asphalt_uv(x1, z0),
        ];
        self.push_face(&pts, uvs, color);
    }

    pub fn push_decal(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, color: [u8; 4]) {
        let pts = [
            [a.x, a.y, a.z],
            [b.x, b.y, b.z],
            [c.x, c.y, c.z],
            [d.x, d.y, d.z],
        ];
        self.push_face(&pts, [UV_WHITE; 4], color);
    }

    pub fn push_beam(&mut self, eye: Vec3, a: Vec3, b: Vec3, width: f32, color: [u8; 4]) {
        let dir = b - a;
        let len2 = dir.length_squared();
        if len2 < 1e-6 {
            return;
        }
        let dir = dir / len2.sqrt();
        let mut side = dir.cross(eye - a);
        if side.length_squared() < 1e-6 {
            side = dir.cross(Vec3::Y);
        }
        side = side.normalize() * width;
        let pts = [
            [a.x - side.x, a.y - side.y, a.z - side.z],
            [a.x + side.x, a.y + side.y, a.z + side.z],
            [b.x + side.x, b.y + side.y, b.z + side.z],
            [b.x - side.x, b.y - side.y, b.z - side.z],
        ];
        self.push_face(&pts, [UV_WHITE; 4], color);
    }
}

pub struct Frustum {
    planes: [Vec4; 6],
}

impl Frustum {
    pub fn from_vp(m: Mat4) -> Self {
        let r0 = m.row(0);
        let r1 = m.row(1);
        let r2 = m.row(2);
        let r3 = m.row(3);
        Self {
            planes: [
                norm(r3 + r0),
                norm(r3 - r0),
                norm(r3 + r1),
                norm(r3 - r1),
                norm(r3 + r2),
                norm(r3 - r2),
            ],
        }
    }

    #[inline]
    pub fn sees(&self, center: Vec3, radius: f32) -> bool {
        let p = Vec4::new(center.x, center.y, center.z, 1.0);
        for plane in &self.planes {
            if plane.dot(p) < -radius {
                return false;
            }
        }
        true
    }

    #[inline]
    pub fn visible(&self, eye: Vec3, center: Vec3, radius: f32) -> bool {
        let max_d = SEE + radius;
        if eye.distance_squared(center) > max_d * max_d {
            return false;
        }
        self.sees(center, radius)
    }
}

#[inline]
fn norm(p: Vec4) -> Vec4 {
    let len = vec3(p.x, p.y, p.z).length();
    if len < 1e-6 {
        p
    } else {
        p / len
    }
}

fn asphalt_uv(x: f32, z: f32) -> [f32; 2] {
    [
        0.52 + (x * 0.13).fract() * 0.36,
        0.04 + (z * 0.13).fract() * 0.90,
    ]
}

/// Một texture 128²: gân container | nhựa đường | ô trắng cho màu vertex.
pub fn make_panel_texture() -> Texture2D {
    const N: u32 = 128;
    let mut image = Image::gen_image_color(N as u16, N as u16, WHITE);
    for y in 0..N {
        for x in 0..N {
            let h = hash(x.wrapping_mul(374761393) ^ y.wrapping_mul(668265263));
            let (r, g, b) = if x >= 120 {
                (255, 255, 255)
            } else if x >= 64 {
                let speck = if h > 0.86 { 0.22 } else { 0.0 };
                let v = (0.58 + 0.28 * h + speck).clamp(0.0, 1.0);
                ((v * 188.0) as u8, (v * 196.0) as u8, (v * 208.0) as u8)
            } else {
                let groove = x % 5 == 0;
                let rail = y < 7 || y > 120;
                let mut v = if groove { 0.48 } else { 0.90 + 0.08 * h };
                if rail {
                    v *= 0.62;
                }
                let c = (v * 255.0) as u8;
                (c, c, c)
            };
            image.set_pixel(x, y, Color::from_rgba(r, g, b, 255));
        }
    }
    let tex = Texture2D::from_image(&image);
    tex.set_filter(FilterMode::Linear);
    tex
}

pub fn build_level_mesh(level: &Level, texture: Texture2D) -> Mesh {
    let mut batch = Batch::new(texture.clone());
    let w = level.world;
    let n = (w / CELL) as i32;
    for z in 0..n {
        for x in 0..n {
            let h = hash((x as u32).wrapping_mul(13) ^ (z as u32).wrapping_mul(29));
            let lift = if h > 0.72 { 38 } else { 0 };
            let color = [96 + lift, 102 + lift, 110 + lift, 255];
            let cx = (x as f32 + 0.5) * CELL;
            let cz = (z as f32 + 0.5) * CELL;
            batch.push_ground_quad(cx, cz, CELL * 0.5, 0.0, color);
        }
    }
    for (i, wall) in level.walls.iter().enumerate() {
        let color = CANS[i % CANS.len()];
        batch.push_box_rib(wall.min, wall.max, color);
        let border = wall.min.x < 0.01
            || wall.min.z < 0.01
            || wall.max.x > w - 0.01
            || wall.max.z > w - 0.01;
        if border {
            let top = CANS[(i + 3) % CANS.len()];
            batch.push_box_rib(
                vec3(wall.min.x, wall.max.y, wall.min.z),
                vec3(wall.max.x, wall.max.y + 2.15, wall.max.z),
                top,
            );
        }
    }
    // Đèn natri. Không chiếu sáng thật — chỉ cột + chỏm sáng + vệt dưới chân.
    const LAMPS: [(f32, f32); 4] = [(10.0, 6.0), (18.0, 6.0), (10.0, 14.0), (22.0, 26.0)];
    for (x, z) in LAMPS {
        batch.push_box(
            vec3(x - 0.05, 0.0, z - 0.05),
            vec3(x + 0.05, 3.15, z + 0.05),
            [36, 34, 32, 255],
        );
        batch.push_box(
            vec3(x - 0.20, 3.02, z - 0.20),
            vec3(x + 0.20, 3.26, z + 0.20),
            [255, 186, 74, 255],
        );
        let c = vec3(x, 0.035, z);
        batch.push_decal(
            c + vec3(-1.2, 0.0, -1.2),
            c + vec3(-1.2, 0.0, 1.2),
            c + vec3(1.2, 0.0, 1.2),
            c + vec3(1.2, 0.0, -1.2),
            [255, 160, 70, 48],
        );
    }
    // Vệt nắng chỉ trên ô trống, kẻo nó trét lên tường.
    let center = vec3(w * 0.5, 0.04, w * 0.5);
    let ang = 0.55_f32;
    let dir = vec3(ang.sin(), 0.0, ang.cos());
    let right = vec3(dir.z, 0.0, -dir.x) * 0.65;
    let along = dir * (CELL * 0.42);
    for step in -3..=4 {
        let p = center + dir * (step as f32) * CELL;
        let cx = (p.x / CELL).floor() * CELL;
        let cz = (p.z / CELL).floor() * CELL;
        if cx < 0.0 || cz < 0.0 || cx + CELL > w || cz + CELL > w {
            continue;
        }
        let blocked = level.walls.iter().any(|wall| {
            wall.overlaps(
                vec3(cx + 0.15, 0.0, cz + 0.15),
                vec3(cx + CELL - 0.15, 1.0, cz + CELL - 0.15),
            )
        });
        if blocked {
            continue;
        }
        let mid = vec3(cx + CELL * 0.5, 0.04, cz + CELL * 0.5);
        batch.push_decal(
            mid - along - right,
            mid + along - right,
            mid + along + right,
            mid - along + right,
            [255, 196, 130, 64],
        );
    }

    Mesh {
        vertices: batch.vertices,
        indices: batch.indices,
        texture: Some(texture),
    }
}

pub fn build_sky_mesh(level: &Level, texture: Texture2D) -> Mesh {
    let mut batch = Batch::new(texture.clone());
    let c = vec3(level.world * 0.5, 0.0, level.world * 0.5);
    let radius = level.world * 0.5 + 22.0;
    let segs = 12i32;
    let y = [-4.0_f32, 7.0, 34.0];
    let c0 = [210, 108, 64, 255];
    let c1 = [236, 170, 98, 255];
    let c2 = [26, 34, 56, 255];
    for i in 0..segs {
        let a0 = (i as f32) / (segs as f32) * std::f32::consts::TAU;
        let a1 = ((i + 1) as f32) / (segs as f32) * std::f32::consts::TAU;
        let p = |ang: f32, yy: f32| [c.x + ang.sin() * radius, yy, c.z + ang.cos() * radius];
        let bands = [(0usize, c0, c1), (1usize, c1, c2)];
        for (band, lo, hi) in bands {
            let pts = [
                p(a0, y[band]),
                p(a1, y[band]),
                p(a1, y[band + 1]),
                p(a0, y[band + 1]),
            ];
            batch.push_face_vc(&pts, [lo, lo, hi, hi]);
        }
        let top0 = p(a0, y[2]);
        let top1 = p(a1, y[2]);
        batch.push_tri([[c.x, y[2], c.z], top1, top0], c2);
    }
    let ang = 0.55_f32;
    let sun = c + vec3(ang.sin() * (radius - 2.5), 9.5, ang.cos() * (radius - 2.5));
    let right = vec3(ang.cos(), 0.0, -ang.sin());
    let halo = right * 7.0;
    let up = vec3(0.0, 5.5, 0.0);
    batch.push_decal(
        sun - halo - up,
        sun - halo + up,
        sun + halo + up,
        sun + halo - up,
        [255, 150, 70, 80],
    );
    let disc = right * 2.4;
    let disc_up = vec3(0.0, 2.2, 0.0);
    batch.push_decal(
        sun - disc - disc_up,
        sun - disc + disc_up,
        sun + disc + disc_up,
        sun + disc - disc_up,
        [255, 228, 170, 255],
    );
    Mesh {
        vertices: batch.vertices,
        indices: batch.indices,
        texture: Some(texture),
    }
}

/// Mưa gần camera. Không particle system, không alloc sau lúc tạo.
pub struct Rain {
    p: [Vec3; RAIN_MAX],
    rng: u32,
}

impl Rain {
    pub fn new() -> Self {
        Self {
            p: [vec3(0.0, -2.0, 0.0); RAIN_MAX],
            rng: 0xA341_316C,
        }
    }

    pub fn tick(&mut self, dt: f32, eye: Vec3, look: Vec3) {
        let dt = dt.clamp(0.0, 0.05);
        let flat = vec3(look.x, 0.0, look.z);
        let fwd = if flat.length_squared() < 1e-4 {
            Vec3::Z
        } else {
            flat.normalize()
        };
        let right = vec3(fwd.z, 0.0, -fwd.x);
        let fall = vec3(2.4, -18.0, 0.5) * dt;
        for drop in &mut self.p {
            *drop += fall;
            let rel = *drop - eye;
            let ahead = rel.x * fwd.x + rel.z * fwd.z;
            let side = rel.x * right.x + rel.z * right.z;
            if drop.y < 0.05 || drop.y > 12.0 || ahead < -2.0 || ahead > 15.0 || side.abs() > 8.0 {
                *drop = spawn_drop(&mut self.rng, eye, fwd, right);
            }
        }
    }

    pub fn draw(&self, batch: &mut Batch, eye: Vec3, count: usize) {
        let n = count.min(RAIN_MAX);
        let wind = vec3(0.12, -0.62, 0.04);
        for drop in self.p.iter().take(n) {
            if eye.distance_squared(*drop) > 16.0 * 16.0 {
                continue;
            }
            batch.push_beam(eye, *drop, *drop + wind, 0.012, [198, 206, 214, 150]);
        }
    }
}

pub fn rain_budget(scale: f32) -> usize {
    if scale < 0.65 {
        16
    } else if scale < 0.75 {
        28
    } else {
        RAIN_MAX
    }
}

fn spawn_drop(rng: &mut u32, eye: Vec3, fwd: Vec3, right: Vec3) -> Vec3 {
    let a = rand01(rng);
    let b = rand01(rng);
    let c = rand01(rng);
    eye + fwd * (0.6 + a * 12.5) + right * ((b - 0.5) * 13.0) + vec3(0.0, 1.4 + c * 8.0, 0.0)
}

fn rand01(rng: &mut u32) -> f32 {
    *rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
    (*rng >> 8) as f32 / 16_777_216.0
}

pub struct Scaler {
    pub scale: f32,
    slow: u8,
    fast: u8,
}

impl Scaler {
    pub fn new() -> Self {
        Self {
            scale: 0.70,
            slow: 0,
            fast: 0,
        }
    }

    pub fn observe(&mut self, frame_dt: f32) {
        if frame_dt > 0.040 {
            self.slow = self.slow.saturating_add(1);
            self.fast = 0;
        } else if frame_dt < 0.028 {
            self.fast = self.fast.saturating_add(1);
            self.slow = 0;
        } else {
            self.slow = 0;
            self.fast = 0;
        }
        if self.slow > 20 && self.scale > 0.60 {
            self.scale = (self.scale - 0.10).max(0.60);
            self.slow = 0;
        }
        if self.fast > 45 && self.scale < 0.80 {
            self.scale = (self.scale + 0.10).min(0.80);
            self.fast = 0;
        }
    }

    pub fn size(&self, sw: f32, sh: f32) -> (u32, u32) {
        let mut w = (sw * self.scale).round() as u32;
        let mut h = (sh * self.scale).round() as u32;
        w = w.max(160);
        h = h.max(120);
        let long = w.max(h);
        if long > 960 {
            let k = 960.0 / long as f32;
            w = ((w as f32) * k) as u32;
            h = ((h as f32) * k) as u32;
        }
        ((w.max(2) & !1), (h.max(2) & !1))
    }
}

struct Slot {
    w: u32,
    h: u32,
    rt: RenderTarget,
}

/// Tối đa 3 render target (0.6 / 0.7 / 0.8). Không xoá/tạo lại mỗi frame.
pub struct RtCache {
    slots: [Option<Slot>; 3],
}

impl RtCache {
    pub fn new() -> Self {
        Self {
            slots: [None, None, None],
        }
    }

    pub fn get(&mut self, w: u32, h: u32) -> RenderTarget {
        for slot in &self.slots {
            if let Some(slot) = slot {
                if slot.w == w && slot.h == h {
                    return slot.rt.clone();
                }
            }
        }
        let rt = render_target(w, h);
        rt.texture.set_filter(FilterMode::Linear);
        let store = Slot { w, h, rt };
        let clone = store.rt.clone();
        if let Some(empty) = self.slots.iter_mut().find(|s| s.is_none()) {
            *empty = Some(store);
        } else {
            self.slots[0] = Some(store);
        }
        clone
    }
}

pub struct Scene<'a> {
    pub eye: Vec3,
    pub target: Vec3,
    pub level: &'a Level,
    pub flock: &'a Flock,
    pub weapon: &'a Weapon,
    pub alpha: f32,
    pub rt: &'a RenderTarget,
    pub level_mesh: &'a Mesh,
    pub sky_mesh: &'a Mesh,
    pub rain: &'a Rain,
    pub drops: usize,
}

pub fn render_scene(scene: Scene, batch: &mut Batch) {
    let aspect = scene.rt.texture.width() / scene.rt.texture.height();
    let cam = Camera3D {
        position: scene.eye,
        target: scene.target,
        up: Vec3::Y,
        fovy: 68.0_f32.to_radians(),
        aspect: Some(aspect),
        projection: Projection::Perspective,
        render_target: Some(scene.rt.clone()),
        viewport: None,
        z_near: 0.08,
        z_far: 120.0,
    };
    let frustum = Frustum::from_vp(cam.matrix());
    set_camera(&cam);
    clear_background(Color::from_rgba(214, 118, 72, 255));
    draw_mesh(scene.sky_mesh);
    draw_mesh(scene.level_mesh);

    let look = scene.target - scene.eye;
    let look = if look.length_squared() < 1e-8 {
        Vec3::Z
    } else {
        look.normalize()
    };
    let flat = vec3(look.x, 0.0, look.z);
    let right = if flat.length_squared() < 1e-4 {
        Vec3::X
    } else {
        let f = flat.normalize();
        vec3(f.z, 0.0, -f.x)
    };

    batch.clear();
    for (i, relay) in scene.level.relays.iter().enumerate() {
        let center = relay.pos + vec3(0.0, 0.75, 0.0);
        if !frustum.visible(scene.eye, center, 1.2) {
            continue;
        }
        if relay.cut {
            batch.push_box(
                relay.pos + vec3(-0.28, 0.15, -0.28),
                relay.pos + vec3(0.28, 1.25, 0.28),
                [52, 56, 60, 255],
            );
        } else {
            let pulse = 0.65 + 0.35 * ((get_time() as f32) * 3.2 + i as f32).sin();
            let body = (150.0 + 80.0 * pulse) as u8;
            batch.push_box(
                relay.pos + vec3(-0.28, 0.15, -0.28),
                relay.pos + vec3(0.28, 1.25, 0.28),
                [body, 120, 36, 255],
            );
            batch.push_box(
                relay.pos + vec3(-0.05, 1.25, -0.05),
                relay.pos + vec3(0.05, 2.15, 0.05),
                [255, 214, 96, 255],
            );
            let g = relay.pos + vec3(0.0, 0.04, 0.0);
            let s = 0.55 + 0.25 * pulse;
            batch.push_decal(
                g + vec3(-s, 0.0, -s),
                g + vec3(-s, 0.0, s),
                g + vec3(s, 0.0, s),
                g + vec3(s, 0.0, -s),
                [255, 170, 60, 70],
            );
        }
    }

    for pickup in &scene.level.pickups {
        if !pickup.alive {
            continue;
        }
        let bob = (get_time() as f32 * 2.2 + pickup.pos.x).sin() * 0.06;
        let base = pickup.pos + vec3(0.0, 0.35 + bob, 0.0);
        if !frustum.visible(scene.eye, base, 0.6) {
            continue;
        }
        let color = if pickup.med {
            [72, 190, 96, 255]
        } else {
            [230, 186, 64, 255]
        };
        batch.push_box(
            base - vec3(0.16, 0.16, 0.16),
            base + vec3(0.16, 0.16, 0.16),
            color,
        );
    }

    scene
        .flock
        .for_each_alive(|enemy| push_enemy(batch, enemy, scene.alpha, &frustum, scene.eye));

    push_gun(batch, scene.eye, look, right, scene.weapon.flash);

    for fx in &scene.weapon.fx {
        if fx.life <= 0.0 {
            continue;
        }
        if fx.spark {
            if frustum.visible(scene.eye, fx.a, 0.3) {
                let s = 0.16;
                batch.push_beam(
                    scene.eye,
                    fx.a - vec3(s, 0.0, 0.0),
                    fx.a + vec3(s, 0.0, 0.0),
                    0.02,
                    [255, 220, 160, 230],
                );
                batch.push_beam(
                    scene.eye,
                    fx.a - vec3(0.0, s, 0.0),
                    fx.a + vec3(0.0, s, 0.0),
                    0.02,
                    [255, 120, 48, 220],
                );
            }
        } else if frustum.visible(scene.eye, (fx.a + fx.b) * 0.5, 0.5) {
            batch.push_beam(scene.eye, fx.a, fx.b, 0.018, [255, 214, 120, 220]);
        }
    }
    if scene.weapon.flash > 0.0 {
        let muzzle = scene.eye + look * 0.72 + right * 0.16 - Vec3::Y * 0.16;
        let s = 0.07 + scene.weapon.flash;
        let r = right * s;
        let u = Vec3::Y * s;
        batch.push_decal(
            muzzle - r - u,
            muzzle + r - u,
            muzzle + r + u,
            muzzle - r + u,
            [255, 236, 190, 230],
        );
    }
    scene.rain.draw(batch, scene.eye, scene.drops);
    batch.draw();
}

fn push_gun(batch: &mut Batch, eye: Vec3, look: Vec3, right: Vec3, flash: f32) {
    let up = Vec3::Y;
    let kick = if flash > 0.0 { 0.035 } else { 0.0 };
    let origin = eye + look * (0.36 + kick) + right * 0.14 - up * 0.20;
    batch.push_prism(
        origin,
        right * 0.055,
        up * 0.065,
        look * 0.24,
        [26, 28, 30, 255],
    );
    let barrel = origin + right * 0.012 + up * 0.02 + look * 0.20;
    batch.push_prism(
        barrel,
        right * 0.028,
        up * 0.028,
        look * 0.30,
        [16, 16, 18, 255],
    );
    let mag = origin + right * 0.012 - up * 0.055 + look * 0.07;
    batch.push_prism(
        mag,
        right * 0.028,
        up * 0.06,
        look * 0.07,
        [42, 44, 48, 255],
    );
}

fn push_enemy(batch: &mut Batch, enemy: &Enemy, alpha: f32, frustum: &Frustum, eye: Vec3) {
    let pos = enemy.draw_pos(alpha);
    let bounds = enemy.bounds_at(pos);
    let center = bounds.center();
    let radius = (bounds.max - bounds.min).length() * 0.5;
    if !frustum.visible(eye, center, radius) {
        return;
    }
    let fwd = vec3(enemy.yaw.sin(), 0.0, enemy.yaw.cos());
    let right = vec3(fwd.z, 0.0, -fwd.x);
    if enemy.gull {
        batch.push_box(bounds.min, bounds.max, [210, 214, 218, 255]);
        let wing = center - right * 0.62 + vec3(0.0, 0.02, 0.0);
        batch.push_prism(
            wing,
            right * 1.24,
            vec3(0.0, 0.045, 0.0),
            fwd * 0.28,
            [150, 160, 170, 255],
        );
        batch.push_box(
            center + fwd * 0.18 - vec3(0.05, 0.05, 0.05),
            center + fwd * 0.18 + vec3(0.05, 0.05, 0.05),
            [40, 220, 170, 255],
        );
        return;
    }
    let body_top = bounds.min.y + 1.15;
    let body = if enemy.ai == Ai::Attack {
        [186, 78, 52, 255]
    } else {
        [150, 92, 62, 255]
    };
    batch.push_box(bounds.min, vec3(bounds.max.x, body_top, bounds.max.z), body);
    let head = vec3(center.x, body_top + 0.16, center.z) + fwd * 0.12;
    batch.push_box(
        head - vec3(0.16, 0.16, 0.16),
        head + vec3(0.16, 0.18, 0.16),
        [186, 176, 160, 255],
    );
    let eye_c = if enemy.ai == Ai::Attack {
        [255, 64, 36, 255]
    } else {
        [255, 196, 80, 255]
    };
    batch.push_box(
        head + fwd * 0.14 - vec3(0.05, 0.02, 0.05),
        head + fwd * 0.14 + vec3(0.05, 0.06, 0.05),
        eye_c,
    );
    batch.push_decal(
        vec3(center.x - 0.42, 0.03, center.z - 0.42),
        vec3(center.x - 0.42, 0.03, center.z + 0.42),
        vec3(center.x + 0.42, 0.03, center.z + 0.42),
        vec3(center.x + 0.42, 0.03, center.z - 0.42),
        [20, 22, 26, 160],
    );
}

pub fn blit(rt: &RenderTarget, sw: f32, sh: f32) {
    set_default_camera();
    draw_texture_ex(
        &rt.texture,
        0.0,
        0.0,
        WHITE,
        DrawTextureParams {
            dest_size: Some(vec2(sw, sh)),
            flip_y: true,
            ..Default::default()
        },
    );
}

fn hash(mut n: u32) -> f32 {
    n ^= n >> 16;
    n = n.wrapping_mul(0x7feb352d);
    n ^= n >> 15;
    (n & 0x00FF_FFFF) as f32 / 16_777_215.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frustum_keeps_forward_and_drops_back() {
        let view = Mat4::look_at_rh(Vec3::ZERO, vec3(0.0, 0.0, 1.0), Vec3::Y);
        let proj = Mat4::perspective_rh_gl(70.0_f32.to_radians(), 1.6, 0.08, 40.0);
        let fr = Frustum::from_vp(proj * view);
        assert!(
            fr.sees(vec3(0.0, 0.0, 5.0), 0.4),
            "điểm phía trước phải thấy"
        );
        assert!(fr.sees(vec3(2.0, 0.0, 10.0), 0.4), "lệch nhẹ vẫn trong fov");
        assert!(!fr.sees(vec3(0.0, 0.0, -3.0), 0.4), "phía sau phải bị loại");
        assert!(!fr.sees(vec3(0.0, 0.0, 55.0), 0.4), "quá far phải bị loại");
        assert!(
            !fr.sees(vec3(20.0, 0.0, 5.0), 0.4),
            "lệch ngang quá phải bị loại"
        );
    }

    #[test]
    fn rain_wraps_back_near_the_camera() {
        let mut rain = Rain::new();
        let eye = vec3(8.0, 1.6, 6.0);
        let look = vec3(0.0, 0.0, 1.0);
        for _ in 0..30 {
            rain.tick(1.0 / 30.0, eye, look);
        }
        for drop in &rain.p {
            let dx = drop.x - eye.x;
            let dz = drop.z - eye.z;
            assert!(
                dx * dx + dz * dz < 20.0 * 20.0,
                "hat mua phai o gan camera, khong bay ra vo cuc"
            );
            assert!(drop.y > -0.5 && drop.y < 14.0);
        }
    }

    #[test]
    fn rain_budget_shrinks_when_the_frame_is_heavy() {
        assert_eq!(rain_budget(0.80), 40);
        assert_eq!(rain_budget(0.70), 28);
        assert_eq!(rain_budget(0.60), 16);
    }
}
