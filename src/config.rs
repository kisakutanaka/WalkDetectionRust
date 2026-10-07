// config.json の読み書き。移植元: ofApp::saveConfig, ofApp::loadConfig

use crate::detector::Params;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

pub struct Settings {
    pub params: Params,
    pub dist_ip: String,
    pub dist_port: i32,
    pub listen_port: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            params: Params::default(),
            dist_ip: "localhost".into(),
            dist_port: 10002,
            listen_port: 10001,
        }
    }
}

// 検出パラメータ1つ分の、config.json のキーと GUI の設定(ラベル / 刻み / 最小 / 最大)
pub struct ParamField<'a> {
    pub key: &'static str,
    pub label: &'static str,
    pub speed: f32,
    pub min: f32,
    pub max: f32,
    pub value: &'a mut f32,
}

pub fn param_fields(p: &mut Params) -> [ParamField<'_>; 11] {
    let f = |key, label, speed, min, max, value| ParamField { key, label, speed, min, max, value };
    [
        f("lifeTime", "Life Time [s]", 0.1, 0.1, 10.0, &mut p.life_time),
        f("matchWindow", "Match Window [s]", 0.01, 0.03, 1.0, &mut p.match_window),
        f("matchMaxDist", "Match Max Dist [m]", 0.01, 0.1, 3.0, &mut p.match_max_dist),
        f("landingSpeed", "Landing Speed [m/s]", 0.01, 0.05, 5.0, &mut p.landing_speed),
        f("directionWindow", "Direction Window [s]", 0.01, 0.05, 1.0, &mut p.direction_window),
        f("stampLifeTime", "Stamp Life Time [s]", 0.1, 0.1, 10.0, &mut p.stamp_life_time),
        f("prevStampWindow", "Prev Stamp Window [s]", 0.1, 0.1, 10.0, &mut p.prev_stamp_window),
        f("prevStampMinDist", "Prev Stamp Min Dist [m]", 0.01, 0.0, 2.0, &mut p.prev_stamp_min_dist),
        f("prevStampMaxDist", "Prev Stamp Max Dist [m]", 0.01, 0.0, 3.0, &mut p.prev_stamp_max_dist),
        f("blockerRadius", "Blocker Radius [m]", 0.01, 0.01, 1.0, &mut p.blocker_radius),
        f("blockerLifeTime", "Blocker Life Time [s]", 0.01, 0.0, 1.0, &mut p.blocker_life_time),
    ]
}

// 実行ファイルの隣の data/config.json。なければカレントの data/config.json
pub fn path() -> PathBuf {
    let beside_exe = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join("data").join("config.json")));
    match beside_exe {
        Some(p) if p.exists() => p,
        _ => PathBuf::from("data").join("config.json"),
    }
}

impl Settings {
    // ファイルがなければ何もしない。キーがない項目は今の値のまま
    pub fn load(&mut self, path: &Path) {
        let Ok(text) = std::fs::read_to_string(path) else { return };
        let json: Value = match serde_json::from_str(&text) {
            Ok(json) => json,
            Err(e) => return eprintln!("{}: {e}", path.display()),
        };
        for f in param_fields(&mut self.params) {
            if let Some(v) = json[f.key].as_f64() {
                *f.value = v as f32;
            }
        }
        if let Some(v) = json["distIp"].as_str() {
            self.dist_ip = v.into();
        }
        if let Some(v) = json["distPort"].as_i64() {
            self.dist_port = v as i32;
        }
        if let Some(v) = json["listenPort"].as_i64() {
            self.listen_port = v as i32;
        }
    }

    // oF 版(nlohmann)と同じ形式: キーはアルファベット順、インデント4、float は f64 にした値
    pub fn save(&mut self, path: &Path) {
        let mut json = Map::new();
        for f in param_fields(&mut self.params) {
            json.insert(f.key.into(), (*f.value as f64).into());
        }
        json.insert("distIp".into(), self.dist_ip.clone().into());
        json.insert("distPort".into(), self.dist_port.into());
        json.insert("listenPort".into(), self.listen_port.into());

        let mut out = Vec::new();
        let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
        let mut ser = serde_json::Serializer::with_formatter(&mut out, formatter);
        serde::Serialize::serialize(&json, &mut ser).unwrap();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(path, out) {
            eprintln!("{}: {e}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 読んで書き戻すと、oF 版が書いた data/config.json と同じになる
    #[test]
    fn roundtrip_matches_of() {
        let mut s = Settings::default();
        s.load(Path::new("data/config.json"));
        let out = std::env::temp_dir().join("walk_detection_rust_config_test.json");
        s.save(&out);
        assert_eq!(std::fs::read_to_string(&out).unwrap(), std::fs::read_to_string("data/config.json").unwrap());
    }
}
