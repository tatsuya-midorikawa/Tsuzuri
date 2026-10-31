# D14: `Text` クラス（`String`／`Utf8String` の API の統一）

| 項目 | 内容 |
| --- | --- |
| ID | D14 |
| 優先度 | P2（D-50 の表。「性能の構造的な保証」の群） |
| 規模 | 全体 L。Phase 1 M（3〜5 日）、Phase 2 M（3〜5 日）、Phase 3 S（1〜2 日） |
| 依存 | なし。Phase 2 は Phase 1 の後、Phase 3 は Phase 1・2 の後。Phase 3 の G21・G26・G34 への接続だけは、それらが実装済みのときに行う（未実装なら文書だけで完了） |
| 後続 | なし（G21 の std 索引・G26 の facts の ID・G34 の doc-test が `Text` を扱う側。「前提とする他チケットのインターフェース」） |
| 状態 | done（全 Phase） |
| 起票 | 2026-10-18（D-50。HEAD `a2313d19` で確認。作業ツリーは `main` の `9dd0fbbc`。`a2313d19` との差は `docs/benchmarks.md` の削除 41 行だけで、`src/`・`std/`・`tests/` は同一） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1〜D12 はすべて承認済み |
| 改善する劣位 | 同じ操作が 2 組ある。`String`（21 関数）と `Utf8String`（20 関数）は 20 個が同名で、`Regex` は 18 関数のうち 7 組が `_utf8` の別名、`Unicode` は 24 関数のうち 10 組が別名（付録 A.0）。ジェネリックな関数は両方の型を受けられず、書く側と AI は「どちらの名前か」を覚える（D-50 の調査結論 (3)「同じ意味に複数の書き方がある」）。型クラス `Text` にすれば 1 つの名前が両方に使え、単相化で直接呼び出しになる（ゼロコスト）。 |
| 手本にする既存実装 | `std/Json.tz` の `instance Encode<string>` と `std/BigInt.tz` の `instance Ord<BigInt>`（std が組み込みクラスの instance を `.tz` に書く形）、`tests/fixtures/computations/Traits.tt`（`.tt` のクラス）、`src/stdlib.rs` の `SOURCES`・`RESERVED_MODULES`・`OPT_IN`・`module_name`（std モジュールの追加。コミット `2c680e12` の Matrix/MatrixView/Tensor）、`src/constants.rs` の `Checker::temporary_borrows`、`tests/features.mjs` の suite `string_library` と `textHash`、`tests/strings.rs` の `accepts`、`tests/stdlib.rs` の `rejects_reserved_modules_and_invalid_std_sources`、`tests/bench.rs` の `renumbered` |
| 主な影響ファイル | **Phase 1**: `std/Text.tt`（新規）、`std/String.tz`、`std/Utf8String.tz`、`src/stdlib.rs`、`src/constants.rs`、`src/polymorph.rs`（単体テストだけ）、`vsc/src/core.ts`、`.github/workflows/targets.yml`、`tests/stdlib.rs`、`tests/computations.rs`、`tests/fixtures/computations/Text.tc`（`Phrase.tc` に改名）、`tests/fixtures/computations/Main.tz`、`tests/text_class.rs`（新規）、`tests/text_class.mjs`（新規）、`tests/features.mjs`、`tests/fixtures/{text_class,text_class_direct,text_class_calls,text_user_instance}/Main.tz`（新規）、文書（`text.md` ほか）。**Phase 2**: `std/RegexText.tt`（新規）、`std/Regex.tz`、`src/stdlib.rs`、`vsc/src/core.ts`、`tests/stdlib.rs`、`tests/text_class.rs`、`tests/features.mjs`、`tests/fixtures/regex_text/Main.tz`（新規）、文書。**Phase 3**: `docs/benchmarks.md`、G21・G26・G34 への接続 |

長い表・実測・全文は付録に置いた。**付録 1** [D14-appendix.md](D14-appendix.md)（A 署名の突き合わせ表、B 実測、C std の骨格、D Rust の変更）、**付録 2** [D14-appendix-tests.md](D14-appendix-tests.md)（E・F テストの全文）、**付録 3** [D14-appendix-docs.md](D14-appendix-docs.md)（G 文書の草稿、H IR の比較手順）。本文の「付録 X」は、この 3 つの中の節番号。

## 方針の最新化（2026-10-31。実装前の再検討）

起票時（HEAD `a2313d19`）の前提と、A18（edition 2027）・D-59（引数の順序）・F14・G34・G35 の実装後の現状が食い違う点を、実装前に次のとおり改めた（人間の包括許可による判断。D1〜D12 の趣旨は保つ）。

| 起票時の前提 | 現状 | 改めた方針 |
| --- | --- | --- |
| `RESERVED_MODULES` は 53 個で、Phase 1 で 54、Phase 2 で 55 | F14 が `Alloc` を足して 54 個。文書（`modules.md` の一覧と数、`docs/language.md` の 2 つの一覧）は `Alloc` を載せず 53 のまま | 現在値から数えて Phase 1 で 55、Phase 2 で 56（`reserves_the_d07_table`）。文書は `Alloc` も足して 56 個にそろえた |
| fixture の無印の引数（`def hash :: string -> i64`）は値渡し | A18 で、マニフェストのない入力（edition 2027）の無印の引数は共有借用。`hash (Text.replace …)` のように結果が `'s` の呼び出しを借用の引数へ渡すと、`'s` が `ref string` に決まって `E1003` になる（`String.length (Maybe.get m)` と同じ、前からある推論の制限） | fixture の `hash`・`hash_utf8` は `string_library` と同じ `own string`。制限は `text.md` の注意点と `docs/language.md` の一時値の借用の段落に書き、直さずに後続へ回した（下の「実装と検証」） |
| 付録の fixture の旧い書き方（`index -> …` のラムダ、`0 .. (n - 1)`、`Array.map_ref`） | edition 2027 は旧ラムダと素の `..` を拒否する。`Array.map_ref` は `Array.mapr` | `\index -> …`、`0 ..< spans.length`、`Array.mapr` に直した（意味は同じ） |
| std の公開関数に例は要らない | G34: 公開 `def` はすべて `///` の例を持ち、新しいモジュールは `tests/std_examples.rs` の `FLOORS` に入る。G35: 新しい公開関数は `@since` | `Regex.*_text` の 7 関数に例・`@since 0.1.0`・`@see`。`FLOORS` に `("Text", 0)`・`("RegexText", 0)`、`Regex` は 18 → 25。クラスのメソッドは `program.functions` に入らず数えられないが、`Text` の 21 メソッドとクラス、`RegexText` のクラスにも例を付けた（`tsuzuri test --doc std` が走らせる） |
| 引数の順序の検査はない | D-59: `tests/std_argument_order.rs` が `std/` の全ファイルを `MODULES` で分類し、トップレベルの `def` の行を読む | 読み取りを `.tt` のクラスのメソッド（字下げした `def`）にも広げ、`Text`（対象 `'s`・`['s]`）・`RegexText`（`'s`）を分類した。`Regex` の対象に `'s` を足し、`*_text` も検査する |
| `_features/README.md`・`GUIDE.md` は統括者が更新する | 共有の手順はチケットの完了で更新する | README の 2 行と GUIDE の台帳（D-55 の表の `Text`・`RegexText`・`Regex.*_text`）を更新した |
| `docs/benchmarks.md` の節は D11 の節の次 | その次に C12 の節がある | `## 性能指標の記録` の前（C12 の節の次）に置いた |
| E2E と IR の比較は `target/release/tsuzuri` | AGENTS.md は dev のバイナリ（opt-level 1）で足りる検査は dev で走らせる。生成物はコンパイラのビルドの種類によらない | `target/debug/tsuzuri` で走らせた（オブジェクトコードの比較の測定も同じ） |
| Phase ごとに別の PR | AGENTS.md は 1 チケット 1 PR | 全 Phase を 1 つのブランチ・PR にする |

## 実装者向け早見

1. 読む順: この早見 → 「仕様」→「実装手順」→ 付録 C（std の骨格。型検査を通した全文）→ 付録 D（Rust の差分）。迷ったら「落とし穴」。
2. 作るもの: `std/Text.tt`（`class Ord<'s> => Text<'s>`、21 メソッド）と、`std/String.tz`・`std/Utf8String.tz` の末尾の `instance Text<string>`・`instance Text<utf8string>`。本体は付録 C の全文をそのまま使う。
3. コンパイラの変更は 1 か所だけ: `src/constants.rs` の `Checker::temporary_borrows` に `TypedExprKind::Method(..)` の腕を 1 つ（付録 D.1）。これがないと `Text.find "a" text` が `E1013` になる（HEAD の不具合。付録 B.1）。
4. 予約名: `RESERVED_MODULES` に `"Text"`（Phase 2 は `"RegexText"`）。ユーザーの `Text.tz`・`Text.tt`・`Text.tc` と `namespace Text` が `E1011` になるので、フィクスチャの `tests/fixtures/computations/Text.tc` は `Phrase.tc` に改名する（付録 D.4）。
5. 最初に書くテスト（手順 2）: `tests/fixtures/text_class*`・`tests/text_class.rs`・`tests/text_class.mjs`・`tests/features.mjs` の suite `text_class`（付録 2 の E）。HEAD では `E1016: unknown type class 'Text'` で赤になるのが正しい。
6. 触ってはいけない: `String.*`・`Utf8String.*`・`Regex.*`・`Json.*`・`Char.*`・`Utf8Char.*` の既存の関数（名前・署名・意味）、組み込み関数、診断のコードとメッセージ、予約語、組み込みクラス、WASM の import。`Text` に `lines`・`to_upper` などを足さない。
7. `Text` は**常に読み込む**（opt-in にしない。決定事項 D1）。opt-in にするのは Phase 2 の `RegexText` だけ。
8. メソッドの本体は**ちょうど 1 回の呼び出し**。ロジックも変換も書かない（`tests/text_class.mjs` が検査する）。
9. 既存テストの期待値・名前を変えてよいのは付録 D.4 の表だけ。それ以外が赤くなったら止める（停止条件 S4）。
10. Phase 1 の完了: `cargo test --locked` が緑（既知の 1 件を除く）、`node tests/features.mjs target/release/tsuzuri text_class` が `native/WASM at O0/O3 passed` と `features: 4595 cases passed`、`node tests/text_class.mjs target/release/tsuzuri` が `text class: 40 instance methods, each one call; no indirect call`、付録 H の IR の比較が `DIFFERENT: 0`、`node scripts/check-docs.mjs` が通る。
11. Phase 2 の完了: suite `regex_text` が `features: 66 cases passed`。Phase 3 の完了: `docs/benchmarks.md` の節と、G21・G26・G34 への接続（実装済みなら）。
12. 環境: `export PATH="$(brew --prefix llvm)/bin:$(brew --prefix lld)/bin:$PATH"`、Node 20 以降（`text_class` は Node 20 で通る）。Rust のファイルは `cargo fmt --all` を通す。`tsuzuri fmt` は std とフィクスチャの対象外（HEAD のものも `not formatted`）。

## 目的

