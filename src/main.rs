// リリースビルドの Windows ではコンソールを出さない
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod detector;
mod gui;

use config::Settings;
use detector::{Detector, FootStamp, Vec2};
use rosc::{OscMessage, OscPacket, OscType};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// 検出の状態と OSC の送受信。60fps のスレッドで update し、GUI からは描画と設定変更に使う
pub struct Core {
    pub settings: Settings,
    pub detector: Detector,
    receiver: Option<UdpSocket>,
    sender: UdpSocket,
    dest: Option<SocketAddr>,
}

impl Core {
    fn new() -> Self {
        let mut settings = Settings::default();
        settings.load(&config::path());
        let mut core = Core {
            settings,
            detector: Detector::default(),
            receiver: None,
            sender: UdpSocket::bind("0.0.0.0:0").expect("送信用ソケットを開けません"),
            dest: None,
        };
        core.setup_sender();
        core.setup_receiver();
        core
    }

    // 送信先を distIp:distPort に設定する。oF(oscpack)と同じく IPv4 で解決する
    pub fn setup_sender(&mut self) {
        let s = &self.settings;
        self.dest = (s.dist_ip.as_str(), s.dist_port as u16)
            .to_socket_addrs()
            .ok()
            .and_then(|mut addrs| addrs.find(|a| a.is_ipv4()));
        if self.dest.is_none() {
            eprintln!("送信先を解決できません: {}:{}", s.dist_ip, s.dist_port);
        }
    }

    // listenPort で受信を開き直す(全インターフェース)
    pub fn setup_receiver(&mut self) {
        self.receiver = None; //同じポートで開き直せるように先に閉じる
        let port = self.settings.listen_port as u16;
        match UdpSocket::bind(("0.0.0.0", port)) {
            Ok(sock) => {
                sock.set_nonblocking(true).unwrap();
                self.receiver = Some(sock);
            }
            Err(e) => eprintln!("受信ポート {port} を開けません: {e}"),
        }
    }

    // 届いている分を全部処理してから、ブロッカーと寿命を更新する
    fn update(&mut self) {
        let mut buf = [0u8; 65536];
        let mut messages = Vec::new();
        while let Some(Ok(n)) = self.receiver.as_ref().map(|r| r.recv(&mut buf)) {
            if let Ok((_, packet)) = rosc::decoder::decode_udp(&buf[..n]) {
                flatten(packet, &mut messages);
            }
        }

        for msg in messages {
            if msg.addr != "/hokuyohub/cluster" {
                continue;
            }
            let (Some(OscType::Long(ts)), Some(OscType::Float(x)), Some(OscType::Float(y))) =
                (msg.args.get(1), msg.args.get(3), msg.args.get(4))
            else {
                continue;
            };
            if let Some(fs) = self.detector.process(&self.settings.params, *ts as u64, Vec2::new(*x, *y)) {
                self.send_foot_stamp(&fs);
            }
        }

        self.detector.end_update(&self.settings.params);
    }

    // /footstamp pos.x pos.y dir.x dir.y side を送る
    fn send_foot_stamp(&self, fs: &FootStamp) {
        let Some(dest) = self.dest else { return };
        let msg = OscPacket::Message(OscMessage {
            addr: "/footstamp".into(),
            args: vec![
                OscType::Float(fs.pos.x),
                OscType::Float(fs.pos.y),
                OscType::Float(fs.dir.x),
                OscType::Float(fs.dir.y),
                OscType::Int(fs.side.osc_value()),
            ],
        });
        if let Ok(bytes) = rosc::encoder::encode(&msg) {
            let _ = self.sender.send_to(&bytes, dest);
        }
    }
}

// バンドルの中のメッセージを展開する
fn flatten(packet: OscPacket, out: &mut Vec<OscMessage>) {
    match packet {
        OscPacket::Message(m) => out.push(m),
        OscPacket::Bundle(b) => b.content.into_iter().for_each(|p| flatten(p, out)),
    }
}

fn main() -> eframe::Result {
    let core = Arc::new(Mutex::new(Core::new()));

    // oF 版と同じく 60fps で update する。ウィンドウが隠れても止まらないように描画とは別のスレッドで回す
    let worker = core.clone();
    std::thread::spawn(move || {
        let period = Duration::from_secs_f64(1.0 / 60.0);
        let mut next = Instant::now();
        loop {
            worker.lock().unwrap().update();
            next += period;
            let now = Instant::now();
            if next > now {
                std::thread::sleep(next - now);
            } else {
                next = now;
            }
        }
    });

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("WalkDetection")
            .with_inner_size([1024.0, 768.0]),
        ..Default::default()
    };
    eframe::run_native("WalkDetection", options, Box::new(|_| Ok(Box::new(gui::App::new(core)))))
}
