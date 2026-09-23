# E03: 階層モジュール・サブディレクトリ
| 項目 | 内容 |
|---|---|
| ID | E03 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | E02 |
| 後続 | E04, G11 |
| 状態 | todo |
| 主な影響ファイル | `src/driver.rs`, `src/lib.rs`, `src/check.rs`, `src/parser.rs`, `src/parse_control.rs`, `src/polymorph.rs`, `src/llvm.rs`, `tests/modules.rs`, `tests/e2e.mjs`, `docs/language.md`, `docs/architecture.md`, `README.md` |

## 目的

大きいプロジェクトでファイルをサブディレクトリに分け、`Geometry/Point.tz` を `Geometry.Point` モジュールとして参照できるようにする。

現状は入力ディレクトリ直下の sibling `.tz` / `.tt` / `.tc` だけを読むため、モジュール数が増えると名前管理と配置が難しい。

E04 の package namespace、G11 の増分 cache、std の将来拡張の前提として、path と module name の決定的な対応を導入する。

暗黙規則を増やさない方針に従い、Phase 1 では `open`、alias、自動 import、複数ファイル同一 module を導入しない。

## 現状

`src/driver.rs` の `Project::load` は `fs::read_dir(parent)` だけを読み、サブディレクトリを探索しない。

`src/driver.rs` の `Project::load` は source paths を file name 順に sort し、`SourceFile.name` は file stem だけを持つ。

`src/lib.rs` の `analyze_modules` は `("Name.tz", source)` を `Name` module として扱う。

`src/check.rs` の module name 検査は `lexer::lex(name)` が単一 `Ident` になることを要求する。

`src/parser.rs` の `Parser::qualified_ident` は `Ident.Member` までの 1 dot だけを読む。

`src/parser.rs` の postfix `.` は `ExprKind::Field(Box<Expr>, Ident)` を作る。

`src/check.rs` の `Checker::value_expression` は `ExprKind::Field(Name(module), field)` を module function として扱う。

`src/check.rs` の `Names::record` は `"Module.Record"` 形式を前提にする。

`src/polymorph.rs` の `Classes::find` は `format!("{module}.{name}")` または raw name を見る。

`src/llvm.rs` の record type と function symbol は `Module.Name` をそのまま `@tz.fn.Module.name` や `%tz.record.Module.Record` に含める。

`tests/modules.rs` は nested directory を明示的に無視する挙動を検査している。

`docs/language.md` と `README.md` は「サブディレクトリは探索しません」と記述している。

## 仕様

### Phase 1 の構文と module name

`Geometry/Point.tz` は module `Geometry.Point` を定義する。

`Geometry/Point.tt` は module `Geometry.Point` の型クラス宣言ファイル。

`Geometry/Point.tc` は builder module `Geometry.Point`。

同じ module path の `.tz` / `.tt` / `.tc` 併存は従来と同じく `E1011`。

module segment は既存 module stem と同じ ASCII identifier 規則に従う。

`_` 単独、予約語、E02 の reserved std module、`Task` は segment として使えない。

ただし nested user module の最初の segment が `Geometry` のような通常名なら許す。

`Option/Foo.tz` は最初の segment `Option` が予約 std module なので `E1011`。

module name の深さ上限は 16 segment。

segment を `.` で連結した文字列長の上限は 255 byte。

上限超過は `E1017`。

`Main.tz` の選択は変更しない。

入力 directory の直下にある `Main.tz` だけが application entry の root。

`App/Main.tz` は module `App.Main` であり、application entry にはならない。

`run` / native executable build の入力は従来通り root `Main.tz` またはその directory。

### 参照構文

関数参照は完全修飾で `Geometry.Point.distance` と書く。

型参照は `Geometry.Point.Point` のように module path と type name を `.` で連結して書く。

レコード literal も `Geometry.Point.Point { x: 1.0, y: 2.0 }` と書ける。

型クラス参照は `Geometry.Metric.Distance` のように module path と class name を連結する。

active pattern は `Geometry.Patterns.Even` のように module path と recognizer case を連結する。

Phase 1 では `module P = Geometry.Point` alias を導入しない。

Phase 1 では `open Geometry` を導入しない。

