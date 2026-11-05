# G38: `features.mjs` の `regex`・`unicode` の期待値が、テストを動かす Node の Unicode の版で変わる（Unicode 18 の Node で失敗する）

| 項目 | 内容 |
| --- | --- |
| ID | G38 |
| 優先度 | P2 |
| 規模 | S |
| 依存 | D09 |
| 後続 | – |
| 状態 | done（V8 の期待値を std と同じ Unicode 17.0 の Node で `tests/unicode-v8.mjs`・`tests/regex-v8.mjs` に書き出して commit した。どの Node でも suite が通り、版が同じ Node は計算し直して比べる） |
| 起票 | 2026-11-05（TZ12 の作業中に発見。HEAD `5ee6b0f2` で確認） |
| 承認 | 不要（テストの参照値の作り方を変える。コンパイラ・std・言語は変えない） |
| 改善する劣位 | 不具合: `node tests/features.mjs <tsuzuri> regex` と `… unicode` が、Unicode 17.0 でない Node（Homebrew の `node@24` は `icu4c@79` に link し、`process.versions.unicode` が `18.0`）で失敗する。std の表は Unicode 17.0.0 に固定されているので、コンパイラは正しく、テストの参照値が実行環境で変わる |
| 手本にする既存実装 | `tests/unicode-cases.mjs` の `--write`（UCD の試験ファイルを SHA-256 で確かめてから `tests/unicode-ucd.mjs` に書き出す）と、`tests/features.mjs` の suite `regex` の `assert.equal(readFileSync(regexCasesPath, "utf8"), regexCasesSource(), "run node tests/regex-cases.mjs --write")` |
| 主な影響ファイル | `tests/unicode-cases.mjs`, `tests/regex-cases.mjs`, `tests/features.mjs`, `tests/unicode-v8.mjs`（新規）, `tests/regex-v8.mjs`（新規）, `scripts/generate-unicode.mjs`, `docs/architecture.md`, `_features/README.md` |

## 目的

`regex` と `unicode` の suite が、開発者の手元の Node の Unicode の版に関係なく、std が固定している Unicode の版（今は 17.0.0）の参照値で
コンパイラの出力を確かめるようにする。今は CI（`actions/setup-node` の Node。ICU を同梱する公式のバイナリ）では通り、Homebrew の Node では通らないので、
手元の T2 の結果が環境で変わる。

参照値をコンパイラの出力から写さないという方針（`tests/unicode-cases.mjs` と `tests/regex-cases.mjs` の冒頭のコメント）は保つ。

## 着手条件と停止条件

### 着手条件

- D09 が `_features/README.md` で done であること（HEAD で done）。
- Unicode 17.0 の Node（`node -p process.versions.unicode` が `17.0`）を用意できること。公式の Node 24 のバイナリ（`actions/setup-node` と同じもの）を
  別のディレクトリに展開して使ってよい。用意できなければ停止する（参照値を書き出せない）。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- Unicode 17.0 の Node で、今の `expectedCases`（両 suite）の値と、書き出した参照値が一致しない。
- Unicode 17.0 の Node で、今の HEAD のコンパイラが `regex`・`unicode` の suite に失敗する（CI の baseline と食い違う）。
- 参照値のファイルが 1 MiB を超える。

## 現状（HEAD `5ee6b0f2` で確認）

- `scripts/generate-unicode.mjs:21` の `VERSION = "17.0.0"`。std の Unicode の表（`std/Unicode.tz`・`std/Regex.tz` と `src/runtime/unicode.ll`）は、
  SHA-256 で固定した Unicode 17.0.0 の UCD から作る。
- `tests/unicode-cases.mjs` の冒頭のコメント: 期待値は UCD の試験ファイル（`tests/unicode-ucd.mjs` に圧縮して commit）と V8（「Node with Unicode 17.0」）から作る。
  V8 から作るのは、一般カテゴリ（190〜192 行。`new RegExp(`^\\p{${name}}$`, "u")`）、正規化（206〜207 行。`String.prototype.normalize`）、
  大文字・小文字（208・251 行。`toUpperCase`・`toLowerCase`）、書記素とワードの区切り（238・265 行。`Intl.Segmenter`）である。
  `expectedCases` はテストのたびにこれらを計算する。実行する Node の Unicode の版は確かめない（`grep -rn "versions.unicode" tests scripts` は 0 件）。
