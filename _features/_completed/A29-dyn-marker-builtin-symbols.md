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

- `src/llvm.rs`（`mangled_type`、`dyn_name` の doc comment。レビュー後に `canonical_type`・`codegen_identity` と型で数える集合）、`src/llvm_abi.rs`・`src/llvm_debug.rs`・`src/llvm_recursive.rs`・`src/llvm_shared.rs`（2 回目）
- `tests/fixtures/generic_records/Main.tz`（レビュー後）
- `tests/dyn_dispatch.rs`
- `tests/fixtures/dyn_dispatch/Main.tz`、`tests/features.mjs`
- `tests/tzroutine_wasm.mjs`
- `tests/fixtures/preemption/Main.tz`
- 上の文書

### レビューの指摘への対応（2026-11-06）

独立したレビューの 4 件の指摘に対応しました。比較の基準は `main` のソースで作った base の binary（`target/debug/tsuzuri`、作り直していない）です。

#### 1（major）: `canonical_type` が関数型の純粋性と引数規約を綴らず、単射でなかった

- 再現: レビューの 2 つのプログラム（`Vec<i64 -> i64>` と `Vec<pure (i64 -> i64)>` の `Vec.empty`、`Box<i64 -> i64>` と `Box<pure (i64 -> i64)>`）は、修正前の binary と base の binary の両方で
  `E2002`（`invalid redefinition of function 'tz.builtin.Vec.empty.fn$Li64$Ai64$R'`、`redefinition of type %"tz.record.Main.Box[fn[i64->i64]]"`）でした。
  引数規約でも同じく衝突することを確かめました: edition 2027 の `def text_length :: string -> i64`（規約 `[Read]`）と `\(text: ref string) -> …` の `Box`（`redefinition of type`）、
  `def bump :: i64 -> mut Counter -> unit`（規約 `[Own, Mut]`）と `\(step: i64) (counter: ref mut Counter) -> …` の `Vec`（`invalid redefinition of function`）。
- `Type` の監査: `Type` の `Eq` が比べるフィールドのうち、`canonical_type` が落としていたのは `FunctionParameters::pure` と `::conventions` だけでした。
  参照の可変性（`ref`／`refmut`）、`SharedKind`（`rc`・`rc.weak`・`arc`・`arc.weak`）、dyn の印と region（`+copy`・`+send`・`+ref`。region の名前は `Type` に無い）、固定長配列の長さ（`fixed[N,T]`）、
  整数の符号と幅（`i64`／`i64u`）、SIMD（`ty.name()`）はすでに綴られています。レコード・共用体の region は `Type::Record` の引数に現れません。
- 修正（`src/llvm.rs` の `canonical_type`）: 関数型を、純粋なら `pure.fn[…]`、規約を持つなら `fn.<規約>[…]`（保持している規約の並びのとおり 1 つごとに `o`・`r`・`m`）と綴ります。既定（純粋でなく、規約が空）は
  `fn[…]` のままです。`pure.` と `fn.` で始まる綴りはほかに無く（レコード・共用体はモジュール名の大文字で始まり、組み込みの型の綴りは `fn` の後に `[` が続くか `f` の後に数字が続く）、
  使う文字は `.` と英小文字だけなので `mangled_type` の置換も変えていません。規約は `convention(i)` でなく保持している並びのまま綴るので、`Eq` が区別する `[Read]` と `[Read, Own]` も別の綴りになります
  （`function_with_conventions` が規約をすべて `Own` のとき空にするので、解決後の型で空でない並びは既定でない規約を含みます）。
- 綴りが変わる名前: 純粋な関数型か規約を持つ関数型を（入れ子も含めて）含む型の綴りだけです。引用符で囲む `%"tz.record.…"`・`%"tz.union.…"`・`%"tz.rec.node.…"`・`@"tz.<op>.rec.…"`・
  `@"tz.shared.drop.…"`・`@"tz.vtable.…[格納型]"`、引用符なしの `@tz.builtin.…` と `@tz.domain.thunk.…` です。ホストの ABI のレコード名（`llvm_abi.rs` の `record_name`。C ヘッダーと bindings）も
  `canonical_type` を使いますが、ABI のレコードは型引数をすべてフィールドで使い（使わない型引数は `E1024`）、フィールドはスカラーかレコードなので、関数型が現れることはありません。debug 情報と
  `llvm_split`・trap の表示はこの綴りを使いません。利用者に見える出力（診断・ヘッダー・ドキュメント）は変わりません。
