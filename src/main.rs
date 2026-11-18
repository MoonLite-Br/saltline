//! Vòng lặp: logic 30 Hz, vẽ nội suy, render target 0.6×–0.8× rồi upscale.
//! onPause: NativeActivity ngừng gọi frame. Frame đầu sau onResume có dt lớn
//! và bị bỏ, để không mô phỏng cả lúc app đang ngủ.

mod audio;
mod enemy;
mod level;
mod player;
mod renderer;
mod touch_ui;
mod weapon;

use arrayvec::ArrayVec;
use macroquad::prelude::*;

use audio::Audio;
use enemy::Flock;
use level::Level;
use player::Player;
use renderer::{Batch, RtCache, Scaler};
use touch_ui::{Pad, TouchUi};
use weapon::{Target, Weapon};

const STEP: f32 = 1.0 / 30.0;

fn window_conf() -> Conf {
    Conf {
        window_title: "Saltline".to_owned(),
        window_width: 1280,
        window_height: 720,
        // Điện thoại: không nhân thêm devicePixelRatio. Manifest vẫn fullscreen.
        high_dpi: false,
        fullscreen: false,
        sample_count: 1,
        window_resizable: true,
        ..Default::default()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Title,
    Play,
    Pause,
    Dead,
    Win,
}

struct Latch {
    mx: f32,
    my: f32,
    fire: bool,
    reload: bool,
    jump: bool,
    use_held: bool,
}

impl Latch {
    fn idle() -> Self {
        Self {
            mx: 0.0,
            my: 0.0,
            fire: false,
            reload: false,
            jump: false,
            use_held: false,
        }
    }
}

struct Game {
    player: Player,
    weapon: Weapon,
    flock: Flock,
    input: Latch,
    mode: Mode,
    resume_to: Mode,
    score: u32,
    hack: f32,
    near_relay: bool,
    flash: f32,
    mark: f32,
    time: f32,
    grace: f32,
}

impl Game {
    fn boot(level: &Level) -> Self {
        Self {
            player: Player::at_spawn(level.spawn, level.spawn_yaw),
            weapon: Weapon::new(),
            flock: Flock::spawn(&level.markers),
            input: Latch::idle(),
            mode: Mode::Play,
            resume_to: Mode::Play,
            score: 0,
            hack: 0.0,
            near_relay: false,
            flash: 0.0,
            mark: 0.0,
            time: 0.0,
            grace: 0.25,
        }
    }

    fn title(level: &Level) -> Self {
        let mut game = Self::boot(level);
        game.mode = Mode::Title;
        game
    }

    fn step(&mut self, level: &mut Level, audio: &mut Audio, dt: f32) {
        self.weapon.tick(dt);
        if self.flash > 0.0 {
            self.flash = (self.flash - dt).max(0.0);
        }
        if self.mark > 0.0 {
            self.mark = (self.mark - dt).max(0.0);
        }
        if self.grace > 0.0 {
            self.grace = (self.grace - dt).max(0.0);
        }
        self.time += dt;
        let jump = std::mem::take(&mut self.input.jump);
        let reload = std::mem::take(&mut self.input.reload);
        if reload {
            self.weapon.queue_reload();
        }
        let stepped = self.player.simulate(
            self.input.mx,
            self.input.my,
            jump,
            dt,
            &level.walls,
            level.world,
        );
        if stepped {
            audio.play_step();
        }

        let eye = self.player.pos + vec3(0.0, player::EYE, 0.0);
        let dir = self.player.look_dir();
        if self.grace <= 0.0 && self.input.fire {
            let mut targets: ArrayVec<Target, 8> = ArrayVec::new();
            self.flock.targets_at(1.0, &mut targets);
            if let Some(hit) = self.weapon.fire(eye, dir, &level.walls, &targets) {
                audio.play_shot();
                if let Some(id) = hit.enemy {
                    audio.play_hit();
                    self.mark = 0.14;
                    if self.flock.damage(id, 14.0) {
                        self.score += 100;
                    }
                }
            } else if self.weapon.dry {
                self.weapon.dry = false;
                audio.play_dry();
            }
        }

        level.tick_pickups(dt);
        match level.take(self.player.pos, self.player.hp, self.weapon.reserve) {
            Some(level::Loot::Med) => {
                self.player.hp = (self.player.hp + 40.0).min(100.0);
                audio.play_pickup();
            }
            Some(level::Loot::Ammo) => {
                self.weapon.reserve = (self.weapon.reserve + 24).min(96);
                audio.play_pickup();
            }
            None => {}
        }

        let mut attacks: ArrayVec<enemy::Attack, 8> = ArrayVec::new();
        self.flock
            .update(dt, eye, self.player.pos, &level.walls, &mut attacks);
        let mut hurt = false;
        for attack in &attacks {
            self.weapon.spawn_beam(attack.from, attack.to);
            if attack.dmg > 0.0 {
                self.player.hp -= attack.dmg;
                hurt = true;
            }
        }
        if hurt {
            self.flash = 0.18;
            audio.play_hurt();
        }

        self.near_relay = false;
        let mut nearest = None;
        let mut best = 1.7_f32;
        for (i, relay) in level.relays.iter().enumerate() {
            if relay.cut {
                continue;
            }
            let d = vec2(
                relay.pos.x - self.player.pos.x,
                relay.pos.z - self.player.pos.z,
            )
            .length();
            if d < best {
                best = d;
                nearest = Some(i);
            }
        }
        if let Some(i) = nearest {
            self.near_relay = true;
            if self.input.use_held {
                self.hack += dt;
            } else {
                self.hack = (self.hack - dt * 2.0).max(0.0);
            }
            if self.hack >= 1.15 {
                level.relays[i].cut = true;
                self.hack = 0.0;
                self.score += 150;
                audio.play_hit();
            }
        } else {
            self.hack = (self.hack - dt * 2.0).max(0.0);
        }

        if self.player.hp <= 0.0 {
            self.player.hp = 0.0;
            self.mode = Mode::Dead;
            self.resume_to = Mode::Dead;
        } else if level.all_cut() {
            self.score += self.player.hp as u32;
            self.mode = Mode::Win;
            self.resume_to = Mode::Win;
        }
    }
}

fn open_pause(game: &mut Game) {
    if game.mode == Mode::Play {
        game.resume_to = Mode::Play;
        game.mode = Mode::Pause;
    }
}

struct NumBuf {
    b: [u8; 32],
    n: usize,
}

impl NumBuf {
    fn new() -> Self {
        Self { b: [0; 32], n: 0 }
    }
    fn clear(&mut self) {
        self.n = 0;
    }
    fn push(&mut self, s: &str) {
        for &c in s.as_bytes() {
            if self.n < self.b.len() {
                self.b[self.n] = c;
                self.n += 1;
            }
        }
    }
    fn push_u(&mut self, mut v: u32) {
        if v == 0 {
            self.push("0");
            return;
        }
        let mut tmp = [0u8; 10];
        let mut i = 0;
        while v > 0 && i < 10 {
            tmp[i] = b'0' + (v % 10) as u8;
            v /= 10;
            i += 1;
        }
        while i > 0 {
            i -= 1;
            if self.n < self.b.len() {
                self.b[self.n] = tmp[i];
                self.n += 1;
            }
        }
    }
    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.b[..self.n]).unwrap_or("0")
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    prevent_quit();
    let mut audio = Audio::load().await;
    let texture = renderer::make_panel_texture();
    let mut level = Level::from_map();
    let level_mesh = renderer::build_level_mesh(&level, texture.clone());
    let sky_mesh = renderer::build_sky_mesh(&level, texture.clone());
    let mut rain = renderer::Rain::new();
    let mut batch = Batch::new(texture);
    let mut rts = RtCache::new();
    let mut scaler = Scaler::new();
    let mut touch = TouchUi::new();
    let mut game = Game::title(&level);
    let mut acc = 0.0_f32;
    let mut num = NumBuf::new();
    let mut fps = Fps::new();
    let mut quit = false;

    loop {
        if quit {
            break;
        }
        let raw = get_frame_time();
        if raw <= 0.20 {
            fps.push(raw);
        }
        let sw = screen_width();
        let sh = screen_height();
        if sw < 2.0 || sh < 2.0 {
            next_frame().await;
            continue;
        }

        if is_quit_requested() {
            match game.mode {
                Mode::Play => open_pause(&mut game),
                _ => quit = true,
            }
        }
        if is_key_pressed(KeyCode::Escape) {
            match game.mode {
                Mode::Play => open_pause(&mut game),
                Mode::Pause => game.mode = game.resume_to,
                Mode::Title => quit = true,
                Mode::Dead | Mode::Win => {}
            }
        }

        let pad = touch.poll(sw, sh);
        let playing = game.mode == Mode::Play;
        if playing {
            apply_play_input(&mut game, &pad, touch.sens);
            let sim_dt = if raw > 0.20 { 0.0 } else { raw };
            if raw <= 0.20 {
                scaler.observe(raw);
            }
            acc += sim_dt;
            let mut steps = 0;
            while acc >= STEP && steps < 3 {
                game.step(&mut level, &mut audio, STEP);
                acc -= STEP;
                steps += 1;
                if game.mode != Mode::Play {
                    acc = 0.0;
                    break;
                }
            }
            if steps == 3 && acc > STEP {
                acc = 0.0;
            }
        } else {
            acc = 0.0;
            game.input.fire = false;
            if game.mode == Mode::Pause {
                if let Some(p) = pad.held {
                    let sx = sw * 0.25;
                    let width = sw * 0.50;
                    let sy = sh * 0.58;
                    if p.x >= sx && p.x <= sx + width && (p.y - sy).abs() < sh * 0.06 {
                        let t = ((p.x - sx) / width).clamp(0.0, 1.0);
                        touch.sens = 0.4 + t * 1.8;
                    }
                }
            }
            if pad.click {
                quit = overlay_click(&mut game, &mut level, pad.cursor, sw, sh);
            }
            let confirm = is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter);
            if confirm {
                match game.mode {
                    Mode::Title | Mode::Dead | Mode::Win => {
                        level.reset();
                        game = Game::boot(&level);
                    }
                    Mode::Pause => game.mode = game.resume_to,
                    Mode::Play => {}
                }
            }
        }

        set_cursor_grab(playing && !pad.touched);
        show_mouse(!(playing && !pad.touched));

        let alpha = (acc / STEP).clamp(0.0, 1.0);
        let (rw, rh) = scaler.size(sw, sh);
        let rt = rts.get(rw, rh);
        let mut eye = game.player.eye(alpha);
        if playing && game.player.on_ground {
            let spd = vec2(game.player.vel.x, game.player.vel.z).length();
            let k = (spd / 5.0).clamp(0.0, 1.0);
            eye.y += (get_time() as f32 * 10.0).sin() * 0.028 * k;
        }
        if playing || game.mode == Mode::Title {
            rain.tick(raw.min(0.05), eye, game.player.look_dir());
        }
        renderer::render_scene(
            renderer::Scene {
                eye,
                target: eye + game.player.look_dir(),
                level: &level,
                flock: &game.flock,
                weapon: &game.weapon,
                alpha,
                rt: &rt,
                level_mesh: &level_mesh,
                sky_mesh: &sky_mesh,
                rain: &rain,
                drops: renderer::rain_budget(scaler.scale),
            },
            &mut batch,
        );
        renderer::blit(&rt, sw, sh);
        if game.mode != Mode::Title {
            draw_hud(&game, &level, touch.sens, rw, rh, sw, sh, &mut num);
        }
        if playing {
            touch.draw(sw, sh);
            if game.time < 6.0 {
                draw_text(
                    "CAT 3 RELAY   E giu de cat   hop xanh mau, hop vang dan",
                    16.0,
                    sh - 16.0,
                    18.0,
                    Color::new(1.0, 1.0, 1.0, 0.8),
                );
            }
        } else {
            draw_cards(&game, sw, sh, touch.sens, &mut num);
        }
        draw_fps(&fps, rw, rh, sw, sh, &mut num);

        next_frame().await;
    }
}

