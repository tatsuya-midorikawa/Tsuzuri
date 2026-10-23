# C12: `Map`／`Set` の更新を対数時間に近づける（チャンク化したソート配列）

| 項目 | 内容 |
| --- | --- |
| ID | C12 |
| 優先度 | P1 |
| 規模 | 全体 M（3〜5 日）。Phase 1: M（3〜4 日。`std` は付録の全文の転記で、テスト・デバッガ表示・文書が作業の大半）、Phase 2: S（1 日未満） |
| 依存 | なし。Rust のコンパイラは 1 行も変えない（`std/Map.tz`・`std/Set.tz` と、それを読むテスト・表示・文書だけを変える） |
| 後続 | なし。G35（`@complexity` による文書の生成）が入るときは、`Map`／`Set` の計算量の表を本チケットの仕様 §5 の値に合わせる。`std/Json.tz` は動作を変えない |
| 状態 | done（Phase 1・2、2026-10-23） |
| 起票 | 2026-10-18（D-50。HEAD `a2313d19` で確認） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1… はすべて承認済み |
| 改善する劣位 | `Map`／`Set` の `insert`・`remove` は、整列した 1 本の `Vec` を `Vec.swap` で 1 要素ずつずらすので $O(n)$。実測（HEAD、`-O3`）はランダムな 10^5 件の挿入が約 1.3 秒、10^6 件が 200 秒。C++ の `std::map`・Rust の `BTreeMap` は同じ作業を 1 秒未満で終える。README は「平衡二分探索木」と書いており、実装と食い違う |
| 手本にする既存実装 | `std/HashMap.tz`（診断用の公開関数 `longest_probe` の前例）、`std/Map.tz`・`std/Set.tz`（置き換える対象。`Vec.swap` と `Vec.pop` で所有値を動かす流儀）、`tests/property.rs`（`tsuzuri test` を Rust から走らせる流儀）、`tests/fixtures/map_set/Main.tz` と `tests/features.mjs` の suite `map_set`、`scripts/lldb/tsuzuri_lldb.py` の `SequenceProvider`・`CollectionProvider` |
| 主な影響ファイル | Phase 1: `std/Map.tz`・`std/Set.tz`、`tests/fixtures/map_set_chunks/Main.tz`（新規）、`tests/map_set_chunks.rs`（新規）、`tests/features.mjs`、`tests/map_set.rs`、`scripts/lldb/tsuzuri_lldb.py`、`src/runtime/tsuzuri.natvis`、`tests/debugger.mjs`、`std/Json.tz`・`tests/fixtures/json/Main.tz`（コメントだけ）、文書（`README.md`・`docs/language.md`・`docs/architecture.md`・`map.md`・`set.md`・`hashmap.md`・`array.md`・`json.md`）。Phase 2: `benchmarks/ordered/Main.tz`（新規）、`docs/benchmarks.md` |

付録: [C12-appendix.md](C12-appendix.md)（A: `std/Map.tz` 全文、B: `std/Set.tz` 全文、C: ベンチマークの program、D: 実測）、[C12-appendix-tests.md](C12-appendix-tests.md)（付録 2。A: fixture 全文、B: `features.mjs`、C: Rust のテスト、D: `map_set.rs` の追加、E: LLDB、F: natvis、G: `debugger.mjs`、H: モック試験、I: 文書の文面）。

## 実装・検証の記録（2026-10-23）

- **Phase 1・2 とも実装・ローカル検証済み**。作業基準は `41f80bc7`。
  実装前に README の D-59 整合確認を `51919beb` としてコミットした。
  付録 A・B は現在の subject-last の公開シグネチャ・`Vec` 呼び出しと一致し、
  既存の公開 API はすべて維持した。`std/Map.tz`・`std/Set.tz` は付録と全文一致
  （後述のレビュー修正は付録 B にも反映した）。
- 本文の旧 release 前提は現行の dev（opt-level 1）に合わせた。
  std 埋め込みを再ビルドし、stack・解析上限は変更していない。性能比較だけは両コンパイラを release ビルドした。
- Rust: `map_set` 3 件、`map_set_chunks` 1 件（5 properties × O0/O3）、
  `std_argument_order` 21 件、特殊化上限・Hash コンテナ予算・PDB/natvis の回帰各 1 件が成功。
  全 nextest は **1,719 passed / 2 skipped**。
  別途、サーバー 19 件と fake-toolchain/キャッシュ 1 件も成功。
- E2E: `map_set` **26**、`map_set_chunks` **33**、`json` **165**、
  `iteration_protocol` **26** を native/WASM の O0/O3 で確認。
  `user_drop` も native/WASM の O0/O3 で成功。
- 文書: 変更 6 ページ、163 links、52 examples、104 native runs（O0/O3）が成功。
  同じ 52 runnable projects を明示 WASI でも O0/O3 の **104 runs** で確認した。
- デバッガー: natvis XML と 7 レイアウトのモックに加え、
  **実 LLDB の `tests/debugger.mjs` 全体が成功**（既存表示・stepping と Set 513 件の `[511]`／`[512]` を含む）。
  実機試験で付録 E の lazy header 読み取りに不備を発見したため、
  `CreateValueFromAddress` が返す Vec ヘッダを `GetNonSyntheticValue()` にしてから読む修正を加えた。
  `tests/lldb_collections.py` は合成 Vec を再現し、修正前に赤・修正後に緑となる永続的な回帰試験。
  Linux CI にも LLDB 21 と実デバッガー suite を追加した。
- Copilot レビューの境界キャッシュの線形再走査の指摘も修正した。
  lazy なヘッダ読み取りは維持し、既存 List provider と同じ二分探索で所属チャンクを求める。
  128 チャンクのキャッシュ済み境界へのアクセスを対数回に制限する回帰試験は修正前に赤・修正後に緑で、
  CI の時間の閾値は使わない。
- 後続レビューで `Set.select` の比較用キーコピーも修正した。比較はチャンクの借用で行い、
  選んだ左キーだけを、借用元の情報を保つ `key_at` 経由でコピーする。
  600,000 要素の配列キー 2 本と選択結果が収まる既存の WASM 16 MiB 上限で、
  修正前は比較用の clone により trap、修正後は積・差とも native/WASM O0/O3 が成功した。
  左右カーソルが別々に進む小さな配列キーのケースも追加し、4 件増の 33 件とした。
- 初回の共有キャッシュの open 待ち、長すぎる／相対の `TMPDIR` は環境要因として切り分けた。
  自分専用の `TSUZURI_CACHE_DIR` と短い絶対 `TMPDIR` で全件を再検証し、共有キャッシュや他 worktree は変更していない。
- `cargo fmt`、全 target の Clippy（warnings denied）、runtime include **58 files** が成功。
  `src/` の累積差分は `src/runtime/tsuzuri.natvis` のみで、Rust・Vec builtin・依存関係は変更していない。