- テスト（修正前に失敗することを確かめました）:
  - `src/llvm.rs` の unit test `canonical_types_are_injective_and_mangled_types_are_identifiers`: 純粋性・規約（`[]`・`[Read]`・`[Mut]`・`[Own]`・`[Own, Read]`・`[Read, Own]`・`[Read, Read]`・`[Own, Mut]`）・
    参照の可変性・4 つの `SharedKind`・dyn の印と region の 8 通り × 2 つのクラスの組・固定長 2 と 3・整数の符号と幅などだけが違う型と、それを record・`Vec`・関数などの引数にした 895 個の型で、
    `canonical_type` と `mangled_type` がどれも違い、`mangled_type` が識別子の文字だけであることを確かめます。修正前は
    `Record(0, [Function(… pure: false …)]) and Record(0, [Function(… pure: true …)]) are both spelled Main.Box[fn[->i64]]` で失敗しました。期待の綴り（`fn[i64->i64]`・`pure.fn$Li64$Ai64$R`・
    `fn.om[i64,string->unit]`）は上の規則から手で書いたものです。
  - suite `generic_records`（`tests/features.mjs`、native と WASM の `-O0`・`-O3`）に export `function_instances` を足しました。純粋と非純粋の `Box`・`Vec`、規約 `[Read]` の `Box` と規約の無い `ref string` の `Box`、
    規約 `[Own, Mut]` の `Vec` と規約の無い `ref mut Counter` の `Vec` を作って呼びます。期待値は手で数えた `(1 + n) + 80 + 500 + 15000 + ((7 - n) + 6) · 10⁵ + (n + 100n) · 10⁷`
    （`n = 5` で `5050815586`、`n = -2` で `-2018484421`）。`inspect` は、`%"tz.record.Main.Box[pure.fn[i64->i64]]"`・`%"tz.record.Main.Box[fn.r[ref[string]->i64]]"` などの型と、
    `@tz.builtin.Vec.empty.pure.fn$Li64$Ai64$R`・`@tz.builtin.Vec.empty.fn.om$Li64$Crefmut$LMain.Counter$R$Aunit$R` などの定義がそれぞれ 1 回だけであることを確かめます。base の binary では
    `%"tz.record.Main.Box[fn[i64->i64]]" = type { %tz.closure } is defined once` で失敗します。
- 修正後の証拠: レビューの `Vec` のプログラムは `0`、`Box` のプログラム（`a.v 3 + b.v 4`）は `12`、規約の 2 つは `16` と `2` を native の `-O0`・`-O3` で返し、IR には
  `@tz.builtin.Vec.empty.fn$Li64$Ai64$R` と `@tz.builtin.Vec.empty.pure.fn$Li64$Ai64$R` が 1 つずつ定義されます。WASM は suite `generic_records` の `function_instances` で確かめました。
- 手順 4 の IR の比較のやり直し: `Main.tz` を持つ fixture の 139 個を、native（`domain`・`domain_host` は `--trap-mode return`）と `--target wasm32` で `--emit llvm` しました。
  - `main` のソースの fixture: base の binary とこの branch の binary で 278 ファイルすべて byte 単位で一致しました。失敗は両方で同じ wasm32 の 10 個（`blocking`・`domain`・`domain_host`・`effects_sweep`・`net_*` の 5 個・`tzroutine_net_wasm`）です。
  - この branch の fixture: 違うのは 6 ファイルで、どれも綴りだけです。`dyn_dispatch`・`preemption`（native と wasm32）は A29 本体が足した印を持つ dyn 型の `$P`（新しい IR の `$P` を `+` に戻すと base の IR と一致）、
    `generic_records`（native と wasm32）はこの修正の `pure.fn`・`fn.r`・`fn.om`（新しい IR の綴りを `fn` に戻すと base の IR と一致。base の IR はその型と関数を 2 回定義していて clang が拒否する）です。
  - 同じ入力で 2 回出力した `generic_records` の IR は一致しました（決定的）。
