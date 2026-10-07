# WalkDetectionRust 引き継ぎ

openFrameworks 製の足跡検出アプリ **WalkeDetection3** を、**まったく同じ動作のまま Rust に移植する**ための引き継ぎ資料です。
次のセッションは、このフォルダで作業を始める前提で書いています。

## 目的

- **GitHub Actions で Windows と macOS の両方をビルドできるようにすること。** oF 版は、リポジトリの外にある oF 本体とアドオン(ofxImGui)に依存していて、CI でのビルドが重い。
- 検出ロジック、パラメータ、`config.json`、OSC の入出力、デバッグ描画、GUI まで、**挙動を変えずに**移植する。改善は移植が終わってから行う。

## このフォルダの中身

| パス | 内容 |
|---|---|
| `HANDOFF.md` | この資料 |
| `src/detector.rs` | 検出ロジック(GUI・ソケットなし)と、TSV を流して oF 版の出力とビット一致を確かめるテスト |
| `src/config.rs` | `config.json` の読み書き。パラメータの表(キー、GUI のラベル、刻み、範囲)もここ |
| `src/main.rs` | OSC の送受信(`Core`)と、60fps で update するスレッド |
| `src/gui.rs` | デバッグ描画と「Debug」ウィンドウ(eframe / egui) |
| `testdata/*_expected.tsv` | oF 版が同じ入力から出した `/footstamp`(pos.x pos.y dir.x dir.y side)。`cargo test` の期待値 |
| `.github/workflows/build.yml` | macOS(arm64)と Windows のビルド、テスト、artifact の作成 |
| `reference/of_src/` | 移植元のソース(`main.cpp`, `ofApp.h`, `ofApp.cpp`)。コミット `e0aa4b8` 時点のスナップショット |
| `data/config.json` | oF 版で調整済みのパラメータ。Rust 版の初期値にもそのまま使う |
| `testdata/capture1_20s.tsv` | プレイバックデータ1を20秒キャプチャしたもの(1,988メッセージ、2人前後) |
| `testdata/capture2_40s.tsv` | プレイバックデータ2を40秒キャプチャしたもの(3,806メッセージ、3人前後、大股で速い区間あり) |
| `tools/osc_capture.py` | UDP の OSC を TSV に書き出す(依存なし)。`python3 tools/osc_capture.py <port> <秒>` |
| `tools/osc_replay.py` | TSV を元のタイミングで再送する。ポートを複数指定すると同時に送る。`python3 tools/osc_replay.py <tsv> <port> [port...]` |

移植元のリポジトリ: `git@github.com:kisakutanaka/WalkDetection3.git`(ローカル: `~/Documents/dev/of_v0.12.0_osx_release/apps/Creators2026/WalkeDetection3`)。
細かい挙動で迷ったら、`reference/of_src/ofApp.cpp` が正です。

## 推奨構成

- **GUI と描画: `eframe` / `egui`** — ImGui と同じ即時モードの GUI で、OpenGL やシステムライブラリの準備なしに Windows と macOS でビルドできる。
- **OSC: `rosc`** — エンコードとデコードだけを使い、ソケットは `std::net::UdpSocket` を non-blocking で使う。
- **設定: `serde_json`**
- 検出ロジックは GUI から切り離したモジュール(例: `src/detector.rs`)にする。描画やソケットを持たずに TSV を流してテストできるようにするため。

---

## 動作仕様

### 起動

1. ウィンドウを 1024×768 で開く(oF 版は 60fps 固定)。
2. `config.json` があれば読み込む(後述)。
3. OSC 送信先を `distIp:distPort` に設定する。
4. OSC 受信を `listenPort` で開始する(全インターフェース)。

### OSC 入力

- アドレス **`/hokuyohub/cluster`** だけを処理し、それ以外は無視する。
- バンドルで届いた場合は中のメッセージを展開する(ofxOscReceiver と同じ)。
- 型タグは `,ihiffffffi`。使うのは次の3つだけ。
  - `arg[1]`: int64 のタイムスタンプ(UNIX 時刻、ns)
  - `arg[3]`, `arg[4]`: float の重心 x, y(m)
