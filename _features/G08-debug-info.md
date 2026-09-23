# G08: デバッグ情報（DWARF／WASM）
| 項目 | 内容 |
|---|---|
| ID | G08 |
| 優先度 | P2 |
| 規模 | M |
| 依存 | – |
| 後続 | G04（トラップ発生位置の報告）、G07（hover と実行時位置情報の整合）、G10（Windows PDB / CodeView 検討） |
| 状態 | todo |
| 主な影響ファイル | `src/main.rs`, `src/driver.rs`, `src/llvm.rs`, `src/llvm_control.rs`, `src/llvm_frame.rs`, `src/diagnostic.rs`, `tests/debug_info.rs`, `tests/debug_info.mjs`, `README.md`, `docs/architecture.md` |

## 目的

`tsuzuri build -g` で LLVM debug metadata を出力し、native と wasm32 の成果物にソース行・関数・ローカル変数・型の情報を保持する。Phase 1 は DWARF metadata の正確性と決定性を優先し、最適化後の完璧な変数表示や PDB 生成は対象外にする。

## 現状

- `src/main.rs` の CLI は `-O0`〜`-O3`, `--target`, `--emit`, `--cpu` を扱うが `-g` はない。
- `src/driver.rs` の `BuildOptions { target, emit, optimization, cpu }` に debug flag はない。`build` は `llvm::emit_target(module, entry, wasm)` を呼び、Clang / wasm-ld を起動する。
- `src/llvm.rs` の `emit_target` は LLVM IR text を手書きで生成する。`Globals { definitions, next_metadata, wasm }` は loop metadata 用 `next_metadata` を持ち、`include_str!("runtime/numeric.ll")` の metadata 番号より後から採番する。
- `FunctionEmitter` は `instruction` / `value` helper で IR 命令を `lines` に積む。現在は `TypedExpr.span` を命令へ伝えていない。
- `FunctionEmitter::bind_local` は entry block alloca を `allocas` に積み、`store` を通常命令として出力する。ローカル変数の debug declare はない。
- LLVM 型は `llvm_type` で `%tz.string`, `%tz.array`, `%tz.list`, `%tz.closure`, `%tz.record.Module.Name`, tuple literal struct へ下げる。
- `Cargo.toml` の `[profile.release] strip = true` はコンパイラ binary 自身の strip であり、生成物の debug info とは無関係。

## 仕様

### CLI

```text
tsuzuri [build] source.tz|directory -g [options]
tsuzuri run Main.tz|directory -g [options]
```

- `-g` と `--debug-info` は同義。
- `check` では無効。指定時は `E2000`。
- `--emit llvm -g` は debug metadata を含む `.ll` を出力する。
- `--emit header -g` は意味がないため `E2000` で拒否する。
- `-g -O3` は許可する。最適化で変数が見えにくくなる可能性は docs に書くが、意味を変える最適化は追加しない。
- `--target wasm32 -g` は DWARF custom sections を残す。

### LLVM metadata

最小必須 metadata:

- `!llvm.dbg.cu = !{!<compile_unit>}`
- `!DICompileUnit(language: DW_LANG_C, file: !DIFile(...), producer: "Tsuzuri <version>", isOptimized: <bool>, emissionKind: FullDebug, ...)`
- source ごとの `!DIFile(filename: "...", directory: "...")`
- 関数ごとの `!DISubprogram`
  - 通常関数: `Module.name`
  - lifted lambda: `$lambda.<id>` ではなく、親 span 由来の `Module.<function>.<lambdaN>` を deterministic に付ける。初期実装で親を取れない場合は `function.qualified_name()` と関数 id を使うが、ユーザーに見える名前と内部名を docs に区別して書く。
  - task lambda: `Module.<function>.<taskN>`
  - instance method / specialization: 既存 `CheckedFunction { module, name }` に基づき、`$instance.<id>.<method>` / `name.$mono.<n>` をそのまま出す。後続で demangle 可能にする。
