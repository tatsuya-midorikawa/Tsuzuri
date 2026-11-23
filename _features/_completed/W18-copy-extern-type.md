# W18: `extern type copy`（所有しない識別子のハンドル）

| 項目 | 内容 |
| --- | --- |
| ID | W18 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | – |
| 後続 | [W19](../W19-thread-bound-handles.md)（`extern type local`）、[W21](../W21-c-layout-records.md)（`@c_layout` の field の `copy` ハンドル）、[W27](../W27-win32-package-from-winmd.md)（解放関数のないハンドルを `copy` に写す） |
| 状態 | done |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 要承認（E12 D3、`check.rs` の Sync の理由、D-55）。台帳は [GUIDE D-63](../GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30) |
| 手本にする既存実装 | `Type::Shared(Box<Type>, SharedKind)`（種類を型の variant に持つ形）、`DynType::copy`（`is_copy`・`can_capture`・`sync_here` が印で分かれる形）、`Parser::extern_declaration`（`src/parser.rs`）、[E12](E12-ffi-extensions.md) の段 C（`tests/ffi_extensions.rs` の `parses_extern_types`・`extern_types_are_noncopy_leaf_types`） |
| 主な影響ファイル | `src/syntax.rs`・`src/parser.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`・`src/check.rs`・`src/polymorph.rs`・`src/llvm.rs`・`src/diagnostic_kinds.rs`・`src/formatter.rs`・`src/docgen.rs`・`src/api_diff.rs`・`src/semantic.rs`・`src/kg_capture.rs`・`src/guide_std.rs`・`src/repl.rs`・`src/find.rs`・`src/type_arena.rs`（と `Type::Handle` を照合する全ファイル）・`tests/ffi_extensions.rs`・`tests/ffi_extensions.mjs`・`tests/ffi_extensions_host.c`・`tests/fixtures/ffi_extensions/Main.tz`・`tests/windows.mjs`・`vsc/syntaxes/tsuzuri.tmLanguage.json`・`spec/rules/ABI.md`・文書 |

## 実装者向け早見

- 範囲: `extern type copy Hwnd` と書いたハンドルを Copy・捕捉できる・Send・Sync にする。値・ABI・C ヘッダー・WASM は今の `extern type` と同じ 1 ポインター。修飾のない `extern type` は 1 byte も変えない。
- 最初に読む: ① 仕様 1〜3 ② 現状の表 ③ `src/check.rs` の `Type::is_copy`・`can_capture`・`can_send`・`can_sync`・`sync_here`・`holds_host_state` ④ E12 D3。
- 最初に書くテスト: `tests/ffi_extensions.rs` の `parses_copy_extern_types`（修飾の読み方と、`extern type copy` だけの行が今どおり型 `copy` を宣言すること）。
- 触ってはいけない: 修飾のない `extern type` の規則・診断の文、`pure`・`Domain.run`・`const` がハンドルを平たいデータと見ないこと、ABI の位置の規則（E12 D5）。
- 完了の見分け方: 次が `check` を通り、Windows の native で終了コード 1 で終わる（`IsWindow` の 1 が 3 回。今は捕捉の行が `E1005`）。

```tsuzuri
extern type copy Hwnd
@trusted
extern "GetDesktopWindow" def desktop :: unit -> Hwnd
@trusted
extern "IsWindow" def is_window :: Hwnd -> i32

def main :: unit -> i32 = \() ->
    let h = desktop ()
    let probe = \() -> is_window h
    let copied = h
    probe () + is_window h + is_window copied - 2
```

## 目的

Win32 の HWND・HINSTANCE・HICON・HCURSOR・`GetStockObject` のブラシは、ホストが所有権を渡さない識別子である。数十の呼び出しへ渡し、アプリの状態に置き、関数値に捕捉する。今の `extern type` は E12 D3 のとおり Copy でなく捕捉できず Sync でないので、すべての使用に `ref h` が要り、関数値に入れられず、それを持つ record も Copy でなくなる。このチケットは、所有しない識別子だと宣言したハンドルに限って D3 を改め、普通の値として扱えるようにする。所有するハンドル（`CreateFontW` の HFONT、ファイルの HANDLE）は今の `extern type` と `Drop` を包む record のまま残す。

