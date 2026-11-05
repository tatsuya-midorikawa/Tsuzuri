# A29: 印を持つ dyn 型（`dyn (C, Send)` など）を型引数にした組み込み関数の LLVM シンボルが不正になる（`Channel<dyn (C, Send)>` がビルドできない）

| 項目 | 内容 |
| --- | --- |
| ID | A29 |
| 優先度 | P1 |
| 規模 | S |
| 依存 | A14, TZ03 |
| 後続 | – |
| 状態 | done |
| 起票 | 2026-11-05（TZ10 の作業中に発見。HEAD `5ee6b0f2` で確認） |
| 承認 | 不要（言語の意味は変えない。生成 IR のシンボル名を正しくする修正） |
| 改善する劣位 | 不具合: `Channel<dyn (C, Send)>` のように、`+copy`・`+send`・`+ref` の印を持つ dyn 型を型引数にした組み込み関数を使うと、clang が `E2002` で失敗する。チャンネルは `Send` の値しか運べず、印の無い `dyn C` は `E1013` なので、dyn 値を運ぶチャンネルはどの書き方でも作れない |
| 手本にする既存実装 | `src/llvm.rs` の `mangled_type`（`->`・`[`・`]`・`,` を `$A`・`$L`・`$R`・`$C` に置き換える）。テスト: `tests/dyn_dispatch.rs` の `accepts`、`tests/features.mjs` の suite `dyn_dispatch` の Phase 2 の `send_area` |
| 主な影響ファイル | `src/llvm.rs`, `tests/dyn_dispatch.rs`, `tests/fixtures/dyn_dispatch/Main.tz`, `tests/features.mjs`, `tests/fixtures/preemption/Main.tz`, `docs/architecture.md`, `_features/README.md` |

## 目的

`Channel<dyn (Shape, Send)>` のような、印を持つ dyn 型を型引数にした組み込み関数を、ほかの型と同じようにビルドできるようにする。
dyn 値を tzroutine の間で受け渡す手段は、今は dyn 値をレコードに包む回避策（TZ10 の `tests/fixtures/preemption/Main.tz` の `Boxed`）しかない。

同時に、組み込み関数のシンボルが LLVM の引用符なしの識別子だけでできていることを、テストで確かめられるようにする。

## 着手条件と停止条件

### 着手条件

- A14 と TZ03 が `_features/README.md` で done であること（HEAD で done）。確認: `grep -nE "^\| (A14|TZ03) " _features/README.md`。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 の再現（失敗すること）を確かめていること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- `+` のほかにも、`canonical_type` の綴りに LLVM の識別子に使えない文字（`[-a-zA-Z$._0-9]` の外）が見つかった。
- 修正の前後で、印を持つ dyn 型を使わないプログラムの IR が byte 単位で変わる（手順 4）。
- レコード型の名前（`%"tz.record.…"`）など、すでに引用符で囲んでいる名前の綴りまで変えたくなった。このチケットは引用符なしのシンボルだけを直す。

## 現状（HEAD `5ee6b0f2` で確認）

- `src/llvm.rs:3491` の `dyn_name` は、dyn 型を `dyn[Shapes.Shape]` と綴り、印を `+copy`・`+send`・`+ref` として `]` の前に足す
  （doc comment: 「`+copy`, `+send`, and `+ref` for its markers and region, which no class name contains」）。
- `src/llvm.rs:3726` の `canonical_type` は、`Type::Dyn` を `dyn_name` で綴る。型の綴りは、引用符で囲むレコード型の名前（`%"tz.record.Channel.Channel[dyn[Spin.Spin+send]]"`）にも、
  引用符なしのシンボルにも使われる。
- `src/llvm.rs:7874` の `builtin_symbol` は、多相の組み込み関数のシンボルを `@tz.builtin.<名前>.<型引数>` と作り、型引数を `mangled_type` で綴る。
  doc comment は「its type arguments as unquoted, injective names」と書く。
- `src/llvm.rs:7896` の `mangled_type` は、`canonical_type` の `->`・`[`・`]`・`,` だけを `$A`・`$L`・`$R`・`$C` に置き換え、doc comment は
  「`canonical_type` spelled with LLVM identifier characters only」と書く。`+` は置き換えないので、印を持つ dyn 型でこの約束が破れる。
- `mangled_type` を使うのは `builtin_symbol` と、`src/llvm_domain.rs:34` の `@tz.domain.thunk.<型>` である。`Domain.run` の閉包は純粋で
  dyn の dispatch を使えない（`E1038`）ので、後者で印を持つ dyn 型が現れる経路は見つかっていない。
