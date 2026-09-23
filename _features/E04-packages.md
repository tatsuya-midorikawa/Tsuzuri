# E04: パッケージと依存管理
| 項目 | 内容 |
|---|---|
| ID | E04 |
| 優先度 | P3 |
| 規模 | XL |
| 依存 | E03 |
| 後続 | なし |
| 状態 | todo |
| 主な影響ファイル | `src/driver.rs`, `src/lib.rs`, `src/check.rs`, `src/main.rs`, `src/diagnostic.rs`, `docs/language.md`, `docs/architecture.md`, `README.md`, `tests/modules.rs`, `tests/e2e.mjs` |

## 目的

Tsuzuri プロジェクトを単一ディレクトリから複数 package へ拡張し、ローカル path dependency と将来の git dependency を再現可能に扱う。

E03 の階層 module により module path は表現できるが、別 repository や shared library を明示的に依存として取り込む仕組みはない。

このチケットは package manifest、namespace mapping、lockfile、offline build の方針を定める。

実装は XL なので段階分割し、Phase 1 は local path dependency のみを完全実装する。

## 現状

`src/driver.rs` の `Project::load` は入力 file/directory と sibling/nested sources だけを見る。

`src/lib.rs` の `analyze_modules` は caller が渡した source list だけを扱う。

E03 後の module name は relative path から `Geometry.Point` のように決まる。

E02 後の std modules は compiler に埋め込まれ、user package とは別 origin になる。

`src/main.rs` の CLI は入力 path を一つだけ受ける。

`docs/language.md` は外部 package、検索 path、import 宣言がないと説明している。

出力保護は読み込んだ全 user source に対して働く。

## 仕様

### Manifest format

manifest file は project root の `Tsuzuri.toml`。

Phase 1 は TOML の strict subset を手書き parser で読む。

新しい crate は追加しない。

理由は P3 機能の Phase 1 で dependency supply chain を増やさず、必要な構文が少ないため。

将来 full TOML が必要になったら、`toml_edit` などの導入を別 PR で検討する。

Phase 1 の grammar:

```text
manifest       ::= ws* package-section dependency-section? ws*
package-section ::= "[package]" nl package-field+
package-field  ::= "name" "=" string nl
                 | "version" "=" string nl
dependency-section ::= "[dependencies]" nl dependency-field*
dependency-field ::= bare-key "=" inline-table nl
inline-table   ::= "{" ws* "path" ws* "=" ws* string ws* "}"
bare-key       ::= ASCII identifier with '-' allowed, not empty
string         ::= double-quoted UTF-8 without escape except \" \\ \n \t \r
comment        ::= "#" until line end
```

Phase 1 では `version` は必須だが依存解決に使わない。

`name` は package namespace に使う。

package name は ASCII kebab-case。

module namespace へ mapping するときは kebab segments を PascalCase に変換する。

例: package `geometry-core` は root namespace `GeometryCore`。

明示 namespace field は Phase 1 では導入しない。

### Local path dependencies

Phase 1 では local path dependency だけを実装する。

例:

```toml
[package]
name = "app"
version = "0.1.0"

[dependencies]
geometry-core = { path = "../geometry-core" }
```

dependency path は manifest directory からの相対 path。

dependency root には `Tsuzuri.toml` が必要。

dependency package の source root は dependency root。

dependency の `Main.tz` は application entry として使わない。

dependency package 内の modules は namespace prefix の下に配置する。

`../geometry-core/Point.tz` は user から `GeometryCore.Point` として参照する。

dependency 内部から自 package の module を参照する場合も、Phase 1 では fully qualified `GeometryCore.Point` を使う。

相対 package import は導入しない。

root package の modules は prefix なし。

### Git dependencies and lockfile

Phase 1 では git dependencies を parse error にする。

Phase 2 以降で次を導入する。

```toml
[dependencies]
math-kernels = { git = "https://github.com/example/math-kernels.git", rev = "..." }
```

git dependency は floating branch/tag を許さず、commit `rev` 必須。

lockfile `Tsuzuri.lock` は dependency name、source、resolved commit/path canonical hash、content hash を持つ。

git checkout は build script を実行しない。

package sources の content hash は file path + bytes の BLAKE3 または SHA-256。

Phase 2 の既定案は Rust 標準だけで使える SHA-256 がないため、crate 追加を明示検討する。