- 判断: 停止条件の「引用符で囲む名前の綴りまで変えたくなった」は、引用符付きの名前も衝突する（`redefinition of type`）ので、オーケストレーターの判断で上書きし、該当する型の綴りだけを変えました。
  意味が同じで `Eq` が違う規約の並び（`[Read]` と `[Read, Own]`）を 1 つにまとめる正規化は、型検査の振る舞いに関わるので行っていません（綴りは `Eq` に合わせて分けます）。
  （この判断の規約の部分は 2 回目で改めました。型検査の `unify` は規約をまったく比べないので、規約を綴ると 1 つの値が 2 つの LLVM 型を持ちます。下の「2 回目」を見てください。）

#### 2（minor）: 「`$` は `canonical_type` の綴りに現れない」は誤りだった

- 再現: レビューの active pattern のプログラム（`(|C|P|)`）は両方の binary で `20` を返し、修正後の IR に `@tz.builtin.Vec.empty.Main.Active$active$C$P$total$result` と
  `%"tz.union.Main.Active$active$C$P$total$result"` が現れることを確かめました。衝突は起きていません。
- 対応: `docs/architecture.md`（組み込み関数のシンボル）、`mangled_type` の doc comment、この文書の「アルゴリズム」と D1 の理由を直しました。`$` が現れるのは複数の case を持つ
  active pattern の暗黙の union の名前（`src/parse_control.rs`。部分 active pattern は case が 1 つで union を作らないので、名前は必ず `$total$result` で終わる）の中だけで、
  モジュールの後の `.Active$active$` で始まり `$total$result` で終わり、間には大文字で始まる識別子の case 名と区切りの `$` しか無く、置換する記号を含みません。そのため case 名が `C` や `P` でも
  綴りの読み方は 1 通りに決まり、単射は保たれる、と書きました。`canonical_type` の doc comment にも、`Eq` が区別する型を別に綴ることを書きました。`docs/architecture.md` の generic record の型名の節には
  `pure.fn[T->R]`・`fn.rm[T,U->R]` を足しました。

#### 3（minor）: tzroutine のページが「Copy でない `dyn` は捕捉できない」と書いていた

- 再現: レビューのプログラムで、`dyn (Display, Send)` と `dyn (Display, Copy, Send)` の捕捉は `42`、`dyn (Display, Copy)` と `dyn Display` は
  `E1013`（`values that move to another tzroutine or thread must be owned; dyn (Display, Copy) contains a reference`）でした。
- 対応: [tzroutine](../../_tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md) の捕捉の規則の表を、`Send` の印を持たない `dyn`（`dyn C`・`dyn (C, Copy)`）が不可で、`dyn (C, Send)` は Copy でなくても捕捉できる、に直しました。
  参照と `docs/language.md` の「Copy でない `dyn`」をすべて grep し、残りは `Sync`（`Arc` の中身・`Routine.scope` の共有）と、関数値への捕捉の `Capture` の規則で、tzroutine の捕捉の話ではないので変えていません。

#### 4（nit）: A24 のエスケープの一覧に `$P` が無かった

- 対応: `_features/A24-anonymous-unions.md` の一覧に `$P` と `P` を足し、`canonical_type`・`mangled_type` の行番号を今の `src/llvm.rs:3729`・`src/llvm.rs:7923` に直しました。合成した名前を足すときは、
  `$` の現れる場所を説明している `mangled_type` の doc comment と `docs/architecture.md` も更新するよう 1 文足しました。

#### 実行したコマンド