- 名前空間の型は `Acme.Shop.Main.Point` のように `.` で綴られる（`@tz.builtin.Channel.make.Acme.Shop.Main.Point` を確かめた）ので、問題は `+` だけである。
- 回避策: `tests/fixtures/preemption/Main.tz:133` の `record Boxed { value: dyn (Spin.Spin, Send) }` と `Channel<Boxed>`。

### 再現（検証済み。HEAD `5ee6b0f2`、macOS arm64、Homebrew clang 21・23）

`tests/fixtures/preemption/Spin.tt`（`class Spin<'a> { def spin :: ref 'a -> i64 }`）を写したディレクトリに、次の `Main.tz` を置く。

```text
record Flag { value: i64 }

instance Spin.Spin<Flag> {
    fn spin flag = flag.value
}

def run :: unit -> i64
fn run _ =
    let spinners: Channel<dyn (Spin.Spin, Send)> = Channel.make 1
    let received = Channel.share spinners
    let out: Channel<i64> = Channel.make 1
    let reply = Channel.share out
    tzroutine {
        match Channel.recv received with
        | Maybe.Some spinner -> Channel.send reply (Spin.Spin.spin (ref spinner))
        | Maybe.None -> ()
    }
    Channel.send spinners (Dyn.of (Flag { value: 7 }))
    match Channel.recv out with
    | Maybe.Some v -> v
    | Maybe.None -> 0

run ()
```

`tsuzuri run <dir>` の結果:

```text
error[E2002]: 'clang' failed (exit status: 1):
.../module.ll:305:103: error: bad payload format for nan literal
  305 |   %v2 = call %"tz.record.Channel.Channel[dyn[Spin.Spin+send]]" @tz.builtin.Channel.make.dyn$LSpin.Spin+send$R(i64 %v1)
```

印の無い `Channel<dyn Spin.Spin>` は `E1013`（`values that move to another tzroutine or thread must be owned; dyn Spin contains a reference`）で、
これは仕様どおりである（[型の制約](../../_tsuzuri/language-reference/types-and-type-inference/constraints.md) の `Send`）。

## 仕様

言語の意味・構文・診断は変えない。生成するモジュールについて次を保証する。

- 組み込み関数のシンボル（`@tz.builtin.…`）と `@tz.domain.thunk.…` は、LLVM の引用符なしの識別子 `[-a-zA-Z$._][-a-zA-Z$._0-9]*` だけでできている。
- 綴りは型に対して単射のままである（違う型引数の 2 つの instance が同じシンボルにならない）。
- 印を持つ dyn 型を使わないプログラムの IR は、修正の前後で byte 単位で同じである。
- `Channel<dyn (C, Send)>` の `make`・`send`・`recv`・`share`・`close`、`SendOnly`・`ReceiveOnly`、`for x in ch`、`select` の腕が、native と WebAssembly
  （既定の 1 本の実行と `--wasm-feature jspi`）の `-O0`・`-O3` でビルドでき、値を運べる。

## 設計

### 段ごとの変更

| ファイル | 関数・場所 | 変更 |
| --- | --- | --- |
| `src/llvm.rs` | `mangled_type` | `+` を `$P` に置き換える（D1）。doc comment に `$P` を足す |
| `src/llvm.rs` | `dyn_name` の doc comment | 印の綴りが `mangled_type` で `$P` になることを書く |
| `tests/dyn_dispatch.rs` | 新しいテストと helper | IR の引用符なしのシンボルを識別子の文字で検査する helper `assert_identifier_symbols` と、印ごとのテスト |
| `tests/fixtures/dyn_dispatch/Main.tz`・`tests/features.mjs` | suite `dyn_dispatch` | `Channel<dyn (Shapes.Shape, Send)>` で値を運ぶ export `channel_area` を足す |
| `tests/fixtures/preemption/Main.tz` | `Boxed` | 回避策を消し、`Channel<dyn (Spin.Spin, Send)>` に戻す（D3） |
| `docs/architecture.md` | 組み込み関数のシンボルの節 | `$P` を足す |

### アルゴリズム

`mangled_type` の置換に `.replace('+', "$P")` を足すだけである。`$` が `canonical_type` の綴りに現れるのは active pattern の暗黙の union の名前の中だけで
（既存の doc comment の「`$` は現れない」は誤りだった。レビューで訂正。D1）、`$P` は既存の `$A`・`$L`・`$R`・`$C` と衝突せず、単射は保たれる。`+` を含む綴りは `dyn_name` の印だけなので、印を持つ dyn 型の無いプログラムの
シンボルは変わらない。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N を必ず見る（GUIDE §3.1）。