- 残りの引数(クラスター番号、フレーム番号、バウンディングボックス、点数)は使わない。参考: arg0 はフレームをまたいだ追跡 ID ではない。arg2 のフレーム番号とタイムスタンプは1対1。

### データ構造

- `footHistory`: タイムスタンプ(u64)→ そのフレームの足の配列。キー順(古い→新しい)に並ぶ map。
- `Foot`: `pos`, `vel`(初期値0), `timeStamp`, `prevFootAddress`(連続とみなす1つ前の足の住所。住所は (タイムスタンプ, 配列インデックス)。なしは `(0,-1)`)。`area` フィールドはあるが未使用。
- `footStampHistory`: タイムスタンプ → 足跡の配列。
- `FootStamp`: `pos`, `dir`, `prevStampAddress`(直前の足跡の住所), `side`(LEFT=-1, UNDEFINED=0, RIGHT=1。初期値 UNDEFINED)。
- `blockers`: `{pos, lastSeen}` の配列(足跡を置かない領域)。

### 補助関数

**findNearestInWindow(history, pos, now, window, minDist, maxDist) → 住所**
- history を新しい順に回す。
  - キーが `>= now` のフレームは飛ばす。
  - `now - key > window*1e9` になったら打ち切る。
- 各要素について `d2 = |pos - 要素.pos|²` を計算し、`d2 < 今までの最小` かつ `d2 < maxDist²` かつ `d2 > minDist²` なら採用する(すべて厳密な不等号)。
- 見つからなければ `(0,-1)`。同じ距離なら、先に見つかったもの(新しいフレーム、小さいインデックス)が残る。

**findOldestLinkedFoot(start, window) → 足**
- `start` から `prevFootAddress` をたどる。
- 次のどれかで止まる: 住所がない、そのフレームが `footHistory` にない、`start.timeStamp - 住所のキー > window*1e9`。存在の確認は時間の判定より先に行う。
- 止まった時点の足を返す。1つもたどれなければ `start` 自身を返す。
- `start` はまだ `footHistory` に入っていない足でもよい(`prevFootAddress` だけ使う)。

### メッセージ1件ごとの処理(update の中で、届いたものを全部処理する)

1. `timeStamp = arg[1]`
2. **ループ検出**: `footHistory` が空でなく、最新キー > `timeStamp` なら、`footHistory` と `footStampHistory` をクリアする。**`blockers` はクリアしない**(後述の符号なし引き算で自然に消える)。
3. `f.pos = (arg[3], arg[4])`、`f.timeStamp = timeStamp`
4. `f.prevFootAddress = findNearestInWindow(footHistory, f.pos, timeStamp, matchWindow, 0, matchMaxDist)`
5. 住所があれば、次の処理を行う。
   1. **速度(平滑化)**: `base = findOldestLinkedFoot(f, matchWindow)`。`timeStamp - base.timeStamp > 0` なら `f.vel = (f.pos - base.pos) / (float)((timeStamp - base.timeStamp) * 1e-9)`、そうでなければ0。
   2. **着地判定**: `|f.vel|² < landingSpeed²` なら着地とみなす(速い→遅いの切り替わりは見ない。重複はブロッカーで防ぐ)。
   3. **ブロック確認**: `|f.pos - b.pos|² < blockerRadius²` のブロッカーが1つでもあれば、何もしない。
   4. ブロックされていなければ足跡を作る。
      - `fs.pos = f.pos`
      - **向き**: `d = fs.pos - findOldestLinkedFoot(f, directionWindow).pos`。`|d|² > 0.0001` なら正規化、そうでなければ `(0,1)`。
      - `fs.prevStampAddress = findNearestInWindow(footStampHistory, fs.pos, timeStamp, prevStampWindow, prevStampMinDist, prevStampMaxDist)`
      - **左右**(直前の足跡があるときだけ):
        - `pred = 直前の足跡`、`h = fs.dir + pred.dir`
        - `|h|² > 1` なら:
          - `toPred = pred.pos - fs.pos`
          - `lat = (h.x*toPred.y - h.y*toPred.x) / |h|`(符号付きの横ずれ[m])
          - `0.1 <= |lat| <= 0.4` なら `side = lat < 0 ? LEFT : RIGHT`
          - それ以外なら `side = -pred.side`(UNDEFINED は UNDEFINED のまま)
        - `|h|² <= 1` なら UNDEFINED のまま。
        - 0.1 と 0.4 はハードコード(パラメータではない)。
      - **OSC 送信**(下記)を、足跡を保存する前に行う。
      - `footStampHistory[timeStamp]` に追加する。
      - `blockers` に `{fs.pos, timeStamp}` を追加する。