- `cargo fmt --all -- --check`: 成功。`cargo clippy --all-targets -- -D warnings`: 成功。
- `cargo test --locked --lib canonical_types_are_injective`: 1 passed（修正の関数型の腕を戻すと 1 failed）。
- `cargo nextest run --locked`: 2936 tests run: 2935 passed, 1 failed, 4 skipped。失敗した `alloc_contracts::std_allocation_contracts_hold_on_native_and_wasm32_at_o0_and_o3` は、ほかの worktree と共有する
  machine の負荷で wasm32 の実行が 30 秒の timeout になったもので、単独の再実行 `cargo nextest run --locked --test alloc_contracts` は 2 passed でした。
- `node tests/features.mjs target/debug/tsuzuri <suite>`: `generic_records`（12 cases）・`dyn_dispatch`（58）・`pure_functions`（302）・`conventions`（36）・`active_patterns`（17）・`unions`（27）が native と WASM の `-O0`・`-O3` で成功。
- `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs _tsuzuri/language-reference/async-tasks-and-lazy/tzroutine.md`: 成功（1 page, 82 links, 20 checked examples, 34 native runs）。
- `node scripts/check-guide-lite.mjs`: 成功（88 references）。診断と generated block は変えていないので、`gen-docs.mjs --check` と `check-cli-docs.mjs` は対象外です。

#### 2 回目: 規約だけが違う関数型が 2 つの LLVM 型になった（検証の指摘、major）

1 回目の修正の検証で、新しい問題の指摘が 1 件ありました。型検査の `unify`（`src/polymorph.rs` の `(Type::Function(a, x), Type::Function(b, y))` の腕）は関数型の純粋性を比べますが、
引数規約は比べません。そのため 1 回目のように規約を `fn.r[…]`・`fn.om[…]` と綴ると、規約だけが違う 2 つの型を持つ 1 つの値が 2 つの LLVM の struct 型を持ち、phi や `ret` で出会うと
clang が拒否しました（`E2002`。base の binary では `redefinition of type`）。

- 再現（2 回目の修正前の binary、native の `-O0`。期待値は手で数えたもの）:
  - c11: `Box { value: text_length }`（`def text_length :: string -> i64`。規約 `[Read]`）と `Box { value: \(t: ref string) -> t.length * 10 }` を if/else で選んで呼ぶ（期待 `5005`）。
    `'%v12' defined with type '%"tz.record.Main.Box[fn[ref[string]->i64]]"' but expected '%"tz.record.Main.Box[fn.r[ref[string]->i64]]"'`。
  - c12: `def get :: own i64 -> Box<ref string -> i64>` が `Box { value: text_length }` を返す（期待 `7`）。`value doesn't match function result type`（`ret %"…Box[fn.r[…]]"`）。
  - c16: `def bump :: i64 -> mut Counter -> unit`（規約 `[Own, Mut]`）と `\(step: i64) (counter: ref mut Counter) -> …` の `Box` を if/else で選ぶ（期待 `4301`）。同じく phi の型の不一致。
- ABI の確認: 規約だけが違う関数型は同じ ABI です。規約は引数の書き方の記録で、引数の型そのものは型検査がすでに変えています（`src/check.rs` の scheme の構築で、既定で読む引数と `mut T` の引数は
  `Type::Reference(T, false)`・`Type::Reference(T, true)` になる）。そのため規約 `[Read]` の `string -> i64` と明示の `ref string -> i64` は、どちらも引数の型が `ref[string]` で、
  `[Own, Mut]` の `i64 -> mut Counter -> unit` と `i64 -> ref mut Counter -> unit` はどちらも `refmut[Main.Counter]` です。`src/llvm*.rs` で `FunctionParameters::conventions` を読むのは
  1 回目の `canonical_type` だけで、関数型の LLVM の型（`%tz.closure`）も呼び出しの引数の型も `parameters.types` と結果の型だけから決まります。生成コードの外で規約を読むのは型検査
  （`closures.rs`・`contracts.rs`・`check.rs`）と `call_specialization.rs` の `transparent_calls`（規約が一致するときだけ関数型の cast を見通す、保守的な最適化）です。ABI が違う規約は
  見つからなかったので、すべての規約を正規化しました。