- `tests/regex-cases.mjs` の冒頭のコメント: 期待値は V8（`u` フラグ）か手書き。`property_hash`（281〜284 行の 43 個の名前、468 行）は `\p{…}` で
  コード空間全体を数える。`W`・`D`・`S` などの JavaScript の綴り（11〜14 行）も `\p{Alphabetic}`・`\p{Nd}`・`\p{White_Space}` を使う。
- `tests/features.mjs:1664` の suite `regex` と `:1677` の suite `unicode` は、`get cases()` でこれらを呼ぶ。
- 手元の Node: `node --version` は `v24.22.2`、`process.versions.icu` は `79.1`、`process.versions.unicode` は `18.0`。
  `otool -L /opt/homebrew/opt/node@24/bin/node` は `/opt/homebrew/opt/icu4c@79/lib/libicuuc.79.dylib` を示す（Homebrew の Node は ICU を同梱しない）。

### 再現（検証済み。HEAD `5ee6b0f2`、macOS arm64、Homebrew `node@24` 24.22.2）

```sh
cargo build --locked
node tests/features.mjs target/debug/tsuzuri unicode
node tests/features.mjs target/debug/tsuzuri regex
```

それぞれ native の `-O0` の host の `assert` で止まる:

```text
Assertion failed: (tz_category_chunk(INT64_C(0)) == INT64_C(581508025)), function main, file host.c, line 50.
Assertion failed: (tz_property_hash(INT64_C(0)) == INT64_C(825879436)), function main, file host.c, line 2269.
```

`features.mjs` の `execute` の `assert.equal(result.status, 0)` は `null !== 0`（signal で終わった）と表示する。値はコンパイラの出力（Unicode 17.0.0 の表）と
V8 の Unicode 18.0 の参照値の差である。

## 仕様

テストの振る舞いを次のように変える（コンパイラは変えない）。

- `regex` と `unicode` の suite の V8 由来の期待値は、commit した参照値のファイル（`tests/unicode-v8.mjs`・`tests/regex-v8.mjs`）から読む。
  参照値は、std と同じ Unicode の版の Node で `--write` して作る。
- 実行する Node の `process.versions.unicode` が std の版（`scripts/generate-unicode.mjs` の `VERSION` の major.minor）と同じなら、テストは参照値を
  V8 で計算し直し、commit した値と一致することも確かめる（参照値が V8 から外れていないことの検査）。違えば計算し直さず、その旨を 1 行表示する。
- `--write` は、実行する Node の Unicode の版が std の版と違えば、何も書かずに失敗する。
- UCD の試験ファイルから作る期待値（`tests/unicode-ucd.mjs`）と手書きの期待値は、今のままにする。

## 設計

### 段ごとの変更

| ファイル | 関数・場所 | 変更 |
| --- | --- | --- |
| `tests/unicode-cases.mjs` | `expectedCases` を 2 つに分ける | V8 を呼ぶ部分を `v8Cases()`（新規）にし、`expectedCases` は UCD・手書きの分と、`tests/unicode-v8.mjs` の値を合わせる |
| `tests/unicode-cases.mjs` | `--write-v8`（新規） | Node の Unicode の版を確かめ、`v8Cases()` を `tests/unicode-v8.mjs` に書き出す |
| `tests/regex-cases.mjs` | `expectedCases`・`--write` | 同じ形で、V8 の結果を `tests/regex-v8.mjs` から読む。`--write` は `Cases.tz` と `regex-v8.mjs` の両方を書く |
| `tests/unicode-v8.mjs`・`tests/regex-v8.mjs` | 新規（生成物） | 先頭に生成の命令・Node の版・`process.versions.unicode`・Do not edit を書いた `export const cases = [...]` |
| `tests/features.mjs` | suite `regex`・`unicode` | 参照値のファイルを読み、版が一致するときだけ計算し直して比べる |
| `scripts/generate-unicode.mjs` | `VERSION` を export する | テストが std の版を読めるようにする（D2） |
| `docs/architecture.md` | Unicode の表の検証の段落（1138 行） | 参照値のファイルと、Unicode の版を上げるときの `--write-v8` を書く |

### アルゴリズム

1. `scripts/generate-unicode.mjs` の `VERSION` を `export const UNICODE_VERSION = "17.0.0"` として読めるようにする（生成の処理は import 時に走らない形にする。
   今は top-level で UCD を読むので、`import.meta.url === pathToFileURL(process.argv[1]).href` のときだけ生成する）。