**問題**。同じ操作が `String` と `Utf8String` に 2 組ある。HEAD で数えた（付録 A.0 のコマンド）: `String` の公開 `def` は 21、`Utf8String` は 20 で、名前が同じものが 20（`String` だけにあるのは `decode_at` で、`Utf8String` の `decode_at` は組み込み）。`Char` と `Utf8Char` は 6 つずつがすべて同名。`Regex` は公開 18 関数のうち 7 組が `_utf8` の別名で、`Unicode` は 24 のうち 10 組。`Json` の 38 関数には対がない。`string` と `utf8string` のどちらも受ける関数は、今は型ごとに 2 つ書くしかない。

**利用者の価値**。
- 1 つの名前（`Text.find`）が両方の型で同じ引数の順・同じ失敗（`None` かトラップ）で使える。ジェネリックな関数は `Text<'s> =>` と書き、`Ord` を含むので `<`・`==` も使える。
- ゼロコスト。呼び出しは instance のメソッドへの直接の呼び出しになり、その本体は `String.f` か `Utf8String.f` を 1 回呼ぶだけ。辞書・vtable・間接呼び出しは作らない。小さいプログラムではオブジェクトコードが直接呼び出しと同じ（付録 B.3）。
- 暗黙のルールを増やさない。`char_count` のように「同じ名前で意味が違う」関数は**入れない**ので、`Text` のメソッドは常に「`String.f`／`Utf8String.f` と同じ」と言い切れる。

**数字**（付録 A・B）。`Text` のメソッドは 21 個（`String`／`Utf8String` の共通の `def` 18 ＋ 組み込みの `compare`・`clone` ＋ 新しい `is_empty`）。入れない公開関数は、`String` 側 8、`Utf8String` 側 6。`Text.m` を型 T に使うと、特殊化の予算を (メソッド, 型) の組ごとに 1 つ、直接の呼び出しより多く使う。

**非目標**。
- `String.*`・`Utf8String.*` の非推奨・削除・改名はしない（`docs/stability.md`）。型が決まっている場所では今までどおり書ける。
- 暗黙の型変換（`string` と `utf8string`）、文字列リテラルの型、既定のエンコーディング（UTF-16）は変えない。
- 新しい操作は足さない（`lines`・`to_upper`・`to_lower` の追加と、`char_count` の統一はしない）。新しいのは `is_empty` だけ。
- `Json`・`Char`・`Utf8Char`・`Unicode` に型クラスは足さない（決定事項 D5・D6。`Unicode` の `_utf8` の対は将来 Phase 2 と同じ型で包める）。
- SIMD・並列・GPU などの新しいアルゴリズムは足さない。性能の約束は「抽象化のコストが増えない」ことだけ。
- 新しい診断コード・予約語・crate・WASM の import は足さない。

## 着手条件と停止条件

### 着手条件

- 依存するチケットはない。Phase 1 から順に進める。
- ベースライン（GUIDE §2.3）: `cargo build --release --locked` と `cargo test --locked` を変更前の HEAD で走らせ、失敗を一覧にして保存する。HEAD でも `bounds_nested_builder_expansion_not_just_source_syntax` は既定の 2 MiB の stack で溢れる（GUIDE の既知の注意）。E2E のベースライン: `node tests/features.mjs target/release/tsuzuri string_library`（3,230 件）、`node tests/e2e.mjs target/release/tsuzuri`。
- 変更前のバイナリを保存する: `cp target/release/tsuzuri target/tsuzuri-before`（付録 H で IR を比べる）。
- 読む箇所: 本文の「現状」、`src/stdlib.rs` の `SOURCES`・`RESERVED_MODULES`・`OPT_IN` とそのテスト、`src/constants.rs` の `temporary_borrows`、`std/String.tz` の末尾、`std/Json.tz` の `instance Encode<string>`、`tests/strings.rs` の先頭、付録 A。

### 停止条件

止めて報告する。選択肢の A・B は、報告に書いて人間が選ぶ。

| 番号 | 条件 | 選択肢 |
| --- | --- | --- |
| S1 | ベースラインの `cargo test --locked` で、`bounds_nested_builder_expansion_not_just_source_syntax` 以外が落ちる | A: 環境の問題なら直して再実行。B: HEAD の不具合なら一覧に残して着手し、最後の報告に含める |
| S2 | 付録 H の IR の比較が `DIFFERENT` を 1 件でも出す（`tests/fixtures/computations` の `Phrase.tc` への改名前だけは許す） | A: 差が生成シンボルの番号だけなら、`ir_compare.py` の正規化の取りこぼし（直す）。B: 構造が違うなら、`Text` を読み込むことで他の関数の IR が変わっている。原因（`SOURCES` の順序、予約名、`Method` の腕が既存の呼び出しを変えた）を調べ、直せなければ報告 |
| S3 | `node tests/text_class.mjs` が「instance のメソッドが 1 回より多く呼ぶ」「間接呼び出しがある」で落ちる | A: instance の本体を付録 C の形に戻す（余計な `ref`・変換を書いていないか）。B: 本体が 1 回の呼び出しでもコンパイラが呼び出しを足すなら、その IR を報告 |
| S4 | `cargo test --locked` で、付録 D.4 の表にない既存テストが赤くなる | A: 名前の衝突（`Text`・`Phrase`）なら、期待値の更新が妥当かを報告。B: コンパイラの挙動の変化なら原因を調べて報告 |
| S5 | 付録 D.5 の予算のテストが落ちる、または `honors_the_exact_specialization_limit` が赤くなる | A: instance の本体の呼び出し数が違うなら付録 C を直す。B: 数え方が違うなら、付録 B.2 の測定を取り直して本文の数字を直す |
| S6 | `Method` の腕（付録 D.1）で、`E1013` を期待する既存のテストが「受理された」で落ちる | A: そのテストが意図的に拒否を期待しているなら、腕を std のクラスだけに絞る案を報告。B: 不具合の固定だったなら期待値を直し、報告に書く |
| S7 | Phase 2: `src/stdlib.rs` の `opt_in_modules_are_reached_only_through_their_names` が落ちる | A: `names`・`uses` の組を付録 D.3 の形に直す。B: `RegexText` を opt-in にしない案（常に読み込むと `Regex` を名指して、このテストの前半が禁じる）。報告して相談 |
| S8 | 付録 G.1・G.2 の例の出力が、`run=` と「実行結果」と違う | A: 実装が付録 C と違う（原因を調べる）。B: 環境依存の例なら、例を直して報告 |
| S9 | suite `text_class` が JavaScript の参照値と合わない | A: instance が別の関数を呼んでいる（付録 C と照合）。B: 参照値の誤り。まず `string_library` の suite が同じ値を返すかを見る。返すなら原因はコンパイラ |
| S10 | `tests/modules.rs` の名前空間のテスト（`tools/Text.tz`）が赤くなる | A: `using` の解決の順が変わった（付録 B.4 の再現で HEAD と比べる）。B: 予約名の判定が依存パッケージのモジュールにも及んでいる。報告 |

## 現状（HEAD `a2313d19` で確認）

すべて HEAD のバイナリ（`target/release/tsuzuri`）か、HEAD のソースの読み取りで確かめた。試作（`Text` を足したコンパイラ）の出力は「試作」と明記する。

**1. 二重の API。** 件数はコマンドで数えた（付録 A.0 に全文）。

| モジュール | 公開 `def` | `_utf8` の対 | 組み込み（名前空間のもの） |
| --- | --- | --- | --- |
| `String` | 21 | 0 | 6（`compare`・`from_utf8`・`is_well_formed`・`to_well_formed`・`to_code_units`・`from_code_units`）＋ `clone_string` |
| `Utf8String` | 20（`String` と名前が共通なのは 20） | 0 | 6（`clone`・`compare`・`decode_at`・`from_bytes`・`from_string`・`to_bytes`） |
| `Char`／`Utf8Char` | 6／6（すべて同名） | 0 | 2／3 |
| `Regex` | 18 | 7 | 0 |
| `Json` | 38 | 0（文書は `utf8string`、キーと文字列は `string`） | 0 |
| `Unicode`（参考） | 24 | 10 | 0 |

**2. `Text` はない。** HEAD で、次の 3 行のプログラム（`Main.tz`）を `check` する。

```tsuzuri
def size :: Text<'s> => ref 's -> i64
fn size text = Text.length text
0
```

```text
$ target/release/tsuzuri check <dir>
<dir>/Main.tz:1:13: error[E1016]: unknown type class 'Text'
```

`Text.compare "a" "b"` だけを書くと `error[E1002]: unknown value 'Text'`。

**3. 同じ名前で意味が違うところ。** `😀`（U+1F600）で測った。出力は付録 A.8 のプログラムを `run` した結果。

```text
2 1 2 4 1 -1 2 1 5 6 1 -1
```

読み方（左から）: `char_count` 2／1、`length` 2／4、`slice 1 2` 長さ 1 の `Some`／`None`、`split ""` 2 個／1 個、`replace "" "-"` 5 単位／6 バイト、`compare "\u{FF5E}" "😀"` 1／−1。表（付録 A.1）の分類: 同じ意味 8 関数、単位だけが違う 3 関数（`length`・`find`・`rfind`）、意味や上限が違う 7 関数（`slice`・`sub`・`join`・`concat`・`split`・`repeat`・`replace`）、入れない 3 関数（`decode_at`・`char_count`・`chars`）。

**4. std が組み込みクラスの instance を書く流儀。** `std/Json.tz` に `instance Encode<string>` と `instance Decode<string>`、`std/BigInt.tz` に `instance Ord<BigInt>`（メソッドは `fn lt left right = …`）がある。std に `.tt` はまだ 1 つもない（`ls std/*.tt | wc -l` が 0）。`.tt` を std の `SOURCES` に置けること自体は、`src/stdlib.rs` の単体テスト（`module_name("std/Classes.tt")` が `Some("Classes")`）が確かめている。クラスは `.tt` にだけ書ける:

```text
$ target/release/tsuzuri check k1         # Main.tz に class を書いたとき
k1/Main.tz:1:7: error[E1018]: type class declarations belong in a .tt file; a .tt file may declare multiple classes
$ target/release/tsuzuri check k2         # Shapes.tt と Shapes.tz を同じ場所に置いたとき
k2/Shapes.tz:1:1: error[E1011]: module 'K2::Shapes' is declared by more than one file; rename a file or change its namespace
```

したがって、クラスの `Text`（`std/Text.tt`）と、`Text` の名前のモジュール関数（`std/Text.tz`）は同じ名前で同時に作れない（決定事項 D1）。

**5. クラスのメソッドへ一時値を借用で渡せない（HEAD の不具合）。** ユーザーのクラスで再現する（付録 B.1 に全文）。

```text
$ target/release/tsuzuri check <dir>          # <dir> は Pairing.tt と Main.tz（付録 B.1）。Pairing.two (ref "a") (ref "b")
<dir>/Main.tz:7:26: error[E1013]: borrow requires a local place; bind the temporary with 'let' first
  7 | let b = Pairing.two (ref "a") (ref "b")
    |                          ^
```

`Text` と同じ 21 メソッドのクラスをユーザーが書くと、引数 1 個の 9 メソッドは通り、引数 2 個以上の 12 メソッド（`compare`・`find`・`rfind`・`contains`・`starts_with`・`ends_with`・`slice`・`sub`・`join`・`split`・`repeat`・`replace`）は `Text.find (ref "a") (ref "ab")` で `E1013`。原因は `src/constants.rs` の `Checker::temporary_borrows`（クラスのメソッドの呼び出しは `TypedExprKind::Method` で、`single_stage` の判定が `_ => arguments.len() == 1` に落ちる）。直接の関数 `String.find (ref "a") (ref "ab")` は通る。