fn apply_play_input(game: &mut Game, pad: &Pad, sens: f32) {
    if !pad.touched {
        let look = mouse_delta_position();
        game.player.apply_look(look.x, look.y, 0.0026 * sens);
        game.input.mx = axis(is_key_down(KeyCode::D), is_key_down(KeyCode::A));
        game.input.my = axis(is_key_down(KeyCode::W), is_key_down(KeyCode::S));
        game.input.fire = is_mouse_button_down(MouseButton::Left);
        if is_key_pressed(KeyCode::Space) {
            game.input.jump = true;
        }
    } else {
        game.player
            .apply_look(pad.look_dx, pad.look_dy, 0.0042 * sens);
        game.input.mx = pad.move_x;
        game.input.my = pad.move_y;
        game.input.fire = pad.fire;
        if pad.jump {
            game.input.jump = true;
        }
        game.input.mx += axis(is_key_down(KeyCode::D), is_key_down(KeyCode::A));
        game.input.my += axis(is_key_down(KeyCode::W), is_key_down(KeyCode::S));
        game.input.mx = game.input.mx.clamp(-1.0, 1.0);
        game.input.my = game.input.my.clamp(-1.0, 1.0);
    }
    if is_key_pressed(KeyCode::R) || pad.reload {
        game.input.reload = true;
    }
    if is_key_down(KeyCode::E) || pad.use_held {
        game.input.use_held = true;
    } else {
        game.input.use_held = false;
    }
}