2. `pinned = UNICODE_VERSION.split(".").slice(0, 2).join(".")`（`17.0`）と `process.versions.unicode` を比べる。
3. `v8Cases()` は、今の `expectedCases` の V8 を呼ぶ行を、そのままの順で集めた配列を返す。参照値のファイルは `[export 名, 引数, 期待値]` の配列で、
   `BigInt` は `"123n"` のような文字列ではなく `123n` のリテラルで書く（import した値をそのまま `assert.deepEqual` に渡せる）。
4. テスト時: 参照値のファイルを import し、版が一致すれば `assert.deepEqual(v8Cases(), cases)` を先に行う。

## 実装手順

### 手順 1: ベースラインと再現

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドと「再現」。続けて、Unicode 17.0 の Node（以下 `$NODE17`）で両 suite が成功することを確かめる。
- 確認: `$NODE17 -p process.versions.unicode` が `17.0`。`$NODE17 tests/features.mjs target/debug/tsuzuri unicode` と `… regex` が成功する。成功しなければ停止する。

### 手順 2: `VERSION` を読めるようにする

- 変更: `scripts/generate-unicode.mjs`。
- 確認: `node -e 'import("./scripts/generate-unicode.mjs").then(m => console.log(m.UNICODE_VERSION))'` が `17.0.0` を表示し、UCD を読まない。
  UCD を用意して `node scripts/generate-unicode.mjs <UCD>` を実行し、`git diff --exit-code src/runtime/unicode.ll std/` が差分なし（UCD が手元に無ければ、
  生成の分岐を変えていないことを diff で示し、CI に任せる）。

### 手順 3: V8 の部分を分け、参照値を書き出す

- 変更: `tests/unicode-cases.mjs`、`tests/regex-cases.mjs`、新規の `tests/unicode-v8.mjs`・`tests/regex-v8.mjs`。
- 確認: `$NODE17 tests/unicode-cases.mjs --write-v8` と `$NODE17 tests/regex-cases.mjs --write` が書き出し、`node`（Unicode 18.0）での同じ命令は
  `the Unicode of this Node (18.0) is not std's (17.0); use a Node with Unicode 17.0` のように失敗して何も書かない。

### 手順 4: suite が参照値を使う

- 変更: `tests/features.mjs`。
- 確認: `node tests/features.mjs target/debug/tsuzuri unicode` と `… regex`（Unicode 18.0）が成功し、計算し直しを省いた旨を表示する。
  `$NODE17` でも成功し、計算し直して比べる。参照値の 1 つを手で変えると、`$NODE17` の実行が参照値の不一致で失敗し、`node` の実行はコンパイラの出力との不一致で失敗する
  （手で変えた値は戻す）。

### 手順 5: ドキュメントと最終確認

- `docs/architecture.md:1138` の検証の段落に、参照値のファイルと、Unicode の版を上げる手順（UCD の生成、`--write`、`--write-v8` を同じ版の Node で）を足す。
- T2 の `features.mjs` の該当 suite。`_tsuzuri/` は変えないので `check-docs.mjs` は不要。

## テスト計画

### Rust テスト

- なし。

### E2E

- `node tests/features.mjs <tsuzuri> unicode`・`regex` を Unicode 17.0 と 18.0 の Node で（手順 4）。
- `TSUZURI_TEST_ALLOCATOR=small` と WASM の行も、suite の既定の組み合わせのとおり成功する。

### 既存テストへの影響

- 期待値の数と値は変わらない（参照値は今の計算を書き出したもの）。CI の Node は今と同じく計算し直しまで行う。

### 性能

- `unicode` の suite の準備は、参照値を読むだけになり速くなる（版が一致するときは今と同じ）。

## ドキュメント

- `docs/architecture.md`: 1138 行の検証の段落（参照値のファイルと `--write-v8`）。
- `_tsuzuri/language-reference/`: 変更不要（テストの基盤だけ。`regex.md` の「Unicode 17.0.0」と `strategy.md` の表は変わらない）。
- `_features/README.md`: 状態を更新する。

## 受け入れ条件

- [ ] Homebrew の `node@24`（Unicode 18.0）で `regex`・`unicode` の suite が成功する。
- [ ] Unicode 17.0 の Node で、両 suite が参照値を計算し直して比べ、成功する。
- [ ] `--write`・`--write-v8` が、版の違う Node では何も書かずに失敗する。
- [ ] 参照値のファイルが生成物と分かる見出し（命令・Node の版・Unicode の版・Do not edit）を持つ。
- [ ] `src/runtime/unicode.ll` と `std/` が変わらない。

## 落とし穴