### 手順 1: ベースラインと再現

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、「再現」の失敗を確かめる。比較用に dyn の fixture の IR を保存する。
- 確認:

```sh
cargo build --locked
mkdir -p target/scratch/a29/base
for fixture in tests/fixtures/dyn_dispatch tests/fixtures/channel tests/fixtures/preemption; do
  target/debug/tsuzuri build "$fixture" --emit llvm -o "target/scratch/a29/base/$(basename "$fixture").ll"
done
```

`tests/fixtures/channel` が無ければ `ls tests/fixtures | grep -i channel` で Channel の fixture を選ぶ。再現が `E2002` にならなければ停止する。

### 手順 2: 失敗するテストを先に足す

- 変更: `tests/dyn_dispatch.rs`。
- 内容: `assert_identifier_symbols(ir: &str)` を足す。IR の `@` に続く引用符なしの名前（`@` の次が `"` でないもの）を取り出し、
  先頭が `[-a-zA-Z$._]`、残りが `[-a-zA-Z$._0-9]` であることを確かめる。テスト `marked_dyn_type_arguments_spell_identifier_symbols`
  を足し、`with_shape` の `Shape` で `Channel<dyn (Shape, Send)>`、`Channel<dyn (Shape, Copy, Send)>`（印の組み合わせが受理されるなら）を `make`・`send`・`recv` する
  library を `accepts` してから検査する。
- 確認: `cargo test --locked --test dyn_dispatch marked_dyn_type_arguments` が `1 failed`（`+send` を報告）。

### 手順 3: `mangled_type` を直す

- 変更: `src/llvm.rs`。
- 内容: 「アルゴリズム」のとおり。
- 確認: 手順 2 のテストが成功する。`cargo test --locked --test dyn_dispatch` が全件成功する。

### 手順 4: 印の無いプログラムの IR が変わらないこと

- 確認: 手順 1 と同じコマンドで `target/scratch/a29/after` に書き、`tests/fixtures/preemption` を除いて `cmp` が一致する
  （preemption は手順 6 で fixture を変えるまでは一致する）。違えば停止する。

### 手順 5: E2E

- 変更: `tests/fixtures/dyn_dispatch/Main.tz`、`tests/features.mjs`。
- 内容: export `channel_area :: i64 -> i64` を足す。`Channel<dyn (Shapes.Shape, Send)>` を作り、tzroutine が `Square { side: x }` を送り、
  呼び出し側が受け取って `Shapes.Shape.area` を返す。suite `dyn_dispatch` に `...[3n, -4n].map((x) => ["channel_area", [x], x * x])` を足す。
  suite が tzroutine の runtime を link するか（`features.mjs` の `/@tsuzuri_(tz|channel)_/` の判定）を確かめる。
- 確認: `node tests/features.mjs target/debug/tsuzuri dyn_dispatch` が native と WASM の `-O0`・`-O3` で成功する。

### 手順 6: 回避策を消す

- 変更: `tests/fixtures/preemption/Main.tz`。
- 内容: `record Boxed` を消し、`Channel<Boxed>` を `Channel<dyn (Spin.Spin, Send)>`、`Boxed { value: Dyn.of (...) }` を `Dyn.of (...)`、受け取り側の
  `boxed.value` を値そのものにする（D3）。
- 確認: `cargo test --locked --test preemption`、`node tests/tzroutine.mjs target/debug/tsuzuri`（`dyn_spin`・`drop_spin` の行）が成功する。

### 手順 7: ドキュメントと最終確認

- `docs/architecture.md` の組み込み関数のシンボルの記述（`grep -n 'tz.builtin' docs/architecture.md`）に `$P` を足す。
- T2（AGENTS.md）を実行する。

## テスト計画

### Rust テスト

- `tests/dyn_dispatch.rs`: `marked_dyn_type_arguments_spell_identifier_symbols`（新規。`+copy`・`+send`・`+ref` のうち、組み込み関数の型引数に置ける印の組み合わせごと）。
  `+ref`（`dyn C {r}`）はチャンネルへ置けない（`Send` ではない）ので、`Vec` や `Maybe` など借用を置ける組み込み関数の instance で確かめる。置ける組み込み関数が
  無ければ、そのことをテストのコメントに書く。
