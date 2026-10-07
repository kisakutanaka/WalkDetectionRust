// 足跡検出のロジック。描画やソケットを持たないので、TSV を流してテストできる。
// 移植元: reference/of_src/ofApp.cpp(update, findNearestInWindow, findOldestLinkedFoot)

use std::collections::BTreeMap;
use std::ops::{Add, Mul, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Vec2 { x, y }
    }

    pub fn length2(self) -> f32 {
        self.x * self.x + self.y * self.y
    }

    pub fn length(self) -> f32 {
        self.length2().sqrt()
    }

    // glm::normalize と同じく、逆数を掛ける
    pub fn normalize(self) -> Self {
        self * (1.0 / self.length())
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }
}

impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }
}

impl Mul<f32> for Vec2 {
    type Output = Vec2;
    fn mul(self, s: f32) -> Vec2 {
        Vec2::new(self.x * s, self.y * s)
    }
}

//検出パラメータ
#[derive(Clone, Debug)]
pub struct Params {
    pub life_time: f32,           //足の寿命[s]
    pub match_window: f32,        //連続とみなす時間[s]
    pub match_max_dist: f32,      //連続とみなす距離[m]
    pub landing_speed: f32,       //着地とみなす速度[m/s]
    pub direction_window: f32,    //足跡の向きを決定するために参照する過去の足の時間[s]
    pub stamp_life_time: f32,     //足跡スタンプの寿命[s]
    pub prev_stamp_window: f32,   //直前の足跡を探す時間[s]
    pub prev_stamp_min_dist: f32, //直前の足跡とみなす最小距離[m]
    pub prev_stamp_max_dist: f32, //直前の足跡とみなす最大距離[m]
    pub blocker_radius: f32,      //足跡設置禁止領域の半径[m]
    pub blocker_life_time: f32,   //足跡設置禁止領域の寿命[s]
}

impl Default for Params {
    fn default() -> Self {
        Params {
            life_time: 1.0,
            match_window: 0.1,
            match_max_dist: 1.0,
            landing_speed: 0.8,
            direction_window: 0.5,
            stamp_life_time: 2.0,
            prev_stamp_window: 2.0,
            prev_stamp_min_dist: 0.25,
            prev_stamp_max_dist: 2.0,
            blocker_radius: 0.2,
            blocker_life_time: 0.15,
        }
    }
}

// 履歴の中の住所 (タイムスタンプ, 配列インデックス)
pub type Address = (u64, usize);

#[derive(Clone, Copy, Debug)]
pub struct Foot {
    pub pos: Vec2,
    pub vel: Vec2,
    pub time_stamp: u64,
    pub prev_foot_address: Option<Address>, //連続とみなすひとつ前の足の住所
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Side {
    Left = -1,
    Undefined = 0,
    Right = 1,
}

impl Side {
    // OSC で送る値。未確定のときは LEFT として送る
    pub fn osc_value(self) -> i32 {
        if self == Side::Undefined { Side::Left as i32 } else { self as i32 }
    }

