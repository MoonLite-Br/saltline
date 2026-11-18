//! Joystick trái, vuốt nhìn bên phải, bốn nút. Toạ độ là tỉ lệ màn hình, không pixel cứng.

use macroquad::prelude::*;

#[derive(Clone, Copy)]
struct Btn {
    cx: f32,
    cy: f32,
    r: f32,
    label: &'static str,
    kind: Kind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Fire,
    Jump,
    Reload,
    Use,
}

const BUTTONS: [Btn; 4] = [
    Btn {
        cx: 0.90,
        cy: 0.72,
        r: 0.145,
        label: "BAN",
        kind: Kind::Fire,
    },
    Btn {
        cx: 0.75,
        cy: 0.80,
        r: 0.082,
        label: "NHAY",
        kind: Kind::Jump,
    },
    Btn {
        cx: 0.90,
        cy: 0.42,
        r: 0.072,
        label: "NAP",
        kind: Kind::Reload,
    },
    Btn {
        cx: 0.75,
        cy: 0.50,
        r: 0.072,
        label: "CAT",
        kind: Kind::Use,
    },
];

#[derive(Clone, Copy)]
pub struct Pad {
    pub move_x: f32,
    pub move_y: f32,
    pub look_dx: f32,
    pub look_dy: f32,
    pub fire: bool,
    pub reload: bool,
    pub jump: bool,
    pub use_held: bool,
    pub touched: bool,
    pub click: bool,
    pub cursor: Vec2,
    pub held: Option<Vec2>,
}

impl Pad {
    fn idle(cursor: Vec2) -> Self {
        Self {
            move_x: 0.0,
            move_y: 0.0,
            look_dx: 0.0,
            look_dy: 0.0,
            fire: false,
            reload: false,
            jump: false,
            use_held: false,
            touched: false,
            click: false,
            cursor,
            held: None,
        }
    }
}

pub struct TouchUi {
    move_id: Option<u64>,
    move_origin: Vec2,
    look_id: Option<u64>,
    look_last: Vec2,
    fire_id: Option<u64>,
    use_id: Option<u64>,
    pub sens: f32,
    knob: Vec2,
    stick_active: bool,
}

impl TouchUi {
    pub fn new() -> Self {
        Self {
            move_id: None,
            move_origin: Vec2::ZERO,
            look_id: None,
            look_last: Vec2::ZERO,
            fire_id: None,
            use_id: None,
            sens: 1.0,
            knob: Vec2::ZERO,
            stick_active: false,
        }
    }

    pub fn poll(&mut self, sw: f32, sh: f32) -> Pad {
        let points = touches();
        let mouse = Vec2::from(mouse_position());
        let mut pad = Pad::idle(mouse);
        pad.touched = !points.is_empty();
        pad.click = is_mouse_button_pressed(MouseButton::Left);

        let alive = |id: u64| {
            points.iter().any(|t| {
                t.id == id && !matches!(t.phase, TouchPhase::Ended | TouchPhase::Cancelled)
            })
        };
        if self.move_id.is_some_and(|id| !alive(id)) {
            self.move_id = None;
        }
        if self.look_id.is_some_and(|id| !alive(id)) {
            self.look_id = None;
        }
        if self.fire_id.is_some_and(|id| !alive(id)) {
            self.fire_id = None;
        }
        if self.use_id.is_some_and(|id| !alive(id)) {
            self.use_id = None;
        }

        for touch in &points {
            let down = !matches!(touch.phase, TouchPhase::Ended | TouchPhase::Cancelled);
            if matches!(touch.phase, TouchPhase::Started) {
                pad.click = true;
                pad.cursor = touch.position;
            }
            if down {
                pad.held = Some(touch.position);
                pad.cursor = touch.position;
            }
            let owned = self.move_id == Some(touch.id)
                || self.look_id == Some(touch.id)
                || self.fire_id == Some(touch.id)
                || self.use_id == Some(touch.id);
            if matches!(touch.phase, TouchPhase::Started) && !owned {
                if let Some(btn) = button_at(touch.position, sw, sh) {
                    match btn.kind {
                        Kind::Fire => self.fire_id = Some(touch.id),
                        Kind::Jump => pad.jump = true,
                        Kind::Reload => pad.reload = true,
                        Kind::Use => self.use_id = Some(touch.id),
                    }
                } else if touch.position.x < sw * 0.48 {
                    self.move_id = Some(touch.id);
                    self.move_origin = touch.position;
                } else {
                    self.look_id = Some(touch.id);
                    self.look_last = touch.position;
                }
            }
            if self.move_id == Some(touch.id) && down {
                let delta = touch.position - self.move_origin;
                let max_r = sh * 0.16;
                let mag = delta.length();
                self.knob = if mag > max_r {
                    self.move_origin + delta / mag * max_r
                } else {
                    touch.position
                };
                if mag > max_r * 0.18 && mag > 0.001 {
                    let t = ((mag.min(max_r) / max_r) - 0.18) / 0.82;
                    let n = delta / mag;
                    // Trục Y màn hình đi xuống, đẩy lên = tiến.
                    pad.move_x = n.x * t;
                    pad.move_y = -n.y * t;
                }
            }
            if self.look_id == Some(touch.id) && down {
                let delta = touch.position - self.look_last;
                self.look_last = touch.position;
                // Dead-zone nhỏ, chỉ nuốt run tay, không nuốt cú ngắm chậm.
                if delta.length_squared() >= 0.64 {
                    pad.look_dx += delta.x;
                    pad.look_dy += delta.y;
                }
            }
        }

        self.stick_active = self.move_id.is_some();
        if !self.stick_active {
            self.knob = vec2(sw * 0.18, sh * 0.72);
            self.move_origin = self.knob;
        }
        pad.fire = self.fire_id.is_some();
        pad.use_held = self.use_id.is_some();
        if !pad.touched && is_mouse_button_down(MouseButton::Left) {
            pad.held = Some(mouse);
        }
        pad
    }

    pub fn draw(&self, sw: f32, sh: f32) {
        let base = if self.stick_active {
            self.move_origin
        } else {
            vec2(sw * 0.18, sh * 0.72)
        };
        draw_circle(base.x, base.y, sh * 0.11, Color::new(1.0, 1.0, 1.0, 0.10));
        draw_circle(
            self.knob.x,
            self.knob.y,
            sh * 0.045,
            Color::new(1.0, 1.0, 1.0, 0.28),
        );
        for btn in BUTTONS {
            let c = vec2(btn.cx * sw, btn.cy * sh);
            let r = btn.r * sh;
            let hot = match btn.kind {
                Kind::Fire => self.fire_id.is_some(),
                Kind::Use => self.use_id.is_some(),
                _ => false,
            };
            let color = if hot {
                Color::new(0.95, 0.78, 0.35, 0.55)
            } else {
                Color::new(1.0, 1.0, 1.0, 0.16)
            };
            draw_circle(c.x, c.y, r, color);
            let size = (sh * 0.032).clamp(12.0, 28.0);
            draw_text(btn.label, c.x - size * 0.9, c.y + size * 0.35, size, WHITE);
        }
    }
}

fn button_at(p: Vec2, sw: f32, sh: f32) -> Option<&'static Btn> {
    BUTTONS.iter().rev().find(|btn| {
        let c = vec2(btn.cx * sw, btn.cy * sh);
        let r = btn.r * sh;
        c.distance_squared(p) <= r * r
    })
}
