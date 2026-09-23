# G11: 増分ビルド・キャッシュ
| 項目 | 内容 |
|---|---|
| ID | G11 |
| 優先度 | P3 |
| 規模 | L |
| 依存 | 弱依存 E03（Phase 1 は不要。Phase 2/3 では必須。未決事項参照） |
| 後続 | E04（package cache）、G07（LSP 解析 cache 共有） |
| 状態 | todo |
| 主な影響ファイル | `src/driver.rs`, `src/main.rs`, `src/lib.rs`, `src/llvm.rs`, `src/check.rs`, `Cargo.toml`, `build.rs`, `tests/cache.rs`, `tests/cache.mjs`, `README.md`, `docs/architecture.md`, `docs/benchmarks.md` |

## 目的

同じ入力・同じ compiler・同じ options・同じ toolchain での再ビルドを高速化し、CI や editor integration の待ち時間を減らす。Phase 1 は final artifact の whole-build cache に限定し、言語意味と出力保護を変えずに cache hit で byte-identical な成果物を返す。

## 現状

- `src/driver.rs` の `Project::load` は同 directory の `.tz` / `.tt` / `.tc` を読み、`Project::analyze` が毎回 `crate::analyze_modules` を呼ぶ。
- `driver::build` は毎回 LLVM IR を生成し、一時 directory に `module.ll` / object / executable / wasm を作り、成功後に `fs::rename` で output に publish する。
- `driver::tool` は `TSUZURI_CLANG` / `TSUZURI_WASM_LD` を見るが、tool version は key 化されていない。
- `BuildOptions` は `target`, `emit`, `optimization`, `cpu` を持つ。G08 後は `debug_info` も key に含める。
- `llvm::emit_target` は deterministic IR を目指し、`BTreeSet` / `BTreeMap` を使う。cache hit はこの deterministic property を検査する良い回帰テストになる。
- E02 の std 同梱、E03 の subdirectory module、E04 の package graph は未実装。

## 仕様

### Phase 1: whole-build cache

対象:

- `tsuzuri build` と省略形 `tsuzuri source.tz`
- `tsuzuri run` の内部 build

対象外:

- `check`
- `--emit header`（生成が軽く、toolchain に依存しないため Phase 1 では cache しない）
- 解析途中の cache

cache key は次の SHA-256。

| 要素 | 内容 |
|---|---|
| compiler | `CARGO_PKG_VERSION`, git commit hash, target OS/arch, cache format version |
| options | `BuildOptions` 全 field（G08 後の `debug_info` 含む）, input action（build/run）, output emit kind |
| sources | `Project.sources` の path（project 相対、UTF-8 正規化しない byte string）と text bytes。並びは `Project::load` と同じ deterministic order |
| std | E02 後の embedded std source names + bytes。Phase 1 実装時に std がなければ空 list |
| environment | `TSUZURI_CLANG`, `TSUZURI_WASM_LD`, relevant target env vars |
| tools | 実際に使う clang / wasm-ld の `--version` stdout/stderr。`--emit llvm` など外部 tool を使わない emit では含めない |
| runtime | `src/runtime/task.c` / embedded runtime `.ll` bytes（`include_str!` される内容）。compiler binary に含まれるが、cache format 明示のため version と合わせて key に入れる |

cache hit 時:

- artifact bytes を output path へ publish する。
- hit でも `protect_sources(project, output)` を publish 前後に実行する。
- output が source alias の場合は cache を使わず `E2003`。
- `run` は cached executable を temp output に publish して実行する。

cache miss 時:

- 現在の `build` と同じ手順で artifact を作る。
- 成功後に cache へ atomic に保存し、その後 output へ publish する。
- tool failure / compiler error は cache しない。

### CLI

```text
tsuzuri build Main.tz --no-cache
tsuzuri run Main.tz --no-cache
```