- 既存の `accepts` を使うテストすべてに `assert_identifier_symbols` をかけてもよい（失敗したら停止条件の 1 つ目）。

### E2E

- `tests/features.mjs` の suite `dyn_dispatch`: `channel_area`（native と WASM、`-O0`・`-O3`）。
- `tests/tzroutine.mjs`: 手順 6 の fixture で `dyn_spin`・`drop_spin`。

### 既存テストへの影響

- `tests/fixtures/preemption/Main.tz` の `Boxed` を消す（手順 6）。ほかの期待値は変わらない。

### 性能

- 影響なし。シンボル名の綴りだけが変わる。

## ドキュメント

- `_tsuzuri/language-reference/`: 変更不要。`dyn (C, Send)` の値がチャンネルで運べないという既知の不具合の記述は無く
  （`grep -rn "Channel<dyn" _tsuzuri/language-reference` が 0 件）、[型の制約](../../_tsuzuri/language-reference/types-and-type-inference/constraints.md) の `Send` の説明は直した後の振る舞いと一致する。
  チャンネルのページに dyn 値の例を足すなら、`check-docs.mjs` を通す。
- `docs/architecture.md`: 組み込み関数のシンボルの綴りに `$P` を足す。
- `_features/README.md`: 状態を更新する。

## 受け入れ条件

- [ ] 「再現」のプログラムが native の `-O0`・`-O3` で `7` を返す。
- [ ] `marked_dyn_type_arguments_spell_identifier_symbols` が成功し、`assert_identifier_symbols` が `+` を含むシンボルを報告できる（修正前に失敗した）。
- [ ] 印を持つ dyn 型を使わない fixture の IR が修正の前後で byte 単位で同じ。
- [ ] suite `dyn_dispatch` の `channel_area` が native と WASM の `-O0`・`-O3` で成功する。
- [ ] `tests/fixtures/preemption/Main.tz` から `Boxed` が消え、`tests/tzroutine.mjs` が成功する。
- [ ] T2 が成功する。

## 落とし穴

- `canonical_type` そのものを変えると、引用符で囲むレコード型の名前（`%"tz.record.…[dyn[…+send]]"`）や、`dyn_name` を使う vtable の名前まで変わり、
  `tests/` の IR の期待値が広く壊れる。変えるのは `mangled_type` だけにする。
- `+` を `$P` 以外の綴り（たとえば `_`）にすると、`Spin.Spin_send` のようなクラス名と衝突しうる。`$` で始まる escape にする。
- WebAssembly のシンボルは wasm-ld の名前にもなる。`$` は既存の escape でも使っているので問題ないが、手順 5 で WASM の実行まで確かめる。

## 対象外

- 引用符で囲む名前（レコード型・vtable・文字列定数）の綴りの変更。
- `Channel<dyn C>`（印なし）の受理。`Send` の規則は変えない。

## 決定事項

### D1: `+` は `$P` に置き換える

- 決定: `mangled_type` で `+` を `$P` に置き換える。
- 理由: 既存の `$A`・`$L`・`$R`・`$C` と同じ規則で読め、単射が保たれる。`$` が型の綴りに現れるのは、複数の case を持つ active pattern の暗黙の union の名前
  （`Main.Active$active$C$P$total$result`、`src/parse_control.rs`）の中だけである。この名前はモジュールの後の `.Active$active$` で始まり `$total$result` で終わり、
  間には大文字で始まる識別子の case 名と区切りの `$` しか無く、置換する記号（`->`・`[`・`]`・`,`・`+`）を含まないので、case 名が `C` や `P` でも綴りの読み方は 1 通りに決まる
  （起票時の「`$` は型の綴りに現れない」は誤りで、レビューで訂正した）。シンボルを引用符で囲む案は、
  `builtin_symbol` の呼び出し元（`src/llvm.rs` の 4 か所、`src/llvm_domain.rs`）と、シンボルを文字列で探すテスト（`grep -rn "tz.builtin\." tests`）を広く変える。
- 状態: 既定案（実装者はこの案に従う）。

### D2: 新しい診断は作らない

- 決定: 綴りの検査はテストの helper だけで行い、`E2002` の手前で止める新しい診断や `debug_assert!` は足さない。
- 理由: 原因は内部の綴りで、利用者が直せるものではない。修正後は起きない。
- 状態: 既定案。

### D3: TZ10 の回避策を消す