## 着手条件と停止条件

### 着手条件

- 承認（決定事項 D1・D2）。D-63 の台帳の見出しが `_features/GUIDE.md` にあること（`grep -n "D-63" _features/GUIDE.md`）。
- `grep -n "Handle(Box<str>)" src/check.rs` が 1 件で、`Type::Handle` が旗を持たないこと（「現状」と同じ）。違えば、このチケットを直してから始める。
- `grep -n "FRONTEND_FORMAT: u32" src/frontend_cache.rs` で今の値を控える（起票時は 20）。

### 停止条件

- **S1** `Type::Handle` に種類を足すと `Type` が大きくなる（`std::mem::size_of::<Type>()` が着手時の値より大きい。`enum Type` の注釈は 4 語を保つと書く）。候補: A 種類を `Box<str>` と一緒に箱へ入れる、B 止めて報告する。
- **S2** 修飾のない `extern type` を使うプログラム（`tests/fixtures/*`・`examples/*`・`tests/ffi_extensions.mjs` の fixture）の `--emit llvm`（native と wasm32）・`--emit header`・`check --json` が 1 byte でも変わる。
- **S3** 関数値の環境の複製（`@tz.env.clone.*`）や再帰型の複製（`clone.new`）が `copy` ハンドルで `clone_value` の `unreachable!` に届き、恒等の複製にする以外の直し方が要る。
- **S4** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

| 所在 | 今の振る舞い |
| --- | --- |
| `src/syntax.rs` の `ExternTypeDecl` | `doc`・`visibility`・`name` だけ。修飾の場所はない |
| `src/parser.rs` の `Parser::extern_declaration` | `extern type` の後に名前を 1 つ読み、`<`・`=` なら `E0002`（`extern_type_with_body`）。リンク名があれば `E0002`（`extern_type_with_link_name`） |
| `src/check.rs` の `enum Type` | `Handle(Box<str>)`（修飾名だけ）。`Names::handles` は `NameInfo { id, name, module, visibility }` で、型の解決（`NamedType::Handle(info) => Type::Handle(info.name.as_str().into())`）は種類を持たない |
| `Type::is_copy` | `Self::Handle(_)` は偽 |
| `Type::can_capture` | 再帰型の枝（`stored_all_closed` の中の `Type::Handle(_)`）と通常の枝（`Self::Handle(_) => false`）の両方で偽。理由は E12 の型規則「関数値の複製がハンドルを複製するため」 |
| `Type::can_send` | `Self::Handle(_)` は真（E12「Task へ move できる」） |
| `Type::can_sync` の doc と `sync_here` | 「Not Sync are an extern handle (host libraries are rarely thread-safe)」。`sync_here` は `Self::Handle(_)` を偽 |
| `Type::holds_host_state` | ハンドルを持つ値で真。`src/polymorph.rs` の `validate_instance` が `Arc` の Send・Capture の誤りの文を選ぶのに使う |
| `src/llvm.rs` の `clone_value` | `Type::Handle(_) => unreachable!("extern handles cannot be cloned")`。環境の複製関数（`closure_slots` の `@tz.env.clone.*`）と `src/llvm_recursive.rs` の `clone.new` は、捕捉・格納するすべての型が `can_capture` のときだけ作る |
| `src/pure_rules.rs` の `plain_part`・`src/const_rules.rs` の `value_part` | ハンドルは平たいデータでなく、`own`・`Domain.run`・`const` に置けない |
| ABI（`src/abi.rs` の `is_handle`、E12 D4・D5） | 引数 `H`・`ref H`、結果 `H`。record の field・buffer の要素・`ref mut H` は `E1008` |

開発版（`target/debug/tsuzuri.exe check`、`check --json` で kind を確認）での再現:

- `extern type Hwnd` の値を `\() -> is_window (ref h)` に捕捉すると `E1005`（kind `capture_exclusive_in_reusable_function`、文「cannot capture Main.Hwnd in a reusable function; …」）。
- `Routine.scope (ref h) 2 (\w _i -> is_window w)` は `E1013`（`task_requires_sync`、「… Main.Hwnd is not Sync …」）。`Arc.new h` を `tzroutine { }` へ渡すと `E1013`（`task_requires_send`、「… shares an extern handle … through an Arc …」）。
- `extern type copy Hwnd` の後に `def` を書くと `E0002`（「expected the end of the file; declarations must precede the entry-point code」）。今の構文は `extern type copy` を型 `copy` の宣言と読み、`Hwnd` をトップレベルの式と読む。そのため `const Hwnd: i64 = 3` の後の `extern type copy Hwnd` は今 `check` を通る（型 `copy` を宣言し、式 `Hwnd` を評価する）。

## 変えてはいけない意味

- 修飾のない `extern type` は E12 D3 のまま（Copy でない、捕捉できない、Send、Sync でない、drop glue なし）。その診断の文と kind も変えない。
- `copy` ハンドルも平たいデータではない（`pure` の `own`・`Domain.run`・`const`（`E1026`）・`export` の record の field に置けない）。`Eq`・`Display`・`Hash` の instance を持たない（利用者も書けない。`E1016`）。
- ABI の位置（E12 D5）と表現（E12 D4: native は `ptr`、wasm32 は `i32`、header は `typedef struct tz_handle_…_s *tz_handle_…;`）。
- `copy` が保証するのは「識別子の値を複製し、どのスレッドへ渡しても Tsuzuri の側は壊れない」ことだけである。その識別子を受け取る API がどのスレッドから呼べるか（`DestroyWindow` は作ったスレッドでないと失敗し、別スレッドからの `SendMessageW` は待ち合わせる）は、今までどおり `extern` の宣言の契約（`@trusted` が表す）であり、コンパイラは確かめない。

## 仕様

### 1. 構文

```text
extern type copy Name
private extern type copy Name
```

- `type` の直後の識別子が `copy` で、**同じ行に**さらに識別子が続くときだけ、`copy` を修飾として読む（文脈キーワード。予約語にしない。D-55）。`extern type copy` の後が行末なら、今どおり型 `copy` の宣言である。
- 修飾は 1 つまで。修飾の後にもう 1 つ修飾の語が続いて同じ行に識別子があれば `E0002`（kind 案 `extern_type_modifier_conflict`、文 `an extern type takes at most one of 'copy' and 'local'; remove one`）。W19 が `local` を足すまで、修飾の語は `copy` だけ。
- `@trusted` は今どおり前に書け、性質は付けられない（`E1037` の `trusted_type_property`）。リンク名・型引数・`=` の `E0002` も今どおり。全 edition で同じに読む（D1）。

### 2. 型規則

| 性質 | 修飾なし（E12 D3、変えない） | `copy`（このチケット） |
| --- | --- | --- |
| Copy | 偽（値渡しは move、`E1012`） | 真（値渡しは複製。`ref` も書ける） |
| 捕捉（`Capture`） | 偽（`E1005`） | 真 |
| Send | 真 | 真 |
| Sync | 偽（`E1013`） | 真（`Routine.scope` で共有でき、`Arc<H>` を tzroutine へ渡せる） |
| drop glue・instance・リテラル | なし | なし |

- `copy` ハンドルだけを（ほかの Copy な値と一緒に）持つ record・union・タプル・配列は Copy になる（今の `record_fields_all` の規則のまま）。
- 同じ名前のハンドルの種類は宣言で 1 つに決まる。型の等しさ・正準順序・表示（`Main.Hwnd`）は種類を含めずに今どおり。

### 3. ABI・WASM・ヘッダー

- 置ける ABI の位置と下げ方は修飾のないハンドルと同じ（E12 D5）。`ref mut H`・record の field・buffer の要素は今どおり `E1008`（`ref mut` は W13、field は W21 が扱う）。
- 生成 IR・object・C ヘッダー・`--emit bindings-*`・wasm32 の JS の型（`number & { readonly __tsuzuri: "tz_handle_…" }`）は、修飾の有無で変わらない。WASM でも使える。

## 設計

### 種類の置き場所（D3）

`src/syntax.rs` に種類の列挙を置き、AST と型の両方が使う:

```rust
/// What an `extern type` declaration says about its values (E12 D3, W18).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HandleKind {
    /// `extern type H`: moved, not capturable, Send, not Sync (E12 D3).
    Owned,
    /// `extern type copy H`: an identifier that the host does not hand over (W18).
    Copy,
}
```

- `ExternTypeDecl` に `kind: HandleKind` と `kind_span: Option<Span>`（修飾の語の位置。整形と診断用）を足す。`Wire for ExternTypeDecl` を足した field ごと符号化し、`FRONTEND_FORMAT` を 1 上げる。
- `Type::Handle(Box<str>)` を `Type::Handle(Box<str>, HandleKind)` にする（`Type::Shared` の `SharedKind` と同じ形）。`Names` にハンドルの番号順の `handle_kinds: Vec<HandleKind>` を足し、解決は `Type::Handle(info.name.as_str().into(), names.handle_kinds[info.id])`。`find.rs` の表示用の `Type::Handle("_".into())` とテストの構築は `HandleKind::Owned`。
- `Type::Handle(..)` を照合する全箇所（`grep -rn "Type::Handle(\|Self::Handle(" src/ --include=*.rs`。起票時 `Type::Handle(` だけで 23 ファイル 49 か所）を `Type::Handle(name, _)` などに直し、種類で分かれる所だけ下の表のとおりにする。`type_arena.rs` のハッシュは種類も入れる。

### 種類で分かれる所

| 関数 | `copy` での値 |
| --- | --- |
| `Type::is_copy` | 真 |
| `Type::can_capture` | 通常の枝と再帰型の枝（`stored_all_closed` の述語）の両方で真 |
| `Type::sync_here` | 真。`can_sync` の doc を「`copy` でない extern handle」に直す |
| `Type::can_send` | 今どおり真（両方の種類） |
| `Type::holds_host_state` | `copy` は数えない（`Arc` の誤りの文が `copy` ハンドルを原因に挙げないため） |
| `Type::is_frozen` | 今どおり偽（最適化の前提を増やさない） |
| `src/llvm.rs` の `clone_value` | `Type::Handle(_, HandleKind::Copy)` は値そのものを返す（ポインターの複製）。`Owned` は今どおり `unreachable!` |
| `pure_rules::plain_part`・`what`、`const_rules::value_part`、`const_static.rs` | 変えない（両方の種類で平たいデータでない） |

### 宣言の見出しを書く所

`extern type {name}` を書く所（`src/docgen.rs`・`src/api_diff.rs`・`src/kg_capture.rs`・`src/semantic.rs` の 2 か所・`src/guide_std.rs`・`src/repl.rs` の一覧）は `extern type copy {name}` と書く。`src/formatter.rs` は AST の比較（`canonicalize`）に種類が入るので、修飾を落とす整形は安全検査で止まる。整形の出力は `extern type copy Name` の 1 つの空白に正規化する。

- `tsuzuri api-diff`: 見出しが変わるので `changed`。`copy` を足すのは互換（今通るプログラムは通る）、外すのは互換でない（note 案 `values of the type are moved again`）。

### 診断

- 新しい kind は `extern_type_modifier_conflict`（`E0002`）だけ。G23 の登録簿（`src/diagnostic_kinds.rs`）に足す。
- 既存の文は変えない。`Sync` の誤りの文（`polymorph.rs` の `TaskRequiresSync`）の「an extern handle」は、修飾のないハンドルについて今も正しい。

## 前提とする他チケットのインターフェース

なし（依存なし）。

## 他チケットへ提供するインターフェース

- `syntax::HandleKind`（`Owned`・`Copy`）。W19 が `Local` を足し、`Type::Handle(_, HandleKind::Local)` を使う。
- `ExternTypeDecl::kind`・`kind_span` と、修飾の語を 1 つ読む `Parser::extern_declaration` の枠（W19 は語の集合に `local` を足すだけ。`extern_type_modifier_conflict` もそのまま使う）。
- `Type::Handle(name, HandleKind::Copy)` の判定。W13（`ref mut T` の T に `copy` ハンドル）、W15（`Maybe<H>`）、W21（`@c_layout` の field の `copy` ハンドル）、W27（RAIIFree のないハンドルを `copy` に生成）がこれで分ける。