offline build は lockfile と local cache だけを使い、network access をしない。

### Security policy

package build scripts は導入しない。

manifest に任意 command を書けない。

dependency source は `.tz` / `.tt` / `.tc` / `Tsuzuri.toml` / `Tsuzuri.lock` だけを読む。

symlink policy は E03 と同じ。

dependency の出力先を source として上書きしない。

同一 physical file が複数 package から読まれる場合も、出力保護は全 source alias を保護する。

### Version resolution

Phase 1 は local path dependency に version resolution を行わない。

同じ dependency name が同じ package の `[dependencies]` に複数出たら manifest parse error。

dependency graph に同じ package name が複数 version/path で出た場合、Phase 1 は `E2000` ではなく source diagnostic `E1011` で拒否する。

理由は module namespace が package name から一意に決まるため。

Phase 2 以降の既定案は semver range resolution ではなく lockfile-first exact version。

### 前提とする他チケットのインターフェース

E03 の module path と recursive traversal を前提にする。

E02 の std origin は package dependency より常に後で読み込む。

E01 の private は package 境界ではなく module 境界にだけ効く。

### 他チケットへの提供インターフェース

G11 は package graph と source content hash を incremental cache key に使う。

E04 は `Project` に package origin を追加する。

```rust
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PackageId {
    pub name: String,
    pub root: PathBuf,
}

pub enum ModuleOrigin {
    User,
    Dependency(PackageId),
    Std,
}
```

E02 の `ModuleOrigin::User` / `Std` から enum を拡張する。

`SourceFile` に `package: Option<PackageId>` を追加してもよいが、`ModuleOrigin` に集約することを推奨する。

## 設計

### Phase 1: local path dependency

`src/package.rs` を追加する。

主要 API:

```rust
pub struct Manifest {
    pub package: Package,
    pub dependencies: Vec<Dependency>,
}

pub struct Package {
    pub name: String,
    pub version: String,
    pub namespace: String,
}

pub struct Dependency {
    pub name: String,
    pub namespace: String,
    pub source: DependencySource,
    pub span: Span,
}

pub enum DependencySource {
    Path(PathBuf),
}

pub fn parse_manifest(source: &str, source_id: usize) -> Result<Manifest, Diagnostic>;
```

manifest parser は `Diagnostic` を返し、path は `Project::source_for` で `Tsuzuri.toml` に対応させる。

manifest diagnostics の code は `E2000` ではなく `E0002` または `E1011` を使う。

syntax error は `E0002`。

invalid package/dependency name は `E1011`。

unsupported key は `E0002`。

unsupported source kind `git` は Phase 1 では `E0002`。

`Project::load` は root directory に `Tsuzuri.toml` があれば package mode に入る。

manifest がない場合は E03 の plain project mode を維持する。

plain project mode と package mode の CLI は同じ。

package mode root package sources は root directory の recursive sources。

dependency package sources は dependency root の recursive sources に namespace prefix を付けて読み込む。

namespace prefix は dependency package name 由来。

root package name は root module prefix に使わない。

dependency graph traversal は DFS だが、最終 source order は package namespace + relative path の BTree order。

cycle detection は package root canonical path と package name の pair で行う。

cycle があれば `E1017` ではなく `E1011` で「cyclic package dependency」を返す。

### Module naming

dependency module name:

```text
<DependencyNamespace>.<RelativeModulePath>
```

例:

```text
../geometry-core/Point.tz -> GeometryCore.Point
../geometry-core/Shapes/Circle.tz -> GeometryCore.Shapes.Circle
```

dependency の own source 内も prefixed module name として check する。

これにより dependency 同士の module name collision を避ける。

dependency package が別 dependency を持つ場合、その dependency も package name namespace で global に配置する。

same package name with different canonical root は Phase 1 で拒否。

same canonical root with same package name は重複読み込みせず共有する。

### Lockfile Phase 1

Phase 1 では `Tsuzuri.lock` を optional にする。

local path dependency だけなら lockfile がなくても build reproducible と見なす。

ただし `--locked` CLI はまだ導入しない。

Phase 2 で git dependency を入れる時に lockfile 必須にする。

### CLI

Phase 1 で新 CLI option は追加しない。

`tsuzuri check .` が `Tsuzuri.toml` を検出すれば package mode。

`tsuzuri build .` も同じ。