- `expectedCases` の V8 の部分には、UCD の試験ファイルの結果と V8 を照合する `assert`（113 行。`--write` のとき）がある。分けるときに、この照合を
  `--write-v8` の側へ残す。
- 参照値を「コンパイラの出力」から作ってはいけない。`--write-v8` はコンパイラを呼ばない。
- `Intl.Segmenter` の結果は ICU の版でも変わりうる（Unicode の版が同じでも）。版の一致は `process.versions.unicode` で見るが、計算し直しの不一致は
  停止条件として報告する。

## 対象外

- std の Unicode の版を 18.0 に上げること（UCD の生成、`std/` の表、言語リファレンスの記述の更新）。上げるときは、この仕組みで参照値も同じ Node で作り直す。
- 文書の見出しの anchor（`scripts/check-docs.mjs` などの `\p{L}`）の版依存。std の表とは関係しない。

## 決定事項

### D1: V8 の参照値を commit する

- 決定: V8 から作る期待値を、std と同じ Unicode の版の Node で書き出して commit する。
- 理由: UCD のファイルから同じ参照値を作り直す案（カテゴリ・大文字小文字・区切りの実装を JavaScript で書く）は、比べる相手を自前の実装にしてしまい、
  V8 という独立した実装との照合を失う。版の違う Node を拒む案は、Homebrew の Node でテストを動かせなくする。
- 状態: 既定案（実装者はこの案に従う）。

### D2: std の版は `scripts/generate-unicode.mjs` から読む

- 決定: テストが比べる版は、生成スクリプトの `VERSION` を export して読む。
- 理由: 版を 2 か所に書くと、片方だけ上げる誤りが起きる。
- 状態: 既定案。

## 実装と検証（2026-11-06）