fn axis(pos: bool, neg: bool) -> f32 {
    (if pos { 1.0 } else { 0.0 }) - (if neg { 1.0 } else { 0.0 })
}

fn overlay_click(game: &mut Game, level: &mut Level, p: Vec2, sw: f32, sh: f32) -> bool {
    if game.mode == Mode::Pause {
        if hit(sw * 0.18, sh * 0.72, sw * 0.28, sh * 0.12, p) {
            game.mode = game.resume_to;
        }
        if hit(sw * 0.54, sh * 0.72, sw * 0.28, sh * 0.12, p) {
            return true;
        }
    } else if game.mode == Mode::Title {
        if hit(sw * 0.18, sh * 0.72, sw * 0.28, sh * 0.12, p) {
            level.reset();
            *game = Game::boot(level);
        }
        if hit(sw * 0.54, sh * 0.72, sw * 0.28, sh * 0.12, p) {
            return true;
        }
    } else if hit(sw * 0.34, sh * 0.62, sw * 0.32, sh * 0.12, p) {
        level.reset();
        *game = Game::boot(level);
    }
    false
}

fn hit(x: f32, y: f32, w: f32, h: f32, p: Vec2) -> bool {
    p.x >= x && p.y >= y && p.x <= x + w && p.y <= y + h
}