- 式由来命令ごとの `!DILocation(line, column, scope: !subprogram)`
- entry block alloca ごとの `!DILocalVariable` と `llvm.dbg.declare`
- 型:
  - scalar: `DIBasicType`（整数は signed/unsigned encoding、bool、float）
  - `unit`: 1 byte の artificial basic type
  - string/array/list/closure: `DICompositeType`（descriptor fields）
  - record: `DICompositeType(tag: DW_TAG_structure_type, name: "Module.Record", elements: ...)`
  - tuple: anonymous `DICompositeType`
  - reference/function pointer fields: pointer-sized derived type

### native / wasm toolchain

- native: Clang invocation に `-g` を追加する。task runtime C をコンパイルする場合も同じ `-g` を付ける。
- wasm32:
  - Clang wasm object 生成にも `-g`。
  - `wasm-ld` は `-g` 時に `--strip-all` を付けない。非 debug build は従来どおり `--strip-all`。
  - debug build で明示 strip option は追加しない。将来 `--strip-debug` を追加する場合は別チケット。
- `--emit llvm -g` は linker を通らないため、IR 内 metadata の検査対象になる。

### 決定性

- metadata id は `Globals::next_metadata` だけから採番し、`BTreeMap` / `BTreeSet` で source / type / subprogram cache を管理する。
- 同じ source / options / path 入力で 2 回出力した `.ll` は byte-identical。
- runtime `numeric.ll` の metadata 番号と衝突しない現状の `Globals::default` 方針を維持する。

## 設計

### 追加する主な型

`src/driver.rs`:

```rust
#[derive(Clone, Copy, Debug)]
pub struct BuildOptions {
    pub target: Target,
    pub emit: Emit,
    pub optimization: u8,
    pub cpu: Cpu,
    pub debug_info: bool,
}
```

`src/llvm.rs`:

```rust
pub struct DebugSource<'a> {
    pub path: &'a std::path::Path,
    pub text: &'a str,
}

pub struct DebugOptions<'a> {
    pub enabled: bool,
    pub optimized: bool,
    pub sources: &'a [DebugSource<'a>],
}

pub fn emit_target_with_options(
    module: &CheckedModule,
    entry: Entry,
    wasm: bool,
    debug: DebugOptions<'_>,
) -> Result<String, Diagnostic>;
```

既存 `emit` / `emit_target` は `DebugOptions { enabled: false, ... }` を渡す互換 wrapper とする。

内部:

```rust
struct DebugContext {
    enabled: bool,
    compile_unit: Option<usize>,
    files: Vec<usize>,
    subprograms: BTreeMap<usize, usize>,
    types: BTreeMap<Type, usize>,
}

struct FunctionEmitter<'a, 'b> {
    // existing fields...
    current_location: Option<usize>,
    subprogram_metadata: Option<usize>,
}
```

### `instruction` / `value` への位置の通し方

- `FunctionEmitter::expression_mode` の入口で `let previous = self.current_location` を保存し、`TypedExpr.span` から `DILocation` を取得して設定する。
- 子式を評価するときは子式の `expression_mode` がさらに上書きする。
- 終了時に previous を戻す。
- `instruction` は `current_location` がある場合、通常命令と terminator に `, !dbg !N` を付ける。
- `value` は従来どおり `fresh` して `instruction` を呼ぶだけにし、metadata 付与を重複させない。
- label 行、type 定義、global 定義、metadata 定義には `!dbg` を付けない。
- PHI:
  - `emit` の loop parameter phi と `tail` の back edge は synthetic なので、Phase 1 では `function.span` の location を付けるか、付けない。
  - ユーザー式由来の `If` phi は `expression.span` の location が付く。

### ローカル変数

- `slot` は alloca text だけを返す現状を維持し、debug 有効時は alloca の直後に出す entry instruction list を追加する。
- `FunctionEmitter` に `entry_instructions: Vec<String>` を追加し、`emit` / `auxiliary` で allocas の後、`br label %loop` の前に出力する。
- `bind_local` は `Local.name != "_"` の場合:
  - `DILocalVariable(name, arg: index+1 if parameter else 0, scope: subprogram, file, line, type)`
  - `call void @llvm.dbg.declare(metadata ptr %slot, metadata !var, metadata !DIExpression())`
  - `declare void @llvm.dbg.declare(metadata, metadata, metadata)` を `intrinsics` に登録。