- Phase 2: 同一ベンチマークの 6 workloads／15 sizes 組を 5 回ずつ交互に測り、
  **150 回すべての checksum が組ごとに一致**。測定した実行ファイルは独立の `--no-cache` build とも byte 一致。
  10 万件のランダム挿入は中央値で 52.8 倍、削除は 65.6 倍。
  検索・カウンタ・集合演算の遅延増、全中央値・範囲、非常に高い load 124.57〜135.08 は
  [実測表](../../docs/benchmarks.md#実測2026-10-23) に明記した。100 万件のランダム操作へは外挿せず、
  CI に速度の合否は追加していない。
- 台帳追記候補: 「C12 Phase 1・2 done、bounded chunk Map/Set、実 LLDB と 15 組の matched benchmark 検証済み」。
  `_features/README.md` と GUIDE の台帳は本チケットの指示どおり統括者に委ねる。CI・レビューの結果は PR で追跡する。

## 実装者向け早見

1. **最初に読む**: 本ファイルの「仕様」§1〜§5 と、付録 A・B（`std/Map.tz`・`std/Set.tz` の全文。検証済み）。`std` のコードは**付録から写し、書き換えない**（TAB インデントのまま。`///` の行だけは別で、G34 の例・G35 のタグが既にあれば手順 3 のとおり保つ）。設計と食い違うと思ったら、書き換えずに「停止条件」に従う。
2. **最初に書くテスト**: 付録 2 A の `tests/fixtures/map_set_chunks/Main.tz`、付録 2 B の suite `map_set_chunks`、付録 2 C の `tests/map_set_chunks.rs`（手順 2）。`std` を差し替える前は `error[E1002]: module 'Map' has no function or union case 'invariants_hold'` で赤になるのが正しい。
3. **構造**: `record Map<'key,'value> { head: Vec<Entry<..>>, rest: Vec<Vec<Entry<..>>>, length: i64 }`、`record Set<'key> { head: Vec<'key>, rest: Vec<Vec<'key>>, length: i64 }`。論理上のチャンクの列は `[head] ++ rest`。各チャンクは 1〜512 エントリ、隣り合う 2 つのエントリ数の合計は 256 を超える。
4. **定数**: `MaxChunk = 512`、`MergeLimit = 256`、`BulkFill = 384`（`Set` だけ）。変えない（決定事項 D3）。
5. **公開 API**: `Map`・`Set` の既存の全関数の名前・型・制約・引数の順序・振る舞い・走査の順序は 1 つも変えない。足すのは各モジュールに診断用の `chunk_count`・`invariants_hold` の 2 つだけ。
6. **触らない**: `src/` の Rust（`src/stdlib.rs` の `opaque_record` も）、`Vec` の組み込み、`std/HashMap.tz`・`std/HashSet.tz`・ほかの std、既存のテストの期待値（`tests/map_set.rs` の既存のソースと `map_set` suite の 26 件）。`Vec` に `insert`・`remove` を足さない（`std` の private 関数で書く）。
7. **罠**: 借用を持つキー・値を、`ref` のローカル変数経由で読むと `E1014`（「落とし穴」1）。評価順序（`f x (g (ref x))` は `x` を先に動かす）。`std` を変えたら `cargo build --release --locked` を**毎回**やり直してから Node のテストを走らせる（std はコンパイラに埋め込まれる）。
8. **Phase 1 の完了の見分け**: `cargo test --locked` が緑、`node tests/features.mjs target/release/tsuzuri map_set` が `26 cases passed`、同 `map_set_chunks` が `29 cases passed`、`json` が `165`、`iteration_protocol` が `26`、`tests/user_drop.mjs` が通る、`tests/debugger.mjs` が通る（LLDB が使える環境で）、`node scripts/check-docs.mjs` が変更した全ページで通る。
9. **Phase 2 の完了の見分け**: `benchmarks/ordered/Main.tz` が `tsuzuri build -O3` で通り、旧実装と新実装の checksum が全 workload で一致し、`docs/benchmarks.md` に節「順序付きコンテナ（C12）」がある。速度の合否の閾値はどこにも入れない。

## 目的

**問題**: `Map`・`Set` は、キー昇順の 1 本の `Vec` に二分探索で位置を決め、`insert`・`remove` で後ろの要素を `Vec.swap` で 1 つずつずらす。検索は $O(\log n)$ だが、更新は $O(n)$ で、100 万件を順序なしに挿入すると 200 秒かかる（付録 D）。
`Json` の decode（`std/Json.tz`）も、この `O(n)` の挿入を避けるために、整列してから昇順に追加している（`by_key`）。

**利用者の価値**: 順序付きの連想配列を、10^5〜10^6 件の規模で、更新を繰り返しても使えるようにする（実測の目標は、10^5 件のランダム挿入が 1.3 秒から約 25 ミリ秒、10^6 件が 200 秒から 1 秒未満）。検索の速さと、公開 API・走査の順序・トラップ・所有権は変えない。

**HEAD で測った数字**（`-O3`、native、Apple M1 Max。ロードアベレージ 11〜36 の揺れた環境。全表は付録 D）: ランダムな 10^5 件の挿入は HEAD が 1.30 秒、付録 A・B の実装が約 24 ミリ秒。10^6 件は 200.5 秒と 0.4〜0.74 秒。10^5 件の削除は 0.85 秒と約 22 ミリ秒。
代わりに、検索は同程度〜1.5 倍遅く、1,024 キーのカウンタ更新（5×10^6 回）は約 235 ミリ秒から約 283 ミリ秒（約 1.2 倍遅い）になる。

**非目標**:
- 公開 API の変更（関数の追加・削除・型の変更・走査順序の変更）。足すのは診断用の 2 関数だけ。
- `Vec` への `insert`・`remove`・`split_off` の追加（Rust のコンパイラは変えない）。
- 木（B 木・赤黒木）の実装、`Map` の範囲検索（`range`・`first`・`last`・`pop_first`）の追加。
- SIMD・並列・GPU の導入（高速化はアルゴリズムの変更だけ。小さいマップの更新が最大 2 倍遅くなる箇所は付録 D に正直に書く）。
- 速度の合否を判定する CI の閾値（実測は参考値として記録するだけ）。

## 着手条件と停止条件

### 着手条件

- 依存するチケットはない。承認は D-50 の包括許可。
- ベースライン（GUIDE §2.3）: `cargo build --release --locked` と `cargo test --locked` を実行し、結果を控える。失敗があれば、`Map`・`Set` と無関係であることを記録してから着手する。
  HEAD の実測（`node tests/features.mjs target/release/tsuzuri <suite>`）: `map_set` が `features: 26 cases passed`、`json` が `165 cases passed`、`iteration_protocol` が `26 cases passed`。`node tests/user_drop.mjs target/release/tsuzuri` が `user drop: native/WASM -O0 passed` と `-O3 passed`。
- 読むもの: `std/Map.tz`・`std/Set.tz`（HEAD。103 行と 143 行）、`tests/map_set.rs`、`tests/features.mjs` の suite `map_set`、`scripts/lldb/tsuzuri_lldb.py` の `is_collection`・`collection_summary`・`CollectionProvider`、`src/runtime/tsuzuri.natvis` の `Map`・`Set`、`_features/GUIDE-LITE.md`。
- 環境: Node.js 20 以降、Clang／LLD 17 以降、Python 3.9 以降。`tests/debugger.mjs` は LLDB が `run` でプロセスを起動できる環境が必要（**この文書の筆者の環境では LLDB の `run` が応答せず、実行できなかった**）。

### 停止条件

次のどれかになったら、即興で回避せず、止めて報告する。選択肢を付けた。

1. **付録 A・B の `std` が `tsuzuri check` で拒否される**（HEAD で言語が変わっていた場合。`E10xx` の診断）。選択肢 A: 診断の指す 1 行だけを最小の書き換えで通し、意味を変えないことを `map_set_chunks` で確かめて報告に書く。B: 書き換えが 3 か所を超えるなら止める。
2. **既存の `map_set`（26 件）・`json`（165 件）・`iteration_protocol`（26 件）・`tests/user_drop.mjs` のどれかが赤になり、期待値を変えないと直らない**。期待値は変えない約束（「変えてはいけない意味」）。選択肢 A: 実装を直す。B: 期待値が誤っていたことを示す独立の根拠（手計算）を添えて報告する。
3. **`map_set_chunks` の手計算の期待値（例: `merge_after_removals` の `22001256`）が実測と違う**。期待値は仕様 §4 の規則から手で導出してある。選択肢 A: 分割・併合の規則（数値）の実装を直す。B: 導出の誤りを示して報告する。
4. **`comparison_bounds`（`32768`）か `set_algebra_bounds`（`16384`）が 0 以外**。`Ord.lt` の回数が上界を超えた（アルゴリズムが $O(n)$ の比較に退化している）。値 11〜13・21〜23 の意味はフィクスチャのコメントにある。止めて報告する。
5. **`std` 内で `E1014`（`cannot move an exclusive reference while it is reborrowed`）が出る**（キー・値が借用を持つ型のとき）。選択肢 A: 「落とし穴」1 の形（`append_*`・`key_at` のように、引数経由で読む関数）に直す。B: 直せなければ止める（`borrowed_keys_and_values` が通ることが条件）。
6. **`tests/debugger.mjs` を実行できる環境がない**（LLDB が `run` で止まる）。選択肢 A: 実行できる環境（人間の手元・CI）で実行してもらい、Phase 1 は「デバッガ表示は未検証」と書いて報告する。B: 付録 2 H のモック試験の結果を添える。**`tests/debugger.mjs` を通さずに Phase 1 を完了にしない**。
7. **`tests/debugger.mjs` の既存の期待（`size=1`・`size=3`）は通るが、付録 2 G の複数チャンクの期待だけが赤**。選択肢 A: LLDB で `frame variable many` を手で実行し、表示が正しければ（`size=513`、`[512] = 512`）期待の正規表現を直す。B: 表示が誤りなら、プロバイダ（付録 2 E）の `address`・`update` を直す。
8. **`cargo test --locked` の `src/polymorph.rs` の `honors_the_exact_specialization_limit`・`hash_containers_leave_the_specialization_budget_to_users` が赤になる**。`std` の生成関数が単独で特殊化されている。止めて報告する（`Map.tz`・`Set.tz` の private 関数のうち、どれが先に特殊化されるかを添える）。
9. **同じソースを 2 回コンパイルした IR が違う**（決定性の破れ。`std` は `Vec` とレコードだけを使うので起きないはず）。止めて報告する。
10. **Phase 2 の測定で、ランダム挿入 10^5 件が HEAD の 5 倍以上速くならない**（実測は約 55 倍）。実装が $O(n)$ のままの疑い。止めて報告する（速度の閾値を CI に入れる話ではなく、実装の異常の検出）。
11. **`node scripts/check-docs.mjs` が、変更していない例を落とす**（`map.md`・`set.md` の `run=` の出力が変わった）。`Map`・`Set` の走査順序が変わった疑い。止めて報告する。
12. **`git status` に、「主な影響ファイル」にないファイルの変更が出る**（特に `src/*.rs`）。戻して、止めて報告する。

## 現状（HEAD `a2313d19` で確認）

作業木の HEAD は `9dd0fbbc`（`a2313d19` の子孫で、差は `docs/benchmarks.md` の 41 行だけ）。実行は `target/release/tsuzuri`（HEAD のビルド）と、複製の作業木に付録 A・B を置いて `cargo build --release` したコンパイラで行った。

**コード**（ファイル + 関数）:
- `std/Map.tz`（103 行）: `private record Entry<'key,'value> { key, value }`、`record Map<'key,'value> { entries: Vec<Entry<..>> }`。`lower_bound`（二分探索）、`found`、`insert`・`remove`（`Vec.swap` を繰り返してずらす）、`to_array`・`keys`・`values`・`fold`・`iter`。
- `std/Set.tz`（143 行）: `record Set<'key> { entries: Vec<'key> }`。`union`（末尾から pop して逆順の一時コレクションを作り反転）、`intersect`・`difference`（2 本の添字）。
- `src/stdlib.rs`: `("std/Map.tz", include_str!("../std/Map.tz"))` で埋め込む。`opaque_record` が `"Map.Map"`・`"Map.Entry"`・`"Set.Set"` を不透明にする（`E1022`）。**名前が同じなら Rust の変更は要らない**。
- 公開関数（`grep -n "^def " std/Map.tz std/Set.tz`）: `Map` は 14、`Set` は 13。付録 A・B はこれらの型・制約を 1 文字も変えず、診断用の `chunk_count`・`invariants_hold` を各モジュールに足す（`diff <(grep "^def " std/Map.tz) <(grep "^def " <新>)` の差は、その 2 行だけ。実測で確認済み）。

**再現コマンドと実出力**:

```sh
$ node tests/features.mjs target/release/tsuzuri map_set
map_set: native/WASM at O0/O3 passed
features: 26 cases passed
$ node tests/features.mjs target/release/tsuzuri json
json: native/WASM at O0/O3 passed
features: 165 cases passed
$ node tests/features.mjs target/release/tsuzuri iteration_protocol
iteration_protocol: native/WASM at O0/O3 passed
features: 26 cases passed
$ node tests/user_drop.mjs target/release/tsuzuri
user drop: native/WASM -O0 passed
user drop: native/WASM -O3 passed
```

新しい fixture を HEAD のコンパイラで走らせると、診断用の関数がないので赤になる（手順 2 の期待）:

```sh
$ target/release/tsuzuri test tests/fixtures/map_set_chunks
tests/fixtures/map_set_chunks/Main.tz:14:24: error[E1002]: module 'Map' has no function or union case 'invariants_hold'
tests/fixtures/map_set_chunks/Main.tz:14:53: error[E1002]: module 'Map' has no function or union case 'chunk_count'
```

付録 A・B を持つコンパイラでは（実測）:

```sh
$ node tests/features.mjs <新しい compiler> map_set_chunks
map_set_chunks: native/WASM at O0/O3 passed
features: 29 cases passed
$ <新しい compiler> test tests/fixtures/map_set_chunks          # -O3 も同じ
ok 1 - Main Map follows the table model on short operation lists
ok 2 - Main Set follows the table model on short operation lists
ok 3 - Main Map follows the table model on long seeded runs
ok 4 - Main Set follows the table model on long seeded runs
ok 5 - Main Set algebra follows the table model on long seeded runs

5 passed; 0 failed; 0 ignored
```

**`Vec` の API で書けるか**（実機で確かめた。`grep -rhoE '"Vec\.[a-z_]+"' src/*.rs` の結果）。組み込みは `at`・`capacity`・`clear`・`clone`・`empty`・`get`・`is_empty`・`length`・`of_array`・`pop`・`push`・`reserve`・`set`・`swap`・`to_array`・`truncate`・`with_capacity` の 17 個で、`std/Vec.tz` が `iter` を足す。足りない操作は、`std` の private 関数で次のように書く（付録 A の関数名）:

| 欲しい操作 | `Vec` にあるか | 代替（付録 A の private 関数。`Map.tz`・`Set.tz` に同じものを置く） | 計算量 |
| --- | --- | --- | --- |
| 位置 `i` への挿入 | ない | `insert_at items index item`: `push` してから `swap` で `index` まで運ぶ | $O(\text{size} - i)$ |
| 位置 `i` の削除 | ない | `remove_at items index`: `swap` で末尾まで運んで `pop` | $O(\text{size} - i)$ |
| 順序を保つ取り出し | ない | `take items index`: `index` と末尾を `swap` して `pop`（順序は壊れる） | $O(1)$ |
| 取り出しの取り消し | ない | `put items index item`: `push` して末尾と `index` を `swap`（`take` の逆） | $O(1)$ |
| 末尾側の切り出し | ない | `split_off items at`: `pop` を繰り返して新しい `Vec` に `push` し、`reverse` | $O(\text{size} - \text{at})$ |
| 連結 | ない | `join head tail`: `tail` を `reverse` して `pop` を繰り返し `head` に `push` | $O(\lvert\text{tail}\rvert)$ |
| 反転 | ない | `reverse items`: 対称な位置の `swap` | $O(\text{size})$ |

**`E1014` の実機での再現**（落とし穴 1 の根拠。キーが借用 `ref Atomic<i64>` を持つとき、ローカルの `ref` 変数経由で `Copy` の要素を読んで `Vec` に集めると拒否され、引数経由なら通る。HEAD のコンパイラ。プログラムは付録 E）:

```sh
$ target/release/tsuzuri check r2/Main.tz       # let keys = chunk_of source chunk; ... Vec.push key current ...; finished = Vec.push current finished
r2/Main.tz:19:33: error[E1014]: cannot move an exclusive reference while it is reborrowed
$ target/release/tsuzuri check r3/Main.tz       # 同じ読み出しを、引数 `ref Bag` を受け取る関数 key_at に分ける
（出力なし。通る）
```

**IR の連番**（変えてはいけない意味 12 の根拠）: `IO.writeln "hello"` だけのプログラム（付録 E）の `--emit llvm -O0` の IR は、HEAD と付録 A・B のコンパイラで 5 行だけ違い、すべて `@tz.fn.$intrinsic.Display.display.426` が `.475` になる連番の接尾辞（`-O3` も同じ）。
連番に依存するテスト・文書はない（`grep -rln 'intrinsic\.[A-Za-z]*\.[a-z_]*\.[0-9]\+' tests src docs` は、git 管理外の生成物 `tests/fixtures/net_sockets/Main.ll` だけ）。

**`tests/map_set.rs` の 28 個のソース**（`Map`・`Set` を含む文字列リテラルすべて）を、HEAD と付録 A・B のコンパイラの `check` に通した結果（受理か最初の診断コード）は 28 件すべて同じ（`ok`・`E1005`・`E1008`・`E1012`・`E1013`・`E1014`・`E1022`）。フィールド名が `entries` から変わっても `E1022` が出る（`record_storage` の検査が、フィールドの探索より前にある）。

## 変えてはいけない意味

次は、Phase 1・2 のどちらでも不変。テストで確かめる（括弧内）。

1. **公開 API**: `Map` の 14 関数と `Set` の 13 関数の、名前・型・制約（`Ord`・`Copy`）・引数の順序・所有（`ref` か値か）。足すのは `chunk_count`・`invariants_hold` の 2 つ × 2 モジュールだけ（`diff` の確認。`tests/map_set.rs` の既存 28 ソースの結果が同じ）。
2. **走査と列挙の順序**: `fold`・`iter`・`keys`・`values`・`to_array` は、キーの昇順（E2E と `Json` の 165 件）。
3. **`insert` が既存のキーを持つとき**: 最初に格納されたキー（代表値）を残し、新しく渡されたキーと古い値を捨てて、値だけを置き換える（suite `map_set` の `map_replace_drop`・`first_representative`、`tests/user_drop.mjs`）。
4. **`union`**: 重複キーは左辺のキー代表値を残す。`union` は両辺を消費し、`intersect`・`difference` は両辺を借用して左辺からコピーする（キー型に `Copy` が必要）。
5. **トラップ**: キーとして渡した値が `Eq.eq key key` を満たさない（NaN）とき、空のマップでも、あらゆる操作がトラップする。格納済みの NaN を比較で踏んだときもトラップする（`nan_query`・`nan_stored`）。`Map.at` は不在のキーでトラップする（`map_at_missing`）。
6. **確保しない検索**: `contains_key`・`get`・`at`・`contains` は、実行中に `@tz.alloc`・`@tz.realloc` を呼ばない（`features.mjs` の `inspect`）。
7. **解放**: すべての E2E で `live == 0`（漏れなし）。`tests/user_drop.mjs` の `map_remove` の drop 順序（`[2,100]` の後に `{1,3}`）。
8. **不透明**: `Map`・`Map.Entry`・`Set` のフィールドへの構築・参照・パターン・更新は `E1022`（`tests/map_set.rs`）。`src/stdlib.rs` は変えない。
9. **決定性**: 同じソースから同じ IR（`std` は `Vec`・レコード・整数だけを使い、`HashMap` の反復順に依存しない）。
10. **WASM**: 既定の import を足さない。新しい組み込み関数・ランタイム関数・`unsafe`・crate を足さない。
11. **デバッガの表示形式**: `Map`・`Set` は `size=N` と、キー順の `[0]`、`[1]`…（`tests/debugger.mjs` の既存の期待 `table = size=1 {`・`[0] = (key = 1, value = 10)`・`keys = size=3 {...}`）。
12. **`Map`・`Set` を使わないプログラムの意味と IR**: 意味は不変。IR は連番の接尾辞（例: `$intrinsic.Display.display.<N>`）だけが `std` の宣言数の増減で変わりうる（「現状」の実測）。

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | `std/Map.tz`・`std/Set.tz` の差し替え、新しいテスト（fixture・suite・Rust）、`features.mjs` の `inspect` の修正、デバッガ表示（LLDB・natvis）、`tests/debugger.mjs` の追加、文書（計算量と内部構造の記述） | M | なし | 公開 API・走査順序・トラップ・所有権は不変で、更新が速くなるだけ。デバッガの表示は同じ PR で更新するので壊れない。文書も同じ PR で、実装と一致する |
| 2 | ベンチマーク program `benchmarks/ordered/Main.tz` と、`docs/benchmarks.md` の節「順序付きコンテナ（C12）」（before／after の記録手順と、その環境の実測値。参考値で閾値なし） | S | Phase 1 | 実行時の挙動を変えない（新しいファイルと文書だけ）。Phase 1 の後に、いつでも足せる |

## 仕様

記号: `n` = `length`（エントリ数）、`c` = チャンクの個数、`size(k)` = 論理チャンク `k`（`0 <= k < c`）のエントリ数、`B` = `MaxChunk` = 512。

### §1 構造

```text
private record Entry<'key, 'value> { key: 'key, value: 'value }                       // 変えない
record Map<'key, 'value> { head: Vec<Entry<'key, 'value>>, rest: Vec<Vec<Entry<'key, 'value>>>, length: i64 }
record Set<'key> { head: Vec<'key>, rest: Vec<Vec<'key>>, length: i64 }
```

論理チャンクの列は、`length == 0` なら空、そうでなければ `[head] ++ rest`。論理チャンク番号 `k` は、`k == 0` が `head`、`k >= 1` が `rest[k - 1]`（関数 `chunk_of`）。`c = chunk_total` は、`length == 0` なら 0、そうでなければ `1 + rest.length`。
`head` を別のフィールドにするのは、小さいマップ（チャンク 1 つ）の検索・置換を、HEAD の 1.04〜1.2 倍に収めるため（決定事項 D1）。

### §2 不変条件（すべての公開関数の入口と出口で成り立つ）

| 番号 | 不変条件 | 保つ場所 |
| --- | --- | --- |
| I1 | `length` は全チャンクのエントリ数の合計 | すべての更新が `length` を 1 つだけ増減（`put_chunk` の `count` 引数）する |
| I2 | 空でないマップのチャンクは 1〜512 エントリ | `add_entry`（満ちたら分割）。`remove` は空のチャンクを作らない（サイズ 1 のチャンクは `delete_chunk` で除く） |
| I3 | キーは全チャンクを通して厳密に昇順（`Ord.lt`） | `locate` が決める挿入位置。`split_off`・`join`・`insert_at`・`remove_at` は順序を保つ |
| I4 | `length == 0` なら `head`・`rest` はともに空 | `empty`・`delete_chunk`・`from_chunks` |
| I5 | 隣り合う 2 つのチャンクのエントリ数の合計は 256 を超える | 分割は半分ずつ。`shrink` が合計 ≤ 256 の組を併合。`delete_chunk` の前後（下の証明） |

I5 から、`c <= 2n/257 + 1`（互いに素な隣り合う組が `⌊c/2⌋` 組あり、各組が 257 以上）。`invariants_hold` は I1〜I5 をすべて検査する。

### §3 定数

| 名前 | 値 | 意味 | 根拠 |
| --- | --- | --- | --- |
| `MaxChunk` | 512 | 1 チャンクの最大エントリ数 | 付録 D の振りと決定事項 D3 |
| `MergeLimit` | 256 | 隣り合う 2 チャンクの合計がこの値以下なら併合 | `MaxChunk / 2`。併合後のチャンクは 256 以下なので、次の分割まで 256 回以上の挿入が要る（分割・併合の往復で時間を失わない） |
| `BulkFill` | 384 | `union`・`intersect`・`difference` が作るチャンクのエントリ数（最後の 1 つを除く） | `MaxChunk × 3/4`。作った直後に 128 回挿入できる余白を残す。最後の 1 つ（1〜384）と隣の 384 の合計は 257 以上なので I5 を満たす |

### §4 操作の規則（決定表）

**4.1 `locate map key` → `(chunk, offset)`**（private。`contains_key`・`get`・`at`・`insert`・`remove` が使う）

1. `assert (Eq.eq key key)`（NaN のトラップ。空のマップでも先に行う）。
2. `c == 0` なら `(0, -1)`。
3. チャンクの二分探索: `low = 0`、`high = c - 1`。`low < high` の間、`middle = low + (high - low) / 2`、そのチャンクの**最後のキー**を `assert (Eq.eq last last)` で検査し、`Ord.lt last key` なら `low = middle + 1`、そうでなければ `high = middle`。`low` は「最後のキーが `key` 以上の最初のチャンク。なければ最後のチャンク」。
4. チャンク `low` の中の二分探索: `start = 0`、`stop = size`。`start < stop` の間、`middle = start + (stop - start) / 2`、そのキーを `assert (Eq.eq k k)` で検査し、`Ord.lt k key` なら `start = middle + 1`、そうでなければ `stop = middle`。`start` は下限位置。
5. `start < size && !(Ord.lt key (チャンク low の start 番目のキー))` なら見つかった。

| 状況 | 戻り値 |
| --- | --- |
| 空のマップ | `(0, -1)` |
| キーが存在し、チャンク `k` の位置 `o` にある | `(k, o)`（`o >= 0`） |
| 存在せず、挿入位置がチャンク `k` の位置 `o`（`0 <= o <= size(k)`） | `(k, -1 - o)` |
| 存在せず、全キーより大きい | `(c - 1, -1 - size(c - 1))` |
| 存在せず、全キーより小さい | `(0, -1)` |

**4.2 `insert key value map`**

| 順 | 条件 | 動作 | `Ord.lt` |
| --- | --- | --- | --- |
| 1 | `length > 0` かつ `Ord.lt (最後のキー) key`（`beyond_last`。最後のキーと新しいキーを `assert (Eq.eq ..)` で検査する） | `add_entry map (c - 1) size(c - 1) entry`（末尾への追加） | 1 |
| 2 | 上でなく、`locate` が見つけた（`offset >= 0`） | `replace_value`: **古いキーを残し**、値だけ差し替える（新しいキーは捨てる） | `locate` の分 |
| 3 | 上でなく、`locate` が見つけなかった | `add_entry map chunk (-1 - offset) entry` | `locate` の分 |

**`add_entry map chunk offset entry`**（前提: 空でなければ `0 <= chunk < c`、`0 <= offset <= size(chunk)`）

| 条件 | 動作 | 結果 |
| --- | --- | --- |
| `length == 0` | `head = [entry]`、`rest = []`、`length = 1` | 1 チャンク（1 エントリ） |
| `size(chunk) < 512` | チャンクを `take_chunk` で取り出し、位置 `offset` に `insert_at`、`put_chunk` で戻す | そのチャンクが 1 増える |
| `size(chunk) == 512` かつ `chunk == c - 1` かつ `offset == 512` | `rest` の末尾に新しいチャンク `[entry]` を `Vec.push` | 新しいチャンク（1 エントリ）。前のチャンクは 512 のまま |
| `size(chunk) == 512`（上以外） | **先に分割してから挿入**: `middle = 512 / 2 = 256`。`split_off` で `left` = 先頭 256 件、`right` = 後ろ 256 件。`offset <= 256` なら `entry` を `left` の位置 `offset` に、そうでなければ `right` の位置 `offset - 256` に `insert_at`。`left` をチャンク `chunk` に戻し、`right` を `rest` の位置 `chunk`（論理チャンク `chunk + 1`）に `insert_at` | 2 チャンク（257 と 256、または 256 と 257） |

「先に分割」にするのは、満ちたチャンクの `Vec` に 513 件目を `push` すると容量が 1,024 に倍化するのを避けるため（D5）。昇順に挿入する列（`Json` の decode）は、`insert` の行 1 と `add_entry` の第 3 行で、各チャンクを 512 まで詰める。

**4.3 `remove key map`**

| 条件 | 動作 |
| --- | --- |
| `locate` が見つけない | `map` をそのまま返す |
| 見つけ、チャンクのサイズが 1 | `delete_chunk`: `chunk == 0` なら（`rest` が空なら空のマップ、そうでなければ `rest[0]` を `head` に昇格して `rest` から除く）。`chunk >= 1` なら `rest` の位置 `chunk - 1` を `remove_at` で除く |
| 見つけ、サイズが 2 以上 | チャンクを `take_chunk` し、`remove_at` で位置 `offset` を除き、`put_chunk` で戻して、`shrink map chunk` |

**`shrink map k`**（`k` のチャンクが 1 減った後）: `s = size(k)`。
1. `k > 0` かつ `size(k - 1) + s <= 256` なら、チャンク `k` をチャンク `k - 1` に併合（`merge_next map (k - 1)`）。
2. そうでなく、`k + 1 < c` かつ `s + size(k + 1) <= 256` なら、チャンク `k + 1` をチャンク `k` に併合（`merge_next map k`）。
3. どちらでもなければ何もしない。併合は 1 回だけで、I5 が回復する:
   - 削除の前の合計は 257 以上なので、削除の後の合計は 256 以上。併合が起きる組は、合計がちょうど 256 の組だけ。
   - 前と併合した場合、新しいチャンク `q = size(k - 1) + s = 256`。`q` と `k - 2` の合計は `size(k - 2) + size(k - 1) + s`、`k - 2` と `k - 1` の合計が 257 以上なので 257 以上。`q` と `k + 1` の合計は 256 + `size(k + 1)` 以上で 257 以上。
   - 次と併合した場合（前とは合計 257 以上で併合しない）、`q = s + size(k + 1) = 256`。外側の 2 つの組も同様に 257 以上。
   - `delete_chunk`（サイズ 1 のチャンクを除く）では、前後の組が 257 以上なので、前のチャンクと後ろのチャンクは各 256 以上で、新しく隣り合う組は 512 以上。

**4.4 `Set.union left right`**（両辺を消費。重複キーは左を残す）

1. `assert (left_count <= 9223372036854775807 - right_count)`（合計が `i64` に収まる）。
2. 各辺の論理チャンクの列を 1 本の `Vec<Vec<'key>>` にする（`chunk_list`: `insert_at rest 0 head`。空なら空の `Vec`）。
3. 各辺は「現在のチャンク `keys`」を**末尾から** `pop` して消費する。`keys` が空なら、`refill` が `chunk_list` の**最後**のチャンクを `keys` にする。
4. 繰り返し: 両辺を `refill`。両方空なら終了。`order` = 左が空なら -1、右が空なら 1、そうでなければ `compare_keys`（左の末尾キー、右の末尾キー）（`-1`・`0`・`1`。`Ord.lt` を最大 2 回）。`order >= 0` なら左の末尾を `pop` して `current` に `push`、そうでなければ右。取り出したキーを `assert (Eq.eq key key)` で検査。`total` を 1 増やす。`order == 0` なら右の末尾も `pop` して捨てる。`current` が 384 件になったら、`reverse current` を `finished` に `push` して `current` を新しくする。
5. 終了後、`current` が空でなければ `reverse current` を `finished` に `push`。`from_chunks (reverse finished) total`。
6. 結果: 最小のキーを持つ先頭のチャンクが残り（1〜384 件）、ほかは 384 件。`total == 0` なら空のマップ。

**4.5 `Set.intersect left right`・`Set.difference left right`**（借用して左からコピー。`select left right include_equal`、`intersect` が `true`）

1. `left` のキーを昇順に、カーソル `(left_chunk, left_offset)` で 1 つずつ見る（`key_at` が値をコピーして返す）。`right` も同様のカーソル。
2. 各左キーについて、`order` = 右を使い切っていれば -1、そうでなければ `compare_keys key other`。
3. `order <= 0` のとき: `(order == 0) == include_equal` なら `key` を `current` に `push`（`intersect` は `key` が `other` と同じときだけ、`difference` は同じでないときだけ）。左のカーソルを 1 つ進める。
4. `order >= 0` のとき: 右のカーソルを 1 つ進める。（`order == 0` では 3 と 4 の両方を実行する。）
5. `current` が 384 件になったら `finished` に `push`。終了後、残りを `push`。`from_chunks finished total`（昇順のまま。反転しない）。

**4.6 走査**: `fold` は論理チャンク 0..c-1 の順に、各チャンクの位置 0.. の順にコールバックを呼ぶ。`iter` は `Seq.defer` で次の `(chunk, offset)` を遅延して作る（要素の参照 `ref` を返す）。`to_array`・`keys`・`values` は容量 `length` の `Vec` に、`append_*`（チャンクごと）で集めて `Vec.to_array`。

**4.7 `invariants_hold`**: I1〜I5 をすべて検査して `bool` を返す（失敗しても続けて全部調べる。トラップしない）。$O(n)$。

### §5 計算量

`L(c) = (c <= 1 ? 0 : ⌊log2(c - 1)⌋ + 1)`（チャンクの二分探索の `Ord.lt` の最大回数）。

| 操作 | `Ord.lt` の最大回数 | 移動（`Vec.swap`／`pop`／`push`） | 確保 |
| --- | --- | --- | --- |
| `contains_key`・`get`・`at`・`contains` | `L(c) + 11` | なし | なし |
| `insert`（既存キーの置換） | `L(c) + 12` | $O(1)$ | なし |
| `insert`（新規。満ちていない） | `L(c) + 12` | 最大 511 回の `swap`（`insert_at`） | チャンクの容量が足りないとき倍化 |
| `insert`（新規。満ちて分割） | `L(c) + 12` | `split_off` の 256 回の `pop`／`push` と 128 回の `swap`、`insert_at` の最大 256 回の `swap`、`rest` への `insert_at` の最大 `c` 回の `swap` | チャンク 1 つ |
| `insert`（末尾への追加） | 1 | 最後のチャンクが満ちていなければ `push` 1 回 | 満ちていれば新しいチャンク |
| `remove` | `L(c) + 11` | 最大 511 回の `swap`。併合では、`remove_at` の最大 `c` 回の `swap`、`join` の最大 256 回の `pop`／`push` と 128 回の `swap` | 併合でチャンク 1 つを解放 |
| `fold`・`iter`・`keys`・`values`・`to_array` | 0 | $n$ | `to_array` 系は結果の配列 1 つ |
| `union`・`intersect`・`difference` | 最大 `2(n + m)` | $n + m$ | $\lceil (n+m)/384 \rceil$ 個のチャンク |
| `length`・`is_empty`・`chunk_count` | 0 | 0 | なし |

定数 11 は、チャンク内の二分探索の最大 10 回（`⌊log2 512⌋ + 1`）と、見つかったか調べる比較 1 回。分割・併合は、少なくとも 256 回の更新ごとに高々 1 回しか起きない（分割の直後のチャンクは 256〜257、併合の直後は 256 以下で、次の分割・併合まで 256 回以上の挿入・削除が要る）。これは設計上の議論で、文書には書かない（文書は最悪の式だけを書く）。

### §6 比較回数の上界とテストの定数の導出

1. チャンクの二分探索: `d = high - low = c - 1`。1 回ごとに `d` は `⌊d/2⌋` 以下になり、`d >= 1` の間続く。回数は `⌊log2(c - 1)⌋ + 1`（`c == 1` は 0 回）。
2. チャンク内: `stop - start` が 512 以下から始まり、1 回ごとに半分以下。`⌊log2 512⌋ + 1 = 10` 回。
3. 見つかったかの比較: 1 回。`insert` は `beyond_last` の 1 回を足す。→ 検索・削除は `L(c) + 11`、挿入は `L(c) + 12`（フィクスチャの `lookup_bound`・`insert_bound`）。
4. `n >= 128` のとき、`c - 1 <= 2n/257 < n/128` なので `⌊log2(c - 1)⌋ <= ⌊log2 n⌋ - 7`、`L(c) <= ⌊log2 n⌋ - 6`。1 操作は検索・削除が `⌊log2 n⌋ + 5` 以下、挿入が `⌊log2 n⌋ + 6` 以下。
5. n = 2^15 = 32,768（`comparison_bounds 32768`）: 1 操作は挿入 21、検索・削除 20 以下。1 局面の合計の上限は `n × (⌊log2 n⌋ + 6) = 32768 × 21 = 688,128`（`n log2 n = 491,520` の 1.4 倍。フィクスチャの `total_bound`）。実測は挿入 512,688、検索 529,816、削除 482,816（付録 D）で、余裕は約 23〜30%。
6. 集合演算: 1 回の繰り返しで `compare_keys`（`Ord.lt` 最大 2 回）を 1 回呼び、繰り返しは `n + m` 回以下。`n = m = 16384`（`set_algebra_bounds 16384`）の上限は `2 × (16384 + 16384) = 65,536`。
7. **この上界は HEAD の実装も満たす**（二分探索は HEAD も $O(\log n)$ の比較）。テストの目的は、新しい実装が線形探索や余分な比較に退化していないことの検出で、速さの証明ではない。移動量の上界は、I2（チャンクは 512 以下）と I5（`c <= 2n/257 + 1`）を `invariants_hold` が検査することで担保する。時間はテストに使わない。

### §7 NaN と `Eq` の反射性

`locate` と `beyond_last` は、渡されたキーと、比較で踏む格納済みのキーのすべてに `assert (Eq.eq k k)` を行う（NaN のトラップ）。`union` は取り出す各キー、`compare_keys` は両辺のキーを検査する。`Map.singleton`・`Set.singleton` は検査しない（HEAD と同じ。格納済みの NaN は、後の `locate` が踏むときにトラップする）。
この検査の位置を変えない（`nan_query`・`nan_stored` の 2 つのトラップがこれを守る）。

### §8 所有権・借用・drop

- `insert`・`remove`・`union` は引数のマップを消費し、チャンクは `take_chunk`（穴を残す）と `put_chunk`（穴を埋める）の組で動かす。クローンはしない。穴を埋め忘れると `invariants_hold` が偽になるか、添字のトラップになる。
- `Map.at` が返す `ref` は、マップの借用に束縛される（`chunk_of` が `ref` を返す）。借用が生きている間の `insert`・`remove` は、利用者側で `E1014`（`tests/map_set.rs` に既存）。
- `Map` の drop は `head` の要素（昇順）、`rest` の各チャンクの要素（昇順）の順で、全体として昇順。HEAD の 1 本の `Vec` と同じ順序（`tests/user_drop.mjs`）。

### §9 診断用の公開関数

| モジュール | シグネチャ | 意味 |
| --- | --- | --- |
| `Map` | `def chunk_count :: ref Map<'key, 'value> -> i64` | エントリを持つチャンクの個数（空は 0） |
| `Map` | `def invariants_hold :: Ord<'key> => ref Map<'key, 'value> -> bool` | I1〜I5 がすべて成り立つか |
| `Set` | `def chunk_count :: ref Set<'key> -> i64` | 同上 |
| `Set` | `def invariants_hold :: Ord<'key> => ref Set<'key> -> bool` | 同上 |

前例は `HashMap.longest_probe`（`docs/language.md` で「内部診断用」と書く）。`std` のファイルにはテストを書けない（`E1018`）ので、テストが検査できるように公開にする（D7）。値（チャンクの個数）は実装の詳細で、将来の版で変わりうると文書に書く。

### §10 互換性

- **診断コード**: 新しいコードはない。`E1022`（不透明）の意味は不変。
- **`docs/stability.md`**: std の公開関数の追加は互換（`tsuzuri api-diff` で `non-breaking`）。既存の関数の型・制約は不変。
- **IR・ABI**: `Map`・`Set` のレイアウトが 1 つの `Vec` から `{Vec, Vec<Vec>, i64}` に変わる。不透明（`E1022`）で C ABI にも出せないので、利用者のコードは影響を受けない。影響を受けるのはデバッガ表示（LLDB・natvis）だけで、Phase 1 で直す。
- **WASM**: import を足さない。`-O0`・`-O3` の native と WASM を E2E で確かめる。
- **edition**: 変更なし。予約語・新しい構文なし。

## 設計

### 設計 1: 関数の一覧（`std/Map.tz` と `std/Set.tz`。付録 A・B の関数名のまま）

擬似コードは、検証済みの Tsuzuri のコード（付録 A・B）そのものである。読む順番と注意を示す（`Set` は `Entry` がなく、`entries` は `keys` と書く）。

| 関数 | 公開 | 責務・注意 |
| --- | --- | --- |
| `take`・`put`・`insert_at`・`remove_at`・`reverse`・`split_off`・`join` | private | `Vec` の足りない操作（「現状」の表）。`take` と `put` は同じ `i` で必ず対にする |
| `chunk_total`・`chunk_of`・`locate` | private | §1・§4.1。`locate` は見つからないとき `offset` が負（`-1 - 挿入位置`） |
| `chunk_count`・`invariants_hold` | public | 診断用（§9）。`invariants_hold` は §4.7 |
| `contains_key`・`get`・`at`・`contains` | public | `locate` の結果を使う。`at` は `assert (offset >= 0)` でトラップして `ref` を返す |
| `take_chunk`・`put_chunk` | private | 「穴」の規則（設計 2） |
| `replace_value`（`Map` だけ）・`add_entry`（`Set` は `add_key`）・`beyond_last` | private | §4.2。`replace_value` は古いキーを残す |
| `insert`・`remove` | public | §4.2・§4.3 |
| `merge_next`・`shrink`・`delete_chunk` | private | §4.3。併合は高々 1 回 |
| `append_pairs`・`append_keys`・`append_values`（`Set` は `append_keys`）・`key_at`（`Set` だけ） | private | 要素を、**引数として受け取った `ref`** から読んで値で返す／出力に足す（`E1014` の回避。「落とし穴」1） |
| `to_array`・`keys`・`values`・`fold`・`iter_from`・`iter` | public／private | §4.6 |
| `compare_keys`・`chunk_list`・`refill`・`from_chunks`・`chunk_size`・`select`（`Set` だけ） | private | §4.4・§4.5 |
| `union`・`intersect`・`difference`（`Set` だけ） | public | §4.4・§4.5 |

### 設計 2: 「穴」の規則（`take_chunk` と `put_chunk`）

チャンクを動かす更新は、`take_chunk map k` → チャンクの中を直す → `put_chunk hole k chunk count` の 3 段で、チャンクのコピーとクローンを作らない。`k == 0` の穴は `head = Vec.empty()`。`k >= 1` の穴は、`take rest (k - 1)` が末尾のチャンクを位置 `k - 1` へ動かす（`rest` の順序が一時的に壊れる）ので、`put rest (k - 1) chunk` が元に戻す。
**穴がある間（`take_chunk` と `put_chunk` の間）は、`chunk_of`・`chunk_total` を呼ばない**。`add_entry` の分割は、先に `put_chunk hole chunk left (count + 1)` で穴を埋めてから、`insert_at restored.rest chunk right` で `right` を差し込む。`merge_next` は、先に `remove_at map.rest index`（論理チャンク `index + 1` を外す）を行ってから `take_chunk` する。

### 設計 3: 容量とメモリ

各チャンクの `Vec` の容量は、`push` の倍化で最大 512 になる。分割の左半分は容量 512 のまま残り（縮めない）、右半分は `Vec.with_capacity (size - at)`（256）で作る。`to_array` 系は容量 `length` で 1 回だけ確保する。
チャンクの平均の詰まり具合はランダム挿入で約 72%（n = 10^6 で 2,726 チャンク、1 チャンク平均 367）。`Entry<i64, i64>` は 16 バイトで、満ちたチャンクは 8 KiB。ピーク RSS は測っていない（実装者が `docs/benchmarks.md` の節に測ってよいが、必須ではない）。

### 設計 4: デバッガ表示

`Map`・`Set` のレコードのフィールドが `["head", "rest", "length"]` になる。LLDB の `is_collection` はこのフィールド名の並びで判定する（旧 `["entries"]` の受理は残さない。D10）。
`CollectionProvider` は、`head` の先頭の番地と長さ、`rest` の先頭の番地と個数だけを `update` で読み、`address(index)` が「子 `index` を含むチャンク」まで `rest[0]`、`rest[1]`… のヘッダー（`data` と `length`）を**必要な分だけ**読む。1,000,000 エントリでも、LLDB が要求する最初の 256 個だけを読む。
natvis は `<CustomListItems>` で `head` の要素、続いて `rest` の各チャンクの要素を順に出す（付録 2 F。Windows 専用で未検証）。

### 設計 5: 変更表

**Phase 1**

| ファイル | 箇所 | 変更 |
| --- | --- | --- |
| `std/Map.tz`・`std/Set.tz` | 全体 | 付録 A・B の全文に置き換える（G34・G35 の `///` の追記が既にあれば保つ。手順 3） |
| `tests/fixtures/map_set_chunks/Main.tz`（新規） | 全体 | 付録 2 A |
| `tests/features.mjs` | suite `map_set` の `inspect` の 1 行、新しい suite `map_set_chunks` | 付録 2 B1・B2 |
| `tests/map_set_chunks.rs`（新規） | 全体 | 付録 2 C |
| `tests/map_set.rs` | `ordered_containers_are_opaque_noncopy_owned_values` の `for source in [...]` の末尾 | 付録 2 D の 6 件 |
| `scripts/lldb/tsuzuri_lldb.py` | `is_collection`・`collection_summary`・`CollectionProvider` | 付録 2 E |
| `src/runtime/tsuzuri.natvis` | `Map`・`Set` の 2 つの `<Type>` | 付録 2 F |
| `tests/debugger.mjs` | `extras` の節の後 | 付録 2 G |
| `std/Json.tz`・`tests/fixtures/json/Main.tz` | コメント 2 か所 | 付録 2 I6 |
| 文書 | 付録 2 I | `README.md`・`docs/language.md`・`docs/architecture.md`・`map.md`・`set.md`・`hashmap.md`・`array.md`・`json.md`・`docs/benchmarks.md` の 1 行 |

**Phase 2**: `benchmarks/ordered/Main.tz`（新規。付録 C）、`docs/benchmarks.md`（節の追加）。

## 前提とする他チケットのインターフェース

前提とするチケットはない。ほかのチケットとの関係だけを書く。

- **G35**（生成物による文書の単一情報源化）: `@complexity` タグを入れるときは、仕様 §5 の表の値（検索 $O(\log n)$ 回の比較、更新 $O(\log n)$ 回の比較 + 1 チャンクの移動）を使う。G35 が未実装の間は、本チケットの文書の手書きの表が正。**G35 が済んでいるなら**、`map.md`・`set.md` の計算量の表は `<!-- generated:complexity:Map -->`（`Set`）のブロックで、本体は手で直さない: タグを §5 に合わせ、`node scripts/gen-docs.mjs` で表を作り直し、ブロックの外の「連続バッファ」の 1 文をチャンクの説明に直す。
- **F14**（構造的な性能表明）: `Map`・`Set` の確保の回数を表明するときは、`insert` が新しいチャンクの確保（分割・末尾への追加）と、容量の倍化を起こすことを前提にする。本チケットは確保の回数を約束しない（検索が確保しないことだけを約束する）。
- **G26・A20**（facts・効果の要約）: `std/Map.tz`・`std/Set.tz` は `assert` を使うので `trap`、`Vec` の確保を使うので `alloc` を要約に持つ。HEAD でも同じで、変わらない。
- **A19**（純粋関数）: `Map`・`Set` の関数は副作用を持たない（`Time`・`IO` を呼ばない）。本チケットで変わらない。

## 実装手順

各手順の「確認」は、そこまでの手順を含めた状態で実行する。手順 3 と 4 は 1 つのコミットにする（手順 3 だけだと、`Map`・`Set` の表示形式が変わって `tests/debugger.mjs` が赤になる）。

### Phase 1

**手順 1: ベースラインを取る**
- 変更: なし。
- 内容: `cargo build --release --locked` と `cargo test --locked` を実行し、結果の行（`test result:`）を控える。Node のテストを実行し、件数を控える。
- 確認:
  ```sh
  node tests/features.mjs target/release/tsuzuri map_set              # 26 cases passed
  node tests/features.mjs target/release/tsuzuri json                 # 165 cases passed
  node tests/features.mjs target/release/tsuzuri iteration_protocol   # 26 cases passed
  node tests/user_drop.mjs target/release/tsuzuri                     # -O0 と -O3 の 2 行
  node tests/debugger.mjs target/release/tsuzuri                      # LLDB が使える環境だけ。成功は `debugger: LLDB formatters, names and stepping verified`
  ```

**手順 2: テストを先に足す（テスト先行。ここは赤でよい）**
- 変更: `tests/fixtures/map_set_chunks/Main.tz`（新規）、`tests/features.mjs`（suite `map_set_chunks`）、`tests/map_set_chunks.rs`（新規）。
- 内容: 付録 2 A のコードブロックを、そのまま新しいファイルにする（空白 4 つのインデント）。付録 2 B2 の suite を、`map_set` suite の閉じ括弧 `},` と `arena: {` の間に足す。付録 2 C の Rust を新しいファイルにする。
- 確認:
  ```sh
  target/release/tsuzuri test tests/fixtures/map_set_chunks    # 終了コード 1。error[E1002]: module 'Map' has no function or union case 'invariants_hold'
  node tests/features.mjs target/release/tsuzuri map_set_chunks # 同じ理由で失敗
  ```
  どちらも、HEAD の `std` には診断用の関数がないために赤になる（想定どおり）。この赤を、手順 3 で緑にする。

**手順 3: `std/Map.tz`・`std/Set.tz` を差し替える**
- 変更: `std/Map.tz`、`std/Set.tz`、`tests/features.mjs`（suite `map_set` の `inspect` の 1 行）、`tests/map_set.rs`（6 件）。
- 内容: (1) 付録 A・B のコードブロックで、2 つのファイルを丸ごと置き換える（**TAB のインデントのまま**）。(1b) **G34 の `///` の例や G35 の `@complexity`・`@since`・`@see` が `std/Map.tz`・`std/Set.tz` に既にあるなら、置き換える前に公開宣言ごとの `///` ブロックを控え**、置いた後に同じ名前の宣言の前へ足し直す（付録の `///` の本文の後ろ。例とタグの位置は G34・G35 の規則）。`chunk_count`・`invariants_hold` など付録で新しい宣言には、付録の `///` のままにし、G35 のタグが既にあるなら新しく付ける。`@complexity` の値は仕様 §5 に合わせる。(2) 付録 2 B1 の `inspect` を直す。(3) 付録 2 D の 6 件を足す。
- 確認: 先に、置いたファイルが付録のコードブロックと（`///` の行を除いて）一致することを確かめる（次の `python3` を、行頭に空白を付けずにそのまま実行する。2 行とも `identical` が出る）。

```sh
python3 - <<'EOF'
import re
def code(text):
    return ''.join(line for line in text.splitlines(True) if not line.lstrip().startswith('///'))
text = open('_features/_completed/C12-appendix.md', encoding='utf-8').read()
for marker in ('std/Map.tz', 'std/Set.tz'):
    block = re.search(r'<!-- file: ' + re.escape(marker) + r' -->\n```text\n(.*?)\n```\n', text, re.S).group(1) + '\n'
    print(marker, 'identical' if code(block) == code(open(marker, encoding='utf-8').read()) else 'DIFFERENT')
EOF
```

続けて、次を実行する（コメントが期待する結果）。

```sh
cargo build --release --locked        # std は埋め込まれるので、毎回やり直す（約 3 分）
node tests/features.mjs target/release/tsuzuri map_set_chunks   # map_set_chunks: native/WASM at O0/O3 passed ・ features: 29 cases passed
node tests/features.mjs target/release/tsuzuri map_set          # 26 cases passed（期待値を変えていない）
target/release/tsuzuri test tests/fixtures/map_set_chunks       # 5 passed; 0 failed; 0 ignored
target/release/tsuzuri test tests/fixtures/map_set_chunks -O3   # 5 passed; 0 failed; 0 ignored
node tests/features.mjs target/release/tsuzuri json             # 165 cases passed
node tests/features.mjs target/release/tsuzuri iteration_protocol   # 26 cases passed
node tests/user_drop.mjs target/release/tsuzuri                 # -O0 と -O3 の 2 行
cargo test --locked --test map_set --test map_set_chunks        # running 3 tests（map_set）と running 1 test（map_set_chunks）、すべて ok
cargo test --locked honors_the_exact_specialization_limit       # running 1 test ... ok（0 件でないこと）
cargo test --locked hash_containers_leave_the_specialization_budget_to_users   # running 1 test ... ok
```

**手順 4: デバッガ表示を更新する**
- 変更: `scripts/lldb/tsuzuri_lldb.py`、`src/runtime/tsuzuri.natvis`、`tests/debugger.mjs`。
- 内容: 付録 2 E の 3 つの定義で `is_collection`・`collection_summary`・`CollectionProvider` を置き換える。付録 2 F の 2 つの `<Type>` で natvis を置き換える。付録 2 G を `tests/debugger.mjs` に足す。
- 確認:
  ```sh
  python3 -c "import xml.dom.minidom as m; m.parse('src/runtime/tsuzuri.natvis'); print('natvis ok')"
  python3 target/c12-mock/mock_test.py "$PWD/scripts/lldb"   # 付録 2 H を target/c12-mock/mock_test.py に作って実行（`target/` は git 管理外）。collection provider: ok
  cargo test --locked pdb_links_write_a_named_pdb_and_embed_the_natvis_views   # running 1 test ... ok
  node tests/debugger.mjs target/release/tsuzuri      # LLDB が使える環境で。debugger: LLDB formatters, names and stepping verified
  ```
  LLDB が使えない環境では、最後の 1 行を実行できない。停止条件 6 に従う（完了にしない）。

**手順 5: 文書とコメントを更新する**
- 変更: 付録 2 I1〜I6 の全ファイル。
- 内容: 各ページの行を、付録 2 I の文面に置き換える。`std/Json.tz` と `tests/fixtures/json/Main.tz` はコメントだけ（コードは変えない）。
- 確認:
  ```sh
  node scripts/check-docs.mjs _tsuzuri/language-reference/built-in-types-and-modules/map.md _tsuzuri/language-reference/built-in-types-and-modules/set.md _tsuzuri/language-reference/built-in-types-and-modules/hashmap.md _tsuzuri/language-reference/built-in-types-and-modules/array.md _tsuzuri/language-reference/built-in-types-and-modules/json.md
  ```
  `check-docs.mjs` は `_tsuzuri/language-reference` のページだけを受け付ける（`README.md`・`docs/*.md` を渡すと `Requested page not found`）。ページごとの出力の `Docs:` の行は、HEAD（編集前）で次のとおりで、**編集後も同じ**（例もリンクも変えない）。
  `map.md`: `Docs: 1 pages, 6 links, 5 checked examples, 10 native runs (O0/O3), 0 test projects.`、`set.md`: `6 links, 4 checked examples, 8 native runs`、`hashmap.md`: `8 links, 5 checked examples, 10 native runs`、`array.md`: `9 links, 11 checked examples, 22 native runs`、`json.md`: `8 links, 10 checked examples, 20 native runs`。
  ```sh
  grep -rn "平衡二分探索木" README.md docs/language.md docs/architecture.md _tsuzuri/language-reference   # 0 件
  grep -n "連続バッファ" _tsuzuri/language-reference/built-in-types-and-modules/map.md _tsuzuri/language-reference/built-in-types-and-modules/set.md _tsuzuri/language-reference/built-in-types-and-modules/array.md _tsuzuri/language-reference/built-in-types-and-modules/hashmap.md   # Map・Set について書いた行が 0 件（Vec や配列の説明は残ってよい）
  node tests/features.mjs target/release/tsuzuri json             # コメントだけの変更なので 165 cases passed のまま
  ```

**手順 6: 全体の確認**
- 変更: なし。
- 確認: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`（Rust は `tests/map_set_chunks.rs` だけを足した）、`cargo test --locked` の全件、手順 3 の Node の 5 コマンド、`git status` が「主な影響ファイル」だけを示すこと（停止条件 12）。

### Phase 2

**手順 7: ベンチマークの program を足す**
- 変更: `benchmarks/ordered/Main.tz`（新規）。
- 内容: 付録 C のコードブロックをそのまま新しいファイルにする（空白 4 つのインデント）。
- 確認:
  ```sh
  target/release/tsuzuri fmt --check benchmarks/ordered/Main.tz      # 出力なし・終了コード 0
  target/release/tsuzuri build benchmarks/ordered -O3 -o target/ordered-after
  for w in insert_random lookup_random remove_random; do ./target/ordered-after $w 10000; done   # 各 1 行: <workload> 10000 <checksum> <ns>
  ./target/ordered-after insert_random 10000       # checksum は 10000（キーが全部異なるので件数と同じ）
  ```
  期待する checksum（手計算）: `insert_random` は件数、`remove_random` は 0、`insert_ascending` は件数。`lookup_random`・`update_counter`・`set_algebra` は、旧実装（HEAD の `std`）と同じ値になることで確かめる（次の手順）。

**手順 8: before／after を測り、`docs/benchmarks.md` に節を足す**
- 変更: `docs/benchmarks.md`（節「順序付きコンテナ（C12）」を、節「静的データ（D11 Phase 2）」の後、「性能指標の記録」の前に追加）。
- 内容: (1) 旧実装のコンパイラを用意する: `git stash` ではなく、**Phase 1 より前のコミット**を別の作業木に取り出して `cargo build --release` したものを `target/baseline/tsuzuri` とする（取り出し方は `git worktree add ../tsuzuri-c12-base <Phase 1 の前のコミット>`）。(2) 両方で `benchmarks/ordered` を `-O3` でビルドし、6 つの workload × 大きさを交互に 5 回ずつ走らせ、中央値と範囲を表にする。(3) 節に、program の場所、使い方（`exe <workload> <size>`）、workload の表（付録 C の 6 つ）、測定環境（CPU・OS・日付・ロードアベレージ）、before／after の表、「参考値で、CI の閾値ではない」こと、検索と小さいマップの更新が遅くなる箇所を正直に書く。
- 確認:
  ```sh
  for w in insert_random lookup_random remove_random insert_ascending update_counter set_algebra; do
    ./target/ordered-before $w 100000; ./target/ordered-after $w 100000
  done      # 各 workload で、before と after の checksum（3 つ目の数）が一致
  ```
  `docs/benchmarks.md` は `check-docs.mjs` の対象外なので、見出しの順（新しい節が `静的データ（D11 Phase 2）` の次で `性能指標の記録` の前）と、表の数字が測定の出力と一致することを目視で確かめる。
  停止条件 10 を確かめる（`insert_random 100000` が before の 5 倍以上速い）。

## テスト計画

期待値は仕様 §4 の規則から手で導出した値で、コンパイラの出力の写しではない。

### 新しいテスト

| テスト | 入力 | 期待値と導出 |
| --- | --- | --- |
| suite `map_set_chunks`（`tests/features.mjs`）の `ascending [0, 512, 513, 1024, 1025]` | 0 から昇順に N 件 | 0、1、2、2、3。昇順は各チャンクを 512 まで詰め（§4.2 の第 3 行）、513 件目で新しいチャンクを始める（⌈N/512⌉） |
| `descending [512, 513, 768, 769]` | 降順に N 件 | 1、2、2、3。先頭のチャンクが満ちたら 256 + 256 に分けて新しいキーを左に入れる。513 件目で 257 + 256 の 2 チャンク。先頭が再び 512 になる 768 件目まで 2 チャンク。769 件目で再び分割して 3 |
| `middle_split` | 偶数キー 512 個（1 チャンク）、続けてキー 1 | 1002（`before` 1 × 1000 + `after` 2）。キー 1 は満ちたチャンクの位置 1 に入るので、257 + 256 に分割 |
| `merge_after_removals` | 513 件（257 + 256）から、`2 mod 4` のキー 256 個、続けてキー 0 を削除 | 22001256。256 個削除後は 129 + 128 = 257 で併合しない（`before` 2）。キー 0 の削除後は 128 + 128 = 256 で併合（`after` 1）、残り 256 件。2×10^7 + 2×10^6 + 1×10^3 + 256 |
| `head_deleted` | 昇順 768 件（512 + 256）から先頭の 512 個を削除 | 201256。先頭のサイズ + 256 が 256 以下にならず併合しない。最後の 1 個でサイズ 1 のチャンクが `delete_chunk` され、次が昇格して 1 チャンク、残り 256 件。2×10^5 + 1×10^3 + 256 |
| `tail_deleted` | 昇順 513 件（512 + 1）から、キー 512 を削除 | 2001。サイズ 1 のチャンクが消えて 1 チャンク |
| `set_algebra_chunks` | `A = 0..999`、`B = 500..1499` | 40202。和集合 1,500 件は ⌈1500/384⌉ = 4 チャンク、積集合 500 件は 2（384 + 116）、差集合 500 件は 2。4×10^4 + 2×10^2 + 2 |
| `comparison_bounds [32768]` | ランダムな 32,768 キーの挿入・検索・削除 | 0。1 操作と 1 局面の `Ord.lt` の回数が §6 の上界以下（超えたときは 11〜13、21〜23） |
| `set_algebra_bounds [16384]` | `2i` と `3i` の 16,384 件ずつ | 0。3 つの集合演算が各 65,536 回以下（§6 の 6） |
| `borrowed_keys_and_values` | 借用を持つキー・値で全関数を呼ぶ | 33。`use_map` = 配列 3 つの長さ 2 + 2 + 2 に、`fold` の 5（(1+1) + (2+1)）、`iter` の 5、`remove` 後の長さ 1、`at` の 1、`contains_key` の 1、`get` の 1 を足した 20。`use_set` = 4（2 + 1 + 1）+ 3 + 3 + 2（和集合の長さ）+ 1 = 13 |
| `model_map`・`model_set`・`model_algebra` × profile 0〜3、seed 11（12 件） | 擬似乱数の操作列（3,000〜12,000 件） | すべて 1。キーで引く表の参照モデルと、32 操作ごと・最後の `invariants_hold`・全操作の `contains_key`・`get` が一致 |
| fixture の `test` 5 件（`tsuzuri test tests/fixtures/map_set_chunks`） | 短い操作列（最大 64 件、キー 0〜49）を 100 回（`Map`・`Set`）、長い列（seed 0〜10^6、profile 0〜3）を 12 回（`Map`・`Set`）と 8 回（集合演算） | モデルと一致。出力の最後が `5 passed; 0 failed; 0 ignored` |
| `tests/map_set_chunks.rs::property_tests_pass_at_o0_and_o3` | 上の 5 件を `-O0`・`-O3` で | 終了コード 0 と `5 passed; 0 failed; 0 ignored` |
| `tests/map_set.rs::ordered_containers_are_opaque_noncopy_owned_values`（6 件追加） | 新しいフィールド名（`head`・`rest`・`length`）の構築・参照・更新・パターン | すべて `E1022` |
| `tests/debugger.mjs`（付録 2 G。1 節追加） | 513 個の昇順の `Set<i64>` | `many = size=513 {`、`[0] = 0`、`many[511] = 511`、`many[512] = 512`（2 つ目のチャンクの先頭） |

### 既存テストへの影響

| テスト | 影響 |
| --- | --- |
| suite `map_set`（26 件 + トラップ 3 件） | 期待値は不変。`inspect` の正規表現だけを直す（`lower_bound`・`found` がなくなるため。直さないと検査する関数が `get`・`at`・`contains_key` だけに減る） |
| `tests/map_set.rs` の既存 3 関数 | なし。`Map`・`Set` を含む 28 ソースの結果（受理か最初の診断コード）が HEAD と同じ（実測） |
| `json`（165）・`iteration_protocol`（26）・`tests/user_drop.mjs` | なし（付録 A・B のコンパイラで同じ件数・同じ出力。実測） |
| `src/polymorph.rs` の 2 テスト | 不変の見込み。**筆者は `cargo test` を実行できなかった**ので、手順 3 で確かめる |
| IR を文字列で比べるテスト | なし（連番に依存するテストは 0 件。「現状」） |
| `tests/debugger.mjs` の既存の期待 | 不変（`size=1`・`size=3`・`[0] = (key = 1, value = 10)` は 1 チャンクの場合） |

筆者は、`std` に 4 種類の変異（M3: `delete_chunk` が先頭のチャンクの昇格をしない、M5: チャンク内の比較の向きの反転、M7: `put` が `swap` しない、M8: 常に併合）を入れて、検出するテストを調べた。性質テストは M3 を見逃し、境界テスト（`head_deleted`）が見つけた。M8 は境界テスト（`merge_after_removals`）だけが見つけた（付録 D、決定事項 D9）。

### 決定性・資源上限・拒否

- IR の決定性: 既存の `tests/map_set.rs` の `emit_target` の 2 回比較が、そのまま `Map`・`Set` を覆う。
- 資源上限: 新しい解析はなく、`E1017` の上限は増えない。計算量の上限は §6 の比較回数のテストが担う。
- 拒否: `E1022`（6 件追加）、既存の `E1005`・`E1008`・`E1012`・`E1013`・`E1014`（結果は不変）。

## ドキュメント

更新するページと文面は付録 2 I にある（置き換える行の行頭と新しい文）。

| ページ | 内容 |
| --- | --- |
| `README.md` | `Map`・`Set` の説明（「平衡二分探索木」は誤り）を、チャンクの列と計算量に直す（I1） |
| `docs/language.md`（節「Map / Set」） | API の列挙に診断用の 2 関数、計算量の 2 行を内部構造・計算量・集合演算の 3 行に（I2） |
| `docs/architecture.md`（段落「順序付きコンテナ」） | レイアウト、`locate`、更新、`union`・`intersect`・`difference`、検証コマンドの 4 行（I3） |
| `_tsuzuri/language-reference/built-in-types-and-modules/map.md`・`set.md` | 導入、ポイント、API 表、計算量の表、まとめ、診断用の小節（I4・I5） |
| `hashmap.md`・`array.md`・`json.md` | 「連続バッファ」と `O(n^2)` の記述（I6） |
| `std/Json.tz`・`tests/fixtures/json/Main.tz` | コメントだけ（I6） |
| `docs/benchmarks.md` | Phase 1 は表の 1 行（I6）。Phase 2 は節「順序付きコンテナ（C12）」 |

検証: 手順 5 の `node scripts/check-docs.mjs`（5 ページ。`Docs:` の行が編集前と同じ）と `grep`。`_features/README.md` と GUIDE の台帳は、本チケットでは更新しない（統括者が更新する。完了報告に「台帳に追記する候補」を書く）。
新しい例を足さない（計算量と構造の記述だけなので、`check-docs.mjs` が走らせる例の数は変わらない）。

## 受け入れ条件

**Phase 1**
- [x] 手順 3 の `std/Map.tz`・`std/Set.tz` が付録 A・B と（`///` の行を除いて）一致する（手順 3 の `python3` の 2 行が `identical`）。
- [x] `cargo test --locked` が全件通る。`cargo test --locked --test map_set --test map_set_chunks` は `running 3 tests` と `running 1 test` で、すべて ok。
- [x] `node tests/features.mjs target/release/tsuzuri` の `map_set`（26）・`map_set_chunks`（29）・`json`（165）・`iteration_protocol`（26）が通る。
- [x] `target/release/tsuzuri test tests/fixtures/map_set_chunks`（`-O0` と `-O3`）が `5 passed; 0 failed; 0 ignored`。
- [x] `node tests/user_drop.mjs target/release/tsuzuri` が通る。`node tests/debugger.mjs target/release/tsuzuri` が通る（LLDB が使える環境。使えない場合は停止条件 6 のとおり未完了）。
- [x] natvis が整形式の XML。モック試験が `collection provider: ok`。
- [x] `src/` の Rust・`src/stdlib.rs`・`Vec` の組み込みに差分がない（`git diff --stat -- src` が `src/runtime/tsuzuri.natvis` だけ）。
- [x] 文書（付録 2 I）が更新され、`check-docs.mjs` の `Docs:` の行が編集前と同じ。`grep "平衡二分探索木"` が 0 件。
- [x] `cargo fmt --all -- --check` と `cargo clippy --all-targets -- -D warnings` が通る。

**Phase 2**
- [x] `benchmarks/ordered/Main.tz` が付録 C と一致し、`tsuzuri fmt --check` が通り、`tsuzuri build -O3` が通る。
- [x] 6 つの workload で、旧実装と新実装の checksum が一致する。
- [x] `docs/benchmarks.md` に節「順序付きコンテナ（C12）」があり、測定環境・日付・before／after の表・「参考値で閾値ではない」の一文・遅くなる箇所の記述がある。

## 落とし穴

1. **借用を持つキー・値が `E1014` になる**。`Probe { value: i64, hits: ref Atomic<i64> }` のような `ref` を持つキー・値を、ローカルの `ref` 変数（`let keys = chunk_of set chunk`）から `Copy` で読んで `Vec` に集め、その `Vec` を動かすと、`cannot move an exclusive reference while it is reborrowed` になる（HEAD の `std` は 1 本の `Vec` なので問題がなかった）。
   直し方は、付録 A・B の `append_*`・`key_at` のように、読む関数が**引数として受け取った `ref`**から読む形にすること。検出: `target/release/tsuzuri check tests/fixtures/map_set_chunks`（`borrowed_keys_and_values` が通らなくなる）。
2. **評価順序と移動**: `f x (g (ref x))` は、引数の評価の前に `x` を動かす。`take_chunk map chunk` の前に必要な値（`count`・`size`・`total`）を `let` で取り出しておく。検出: `E1012`（moved value）。
3. **穴を埋め忘れる／穴の間に読む**（設計 2）。検出: `invariants_hold` が偽になる、または添字のトラップ（`map_set_chunks` の model 系が 0 を返す）。
4. **TAB と空白**: `std/Map.tz`・`std/Set.tz` は TAB、fixture とベンチマークは空白 4 つ。付録から写すとき、変換しない。検出: 手順 3 の `python3` の `identical`。`tsuzuri fmt` を `std` に掛けない。
5. **コンパイラの再ビルドを忘れる**: `std` は `include_str!` でコンパイラに埋め込まれる。古い `target/release/tsuzuri` で Node のテストを走らせると、`map_set_chunks` が `E1002` のまま赤になる。検出: 手順 3 の `cargo build --release --locked` の後に実行する。
6. **`inspect` の正規表現**: 関数名が変わると、`lookups.length > 0` が通ったまま検査の対象が減る。検出: `grep -n "locate|chunk_of|chunk_total" tests/features.mjs` が `map_set` と `map_set_chunks` の 2 行を出す。
7. **IR の連番**: `Map`・`Set` を使わないプログラムでも、`$intrinsic.Display.display.<N>` の `N` が変わる。IR を比べる作業では、連番を正規化してから比べる（`sed -E 's/\.[0-9]+\b/.N/g'`）。
8. **LLDB が `run` で止まる環境**: `tests/debugger.mjs` を実行できない。検出: `lldb -b -o 'target create <exe>' -o run` が戻らない。モック試験は代わりにならない（停止条件 6）。
9. **ベンチマークの揺れ**: 5 回の中央値と範囲、ロードアベレージを必ず併記し、1 回の値で「遅くなった」「速くなった」と書かない。速度の閾値をテスト・CI に入れない。
10. **`cargo test` の件数 0**: 名前のフィルターは 0 件でも成功する。`running N tests` の `N` を確かめる。
11. **fixture を直すときの言語の癖**: `let (a, b) = ...` は書けない（`match` を使う）。引数なしの関数は `f()` と呼ぶ。`$"...{a.length}..."` は `E1013`（先に `let` に束縛する）。`Array.update_at` は配列を消費する（`ref` ではない）。`Gen.array_up_to` は 100 件未満の配列しか作らないので、長い列は seed から作る擬似乱数で生成し、`Gen` は `(seed, profile)` を選ぶ。`Maybe` には `Eq` がない。

## 対象外

- `Map`・`Set` の新しい API（`range`・`first`・`last`・`pop_first`・`retain`・`extend`・`with_capacity`）と、`Vec` への `insert`・`remove`・`split_off` の追加。
- 木（B 木・赤黒木・skip list）への変更、並列・SIMD・GPU による集合演算、ハッシュ表との統合。
- `HashMap`・`HashSet` の変更。`Json`・`Cbor` の変更（コメントを除く）。
- メモリ使用量（ピーク RSS）の目標と測定（Phase 2 の節に測ってよいが約束しない）。
- 速度の合否の CI への導入。償却計算量の文書化（D13）。

## 決定事項

すべて承認済み（D-50: 人間の包括許可 2026-10-18）。

- **D1 構造は「`head` + `rest` + `length`」の 2 段のチャンク化ソート配列**。理由: チャンクが 1 つの小さいマップで、`Vec<Vec>` の添字という 1 段の間接を避けると、検索が HEAD の約 1.04 倍、置換が約 1.2 倍に収まる。却下: (a) 種の「単一の `chunks: Vec<Chunk>`」: 小さいマップの検索・置換が 1.6〜2.9 倍遅くなった（試作の実測）。(b) 平らな `Vec` とチャンクの 2 形式を大きさで切り替える: コードが 2 倍。(c) B 木・赤黒木: コード量が大きく、定数がチャンクより重い。
- **D2 `Set` は同じ構造を別に持つ**（`Map<'key, unit>` の薄い包みにしない）。理由: `Entry<i64, unit>` は 16 バイトで、キーだけの 8 バイトの 2 倍。`union`・`intersect`・`difference` は `Set` 専用のアルゴリズム。却下: 薄い包み（コードは減るがメモリが 2 倍で、`Entry` の間接が 1 段増える）。
- **D3 定数は `MaxChunk` 512・`MergeLimit` 256・`BulkFill` 384**。理由: B = 128・256・512・1024・2048 の振り（付録 D）。B = 256 はランダム挿入で 12〜19% 速かったが、257〜512 エントリのマップが 1 チャンクの経路で済む利点と種の値を優先した。B = 1024・2048 は小さいマップの置換が遅く、B = 128 はチャンクが増えて比較が増える。`MergeLimit = MaxChunk / 2` は、併合後（256 以下）から次の分割まで 256 回以上の更新が要るようにするため。`BulkFill = 384` は `MaxChunk` の 3/4 で、作った直後に 128 回挿入できる余白を残す。却下: 種の「`MAX / 4` 未満で併合し、併合後 `3/4` 以下のときだけ」。後者の条件が前者に含まれて意味を持たず、隣り合う 2 つの合計の下限が 128 では `c <= n/64 + 1` と緩くなる。
- **D4 併合の規則は I5（隣り合う 2 つの合計が 256 以下になったら併合）**。理由: 1 回の併合で I5 が回復することを証明できる（§4.3）。却下: (a) チャンクが小さくなったら常に隣と併合する: 併合の直後に分割が起きる往復で遅くなる（変異 M8 と同じ状態）。(b) 併合しない: 削除を繰り返すと小さいチャンクが溜まり、`c` の上界（検索の比較回数）が崩れる。
- **D5 分割は「先に分割してから挿入」**。理由: 満ちたチャンクの `Vec` に 513 件目を `push` すると容量が 1,024 に倍化し、8 KiB を無駄にして要素をコピーする。却下: 挿入してから分割。
- **D6 末尾への追加の高速路（`beyond_last`）は `insert` だけに置く**。理由: 昇順の挿入（`Json` の decode が整列してから追加する、ループの添字を鍵にする）が最も多く、ランダム挿入への代償は 1 回の比較だけ（比較回数の上界に `+1` として入れた）。却下: `locate` に入れる（検索の全部が 1 回余分に比較する）。高速路なし（昇順の挿入が毎回 $O(\log n)$ の比較と位置の計算になる）。
- **D7 診断用の `chunk_count`・`invariants_hold` を公開関数にする**。理由: `std` にはテストを書けない（`E1018`）ので、利用者側のテスト（fixture）が検査できる形にする。前例は `HashMap.longest_probe`。値は実装の詳細で、将来の版で変わりうると文書に書く。却下: private にする（検査できない）。Rust の内部関数でチャンクを検査する（`std` の private 関数を Rust から呼べない）。
- **D8 `Vec` に `insert`・`remove`・`split_off` を足さない**（種）。理由: Rust のコンパイラ・IR・WASM・文書に変更が及ぶ。`std` の private 関数（`insert_at`・`remove_at`・`split_off`・`join`・`reverse`・`take`・`put`）で足りることを実機で確かめた。却下: 組み込みの追加。
- **D9 テストは境界（手計算の期待値）・性質（モデル）・比較回数の上界の 3 本立て**。理由: 変異の実験で、性質テストが M3 を見逃し、境界テストが M3・M8 を見つけた（「筆者が行った変異」）。比較回数は時間を使わず `Ord.lt` の回数で数える（共有 CI に速度の閾値を入れない方針）。却下: 性質テストだけ。時間の閾値。
- **D10 デバッガ表示は、旧レイアウト `["entries"]` の受理を残さず、LLDB は子を要求された分だけチャンクを読み、natvis は `CustomListItems` にする**。理由: レイアウトの変更と同じ PR で表示も変えるので移行期間がない。10^6 件でも最初の 256 個だけを読む。却下: 旧レイアウトの分岐を残す（死んだコード）。`update` で全チャンクを先読みする（10^6 件で遅い）。natvis を `head` だけ表示する（複数チャンクが見えない）。
- **D11 `std/Json.tz` の実装は変えない（コメントだけ）**。理由: 整列してから昇順に追加する構造は、末尾への追加の高速路にそのまま乗り、165 件が不変。却下: 整列をやめて文書順に挿入する（降順の入力で 1 件ごとに最大 511 エントリの移動と分割）。
- **D12 `union`・`intersect`・`difference` は、384 エントリのチャンクを新しく作る**。理由: 結果が I1〜I5 を満たす形に決まる。`union` は両辺を消費して末尾から `pop` する（コピーなし）。却下: 片方のチャンクを再利用して差し込む（位置合わせが複雑で I5 の維持が難しい）。結果を 1 本の `Vec` に集めてから分割する（メモリが 2 倍）。
- **D13 文書は最悪の式だけを書き、償却の計算量を主張しない**。理由: 分割・併合が 256 回以上の更新ごとに高々 1 回という議論は設計上のもので、実測の裏づけが薄い。却下: 「償却 $O(\log n)$」と書く。
- **D14 Phase は 2 つ**。理由: Phase 1 は挙動を変える全部（実装・テスト・表示・文書）を 1 つの PR にして、表示や文書が実装と食い違う状態を作らない。Phase 2 は挙動を変えない記録だけで、いつでも足せる。却下: `Map` と `Set` を別 Phase にする（デバッガ表示の判定が 2 つのレイアウトを持つ移行期間が要る）。

## 付録

- [C12-appendix.md](C12-appendix.md)（付録）: A `std/Map.tz` 全文、B `std/Set.tz` 全文、C ベンチマークの program、D 実測（before／after の表、B の振り、`Ord.lt` の回数）。
- [C12-appendix-tests.md](C12-appendix-tests.md)（付録 2）: A fixture 全文、B `features.mjs`、C `tests/map_set_chunks.rs`、D `tests/map_set.rs` の追加、E LLDB の定義、F natvis、G `debugger.mjs` の追加、H モック試験、I 文書の文面。
- 付録のコードブロックは、`<!-- file: ... -->` の行の直後のブロックが写す対象。付録 A・B・C、付録 2 A・E・H は、筆者の複製の作業木の検証済みのファイルと 1 バイトも違わない（抽出して比較した）。