## 実装手順

1. **変更**: ブランチ `feat/w18-copy-extern-type`。**内容**: `node scripts/repo-kb.mjs brief W18`、着手条件の `grep`、`tests/fixtures/*`・`examples/*` の `--emit llvm`（native と `--target wasm32`）・`--emit header`・`check --json` を `target/scratch/w18/before/` に保存。**確認**: 保存した組の数を控える。
2. **変更**: `src/syntax.rs`（`HandleKind`・`ExternTypeDecl`）、`src/parser.rs`、`src/syntax_codec.rs`、`src/frontend_cache.rs`、`src/diagnostic_kinds.rs`、`src/formatter.rs`。**内容**: 仕様 1。**確認**: `cargo test --locked --test ffi_extensions parses_` が `parses_copy_extern_types` を含めて通る（`running N tests` を見る）。`cargo test --locked --test formatter`。
3. **変更**: `src/check.rs`（`Type::Handle` の形、`Names::handle_kinds`、上の表）、`Type::Handle` を照合する全ファイル。**内容**: 仕様 2。**確認**: `copy_extern_types_are_copy_capturable_send_sync` と既存の `extern_types_are_noncopy_leaf_types` が通る。T0。
4. **変更**: `src/llvm.rs` の `clone_value`。**内容**: 恒等の複製。**確認**: 捕捉した関数値を複製する E2E（`copy_handle_closure_clone`）が `-O0`・`-O3` で通り、`live == 0`。S3 に当たれば止める。
5. **変更**: `src/docgen.rs`・`src/api_diff.rs`・`src/kg_capture.rs`・`src/semantic.rs`・`src/guide_std.rs`・`src/repl.rs`・`vsc/syntaxes/tsuzuri.tmLanguage.json`（`(?<=\bextern\s+type\s+)copy\b(?=\s+[A-Za-z_])`）。**内容**: 「宣言の見出しを書く所」と `api-diff` の互換の判定（D6）。**確認**: `tests/api_diff.rs` に足した 2 件（足す・外す）、`tests/lsp.rs` の hover に足した 1 件、`node tests/docgen.mjs target/debug/tsuzuri`、`cargo test --locked --test kg_build`。
6. **変更**: `tests/ffi_extensions.mjs`・`tests/ffi_extensions_host.c`・`tests/fixtures/ffi_extensions/Main.tz`・`tests/windows.mjs`。**内容**: テスト計画の E2E（`copy_handles`・`copy_handles_routine`・`copy_handle_closure_clone`、Windows の早見の例）。**確認**: `node tests/ffi_extensions.mjs target/debug/tsuzuri`（native と WASM、`-O0`・`-O3`）。Windows の段は CI（`ci:full`）。
7. **変更**: 文書と `spec/rules/ABI.md`・`tests/conformance/ABI/`。**内容**: 「ドキュメント」の節。修飾を使わないプログラムを手順 1 と同じ形で出し直す。**確認**: 手順 1 の保存と `cmp` で全組が一致（S2）。T2。PR に `ci:full`。

## テスト計画

### Rust（`tests/ffi_extensions.rs`）

| テスト | 内容 | 期待 |
| --- | --- | --- |
| `parses_copy_extern_types` | `extern type copy A`・`private extern type copy B`・`/// doc` 付き、`extern type copy` だけの行（型 `copy`）、`extern type copy local`（W19 の前は型 `local` の `copy` ハンドル）、`extern type copy copy X`、整形の `extern   type  copy   A` | 種類と名前、`E0002` は kind `extern_type_modifier_conflict`、整形は `extern type copy A` |
| `copy_extern_types_are_copy_capturable_send_sync` | move した後の使用、関数値への捕捉と複製、`tzroutine { }` への複製、`Routine.scope (ref h)`、`Arc<H>` を tzroutine へ、`copy` ハンドルの field だけの record の複製 | すべて受理 |
| 同じテストの拒否 | `const` に置く、`export def` の record の field、`ref mut H` の extern 引数、`h == h`・`to_string h`、`pure def` の `own` 引数 | `E1026`・`E1008`・`E1008`・`E1005`・既存のコード（期待は今の修飾なしと同じコード） |
| `copy_handles_lower_like_handles` | 同じ使い方（すべて `ref` か move）のプログラムを修飾の有無で 2 つ書き、`--emit llvm` 相当（`llvm::emit`）と header を比べる | 一致 |
| `extern_types_are_noncopy_leaf_types`（既存） | 修飾なしの規則 | 変わらない |