- 既定は cache 有効。
- `--no-cache` は build/run 専用。`check` では `E2000`。
- Phase 1 では `--cache-dir` は追加しない。環境変数 `TSUZURI_CACHE_DIR` は未決事項。

### cache location

標準 library のみで決める。

- `TSUZURI_CACHE_DIR` があればそれを使う（Phase 1 で採用する場合）。
- macOS: `$HOME/Library/Caches/tsuzuri/build-cache`
- Linux: `$XDG_CACHE_HOME/tsuzuri/build-cache`、なければ `$HOME/.cache/tsuzuri/build-cache`
- Windows: `%LOCALAPPDATA%\Tsuzuri\Cache\build-cache`
- どれも取れない場合は cache disabled warning（`W2001`）を出して build 継続。

cache entry:

```text
<cache-root>/<key-hex>/
  artifact
  messages.txt
  metadata.txt
```

`metadata.txt` は format version、emit kind、created time、artifact size を含む手書き key-value。JSON crate は追加しない。

### eviction

- 既定上限: 2 GiB または 30 日未使用。
- cache access 時に軽量 GC を行う。ただし build の critical path を長くしないため、1 回の GC で削除する entry は最大 128。
- LRU は `metadata.txt` の `last_used_unix_ms` を更新して管理する。更新失敗は warning とし、build は継続。
- 共有 CI に性能閾値は入れない。

### concurrency / atomicity

- entry は content-addressed なので、同じ key を複数プロセスが同時に build しても正しさは保てる。
- 保存は `<cache-root>/.tmp-<pid>-<counter>` に全ファイルを書き、最後に `fs::rename(tmp, entry)`。
- rename が `AlreadyExists` の場合は他プロセスが勝ったとみなし、tmp を削除して成功扱い。
- 部分 entry（metadata 不足、artifact 不足）は miss として扱い、GC 対象にする。
- cache hit の artifact copy と output publish は既存 output protection を通す。

### hash function

新 crate は追加せず、`src/cache/sha256.rs` に SHA-256 を実装する。

理由:

- final artifact cache の collision は誤った binary を返す correctness bug になる。
- 非暗号学的 128-bit hash は実用上低確率でも、言語処理系の cache key としては人間レビューなしに採用しない。
- SHA-256 実装は小さく、NIST test vectors で検証できる。

実装 API:

```rust
pub struct Sha256 { /* private */ }
impl Sha256 {
    pub fn new() -> Self;
    pub fn update(&mut self, bytes: &[u8]);
    pub fn finalize(self) -> [u8; 32];
}
```

### Phase 2: per-module parse cache

- Key: source file path + bytes + parser version + source kind。
- Value: parsed `Program`。
- 注意: `Program` は `Span.source` を含むため、deserialize 後に source id remap が必要。Phase 2 では file-local span と source id を分ける構造が望ましい。
- E03 後は subdirectory module path を key に含める。

### Phase 3: check / IR cache

- 関数単位や module 単位の cache を検討する。
- 難所:
  - `polymorph::specialize` は whole-program の `(function id, type args)` queue と上限 1,024 に依存する。
  - `call_specialization::Specializations` は whole module の関数値 escape / read-only 解析に依存する。
  - `Classes` の instance coherence と constraints は project 全体。
  - `CheckedModule` の function / record id は source set 全体の deterministic order に依存する。
- したがって Phase 3 は E03/E04 の module graph と stable item id 設計が入ってから着手する。

## 設計

### 追加する主な型

`src/cache.rs`:

```rust
pub struct CacheConfig {
    pub enabled: bool,
    pub root: Option<PathBuf>,
    pub max_bytes: u64,
    pub max_age_days: u64,
}

pub struct BuildCache {
    root: PathBuf,
}

pub struct CacheKey {
    digest: [u8; 32],
}

pub struct CachedArtifact {
    pub artifact: Vec<u8>,
    pub messages: Vec<String>,
}

impl BuildCache {
    pub fn open(config: CacheConfig) -> Result<Option<Self>, Diagnostic>;
    pub fn load(&self, key: &CacheKey) -> Result<Option<CachedArtifact>, Diagnostic>;
    pub fn store(&self, key: &CacheKey, artifact: &Path, messages: &[String]) -> Result<(), Diagnostic>;
}
```

