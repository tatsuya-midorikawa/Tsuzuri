# G38: `features.mjs` の `regex`・`unicode` の期待値が、テストを動かす Node の Unicode の版で変わる（Unicode 18 の Node で失敗する）

| 項目 | 内容 |
| --- | --- |
| ID | G38 |
| 優先度 | P2 |
| 規模 | S |
| 依存 | D09 |
| 後続 | – |
| 状態 | todo |
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