6. `footHistory[timeStamp]` に `f` を追加する(着地したかどうかに関係なく)。

### update の最後(届いたメッセージを全部処理した後で、1回だけ)

1. **ブロッカー**: `footHistory` が空でなければ、`now = 最新キー` とする。各ブロッカーについて:
   - **最新フレームの足だけ**を見て、`blockerRadius` 以内に1つでもあれば `lastSeen = now`
   - `now - lastSeen > blockerLifeTime*1e9` なら削除する。この引き算は **u64 の符号なし**。ループ直後は `lastSeen > now` で巨大な値になり、削除される。Rust では `wrapping_sub` で再現すること。
2. **足の寿命**: `footHistory` の古い順に、`最新キー - キー > lifeTime*1e9` のフレームを削除する。条件を満たさなくなったら止める。
3. **足跡の寿命**: `footStampHistory` も同じく `stampLifeTime` で削除する。

メッセージは60fps の update ごとにまとめて処理され、ブロッカーの更新はその最後に1回だけ行われる。LiDAR は約30Hz なので、1回の update に0〜2フレーム分が届く。Rust 版も「届いている分を全部処理 → ブロッカー更新」の順にすること。

### 数値の型(完全一致させるための注意)

- 位置、ベクトル、パラメータは **f32**(glm::vec2 / float)。タイムスタンプは **u64**。
- 時間の比較は C++ では `u64 → double` と `float*1e9(double)` の比較。Rust では `(now - t) as f64 > window as f64 * 1e9`。
- 速度は **割り算ではなく逆数の掛け算**。oF は `glm/gtx/scalar_multiplication.hpp` を読み込んでいて、`vec2 / double` が `v * (1.0f / (float)s)` になる。Rust では `(pos - base.pos) * (1.0 / (dt as f64 * 1e-9) as f32)`。割り算にすると丸めがずれて、着地の判定が1件変わった(実測)。
- `|dir|² > 0.0001` と `0.1 <= |lat| <= 0.4` は、f32 を f64 に上げてから比べる(C++ の暗黙変換と同じ)。
- `prevFoot`(1つ前の足そのもの)は、今の判定では使っていない。

### パラメータ(初期値はコードと `data/config.json` で同じ)

| 名前 | 値 | 意味 |
|---|---|---|
| `lifeTime` | 1.0 s | 足の履歴の寿命。描画のフェードにも使う |
| `matchWindow` | 0.1 s | 前の足を探す時間。平滑速度でさかのぼる時間にも使う |
| `matchMaxDist` | 1.0 m | 前の足とみなす最大距離 |
| `landingSpeed` | 0.8 m/s | これ未満で着地 |
| `directionWindow` | 0.5 s | 向きを出すためにさかのぼる時間 |
| `stampLifeTime` | 2.0 s | 足跡の寿命 |
| `prevStampWindow` | 2.0 s | 直前の足跡を探す時間 |
| `prevStampMinDist` | 0.25 m | 直前の足跡とみなす最小距離 |
| `prevStampMaxDist` | 2.0 m | 直前の足跡とみなす最大距離 |
| `blockerRadius` | 0.2 m | ブロッカーの半径 |
| `blockerLifeTime` | 0.15 s | 近くに足がなくなってからブロッカーが消えるまで |
| `distIp` | "localhost" | 送信先 |
| `distPort` | 10002 | 送信先ポート |
| `listenPort` | 10001 | 受信ポート |

### OSC 出力

- 着地(足跡を作った瞬間)ごとに、**`/footstamp`** を1件、バンドルにせずに送る。
- 型タグ `,ffffi`: `pos.x, pos.y, dir.x, dir.y, side`
- `side` が UNDEFINED のときは **LEFT(-1)として送る**。
- 時刻は送らない(受け取る側の描画アプリが即時に使うため、ユーザーが外した)。
- 注意: `localhost` は IPv4 で解決すること。oF(oscpack)は IPv4 で送っている。Rust で `::1` に解決されると届かないことがある。