- **範囲**: 手順 1〜5 をすべて実装した。コンパイラ・std・`src/runtime/unicode.ll`・fixture は変えていない（`git diff --stat 72f06862 HEAD -- src std tests/fixtures tests/unicode-ucd.mjs` が空）。Unicode 17.0 の Node（以下 `$NODE17`）は nodejs.org の公式の Node v24.22.2（ICU 78.3、`process.versions.unicode` 17.0。CI の `actions/setup-node` の 24 と同じ build）を別のディレクトリに展開して使った。Unicode 18.0 の Node は Homebrew の `node`（v24.22.2、ICU 79.1）。
- **手順 1（ベースライン）**: `main` の最新の `ci.yml` は成功。`cargo build --locked` の dev のバイナリで、`$NODE17 tests/features.mjs target/debug/tsuzuri unicode` は 4,341 件、`… regex` は 2,273 件で成功し、Homebrew の `node` ではチケットの「再現」のとおり `tz_category_chunk(INT64_C(0)) == INT64_C(581508025)`・`tz_property_hash(INT64_C(0)) == INT64_C(825879436)` の host の `assert` で止まった。変更前の `expectedCases` を両方の Node で書き出して比べると、Unicode 17.0 と 18.0 で違うのは `regex` の 24 件（`property_hash` 23、`fold_orbit_hash` 1）と `unicode` の 11 件（`category_chunk` 4、`normalize_chunk` 4、`case_chunk` 2、`segments_packed` 1）だった。
- **手順 2**: `scripts/generate-unicode.mjs` は `export const UNICODE_VERSION = "17.0.0"`（`VERSION` はその別名）を持ち、生成の本体を `generate(args)` に包んで、スクリプトとして実行したときだけ呼ぶ。`node -e 'import("./scripts/generate-unicode.mjs").then(m => console.log(m.UNICODE_VERSION))'` は `17.0.0` を表示し、`fs.readFileSync` を差し替えて数えると、import で読むのはモジュール自身のソースだけだった。`git diff -w` の差分は包む行だけ（生成の分岐は字下げ以外変わらない）。unicode.org から Unicode 17.0.0 の UCD（13 ファイル。SHA-256 は `EXPECTED_SHA256`・`SOURCES` と一致）を `target/scratch/g38/` に取ってきて `node scripts/generate-unicode.mjs <UCD>` を実行し、`git diff --exit-code src/runtime/unicode.ll std/` は差分なし、`--check` も成功、引数なしは従来どおり usage で exit 2。
- **手順 3**: `tests/unicode-cases.mjs` と `tests/regex-cases.mjs` は、ケースの並びを 1 つの関数 `suiteCases(…, v8)` で作り、V8 が計算する期待値だけを `v8(name, args, compute)` で受け取る。`v8Cases` はそれらを計算し（`--write-v8`・`--write` が書き出すもの）、`expectedCases` は commit した参照値を同じ位置に入れる。共通の処理（版の比較、書き出し、計算し直しと比較、並びの照合）は新規の `tests/pinned-unicode.mjs` に置いた。`$NODE17 tests/unicode-cases.mjs --write-v8` は `tests/unicode-v8.mjs`（988 件、94,494 bytes）を、`$NODE17 tests/regex-cases.mjs --write` は `tests/fixtures/regex/Cases.tz`（変わらず）と `tests/regex-v8.mjs`（2,010 件、65,243 bytes）を書いた。1 MiB の上限は書き出す前に検査する。Homebrew の `node` での同じ命令（と `node tests/unicode-cases.mjs --write <UCD>`）は `the Unicode of this Node (18.0) is not std's (17.0); use a Node with Unicode 17.0` で exit 1 になり、何も書かない（`git status` と `cmp` で確かめた）。新しい `expectedCases(…, 参照値)` の結果は、両方の Node で、変更前に `$NODE17` で計算した `expectedCases` と並びと値まで同じ（`regex` 2,273 件、`unicode` 4,341 件。停止条件には当たらない）。`$NODE17 tests/unicode-cases.mjs --write <UCD>` は `tests/unicode-ucd.mjs` を 1 byte も変えずに書き直した。
- **手順 4**: `tests/features.mjs` の suite `regex`・`unicode` は `tests/regex-v8.mjs`・`tests/unicode-v8.mjs` を import して `expectedCases` に渡す。Homebrew の `node` では `tests/unicode-v8.mjs: the V8 expectations are not recomputed, since this Node has Unicode 18.0 and std has 17.0` のように 1 行表示して成功し、`$NODE17` では `tests/unicode-v8.mjs: the 988 V8 expectations equal V8's with Unicode 17.0`（`regex` は 2,010 件）と表示して成功する。参照値を手で 1 つずつ変えると（`category_chunk 0` を 192930686n → 192930687n、`property_hash 0` を 556457773n → 1556457773n）、`$NODE17` は `tests/unicode-v8.mjs differs from V8 at case 0; …`・`tests/regex-v8.mjs differs from V8 at case 1966; …` で、`node` は native の host の `assert`（`tz_category_chunk(INT64_C(0)) == INT64_C(192930687)`・`tz_property_hash(INT64_C(0)) == INT64_C(1556457773)`）で失敗した。値は `$NODE17` で書き直して戻した（書き直したファイルは最初に書いたものと byte まで同じ）。参照値の行を 1 つ消すと、`node` でも `tests/regex-v8.mjs is not for these cases at case 6; run node tests/regex-cases.mjs --write with a Node with Unicode 17.0` で失敗する（scratch の複製で確かめた）。
- **手順 5**: `docs/architecture.md` の D09 の検証の段落に、参照値のファイル、版が同じ Node と違う Node の振る舞い、書き出す命令の拒否、Unicode の版を上げる手順を足した。
- **チケットから外れた判断・細かくした判断**:
  - 共通の処理を `tests/pinned-unicode.mjs`（新規。設計の表にない）にまとめた。版の比較・参照値のファイルの書き方・比較の出力を 2 つの suite で同じにするため。
  - ケースの並びは変えない。V8 の期待値を `suiteCases` の同じ位置で受け取るので、host の C の `assert` の並びも変更前と同じ。さらに、参照値のケースごとの関数名と引数を、ケースの並びの同じ位置のものと照合する。版の違う Node でも、参照値が古い（ケースを足した、乱数の種を変えた）ことを V8 なしで検出するため。
  - V8 のケースに入れたもの: `unicode` は `category_chunk`・`normalize_chunk`・`case_chunk`（種類 0）・無作為な列の `normalize_packed`・`segments_packed`・`case_packed`・`word_run`（`Intl.Segmenter` との照合と `\p{Cased}`・`toUpperCase` による題字化。チケットの 265 行）。UCD の試験から作るケースと `case_chunk`（種類 3）・手書きの題字化は今のまま。`regex` は V8 の照合で決まる match の期待値と `property_hash`・`fold_orbit_hash`。手書きの `captures`、境界でない位置と範囲外の `-1`、`compile_result`・`message_hash`・`escape16`・`linear` は今のまま。手書きの期待値を V8 と照らす `assert`（`V8 and D09 disagree`、`escape` の再照合）は Unicode の表に依存しないので、どの Node でも今のまま実行する。
  - 落とし穴の「113 行の照合を `--write-v8` の側へ残す」: 照合は UCD のファイルを読む `--write <UCD>` に残し、`--write <UCD>` にも版の検査を足した（V8 を使うため）。加えて `v8Cases`（`--write-v8` と計算し直し）が、V8 の正規化を `tests/unicode-ucd.mjs` の `NormalizationTest.txt` の行（commit した抜粋）と照合する。このため `unicode` の `v8Cases` は、設計の `v8Cases()` と違い、commit した UCD のデータを引数に取る（V8 のケース自体はそれに依存しない）。
  - スクリプトとして実行したかの判定は、設計の `import.meta.url === pathToFileURL(process.argv[1]).href` ではなく、`tests/unicode-cases.mjs` が使っていた `process.argv[1] === fileURLToPath(import.meta.url)` にした。`node -e` では `process.argv[1]` が無く、`pathToFileURL(undefined)` が例外になるため。2 つの cases のファイルの書き出しも、スクリプトとして実行したときだけ動くようにした（以前は、import した側の引数に `--write` があれば書いた）。
  - 参照値のファイルの見出しには、命令・Node の版・`process.versions.unicode`・Do not edit に加えて ICU の版を書いた（落とし穴のとおり `Intl.Segmenter` は ICU の版でも変わりうるため）。版の一致は `process.versions.unicode` だけで判定する。