    fn opposite(self) -> Side {
        match self {
            Side::Left => Side::Right,
            Side::Undefined => Side::Undefined,
            Side::Right => Side::Left,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FootStamp {
    pub pos: Vec2,
    pub dir: Vec2,
    pub prev_stamp_address: Option<Address>, //連続とみなすひとつ前の足跡の住所
    pub side: Side,
}

//足跡設置禁止領域
#[derive(Clone, Copy, Debug)]
pub struct Blocker {
    pub pos: Vec2,
    pub last_seen: u64,
}

trait HasPos {
    fn pos(&self) -> Vec2;
}

impl HasPos for Foot {
    fn pos(&self) -> Vec2 {
        self.pos
    }
}

impl HasPos for FootStamp {
    fn pos(&self) -> Vec2 {
        self.pos
    }
}

// C++ の「u64 の差 > float*1e9」の比較(double で比べる)
fn older_than(dt_ns: u64, seconds: f32) -> bool {
    dt_ns as f64 > seconds as f64 * 1e9
}

// history の中から、now より前の window[s] 以内で、pos から minDist〜maxDist にある最も近い要素の住所を返す
fn find_nearest_in_window<T: HasPos>(
    history: &BTreeMap<u64, Vec<T>>,
    pos: Vec2,
    now: u64,
    window: f32,
    min_dist: f32,
    max_dist: f32,
) -> Option<Address> {
    let mut min_dist2 = f32::MAX;
    let mut nearest = None;
    for (&t, items) in history.range(..now).rev() {
        if older_than(now - t, window) {
            break;
        }
        for (i, item) in items.iter().enumerate() {
            let dist2 = (pos - item.pos()).length2();
            if dist2 < min_dist2 && dist2 < max_dist * max_dist && dist2 > min_dist * min_dist {
                min_dist2 = dist2;
                nearest = Some((t, i));
            }
        }
    }
    nearest
}

// 最新キーから life_time[s] より古いフレームを、古い順に削除する
fn remove_old<T>(history: &mut BTreeMap<u64, Vec<T>>, life_time: f32) {
    let Some(&now) = history.keys().next_back() else { return };
    while let Some(entry) = history.first_entry() {
        if !older_than(now - *entry.key(), life_time) {
            break;
        }
        entry.remove();
    }
}

#[derive(Default)]
pub struct Detector {
    pub foot_history: BTreeMap<u64, Vec<Foot>>,
    pub foot_stamp_history: BTreeMap<u64, Vec<FootStamp>>,
    pub blockers: Vec<Blocker>,
}

impl Detector {
    // /hokuyohub/cluster 1件分の処理。足跡を置いたらそれを返す(呼び出し側で OSC 送信する)
    pub fn process(&mut self, p: &Params, time_stamp: u64, pos: Vec2) -> Option<FootStamp> {
        //ループしたら配列をクリアする(blockers は残す)
        if self.foot_history.keys().next_back().is_some_and(|&last| last > time_stamp) {
            self.foot_history.clear();
            self.foot_stamp_history.clear();
        }

        let mut f = Foot {
            pos,
            vel: Vec2::ZERO,
            time_stamp,
            //連続とみなすひとつ前の足の住所を探す
            prev_foot_address: find_nearest_in_window(&self.foot_history, pos, time_stamp, p.match_window, 0.0, p.match_max_dist),
        };

        //連続する足が見つかった場合は速度を計算し、遅ければ着地とみなす
        let mut stamp = None;
        if f.prev_foot_address.is_some() {
            let base = self.find_oldest_linked_foot(&f, p.match_window);
            let dt = time_stamp.wrapping_sub(base.time_stamp);
            if dt > 0 {
                // oF の vec2 / double は割り算ではなく逆数を掛ける(glm/gtx/scalar_multiplication.hpp)。丸めを合わせる
                f.vel = (f.pos - base.pos) * (1.0 / (dt as f64 * 1e-9) as f32);
            }
            if f.vel.length2() < p.landing_speed * p.landing_speed {
                stamp = self.place_stamp(p, &f);
            }
        }

        self.foot_history.entry(time_stamp).or_default().push(f);
        stamp
    }

    // 着地した足 f に、blocker の範囲外なら足跡スタンプを設置する
    fn place_stamp(&mut self, p: &Params, f: &Foot) -> Option<FootStamp> {
        let r2 = p.blocker_radius * p.blocker_radius;
        if self.blockers.iter().any(|b| (f.pos - b.pos).length2() < r2) {
            return None;
        }

        //向きを設定するために過去の足の履歴を参照する
        let d = f.pos - self.find_oldest_linked_foot(f, p.direction_window).pos;
        let dir = if d.length2() as f64 > 0.0001 {
            d.normalize()
        } else {
            Vec2::new(0.0, 1.0) //向きが決まっていない場合は上向きにする
        };

        let mut fs = FootStamp {
            pos: f.pos,
            dir,
            prev_stamp_address: find_nearest_in_window(
                &self.foot_stamp_history,
                f.pos,
                f.time_stamp,
                p.prev_stamp_window,
                p.prev_stamp_min_dist,
                p.prev_stamp_max_dist,
            ),
            side: Side::Undefined,
        };

        if let Some((t, i)) = fs.prev_stamp_address {
            let pred = self.foot_stamp_history[&t][i];
            let h = fs.dir + pred.dir;
            if h.length2() > 1.0 {
                let to_pred = pred.pos - fs.pos; // 今の足跡 → 直前の足跡
                let lat = (h.x * to_pred.y - h.y * to_pred.x) / h.length(); // 符号付きの横ずれ[m]
                let abs_lat = lat.abs() as f64;
                fs.side = if (0.1..=0.4).contains(&abs_lat) {
                    if lat < 0.0 { Side::Left } else { Side::Right }
                } else {
                    pred.side.opposite()
                };
            }
        }

        self.foot_stamp_history.entry(f.time_stamp).or_default().push(fs);
        self.blockers.push(Blocker { pos: fs.pos, last_seen: f.time_stamp });
        Some(fs)
    }

    // start から prev_foot_address をたどり、window[s] 以内で最も古い足を返す
    fn find_oldest_linked_foot(&self, start: &Foot, window: f32) -> Foot {
        let mut cur = *start;
        while let Some((t, i)) = cur.prev_foot_address {
            let Some(feet) = self.foot_history.get(&t) else { break };
            if older_than(start.time_stamp.wrapping_sub(t), window) {
                break;
            }
            cur = feet[i];
        }
        cur
    }

    // 届いたメッセージを全部処理した後に、1回だけ呼ぶ
    pub fn end_update(&mut self, p: &Params) {
        //blockerの範囲内に最新フレームの足があれば lastSeen を更新し、古いものを削除する
        if let Some((&now, feet)) = self.foot_history.last_key_value() {
            let r2 = p.blocker_radius * p.blocker_radius;
            self.blockers.retain_mut(|b| {
                if feet.iter().any(|f| (f.pos - b.pos).length2() < r2) {
                    b.last_seen = now;
                }
                // ループ直後は last_seen > now になり、符号なしの引き算で巨大な値になって消える
                !older_than(now.wrapping_sub(b.last_seen), p.blocker_life_time)
            });
        }

        remove_old(&mut self.foot_history, p.life_time);
        remove_old(&mut self.foot_stamp_history, p.stamp_life_time);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // osc_capture.py の TSV を、受信時刻 1/60 秒ごとに update 1回分として流す
    fn replay(path: &str) -> Vec<FootStamp> {
        let text = std::fs::read_to_string(path).unwrap();
        let p = Params::default();
        let mut det = Detector::default();
        let mut stamps = Vec::new();
        let mut t0 = None;
        let mut frame = 0i64;
        for line in text.lines() {
            let cols: Vec<&str> = line.split('\t').collect();
            if cols[2] != "/hokuyohub/cluster" {
                continue;
            }
            let recv: f64 = cols[0].parse().unwrap();
            let t0 = *t0.get_or_insert(recv);
            let f = ((recv - t0) * 60.0).floor() as i64;
            while frame < f {
                det.end_update(&p);
                frame += 1;
            }
            let ts: u64 = cols[5].parse().unwrap();
            let pos = Vec2::new(cols[7].parse().unwrap(), cols[8].parse().unwrap());
            stamps.extend(det.process(&p, ts, pos));
        }
        det.end_update(&p);
        stamps
    }

    // oF 版の出力(/footstamp を osc_capture.py で録ったもの)と、ビット単位で一致する
    fn assert_same_as_of(stamps: &[FootStamp], expected_path: &str) {
        let expected = std::fs::read_to_string(expected_path).unwrap();
        let expected: Vec<&str> = expected.lines().collect();
        assert_eq!(stamps.len(), expected.len());
        for (s, e) in stamps.iter().zip(expected) {
            let c: Vec<f64> = e.split('\t').map(|v| v.parse().unwrap()).collect();
            let actual = [s.pos.x, s.pos.y, s.dir.x, s.dir.y];
            assert!(actual.iter().zip(&c).all(|(a, e)| *a == *e as f32), "{actual:?} != {e}");
            assert_eq!(s.side.osc_value(), c[4] as i32, "{e}");
        }
    }

    #[test]
    fn capture1() {
        assert_same_as_of(&replay("testdata/capture1_20s.tsv"), "testdata/capture1_20s_expected.tsv");
    }

    #[test]
    fn capture2() {
        assert_same_as_of(&replay("testdata/capture2_40s.tsv"), "testdata/capture2_40s_expected.tsv");
    }
}