**6. 名前を予約すると何が起きるか。** `Text` を予約名にすると、HEAD では通る次の入力が `E1011`／`E2000` になる（試作で確認。付録 B.4）: ルートの `Text.tz`・`Text.tt`・`Text.tc`、`namespace Text`、マニフェストの `namespace = "Text"`、`tsuzuri new text`。`Utils/Text.tz` や依存パッケージの `Acme::Tools::Text`（`tests/modules.rs`）、`union Text`・`record Text`・`const Text`・`type Text = …` の宣言は通る。

**7. 特殊化の予算。** 上限は 65,536（`src/polymorph.rs` の `specialization_limits`）。使わない std は 0 個。直接の `String.length "abc"` は 1 個、`Text.length "abc"` は 2 個（instance のメソッドと `String.length`）。付録 B.2 の表。

**8. opt-in の std モジュールの規則（D-40）。** `Regex` のような opt-in の std モジュールの宣言は、ユーザーのコードから無修飾の候補にならない（`src/stdlib.rs` の `user_code_names_opt_in_declarations_only_qualified`、`docs/language.md` の「後発の std モジュール」で始まる段落）。`Text` を opt-in にすると、利用者は `Text.Text.length` と書くことになるので、`Text` は常に読み込む。常に読み込む std のソースは opt-in のモジュール名を名指せない（同じ `src/stdlib.rs` の `opt_in_modules_are_reached_only_through_their_names`）。

**9. 型クラスの機構（試作で確認）。** メソッドは `ref` の引数を取れる。メソッドが `'s` 以外の型変数を持つクラスは書けない（`E1016: each class method must mention exactly the declared class type variable`。このため、検索の結果を返すコールバック型のメソッドで `Regex` を包む設計はできない。決定事項 D4）。`Name.method` は、モジュールの関数を先に探し、なければクラスのメソッドを（クラス名を修飾として）探す。std のコードからは std のクラスだけが見え、ユーザーのクラスは見えない。

## 変えてはいけない意味

- `String.*`・`Utf8String.*`・`Char.*`・`Utf8Char.*`・`Regex.*`・`Json.*`・`Unicode.*` の既存の公開関数: 名前・署名・戻り値・失敗（`None` かトラップ）・計算量・上限は 1 つも変えない。suite `string_library`・`regex`・`unicode`・`json`・`chars` の期待値も変えない（試作で全部通る。付録 B.5）。
- `Text` を使わない既存プログラムの IR: 生成シンボルの番号（`$intrinsic.Display.display.428`、`$instance.N`）を `tests/bench.rs` の `renumbered` と同じ規則で正規化すれば、1 byte も変わらない。HEAD で作れる 102 個のうち 50 個は番号も含めて一致し、51 個は正規化して一致する（残る 1 個はフィクスチャの改名。付録 B.4）。番号が動くのは、常に読む std が関数 ID を 41 個足すため。`-g` では `Text.tt` の `!DIFile` が 1 つ増える。
- 診断のコードとメッセージと JSON の形。新しいコードは足さない。クラスのメソッドへ一時値の借用を渡す呼び出しは、`E1013` から「受理」へだけ変わる（引数をすべてそろえ、結果が借用を持たない場合。部分適用と、借用を持つ結果は `E1013` のまま）。
- ABI・WASM の import・ランタイム・予約語・組み込みクラス（`src/polymorph.rs` の `BUILTIN_CLASSES`）。
- 既定のエンコーディング（UTF-16）、リテラルの型、暗黙の型変換をしないこと。
- Phase 2 は純粋な追加（`Regex.*` の既存の 18 関数は変わらない）。
- ユーザーのソースが壊れるのは、`docs/stability.md` の既知の例外「std へのモジュールの追加」だけ: `Text`（Phase 2 は `RegexText`）という名前のモジュールと名前空間（`E1011`／`E2000`）。PR の説明に書く。

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | `Text` クラス（`std/Text.tt`）、`String`・`Utf8String` の instance、予約名 `Text`、`temporary_borrows` の `Method` の腕、テスト、`text.md` ほかの文書 | M | なし | std への追加だけで、既存の関数は変わらない。`Text` を使わないプログラムの IR は（番号の正規化の後）変わらない |
| 2 | opt-in の `RegexText`（`std/RegexText.tt`）と `Regex.*_text` の 7 関数、予約名 `RegexText`、テスト、`regex.md` ほか | M | Phase 1（`tests/text_class.rs`・`vsc/src/core.ts` の足場を使う） | 追加だけ。`Regex` を使わないプログラムは `RegexText` を読み込まない |
| 3 | `docs/benchmarks.md` の節（`Text` の呼び出しのオーバーヘッドの測定）、G21・G26・G34 への接続 | S | Phase 1・2 | 文書と測定だけ。接続先が未実装なら何もしない |

## 仕様

構文の変更はない（新しい構文・予約語・属性はない）。コンパイラの CLI のフラグ・サブコマンド・環境変数、診断の JSON のキー、出力の行も変わらない（`tests/text_class.mjs` の `--objects` はテスト用スクリプトの引数で、コンパイラの CLI ではない）。型クラスと instance は既存の機構で書く。

### 仕様 1: クラス `Text`

`std/Text.tt`（付録 C.1 が全文）。ファイル名の `Text` がモジュール名、クラス名も `Text`。

```tsuzuri
class Ord<'s> => Text<'s> {
    def length :: ref 's -> i64
    def is_empty :: ref 's -> bool = \text -> Text.length text == 0
    // …… 以下 19 メソッド（下の表。宣言の順序は表のとおり）
}
```

`'s` は `string` か `utf8string`（std の instance はこの 2 つ。ユーザーも自分の型の instance を書ける）。スーパークラスは `Ord`（`Eq` を含む）。

### 仕様 2: メソッド（21 個。宣言の順序）

instance の本体は、メソッド `m` が `String.m <引数>`（`utf8string` は `Utf8String.m <引数>`）の 1 回の呼び出し。例外は 3 つだけ: `compare`（`String.compare`／`Utf8String.compare`。組み込み）、`clone`（`clone_string`／`Utf8String.clone`。組み込み）、`is_empty`（instance に書かない。クラスの既定の実装）。

| # | メソッド | 署名（`'s` は `string` か `utf8string`） | 型ごとの違い（付録 A.1 の行） |
| --- | --- | --- | --- |
| 1 | `length` | `ref 's -> i64` | 単位（#1） |
| 2 | `is_empty` | `ref 's -> bool`（既定: `length` が 0） | なし |
| 3 | `compare` | `ref 's -> ref 's -> i64` | 順序: コード単位／バイト（A.2 の #1・#2） |
| 4 | `clone` | `ref 's -> 's` | なし |
| 5 | `find` | `ref 's -> ref 's -> Maybe<i64>` | 位置の単位（#2） |
| 6 | `rfind` | `ref 's -> ref 's -> Maybe<i64>` | 位置の単位（#3） |
| 7 | `contains` | `ref 's -> ref 's -> bool` | なし（#4） |
| 8 | `starts_with` | `ref 's -> ref 's -> bool` | なし（#5） |
| 9 | `ends_with` | `ref 's -> ref 's -> bool` | なし（#6） |
| 10 | `slice` | `i64 -> i64 -> ref 's -> Maybe<'s>` | 境界（#7） |
| 11 | `sub` | `i64 -> i64 -> ref 's -> Maybe<'s>` | 境界（#8） |
| 12 | `join` | `ref 's -> ref ['s] -> 's` | 結果の上限（#12） |
| 13 | `concat` | `ref ['s] -> 's` | 結果の上限（#13） |
| 14 | `split` | `ref 's -> ref 's -> ['s]` | 空の区切り（#14） |
| 15 | `trim` | `ref 's -> 's` | なし（#17） |
| 16 | `trim_start` | `ref 's -> 's` | なし（#15） |
| 17 | `trim_end` | `ref 's -> 's` | なし（#16） |
| 18 | `to_ascii_lower` | `ref 's -> 's` | なし（#18） |
| 19 | `to_ascii_upper` | `ref 's -> 's` | なし（#19） |
| 20 | `repeat` | `i64 -> ref 's -> 's` | 結果の上限（#20） |
| 21 | `replace` | `ref 's -> ref 's -> ref 's -> 's` | 空のパターン（#21） |

引数の順は `String.*` と同じ: パターン・区切り・接頭辞・接尾辞が**先**で本文が後（`find needle text`、`split separator text`、`replace needle replacement text`、`join separator parts`）。`slice first last text`・`sub first count text`・`repeat count text` も本文が後。

### 仕様 3: 意味の規則（決定表）

| 規則 | 内容 |
| --- | --- |
| R1 | `Text.f args` の値は、`string` なら `String.f args`、`utf8string` なら `Utf8String.f args` と同じ（値・`None`・トラップ）。メソッドは何も足さず、何も変えない |
| R2 | 長さ・位置・範囲の単位は型で決まる: `string` は UTF-16 のコード単位、`utf8string` はバイト。`Text` はこの違いを隠さない（付録 A.1・A.8） |
| R3 | すべてのテキストの引数は `ref`。リテラルや関数呼び出しの結果は、`ref` を付けても付けなくても、`String.f` と同じ規則で一時値として借用できる（`Method` の腕。付録 B.1）。部分適用（`let f = Text.find "a"`）は `E1013` のまま |
| R4 | `Ord` を含むので、ジェネリックな本体の中で `'s` に `<`・`<=`・`>`・`>=`・`==`・`!=` が使える（借用した値の比較は `deref left < deref right`）。`Text.compare` は `<`（`string` はコード単位の順、`utf8string` はバイトの順）と同じ向きの `-1`・`0`・`1` を返す |
| R5 | `is_empty` は既定の実装 `\text -> Text.length text == 0`。ユーザーの instance は上書きしてよい |
| R6 | ユーザーの型の instance は、`Ord`（と `Eq`）の instance と、`is_empty` 以外の 20 メソッドが要る（付録 2 の E.5 の例） |
| R7 | 将来 `Text` にメソッドを足すときは、**既定の実装を付ける**（`docs/stability.md`: 既定のない class メソッドの追加は互換性のない変更。ユーザーの instance が壊れる） |

### 仕様 4: 名前解決（決定表）

| 書き方 | 結果 |
| --- | --- |
| `Text.length x`、`Text<'s> =>`（ユーザーが `Text` の宣言を持たない） | std のクラス |
| `Text.Text.length x`、`Text.Text<'s> =>` | 常に std のクラス |
| ユーザーが `class Text<…>` か `union Text` を宣言している | `Text.length` はユーザーの宣言に解決（union なら `E1002: union 'Main.Text' has no case 'length'`）。std のクラスは `Text.Text.length` |
| ユーザーが `record Text`・`type Text = …`・`const Text` を宣言している | `record` は宣言できて、`Text.length` は std のクラスのまま。`type`・`const` も宣言できる（付録 B.4） |
| ルートの `Text.tz`・`Text.tt`・`Text.tc` | `E1011: module name 'Text' is reserved for the standard library; rename the file` |
| `namespace Text`、`namespace Text::Tools` | `E1011: namespace 'Text' puts module 'Main' under 'Text', a name reserved for the standard library; choose another namespace` |
| マニフェストの `namespace = "Text"` | `E1011: package namespace must be identifiers joined by '::' such as "Acme::Tools", at most 16 segments and 255 bytes, that do not start with 'std' or a standard library module name` |
| `tsuzuri new text` | `E2000: cannot derive a namespace from the folder name 'text'; pass --namespace` |
| `Utils/Text.tz`、依存パッケージの `Acme::Tools::Text` | 通る |