fn draw_hud(
    game: &Game,
    level: &Level,
    sens: f32,
    rw: u32,
    rh: u32,
    sw: f32,
    sh: f32,
    num: &mut NumBuf,
) {
    let s = (sh / 720.0).clamp(0.65, 1.4);
    let hp = (game.player.hp / 100.0).clamp(0.0, 1.0);
    draw_rectangle(
        16.0,
        16.0,
        220.0 * s,
        16.0 * s,
        Color::new(0.0, 0.0, 0.0, 0.45),
    );
    let hp_color = if hp < 0.35 {
        Color::new(0.85, 0.25, 0.22, 1.0)
    } else {
        Color::new(0.45, 0.8, 0.45, 1.0)
    };
    draw_rectangle(16.0, 16.0, 220.0 * s * hp, 16.0 * s, hp_color);

    num.clear();
    num.push("RELAY ");
    num.push_u(level.relays_cut());
    num.push("/3");
    draw_text(num.as_str(), 16.0, 52.0 * s, 22.0 * s, WHITE);

    num.clear();
    num.push_u(game.score);
    draw_text(
        num.as_str(),
        16.0,
        78.0 * s,
        22.0 * s,
        Color::new(0.95, 0.85, 0.55, 1.0),
    );

    let _ = (sens, rw, rh);

    let cx = sw * 0.5;
    let cy = sh * 0.5;
    let aim = if game.mark > 0.0 {
        Color::from_rgba(255, 214, 90, 255)
    } else {
        WHITE
    };
    draw_rectangle(cx - 7.0 * s, cy - 1.0, 14.0 * s, 2.0, aim);
    draw_rectangle(cx - 1.0, cy - 7.0 * s, 2.0, 14.0 * s, aim);
    if game.mark > 0.0 {
        let g = 10.0 * s;
        draw_rectangle(cx - 14.0 * s, cy - 1.0, 4.0 * s, 2.0, aim);
        draw_rectangle(cx + 10.0 * s, cy - 1.0, 4.0 * s, 2.0, aim);
        draw_rectangle(cx - 1.0, cy - g - 4.0 * s, 2.0, 4.0 * s, aim);
        draw_rectangle(cx - 1.0, cy + g, 2.0, 4.0 * s, aim);
    }
    draw_objective(game, level, cx, cy, s, num);

    num.clear();
    num.push_u(game.weapon.mag as u32);
    draw_text(num.as_str(), cx - 28.0 * s, sh - 36.0 * s, 36.0 * s, WHITE);
    num.clear();
    num.push_u(game.weapon.reserve as u32);
    draw_text(
        num.as_str(),
        cx + 18.0 * s,
        sh - 28.0 * s,
        20.0 * s,
        Color::new(0.8, 0.8, 0.8, 1.0),
    );
    if game.weapon.reload > 0.0 {
        draw_text(
            "NAP",
            cx - 16.0 * s,
            sh - 64.0 * s,
            18.0 * s,
            Color::new(1.0, 0.8, 0.3, 1.0),
        );
    } else if game.weapon.mag == 0 && game.weapon.reserve == 0 {
        draw_text(
            "HET DAN",
            cx - 36.0 * s,
            sh - 64.0 * s,
            18.0 * s,
            Color::from_rgba(230, 80, 60, 255),
        );
    }
    if game.near_relay || game.hack > 0.0 {
        draw_text(
            "GIU E",
            cx - 28.0 * s,
            cy + 28.0 * s,
            18.0 * s,
            Color::from_rgba(255, 214, 120, 255),
        );
        let w = 160.0 * s * (game.hack / 1.15).clamp(0.0, 1.0);
        draw_rectangle(
            cx - 80.0 * s,
            cy + 36.0 * s,
            160.0 * s,
            8.0 * s,
            Color::new(0.0, 0.0, 0.0, 0.45),
        );
        draw_rectangle(
            cx - 80.0 * s,
            cy + 36.0 * s,
            w,
            8.0 * s,
            Color::new(0.95, 0.78, 0.3, 1.0),
        );
    }
    if hp < 0.35 {
        let edge = Color::new(0.55, 0.05, 0.04, 0.28);
        let t = 18.0 * s;
        draw_rectangle(0.0, 0.0, sw, t, edge);
        draw_rectangle(0.0, sh - t, sw, t, edge);
        draw_rectangle(0.0, 0.0, t, sh, edge);
        draw_rectangle(sw - t, 0.0, t, sh, edge);
    }
    if game.flash > 0.0 {
        draw_rectangle(
            0.0,
            0.0,
            sw,
            sh,
            Color::new(0.6, 0.05, 0.05, game.flash.min(0.35)),
        );
    }
}