Phase 1 では relative path module reference を導入しない。

少ない暗黙規則を維持するため、曖昧な場合は常に完全修飾を要求する。

### `a.b.c` の曖昧性

ローカル束縛 `a` が存在する場合、`a.b.c` は常に field access chain。

ローカル束縛 `a` が存在しない場合、checker は `a`, `a.b`, `a.b.c` の prefix が module path かどうかを調べる。

`a.b.c` が既知の module function `a.b.c` として解決できる場合、関数値として扱う。

`a.b` が module で `c` が function なら `a.b.c`。

`a` が module で `b` が function の場合、`a.b.c` は `(a.b).c` ではなく、関数結果の field access が必要なら `(a.b args).c` と書く。

source 上は `ExprKind::Field(Field(Name(a), b), c)` として parse されてよい。

checker に `flatten_field_path` helper を追加し、module/function/type/class 解決時だけ path へ変換する。

ローカルと module が同名の場合はローカル優先。

これは現行 docs の「ローカル変数がモジュールと同名の場合、`name.field` はローカル値のフィールドアクセスを優先」を維持する。

### 探索規則

driver は入力 directory root から再帰的に `.tz` / `.tt` / `.tc` を探索する。

探索順は path component ごとの byte sort。

file より directory を優先しない。全 path を集めてから normalized relative path で sort する。

隠し directory と隠し file は無視する。

隠しとは path segment が `.` で始まるもの。

symlink は辿らない。

symlink が `.tz` / `.tt` / `.tc` に見える場合は `E1011` で拒否する。

symlink directory は無視ではなく `E1011` で拒否する。

これは cycles と content spoofing を避けるため。

最大探索 file 数は 4096。

最大探索 directory 数は 1024。

上限超過は `E1017`。

旧 `.tzr` は従来通り直接入力なら `E2000`、探索対象外。

### std との関係

E02 の std module は flat reserved names のまま。

`Std.Option` には移動しない。

E03 後も `Option.map`、`Array.sum`、`Debug.print` を使う。

std の内部実装が将来 subdirectory を使う場合でも、public module name は D-07 の表を維持する。

### 評価順序・所有権・ABI

階層 module は名前解決だけの変更。

評価順序、所有権、借用、トラップ、数値意味は変更しない。

public ABI の `tz_<name>` は既存互換を維持する。

export 名は module path を含めない既存仕様のままなので、異なる nested module の `export def value` 同士は `E1001`。

内部 LLVM symbol は module path を含む。

例: `Geometry.Point.distance` は `@tz.fn.Geometry.Point.distance`。

既存 LLVM textual name は `.` を含めて使っているため、同じ形式を維持できる。

将来 `-` などを segment に許さないため quote は不要。

### 前提とする他チケットのインターフェース

E02 の `ModuleOrigin`、std reservation、`stdlib::module_name` を利用する。

E02 の std module reserved table は階層 user module の最初の segment にも適用する。

E01 の private 可視性は module path が違えば別 module として扱う。

### 他チケットへの提供インターフェース

E04 は package dependency を root directory ごとの module namespace として E03 の module path に map する。

G11 は relative path と module name の正規化結果を cache key に使う。

`driver::Project` は user source の `relative_path` と `module_name` を保持する。

```rust
pub struct SourceFile {
    pub path: PathBuf,
    pub relative_path: PathBuf,
    pub name: String,
    pub text: String,
    pub origin: ModuleOrigin,
}
```

`name` は `Geometry.Point`。

`relative_path` は `Geometry/Point.tz`。

`lib::analyze_modules` の test input は `("Geometry/Point.tz", source)` を受け、module `Geometry.Point` にする。

互換の `("Point", source)` は module `Point`。

## 設計

### Phase 1: サブディレクトリ module

`driver.rs` に recursive collector を追加する。

```rust
fn collect_sources(root: &Path) -> Result<Vec<PathBuf>, SourceError>;
```

collector は `symlink_metadata` を使って symlink を判定する。

`fs::metadata` で symlink を辿らない。

directory input の root は入力 directory。

file input の root は file の parent。

file input が nested file の場合も、root はその file の parent ではなく project root を推測しない。