### 仕様 5: 診断（新しいコードはない。すべて試作で確認した出力）

| 状況 | コード | メッセージ（先頭） |
| --- | --- | --- |
| `Text` のない HEAD で `Text<'s>` を書く | `E1016` | `unknown type class 'Text'` |
| instance のない型に使う（`Text.length (ref 5)`） | `E1005` | `no instance for Text.Text<i32>; define an instance or use a supported type` |
| 2 つの型を混ぜる（`Text.find "a" u8"abc"`） | `E1003` | `expected string, found utf8string` |
| 存在しないメソッド（`Text.reverse "abc"`） | `E1002` | `type class 'Text' has no method 'reverse'` |
| `dyn Text` | `E1028` | `dyn Text is not allowed: method Eq.eq uses the class type outside its first parameter; use a generic function with a Text constraint instead` |
| ユーザーの instance にメソッドが足りない | `E1016` | `missing method 'Text.Text.trim_end'` |
| ユーザーの型に `Eq`・`Ord` の instance がない | `E1027` | `no instance for Eq<Main.Name>; define an instance or use a supported type` |
| 部分適用で一時値を借用（`let f = Text.find "a"`） | `E1013` | `borrow requires a local place; bind the temporary with 'let' first`（直接の `String.find "a"` と同じ） |
| 予約名 | `E1011`・`E2000` | 仕様 4 |

### 仕様 6: `RegexText` と `Regex.*_text`（Phase 2）

- `std/RegexText.tt`（opt-in。付録 C.4）: `class RegexText<'s>`（スーパークラスなし）。7 メソッド `is_match`・`find`・`find_at`・`find_all`・`captures`・`replace_all`・`split`。各メソッドの第 1 引数は `ref Regex`、最後の引数はテキスト `ref 's`。
- instance（`std/Regex.tz` の末尾。付録 C.5）: `instance RegexText<string>` は `Regex.is_match`・`find`・`find_at`・`find_all`・`captures`・`replace_all`・`split` を、`instance RegexText<utf8string>` は `Regex.is_match_utf8`・`find_utf8`・`find_at_utf8`・`find_all_utf8`・`captures_utf8`・`replace_all_utf8`・`split_utf8` を、それぞれ 1 回呼ぶ。
- 利用者が呼ぶ 7 つの関数（`std/Regex.tz`）: `Regex.is_match_text`・`find_text`・`find_at_text`・`find_all_text`・`captures_text`・`replace_all_text`・`split_text`。署名は `RegexText<'s> => ref Regex -> …`（テキスト `ref 's` は最後の引数。付録 A.4 と C.5）。`replace_all_text` の置換文字列は本文と同じ型（`'s`）。範囲の単位は `string` がコード単位、`utf8string` がバイト。
- `Regex.compile`・`escape`・`group_count`・`search_steps` は対象外（A.4）。
- ユーザーのコードから: `Regex.find_text (ref re) text` は普通に呼べる。ジェネリックな関数に制約を書くときは `RegexText.RegexText<'s>`、メソッドを直接呼ぶときは `RegexText.RegexText.find_all`（`RegexText<'s>` の無修飾は `E1016: unknown type class 'RegexText'`、`RegexText.find_all` は `E1002: module 'RegexText' has no function or union case 'find_all'`）。`Regex.is_match_text (ref re) (ref 5)` は `E1005: no instance for RegexText.RegexText<i32>; define an instance or use a supported type`。
- `Regex` か `RegexText` を名指さないプログラムは、どちらも読み込まない（D-40）。

### 仕様 7: 上限・決定性・互換性

- **上限**: `Text` は新しい上限を足さない。`join`・`concat`・`repeat` の結果の上限は呼ぶ関数と同じ（`string` は 2^53 − 1 コード単位、`utf8string` は `i64` の範囲）。特殊化の予算（65,536）は、`Text.m` を型 T に使うと (メソッド, 型) の組ごとに 1 個、直接の呼び出しより多く使う（付録 B.2）。
- **決定性**: 生成コードは決定的。instance のメソッドの IR の名前は `@tz.fn.<型>.$instance.<番号>.<メソッド>`（番号は動いてよい）。
- **`docs/stability.md`**: std の公開 API は削除しない。新しい公開名の追加は「既知の例外」（std へのモジュールの追加）。診断のコード・JSON のキーは変えない。IR・ABI・WASM の import は、`Text` を使わないプログラムでは（番号の正規化の後）変わらない。
- **edition（D-45・D-54）**: 予約語を足さないので `EDITION_KEYWORDS` は変えない。型検査と IR は edition を見ないので、edition `2026` のままでよい。
- **LSP・VS Code**: `vsc/src/core.ts` の `libraryModules` が `RESERVED_MODULES` と同じ集合であることを `vsc/src/test/core.test.ts` が検査する。足し忘れると `npm run test:unit` が落ちる。

## 設計

### 設計 1: 変更の一覧

| ファイル | Phase | 変更 |
| --- | --- | --- |
| `std/Text.tt`（新規） | 1 | 付録 C.1 の全文 |
| `std/String.tz` | 1 | 末尾に付録 C.2 の `instance Text<string>` |
| `std/Utf8String.tz` | 1 | 末尾に付録 C.3 の `instance Text<utf8string>` |
| `src/stdlib.rs` | 1・2 | `SOURCES` の**末尾**に `std/Text.tt`（Phase 2 は続けて `std/RegexText.tt`）、`RESERVED_MODULES` の末尾に `"Text"`（Phase 2 は `"RegexText"`）、Phase 2 は `OPT_IN` の `Regex` の `names`・`uses` と `RegexText` の項目（付録 D.2・D.3） |
| `src/constants.rs` | 1 | `temporary_borrows` に `Method` の腕（付録 D.1） |
| `src/polymorph.rs` | 1 | 単体テスト 1 件（付録 D.5） |
| `std/RegexText.tt`（新規）・`std/Regex.tz` | 2 | 付録 C.4・C.5 |
| `vsc/src/core.ts` | 1・2 | `libraryModules` に ` Text`（Phase 2 は ` RegexText`） |
| テスト・文書 | 1〜3 | 付録 D.4、付録 2、付録 3 |

新しい Rust の関数・型・フィールドはない（コンパイラの変更は 1 腕）。

### 設計 2: 呼び出しが直接呼び出しになる流れ

1. 名前解決: `Text.find (ref n) (ref t)` は、モジュールの関数 `Text.find` がないので、クラスのメソッドとして `TypedExprKind::Method(クラス, メソッド, 型)`（`src/polymorph.rs` の `method` が作る）になる。
2. 型検査: `'s` が引数から決まる（`string` か `utf8string`）。決まらなければ `E1005`・`E1003`（仕様 5）。
3. 単相化: 具体型が決まると、instance `Text<string>` のメソッド `find` の関数（IR の名前 `@tz.fn.String.$instance.<番号>.find`）への直接の呼び出しになる。`Text<'s> =>` の制約を持つ関数は、使う型ごとに 1 つ特殊化され、中の `Text.f` も同じ。辞書も `dyn` も使わない。
4. instance のメソッドの本体は `String.find needle text` の 1 回の呼び出し。`-O0` では関数として残る（呼び出しが 1 段増える）。`-O1` 以上は LLVM が呼び出し元に組み込む（付録 B.3 の IR と表）。

### 設計 3: `Checker::temporary_borrows` の変更（付録 D.1）

借用する一時値（`ref "a"` や自動で借用されるリテラル）を持つ呼び出しは、「引数をすべて一度に適用し、結果が借用を持たない」ときだけ `BorrowOperand` に直る。判定の擬似コード（変更は `Method` の行だけ）:

```text
single_stage(callee, arguments) =
  match callee.kind
    Function(User(id)) | GenericFunction(id, _)  -> signatures[id].parameters.len() == arguments.len()
    Function(Builtin(instance))                  -> instance.builtin.scheme().parameters.len() == arguments.len()
    Method(..)                                   -> callee.ty is Function(parameters, _) and parameters.len() == arguments.len()   // 新規
    otherwise                                    -> arguments.len() == 1
if not single_stage or not loan_free(result): （従来どおり。定数でなければ何もせず、あとの所有権の検査が E1013）
else: borrowed な引数を BorrowOperand に書き換える
```

計算量は引数の数に比例（変更前と同じ）。組み込みクラス（`src/polymorph.rs` の `BUILTIN_CLASSES`）のメソッドの呼び出しも `TypedExprKind::Method` なので、同じ腕に当たる。変わるのは「引数が 2 個以上で、一時値の借用を渡し、結果が借用を持たない」呼び出しが `E1013` から受理になることだけ。仮引数が 1 個のメソッドの呼び出しは従来と同じ判定で、部分適用は従来どおり `E1013`（ユーザーのクラスで HEAD と試作の出力が同じ。`Pairing.two (ref "a")` は両方とも 6 行 26 列の `E1013`）。S6 の確認（`cargo test --locked` で `E1013` を期待するテストが受理で落ちないこと）が要る。

### 設計 4: なぜ `Text` は常に読み込む `.tt` か（決定事項 D1 の要点）

- クラスは `.tt` にしか書けず（`E1018`）、同じ名前の `.tz` は同じモジュールにできない（`E1011`）。だから `Text.length` のような関数をモジュール関数として別に作る層は作れない。メソッドの呼び出しがそのまま `Text.length` になる。
- opt-in にすると、ユーザーのコードはクラスを無修飾で書けず `Text.Text.length` になる（D-40）。`Text` はよく使う名前なので、常に読み込む。常に読み込む std のソースは opt-in のモジュールを名指せないので、`Text.tt`・`String.tz`・`Utf8String.tz` は `Regex` に触れない。そのため `Regex` との橋は `RegexText` に分けた（Phase 2）。
- 読み込んでも使わない `Text` は、IR にも特殊化の予算にも出ない（付録 B.2・B.3）。

### 設計 5: 将来の拡張の型

`Unicode` の `_utf8` の対（付録 A.6）も、Phase 2 と同じ型（フック用のクラス＋ `*_text` の薄い関数）で包める。新しいメソッドは既定の実装を付けて `Text` に足す（R7）。`Json` に包みを足すなら、`utf8string` の内容用の経路（現在は `string`）を先に設計する（決定事項 D5）。

## 前提とする他チケットのインターフェース

D14 は他のチケットに依存しない。次のチケットが `Text` を扱う側になる。どれも未実装でも D14 は完了する。