root package の `Main.tz` が application entry。

dependency package の `Main.tz` は normal module `<Dep>.Main`。

### Diagnostics path

manifest source も `Project.sources` 相当の diagnostic source として保持する。

`source_for` は `Tsuzuri.toml` の diagnostics を返せる。

dependency source path は実 path を表示する。

std source は E02 通り `std/...`。

## 実装手順

1. `src/package.rs` に strict manifest parser を追加する。
   - 確認: unit tests で valid manifest、comment、string escape、duplicate dependency、unknown key を検査する。

2. `Project::load` で root `Tsuzuri.toml` を検出する。
   - 確認: manifest なし plain project が既存通り動く。

3. `PackageId`, `ModuleOrigin::Dependency` を追加する。
   - 確認: E02 の std origin tests が壊れない。

4. dependency graph loader を実装する。
   - 確認: root app + local `../geometry-core` を読み、dependency source が `GeometryCore.*` になる。

5. package namespace collision と cycle detection を入れる。
   - 確認: same package name different path、A→B→A を拒否する。

6. `Project::source_for` と output protection を dependency source に対応する。
   - 確認: dependency source を output に指定すると `E2003`。

7. check/analyze integration を修正する。
   - 確認: dependency module から std module も参照できる。

8. E2E fixture を追加する。
   - 確認: native/WASM × `-O0`/`-O3`、WASM imports 空。

9. docs/README を更新する。
   - 確認: Phase 1 は local path only と明記する。

## テスト計画

### Manifest parser

valid `[package]` + `[dependencies]` を parse する。

comments と空行を許す。

unknown section は `E0002`。

missing package name は `E0002`。

invalid name `Bad_Name` は `E1011`。

duplicate dependency は `E1011`。

`git = ...` は Phase 1 で `E0002`。

### Package loading

root `Main.tz` が dependency function `GeometryCore.Point.distance` を呼べる。

dependency `Main.tz` は entry にならない。

dependency private helper は package 外から E01 の module private として拒否される。

same dependency を二経路から参照しても canonical root が同じなら一度だけ読む。

same package name different root は `E1011`。

cycle は `E1011`。

dependency source の error path が実 path になる。

### E2E

temporary directory に root package と dependency package を作る。

root package:

```toml
[package]
name = "app"
version = "0.1.0"

[dependencies]
geometry-core = { path = "../geometry-core" }
```

dependency exports no host ABI。

root `export def answer :: f64` が dependency module を呼び、期待値 5.0。

C host と WASM host で確認。

native/WASM × `-O0`/`-O3`。

WASM imports 空。

IR deterministic。

## ドキュメント

`docs/language.md` に package manifest は compile input の機能であり、言語内 import 宣言ではないと書く。

`docs/architecture.md` に package graph loading、security policy、no build scripts を追加する。

`README.md` に local path dependency の最小例を追加する。

将来 git dependency と lockfile は未実装として明記する。

## 受け入れ条件

- [ ] `Tsuzuri.toml` strict subset を parse できる。
- [ ] manifest なし project は既存通り。
- [ ] local path dependency を namespace prefix 付きで読み込む。
- [ ] dependency `Main.tz` は entry にならない。
- [ ] package name collision と cycle を拒否する。
- [ ] build scripts や arbitrary command は存在しない。
- [ ] output protection が dependency source にも効く。
- [ ] native/WASM × `-O0`/`-O3` の package E2E が通る。

## 落とし穴

root package に prefix を付けると既存 project の module name が壊れる。

dependency package 内部を prefix なしで check すると、root package と collision する。

manifest parser を TOML 全体として曖昧に実装すると、後で互換性を保てない。

build scripts を入れると security model が大きく変わるため導入しない。

git branch/tag を lock なしで許すと reproducibility が壊れる。

dependency source を output protection から外すと source overwrite が起こる。

## 対象外

git dependency の実装。

semver range resolution。

registry。

package publish。

build scripts。

features / optional dependencies。

workspace。

source replacement。

`open` / import declarations。

## 未決事項

full TOML parser crate を導入するかは未決。既定案は Phase 1 では手書き subset。

lockfile hash algorithm は未決。既定案は git 導入時に SHA-256 か BLAKE3 crate を明示選択。

package namespace を manifest で上書き可能にするかは未決。既定案は package name 由来で固定。

台帳の見直し提案はない。