### E2E

- `tests/fixtures/ffi_extensions/Main.tz` に `copy_handles`（ホストの `w18_id_new` が静的な struct のアドレスを返し、Tsuzuri が複製・捕捉・関数値の複製をして `w18_id_value` を 4 回呼ぶ）と、native だけ `copy_handles_routine`（`tzroutine { }` と `Routine.scope` へ渡してホストで数を合わせる）。期待値はホストの C の参照実装の手計算。native `-O0`・`-O3`（`live == 0`）、WASM `-O0`・`-O3`（tzroutine の段を除く）。
- `tests/windows.mjs`: 早見の例（`GetDesktopWindow`・`IsWindow`）を `-l user32` を付けて MSVC で build・実行し、終了コード 1。

### 不変

- 手順 1 の保存と比べ、修飾を使わない全プロジェクトの IR（native・wasm32）・header・`check --json` が一致する。

## ドキュメント

- `docs/language.md` の「不透明ハンドル」（`copy` の段落、所有しない識別子だけに使うこと、使ってはいけない例: `CreateFontW` の HFONT・ファイルの HANDLE・HDC（W19））、`Sync`・`Capture` の表（「`copy` でない extern ハンドル」）、`Arc` の段落。
- `_tsuzuri/language-reference/compiler/native-interop.md` の「不透明ハンドル」と「スレッドとホストの責任」、`types-and-type-inference/constraints.md` の表と `Sync` の段落、`built-in-types-and-modules/rc.md`・`routine.md` の `Sync` の列挙、`values-and-functions/lambda-expressions.md` の捕捉の段落、`compiler/diagnostics.md`（新しい kind。kind の表は `<!-- generated:diagnostic-kinds -->` の生成の部分なので手で書かず、`src/diagnostic_kinds.rs` に登録してから `node scripts/gen-docs.mjs`）。
- `README.md` の機能一覧の「不透明ハンドルとコールバック」に 1 文。`spec/rules/ABI.md` に規則 1 つ（番号は着手時の最大 + 1。案「A value of an `extern type copy` MUST be copied, and MUST cross the C ABI as the same pointer as other handles.」）と `tests/conformance/ABI/` の例。
- `docs/architecture.md` の `Type::Handle` の記述。D-55 の表に文脈キーワード `copy`（`extern type` の直後）を足す（台帳の更新はオーケストレーターと相談）。

## 受け入れ条件

- [x] 仕様 1〜3 のとおりに動き、テスト計画の Rust のテストと E2E が通る。
- [x] 修飾を使わないプログラムの IR・object・header・診断が 1 byte も変わらない（S2 の比較を記録した）。
- [x] `extern type copy` だけの行が型 `copy` の宣言のままである。
- [x] `FRONTEND_FORMAT` を 1 上げた。
- [x] 文書・`spec/rules/ABI.md`・D-55 の表を更新し、`node scripts/check-docs.mjs <変更したページ>` が通る。
- [x] T2 が通り、PR に `ci:full` を付けて `windows.yml`・`vscode.yml` が通った。

## 落とし穴

- **所有するハンドルに `copy` を付けると二重解放になる**: 複製が `Drop` を包む record（B07）の外へ漏れる。コンパイラは見分けられないので、文書に「解放関数があるハンドルには付けない」と例つきで書く。
- **`unreachable!` の複製**: `copy` ハンドルを捕捉した関数値の複製と、それを持つ再帰型の複製は `clone_value` を通る。手順 4 の E2E で必ず通す。
- **照合の漏れ**: `Type::Handle(_)` を `_ =>` で拾う `match` が多い。`grep -rn "Handle" src/ --include=*.rs` の全件を見て、`is_copy` と同じ判断が要る所（`llvm_escape.rs`・`llvm_facts.rs`・`call_specialization.rs` の `can_capture`）を確かめる。
- **構文の衝突**: `const Hwnd: i64 = 3` の後の `extern type copy Hwnd` は意味が変わる（D1）。release notes に書く。
- **WASM の 0 ハンドル**: `copy` は値の表し方を変えないので、wasm32 の 0 の扱い（B07 の懸念）も今と同じ。