### config.json

- oF 版の場所は `bin/data/config.json`(実行ファイルの data フォルダ)。
  - Rust 版は CI で配布するので、**実行ファイルの隣の `data/config.json`** を見るのがおすすめ(なければカレントの `data/config.json`)。ここは未決なので、ユーザーに確認すること。
- **Save**: 上の14項目を書き出す。nlohmann の出力に合わせると、キーはアルファベット順、インデントは4スペース、float は f32 を double にした値(例: `0.15000000596046448`)になる。完全に同じファイルにしたい場合は f32 を f64 にキャストしてから出力する。
- **Load**: ファイルがなければ何もしない。キーがない項目は今の値のまま(`json.value(key, 今の値)`)。
- 起動時に Load してから、ソケットを開く。

### 描画(毎フレーム、update の後)

背景は oF 標準の暗いグレー(スクリーンショットで RGB 約 60,60,60)。線の太さは1。
ワールド座標(m)から画面座標への変換は **`sx = 50 + 100*x`, `sy = 50 - 100*y`**(`ofTranslate(50,50)` と `ofScale(100,-100)`)。描く順番は次のとおり。

1. **枠**: (0,0)〜(5.4,-5.4) の白い線の四角。
2. **ブロッカー**: 塗りつぶしの円。半径 `blockerRadius`、色 (0,0,255,50)。
3. **足跡**: 塗りつぶしの足の形。`alpha = clamp(1 - (最新足跡キー - キー)/(lifeTime*1e9), 0, 1)`。**`stampLifeTime` ではなく `lifeTime` を使っている**点に注意。色は LEFT=(0,0,255)、RIGHT=(255,0,0)、UNDEFINED=(127,127,127)、透明度は alpha×255。
   - 足の形(`drawFoot`、size=0.2): pos に移動し、`atan2(dir.y,dir.x)+90°` 回転(y 上向きのワールド座標で反時計回り)する。その上で、楕円(中心 (0,0)、幅 0.1、高さ 0.2)と円(中心 (0, 0.12)、直径 0.08)を描く。小さい円がかかとで、進行方向の後ろ側に来る。
4. **足の履歴**: 塗りなしの円。半径 0.1、色 (255,0,0, alpha×50)。alpha は `lifeTime` で上と同じ計算で、基準は `footHistory` の最新キー。
5. **最新フレームの足**: 緑 (0,255,0) の円(半径 0.1、塗りなし)と、黄色 (255,255,0) の線 `pos → pos + vel*0.1`。
6. **連続する足の軌跡**: 最新フレームの各足から `prevFootAddress` を、フレームが存在する限りたどる。白い線で、透明度は 1, 0.5, 0.25… と半分ずつ下げる(×255)。

### GUI(ImGui のウィンドウ「Debug」、毎フレーム (600,100) に固定)

上から順に並べる。数値の表示は小数3桁(ImGui の DragFloat の標準)。

1. FPS の表示
2. ドラッグで変える数値(ラベル / 刻み / 最小 / 最大)
   - `Life Time [s]` 0.1 / 0.1 / 10
   - `Match Window [s]` 0.01 / 0.03 / 1
   - `Match Max Dist [m]` 0.01 / 0.1 / 3
   - `Landing Speed [m/s]` 0.01 / 0.05 / 5
   - `Direction Window [s]` 0.01 / 0.05 / 1
   - `Stamp Life Time [s]` 0.1 / 0.1 / 10
   - `Prev Stamp Window [s]` 0.1 / 0.1 / 10
   - `Prev Stamp Min Dist [m]` 0.01 / 0 / 2
   - `Prev Stamp Max Dist [m]` 0.01 / 0 / 3
   - `Blocker Radius [m]` 0.01 / 0.01 / 1
   - `Blocker Life Time [s]` 0.01 / 0 / 1