Phase 1 の既定案: file input は従来通りその file の parent を project root とする。

つまり `tsuzuri check src/Geometry/Point.tz` は `src/Geometry` を root とし module `Point` になる。

階層 module を使う場合は project root directory を入力する。

この挙動は docs に明記する。

directory input では root 直下 `Main.tz` を application root とする。

`Project::load` は relative path から module name を作る。

`module_name_from_relative("Geometry/Point.tz") -> "Geometry.Point"`。

拡張子を除いた path segments を検査する。

### Parser

`Parser::qualified_ident` を 1 dot から N dot に拡張する。

```rust
fn qualified_ident(&mut self) -> Result<Ident, Diagnostic> {
    let mut name = self.ident()?;
    while self.eat(&TokenKind::Dot) {
        let member = self.ident()?;
        name.text.push('.');
        name.text.push_str(&member.text);
        name.span = name.span.through(member.span);
    }
    Ok(name)
}
```

type syntax と pattern syntax はこの関数を使うため、`Geometry.Point.Point` を自然に受理する。

expression syntax の `a.b.c` は既存 postfix の field chain のままにする。

`QualifiedFunction` を parser が直接作る既存箇所があれば、N dot 対応にする。

### Checker

`Names.modules` を `BTreeSet<String>` から `BTreeMap<String, ModuleOrigin>` にする。

module prefix 判定 helper を追加する。

```rust
fn field_path(expression: &Expr) -> Option<(Ident, Vec<Ident>)>;
```

`field_path` は `Name(a).b.c` を `[a,b,c]` にする。

`Checker::value_expression` の先頭で、ローカルが first segment に存在しない場合に module function 解決を試す。

最長 prefix module を探し、残り 1 segment が function name なら function 解決する。

残りが 0 segment なら module value は存在しないので `E1002`。

残りが 2 segment 以上の場合は Phase 1 では function ではないため `E1002`。

型クラス method `Module.Class.method` は `Classes::method` の既存 `ExprKind::Field(root, class)` 解析を N segment に拡張する。

`Classes::find(module, name)` は `name` が already qualified の場合、そのまま names map を見る。

`Names::record` は qualified name 全体を key として扱う。

無修飾 fallback は last segment の record/type/class name で alias を作る。

### LLVM

内部 symbol に `Geometry.Point` を含める。

`CheckedFunction::qualified_name()` は `format!("{}.{}", self.module, self.name)` のままでよい。

record type `%tz.record.Geometry.Point.Point` は textual LLVM で有効な文字だけを含む。

export wrapper は既存通り `tz_<function.name>`。

異なる modules の export name collision は従来の `export_names` check で拒否する。

### Phase 2 以降の設計だけ

module alias は Phase 2 以降も既定では導入しない。

必要になった場合は `module P = Geometry.Point` を top-level 宣言として追加するが、E03 Phase 1 の対象外。

`open` は導入しない既定案。

相対 module path は導入しない既定案。

## 実装手順

1. `lib.rs` の module name derivation を path-aware にする。
   - 確認: `analyze_modules(&[("Geometry/Point.tz", "def x :: i64\nfn x = 1")])` の function module が `Geometry.Point`。

2. `driver.rs` に recursive collector を追加する。
   - 確認: root directory with `Geometry/Point.tz` and `Main.tz` を読み、source names が deterministic。

3. symlink/hidden/depth/resource 上限を実装する。
   - 確認: `.hidden/Bad.tz` は無視、`link.tz` は `E1011`、17 segment は `E1017`。

4. `Parser::qualified_ident` を N dot にする。
   - 確認: `def f :: Geometry.Point.Point -> i64` を parse できる。

5. `Names.modules` と module name validation を dotted path 対応にする。
   - 確認: `Bad-Name/Point.tz`、`fn/Point.tz`、`Option/Foo.tz` が `E1011`。

6. `Names::record`、`Classes::find`、`Classes::resolve` を qualified path 対応にする。
   - 確認: `Geometry.Point.Point` 型注釈、record literal、class constraint が通る。

7. `Checker::value_expression` に `field_path` と longest module prefix resolution を入れる。
   - 確認: `Geometry.Point.distance p` が通り、`let Geometry = ...; Geometry.Point.distance` は field access として扱われる。