fn draw_objective(game: &Game, level: &Level, cx: f32, cy: f32, s: f32, num: &mut NumBuf) {
    let mut best: Option<(f32, f32, f32)> = None;
    for relay in &level.relays {
        if relay.cut {
            continue;
        }
        let dx = relay.pos.x - game.player.pos.x;
        let dz = relay.pos.z - game.player.pos.z;
        let dist = (dx * dx + dz * dz).sqrt();
        if best.is_none_or(|(_, _, d)| dist < d) {
            best = Some((dx, dz, dist));
        }
    }
    let Some((dx, dz, dist)) = best else {
        return;
    };
    if dist < 2.4 {
        return;
    }
    let rel = dx.atan2(dz) - game.player.yaw;
    let ring = 78.0 * s;
    let mx = cx + rel.sin() * ring;
    let my = cy - rel.cos() * ring;
    draw_circle(mx, my, 7.0 * s, Color::from_rgba(255, 186, 64, 230));
    num.clear();
    num.push_u(dist.round() as u32);
    num.push("m");
    draw_text(
        num.as_str(),
        mx - 10.0 * s,
        my - 12.0 * s,
        16.0 * s,
        Color::from_rgba(255, 220, 150, 255),
    );
}

fn draw_cards(game: &Game, sw: f32, sh: f32, sens: f32, num: &mut NumBuf) {
    draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.0, 0.45));
    let size = (sh * 0.06).clamp(22.0, 48.0);
    match game.mode {
        Mode::Title => {
            draw_text(
                "SALTLINE",
                sw * 0.5 - size * 2.6,
                sh * 0.32,
                size,
                Color::from_rgba(255, 214, 140, 255),
            );
            draw_text(
                "CAT 3 RELAY. DUNG DE BI HA GUC.",
                sw * 0.5 - size * 4.4,
                sh * 0.42,
                size * 0.42,
                WHITE,
            );
            card(sw * 0.18, sh * 0.72, sw * 0.28, sh * 0.12, "CHOI");
            card(sw * 0.54, sh * 0.72, sw * 0.28, sh * 0.12, "THOAT");
        }
        Mode::Pause => {
            draw_text("THOAT GAME?", sw * 0.5 - size * 3.2, sh * 0.34, size, WHITE);
            let sx = sw * 0.25;
            let sy = sh * 0.58;
            let width = sw * 0.50;
            draw_rectangle(sx, sy, width, sh * 0.012, Color::new(1.0, 1.0, 1.0, 0.25));
            let t = ((sens - 0.4) / 1.8).clamp(0.0, 1.0);
            draw_circle(sx + width * t, sy + sh * 0.006, sh * 0.025, WHITE);
            draw_text("DO NHAY", sx, sy - 10.0, 18.0, WHITE);
            card(sw * 0.18, sh * 0.72, sw * 0.28, sh * 0.12, "O LAI");
            card(sw * 0.54, sh * 0.72, sw * 0.28, sh * 0.12, "THOAT");
        }
        Mode::Dead | Mode::Win => {
            let title = if game.mode == Mode::Win {
                "DA CAT 3 RELAY"
            } else {
                "BAN DA GUC"
            };
            draw_text(title, sw * 0.5 - size * 3.6, sh * 0.32, size, WHITE);
            num.clear();
            num.push("DIEM ");
            num.push_u(game.score);
            draw_text(
                num.as_str(),
                sw * 0.5 - 70.0,
                sh * 0.44,
                28.0,
                Color::from_rgba(255, 214, 120, 255),
            );
            num.clear();
            num.push_u(game.time.round() as u32);
            num.push(" GIAY");
            draw_text(num.as_str(), sw * 0.5 - 50.0, sh * 0.50, 22.0, WHITE);
            card(sw * 0.34, sh * 0.62, sw * 0.32, sh * 0.12, "CHOI LAI");
        }
        Mode::Play => {}
    }
}