`src/driver.rs`:

```rust
#[derive(Clone, Copy, Debug)]
pub struct BuildOptions {
    pub target: Target,
    pub emit: Emit,
    pub optimization: u8,
    pub cpu: Cpu,
    pub cache: CacheMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheMode {
    Enabled,
    Disabled,
}
```

G08 が `debug_info` を追加している場合は同じ struct に共存させる。

### compiler fingerprint

`build.rs` を追加する。

```rust
fn main() {
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=TSUZURI_GIT_COMMIT={commit}");
}
```

key には `env!("CARGO_PKG_VERSION")` と `env!("TSUZURI_GIT_COMMIT")` を入れる。`unknown` の場合も cache は使えるが、開発 build で古い cache を拾いやすいため `profile=debug` では compiler binary mtime を追加する案を未決事項に残す。

### driver integration

既存 `build` の中を次のように分ける。

```rust
fn build_uncached(...) -> Result<(PathBuf, Vec<String>, TemporaryDirectory), Diagnostic>;
fn publish_artifact(project: &Project, artifact: &Path, output: &Path) -> Result<(), Diagnostic>;
```

ただし temporary directory の lifetime を壊さないよう、実装では `build_uncached` が artifact を cache store / publish する callback を受けてもよい。

流れ:

1. `options.validate()`
2. entry / wasm export checks
3. `protect_sources(project, output)`
4. key 作成
5. cache load
6. hit: temp artifact file に bytes を書き、publish
7. miss: 現在の build path
8. success artifact を cache store
9. publish
10. temp close

cache hit でも output path の parent 作成と atomic publish は miss と同じ helper を使う。

### tool version capture

`run_tool_version(tool: OsString) -> Result<String, Diagnostic>`:

- `clang --version`
- `wasm-ld --version`
- timeout は `Command::output` にはないため、Phase 1 は通常実行。hang が問題になったら G11 follow-up で timeout process 管理を追加。
- version command failure は cache disabled warning にし、build は miss として継続する。ただし実際の compile/link failure は従来通り `E2002`。

### コンパイラ段階ごとの変更

| 段階 | 変更 |
|---|---|
| lexer / parser / check / polymorph / control / closures / ownership | Phase 1 は変更なし |
| llvm | 変更なし。ただし output bytes が key により再利用されるため deterministic tests を追加 |
| runtime | runtime source bytes を key に含める |
| driver | cache key 計算、load/store/publish 分離 |
| main | `--no-cache` parse |
| build system | `build.rs` で commit hash を埋め込む |

## 実装手順

1. **SHA-256**
   - `src/cache/sha256.rs` を実装。
   - 確認: NIST vectors (`""`, `"abc"`, long message)。
2. **cache module**
   - `src/cache.rs` に location, load, store, metadata parser を実装。
   - 確認: temp cache root で store/load/partial entry/concurrent AlreadyExists。
3. **CLI**
   - `--no-cache` と `BuildOptions.cache`。
   - `check --no-cache` は `E2000`。
   - 確認: `parse_arguments` tests。
4. **compiler fingerprint**
   - `build.rs` 追加、key に version/commit。
   - 確認: `tsuzuri --version` は従来表示を維持。unit test で fingerprint string が空でない。
5. **key builder**
   - project sources, options, env, tool versions, runtime bytes を length-prefix 付きで hash。
   - 文字列連結の曖昧さを避け、各 field は `tag + u64 length + bytes`。
   - 確認: source 1 byte 変更、option 変更、tool version 変更で key が変わる。
