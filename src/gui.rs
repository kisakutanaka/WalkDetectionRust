// デバッグ描画と GUI。移植元: ofApp::draw, ofApp::drawFoot

use crate::config::{self, param_fields};
use crate::detector::{Side, Vec2};
use crate::Core;
use eframe::egui::{self, Color32, Painter, Pos2, Shape, Stroke};
use std::sync::{Arc, Mutex};

pub struct App {
    core: Arc<Mutex<Core>>,
    //入力中の文字列(Enter で確定する欄は、確定しないまま離れると元に戻す)
    ip_buf: String,
    dist_port_buf: String,
    listen_port_buf: String,
}

impl App {
    pub fn new(core: Arc<Mutex<Core>>) -> Self {
        App { core, ip_buf: String::new(), dist_port_buf: String::new(), listen_port_buf: String::new() }
    }
}

// ワールド座標(m)→ 画面座標。ofTranslate(50,50) と ofScale(100,-100) に相当
fn to_screen(p: Vec2) -> Pos2 {
    Pos2::new(50.0 + 100.0 * p.x, 50.0 - 100.0 * p.y)
}

// 長さ(m)→ 画面上の長さ
fn px(m: f32) -> f32 {
    100.0 * m
}

fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, a as u8)
}

// ofMap(now-t, 0, lifeTime*1e9, 1, 0, true)
fn fade(now: u64, t: u64, life_time: f32) -> f32 {
    (1.0 - (now - t) as f32 / (life_time * 1e9)).clamp(0.0, 1.0)
}

// 向きがわかる足を描画する。小さい円がかかとで、進行方向の後ろ側に来る
fn draw_foot(painter: &Painter, pos: Vec2, dir: Vec2, color: Color32) {
    let size = 0.2;
    let a = dir.y.atan2(dir.x) + std::f32::consts::FRAC_PI_2;
    let (s, c) = a.sin_cos();
    let local = |x: f32, y: f32| to_screen(pos + Vec2::new(x * c - y * s, x * s + y * c));
    let ellipse = (0..32)
        .map(|i| {
            let t = i as f32 / 32.0 * std::f32::consts::TAU;
            local(size * 0.25 * t.cos(), size * 0.5 * t.sin())
        })
        .collect();
    painter.add(Shape::convex_polygon(ellipse, color, Stroke::NONE));
    painter.circle_filled(local(0.0, size * 0.6), px(size * 0.2), color);
}

fn draw_world(painter: &Painter, core: &Core) {
    let det = &core.detector;
    let p = &core.settings.params;
    let line = |c: Color32| Stroke::new(1.0, c);

    painter.rect_stroke(
        egui::Rect::from_two_pos(to_screen(Vec2::ZERO), to_screen(Vec2::new(5.4, -5.4))),
        0.0,
        line(Color32::WHITE),
        egui::StrokeKind::Middle,
    );

    for b in &det.blockers {
        painter.circle_filled(to_screen(b.pos), px(p.blocker_radius), rgba(0, 0, 255, 50.0));
    }

    //足跡スタンプ(フェードには stampLifeTime ではなく lifeTime を使う。oF 版と同じ)
    if let Some(&now) = det.foot_stamp_history.keys().next_back() {
        for (&t, stamps) in &det.foot_stamp_history {
            let alpha = fade(now, t, p.life_time) * 255.0;
            for fs in stamps {
                let color = match fs.side {
                    Side::Left => rgba(0, 0, 255, alpha),
                    Side::Right => rgba(255, 0, 0, alpha),
                    Side::Undefined => rgba(127, 127, 127, alpha),
                };
                draw_foot(painter, fs.pos, fs.dir, color);
            }
        }
    }

    let Some((&now, latest)) = det.foot_history.last_key_value() else { return };
    //履歴の足
    for (&t, feet) in &det.foot_history {
        let alpha = fade(now, t, p.life_time) * 50.0;
        for f in feet {
            painter.circle_stroke(to_screen(f.pos), px(0.1), line(rgba(255, 0, 0, alpha)));
        }
    }
    //最新の足と速度
    for f in latest {
        painter.circle_stroke(to_screen(f.pos), px(0.1), line(Color32::GREEN));
        painter.line_segment([to_screen(f.pos), to_screen(f.pos + f.vel * 0.1)], line(Color32::YELLOW));
    }
    //連続とみなす足の軌跡
    for f in latest {
        let mut cur = f;
        let mut alpha = 1.0;
        while let Some((t, i)) = cur.prev_foot_address {
            let Some(feet) = det.foot_history.get(&t) else { break };
            let prev = &feet[i];
            painter.line_segment([to_screen(cur.pos), to_screen(prev.pos)], line(rgba(255, 255, 255, alpha * 255.0)));
            cur = prev;
            alpha *= 0.5;
        }
    }
}

// Enter で確定する入力欄。確定したら入力された文字列を返す。確定前に離れると current に戻る
fn enter_field(ui: &mut egui::Ui, label: &str, buf: &mut String, current: String) -> Option<String> {
    let r = ui.horizontal(|ui| {
        let r = ui.add(egui::TextEdit::singleline(buf).desired_width(160.0));
        ui.label(label);
        r
    });
    let r = r.inner;
    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        return Some(buf.clone());
    }
    if !r.has_focus() {
        *buf = current;
    }
    None
}

impl App {
    fn debug_window(&mut self, ui: &mut egui::Ui, core: &mut Core) {
        ui.label(format!("{}", 1.0 / ui.input(|i| i.stable_dt)));

        for f in param_fields(&mut core.settings.params) {
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(f.value).speed(f.speed).range(f.min..=f.max).fixed_decimals(3));
                ui.label(f.label);
            });
        }

        //送信先(Enterで反映)
        let s = &mut core.settings;
        if let Some(ip) = enter_field(ui, "Dist IP", &mut self.ip_buf, s.dist_ip.clone()) {
            s.dist_ip = ip;
            core.setup_sender();
        }
        let s = &mut core.settings;
        if let Some(port) = enter_field(ui, "Dist Port", &mut self.dist_port_buf, s.dist_port.to_string())
            && let Ok(port) = port.trim().parse()
        {
            s.dist_port = port;
            core.setup_sender();
        }

        //受信ポート(入力はすぐ反映、Resetで開き直す)
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut self.listen_port_buf).desired_width(160.0));
            if r.changed()
                && let Ok(port) = self.listen_port_buf.trim().parse()
            {
                core.settings.listen_port = port;
            }
            if !r.has_focus() {
                self.listen_port_buf = core.settings.listen_port.to_string();
            }
            ui.label("Listen Port");
            if ui.button("Reset").clicked() {
                core.setup_receiver();
            }
        });

        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                core.settings.save(&config::path());
            }
            if ui.button("Load").clicked() {
                core.settings.load(&config::path());
                core.setup_sender(); //送信先が変わっている場合に備えて再設定する
            }
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let core = self.core.clone();
        let mut core = core.lock().unwrap();

        let painter = ui.painter();
        painter.rect_filled(ui.max_rect(), 0.0, Color32::from_gray(60));
        draw_world(painter, &core);

        egui::Window::new("Debug")
            .fixed_pos([600.0, 100.0])
            .resizable(false)
            .show(ui.ctx(), |ui| self.debug_window(ui, &mut core));

        ui.ctx().request_repaint();
    }
}