| チケット | 使う名前・形 | 未実装・実装済みのときの動き |
| --- | --- | --- |
| G21（`guide`。std の索引） | G21 の `collect` は `stdlib::SOURCES` の各ソースを `parser::parse_with_source` で読み、`program.functions`・`externs`・`classes` の各メソッド・型を行にする（G21 付録 J の「クラスのメソッドの行」）。`.tt` の `Text` のメソッドは `Text.find :: Text<'s> => ref 's -> ref 's -> Maybe<i64>` の 1 行になる。`RegexText` は opt-in として既存の規則に従う。D-56 の上限（1 モジュールのページ 6,000 字、std 全体の 1 行索引 45,000 字）に対し、`Text` は約 21 行 ≒ 1.7 KB | 未実装: 何もしない。実装済み: `Text.tt` を足すだけで `guide std Text` に 21 行が載る（G21 側の変更は要らない。Phase 3 の確認で `guide std Text` に `Text.find` の行があることを見る） |
| G26（facts） | D-52 の ID: クラス `Text.Text`（kind `class`）、メソッド `Text.Text.<メソッド名>`（21 個。kind `method`。例 `Text.Text.length`）、instance `instance:Text.Text<string>`・`instance:Text.Text<utf8string>`、Phase 2 は `RegexText.RegexText`・`instance:RegexText.RegexText<string>`・`instance:RegexText.RegexText<utf8string>` | facts が std を対象にするなら上の ID。ユーザーのコードの `Text.find` の呼び出しは、`calls` の辺で `Text.Text.find` へ向く |
| G34（doc-tests） | `text.md` の 4 例と `regex.md` の 1 例は `check-docs.mjs` が検査する。G34 の Phase 3 の `tests/std_examples.rs`（`ls tests/std_examples.rs`）がある木では、`FLOORS` が `SOURCES` の全モジュールを要るので、`("Text", 0)`・`("RegexText", 0)` を足す（クラスのメソッドは `program.functions` に入らないので、`tally()` は数えず、例は要らない。D14 が公開 `def` を足したら、GUIDE-LITE §7 のとおり例を付ける） | G34 が `check-docs.mjs` を置き換えたら、同じページを G34 の対象に入れる。`tests/std_examples.rs` がなければ何もしない |
| D13（補間の穴の一時値。`{s.length}` の `E1013`） | D14 の例・テストは補間の穴に `.length` を直接書かない（`Text.length x` の呼び出しは書ける） | D13 が先に入っても D14 に影響しない |
| A19・A20（純粋関数・効果の要約） | `Text` のメソッドと instance に属性を足さない | A20 は instance の本体から効果を推論する。D14 は何もしない |

## 実装手順

各手順の最後でビルドと既存のテストが緑であること（手順 2 だけは、新しいテストが赤で終わる。それ以外は緑のまま）。コマンドはリポジトリのルートで走らせる。

### Phase 1: `Text` クラス

**手順 1: ベースライン**
- 変更: なし。
- 内容: `cargo build --release --locked`、`cargo test --locked`（失敗の名前を `target/d14-baseline.txt` に残す）、`cp target/release/tsuzuri target/tsuzuri-before`、`node tests/features.mjs target/release/tsuzuri string_library`、`node tests/e2e.mjs target/release/tsuzuri`。
- 確認: 失敗は `bounds_nested_builder_expansion_not_just_source_syntax` だけ（S1）。`string_library` は `features: 3230 cases passed`、`e2e.mjs` は終了コード 0。

**手順 2: テスト先行（赤で終わる）**
- 変更: `tests/fixtures/text_class/Main.tz`・`text_class_direct/Main.tz`・`text_class_calls/Main.tz`・`text_user_instance/Main.tz`（新規。付録 2 の E.1・E.3・E.5）、`tests/features.mjs`（E.2 の 2 つの塊）、`tests/text_class.mjs`（新規。E.4）、`tests/text_class.rs`（新規。E.5。`cargo fmt --all` で整える）。
- 内容: 付録の全文を写す。fixture の字下げ（`text_class`・`regex_text` はスペース 4 個、`text_class_direct`・`text_class_calls`・`text_user_instance` はタブ）を変えない。
- 確認: `target/release/tsuzuri check tests/fixtures/text_class` が `error[E1016]: unknown type class 'Text'`。`cargo test --locked --test text_class` が `running 9 tests` で 8 件が失敗（`Text` がないため。`unused_text_instances_never_reach_the_ir` だけは HEAD でも通る。あとの退行を防ぐ検査）。`node tests/features.mjs target/release/tsuzuri text_class` が失敗。それ以外の既存のテストは緑のまま。

**手順 3: コンパイラの 1 腕**
- 変更: `src/constants.rs`（付録 D.1）。
- 内容: `temporary_borrows` の `single_stage` の `match` に `TypedExprKind::Method(..)` の腕を足す。
- 確認: `cargo build --release --locked` が通る。付録 B.1 の再現を `target/d14-r1/`（`Pairing.tt` と `Main.tz`）に作り、`target/release/tsuzuri run target/d14-r1` が `2 2 4`（変更前は `E1013`）。`cargo test --locked` の全体が手順 1 と同じ失敗だけ（既存のテストが 1 つも新しく落ちない。S6）。

**手順 4: std・予約名・既存テストの調整**
- 変更: `std/Text.tt`（新規。付録 C.1）、`std/String.tz`（C.2）、`std/Utf8String.tz`（C.3）、`src/stdlib.rs`（D.2。単体テスト `reserves_the_d07_table` は D.4 の 1 行目）、`tests/stdlib.rs`（D.4）、`git mv tests/fixtures/computations/Text.tc tests/fixtures/computations/Phrase.tc` と `tests/computations.rs`・`tests/fixtures/computations/Main.tz` の `Text` → `Phrase`（D.4）、`vsc/src/core.ts`（D.4）、`src/polymorph.rs` の単体テスト（D.5）。
- 内容: 付録 C・D の全文を写す。`SOURCES`・`RESERVED_MODULES` は末尾に足す。instance の頭は裸のクラス名（`instance Text<string>`）。
- 確認（すべて通る）:
  - `cargo build --release --locked`。
  - `cargo test --locked --test text_class`: `running 9 tests`、すべて ok。
  - `cargo test --locked --lib reserves_the_d07_table` と `cargo test --locked a_text_method_costs`: それぞれ `running 1 test`、ok。
  - `cargo test --locked --test stdlib --test computations --test modules --test json --test constants --test type_aliases`: ok。
  - `cargo test --locked` の全体が手順 1 と同じ失敗だけ。`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`。
  - `(cd vsc && npm run test:unit)`。
  - `node tests/features.mjs target/release/tsuzuri text_class`: `text_class: native/WASM at O0/O3 passed` と `features: 4595 cases passed`。
  - `node tests/text_class.mjs target/release/tsuzuri`: `text class: 40 instance methods, each one call; no indirect call`（S3）。

**手順 5: 既存への影響の確認と CI**
- 変更: `.github/workflows/targets.yml`（付録 D.6 の step を足す）。
- 内容: 付録 H の手順で IR を比べる（`target/ir_compare.py` を付録 H から写して使う。コミットしない）。付録 B.5 の 20 の suite（`typeclasses`・`higher_kinds`・`deriving`・`string_interpolation`・`constants`・`const_functions`・`const_statics`・`string_library`・`regex`・`unicode`・`chars`・`json`・`cbor`・`display_parse`・`stdlib`・`maybe_result`・`visibility`・`dyn_dispatch`・`borrowed_comparisons`・`generic_records`）と `tests/e2e.mjs` を走らせる。`Utils/Text.tz` を含むディレクトリを `check` する（付録 B.4 の最後の行）。
- 確認: IR の比較が `DIFFERENT: 0`（S2）。suite は件数が付録 B.5 と同じで `passed`。`e2e.mjs` は終了コード 0。`Utils/Text.tz` は通る。PR の CI の job（`targets`）が緑。

**手順 6: 文書**
- 変更: 付録 3 の G.1（`text.md` を新規）、G.3・G.4 の Phase 1 の行。
- 内容: 文面をそのまま使い、`run=` の値と「実行結果」を実際の出力で確かめる。
- 確認: `node scripts/check-docs.mjs <変更したページ>` が `Docs: …` の 1 行で成功する（`text.md` の 4 例が `-O0`・`-O3` で通る）。`grep -rn "53 個" _tsuzuri docs README.md` が 0 件。

**手順 7: 仕上げ**
- 内容: PR の説明に、`docs/stability.md` の既知の例外（`Text` が予約名になる。`Text.tz`・`Text.tt`・`Text.tc` は `E1011`）、D.4 の既存テストの変更の 10 行、付録 B の測定の要約を書く。`git status` に意図しないファイルがないことを確かめる（`target/` は管理外）。
- 確認: `cargo test --locked`・`cargo clippy`・`cargo fmt`・E2E がすべて手順 1 と同じかそれ以上。

### Phase 2: `RegexText` と `Regex.*_text`

**手順 1: テスト先行（赤で終わる）** — 変更: `tests/fixtures/regex_text/Main.tz`（F.1）、`tests/features.mjs`（F.2。suite `regex_text` と参照値）、`tests/text_class.rs` に 3 件（F.3）。確認: `target/release/tsuzuri check tests/fixtures/regex_text` が `E1002: module 'Regex' has no function or union case 'find_all_text'`。

**手順 2: std** — 変更: `std/RegexText.tt`（新規。C.4）、`std/Regex.tz`（C.5）、`src/stdlib.rs`（D.3 と D.4 の Phase 2 の行）、`tests/stdlib.rs`（D.4）、`vsc/src/core.ts`（D.4）。確認: `cargo build --release --locked`、`cargo test --locked --test text_class`（`running 12 tests`、すべて ok）、`cargo test --locked --lib stdlib`（`reserves_the_d07_table`・`sources_for_loads_named_opt_in_modules_and_their_uses`・`opt_in_modules_are_reached_only_through_their_names` が ok。S7）、`node tests/features.mjs target/release/tsuzuri regex_text`（`features: 66 cases passed`）、`regex`（2,273 件のまま）。

**手順 3: 既存への影響と CI** — 付録 H の IR の比較（`Regex` を使うプログラムと使わないプログラムの両方）が `DIFFERENT: 0`。`.github/workflows/targets.yml` の step に `node tests/features.mjs target/release/tsuzuri regex_text` を足す（付録 D.6）。

**手順 4: 文書** — 付録 3 の G.2（`regex.md`）と G.3・G.4 の Phase 2 の行。`node scripts/check-docs.mjs _tsuzuri/language-reference/built-in-types-and-modules/regex.md …` が成功。

### Phase 3: 測定と接続

**手順 1: 測定の記録** — 変更: `docs/benchmarks.md`（付録 3 の G.5 の節。表は自分の測定に置き換える）。確認: `node tests/text_class.mjs target/release/tsuzuri --objects` の表が節の表と同じ。

**手順 2: 接続（接続先が実装済みの場合だけ）** — G21 が実装済みなら、`tsuzuri guide std Text` に `Text.find :: Text<'s> => ref 's -> ref 's -> Maybe<i64>` の行があることを確かめる（G21 の `collect` がクラスのメソッドを最初から読むので、G21 側に足すものはない。なければ G21 の付録 J の「クラスのメソッドの行」が実装されていないので、G21 の実装を直す側に報告する）。G26・G34 も同じ。未実装なら何もしない。

**手順 3: 最終確認** — 受け入れ条件の全項目。

## テスト計画

名前は `ファイル::関数`。期待値は付録の全文にある。独立した参照は、JavaScript の標準の機能（`String`・`RegExp`・`Buffer`）と、HEAD の直接の関数（`string_library`・`regex` の suite）。

### Rust（`tests/text_class.rs`。Phase 1 の 9 件、Phase 2 の 3 件。コードは付録 2 の E.5・F.3）