## 対象外

- `extern type` への `Drop`（B07 の拡張）、`Eq`・`Hash` などの instance、ハンドルの比較。
- `ref mut H`（W13）、`Maybe<H>` と NULL（W15）、`@c_layout` の field（W21）。
- スレッドに結び付くハンドル（W19）。
- `tsuzuri bindgen` が `copy` を推測して付けること（C のヘッダーからは所有を判断できない。W27 は win32metadata の RAIIFree で決める）。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 修飾は `extern type copy Name`。`copy` は `type` の直後で同じ行に識別子が続くときだけの文脈キーワードで、全 edition で読む。今 `check` を通る形のうち意味が変わるのは「型 `copy` の宣言の直後の同じ行に、識別子で始まるトップレベルの式を書いた形」（`const Hwnd: i64 = 3` の後の `extern type copy Hwnd`）だけで、release notes に書く | 予約語を足さない（D-55）。TZ02 の `tzroutine`（直後が `{`）と同じ前例。[共通の綴り](../README.md#共通の綴り起票時に決めた契約)のとおり | 要承認（承認前は該当する段に着手しない） |
| D2 | `copy` のハンドルは Copy・捕捉できる・Send・Sync で、drop glue と instance を持たない。E12 D3 と `src/check.rs` の `can_sync` の理由（「host libraries are rarely thread-safe」）を `copy` に限って改める | 関数値は Send・Sync と扱われるので、捕捉できる値は Sync でなければならない。Copy で Send な値は複製を各スレッドへ送れるので、Sync を偽にしても同時の使用は防げず、`Routine.scope` と `Arc` が不便になるだけである。Tsuzuri はハンドルを参照外ししないので、複製で壊れるのはホストの契約だけ | 要承認（承認前は該当する段に着手しない） |
| D3 | 種類は `Type::Handle(Box<str>, HandleKind)` に持つ | `TypeContext` の表で引く案は、構築する 18 か所（表示だけの `guide_std.rs`・`kg/ids.rs` を含む）のどれかが表を渡し忘れると、型検査と生成で Copy の答えが黙って食い違う。`SharedKind` と同じく型に持てば判定は局所的で、新しい種類を選ぶのは名前の解決の 1 か所（置き換え・作り直しは照合した種類を写すだけ） | 既定案（実装者はこの案に従う） |
| D4 | ABI・ヘッダー・WASM・`--emit bindings-*` は修飾で変えない。`ref mut H` と record の field は今どおり `E1008` | E12 D4・D5 を保ち、ABI の拡張は W13・W15・W21 にまとめる | 既定案（実装者はこの案に従う） |
| D5 | `copy` に `@trusted` を求めず、`tsuzuri audit` にも項目を足さない | 値は `extern` の結果か export の引数からだけ来る。それを作る `extern` の使用はすでに `ffi` の権限と `W1010` の対象で、「識別子である」ことはその宣言の所有権の約束（`@trusted` が表す）に含まれる。別の印は同じ確認を 2 度求める。未確認の `copy` を `W1010` に入れる案は E15 の範囲を変えるので後続に回す | 既定案（実装者はこの案に従う） |
| D6 | `api-diff` は `copy` を足すのを互換、外すのを互換でない変更と報告する | 足しても今通るプログラムはすべて通る。外すと複製・捕捉が `E1012`・`E1005` になる | 既定案（実装者はこの案に従う） |
| D7 | 新しい診断は `E0002` の kind `extern_type_modifier_conflict` だけで、既存の文を変えない | 新しいコードを割り当てない（E12 D12、D-51）。修飾を使わないプログラムの診断を変えない | 既定案（実装者はこの案に従う） |