- entry block alloca と dbg.declare は決定的に local bind 順。

### 型 metadata

`debug_type(&Type)` を cache する。

| Tsuzuri 型 | Debug metadata |
|---|---|
| `iN` / `iNu` | `DIBasicType(name: "i64", size: 64, encoding: DW_ATE_signed/unsigned)` |
| `f32` / `f64` | `DIBasicType(... DW_ATE_float)` |
| `f16` / `f128` / `d*` | storage は `iN` だが name は `f128` / `d128`、encoding は float / decimal を表せない環境があるため `DW_ATE_float` または unspecified。未決事項に記録 |
| `bool` | `DIBasicType(name: "bool", size: 1, encoding: DW_ATE_boolean)` |
| `string` | `{ ptr data, i64 length }` |
| array/list | descriptor `{ ptr, i64 }`。element type は member 名または artificial member に保持 |
| record | `CheckedRecord.fields` を順に member にする |
| tuple | `tuple.<hash>` の anonymous composite |
| function/task | `%tz.closure` descriptor |
| reference | pointer type |

サイズは LLVM layout と矛盾しない保守値にする。正確な ABI offset が難しい型は `size` だけを出し、member offset は Phase 2 に回してもよいが、IR verifier を通す。

### コンパイラ段階ごとの変更

| 段階 | 変更 |
|---|---|
| lexer / parser / check / ownership | 変更なし。既存 `Span` を利用 |
| polymorph / closures | 変更なし。ただし lowered function 名を debug 名へ deterministic に写す |
| llvm | debug metadata 生成、DILocation、DILocalVariable、型 metadata |
| llvm_control | `while_loop`, `range_loop`, `for_each`, `match_expression` は `expression` 呼び出し経由で位置が付く。手書き branch/trap は現在 location を維持 |
| llvm_frame | 変更なし。frame 由来 alloca に user local 名は付けず、必要なら artificial variable |
| runtime | task runtime C compile に `-g`。runtime `.ll` は Phase 1 では debug metadata なし |
| driver | `BuildOptions.debug_info` と tool flags |
| main | `-g` / `--debug-info` parse |

## 実装手順

1. **CLI と options**
   - `BuildOptions.debug_info` を追加し、`Default` は false。
   - `parse_arguments` で `-g` / `--debug-info` を読む。
   - `check` と `--emit header` では `E2000`。
   - 確認: `rejects_ambiguous_or_unused_arguments` に追加。
2. **LLVM API 分離**
   - `emit_target_with_options` を追加し、既存 API は debug 無効 wrapper にする。
   - `driver::build` から project sources を `DebugSource` として渡す。
   - 確認: debug 無効時の IR が変更前と同一。
3. **compile unit / file / subprogram**
   - `Globals` に debug context を追加。
   - `DICompileUnit`, `DIFile`, `DISubprogram` を deterministic に生成。
   - 確認: `--emit llvm -g` に `!llvm.dbg.cu`, `DICompileUnit`, `DISubprogram` が含まれる。
4. **DILocation**
   - `current_location` を追加し、`instruction` / `value` に `!dbg` を付ける。
   - `diagnostic::location(source, span.start)` で line/column を得る。column は DWARF 用にも 1 始まりでよい。
   - 確認: source の加算式、if、match 由来命令に期待 line の `DILocation` が付く。
5. **locals**
   - `entry_instructions` と `llvm.dbg.declare` を追加。
   - parameters と block locals の `DILocalVariable` を生成。
   - 確認: `let answer = ...` と関数引数の `DILocalVariable(name: "...")` が出る。
6. **types**
   - `debug_type` cache を実装。
   - record / tuple / descriptors の `DICompositeType` を追加。
   - 確認: record fields の名前が metadata に含まれる。