- 修正（`src/llvm.rs`）:
  - `codegen_identity`: 型の中のすべての関数型から引数規約を除いた型を返します（純粋性は残す）。生成コード上の型の同一性です。
  - `canonical_type` は規約を綴らず、純粋性だけを `pure.fn[…]` と綴ります。どの型の綴りも `codegen_identity` の綴りと同じで、同一性が違う型は別の綴りです。`mangled_type` の doc comment にも書きました。
  - 型で数える集合とキーを `codegen_identity` で数えます: 組み込み関数の具体化のキー（`Builtins`。キーの型引数と、定義に使う型）、レコードと共用体の定義の集合（`named_types`）、
    再帰型の helper の集合（`recursive_types`）、共有ブロックの helper の集合（`shared_types`）、vtable（`emit_dyn_tables`。規約だけが違う格納型の vtable は最初の 1 つだけを出し、
    `tz.dyn.drop[…]`・`tz.dyn.clone[…]` の集合も同一性で数える）、ホストの ABI のレコード（`llvm_abi.rs` の `record_types`）、debug 情報の型（`llvm_debug.rs` の `types`）。
  - `emit_typed_builtin` は具体化のキーと同じ checked wrapper を探していたので、キーが同一性になった後は規約の付いた wrapper が見つからず panic しました（c20 で再現）。型引数の同一性が
    キーと等しい wrapper を探すようにしました。
  - ユーザーのジェネリック関数の具体化（`polymorph.rs` の `request`）は型検査の側にあり、`$mono.N` の番号で名前が決まるので衝突しません。規約だけが違う型引数では同じ本体が 2 つ出ますが、
    正しさには影響しないので変えていません（vtable のスロット関数も、出さない方の vtable の adapter は出しません）。
- テスト（どれも base の binary で失敗します）:
  - suite `generic_records` に export を 2 つ足しました。`convention_mix` は c11・c12・c16 を 1 つにしたもので、期待値は `n > 0` で `5 + 500 + 301 · 10⁴`、それ以外で `50 + 500 + 4 · 10⁴`。
    `convention_instances` は規約だけが違う `Box` を再帰的な union（`Chain`）・`Rc`・region 付きの `dyn Display {r}`（vtable）・`Vec` に入れて混ぜるもので、期待値は手で数えた
    `n > 0` で `3 + 3 · 10⁴ + 12 · 10⁶ + 3000 · 10⁸`、それ以外で `33 + 300 · 10⁴ + 12 · 10⁶ + 3000 · 10⁸` です。`inspect` は、`%"tz.record.Main.Box[fn[ref[string]->i64]]"`・
    `%"tz.record.Main.Box[fn[i64,refmut[Main.Counter]->unit]]"`・`%"tz.record.Main.Box[pure.fn[i64->i64]]"`、`Vec.empty` の 4 つの具体化、
    `@"tz.vtable.Display[Main.Box[fn[ref[string]->i64]]]"` がそれぞれ 1 回だけ定義され、`fn.r`・`fn.om` のような規約の綴りがどこにも無いことを確かめます。
    1 回目の `function_instances` の `inspect` の期待（`fn.r`・`fn.om` の定義）は、規約を綴らない形に直しました。
  - unit test `canonical_types_are_injective_and_mangled_types_are_identifiers` を、同一性の上の単射に直しました。すべての型で `codegen_identity` が冪等で、`canonical_type` が同一性の綴りと
    同じであること、同一性の異なる型（純粋性・参照の可変性・`SharedKind`・dyn の印・長さ・整数の符号など）の綴りがどれも違うこと、規約 `[Read]` の `Box` の同一性が規約の無い `Box` と等しく、
    純粋な `Box` とは違うことを確かめます。`fn.om[i64,string->unit]` の期待は `fn[i64,string->unit]` に直しました。