| 番号 | 関数 | 入力 | 期待 |
| --- | --- | --- | --- |
| T1 | `every_method_type_checks_for_both_encodings` | `text_class_calls`・`text_class` の fixture | 型検査が通り、両ターゲットのライブラリの IR が決定的で重複宣言がない |
| T2 | `generic_code_compares_texts_through_the_ord_superclass` | `Text<'s> =>` の関数が `deref left < deref right` を使い、`string` と `utf8string` に使う | 通る |
| T3 | `every_method_takes_a_literal_argument_without_a_let` | 全メソッドの 30 行（`ref` つき）と 12 行（`ref` なしのリテラル） | すべて型検査が通る（HEAD の同じ形のクラスでは 12 メソッドが `E1013`） |
| T4 | `partial_application_and_loan_returning_methods_still_need_a_let` | `let f = Text.find "a"`、ユーザーのクラスで `ref 'a` を返すメソッドへのリテラル | どちらも `E1013`（`borrow requires a local place`）。結果が `i64` のメソッドは通る |
| T5 | `a_user_type_can_be_a_text` | `text_user_instance`、`deriving` を外したもの、`trim_end` を外したもの | 通る／`E1027`（`no instance for Eq<Main.Name>`）／`E1016`（`missing method 'Text.Text.trim_end'`） |
| T6 | `wrong_uses_report_the_existing_diagnostics` | `Text.length (ref 5)`、`Text.find "a" u8"abc"`、`Text.reverse "abc"`、`dyn Text` | `E1005`・`E1003`・`E1002`・`E1028`（仕様 5 のメッセージ） |
| T7 | `text_is_a_reserved_module_and_namespace` | `namespace Text`、`namespace Text::Tools` | `E1011`（`a name reserved for the standard library`） |
| T8 | `declarations_named_text_coexist_with_the_class` | `record Text`・`union Text`・`class Text`（`Traits.tt`）を宣言したプログラム | 通る。`union Text` で `Text.length` は `E1002: union 'Main.Text' has no case 'length'` |
| T9 | `unused_text_instances_never_reach_the_ir` | `String.length`・`Utf8String.length` だけを使うライブラリ | IR に `@tz.fn.String.$instance`・`@tz.fn.Utf8String.$instance` がない |
| R1 | `regex_text_functions_take_both_encodings` | `regex_text` の fixture | 型検査が通り、IR が決定的 |
| R2 | `regex_text_functions_reject_a_text_without_an_instance` | `Regex.is_match_text (ref re) (ref 5)` | `E1005: no instance for RegexText.RegexText<i32>` |
| R3 | `the_regex_text_class_is_named_with_its_module_outside_std` | `RegexText<'s> =>`、`RegexText.find_all`、`RegexText.RegexText<'s> =>`、`RegexText.RegexText.find_all` | `E1016`／`E1002`／通る／通る |

### Rust の単体テスト（`src/`）

| 関数 | 場所 | 内容 |
| --- | --- | --- |
| `reserves_the_d07_table` | `src/stdlib.rs` | `RESERVED_MODULES.len()` が 54（Phase 2 は 55）、`is_reserved_module("Text")`（Phase 2 は `"RegexText"` も） |
| `sources_for_loads_named_opt_in_modules_and_their_uses` | `src/stdlib.rs` | Phase 2: `Regex` を名指すと `["Regex", "Unicode", "RegexText"]` |
| `opt_in_modules_are_reached_only_through_their_names` | `src/stdlib.rs` | 変更なし。Phase 2 の `OptIn` の形（付録 D.3）がこのテストを満たす |
| `a_text_method_costs_one_more_specialization_than_the_direct_call` | `src/polymorph.rs` | 付録 D.5。`String.length "abc"` は総数 − 1 の記録で通り、`Text.length "abc"` は − 2 で通り、− 1 では `E1017` |
| `rejects_reserved_modules_and_invalid_std_sources` | `tests/stdlib.rs` | `Text.tz`・`Text.tt`・`Text.tc`（Phase 2 は `RegexText` の 3 つも）が `E1011`（`reserved for the standard library`） |

### E2E と構造のテスト

| 名前 | コマンド | 内容・期待 |
| --- | --- | --- |
| suite `text_class` | `node tests/features.mjs target/release/tsuzuri text_class` | fixture `text_class` の 15 個の `export` を native と WASM の `-O0`・`-O3` で走らせ、JavaScript の参照値と照合（付録 E.2）。`native/WASM at O0/O3 passed`、`features: 4595 cases passed`。`inspect` が、辞書・vtable がなく、instance のメソッドが関数として存在することを検査。トラップ 2 件（負の `repeat`） |
| suite `regex_text` | `node tests/features.mjs target/release/tsuzuri regex_text` | JavaScript の `RegExp` との照合と `same_as_direct`（付録 F）。`features: 66 cases passed` |
| `tests/text_class.mjs` | `node tests/text_class.mjs target/release/tsuzuri [--objects]` | IR の構造（instance のメソッド 40 個が 1 回の呼び出しで、呼び先が同名の関数。呼び出し側に間接呼び出しがない。直接の版に instance のメソッドがない）。`--objects` は表を出す測定で、表明ではない |

### 既存テストへの影響

- 変わるもの: 付録 D.4 の表の行だけ（`reserves_the_d07_table`、`sources_for_loads_named_opt_in_modules_and_their_uses`、`tests/stdlib.rs` の追加、`tests/computations.rs` とフィクスチャの `Text` → `Phrase`、`vsc/src/core.ts`）。
- 変わらないもの（根拠つき）: `tests/modules.rs`（依存パッケージの `Text.tz`。付録 B.4 の再現で HEAD と試作が同じ）、`tests/json.rs`・`tests/constants.rs`・`tests/type_aliases.rs`（ユーザーの `Text`。付録 B.4 の 8 件で出力が一致）、`honors_the_exact_specialization_limit`（未使用の `Text` は 0 個。付録 B.2）、`tests/e2e.mjs`（試作で終了コード 0）、20 の suite（付録 B.5）。生成シンボルの番号を決め打ちする追跡済みのテストはない（`tests/fixtures/net_sockets/Main.ll` は管理外）。
- ミューテーションの確認（手順 4・Phase 2 の手順 2 で 1 回ずつ）: `tests/features.mjs` の期待値を 1 か所変えると、その suite が失敗する（`literal_arguments` を 22608n、`regex_text` の `_groups` の 3n を 4n）。

## ドキュメント

更新するページ（文面は付録 3。`Text` の分は Phase 1、`RegexText` の分は Phase 2）。

| 場所 | 内容 | 付録 |
| --- | --- | --- |
| `_tsuzuri/language-reference/built-in-types-and-modules/text.md`（新規） | 型クラス `Text` のページ（4 つの実行例、メソッドの表、入れない関数、費用、注意点） | G.1 |
| `…/built-in-types-and-modules/regex.md` | 「型を選ばずに使う」の節、API の表に 7 行 | G.2 |
| `…/string.md`・`utf8string.md`・`char.md`・`utf8char.md`・`json.md`・`unicode.md`、`literals-and-strings/strings.md`、`types-and-type-inference/type-classes.md`、`index.md`、`organizing-tsuzuri/modules.md` | `Text` への案内、予約名の数（53 → 54 → 55）と一覧 | G.3 |
| `docs/language.md`・`docs/architecture.md`・`README.md` | 予約名の一覧、一時値の借用の段落、文字列の表の次に `Text` の段落、`Regex` の節、アーキテクチャの段落 | G.4 |
| `docs/benchmarks.md`（Phase 3） | 節「Text クラスの呼び出し（D14）」 | G.5 |
| `docs/stability.md` | 変更なし（「std へのモジュールの追加」が既にある）。PR の説明に書く | — |
| `_features/README.md`・`GUIDE.md`・台帳 | 統括者が更新する。報告に「ledger に足す名前」を書く（D-55: std `Text`、`RegexText`、関数 `Regex.*_text`。診断コードの追加なし） | — |

検証コマンド:

```sh
node scripts/check-docs.mjs \
  _tsuzuri/language-reference/built-in-types-and-modules/text.md \
  _tsuzuri/language-reference/built-in-types-and-modules/regex.md \
  _tsuzuri/language-reference/built-in-types-and-modules/string.md \
  _tsuzuri/language-reference/built-in-types-and-modules/utf8string.md \
  _tsuzuri/language-reference/built-in-types-and-modules/char.md \
  _tsuzuri/language-reference/built-in-types-and-modules/utf8char.md \
  _tsuzuri/language-reference/built-in-types-and-modules/json.md \
  _tsuzuri/language-reference/built-in-types-and-modules/unicode.md \
  _tsuzuri/language-reference/literals-and-strings/strings.md \
  _tsuzuri/language-reference/types-and-type-inference/type-classes.md \
  _tsuzuri/language-reference/organizing-tsuzuri/modules.md \
  _tsuzuri/language-reference/index.md
```

試作で `text.md` と `regex.md` の 2 ページは `Docs: 2 pages, 17 links, 10 checked examples, 20 native runs (O0/O3), 0 test projects.` だった。ほかのページはリンクの追加だけなので、リンクとアンカー（`text.md#入っていないもの`）が解決することを確かめる。

## 受け入れ条件

### Phase 1

- [ ] `std/Text.tt`・`instance Text<string>`・`instance Text<utf8string>` が付録 C.1〜C.3 のとおり。メソッドは 21 個で、instance の本体は 1 回の呼び出し。
- [ ] `src/constants.rs` の変更が `Method` の腕 1 つだけ。`RESERVED_MODULES` が 54 個。
- [ ] `cargo test --locked --test text_class` が `running 9 tests` ですべて ok。`cargo test --locked` が手順 1 と同じ失敗だけ。`cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings` が通る。`npm run test:unit`（`vsc`）が通る。
- [ ] suite `text_class` が `features: 4595 cases passed`、`tests/text_class.mjs` が `40 instance methods, each one call`。
- [ ] 付録 H の IR の比較が `DIFFERENT: 0`。付録 B.5 の 20 の suite と `tests/e2e.mjs` が通る。`.github/workflows/targets.yml` に付録 D.6 の step があり、PR の CI が緑。
- [ ] `text.md` ほかの文書が `check-docs` を通る。`grep -rn "53 個" _tsuzuri docs README.md` が 0 件。
- [ ] 既存テストの変更が付録 D.4 の行だけ。新しい診断コード・予約語・crate・WASM の import がない。
- [ ] PR の説明に、`docs/stability.md` の既知の例外と測定の要約がある。

### Phase 2

- [ ] `std/RegexText.tt`・`Regex.tz` の追記が付録 C.4・C.5 のとおり。`RESERVED_MODULES` が 55 個。`OPT_IN` が付録 D.3 のとおり。
- [ ] `cargo test --locked --test text_class` が `running 12 tests` ですべて ok。`opt_in_modules_are_reached_only_through_their_names` ほかの `src/stdlib.rs` のテストが ok。
- [ ] suite `regex_text` が `features: 66 cases passed`。suite `regex` が 2,273 件のまま通る。`Regex` を名指さないプログラムの IR が変わらない。
- [ ] `regex.md` ほかが `check-docs` を通る。

### Phase 3

- [ ] `docs/benchmarks.md` の節の表が、`node tests/text_class.mjs target/release/tsuzuri --objects` の結果と同じ。
- [ ] G21・G26・G34 が実装済みなら接続した（未実装なら記録だけ）。

## 落とし穴