8. `tests/modules.rs` の nested directory ignored 期待を、recursive loading 期待へ更新する。
   - 確認: 既存 flat module tests が通る。

9. E2E fixture を追加する。
   - 確認: native/WASM × `-O0`/`-O3`、IR 決定性、WASM imports 空。

10. docs/README を更新する。
    - 確認: 「サブディレクトリは探索しません」を削除し、root directory 入力の注意を書く。

## テスト計画

### Rust 受理

`Geometry/Point.tz` の `record Point` と `def distance` を `Main.tz` から `Geometry.Point.distance` で呼ぶ。

`Geometry/Point.tz` の record を `Geometry.Point.Point { ... }` で構築する。

型注釈 `let p: Geometry.Point.Point = ...` が通る。

`Geometry/Traits.tt` の class を `Geometry.Traits.Score` として制約に使う。

`Geometry/Builder.tc` を `Geometry.Builder { return 1 }` として使う。

flat `Point.tz` と nested `Geometry/Point.tz` の同名 record は別型。

無修飾 `Point` は自モジュール優先、次に user unique。

### Rust 拒否

`Geometry/Point.tz` と `Geometry/Point.tt` の併存は `E1011`。

`Geometry/_/Point.tz` は `E1011`。

`Geometry/fn/Point.tz` は `E1011`。

`Option/Foo.tz` は `E1011`。

17 segment path は `E1017`。

symlinked `.tz` は `E1011`。

`Geometry.Point.missing` は `E1002`。

local shadowing で `let Geometry = Callback { ... }; Geometry.Point` が module 解決されないことを確認する。

### E2E

fixture `tests/fixtures/hierarchical`:

`Geometry/Point.tz`、`Geometry/Vector.tz`、`Main.tz` を置く。

`export def distance_sum :: f64 -> f64 -> f64 -> f64 -> f64` が nested module を複数呼ぶ。

期待値は JavaScript `Math.hypot` ではなく独立に `sqrt(dx*dx + dy*dy)` を計算する。

native/WASM × `-O0`/`-O3`。

WASM imports は空。

`--emit llvm` を二回実行して一致。

## ドキュメント

`docs/language.md` の module 節を「サブディレクトリ → dotted module」に更新する。

file input と directory input の root 解釈差を明記する。

`README.md` の CLI 節に `Geometry/Point.tz` の例を追加する。

`docs/architecture.md` の module invariant を更新し、recursive traversal の deterministic order と symlink policy を書く。

E02 の std flat module 方針は変更しないと明記する。

## 受け入れ条件

- [ ] directory input が nested `.tz` / `.tt` / `.tc` を決定的に読み込む。
- [ ] `Geometry/Point.tz` が module `Geometry.Point` になる。
- [ ] `Geometry.Point.distance` が関数値として解決される。
- [ ] `Geometry.Point.Point` が type/record/pattern で使える。
- [ ] ローカル field access が module path より優先される。
- [ ] `Main.tz` entry selection は root 直下のまま。
- [ ] symlink、hidden、depth/resource 上限が仕様通り。
- [ ] std は `Std.*` に移動しない。
- [ ] native/WASM × `-O0`/`-O3` の E2E が通る。

## 落とし穴

parser で `a.b.c` を単一 qualified name に変えると field access が壊れる。

ローカル shadowing を無視すると既存仕様に反する。

file input から project root を推測しようとすると、親方向探索や package marker が必要になり E04 と衝突する。

symlink を辿ると cycle と source spoofing が起きる。

export name に module path を含めると D-17 の `tz_` 互換を壊す。

std を `Std.*` に移すと D-07 と全後続チケットに反する。

## 対象外

`open` 宣言。

module alias。

relative import。

package dependency。

複数ファイル同一 module。

root 推測。

std の namespace 移動。

## 未決事項

file input で nested project root をどう推測するかは未決。既定案は推測せず、directory input を使わせる。

module alias は未決。既定案は導入しない。

`open` は未決。既定案は導入しない。

台帳の見直し提案はない。D-07 の std flat module 方針を維持する。