- 決定: `tests/fixtures/preemption/Main.tz` の `Boxed` を消し、dyn 値を直接チャンネルで運ぶ形に戻す。
- 理由: 回避策は不具合を隠す。直した形を、TZ10 の横取りのテストが日常的に通る経路にする。
- 状態: 既定案。

## 実装と検証（2026-11-06）

手順 1〜7 をすべて実施しました。`src/` の変更は `mangled_type` の 1 行（と doc comment）だけです。

### 手順と結果

- 手順 1: `cargo build --locked` の後、「再現」のプログラムが `E2002`（`@tz.builtin.Channel.make.dyn$LSpin.Spin+send$R`）になることを確かめました。比較用の IR は、チケットの 3 つに限らず、`Main.tz` を持つ `tests/fixtures` の 139 個すべてを native と `--target wasm32` で `--emit llvm` しました（278 ファイル。`domain`・`domain_host` の native は `--trap-mode return`。wasm32 が `E2000` になる 10 個（OS の API・`net`・`Domain.run`）は両方の版で同じく除外）。
- 停止条件 1 の確認: 修正前の 278 ファイルの、引用符なしの `@` の名前を `[-a-zA-Z$._][-a-zA-Z$._0-9]*`（と無名の `@N`）で調べ、0 件でした。`+` のほかに識別子でない文字はありません。`Domain.run` の閉包は dyn 値を作れない（`Dyn.of` も `E1038` の `uses dyn dispatch`）ので、`@tz.domain.thunk.` に印を持つ dyn 型は現れません。
- 手順 2: `tests/dyn_dispatch.rs` に `assert_identifier_symbols`（と、最初の違反を返す `non_identifier_symbol`）を足し、`accepts` を通るすべてのプログラムと `runtime_fixture_lowers_for_both_targets` の native・WASM の IR にかけました。`marked_dyn_type_arguments_spell_identifier_symbols` は修正前に `1 failed`（`'@tz.builtin.Channel.make.dyn$LMain.Shape+send$R' is not an LLVM identifier`）、ほかの 23 件は成功でした。
- 手順 3: `mangled_type` に `.replace('+', "$P")` を足しました。`cargo test --locked --test dyn_dispatch`: 24 passed（23 + 新規 1）。
- 手順 4: 修正後の 278 ファイルを同じコマンドで書き直し、`cmp` で 278 件すべて一致しました（`preemption`・`dyn_dispatch` を含む。どれも印を持つ dyn 型を組み込み関数の型引数にしていません）。修正後の 278 ファイルの違反も 0 件です。
- 「再現」のプログラム: native の `-O0`・`-O3` で `7`。
- チャンネル以外の確認: `Channel<dyn (C, Send)>`・`Channel<dyn (C, Copy, Send)>`・`Vec<dyn (C, Copy)>`・`dyn C {r}` の `Vec` を使う一時のプロジェクト（`target/scratch/a29/`。作業後に削除）が、native の `tsuzuri run` と wasm32 の export（import なし）の `-O0`・`-O3` で、手で数えた `9 + 16 + 1 + 1 = 27` を返しました。
- 手順 5: `tests/fixtures/dyn_dispatch/Main.tz` に `channel_ops` と `channel_area` を足し、`node tests/features.mjs target/debug/tsuzuri dyn_dispatch` が成功しました（`dyn_dispatch: native/WASM at O0/O3 passed`、58 cases）。`node tests/tzroutine_wasm.mjs target/debug/tsuzuri` の `dyn_dispatch -O0`・`-O3`（JSPI）も成功しました。
- 手順 6: `tests/fixtures/preemption/Main.tz` の `record Boxed` を消し、`Channel<dyn (Spin.Spin, Send)>` で直接運ぶ形にしました。`cargo test --locked --test preemption`: 6 passed。`node tests/tzroutine.mjs target/debug/tsuzuri`: 151 runs passed（macOS arm64 なので `dyn_spin`・`drop_spin` を P1・P4、`-O0`・`-O3` で実行）。
- 手順 7（T2）: 下記。

### 実行したコマンド（T0〜T2）

- `cargo fmt --all -- --check`: 成功。
- `cargo clippy --all-targets -- -D warnings`: 成功。
- `cargo nextest run --locked`: 2935 tests run: 2935 passed, 4 skipped。
- `node tests/features.mjs target/debug/tsuzuri dyn_dispatch`: 成功（58 cases、native と既定の WASM の `-O0`・`-O3`。native は `channel_area` も）。
- `node tests/tzroutine_wasm.mjs target/debug/tsuzuri`: 成功（`tzroutines through JSPI passed at -O0 and -O3`。`dyn_dispatch` を含む）。
- `node tests/tzroutine.mjs target/debug/tsuzuri`: 151 runs passed。
- `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs _tsuzuri/language-reference/built-in-types-and-modules/channel.md _tsuzuri/language-reference/types-and-type-inference/type-classes.md`: 成功（2 pages, 41 links, 14 checked examples, 28 native runs）。
- `node scripts/check-guide-lite.mjs`: 成功。