| 落とし穴 | 検出 |
| --- | --- |
| `Text.tz` と `Text.tt` を同じ場所に置く（同じモジュール名） | `E1011: module '…' is declared by more than one file`。`Text` のモジュールは `Text.tt` だけ |
| instance の頭を `Text.Text<string>` と書く、または別のモジュールの instance にする | std の中では裸の `Text<string>`。`tests/text_class.mjs` の `@tz.fn.String.$instance` の正規表現が合わない |
| `SOURCES` の途中・先頭に足す | `git diff src/stdlib.rs`。ソースの添字がずれて IR の比較（付録 H）が `DIFFERENT`。末尾に足す |
| `Text.tt`・`String.tz`・`Utf8String.tz` に opt-in のモジュール名（`src/stdlib.rs` の `OPT_IN` の `module` に並ぶ名前。たとえば `Regex`・`Json`）を書く | `cargo test --locked --lib opt_in_modules_are_reached_only_through_their_names`（常に読む std が opt-in のモジュール名を名指すと落ちる。`///` の中は数えない） |
| メソッドに `'s` 以外の型変数（たとえば `'t`）を足す | `E1016: each class method must mention exactly the declared class type variable`（`tsuzuri check` で出る） |
| instance のメソッドに余計な処理を書く（`ref` の付け替え、`clone`、`match`） | `tests/text_class.mjs` が「1 回の呼び出し」で落ちる |
| `Text.tc` の改名を `tests/computations.rs` の 3 か所の文字列・定数と `tests/fixtures/computations/Main.tz` に反映し忘れる | `grep -rn "Text {\|Text\.tc" tests/` の残りが `tests/type_aliases.rs` の 1 行（型エイリアス `Text`。無関係）だけになり、`grep -n "\bTEXT\b" tests/computations.rs` が 0 件になるまで直す |
| `vsc/src/core.ts` の `libraryModules` に足し忘れる | `(cd vsc && npm run test:unit)` |
| `Text` を opt-in にしたくなる | ユーザーのコードが `Text.Text.length` になる（D-40。決定事項 D1） |
| fixture の `u8"\uFF5E"` | `\u` に続く 4 桁の escape は `utf8string` のリテラルで無効。`u8"\u{FF5E}"` と書く（付録 A.8・text.md の例） |
| 補間の穴に `{x.length}` を書く | `E1013`（D13 の担当）。`Text.length x` の呼び出しか、先に `let` で束縛する |
| `Array.sort` で `[string]` を並べる例を文書に書く | `Array.sort` は `Copy` を要求して `E1005`。例に使わない |
| `compare` の結果を `<` と同じだと文書に書く（単位が違う型の間で比べる） | `Text.compare` は同じ型の 2 つの値だけ。`string` と `utf8string` を混ぜると `E1003` |
| `--emit llvm` の IR を `-O` の違いで比べる | `--emit llvm` は `-O` を無視し、`-O0` と `-O3` で同じバイト列。オブジェクトコードの比較は `--objects` |
| `features.mjs` の BigInt の多い suite を Node 20 で走らせる | 異常終了することがある。`text_class` と `regex_text` は Node 20 で通る。ほかは Node 24 |

## 対象外

- `Json` の包み（付録 A.5: 対になる関数が 0）、`Char`・`Utf8Char` の型クラス、`Unicode` の `_utf8` の対（付録 A.6）。
- `lines`・`to_upper`・`to_lower`・`char_count` の統一、新しい操作（`Text` に入れるのは既存の関数の共通部分と `is_empty` だけ）。
- `String.*`・`Utf8String.*` の非推奨・削除・改名、暗黙の型変換、リテラルの型・既定のエンコーディングの変更。
- SIMD・並列・GPU などの新しいアルゴリズム、速度の合否の閾値。
- `dyn Text`（`E1028`。ジェネリックな関数を使う）。
- 新しい診断コード・予約語・crate・WASM の import。
- G21・G26・G34 の実装そのもの（接続の引き継ぎは「前提とする他チケットのインターフェース」に書いた）。

## 決定事項

すべて承認済み（D-50）。実装者は蒸し返さない。

| 番号 | 決定 | 理由 | 却下した案 |
| --- | --- | --- | --- |
| D1 | `Text` は、常に読み込む `std/Text.tt` のクラス（モジュールも `Text`）。`Text.f` をモジュール関数として別に作らない | クラスは `.tt` にしか書けず、同名の `.tz` は作れない（本文の「現状」4）。常に読み込めば `Text.length` と無修飾で書ける。未使用の分は IR・予算に出ない | opt-in（ユーザーが `Text.Text.length` と書くことになる。D-40）。モジュール関数の層（`.tt` と `.tz` が共存できず、特殊化が増える）。コンパイラの組み込みクラス（コンパイラの面が増え、std の `.tt` で足りる） |
| D2 | スーパークラスは `Ord`（`Eq` を含む） | ジェネリックな本体で `<`・`==` が使える。`compare` が `<` と同じ向きになる | なし（比較できない）、`Eq` だけ（順序が使えない） |
| D3 | メソッドは 21 個: 共通の `def` 18 ＋ `compare`・`clone` ＋ 既定の `is_empty`。`char_count`・`chars`・`decode_at`・変換・`is_well_formed` 系は入れない | 「`Text.f` は常に `String.f`／`Utf8String.f` と同じ」と言い切れるものだけ。`char_count` は同名で意味が違う（O(1) のコード単位数と O(n) のスカラー数）ので、入れると統一にならない | 種にあった `lines`・`to_upper`・`to_lower`（`String`・`Utf8String` にない。`Unicode` にある `to_upper` は完全な変換で意味が違う） |
| D4 | `Regex` は、opt-in のフック用クラス `RegexText`（7 メソッド）と、それを使う 7 つの薄い関数 `Regex.*_text` | 対象の 6 モジュールで、`String`／`Utf8String` の次に数の多い二重（公開 18 関数のうち 7 組が `_utf8` の別名）。クラスのメソッドは 1 つの型変数 `'s` しか使えないので、エンジンをジェネリックにせず、既存の 2 版へ委譲する | エンジンを単位の取り出しでジェネリックに書き直す（コストとリスクが大きい）。訪問者のメソッド（メソッドの型変数が要り、`E1016` になる）。コンパイラの組み込みの振り分け。何もしない（二重が残る） |
| D5 | `Json` は包まない | 公開 38 関数に対がない。文書は `utf8string`、内容は `string` に固定で、同じ操作の 2 版がない（付録 A.5）。`string` の文書を受けると O(n) の変換を隠し、エラーの位置も変わる | `deserialize_text`（隠れた変換）。`utf8string` の内容の経路を先に作る（別のチケット） |
| D6 | `Char`・`Utf8Char` に型クラスは作らない | `Text` から文字を取り出す手段を入れない（`chars`・`decode_at` は意味が違う）ので、文字の型を `'s` から決められない | `Character<'c>` の型クラス（使い道がない） |
| D7 | `temporary_borrows` に `Method` の腕を足す | これがないと 21 メソッドのうち 12 がリテラルを受けられない（付録 B.1）。ユーザーのクラスも同じ不具合なので、直す価値が広い。受理が増えるだけ | 制限として文書に書くだけ（`Text.find "a" text` が通らず、目的に反する）。メソッドごとのラッパー（`Text` のモジュール関数が作れない）。std のクラスだけに絞る（同じコードの経路で、理由がない） |
| D8 | instance は `std/String.tz`・`std/Utf8String.tz` の末尾に、裸のクラス名で書く | std のコードはユーザーのクラスを見ず、std のクラスだけが見える。新しいモジュールを足さない | 別のファイル（`TextInstances.tz`。予約名が 1 つ増え、読み込みも増える）。修飾した頭 `Text.Text<string>`（不要） |
| D9 | `String.*`・`Utf8String.*` は非推奨にしない。instance の本体は 1 回の呼び出し | std の公開 API は削除しない（`docs/stability.md`）。型が決まっている場所では直接の呼び出しのほうが短い。アルゴリズムを 2 か所に持たない | 非推奨（利益がなく、`String.*` を使う全ユーザーに `W1005` が出る）。instance に実装を移す（二重になる） |
| D10 | ユーザーの型にも `Text` の instance を書ける。`Ord`（と `Eq`）と、`is_empty` 以外の 20 メソッドが要る。将来のメソッドには既定の実装を付ける | クラスの機構のままで、特別扱いが要らない | ユーザーの instance を禁止（機構にない規則が増える） |
| D11 | フィクスチャの `tests/fixtures/computations/Text.tc` は `Phrase.tc` に改名する | `Text` が予約名になり、ビルダー名はファイル名から決まる | 改名せず落とす。`Text` を予約しない（クラスが読み込めなくなる） |
| D12 | ゼロコストの保証は、IR の構造のテスト（instance のメソッドが 1 回の呼び出し、間接呼び出しなし）で行い、オブジェクトコードの一致は測定（`--objects`）にとどめる | 小さいプログラムではバイト一致するが、大きいプログラムは LLVM の組み込みの判断で wasm32 の `-O3` が +1.2%、native の `-O1` が数命令動く（付録 B.3）。AGENTS.md の方針（速度の合否を共有の CI に置かない、実測を主張と分ける） | 全構成のバイト一致を表明する（LLVM の判断に依存して不安定）。速度の閾値 |

## 付録

| ファイル | 内容 |
| --- | --- |
| [D14-appendix.md](D14-appendix.md) | A 署名の突き合わせ表（`String`／`Utf8String` 21＋20、組み込み 18、`Char`／`Utf8Char` 6＋6、`Regex` 18、`Json` 38、参考に `Unicode` 24）と件数のコマンド、B 実測（E1013、特殊化の予算、IR とオブジェクトコード、既存への影響、検証の結果）、C std の骨格の全文、D Rust の差分と既存テストの変更の表、CI の step |
| [D14-appendix-tests.md](D14-appendix-tests.md) | E Phase 1 のテストの全文（fixture 4 つ、`features.mjs` の差分、`tests/text_class.mjs`、`tests/text_class.rs`）、F Phase 2 のテストの全文 |
| [D14-appendix-docs.md](D14-appendix-docs.md) | G 文書の草稿（`text.md` の全文、`regex.md` の差分、他のページ・`docs`・`README.md` の編集箇所、`docs/benchmarks.md` の節）、H IR を比べる手順とスクリプト |



## 実装と検証（2026-10-31）