6. **driver build integration**
   - cache hit path / miss path を統合。
   - `protect_sources` は hit/miss 両方で publish 前後に実行。
   - 確認: cache hit が output を byte-identical に作る。
7. **eviction**
   - max bytes / age の軽量 GC。
   - 確認: small max_bytes test で古い entry が消える。
8. **tests / docs / benchmarks**
   - `tests/cache.rs`, `tests/cache.mjs`。
   - `docs/benchmarks.md` に測定計画を追記。

## テスト計画

- Rust (`tests/cache.rs`):
  - SHA-256 vectors。
  - key determinism。
  - source bytes / options / env / runtime bytes 変更で key 変更。
  - partial entry is miss。
  - atomic store race simulation。
- Node (`tests/cache.mjs`):
  - temporary project を `--emit llvm` で build し、2 回目 hit を確認（stderr warning ではなく metadata / env `TSUZURI_CACHE_DIR` の entry count で確認）。
  - hit artifact が byte-identical。
  - source 変更で miss。
  - `--no-cache` で entry を使わない。
  - output protection: source hard link / source path output は cache hit でも `E2003`。
  - native exe cache hit で実行できる。
- 性能測定:
  - `benchmarks/` に synthetic large project generator を追加（例: 200 modules × small functions）。
  - cold build / warm build の中央値を測る。
  - 共有 CI に速度 pass/fail 閾値を置かない。

## ドキュメント

- `README.md`:
  - cache は既定有効、`--no-cache`。
  - cache location と削除方法。
  - cache hit は semantics を変えないこと。
- `docs/architecture.md`:
  - key に含める要素、atomic write、concurrency。
  - Phase 2/3 の制約。
- `docs/benchmarks.md`:
  - cold / warm build 測定手順、環境、limitations。

## 受け入れ条件

- [ ] 同一入力の 2 回目 build が cache hit し、byte-identical output を生成する。
- [ ] source / options / compiler / tool version / runtime bytes の変更で cache miss する。
- [ ] cache hit でも output protection を迂回しない。
- [ ] `--no-cache` が cache load/store を行わない。
- [ ] cache entry の部分書き込みや同時 store で壊れた artifact を返さない。
- [ ] SHA-256 test vectors が通る。
- [ ] 速度の改善を主張する場合は `docs/benchmarks.md` に測定条件と結果を記録する。

## 落とし穴

- `DefaultHasher` や process-random seed を含む hash は再現性がない。cache key に使わない。
- non-cryptographic hash collision は final artifact cache では correctness bug になる。
- tool version を key に入れないと clang upgrade 後に古い object/exe を返す。
- cache hit path が `protect_sources` を省くと source overwrite 防止を破る。
- output path を key に入れると同じ artifact を別 path に出す cache が効かない。output path は publish 先であり key には入れない。ただし target / emit は入れる。
- G08 の `-g`、G10 の target ABI など後続/並行 ticket の options を key に入れ忘れない。

## 対象外

- Phase 1 での parse / check / per-function IR cache。
- package dependency cache。
- distributed cache / remote cache。
- cache hit 率や build time の CI 閾値。
- LSP incremental analysis との共有。

## 未決事項

- **E03 依存の評価**: README は G11 の依存を E03 としているが、Phase 1 の whole-build cache は現行の flat directory project だけで正しく実装できるため、E03 は hard dependency ではない。E03 は Phase 2/3 で module graph / subdirectory path / stable item id を cache key と invalidation に組み込む時点の hard dependency、と結論づける。README の依存表は `(E03)` の弱い依存へ見直す提案をする。
- **`TSUZURI_CACHE_DIR`**: 既定案は採用する。採用しない場合、tests で user cache を汚さないための injection API が別途必要。
- **debug build fingerprint**: git commit が `unknown` の開発 build で stale cache を避けるため、compiler binary mtime や `option_env!("PROFILE")` を key に入れるか要判断。既定案は profile と version/commit を入れ、commit unknown でも cache を有効にする。