- **実行したコマンドと結果**（手順 4 の後の HEAD で）: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`（警告なし）、`cargo nextest run --locked`（2,934 件すべて成功、4 件 skip。手順 2 の後も同じ）、`cargo build --locked` の dev のバイナリで `node tests/features.mjs target/debug/tsuzuri unicode`・`regex` を Homebrew の `node`（18.0）と `$NODE17`、既定と `TSUZURI_TEST_ALLOCATOR=small` の 8 通り（すべて native と WASM の `-O0`・`-O3`、`unicode` 4,341 件・`regex` 2,273 件で成功。17.0 では計算し直して一致、18.0 では計算し直しを省いた行を表示）、`node tests/unicode-cases.mjs`・`node tests/regex-cases.mjs`（両方の Node で 4,341・2,273 expectations）、`node scripts/check-guide-lite.mjs`（88 件、0 失敗）、`node scripts/check-links.mjs --into docs/language.md`（0 broken）。
- **CI について**: `regex`・`unicode` の suite は、今の CI のどの workflow も実行していない（`targets.yml` が実行するのは `regex_text` だけ）。「既存テストへの影響」の「CI の Node は今と同じく計算し直しまで行う」は、CI がこれらの suite を動かすときに成り立つ。この変更では CI の step を足していない。
- **文書**: `docs/architecture.md`（D09 の検証の段落）。`_tsuzuri/language-reference/`・`docs/language.md`・`README.md` は利用者に見える振る舞いが変わらないので変更なし（`check-docs.mjs` の対象のページはない）。`_features/README.md` の行。
- **変更したファイル**: `scripts/generate-unicode.mjs`、`tests/pinned-unicode.mjs`（新規）、`tests/unicode-cases.mjs`、`tests/regex-cases.mjs`、`tests/unicode-v8.mjs`（新規・生成物）、`tests/regex-v8.mjs`（新規・生成物）、`tests/features.mjs`、`docs/architecture.md`、`_features/README.md`、このチケット（`_features/_completed/` へ移動）。

### レビューの指摘への対応（2026-11-06）

独立したレビューの指摘 2 件（minor 1、nit 1）に対応した。どちらもテストの基盤とスクリプトだけの変更で、コンパイラ・std・`src/runtime/unicode.ll`・参照値のファイル・fixture は変えていない（`git diff --stat fc6cacf5 HEAD -- src std tests/fixtures tests/unicode-ucd.mjs tests/unicode-v8.mjs tests/regex-v8.mjs` が空）。

- **指摘 1（minor）: symlink を含むパスから実行すると、生成スクリプトの `--check` と書き出す命令が何もせず exit 0 になる**
  - 再現: worktree への symlink（`target/scratch/g38-review/link`）を通して、`node link/scripts/generate-unicode.mjs --check <UCD>`・`--check /nonexistent`・引数なしが出力なしで exit 0、`$NODE17 link/tests/unicode-cases.mjs --write-v8`・`$NODE17 link/tests/regex-cases.mjs --write`・`… --write <UCD>` が何も書かずに exit 0、Homebrew の `node`（18.0）でも拒否の行を出さずに exit 0（ファイルの mtime は変わらない）。原因は、`process.argv[1]`（打ったパスを `path.resolve` しただけ）と、Node の loader が symlink を解決して作る `import.meta.url` を比べていたこと。
  - 対応: `scripts/generate-unicode.mjs` に `export function isScript(url)` を置き、`process.argv[1]` があって存在するときに、両方を `realpathSync` した実際のパスで比べる。`tests/pinned-unicode.mjs` がそれを `UNICODE_VERSION` と一緒に再 export し、`tests/unicode-cases.mjs`・`tests/regex-cases.mjs` も使う。
  - 回帰テスト: `tests/regex.rs` の `unicode_scripts_run_through_a_symlinked_directory`（`#[cfg(unix)]`。`node` が無ければ skip）。`target/scratch/unicode-scripts-symlinked/` に `scripts`・`tests` への symlink を作り、それを通して、生成スクリプトの引数なしが usage で exit 2、存在しないディレクトリの `--check` が `ENOENT` で exit 1、`tests/unicode-cases.mjs --write`（ディレクトリなし）が usage で exit 1、`tests/regex-cases.mjs`（命令なし）が `regex cases: ` を表示して成功することを確かめる。Node の Unicode が std の版と違えば、さらに `--write-v8`・`--write <ディレクトリ>`・`regex-cases --write` が拒否の文言で exit 1 になり、参照値のファイル・`Cases.tz`・`tests/unicode-ucd.mjs` が 1 byte も変わらないことを確かめる（同じ版の Node では書き出してしまうので、この部分は 1 行表示して省く）。修正前の HEAD（`fc6cacf5` の 3 つのスクリプト）では最初の assert（`left: Some(0)`、`right: Some(2)`）で失敗し、修正後は成功する。
  - 修正後の証拠（レビューと同じ、worktree 全体への symlink を通して）: Homebrew の `node` と `$NODE17` のどちらでも、引数なしは usage で exit 2、`--check <UCD>` は `src/runtime/unicode.ll matches the generator output`、`--check /nonexistent` は `ENOENT` で exit 1。`node` では `--write-v8`・`regex-cases --write`・`--write <UCD>` が `the Unicode of this Node (18.0) is not std's (17.0); use a Node with Unicode 17.0` で exit 1。`$NODE17` ではそれぞれ `tests/unicode-v8.mjs`（988 件、94,494 bytes）、`Cases.tz` と `tests/regex-v8.mjs`（2,010 件、65,243 bytes）、`tests/unicode-ucd.mjs`（53,351 bytes）を書き、symlink を通した生成スクリプトの書き出しも `src/runtime/unicode.ll` を書いた。書いた 5 つのファイルは前後の `shasum` が同じ。`node --preserve-symlinks-main link/…` でも usage と `regex cases: 2273 expectations`。`node -e 'import("./scripts/generate-unicode.mjs")…' -- --check` と stdin（`node --input-type=module - --write`）からの import は何も生成・書き出ししない。