### チケットから外れた判断・詳細化

- 不具合の範囲はチャンネルに限りませんでした。`Vec<dyn (C, Copy)>`（`@tz.builtin.Vec.empty.dyn$L…+copy$R`）や、region 付きの `dyn C {r}` を要素にした `Vec`（`+ref`）も、修正前は同じ `E2002` でした。修正はどれにも効き、`marked_dyn_type_arguments_spell_identifier_symbols` は `+send`（`Channel`）・`+copy+send`（`Channel`）・`+copy`（`Vec`）・`+ref`（`Vec`）の 4 つの印の組み合わせを、`$P` で綴った期待のシンボルで確かめます（期待値は D1 の規則から手で書いたもの）。`+copy+ref` は region 付きの dyn に `Copy` 印を付ける形で、組み込み関数の型引数に置く経路を作る必要がないので足していません。
- 手順 5 の `channel_area` は tzroutine を起動するので、既定の WebAssembly ではビルドできません（`E2000`、`tzroutine on WebAssembly needs --wasm-feature jspi`）。チケットの「native と WASM の `-O0`・`-O3`」を次のように満たしました。
  - suite `dyn_dispatch` に `nativeFrom: "// native only:"` を足し、`channel_area` をその行の下に置いて `nativeCases` にしました（`concurrency` と同じ仕組み）。
  - 1 本の実行で済む `channel_ops` を行の上に足し、`cases` にしました。`make`・`share`・`send_only`・`receive_only`・`SendOnly.send`・`select`（受け取る腕と、選ばれず drop される送る腕）・`SendOnly.close`・`for x in ch` を `dyn (Shapes.Shape, Send)` で通し、native と既定の WASM の `-O0`・`-O3` で走ります（確保の追跡 `live == 0` も）。期待値は `x² · 10⁶ + 2x · 10³ + (x + 1)²`（JavaScript の BigInt の式）。
  - 仕様の「`--wasm-feature jspi`」は `tests/tzroutine_wasm.mjs` に suite `dyn_dispatch`（`channel_area` と `channel_ops`）を足して確かめました。このため、その `fixture()` が `Main.tz` だけでなく fixture の `.tz`・`.tt`・`.tc` をすべて写すようにしました（ほかの fixture は `Main.tz` だけなので結果は変わりません）。
- `assert_identifier_symbols` は、引用符の中（文字列定数・引用符付きの名前）と `;` の注釈を飛ばし、名前を空白か `(),[]{}<>"` までとします。ヘルパー自身の確認（`+` を見つけること、引用符の中の `@` と `+` を数えないこと）を新しいテストに入れました。
- 文書: チケットは参照の変更を不要としていましたが、チャンネルで dyn 値を運べることが実際に動くようになったので、[Channel](../../_tsuzuri/language-reference/built-in-types-and-modules/channel.md#要素と借用) に `Channel<dyn (Display, Send)>` の `run=` の例と、印の無い `dyn C`・`dyn C {r}` が `E1013` であること（実行して確認）を足し、[型クラス](../../_tsuzuri/language-reference/types-and-type-inference/type-classes.md#動的ディスパッチ) と `docs/language.md` の dyn の節に `Send` の dyn 値の移し先を 1 文足しました。

### 更新した文書

- `docs/architecture.md`（組み込み関数のシンボル: `builtin_symbol` と `mangled_type` の綴り、`$P`、引用符付きの名前は `+` のまま）
- `docs/language.md`（dyn 値の節）
- `_tsuzuri/language-reference/built-in-types-and-modules/channel.md`（要素と借用）
- `_tsuzuri/language-reference/types-and-type-inference/type-classes.md`（動的ディスパッチ）
- `_features/README.md`（状態）

### 変更したファイル

- `src/llvm.rs`（`mangled_type`、`dyn_name` の doc comment）
- `tests/dyn_dispatch.rs`
- `tests/fixtures/dyn_dispatch/Main.tz`、`tests/features.mjs`
- `tests/tzroutine_wasm.mjs`
- `tests/fixtures/preemption/Main.tz`
- 上の文書