3. `Dist IP`(テキスト入力): **Enter で確定**し、送信先を設定し直す。確定前に入力欄から離れると、元の値に戻る。
4. `Dist Port`(整数入力): Enter で確定し、送信先を設定し直す。
5. `Listen Port`(整数入力): 入力した値はすぐ変数に入る。横の **Reset** ボタンを押したときに受信を開き直す。
6. **Save** と **Load** を横に並べる。Load は読み込んだあと送信先を設定し直す。受信は開き直さない。

---

## 検証方法(oF 版と Rust 版の出力を突き合わせる)

1. oF 版と Rust 版の `config.json` で、受信ポートと送信先ポートを別々にする。例: oF は listen 11001 / dist 11002、Rust は listen 12001 / dist 12002。
   - oF 版の `bin/data/config.json` は git 管理されているので、終わったら `git checkout bin/data/config.json` で戻すこと。
   - ユーザーの oF 版が 10001 番で動いていることがあるので、10001 番は使わない。
2. 両方のアプリを起動する。
3. 受信を2つ同時に始める: `python3 tools/osc_capture.py 11002 50 > of.tsv` と `python3 tools/osc_capture.py 12002 50 > rs.tsv`
4. 同じ入力を両方に送る: `python3 tools/osc_replay.py testdata/capture2_40s.tsv 11001 12001`
5. `of.tsv` と `rs.tsv` の `/footstamp` を順に比べる(件数、pos、dir、side)。

update の区切り(60fps の中でどのメッセージが同じフレームに入るか)が両アプリで完全には一致しないので、ブロッカーの更新タイミングがずれて、まれに1〜2件の差が出る可能性はある。決定的に比べたい場合は、detector モジュールに TSV を直接流すテストを作る。受信時刻(TSV の1列目)を 1/60 秒ごとに区切って update 1回分とする。

参考値(Python のシミュレーションによる。今の設定に近い条件): データ2の40秒で足跡は約180個、データ1の20秒で約60個。

## GitHub Actions

- matrix: `macos-latest`(arm64)と `windows-latest`
- 手順: `actions/checkout` → `dtolnay/rust-toolchain@stable` → `Swatinem/rust-cache` → `cargo build --release` → `actions/upload-artifact`(実行ファイルと `data/config.json`)
- Windows: リリースビルドでコンソールを出さないなら、`#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` を付ける。初回の UDP 受信でファイアウォールの許可ダイアログが出る。
- macOS: 素のバイナリで十分。`.app` にするなら `cargo-bundle` などを使う。未署名なので、初回は Gatekeeper の許可が必要。
- Intel Mac には対応しない。

## 進め方の注意(ユーザーについて)

- やりとりは日本語。開発方針は「**堅牢性より、可読性・ロジックの小ささ・調整する変数の少なさ**」。
- パラメータの値は、ユーザーが画面を見て決めることが多い。数字で比べた結果を出したうえで、最終判断はユーザーに任せる。
- コミットは頼まれたときだけ。push は外部に公開する操作なので確認してから。
- ゴール: ①着地の瞬間に位置・向き・左右を出す(oF 版で達成済み)、②その場の足踏みも検出する(未着手)。

## 進捗(2026-10-07)

- 移植は完了。`cargo test` で、testdata の2本とも oF 版の出力と **件数・pos・dir・side がビット単位で一致**する(データ2: 184件、データ1: 59件)。
- 実際に oF 版と Rust 版を並べて同じ入力を流した比較でも、243件すべてビット一致した。
- `config.json` の読み書きは、oF 版が書いたファイルとバイト単位で同じになる(テストあり)。
- oF 版との違い(意図したもの):
  - update は描画とは別のスレッドで 60fps で回す。ウィンドウを最小化しても検出と送信が止まらないようにするため。描画は画面のリフレッシュレートで行う。
  - `config.json` は「実行ファイルの隣の `data/config.json`、なければカレントの `data/config.json`」(仮決め)。
- Windows はローカルで `cargo check --target x86_64-pc-windows-msvc` まで確認した。
- GUI の見た目は、まだ人の目で確認していない。

## 未決事項

- `config.json` の置き場所(今は仮決め。上の「進捗」を参照)。

## 決まったこと

- リポジトリ: `git@github.com:kisakutanaka/WalkDetectionRust.git`(WalkDetection3 とは別)。
- Intel Mac には対応しない(CI は `macos-latest` の arm64 だけ)。