- **指摘 2（nit）: Unicode の版を上げる手順に、`--write <UCD>` と `tests/regex.rs` が使う `17.0.0` の直書きが残る**
  - 再現: `scripts/`・`tests/` の複製で `UNICODE_VERSION` を `18.0.0` にすると（`target/scratch/g38-review/sim/`。Homebrew の `node` が std の版になる）、`tests/unicode-cases.mjs --write` の usage、SHA-256 の不一致の文言、書き直した `tests/unicode-ucd.mjs` の見出しが `17.0.0` のままだった。`tests/regex.rs` は見出し `from Unicode 17.0.0 UCD` を直書きしていた。
  - 対応: `tests/unicode-cases.mjs` は見出し（`ucdHeader()`）・usage・SHA-256 の文言を `UNICODE_VERSION` から作る（生成スクリプトの usage も `VERSION` から作る。指摘 1 の commit に入れた）。`tests/regex.rs` は `scripts/generate-unicode.mjs` の `export const UNICODE_VERSION = "…"` を `include_str!` で読む `unicode_version()` から期待する見出しを作る。加えて、`tests/unicode-cases.mjs` の `checkUcdData()` が、commit した `tests/unicode-ucd.mjs` の見出しが今の `UNICODE_VERSION` と `SOURCES` から作るものと同じことを確かめ、suite `unicode`（`tests/features.mjs`）と `node tests/unicode-cases.mjs` が呼ぶ（suite `regex` が `Cases.tz` を照合するのと同じ形）。`docs/architecture.md` の手順に、版から作る文言と、作り直し忘れを検出する検査と、生成スクリプトの先頭のコメントの取得の命令の版（手で変える）を書いた。
  - 回帰テスト: 版を上げたときにだけ違いが出る指摘なので、今の版のままで修正前に失敗するテストは作れない。代わりに次で確かめた。`UNICODE_VERSION` と `src/runtime/unicode.ll` の 1 行目を一時的に `18.0.0` にすると（作り直した表に当たる）、新しい `generated_unicode_tables_are_private_and_linked_on_use` は成功し、`main` の同じテストは失敗した。`UNICODE_VERSION` だけを変えると（表の作り直し忘れ）、新しいテストは失敗した（どちらも戻した）。複製で `UNICODE_VERSION` を `18.0.0` にすると、`checkUcdData` が `tests/unicode-ucd.mjs is not from std's Unicode 18.0.0 files (SOURCES); run node tests/unicode-cases.mjs --write <UCD 18.0.0 directory>` で失敗し、usage・SHA-256 の文言・書き直した見出しが `18.0.0` になった。
  - 修正後の証拠: Unicode 17.0.0 の UCD の試験ファイル（`SOURCES` の SHA-256 と一致）で `$NODE17 tests/unicode-cases.mjs --write <UCD>` を実行すると、`tests/unicode-ucd.mjs` は 1 byte も変わらない（見出しの文字列は以前と同じ）。`grep -n "17\.0" tests/unicode-cases.mjs tests/regex.rs` は 0 件。