fn card(x: f32, y: f32, w: f32, h: f32, label: &str) {
    draw_rectangle(x, y, w, h, Color::new(0.15, 0.16, 0.18, 0.95));
    draw_text(label, x + 16.0, y + h * 0.65, h * 0.42, WHITE);
}

/// Trung bình 32 frame. Bỏ frame > 250ms (vừa resume) để số không tụt giả.
struct Fps {
    samples: [f32; 32],
    i: u8,
    n: u8,
}

impl Fps {
    fn new() -> Self {
        Self {
            samples: [0.0; 32],
            i: 0,
            n: 0,
        }
    }

    fn push(&mut self, dt: f32) {
        if !(0.0008..=0.25).contains(&dt) {
            return;
        }
        self.samples[self.i as usize] = dt;
        self.i = (self.i + 1) % 32;
        if self.n < 32 {
            self.n += 1;
        }
    }

    fn avg(&self) -> f32 {
        if self.n == 0 {
            return 1.0 / 30.0;
        }
        let mut sum = 0.0;
        for s in self.samples.iter().take(self.n as usize) {
            sum += *s;
        }
        sum / self.n as f32
    }
}

fn draw_fps(fps: &Fps, rw: u32, rh: u32, sw: f32, sh: f32, num: &mut NumBuf) {
    let avg = fps.avg();
    let v = if avg > 1e-4 {
        (1.0 / avg).round() as u32
    } else {
        0
    };
    let ms = (avg * 1000.0).round() as u32;
    let s = (sh / 720.0).clamp(0.65, 1.4);
    let color = if v >= 30 {
        Color::from_rgba(88, 210, 120, 255)
    } else if v >= 20 {
        Color::from_rgba(230, 190, 70, 255)
    } else {
        Color::from_rgba(220, 70, 60, 255)
    };
    let bw = 132.0 * s;
    let bh = 54.0 * s;
    let x = sw - bw - 12.0;
    let y = 12.0;
    draw_rectangle(x, y, bw, bh, Color::new(0.0, 0.0, 0.0, 0.55));
    draw_rectangle(x, y, 5.0 * s, bh, color);
    draw_text("FPS", x + 14.0 * s, y + 16.0 * s, 14.0 * s, color);
    num.clear();
    num.push_u(v);
    draw_text(num.as_str(), x + 14.0 * s, y + 40.0 * s, 26.0 * s, WHITE);
    num.clear();
    num.push_u(ms);
    num.push("ms ");
    num.push_u(rw);
    num.push("x");
    num.push_u(rh);
    draw_text(
        num.as_str(),
        x + 62.0 * s,
        y + 38.0 * s,
        14.0 * s,
        Color::new(0.85, 0.85, 0.85, 1.0),
    );
}