- 修正後の証拠: c11 は `5005`、c12 は `7`、c16 は `4301`、c20（`convention_instances` と同じ形を整数のあふれる式で返したもの）は手で数えた `-8455817821460796237` を native の `-O0`・`-O3` で返します。
  1 回目のプログラム（`Vec.empty` の純粋と非純粋、`Box` の純粋と非純粋）と規約の衝突（`redefinition`）は suite `generic_records` の `function_instances` で、native と WASM の `-O0`・`-O3` で動きます。
  純粋と非純粋を混ぜる if/else は、どちらの binary でも型検査の `E1003` のままです（純粋性は `unify` が区別します）。
- 手順 4 の IR の比較のやり直し: `tests/fixtures/*` と `examples/*` の 176 個のディレクトリ（`Main.tz` を持つのは 153 個）と、`main` の版の `dyn_dispatch`・`preemption`・`generic_records` を、
  native と `--target wasm32` で `--emit llvm` し、base の binary とこの branch の binary の出力を比べました。
  - `main` のソースの fixture と example: 両方が出力したファイルはすべて byte 単位で一致しました。両方で同じく失敗したのは、`Main.tz` を持たないディレクトリと、wasm32 の 10 個（`blocking`・
    `domain`・`domain_host`・`effects_sweep`・`net_*` の 5 個・`tzroutine_net_wasm`）、`--trap-mode return` の要る native の `domain`・`domain_host` です（`--trap-mode return` を付けると一致）。
    `main` の fixture に純粋な関数型を型引数に持つプログラムは無いので、変わらないのが期待どおりです。
  - この branch の fixture で違うのは 6 ファイルです。`dyn_dispatch`・`preemption`（native と wasm32）は A29 本体の `$P`（新しい IR の `$P` を `+` に戻すと base の IR と一致）。
    `generic_records`（native と wasm32）は、純粋な関数型の `pure.fn` の綴り（13 行）と、base の IR が規約の変種ごとに 2 回定義していた 14 個の定義（型 4 つ、`Vec.empty`・`Vec.push_mut`・`Rc.new` の
    具体化、再帰型の helper 3 つ、`tz.dyn.drop`、vtable とそのスロット）を 1 回にしたことだけです（新しい IR の `pure.fn` を `fn` に戻すと、新しい IR の定義はすべて base の IR にあります）。
  - 同じ入力で 2 回出力した `generic_records` の IR は native と wasm32 で一致しました（決定的）。
- 見つけた既存の問題（`main` でも同じなので変えていません）: 期待の型が規約 `[Read]` の関数型のとき、引数に `ref string` と注釈したラムダを直接書くと `E1003`（`expected ref string, found string`）
  になります（`if n > 0 then first else Box { value: \(text: ref string) -> … }`）。ラムダをいったん変数に束縛すると通ります。型検査の側の問題なので、この branch では扱いません。

##### 2 回目に実行したコマンド
- `cargo fmt --all -- --check`: 成功。`cargo clippy --all-targets -- -D warnings`: 成功。
- `cargo test --locked --lib canonical_types_are_injective`: 1 passed。
- `cargo nextest run --locked`: 2936 tests run: 2936 passed (9 slow), 4 skipped。
- `node tests/features.mjs target/debug/tsuzuri <suite>`: `dyn_dispatch`（58 cases）・`generic_records`（16。base の binary では `inspect` の「1 回だけ定義」で失敗）・`conventions`（36）・
  `pure_functions`（302）・`unions`（27）・`active_patterns`（17）が native と WASM の `-O0`・`-O3` で成功。
- 手順 4 の IR の比較（上のとおり）と、`generic_records` の 2 回の出力の比較。
- `node scripts/check-guide-lite.mjs`: 成功（88 references）。`_features/A24-anonymous-unions.md` の `canonical_type`・`mangled_type` の行番号を `src/llvm.rs:3795`・`src/llvm.rs:7980` に直しました。
- 参照（`_tsuzuri/language-reference/`）は変えていません。規約の綴りは利用者に見えず、1 回目も参照に書いていないためです。`docs/architecture.md` の組み込み関数のシンボルの節と
  generic record の型名の節を、規約を綴らず `codegen_identity` で数える形に直しました。