- **範囲**: Phase 1〜3 をすべて実装した（1 つのブランチ）。冒頭の「方針の最新化」のとおり、A18・D-59・F14・G34・G35 の後の現状に合わせた。std の骨格（付録 C）とテスト（付録 2）は、引数の順序を D-59 のまま（本文が最後。`String` 系の最後の仮引数の名前は `text`／`parts`）、構文を edition 2027 にして使った。
- **Phase 1**: `std/Text.tt`（`class Ord<'s> => Text<'s>`、21 メソッド。クラスと全メソッドに `///` の例）、`std/String.tz`・`std/Utf8String.tz` の末尾の `instance Text<string>`・`instance Text<utf8string>`（本体はそれぞれ 1 回の呼び出し）。`src/constants.rs` の `Checker::temporary_borrows` に `TypedExprKind::Method(..)` の腕（コンパイラの変更はこれだけ）。`src/stdlib.rs` の `SOURCES`・`RESERVED_MODULES` の末尾に `Text`（54 → 55）。`vsc/src/core.ts` の `libraryModules`。`git mv tests/fixtures/computations/Text.tc …/Phrase.tc` と `tests/computations.rs`・`Main.tz` の `Text` → `Phrase`。テスト: `tests/text_class.rs`（9 件）、`tests/fixtures/{text_class,text_class_direct,text_class_calls,text_user_instance}`、`tests/features.mjs` の suite `text_class`、`tests/text_class.mjs`、`src/polymorph.rs` の `a_text_method_costs_one_more_specialization_than_the_direct_call`（直接の呼び出しの境界も確かめる）、`tests/stdlib.rs` の `Text.tz`・`Text.tt`・`Text.tc`。`.github/workflows/targets.yml` に step（`text_class`・`regex_text`・`text_class.mjs`）。
- **Phase 2**: `std/RegexText.tt`（opt-in、7 メソッド）、`std/Regex.tz` の末尾の 2 つの instance と `Regex.*_text` の 7 関数（例・`@since 0.1.0`・`@see`）。`OPT_IN` の `Regex` の `names`・`uses` に `RegexText`、`RegexText` の項目（`uses: ["Regex"]`）、`RESERVED_MODULES` 55 → 56。テスト: `tests/text_class.rs` に 3 件（計 12 件）、`tests/fixtures/regex_text`、suite `regex_text`、`sources_for_loads_named_opt_in_modules_and_their_uses` の 3 行、`tests/stdlib.rs` の `RegexText.*`。
- **G34・D-59 への接続**: `tests/std_examples.rs` の `FLOORS` に `("Text", 0)`・`("RegexText", 0)`、`Regex` 18 → 25。`tests/std_argument_order.rs` は `.tt` のクラスのメソッド（字下げした `def`）も読むようにし、`Text`（対象 `'s`・`['s]`）・`RegexText`（`'s`）を分類し、`Regex` の対象に `'s` を足した。読み取りの単体テストとパイプの 2 例を足した。
- **Phase 3**: `docs/benchmarks.md` の節「Text クラスの呼び出し（D14）」（`node tests/text_class.mjs target/debug/tsuzuri --objects` の結果）。接続: G21 は `tsuzuri guide std Text` に `Text.find :: Text<'s> => ref 's -> ref 's -> Maybe<i64>` など 21 行とクラスの行が出る（G21 側の変更なし）。G26 は `kg search` で `Text.Text.find`（`method`）・`instance:Text.Text<string>`・`instance:RegexText.RegexText<string>` が出る（D-52 の ID のとおり）。G34 は上のとおりで、`tsuzuri test --doc std` が `Text` の 22 例（クラスと 21 メソッド）、`RegexText` のクラスの 1 例、`*_text` の 7 例を走らせる。
- **検証**: `cargo fmt --all -- --check`・`cargo clippy --all-targets --locked -- -D warnings` 成功。`cargo nextest run --locked`（`RUST_MIN_STACK=4194304`）は 2,797 件すべて成功（`tests/text_class.rs` の 12 件、`src/` の単体テストの追加・変更を含む）。`target/debug/tsuzuri test --doc std` は `-O0`・`-O3` とも `853 passed; 0 failed`。`node tests/e2e.mjs target/debug/tsuzuri` は終了コード 0（`Polymorphism` の特殊化の上限を含む）。付録 B.5 の 20 の suite のうち 18 は B.5 と同じ件数で `native/WASM at O0/O3 passed`（`typeclasses` 39、`higher_kinds` 17、`deriving` 209、`string_interpolation` 82、`constants` 9、`const_functions` 19、`const_statics` 52、`string_library` 3,230、`chars` 19、`json` 165、`cbor` 86、`display_parse` 10、`stdlib` 3、`maybe_result` 26、`visibility` 7、`dyn_dispatch` 55、`borrowed_comparisons` 10、`generic_records` 10）。`regex` と `unicode` は、この機械の Node 20（V8 の Unicode のデータが古い）で参照値の計算が合わず、変更前のバイナリでも同じ箇所で落ちる（`tz_property_hash` と `V8 words of a:b`）。Node 24 はネットワークなしで用意できず、CI に任せた（IR の比較が `tests/fixtures/regex` を含み、suite `regex_text` は通る）。`node tests/features.mjs target/debug/tsuzuri text_class` は `text_class: native/WASM at O0/O3 passed`・`features: 4595 cases passed`、`regex_text` は `features: 66 cases passed`。`node tests/text_class.mjs target/debug/tsuzuri` は `text class: 40 instance methods, each one call; no indirect call`。付録 H の IR の比較（変更前の dev バイナリと変更後、`examples/*` と `tests/fixtures/*` の 151 個。新しい fixture 4 つを除く）は `byte-identical: 64`・`identical after renumbering: 66`・`both failed to build: 21`・`DIFFERENT: 0`。`node scripts/check-docs.mjs`（13 ページ）は `Docs: 13 pages, 316 links, 62 checked examples, 124 native runs (O0/O3), 0 test projects, 1 generated blocks.`。`node scripts/gen-docs.mjs --check` は `19 blocks checked, 0 stale`、`node scripts/check-cli-docs.mjs` は `4 checks, 0 failed`、`node tests/repo_kb.mjs` は 78 件成功、`node scripts/check-guide-lite.mjs` は 86 件 0 失敗。`vsc` の `npm run test:unit` は、この木に `node_modules` がなく（ネットワークを使わない作業）走らせていない。代わりに `core.test.ts` と同じ比較（`RESERVED_MODULES` と `libraryModules` が同じ 56 個の集合）を Node で確かめた。
- **既存テストの変更**: 付録 D.4 の行だけ（`reserves_the_d07_table` の数は現在値から 56、`sources_for_loads_named_opt_in_modules_and_their_uses`、`tests/stdlib.rs`、`tests/computations.rs` と fixture の `Phrase`、`vsc/src/core.ts`）と、上の G34・D-59 の表。期待値を変えたテストはほかにない。停止条件 S1〜S10 には当たらなかった（S6: `E1013` を期待する既存のテストは、新しい腕のあとも全部 `E1013`）。
- **文書**: `text.md`（新規。引数の順序の節と、下の制限を注意点に）、`regex.md`（節「型を選ばずに使う」、API の表の 7 行、引数の順序）、`string.md`・`utf8string.md`・`char.md`・`utf8char.md`・`json.md`・`unicode.md`・`strings.md`・`type-classes.md`・`argument-order.md`・`modules.md`（予約名 56 個。`Alloc` の抜けも直した）・`index.md`、`docs/language.md`（予約名の 2 つの一覧、opt-in の一覧と `uses`、一時値の借用の段落、文字列の表の次の `Text` の段落、`Regex` の箇条書き）、`docs/architecture.md`（節「Text クラス（D14）」、opt-in の一覧、`RESERVED_MODULES` の数）、`README.md`（一時値の借用、`Regex`、予約名の表）、`docs/benchmarks.md`、`_features/README.md`、GUIDE の D-55 の表。`docs/stability.md` は変えていない（「std へのモジュールの追加」の既知の例外: `Text`・`RegexText` という名前のルートのモジュールと名前空間が `E1011` になる。PR の説明に書く）。
- **見つけた不具合（直していない。後続へ）**: (1) 結果の型が型変数そのものの呼び出し（`Maybe.get :: own Maybe<'a> -> 'a`、`Text.trim :: ref 's -> 's`）を `ref T` の仮引数へそのまま渡すと、`Checker::argument_hint`（`src/check.rs`）が型変数を `ref T` に決めてしまい、引数が合わず `E1003` になる。再現: `let word = "ab"` と `String.length (Maybe.get (String.slice 0 1 (ref word)))`（`expected Maybe<ref string>, found Maybe<string>`。変更前の HEAD でも同じ）、`Text.length (Text.trim "  a ")`（`expected ref an undetermined type, found string`）。edition 2027 では無印の引数も `ref T` なので、`def size :: string -> i64` に `size (Text.clone x)` を渡す形でも出る。直し方の候補は、結果がまだ決まらない型変数のときは期待型を先に単一化せず、引数を見たあとで借用か再借用かを決めること（推論の順序を変えるので、別のチケットにする）。文書には制限と回避（`let` で束縛するか `ref (…)` と書く）を書いた。(2) 部分適用に一時値を借用で渡す形（`text |> Text.find "o"`）は、`String.find` と同じく `E1013`（仕様どおり。文書に回避を書いた）。
- **既知の揺れ**: 最初の `cargo nextest run --locked`（2,797 件）で `kg_query` の 3 件（`json_rows_of_the_queries`・`find_searches_the_graph_by_type`・`show_matches_the_appendix_output`）が 1 秒前後で落ち、単独の `cargo test --test kg_query`（17 件）と `nextest` の再実行（56 件）ではすべて通った。std を変えた直後に、ほかの作業木と共有する std の知識グラフのキャッシュを並行して作り直したためと考えられる（コードの変更による失敗ではない）。
- **レビュー（PR #73）**: `temporary_borrows` の `Method` の腕は、メソッドの宣言の引数の数（`callee.ty`）で「引数を一度に適用する呼び出し」と判断していたが、instance の実装は関数と同じく段階を分けられる（`fn pick text = { let first = Debug.trace 1; \n -> … }`。`instance_function` は宣言より少ない仮引数を許す）。そのため、一時値を借用した最初の引数と副作用のある 2 つ目の引数で `Stage.pick "ab" (Debug.trace 2)` と呼ぶと `BorrowOperand` になり、2 つ目の引数が最初の段階より先に評価されていた（`-O0`・`-O3` とも標準エラーが `2` `1`。言語仕様は最初の段階の後に後の引数を評価する。`let word = "ab"` と束縛すれば `1` `2`）。腕を、型から決まる実装（`Classes::resolved_method`。instance のメソッドかクラスの既定の実装。`pub(super)` にした）の段階の数（`signatures[実装].signature.parameters.len()`。関数の腕と同じ数え方）と実引数の数を比べる形に改めた。型がまだ開いている、ジェネリックな本体の型パラメーター、組み込みの instance のときは、ほかの呼ばれるものと同じ規則（引数 1 個だけ）に落ちる。`Text` の instance はすべての引数に名前を付けるので、`string`・`utf8string` への呼び出しは従来どおり一時値を借用できる（suite `text_class` の `literal_arguments` ほか）。`Text<'s> =>` のジェネリックな本体の中で、引数 2 つ以上のメソッドへ一時値を借用で渡す形は `E1013` になった（`text.md` の注意点と `docs/language.md`・`docs/architecture.md` に書いた）。回帰: `tests/fixtures/text_class_stages`（段階を分けた `string` の instance と、2 つの引数に名前を付けた `utf8string` の instance）、`tests/text_class.rs` の `a_staged_instance_method_still_needs_a_let_for_a_temporary`（一時値の段階的な呼び出しが `E1013`、同じ形の関数も `E1013`、ジェネリックな本体の一時値が `E1013` で `let` なら通る）、`tests/text_class.mjs` が fixture を `-O0`・`-O3` で実行し、標準出力 `221 343` と手順の順 `1` `2` `4` `3`（最初の段階、後の引数、引数、本体）を確かめる。検証: `cargo fmt --all -- --check`・`cargo clippy --all-targets --locked -- -D warnings` 成功、`cargo nextest run --locked` 2,798 件すべて成功（`E1013` を期待する既存のテストを含む）、`cargo test --locked --test text_class` 13 件、suite `text_class` 4,595 件・`regex_text` 66 件・`typeclasses` 39 件、`node tests/text_class.mjs` 成功、`tsuzuri test --doc std --filter Text`／`_text` が 23／14 件成功、`check-docs`（`text.md`）成功。