7. **toolchain flags**
   - Clang native / wasm object compile に `-g`。
   - task runtime C compile に `-g`。
   - wasm-ld は debug 有効時 `--strip-all` を外す。
   - 確認: wasm output に debug custom section があることを `llvm-dwarfdump` または `wasm-objdump` で検査。
8. **docs と tests**
   - README / architecture を更新。
   - `tests/debug_info.rs` と `tests/debug_info.mjs` を追加。

## テスト計画

- Rust (`tests/debug_info.rs`):
  - `llvm::emit_target_with_options(... enabled: true ...)` が metadata を含む。
  - debug 無効 IR は従来と同じ形式で `!dbg` を含まない。
  - 同じ入力で 2 回 emit して byte-identical。
  - record / tuple / string / array / closure の type metadata が重複しない。
  - `llvm.dbg.declare` が entry block alloca 後に出る。
- Node (`tests/debug_info.mjs`):
  - `target/release/tsuzuri build fixture -g --emit llvm -O0/-O3` を実行し、`clang -x ir -c` または `llc` で verifier 相当を通す。
  - native executable を `-g -O0/-O3` で build し実行結果が同じ。
  - wasm32 を `-g -O0/-O3` で build し、通常の export 実行結果が同じ。
  - `llvm-dwarfdump` が PATH にない場合はテストを成功扱いで skip しない。`assert.fail("llvm-dwarfdump is required for debug_info.mjs; install LLVM or set PATH")` のように明示失敗する。
- 既存回帰:
  - `cargo test --locked`
  - `node tests/e2e.mjs target/release/tsuzuri`
  - codegen 変更なので native/WASM × `-O0`/`-O3` の代表 E2E を実行。

## ドキュメント

- `README.md` CLI 表に `-g, --debug-info`。
- `docs/architecture.md` の LLVM 節に metadata 生成規則、`--strip-all` との関係、Cargo profile `strip = true` はコンパイラ binary 用であることを追記。
- `docs/architecture.md` の検証コマンド一覧に `node tests/debug_info.mjs target/release/tsuzuri`。

## 受け入れ条件

- [ ] `--emit llvm -g` が verifier を通る debug metadata を含む。
- [ ] `-g` なしの出力は metadata を含まず、既存 tests が通る。
- [ ] native / wasm32 の `-O0` / `-O3` で実行結果が変わらない。
- [ ] metadata id が決定的で、同一入力 2 回の IR が byte-identical。
- [ ] `DILocation` が `TypedExpr.span` 由来の source line を指す。
- [ ] locals に `DILocalVariable` と `llvm.dbg.declare` が出る。
- [ ] wasm debug build で `--strip-all` により debug section が消えない。

## 落とし穴

- fast-math や reassociation を debug build のために変えてはいけない。
- `FunctionEmitter::instruction` で metadata を付けるとき、metadata 定義行や label に付けない。
- loop metadata と debug metadata の番号を別々に採番すると衝突する。`Globals::next_metadata` に統一する。
- `-g -O3` では変数が optimized out になることがある。テストは「metadata が存在し verifier を通る」ことを見て、debugger 表示の完全性を合否にしない。
- runtime `numeric.ll` は既に metadata 番号を持つ可能性がある。`Globals::default` の既存衝突回避を壊さない。

## 対象外

- PDB / CodeView の直接生成。
- source-level stepping の完全な品質保証。
- inline function の精密な `DILexicalBlock`。
- panic / trap の stack unwinding。
- runtime `.ll` 全体への詳細 debug metadata。

## 未決事項

- **f16/f128/decimal の DWARF encoding**: 既定案は storage size と Tsuzuri 型名を優先し、encoding は `DW_ATE_float` か unspecified で verifier を通す。正確な decimal encoding は LLVM/DWARF 対応を確認して Phase 2 で見直す。
- **lambda / specialization の表示名**: 既定案は deterministic な内部名を出す。G07 / debugger UX で必要なら demangle 表を追加する。
- **外部ツール不足時の tests**: `llvm-dwarfdump` は明示失敗にする。共有 CI に入れる前に LLVM toolchain を用意する。