- **判断**:
  - スクリプトかどうかの判定は、指示の `realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)` ではなく、両辺を `realpathSync` する形にした。`--preserve-symlinks-main` では `import.meta.url` が symlink のパスのままなので（probe で確かめた）、片側だけの解決ではその場合に何もしなくなる。`process.argv[1]` が無い（`node -e`）か存在しない（stdin の `-`）ときは false にして、`realpathSync` が例外を出さないようにした。README の Node 20 で使えない `import.meta.main` は使わない。
  - 判定は 1 か所（生成スクリプト）に置き、tests の 2 つのモジュールは `tests/pinned-unicode.mjs` 経由で使う。版の文字列を 1 か所にした D2 と同じ理由。
  - 回帰テストは Rust のテスト（`cargo nextest` で CI の `ci.yml` が実行する。CI の Rust の job は `actions/setup-node` の Node 24 を持つ）に置いた。`regex`・`unicode` の suite はどの CI の workflow も実行しないため。symlink を作るので Unix だけで実行する。repository の root への symlink は木の走査で輪になるので、`scripts`・`tests` だけに張り、`Drop` で消す。
  - `std/Unicode.tz` の `Unicode.version` の直書きは、すでに `docs/architecture.md` の手順に書いてあり（docs/language.md の Unicode の節を参照）、指摘の範囲外なので変えていない。
- **実行したコマンドと結果**（`0cdba542` で）: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`（警告なし）、`cargo nextest run --locked`（2,935 件すべて成功、4 件 skip。新しい 1 件を含む）、`cargo nextest run --locked --test regex`（7 件。Homebrew の `node` では拒否まで確かめ、`$NODE17` を PATH の先頭にすると `the refusal is not checked: this Node has std's Unicode 17.0` を表示して成功）、`cargo build --locked` の dev のバイナリで `node tests/features.mjs target/debug/tsuzuri unicode`・`regex` を Homebrew の `node`（18.0）と `$NODE17` で 4 通り（すべて native と WASM の `-O0`・`-O3`、`unicode` 4,341 件・`regex` 2,273 件で成功。17.0 では `the 988 V8 expectations equal V8's with Unicode 17.0`・`the 2010 …`、18.0 では計算し直しを省いた 1 行）、`node tests/unicode-cases.mjs`・`node tests/regex-cases.mjs`（両方の Node で 4,341・2,273 expectations）、`node scripts/check-links.mjs --into docs/language.md`（0 broken）、`node scripts/check-guide-lite.mjs`（88 件、0 失敗）。`_tsuzuri/`・診断・生成された文書の block は変えていないので、`check-docs.mjs`・`gen-docs.mjs --check`・`check-cli-docs.mjs` は対象がない。
- **変更したファイル**: `scripts/generate-unicode.mjs`、`tests/pinned-unicode.mjs`、`tests/unicode-cases.mjs`、`tests/regex-cases.mjs`、`tests/features.mjs`、`tests/regex.rs`、`docs/architecture.md`、このチケット。`_features/README.md` の行の要約は、出荷するものが変わらないので変えていない。
