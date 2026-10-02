# E12: FFI の拡張（リンク名・ホストのリンク指定・不透明ハンドル・コールバック）

| 項目 | 内容 |
| --- | --- |
| ID | E12 |
| 優先度 | P1 |
| 規模 | L |
| 依存 | E05, E06, (B07) |
| 後続 | E11, E13, E08, F13 |
| 状態 | done（Phase 1 の段 A–D） |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | D1（リンク名の構文 `extern "symbol" def`・`extern "module" "symbol" def`）, D3（`extern type` の意味）, D6（リンク入力の CLI `--link`・`-l`・`-L` と manifest `[native]`）, D8（静的コールバックを Phase 1 に含める）は 2026-10-02 に既定案どおり承認された |
| 改善する劣位 | C/C++ 比: 任意の外部 ABI・既存ライブラリとの接続の自由度が小さい（[なぜ Tsuzuri か](../../_docs/learn/why-tsuzuri.md#cc-に対する劣位点)） |
| 手本にする既存実装 | リンク名: `src/check.rs` の `HostImport` を作る loop（`external_symbols` の衝突検査）と `src/llvm_imports.rs` の `FunctionEmitter::host_call`（`declare` と WASM 属性）。CLI: `src/main.rs` の `--cpu`・`--wasm-feature` の解析と `src/driver.rs` の `BuildOptions`、実行ファイルのリンク（`clang.arg("-lm")` の箇所）。manifest: `src/package.rs` の `parse_manifest` の `[dependencies]` 節。ハンドル型: Copy でない葉の型 `Type::Task` の型性質（`Type::is_copy`）と `Type::is_noncopy_record`、header 名の決定的な符号化 `src/llvm_abi.rs` の `record_name`。コールバック: `src/llvm_abi.rs` の `wrapper`（公開 ABI の wrapper）と `c_parameters` |
| 主な影響ファイル | `src/syntax.rs`, `src/parser.rs`, `src/formatter.rs`, `src/docgen.rs`, `src/semantic.rs`, `src/check.rs`, `src/abi.rs`, `src/llvm.rs`, `src/llvm_abi.rs`, `src/llvm_imports.rs`, `src/llvm_debug.rs`, `src/driver.rs`, `src/main.rs`, `src/package.rs`, `tests/host_imports.rs`, `tests/ffi_extensions.rs`（新規）, `tests/ffi_extensions.mjs`（新規）, `tests/ffi_extensions_host.c`（新規）, `tests/fixtures/ffi_extensions/Main.tz`（新規）, `docs/language.md`, `docs/architecture.md`, `_docs/guides/native-interop.md`, `_docs/guides/webassembly.md`, `_docs/tools/command-line.md`, `_docs/feature-status.md`, `_features/README.md` |

## 目的

既存の C ABI 関数（`libm` の `sqrt`、C ライブラリの API）を shim なしで呼び、ホストの資源を型安全なハンドルとして受け渡し、
実行ファイルへホストの object・static library・system library を直接リンクできるようにする。さらに、捕捉のないトップレベル関数を
C の関数ポインターとしてホストへ渡せるようにする（qsort の比較関数、イベントの登録）。

Phase 1 は四つの段からなり、各段は単独で出荷できる。実装者は段 A → B → C → D の順に Phase 1 だけを実装する。Phase 2 は人間が求めた場合だけ着手する。

| 段 | 内容 | 承認 |
| --- | --- | --- |
| A | リンク名（`extern "symbol" def`、`extern "module" "symbol" def`） | D1 |
| B | リンク入力（`--link`・`-l`・`-L`、manifest `[native]`） | D6 |
| C | 不透明ハンドル型（`extern type Name`） | D3 |
| D | 静的コールバック（extern の関数型引数にトップレベル関数を渡す） | D8 |

## 着手条件と停止条件

### 着手条件

- E05・E06 が `_features/README.md` の状態欄で done であること（HEAD で done）。確認: `grep -nE "^\| (B07|E05|E06) " _features/README.md`。
- B07 は開始条件にしない。B07 がなくてもハンドルは動く（D3）。B07 の完了後も E12 の変更は要らない（利用者はハンドルを record で包んで `Drop` を書く）。
- 各段は対応する決定（上の表）が承認済みであること。承認前の段には着手しない。段 C は段 A に依存しない。段 D は段 C のハンドル型を
  コールバックの引数に使うので、段 C の後に行う。
- GUIDE §2.3 の基準コマンドが成功し、実装手順 1 のベースライン（既存 fixture の IR と import 一覧）を保存していること。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- extern の文字列を読むために lexer の文字列 token の形（UTF-16 の値、escape の扱い）を変える必要がある。
- `Type` を 4 語より大きくしないと `Type::Handle`（新規）を表せない。
- `BuildOptions` の `Copy`（または `Default` の構築箇所）を広く変えないとリンク入力を渡せない（D7 は別の struct で渡す）。
- リンク名を使わないプログラムで、既存の IR・header・WASM import 一覧・診断が 1 byte でも変わる。
- 既存テストの期待値（`tests/host_imports.rs`、`tests/host_imports.mjs`、`tests/host_abi.rs`、`src/main.rs` の CLI テスト）を変える必要がある。
- `llvm_abi::wrapper` を公開名 `tz_<name>` 以外の名前・internal linkage で出せず、コールバック用に wrapper を複製したくなった。
- ホストの別スレッドからコールバックを呼ぶために task runtime の thread 登録・thread local の初期化が要ると分かった（D10 の契約を変える）。
- build cache が link 入力の内容を考慮しない経路を消せない（D7）。
- stack-depth の 3 テスト（GUIDE §11.1 の一覧）が失敗する。または上限・stack サイズを上げたくなった。
- `unsafe`、新しい crate、既定の WASM import が必要になった。

## 現状（HEAD `f8dc655` で確認）

- 構文: `src/parser.rs` のトップレベル宣言 loop は `self.eat(&TokenKind::Extern)` の直後に `Export` を E1008
  （`extern declarations cannot be exported`）、次に `self.expect(&TokenKind::Def, "'def' after extern")` を要求し、`Parser::signature` の結果を
  `Program::externs: Vec<SignatureDecl>`（`src/syntax.rs`）へ積む。`extern` と `def` の間に何かを書く形はない。
- 名前: `src/check.rs` は全 module の `program.externs` を回り、`format!("tsuzuri_host_{}_{}", module.name.replace('.', "_"), external.name.text)`
  を native 名、`format!("{}.{}", module.name, external.name.text)` を WASM 名とする `HostImport { native_symbol, wasm_name }` を作る。
  native 名の重複は `duplicate(&external.name)`（E1001）、制約・named region は E1008
  （`extern declarations cannot have constraints or named regions`）。本体は `TypedExprKind::HostCall(import, _)` になる。
- IR: `src/llvm_imports.rs` の `FunctionEmitter::host_call` は `declare` を `self.intrinsics`（`BTreeSet<String>`）へ入れ、WASM では
  `"wasm-import-module"="tsuzuri" "wasm-import-name"="<Module>.<name>"` を付ける。bool・8/16-bit 整数は i32 へ zext/sext し、結果を trunc する。
  buffer は `ptr, i64`、スカラー record は一時 struct への `ptr`、所有結果は `host_result_slot` の out 引数。`header` はホスト側 prototype を出す。
- ABI 型: `Type::exportable`（`src/check.rs`）は i8–i64・u8–u64・f32・f64・bool だけ。`src/abi.rs` の `parameter`・`result`・`scalar_record`
  がその上に buffer とスカラー record を足す。`scalar_record` の field 判定も `exportable` を使う。関数値・Task・i128・f16 は E1008。
- 公開 ABI: `src/llvm_abi.rs` の `wrapper` が `export def` の C ABI 関数を作り、`c_parameters` が header の引数列を作る。`record_name` は
  module の区切りと型引数を長さ付きで符号化する（`tz_record_<len><segment>..._T<len>_<hex>`）。
- 型: `src/check.rs` の `Type` はハンドル・ポインターを持たない（コメントのとおり 4 語）。Copy でない std 型は `Type::is_noncopy_record`
  （`Seq.Seq`・`Gpu.Device`・`Gpu.Buffer`）で表す。
- リンク: `src/driver.rs` は `Emit::Executable` で `clang.arg("-lm")` と runtime object を足すだけで、利用者のリンク入力を受ける field は
  `BuildOptions` にない。`src/main.rs` は未知の option を E2000 にする。`src/package.rs` の `parse_manifest` は `[package]` と任意の
  `[dependencies]` だけを受け、それ以外は `expected [package] followed by optional [dependencies], each once`（E0002）。
- テスト: `tests/host_imports.rs` の 4 テスト（`parses_extern_signatures_without_bodies` など）、`tests/host_imports.mjs`（IR の `@malloc`・
  `@free` を追跡関数へ置換した native `-O0`/`-O3` と、import の module が `tsuzuri` であることを確かめる WASM）、`tests/host_imports_runtime.c`
  （`live` の原子的な計数）、`tests/host_abi.rs`。
- 文書: `docs/language.md` の「ホスト関数のインポート」は「未解決hostを含むexe/runはlink error E2002で、host object指定CLIはありません」
  「制約・named region・可変参照・callback/Task/list等は受け取りません」と書く。

### 再現（検証済み）

`tests/fixtures/host_imports` の写しに 1 行足すと、三つの新構文はどれも `extern` の直後で止まる（列 8）。`--link` は未知の option。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p /tmp/tz-work-E12 && rm -rf /tmp/tz-work-E12/handle && cp -R tests/fixtures/host_imports /tmp/tz-work-E12/handle
printf '\nextern type FileHandle\n' >> /tmp/tz-work-E12/handle/Main.tz
target/release/tsuzuri check /tmp/tz-work-E12/handle
# error[E0002]: expected 'def' after extern（extern "sqrt" def ... と extern "env" "now" def ... も同じ）
target/release/tsuzuri build tests/fixtures/host_imports --link x.o
# error[E2000]: unknown option '--link'; use --help
```

## 仕様

### 前提とする他チケットのインターフェース

- E05（done）: buffer・スカラー record の ABI（`src/abi.rs` の `parameter`・`result`・`scalar_record`、`tsuzuri_alloc`／`tsuzuri_free`）。
  E12 はこの集合にハンドルだけを足し、record の field にはハンドルを許さない。
- E06（done）: `HostImport` と `TypedExprKind::HostCall`。extern は本体が `HostCall` の関数として型付けされ、関数値としても使える
  （`tests/host_imports.rs` の `fn indirect() -> i64 { apply now }`）。
- B07（todo、開始条件ではない）: B07 D1 は `Drop` の instance を source で宣言した record・union に限る。ハンドルを自動で閉じたい利用者は
  `record File { handle: FileHandle }` に `instance Drop<File>` を書き、`drop` から `ref FileHandle` を受ける close を呼ぶ。E12 は B07 の型性質に触れない。
- E11・E13・PR08（後続）: E11 は `extern "symbol" def` と `extern type` を生成し、E13 はハンドルを JS の number として扱う。PR08 は `--link` に
  bitcode を渡す拡張を足してよい（E12 は path をそのまま clang へ渡す）。E14 がトラップ境界を定めるまで、コールバック中の trap は D10 に従う。

### 構文

新構文（実装後に有効。未検証）。`string` は既存の文字列 literal token（`TokenKind::String`）。

```text
extern_decl = [ "private" ] "extern" [ link_name ] "def" ident "::" signature
link_name   = string            (* native symbol。WASM は module "tsuzuri"、name = symbol *)
            | string string     (* WASM import module, symbol。native は symbol だけを使う *)
handle_decl = [ "private" ] "extern" "type" ident
```

- 文字列は escape を解いた値で扱う。UTF-16 literal は `String::from_utf16` で戻し、失敗は E0002（`Parser::test_declaration` と同じ扱い）。
- `extern type` は型引数・`=`・本体・link name を取らない。doc comment は付けられる。`export` とは併用できない（既存の E1008）。

### 型規則

- 段 A: link name のない extern は E06 の名前のまま（IR・header・import は 1 byte も変えない）。symbol は C 識別子
  `[A-Za-z_][A-Za-z0-9_]*`、1–255 bytes。`tz_`・`tsuzuri`・`__` で始まる名前と、生成 IR 自身が宣言する名前（`RESERVED_HOST_SYMBOLS`（新規）、
  `src/abi.rs`）は E1008。WASM module は `[A-Za-z0-9_.-]`、1–255 bytes、`tsuzuri_` で始まらない（`tsuzuri` そのものは許す）。
- 段 A: 同じ symbol を複数の extern（別 module を含む）で宣言してよい。関数型（`Signature::as_type`）と WASM module が一致しなければ E1008。
  一致すれば `declare` は同じ文字列になり `BTreeSet` で一つになる。
- 段 C: `extern type Counter` は module `Main` に型 `Type::Handle("Main.Counter")`（新規）を作る。名前・修飾・`private`・重複（E1001）の規則は record と同じ。
  値を作る式はなく、extern の結果（または `export def` の引数）からだけ得る。Copy でも clone 可能でもなく、drop glue を持たない。
  `Eq`・`Display` などの instance はない（利用者も書けない。既存の E1016）。
- 段 C: ハンドルを使える ABI の位置は、extern と `export def` の引数（値 `H`・共有参照 `ref H`）と結果（`H`）。record の field（スカラー record）、
  buffer の要素、可変参照 `ref mut H` は ABI では不可（既存の E1008 メッセージ）。ABI 以外では通常の Copy でない値として record・union・
  配列・`Option` に入れてよい。
- 段 D: extern の引数に関数型（コールバック）を書ける。コールバックの引数は `exportable` なスカラー・`H`・`ref H`（unit は唯一の引数のときだけで、
  ABI では省略）、結果はスカラーか unit。関数型の結果・入れ子の関数型・buffer・record は E1008。
- 段 D: 関数型の引数を持つ extern（コールバック extern）は、直接・全引数で呼ぶ場合だけ使える。関数値としての使用・部分適用は E1008。
  コールバックの位置の実引数は、生成でも std でも extern でもない、型引数のないトップレベルの利用者関数の名前（`TypedExprKind::Function(FunctionRef::User(id))`）
  だけで、ラムダ・部分適用・局所変数・捕捉のある関数は E1008。

### 評価順序・所有権・借用

- 引数は E06 と同じく左から右に一度だけ評価する。コールバックの位置は関数名なので副作用も確保もない（`%tz.closure` を作らない）。
- `H` の値渡しは move（その後の使用は E1012）。`ref H` は共有借用で、ホストへはハンドルの値そのもの（pointer 幅）を渡す。
- ハンドルは `Type::can_capture` を false にする（関数値の複製がハンドルを複製するため。捕捉は既存の E1005）。`Type::can_send` は true
  （Task へ move できる。スレッド安全性は docs/language.md の既存の規則どおりホストの責任）。
- ハンドルが scope を抜けても何も呼ばない（drop glue なし）。解放は利用者が close の extern へ値で渡して行う。閉じ忘れはホスト資源の leak で、
  Tsuzuri の `live` 計数には現れない（D3）。

### 数値・トラップ・native と WASM の差

| 項目 | native | wasm32 |
| --- | --- | --- |
| link name | `declare ... @<symbol>`。Mach-O の `_` はリンカーが付ける | `"wasm-import-module"="<module>" "wasm-import-name"="<symbol>"` |
| ハンドル | LLVM `ptr`、C は `tz_handle_...` typedef | LLVM `ptr`（i32）。JS には number |
| コールバック | C 関数ポインター | 関数 table の index（i32）。`--export-table` を足し、JS は `instance.exports.__indirect_function_table.get(i)` |
| リンク入力 | exe の clang 起動へ追加 | 指定すると E2000。manifest の `[native]` は無視 |

- extern を書かないプログラム・link name を使わないプログラムの WASM import は従来どおり（D-18）。import は利用者の extern だけから生じる。
- コールバック中の trap は通常の trap と同じ（native は process の異常終了、WASM は `WebAssembly.RuntimeError` がホストのフレームを通って
  export の呼び出し元へ伝わる）。trap 後にホストが instance を使い続けてはいけない（E14 まで）。

### 診断

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| E0002 | `extern` の後が文字列・`def`・`type` でない | `expected 'def' after extern`（既存のまま） | その token |
| E0002 | 文字列が 3 つ以上 | `extern takes at most two link name strings; write extern "symbol" def or extern "module" "symbol" def` | 3 つ目の文字列 |
| E0002 | `extern "..." type` | `extern type declarations take no link name; remove the string` | 文字列 |
| E0002 | `extern type T<...>`・`=` が続く | `extern type declarations have no type parameters or definition` | `<` か `=` |
| E0002 | 文字列が Unicode でない | `extern link names must contain valid Unicode scalars` | 文字列 |
| E1008 | symbol が C 識別子でない・長すぎる | `invalid extern symbol '{symbol}'; use a C identifier of at most 255 bytes` | symbol の文字列 |
| E1008 | 予約された symbol | `extern symbol '{symbol}' is reserved by Tsuzuri; choose a name that does not start with 'tz_', 'tsuzuri' or '__'` | symbol の文字列 |
| E1008 | WASM module が不正 | `invalid WASM import module '{module}'; use 1 to 255 ASCII letters, digits, '_', '-' or '.' not starting with 'tsuzuri_'` | module の文字列 |
| E1008 | 同じ symbol で型か module が違う | `extern symbol '{symbol}' is declared with different signatures or import modules; declare it once and call it from other modules` | 後の宣言の名前 |
| E1008 | コールバックの型が不正 | `extern callback parameters support functions over scalar, unit and extern handle types; buffers, records and nested functions are not supported` | extern の名前 |
| E1008 | コールバック extern を関数値・部分適用で使う | `extern '{name}' takes a callback and must be called directly with all arguments` | その式 |
| E1008 | コールバックの実引数が関数名でない | `callback arguments must name a top-level user function without captures; move the logic into a 'def' and pass its name` | 実引数 |
| E1001 | `extern type` の名前が型名と重複 | 既存の `duplicate` のメッセージ | 名前 |
| E1005 | ハンドルを関数値に捕捉 | 既存（`can_capture` が false） | 既存 |
| E1012 | move 後のハンドルの使用 | 既存 | 既存 |
| E2000 | リンク入力を wasm32・`--emit` が exe 以外・`check`/`fmt`/`doc` で指定 | `link inputs require a native executable; remove --link, -l and -L or build the native target with --emit exe` | コマンド行 |
| E2000 | `-l` の名前が不正 | `invalid library name '{name}'; pass the name without the 'lib' prefix or extension, for example -l sqlite3` | コマンド行 |
| E2000 | 同じ入力の重複・256 超 | `link input '{input}' specified more than once` / `too many link inputs; at most 256 are supported` | コマンド行 |
| E2000 | 依存 package の `[native]` | `only the root package may declare [native] link settings; move them to the application manifest` | `[native]` の行 |
| E0002 | manifest の `[native]` の形が不正 | `expected [native] keys link, libraries or search, each an array of strings on one line` | その行 |
| E2001 | `--link` の file・`-L` の directory がない | `cannot read link input '{path}': {error}` | コマンド行 |
| E2003 | 出力 path がリンク入力と同じ | `output '{path}' would overwrite a link input; choose a different -o path` | コマンド行 |
| E2002 | 未解決 symbol | 既存（リンカーの出力を含む） | コマンド行 |

### 資源上限

- symbol・module は 255 bytes。リンク入力は CLI と manifest の合計 256 個。コールバックの引数は `export def` と同じ既存の上限に従う。新しい上限の
  超過は上の表のとおり E1008・E2000（解析の資源ではないので E1017 は使わない）。

### 例

新構文（実装後に有効。未検証）。受理される宣言:

```tsuzuri
extern "sqrt" def c_sqrt :: f64 -> f64
extern "env" "host_now" def host_now :: unit -> i64
extern type Counter
extern "counter_new" def counter_new :: i64 -> Counter
extern "counter_add" def counter_add :: ref Counter -> i64 -> i64
extern "counter_free" def counter_free :: Counter -> unit
extern "apply_twice" def apply_twice :: (i64 -> i64) -> i64 -> i64
```

拒否される宣言と期待するコード:

```tsuzuri
extern "tz_add" def bad1 :: i64 -> i64               -- E1008 reserved
extern "2x" def bad2 :: i64 -> i64                   -- E1008 invalid symbol
extern "tsuzuri_io" "f" def bad3 :: i64              -- E1008 invalid module
extern "a" "b" "c" def bad4 :: i64                   -- E0002
extern "x" type Bad5                                 -- E0002
extern "take" def bad6 :: ref mut Counter -> unit    -- E1008 (ABI)
extern "cb" def bad7 :: (i64 -> [i64]) -> unit       -- E1008 callback
```

## 設計

### データ構造

```rust
// src/syntax.rs
pub struct LinkName { pub module: Option<String>, pub symbol: String, pub span: Span }       // （新規）symbol の span は最後の文字列
pub struct ExternTypeDecl { pub doc: Option<Documentation>, pub visibility: Visibility, pub name: Ident } // （新規）
// SignatureDecl に `pub link: Option<Box<LinkName>>`、Program に `pub extern_types: Vec<ExternTypeDecl>` を足す

// src/check.rs
pub enum Type { /* 既存 */ Handle(Box<str>) }  // 修飾名。Box<str> は 2 語で Type は 4 語のまま
pub struct HostImport {
    pub native_symbol: String,
    pub wasm_module: String,     // （新規）既定は "tsuzuri"
    pub wasm_name: String,
    pub explicit: bool,          // （新規）link name あり。header に prototype を出さない
    pub callbacks: bool,         // （新規）関数型の引数がある
}
// CheckedModule に `pub callbacks: BTreeSet<usize>`（新規）: コールバックとして渡される利用者関数の id

// src/driver.rs（BuildOptions は Copy のまま。別の値で渡す）
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinkInputs { pub paths: Vec<PathBuf>, pub libraries: Vec<String>, pub search: Vec<PathBuf> } // （新規）
// src/package.rs の Manifest に `pub native: Option<(LinkInputs, Span)>`（新規）
```

### 段ごとの変更

GUIDE §6.3（`Type` の variant）のチェックリストを HEAD の match に当てはめた一覧を含む。fallback が正しい箇所は「変更なし」と書く。

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| A | `src/syntax.rs` | `SignatureDecl`, `LinkName` | `link` field。`Parser::signature` の構築は `link: None` |
| A | `src/parser.rs` | トップレベル loop の `TokenKind::Extern` 分岐 | `Extern` の後で文字列を 0–2 個読む（3 個目は E0002）。`signature.link` に入れる |
| A | `src/formatter.rs` | extern を出す箇所（`program.externs` の loop と `TokenKind::Extern` の分岐） | `extern "m" "s" def` の順で出す。文字列は元の literal の綴りを保つ |
| A | `src/check.rs` | `HostImport` を作る loop | link name があれば検査（D2）し `native_symbol = symbol`、`wasm_module`、`wasm_name = symbol`、`explicit = true` |
| A | `src/check.rs` | extern の ABI 検査（`external_functions.contains_key(&id)` の分岐） | 明示 symbol → `(signature.as_type(), wasm_module)` の表で不一致を E1008 |
| A | `src/abi.rs` | `RESERVED_HOST_SYMBOLS`（新規）, `link_symbol_error`（新規） | 予約名と C 識別子の検査。一覧は手順 2 の grep で作る |
| A | `src/llvm_imports.rs` | `FunctionEmitter::host_call`, `header` | WASM 属性の module を `import.wasm_module` に。`header` は `explicit` の import を飛ばす |
| B | `src/main.rs` | option の解析 loop | `--link`・`-l`・`-L`（繰り返し可）。`check`・`fmt`・`doc` での拒否は `--cpu` と同じ一覧に足す |
| B | `src/driver.rs` | `LinkInputs`, `build`, `run`, test の build 経路 | `&LinkInputs` を引数で渡す。exe の clang 起動（`clang.arg("-lm")` の経路と `dwarf_sidecar` の linker）の最後に `-L`・path・`-l` を足す |
| B | `src/driver.rs` | cache を開く箇所（`crate::cache::build_key`）, `protect_sources` | 入力があれば cache を使わない（D7）。出力と入力が同じなら E2003 |
| B | `src/package.rs` | `Manifest`, `parse_manifest` | `[native]` 節（`[dependencies]` の後、一回）。`link`・`libraries`・`search` の 1 行配列 |
| C | `src/syntax.rs`・`src/parser.rs` | `ExternTypeDecl`, `Program::extern_types` | `extern type Name` |
| C | `src/formatter.rs`・`src/docgen.rs`・`src/semantic.rs` | extern の loop | `extern type Name` の整形、文書の型一覧、LSP の型 symbol |
| C | `src/check.rs` | `Names`（`records: BTreeMap<String, NameInfo>` の隣）, `NamedType`, `named_type` | `handles`（新規）と `NamedType::Handle`（新規）。record と同じ探索・可視性。重複は `duplicate` |
| C | `src/check.rs` | `resolve_type_with_kinds` の `TypeExprKind::Named` | `NamedType::Handle(info) => Type::Handle(info.name.as_str().into())`。型引数付きは既存の arity 検査（E1004/E1015） |
| C | `src/check.rs` | `Type::display` | 修飾名を record と同じ形で表示 |
| C | `src/check.rs` | `Type::is_copy`, `Type::can_capture` | `Self::Handle(_)` を false（`can_capture` は `stored_all` の closure と match の両方） |
| C | `src/check.rs` | `needs_drop`, `can_send`, `exportable`, `contains_*`, `carries_loans` | 変更なし（fallback が正しい。`exportable` を変えると `scalar_record` がハンドルを受けてしまう） |
| C | `src/check.rs` | 値サイズの match（`Type::Function(..) \| Type::Task(_) => 32` を返す関数） | `Type::Handle(_) => 8` |
| C | `src/abi.rs` | `abi_scalar`（新規）, `parameter`, `result` | `exportable()`・`H`・`ref H`。`parameter`・`result` はこれを使う。`scalar_record` は変えない |
| C | `src/llvm_abi.rs` | `extended`, `uses_host_abi`, `c_parameters`, `wrapper` | `exportable()` を `abi_scalar` に。ハンドルは `ptr %argN`、`ref H` は `spill` した slot を渡す |
| C | `src/llvm.rs` | `llvm_type`, `canonical_type`, `storage_layout`, `c_type`, `abi_type`, `header` | `ptr`、`extern.{name}`、`Type::Reference(..)` と同じ layout、`handle_c_name`（新規）、`ptr`、typedef を prototype の前に |
| C | `src/llvm.rs` | `drop_value`, `clone_value` | drop は変更なし（呼ばれない）。clone は `Type::Task` と同じく `unreachable!("extern handles cannot be cloned")` |
| C | `src/llvm_debug.rs` | 大きさの match（`Type::Reference(..) => (pointer, pointer)`） | `Type::Handle(_)` を同じ arm に。型名は修飾名 |
| C | `src/llvm_frame.rs`, `src/polymorph.rs`, `src/warnings.rs`, `src/ownership.rs` | `Type::Task` を扱う match | 変更なし（葉として fallback が正しい。`polymorph::type_expression` は display 名から Named を作る fallback） |
| C | `src/llvm_imports.rs` | `host_call` | `H` は値をそのまま `ptr`、`ref H` は `load ptr, ptr` して `ptr`。結果 `H` は `ptr` をそのまま返す |
| D | `src/check.rs` | extern の ABI 検査, `validate_callbacks`（新規） | 関数型の引数の検査。型付け後に `TypedExpr::children` で全関数本体を辿り、D の規則を検査して `CheckedModule::callbacks` を埋める |
| D | `src/llvm.rs` | `reachable_functions` の結果を使う関数の emit loop, `FunctionEmitter::call` | コールバック extern の関数本体は出さない。`FunctionRef::User(id)` の callee が `import.callbacks` なら `self.host_call(import, arguments, result)` を直接出す |
| D | `src/llvm_imports.rs` | `host_call`, `header` | 関数型の引数は評価せず `ptr @tz.callback.<suffix>` を渡す。prototype は `int64_t (*arg0)(int64_t)` |
| D | `src/llvm_abi.rs` | `wrapper` | 出力名と linkage を引数にする。export は `tz_<name>`・external、コールバックは `tz.callback.<suffix>`（`@tz.fn.` と同じ接尾辞）・internal |
| D | `src/driver.rs` | wasm-ld の引数（`--export=tz_{}` の loop の前） | `module.callbacks` が空でなければ `--export-table` |

### 生成 IR とランタイム

段 A（link name `sqrt`、wasm32 の module 既定）:

```llvm
declare double @sqrt(double) "wasm-import-module"="tsuzuri" "wasm-import-name"="sqrt"
```

段 C と D（native。`ref Counter` は slot から読んでから渡す）:

```llvm
declare ptr @counter_new(i64)
declare i64 @counter_add(ptr, i64)
declare i64 @apply_twice(ptr, i64)
  %h = load ptr, ptr %slot
  %r = call i64 @counter_add(ptr %h, i64 5)
  %t = call i64 @apply_twice(ptr @tz.callback.Main.inc, i64 %x)
define internal i64 @tz.callback.Main.inc(i64 %arg0) nounwind { ... }
```

header（C。typedef は handle 名の昇順で prototype の前に一度だけ。`explicit` の import には prototype を出さない）:

```c
typedef struct tz_handle_4Main_7Counter_s *tz_handle_4Main_7Counter;
tz_handle_4Main_7Counter tz_make_counter(int64_t arg0);
```

- `handle_c_name`（新規）は `record_name` と同じ長さ付き符号化で `tz_handle` に続けて module の各区切りと名前を `_<len><segment>` で並べる。
- runtime・`%tz.*` のヘッダー型・`@tz.alloc` は変えない。ハンドルは確保しない。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3）。段の承認がなければ、その段の手順の前で止める。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、E06 fixture の IR・header を保存する。
- 確認: 次がすべて成功する。`host_imports` と `host_abi` は各 `4 passed`、mjs は `host imports: native/WASM -O0 passed` と `-O3` の 2 行。
  wasm32 の IR は `"wasm-import-module"="tsuzuri"` を 10 行含む（HEAD で確認済み）。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
mkdir -p /tmp/tz-e12
target/release/tsuzuri build tests/fixtures/host_imports --emit llvm -o /tmp/tz-e12/before.ll
target/release/tsuzuri build tests/fixtures/host_imports --target wasm32 --emit llvm -o /tmp/tz-e12/before-wasm.ll
target/release/tsuzuri build tests/fixtures/host_imports --emit header -o /tmp/tz-e12/before.h
cargo test --locked --test host_imports
cargo test --locked --test host_abi
node tests/host_imports.mjs target/release/tsuzuri
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
```

### 手順 2: link name の構文（段 A）

- 変更: `src/syntax.rs`（`LinkName`、`SignatureDecl::link`）、`src/parser.rs`（`Parser::link_name`（新規）と extern 分岐）、`src/formatter.rs`、
  `tests/ffi_extensions.rs`（新規）。
- 内容: `Parser::link_name` は文字列 token を最大 2 個読むだけの非再帰 helper（stack 深さに影響しない）。`Parser::signature` と class method の
  `SignatureDecl` 構築は `link: None`。整形は literal の綴り（UTF-8 literal の接頭辞を含む）を保つ。
- 確認: `cargo test --locked --test ffi_extensions` が `1 passed`（`parses_link_names`）。`cargo test --locked --test host_imports` が `4 passed`、
  `cargo test --locked --test formatter` が成功する。

### 手順 3: link name の検査と IR（段 A）

- 変更: `src/check.rs`（`HostImport` の 3 field、ABI 検査の分岐）、`src/abi.rs`（`RESERVED_HOST_SYMBOLS`・`link_symbol_error`）、`src/llvm_imports.rs`。
- 内容: 予約一覧は次の出力（生成 IR が自分で宣言する C 名）に `malloc`・`free`・`realloc`（`tests/host_imports.mjs` が IR 中で置換する名前）を
  足して昇順の定数にする。HEAD では少なくとも `write`・`_write`・`_setmode` が出る。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
grep -ohE 'declare [^@"]*@[A-Za-z_][A-Za-z0-9_]*\(' src/*.rs src/runtime/*.ll | grep -oE '@[A-Za-z_][A-Za-z0-9_]*' | sort -u | grep -vE '^@(tz|tsuzuri)'
```

- 確認: `cargo test --locked --test ffi_extensions` が `3 passed`。`cargo build --release --locked` の後、手順 1 の 3 ファイルを同じコマンドで
  `/tmp/tz-e12/after*.ll`・`after.h` に出し、`cmp /tmp/tz-e12/before.ll /tmp/tz-e12/after.ll` など 3 組がすべて一致する。

### 手順 4: リンク入力の CLI（段 B）

- 変更: `src/main.rs`（解析と command ごとの拒否）、`src/driver.rs`（`LinkInputs`、`build`・`run`・test の経路、exe の link、cache、`protect_sources`）。
- 内容: 入力は `-L`（順序どおり）、`--link` の path、`-l` の順で、既存の引数（runtime object・`-pthread` を含む）の後に足す。`dwarf_sidecar` の
  linker にも同じ引数を足す。path は存在を検査し（E2001）、正規化はしない。入力が一つでもあれば cache を開かない。
- 確認: `cargo test --locked --bin tsuzuri link_inputs` が `2 passed`（`parses_link_inputs`・`rejects_link_inputs_outside_native_executables`、
  `src/main.rs` の tests module に追加）。`cargo test --locked --bin tsuzuri` の既存テストが成功する。

### 手順 5: manifest の `[native]`（段 B）

- 変更: `src/package.rs`（`Manifest::native`、`parse_manifest`、依存 package を読む箇所）、`src/driver.rs`（manifest の入力を CLI の前に連結）。
- 内容: path は package root からの相対 path だけ（`[dependencies]` の `path` と同じく絶対 path は E0002）。wasm32 と exe 以外の出力では無視する。
  依存 package の manifest に `[native]` があれば E2000。
- 確認: `cargo test --locked --lib native_link_section` が `2 passed`（`parses_native_link_section`・`rejects_malformed_native_link_section`）。

### 手順 6: `Type::Handle` と `extern type`（段 C、共有変更）

- 変更: 「段ごとの変更」の段 C のうち ABI・IR 以外の行（構文、整形、文書生成、LSP、`Names`、`NamedType`、`resolve_type_with_kinds`、`Type` の型性質、値サイズ、
  `llvm_type`・`canonical_type`・`storage_layout`・`clone_value`・`llvm_debug`）。
- 内容: variant を足したら `cargo build` の非網羅 match の error をすべて直し、`_ =>` の fallback は表のとおり確認だけする。
- 確認: `cargo test --locked --test ffi_extensions` が `6 passed`。手順 1 の stack-depth 3 テストが成功する。`cargo test --locked` が成功する。

### 手順 7: ハンドルの ABI と header（段 C）

- 変更: `src/abi.rs`（`abi_scalar`、`parameter`、`result`）、`src/llvm_abi.rs`、`src/llvm.rs`（`c_type`、`abi_type`、`header`、`handle_c_name`）、`src/llvm_imports.rs`。
- 確認: `cargo test --locked --test ffi_extensions` が `8 passed`。`cargo test --locked --test host_abi` が `4 passed`。手順 3 の `cmp` 3 組が一致する。

### 手順 8: 静的コールバック（段 D）

- 変更: `src/check.rs`（コールバック型の検査、`validate_callbacks`、`CheckedModule::callbacks`）、`src/llvm.rs`（emit loop の skip、`FunctionEmitter::call`）、
  `src/llvm_imports.rs`、`src/llvm_abi.rs`（`wrapper` の名前と linkage）、`src/driver.rs`（`--export-table`）。
- 内容: `validate_callbacks` は `TypedExpr::children` で再帰し、コールバック extern を callee とする `Call` では callee を辿らず引数だけを辿る。
  それ以外で `FunctionRef::User(id)` がコールバック extern なら E1008。wrapper は `module.callbacks` の昇順に export の wrapper の後で出す。
- 確認: `cargo test --locked --test ffi_extensions` が `10 passed`。`cargo test --locked` が成功する。

### 手順 9: E2E

- 変更: `tests/fixtures/ffi_extensions/Main.tz`、`tests/ffi_extensions_host.c`、`tests/ffi_extensions.mjs`（すべて新規）。
- 確認: `cargo build --release --locked && node tests/ffi_extensions.mjs target/release/tsuzuri` が `ffi extensions: native -O0 passed` など 5 行
  （native `-O0`・`-O3`、run、WASM `-O0`・`-O3`）を出して終了コード 0。

### 手順 10: 文書と最終確認

- 変更: 「ドキュメント」の各ファイル。
- 確認: `node scripts/check-docs.mjs docs/language.md _docs/guides/native-interop.md _docs/guides/webassembly.md _docs/tools/command-line.md` が成功する。
  `cargo fmt --check`、`cargo test --locked`、`node tests/host_imports.mjs target/release/tsuzuri`、`node tests/ffi_extensions.mjs target/release/tsuzuri`、
  手順 3 の `cmp` がすべて成功する。GUIDE §10 を満たす。

## テスト計画

### Rust テスト

`tests/ffi_extensions.rs`（新規）。IR を出す helper は `tests/host_imports.rs` の
`scalar_imports_use_effectful_abi_wrappers_and_prune_unused_declarations` の書き方を写し、各 IR を 2 回出して一致も確かめる。

| 手順 | テスト | 確かめること |
| --- | --- | --- |
| 2 | `parses_link_names` | 1 個・2 個の文字列が `LinkName` に入る。3 個、`extern "x" type T`、`extern "x" export def` が E0002/E1008 |
| 3 | `link_names_lower_to_symbols_and_wasm_modules` | native IR に `declare double @sqrt(double)`、wasm32 IR に `"wasm-import-module"="env" "wasm-import-name"="host_now"`。header に `sqrt(` がない。別 module の同じ宣言で `declare` が 1 行 |
| 3 | `link_names_reject_reserved_invalid_and_conflicting` | `tz_add`・`tsuzuri_x`・`__x`・`malloc`・`write`・`2x`・256 bytes の symbol、`tsuzuri_io` module、型か module が違う同じ symbol がすべて E1008 |
| 6 | `parses_extern_types` | `extern type T`・`private extern type T` を受理し、`extern type T<A>`・`extern type T = i64` が E0002 |
| 6 | `extern_types_are_noncopy_leaf_types` | move 後の使用 E1012、関数値への捕捉 E1005、record と同名 E1001、record field・`Option` への格納は受理 |
| 6 | `extern_types_resolve_across_modules` | `analyze_modules` で `Io.Counter` を別 module から使え、`private extern type` は E1022 |
| 7 | `handles_lower_to_pointers_and_header_typedefs` | `declare ptr @counter_new(i64)`、`ref Counter` の `load ptr, ptr`、header の typedef 1 行、`export def` の `ptr` 引数・結果 |
| 7 | `handles_are_rejected_inside_abi_records` | ハンドルを field に持つ record の export・extern が E1008、`ref mut Counter` が E1008 |
| 8 | `callbacks_lower_to_internal_c_wrappers` | `define internal i64 @tz.callback.Main.inc(i64`、`call i64 @apply_twice(ptr @tz.callback.Main.inc`、コールバック extern の `@tz.fn.` 定義がない、header の `(*arg0)` |
| 8 | `callbacks_reject_values_lambdas_and_bad_types` | extern を関数値・部分適用で使う、ラムダ・局所変数・std 関数・extern を渡す、`(i64 -> [i64])` がすべて E1008 |

### E2E

`tests/ffi_extensions.mjs`（新規、形は `tests/host_imports.mjs` を写す。fixture は `mkdtempSync` の root へ写してから使う）。fixture の中身は
新構文（実装後に有効。未検証）。呼び出し・借用 `(&x)`・`assert` の書き方は `tests/fixtures/host_imports/Main.tz` と同じ。

```tsuzuri
extern type Counter
extern "sqrt" def c_sqrt :: f64 -> f64
extern "env" "e12_host_now" def host_now :: unit -> i64
extern "e12_counter_new" def counter_new :: i64 -> Counter
extern "e12_counter_add" def counter_add :: ref Counter -> i64 -> i64
extern "e12_counter_free" def counter_free :: Counter -> i64
extern "e12_add_one" def add_one :: i64 -> i64
extern "e12_apply_twice" def apply_twice :: (i64 -> i64) -> i64 -> i64

private def triple :: i64 -> i64
fn triple value = value * 3

private def via_host :: i64 -> i64
fn via_host value = add_one value

private def boom :: i64 -> i64
fn boom value =
    assert (value < 0)
    value

export def counters :: i64 -> i64
fn counters start =
    let counter = counter_new start
    let first = counter_add (&counter) 5
    let second = counter_add (&counter) 7
    first + second + counter_free counter

export def callbacks :: i64 -> i64
fn callbacks value = apply_twice triple value + apply_twice via_host 1

export def callback_trap :: i64 -> i64
fn callback_trap value = apply_twice boom value

export def misc :: f64 -> i64
fn misc value = if c_sqrt value == 1.5 then host_now () else 0

export def pass_through :: Counter -> Counter
fn pass_through counter = counter
```

期待値は C の参照実装どおりに手で計算した値で、コンパイラの出力から取らない。counter は `new(s)` が値 s、`add(c, n)` が加えた後の値、
`free(c)` が最後の値を返して解放する。

| export | 入力 | 期待 | 計算 |
| --- | --- | --- | --- |
| `counters` | 10 | 59 | 15 + 22 + 22 |
| `callbacks` | 4 | 39 | triple(triple(4)) = 36、via_host 二回で 1 → 3 |
| `misc` | 2.25 | 40 | sqrt(2.25) = 1.5、host_now は 40 を返す |
| `pass_through` | ホストのハンドル | 同じ pointer（JS は同じ number） | 恒等 |
| `callback_trap` | 1 | trap | assert の失敗 |

- native（`-O0`・`-O3`）: `--emit llvm` の IR の `@malloc`・`@free`・`@realloc` を追跡関数へ置換し（`tests/host_imports.mjs` と同じ）、
  `tests/ffi_extensions_host.c` を `-DE12_HOST_MAIN` で `main` 付きにして clang で連結する。`main` は上の表を検査し、各呼び出しの後で Tsuzuri の
  `live == 0` と生きている counter 数 0 を assert する。`callback_trap` は argv の mode で別プロセスとして実行し、終了状態が 0 でないこと。
  `e12_host_now` は native では module を無視した同名の C 関数。
- run: fixture を写した root に 1 行 `counters 10 + callbacks 4` を足し、`clang -c tests/ffi_extensions_host.c`（`main` なし）の object で
  `tsuzuri run <root> --link host.o` が stdout `98` と終了コード 0。同じ object から `ar rcs libe12host.a` を作り、`-L <dir> -l e12host` と、
  manifest の `[native] link = ["host.o"]` でも `98`。依存 package に `[native]` を置くと E2000。`--target wasm32 --link host.o` は E2000。
- WASM（`-O0`・`-O3`）: `WebAssembly.Module.imports` が module `tsuzuri` の `sqrt`・`e12_counter_new`・`e12_counter_add`・`e12_counter_free`・
  `e12_add_one`・`e12_apply_twice` と module `env` の `e12_host_now` に一致する。JS ホストはハンドルを 1 から振る number と `Map` で管理し、
  `e12_apply_twice` は `instance.exports.__indirect_function_table.get(index)` を BigInt で二回呼ぶ。上の表を検査し、Map が空に戻ること、
  `callback_trap` が `WebAssembly.RuntimeError` を投げることを確かめる。`tests/fixtures/host_imports` の wasm32 出力の exports に
  `__indirect_function_table` がないことも確かめる。

### 既存テストへの影響

なし。`tests/host_imports.rs`・`tests/host_imports.mjs`・`tests/host_abi.rs`・`src/main.rs` と `src/package.rs` の既存テストは期待値を変えずに成功する。

### 性能

計測しない。link name の呼び出しは E06 と同じ直接呼び出しで、ハンドルは pointer 一つ、コールバックは wrapper 一段。

## ドキュメント

- `docs/language.md`「ホスト関数のインポート」: link name、`extern type`、コールバック、リンク入力。「host object指定CLIはありません」と
  「callback/Task/list等は受け取りません」の文を新しい規則に置き換える。「公開 ABI」: ハンドルの引数・結果。
- `docs/architecture.md`「不変条件」: import は利用者の extern からだけ生じること、リンク入力は native exe だけ、`--export-table` の条件。
- `_docs/guides/native-interop.md`「ホスト関数をインポートする」「対応型」「非対応の境界型」: link name・ハンドル・コールバック・`--link` の例。
- `_docs/guides/webassembly.md`「extern import」: module 指定、ハンドルは number、関数 table 経由のコールバック。
- `_docs/tools/command-line.md`「build と run の主なオプション」「出力保護と終了コード」: `--link`・`-l`・`-L` と E2000・E2001・E2003。
- `_docs/feature-status.md` と `_features/README.md` の E12 の状態。

## 受け入れ条件

- [ ] `extern "sqrt" def` で libm の `sqrt` を shim なしで呼べ、`extern "module" "symbol" def` の WASM import が module・name とも一致する。
- [ ] `tsuzuri run` と `build` が `--link`・`-l`・`-L` と manifest の `[native]` でホストの object・static library をリンクする。
- [ ] `extern type` のハンドルが Copy でない値として move・借用・export でき、native と WASM で pointer 幅の値として渡る。
- [ ] トップレベル関数をコールバックとして C と JS のホストへ渡せ、再入（コールバックから extern）と trap が D10 どおりに動く。
- [ ] E2E が native・WASM × `-O0`・`-O3` で成功し、native の各呼び出し後に `live == 0`。
- [ ] link name・ハンドル・コールバックを使わないプログラムの IR・header・WASM import と export が HEAD と byte 単位で同じ。
- [ ] 診断の表の全行に Rust テストか E2E がある。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `BuildOptions` は `#[derive(Clone, Copy, Debug)]`。`Vec` の field を足すと Copy が外れ、呼び出し元が広く壊れる。`LinkInputs` を別の引数で渡す。
- `Type::exportable` は `scalar_record` の field 判定にも使われる。ここにハンドルを足すと record の field にハンドルが入り、record の ABI
  （`record_name`・`record_layout`）が壊れる。`abi_scalar` を別に作る。`extended`・`uses_host_abi` も `abi_scalar` にしないと、ハンドルだけの
  関数で `tsuzuri_alloc` の export と `%tz.abi.buffer` が不要に出る。
- 明示 symbol の prototype を header に出すと、利用者が同時に include する system header と型が衝突する（macOS の `int64_t` は `long long`、
  `long` を使う宣言と非互換）。`explicit` の import は prototype を出さない。
- `reachable_functions` はコールバック extern の id を到達可能として集める。その関数本体を出すと `host_call` が `%tz.closure` の引数を受けて
  壊れる。emit loop で本体が `HostCall` かつ `import.callbacks` の関数を飛ばす。
- `Type::can_capture` は `types.recursive(self)` の分岐（`stored_all` の closure）と通常の match の 2 か所に `Type::Task` がある。両方に足す。
- Mach-O は C 名に `_` を付けるが、IR の名前には付けない（リンカーが付ける）。symbol 検査も `_` を付けない名前で行う。
- 静的 library は参照する object より後に置く。Linux で static library が libm を使うなら利用者が `-l m` を足す（既存の `-lm` の位置は変えない）。
- wasm-ld は `--export-table` がないと関数 table を export しない。JS からコールバックを呼べなくなる。コールバックのない出力には足さない。
- JS では i64 は BigInt、ハンドルと関数 table の index は number。`-O3` でも address を取った internal 関数は table に残る。
- `tsuzuri test` も exe を作るので `LinkInputs` を渡す経路を忘れない。cache を使う経路が残ると、入力を変えても古い exe が再利用される。
- E2E は fixture を必ず temp root に写す（E03 の再帰的な source 探索）。`cargo test --locked <filter>` の 0 件成功に注意する。

## 対象外

- `unsafe`・生ポインター型、C++ ABI、可変長引数関数、externref、共有 library の出力（E13）、bitcode・LTO（PR08）。
- 捕捉のある関数値のコールバック、i128・f16・タプル・固定長配列（A16）の ABI、`Option<H>` と NULL の対応、
  `extern type` への `Drop` instance（B07 の拡張）。依存 package の `[native]` は root の `native = true` による opt-in として後から実装した（「追加した拡張と確認」）。
- Phase 2（設計方針）: 捕捉のある関数値は `{ 関数 pointer, void *context }` として呼び出し中だけ貸す（非 escaping。保存しないことはホストとの契約）。
  ABI 型の追加は E05 の buffer・out 引数の型の上に足す。Windows でのリンク入力の検証は G10 と行う。

## 決定事項

### D1: link name の構文

- 決定: `extern "symbol" def` と `extern "module" "symbol" def`。文字列一つは native symbol と WASM name（module は `tsuzuri`）、二つは WASM module と
  symbol（native は symbol だけ）。指定がなければ E06 の名前のまま。
- 理由: GUIDE D-30 が既存の予約語の組み合わせとして挙げる形で、予約語を増やさない。module の既定を E06 と同じ `tsuzuri` にすると JS ホストの
  import object が一つで済む。
- 状態: 要承認（承認前は段 A の手順 2・3 に着手しない）

### D2: symbol・module の規則と header

- 決定: symbol は 255 bytes 以下の C 識別子で、`tz_`・`tsuzuri`・`__` で始まらず `RESERVED_HOST_SYMBOLS` にない。同じ symbol の複数宣言は関数型と
  module が一致すれば許す。明示 symbol の import は header に prototype を出さない。
- 理由: `tz_` は export、`tsuzuri` は runtime と E06 の名前空間。生成 IR 自身の `declare` と型が違うと LLVM が拒否する。libm などは利用者が
  system header を持っているので、prototype を出すと衝突の危険だけが増える。
- 状態: 既定案（実装者はこの案に従う）

### D3: `extern type` の意味

- 決定: `extern type Name` は Copy でも clone 可能でもない葉の型 `Type::Handle`。drop glue を持たず、関数値に捕捉できず、Task へは move できる。
  解放は利用者が close の extern へ値で渡して行う。自動解放は B07 の `Drop` を包む record に書く。
- 理由: Copy でないので close 後の使用と二重 close を E1012 で防げる。B07 D1 は `Drop` を record・union に限るので、ハンドル自体に drop を
  持たせると B07 の変更が要る。drop glue がなければ E12 は B07 と独立に出荷できる。
- 状態: 要承認（承認前は段 C の手順 6・7 に着手しない）

### D4: ハンドルの表現・ABI・header 名

- 決定: LLVM `ptr`（wasm32 では i32、JS では number）。`ref H` もハンドルの値を渡す。header は `handle_c_name` の長さ付き符号化で
  `typedef struct <name>_s *<name>;` を一度だけ出す。
- 理由: ホストは pointer でも整数の番号でも表せる。長さ付き符号化は `record_name` と同じく module の区切りで衝突しない（`A_B.C` と `A.B_C`）。
  struct の tag と typedef 名を分けると C++ からも include できる。
- 状態: 既定案（実装者はこの案に従う）

### D5: ハンドルを使える ABI の位置

- 決定: extern・`export def`・コールバックの引数（`H`・`ref H`）と extern・`export def` の結果（`H`）。ABI の record の field・buffer の要素・
  `ref mut H` は不可。
- 理由: record の field に許すと record の layout（native 8 bytes、wasm32 4 bytes）と JS glue を変える必要がある。
- 状態: 既定案（実装者はこの案に従う）

### D6: リンク入力の CLI と manifest

- 決定: `build`・`run`・`test` の `--link <path>`・`-l <name>`・`-L <dir>`（分離形だけ、繰り返し可、合計 256）と、root package の manifest の
  `[native]`（`link`・`libraries`・`search`）。native exe でだけ有効。
- 理由: 分離形は既存の option 解析（`-o` と同じ）に合う。依存 package のリンク入力は供給網の危険があり、E10 と合わせて決める。
- 状態: 要承認（承認前は段 B の手順 4・5 に着手しない）

### D7: cache・出力保護・引数の順序

- 決定: リンク入力があれば build cache を使わない。出力 path が入力と同じなら E2003。順序は manifest → CLI、各種別の中は記述順で、
  `-L`・path・`-l` の順に既存の引数の後へ足す。
- 理由: `crate::cache::build_key` は入力 file の内容を知らない。内容を key に含めるのは費用に見合わない（リンク入力のある build は最終段だけが違う）。
- 状態: 既定案（実装者はこの案に従う）

### D8: 静的コールバックを Phase 1 に含める

- 決定: extern の関数型の引数に、型引数のないトップレベルの利用者関数の名前だけを渡せる。コールバック extern は直接・全引数で呼ぶ。
- 理由: 捕捉がないので関数 pointer は静的な寿命を持ち、寿命の契約が要らない。`export def` の wrapper をそのまま使える。捕捉のある関数値は
  context pointer と非 escaping の契約が要るので Phase 2 に残す。
- 状態: 要承認（承認前は段 D の手順 8 に着手しない）

### D9: コールバックの lowering

- 決定: 呼び出し側で `host_call` を直接出し、関数型の引数は `ptr @tz.callback.<suffix>`（internal、`wrapper` で生成）。コールバック extern の
  関数本体は出さない。WASM は `module.callbacks` が空でないときだけ `--export-table`。
- 理由: extern の関数本体は実行時の `%tz.closure` しか受け取れず、どの関数かを知れない。直接呼び出しに限れば呼び出し側で静的に決まる。
- 状態: 既定案（実装者はこの案に従う）

### D10: 再入・スレッド・trap

- 決定: コールバックは名前のない `export def` と同じ規則で呼べる（再入可、いつでも呼べる）。コールバック中の trap は通常の trap と同じで、WASM では
  trap 後に instance を使い続けてはいけない。当初の E2E は同じスレッドの同期呼び出しと再入だけを検証した（ホストの別スレッドからの呼び出しと同時呼び出しは、後の「追加した拡張と確認」で native について検証した）。
- 理由: Tsuzuri は可変の大域状態を持たないので再入で壊れる状態がない。回復可能なトラップ境界は E14 が定める。
- 状態: 既定案（実装者はこの案に従う）

### D11: ABI 型の追加範囲

- 決定: Phase 1 で足す ABI 型はハンドルとコールバックだけ。i128・f16・タプル・固定長配列・ハンドルを含む record は従来どおり E1008。
- 理由: i128 は C ABI と JS の変換が target ごとに異なり、E05 の buffer・out 引数の型で表せる範囲を先に使い切るほうが安全。
- 状態: 既定案（実装者はこの案に従う）

### D12: 診断コード

- 決定: 新しいコードを割り当てず、E0002・E1001・E1005・E1008・E1012・E2000・E2001・E2002・E2003 を使う（「診断」の表）。
- 理由: どの条件も既存コードの分類（構文、名前、ABI、所有権、CLI、I/O、リンク、出力保護）に入る。
- 状態: 既定案（実装者はこの案に従う）

### D13: 生ポインター

- 決定: 導入しない。低水準の操作はホスト側に置き、ハンドルで受け渡す。
- 理由: 生ポインターは `unsafe` と借用規則の例外を要し、GUIDE §1 の安全性の方針に反する。
- 状態: 既定案（実装者はこの案に従う）

## 実装と検証（2026-10-02）

D1・D3・D6・D8 の承認（四つとも既定案どおり）を 2026-10-02 に受け、Phase 1 の段 A → B → C → D（手順 1–10）をこの順に実装した。
「対象外」の項（捕捉のある関数値のコールバック、依存 package の `[native]`、`Option<H>` と NULL の対応、`extern type` への `Drop`、
i128・f16・タプル・固定長配列の ABI、生ポインター）は実装していない。着手時の作業ツリーは HEAD `30b2d1d` に F12 と E14 の未コミットの変更を
加えたもの（チケットの確認時の HEAD は `f8dc655`）。コミットは作っていない。

### 実装

- 段 A（D1・D2）: `src/syntax.rs`（`LinkName`、`SignatureDecl::link`）、`src/parser.rs`（`Parser::link_name`。トップレベルの `extern` の読み取りは
  別 method の `Parser::extern_declaration`。下の判断 3）、`src/formatter.rs`（literal の綴りを保つ）、`src/check.rs`（`HostImport` の
  `wasm_module`・`explicit`・`callbacks`。`explicit_symbols` の表で同じ symbol の再宣言を照合）、`src/abi.rs`（`RESERVED_HOST_SYMBOLS`、
  `link_name_error`）、`src/llvm_imports.rs`（WASM 属性の module、明示 symbol の prototype を header に出さない）。
- 段 B（D6・D7）: `src/driver.rs`（`LinkInputs`、`library_name_error`、`check_shape`、`check_readable`、`add_to`、`protect_links`、`build_linked`、
  `run_with_diagnostics`。リンク入力があれば build cache を使わない）、`src/test_runner.rs`（`run_tests_linked`、`build_runner`）、
  `src/main.rs`（`--link`・`-l`・`-L`、`links_apply`、HELP）、`src/package.rs`（`[native]`、`Manifest::native`）。
- 段 C（D3–D5）: `Type::Handle(Box<str>)`、`NamedType::Handle`、`Names::handles`・`handle_aliases`（`src/check.rs`）、`ExternTypeDecl`・`extern type`
  （`src/syntax.rs`、`src/parser.rs`）、`src/formatter.rs`・`src/docgen.rs`・`src/semantic.rs`・`src/higher_kinds.rs`、`abi_scalar`・`is_handle`（`src/abi.rs`）、
  `handle_c_name`・`handle_typedefs`・`extended`・`uses_host_abi`・`c_parameters`・`wrapper`（`src/llvm_abi.rs`）、`llvm_type`・`c_type`・`abi_type`・
  `canonical_type`・`clone_value`（`src/llvm.rs`）、`src/llvm_debug.rs`、`src/llvm_imports.rs`。
- 段 D（D8–D10）: `src/check.rs`（コールバック型の検査、`validate_callbacks`）、`src/llvm.rs`（`Globals::callbacks`、callback extern の本体を出さない、
  `FunctionEmitter::call` から `host_call` を直接出す、特殊化の後に wrapper を出す）、`src/llvm_imports.rs`（`host_call` の関数型の引数）、
  `src/llvm_abi.rs`（`WrapperKind`、`wrapper` の名前と linkage、unit 引数の省略、header の関数 pointer 引数）、`src/driver.rs`（`--export-table`）。
- テスト: `tests/ffi_extensions.rs`（新規、10 件）、`tests/ffi_extensions.mjs`・`tests/ffi_extensions_host.c`・`tests/fixtures/ffi_extensions/Main.tz`（新規）、
  `src/main.rs` の `parses_link_inputs`・`rejects_link_inputs_outside_native_executables`、`src/package.rs` の `parses_native_link_section`・
  `rejects_malformed_native_link_section`、`src/abi.rs` の `reserved_symbols_cover_the_runtime_names`、`tests/host_imports.rs`（1 行。判断 1）。
- 文書: `docs/language.md`、`docs/architecture.md`、`_docs/guides/native-interop.md`、`_docs/guides/webassembly.md`、`_docs/tools/command-line.md`、
  `_docs/guides/README.md`、`_docs/language-reference/functions.md`、`_docs/feature-status.md`、`README.md`、`_features/README.md`、
  `_features/GUIDE.md`（D-30 の台帳）、`_perfs/README.md`（リンク）。

### 決定事項への追記（チケットから外れた判断）

1. `tests/host_imports.rs` の期待値を 1 行変えた。`extern def bad :: (i64 -> i64) -> i64`（E1008）を `(string -> i64) -> i64`（E1008）にした。
   D8 の承認で `(i64 -> i64) -> i64` は受理されるコールバックになるため、「既存テストへの影響: なし」からの意図した逸脱である。ほかの既存テストの期待値は変えていない。
2. `CheckedModule::callbacks`（「データ構造」）は作らなかった。コールバックとして渡される関数は emit 中に `Globals::callbacks`（`BTreeSet<usize>`）へ集め、
   特殊化の emit の後に id 順で wrapper を出す。到達性を二重に持たず、到達しない関数のコールバックに wrapper を出さないため。driver は IR の `@tz.callback.`
   の有無で `--export-table` を決める（`io_runtime` などの IR 走査と同じ形）。
3. `extern` の読み取りは別 method の `Parser::extern_declaration` に切り出した。`program_all` の closure に足すと `bounds_type_growing_polymorphic_recursion` が
   2 MiB の stack で溢れた（GUIDE §11.1。上限は変えていない）。
4. `RESERVED_HOST_SYMBOLS` は手順 3 の grep の出力に加え、numeric runtime が定義する内部名（`decode`、`power` など）も含む。明示 symbol がこれらと同名だと
   clang が再定義で失敗した。`reserved_symbols_cover_the_runtime_names` が、runtime の定義と予約一覧の食い違いを検出する。
5. `llvm_abi::wrapper` は 8 引数になったので `#[allow(clippy::too_many_arguments)]` を付けた（`src/llvm.rs` の既存の例と同じ）。
6. WASM の E2E は wasm64 も扱う。Node.js 24 以降は instance を作って実行し、それ未満（20.17.0）は link だけを確かめる。wasm64 ではハンドルと関数 table の index が BigInt。
7. `unit` を唯一の引数とするコールバックの wrapper は C の引数を持たず、Tsuzuri の関数を `i8 0` で呼ぶ。ホストの prototype は `(*arg0)(void)`。
8. E2E の fixture に `callback_shapes`（unit 引数・2 引数・`ref` ハンドルのコールバック）を足し、「E2E」の例より広く確かめた。期待値 82（= 15 + 15 + 10 + 42）は C の参照実装の手計算。
9. 診断は D12 のとおり既存のコードだけを使った。`doc` に `--link` を付けると、リンク入力の E2000 より先に `-o` の要求（E2000）が報告される。

### 確認（Apple M1 Max、macOS、Apple clang 21.0.0、Homebrew LLD 23.1.1、rustc 1.98.1、Node v20.17.0 と v24.21.0）

- ベースライン: `tests/fixtures/host_imports` の IR・wasm32 IR・header を `/tmp/tz-e12/before*` に保存した（F12・E14 適用済みのコンパイラ）。
  手順 3・7・8 の後に出し直した 3 組が `cmp` で一致（`SAME`）。wasm32 の `.wasm` も一致した。
- 段ごとの Rust: `cargo test --locked --test ffi_extensions` が段 A で 3、段 C で 8、段 D で 10 passed。`--bin tsuzuri` の `parses_link_inputs`・
  `rejects_link_inputs_outside_native_executables`、`--lib` の `parses_native_link_section`・`rejects_malformed_native_link_section`・
  `reserved_symbols_cover_the_runtime_names` が passed。`--test host_imports` 4、`--test host_abi` 4、`--test formatter` 13 passed。
  §3.1 の stack-depth 3 テスト（`bounds_type_growing_polymorphic_recursion`・`bounds_recursive_and_flat_expression_depth`・
  `bounds_nested_builder_expansion_not_just_source_syntax`）は各段の後と最後に成功した。
- 全体: `cargo test --locked` は 548 passed・0 failed（E12 の前は 533）。`cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings` が成功。
  `cargo check --locked --all-targets --target x86_64-pc-windows-msvc` と `aarch64-pc-windows-msvc` が成功（Windows での実行は未検証）。
- 出力の不変: 変更前（F12・E14 適用済み）のコンパイラと新しいコンパイラで、`tests/fixtures/*`・`examples/*`・`benchmarks/{control,computations,tasks}` の
  各 project について `--emit llvm`（native と wasm32）と `--emit header` の出力・終了状態・診断を比べ、192 組すべてが一致した（E12 の fixture は除く）。
- E2E: `node tests/ffi_extensions.mjs target/release/tsuzuri` が Node 20.17.0 と 24.21.0 のどちらでも `ffi extensions: native -O0 passed`・`native -O3 passed`・
  `run passed`・`WASM -O0 passed`・`WASM -O3 passed` の 5 行を出して終了コード 0（native は `live == 0` と counter 数 0 を各呼び出しの後に検査、
  `callback_trap` は別プロセスで異常終了、`run` は `--link`・`-l`/`-L`・`[native]` のどれでも stdout `98`、`build -g` の実行ファイルと `tsuzuri test`（2 passed）も `--link` を受ける、wasm32 の import は表の 10 個に一致し
  `__indirect_function_table` を export、コールバックのない `tests/fixtures/host_imports` は table を export しない、Node 24 では wasm64 も実行）。
  拒否: wasm32 への `--link`、不正な `-l`、存在しない path・directory、出力と入力が同じ、unresolved symbol、依存 package の `[native]` が
  それぞれ `E2000`・`E2001`・`E2003`・`E2002` で失敗する。`tests/host_imports.mjs` は `-O0`・`-O3` で成功。
- 既存の suite（release、native/WASM × `-O0`・`-O3`）: 29 項目を一つの gate script で実行し、すべて成功した（29 PASS・0 FAIL）。
  Node 20.17.0 を使い、primitives・features・wasm64 の suite と、trap_boundary・ffi_extensions の二回目は Node 24.21.0。
  `check-runtime-includes.sh`（21 files、変更なし）、host_imports、trap_boundary（17 case）、e2e、primitives、strings、tasks（41 結果・4 trap）、
  computations（70）、control（270）、numeric_casts（1585）、integer_intrinsics（263182）、display_parse、features（4958）、
  features の wasm64 `bounds_checks`（28）、examples、wasm_threads、wasm_memory、wasm64、io、cache、simd（232）、wasm_simd、cpu_dispatch、debug_info、
  docgen、lsp_sessions。display_parse は作業機の `/usr/bin/python3`（3.9.6）に `sys.set_int_max_str_digits` がないため、`PATH` の先頭に Homebrew の Python を置いて実行した。
- 文書: `node scripts/check-docs.mjs` が全体で成功した（83 pages・746 links・141 checked examples・246 native runs（O0/O3）・9 test projects）。
  変更した Markdown の診断は増えていない（`_features/README.md` の MD060 は既存の表の区切り行）。`git diff --check` が空。
- 性能: link 名・ハンドル・コールバックを使わないプログラムの IR・header・WASM import が同一なので計測していない。性能は主張しない。

### 残作業

- 捕捉のある関数値のコールバック（Phase 2 の設計方針）、`Option<H>` と NULL の対応、`extern type` への `Drop`（B07 の拡張）、i128・f16・タプル・固定長配列の ABI は未実装。
  依存 package の `[native]` は、次の「追加した拡張と確認」で root の `native = true` による opt-in として実装した。
- Windows でのリンク入力は、`-L`・`-l` が MSVC の linker へ `-libpath:` と `<name>.lib` として渡ることを `clang -###` の dry run で確かめた（単体テスト）。Windows での実際のリンクと実行は未検証。
- この環境の WASM の build は `build cache disabled: tool version query failed` を表示する。変更前のコンパイラでも同じで、E12 とは無関係。

## 追加した拡張と確認（2026-10-02、包括承認の後）

「未実装・未検証の項目をすべて実装・検証する。判断は実装者に任せる」との指示を受けて、残作業のうち次を実装・検証した。

- 依存 package の `[native]`（D6 の拡張）: root の `[dependencies]` に `lib1 = { path = "lib1", native = true }` と書いた**直接の**依存だけが `[native]` を持てる。許可のない依存と、
  依存の依存が自分で書いた `native = true`（root が許可していない）は `E2000`（`only the root package and dependencies it marks with native = true may declare [native] link settings; …`）。
  許可した依存の path は、その依存の root からの相対で解決し、root の入力の後に連結する。`src/package.rs`（`Dependency::native`、`parses_the_native_opt_in_of_a_dependency`）、
  `src/driver.rs`（`Project::load_from_root` の `trusted`）、`tests/ffi_extensions.mjs`（許可なし・許可あり（stdout `42`）・連鎖の 3 case）。供給網の観点で、許可は root が一つずつ書く形にした。
- ホストの別スレッドからのコールバック: fixture に `threaded`（ホストの `e12_apply_threaded` が自分で pthread を起こしてコールバックを呼び、join してから戻る）を足し、native の host main が
  8 スレッドから 200 回ずつ `tz_counters`・`tz_callbacks`・`tz_threaded` を同時に呼んで期待値と `live == 0` を検査する。`-O0`・`-O3`、object と IR のリンクの両方で成功した。
  WASM の import 表に `e12_apply_threaded` が増え（11 個）、`tz_threaded` も検査する。host の object が pthread を使うので、E2E のリンクに `-pthread`・`-l pthread` を足した。
- Windows の link 入力: `driver::tests::link_inputs_reach_the_msvc_linker_as_libpath_and_lib_names`（`clang -###` で x86_64・aarch64 の `windows-msvc` に `-L libs -l sqlite3 host.obj` を渡し、
  linker 行に `-libpath:libs`・`"sqlite3.lib"`・`host.obj` が出ることを確かめる。Clang がない環境では何も検査しない）。
- 実装しなかったもの（理由）: 捕捉のある関数値のコールバック（`{ 関数 pointer, void *context }` の契約、closure の呼び出し ABI の trampoline、型検査の変更が要り、E14 の境界・B07 と一緒に設計するほうが安全）、
  `Option<H>` と NULL の対応（`abi_scalar` が型の文脈を持たないため、ABI 判定の関数すべてに文脈を通す変更になる）、i128・f16・タプル（D11 のとおり C ABI と JS の変換が target ごとに異なる）、
  `extern type` への `Drop`（B07 が未実装）。いずれも既存の診断（E1008）のままで、挙動は変えていない。

## スレッド検証のレビュー修正（2026-10-02）

`tests/ffi_extensions_host.c` は `assert` にスレッド生成・待機を含むので、`assert.h` の前で `NDEBUG` を解除し、検証を常に有効にした。`tests/ffi_extensions.mjs` は native の `-O0`・`-O3` の host と、リンク入力に使う host object に明示的に `-DNDEBUG` を渡す。別スレッドのコールバックと 8 スレッドの同時呼び出しが、その設定でも実行・検証されることを確かめる。
