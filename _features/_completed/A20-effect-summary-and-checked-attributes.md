# A20: 効果の要約と検査付き関数属性（`@no_alloc`・`@no_copy`・`@no_trap`・`@terminates`）

| 項目 | 内容 |
| --- | --- |
| ID | A20 |
| 優先度 | P1 |
| 規模 | XL。Phase 1: L／2: M／3: L／4: L／5: M／6: L／7: M（Phase は上の順に独立して出荷できる） |
| 依存 | [A19](A19-pure-functions.md)（`done` なら**経路 A**、未実装なら**経路 B**で A19 の `pure` を本チケットの基盤の上に実装する。「実装手順」）。G22（診断 v2 の `kind`・`related`・`help`。Phase 2 以降の診断が使う。未実装なら縮退形。「前提とする他チケットのインターフェース」）。弱い依存: [F12](F12-bounds-check-elimination.md)（`src/ranges.rs`。done）、[A15](A15-copy-cost-visibility.md)（`src/copies.rs`。done） |
| 後続 | A21（契約。`pure` と効果の要約を使う）、E15（`@trusted`・`audit`・権限）、G26 Phase 3・4（facts の `effects`・`effect_sites`・`alloc_sites`・`trap_sites`）、B10（`Domain`）、F15（並列化の純粋性）、F14（確保数の表明） |
| 状態 | done（全 Phase。実装と検証は末尾の節） |
| 起票 | 2026-10-18（D-50。確認した HEAD は `main` の `9dd0fbbc`。起票の指示にあった `a2313d19` との差は `docs/benchmarks.md` の 41 行だけ: `git diff --stat 9dd0fbbc a2313d19`） |
| 承認 | 承認済み（D-50: 人間の包括許可 2026-10-18）。決定事項 D1〜D19 はすべて承認済み |
| 改善する劣位 | 確保しない・複製しない・トラップしない・停止するという性質を、署名の側で固定して検査する手段がない（D-50 の調査結論 (2)）。Rust の `#![no_std]` と `no_panic`、C++ の `noexcept`、Ada/SPARK の `Always_Terminates`、D の `@nogc` に当たる保証がなく、性能の約束（確保なし、`@no_alloc`）を人間も AI もコードで表現できない |
| 手本にする既存実装 | 呼び出しグラフ・強連結成分: `src/recursion.rs` の `check_graph`、`src/llvm_escape.rs` の `references`。関数値の実体: `src/call_specialization.rs` の `target`。暗黙の複製: `src/copies.rs` の `sites`。添字の証明: `src/ranges.rs` の `analyze`・`RangeFacts::enter_condition`・`index_in_bounds`。wildcard なしの全 variant 判定: `src/const_rules.rs` の `value_part`。上限と `E1017`: `src/gpu.rs` の `extract_kernel`。属性: `src/parser.rs` の `cpu_attribute`、`src/parse_control.rs` の `checked_expression`、`src/check.rs` の `cpu_levels`・`FunctionOrigin.cpu`、`src/polymorph.rs` の `instantiate`。符号化: `src/syntax_codec.rs`・`src/frontend_cache.rs` の `FRONTEND_FORMAT`。テスト: `tests/copy_inventory.rs`・`tests/bounds_checks.rs`・`tests/constants.rs`・`tests/api_diff.rs`・`tests/formatter.rs` |
| 主な影響ファイル | Phase 1: `src/effects.rs`（新規。`src/check.rs` に `#[path = "effects.rs"] pub mod effects;`）, `src/check.rs`, `src/polymorph.rs`, `tests/effects_summary.rs`（新規）。Phase 2: `src/syntax.rs`・`parser.rs`・`parse_control.rs`・`syntax_codec.rs`・`frontend_cache.rs`・`formatter.rs`・`api_diff.rs`・`check.rs`・`polymorph.rs`、`tests/effects_attributes.rs`（新規）。Phase 3: `src/effects.rs`、`llvm_frame.rs`・`llvm_escape.rs`・`llvm.rs`（再公開だけ）、`tests/effects_alloc.rs`・`effects_alloc_conservative.rs`・`common/ir_scan.rs`・`fixtures/effects_sweep/`（新規）。Phase 4: `src/ranges.rs`（`enter_else_condition`）、`src/driver.rs`（`Project::source_paths`）、`tests/effects_trap.rs`（新規）。Phase 5・6: `src/effects.rs`。Phase 7: `src/docgen.rs`・`semantic.rs`・`api_diff.rs`・`docs/`・`_tsuzuri/language-reference/` |

付録（長い表・実測）は [A20-appendix.md](A20-appendix.md)。本文は「付録 X」でそこを指す。

## 実装者向け早見

- **最初に読む**: この早見 → 「仕様」の 7.1〜7.4 → 「設計」の `src/effects.rs` の節 → 付録 A・B。コードは `src/check.rs` の `TypedExprKind`・`Builtin`・`FunctionOrigin`・`check_modules_collect` の末尾、`src/llvm_escape.rs` の `references`、`src/copies.rs` の `sites`、`src/ranges.rs` の `analyze` の順に開く。
- **最初に書くテスト**: `tests/effects_summary.rs::summary_of_a_scalar_function_is_empty` と、`src/effects.rs` の単体テスト `every_builtin_has_a_verdict`。以降は各手順の「確認」に名前がある。
- **原則**: 効果の要約は**型付き IR を読むだけの解析**。型検査・所有権・特殊化・生成 IR を変えない。`match` は `TypedExprKind` と `Builtin` に wildcard（`_ =>`）を置かない（新しい variant をコンパイラが拒む）。
- **触ってはいけないもの**: `llvm*.rs` の生成（IR は 1 byte も変えない。例外は `pub(crate)` の再公開だけ）、`ranges.rs` の既存の証明規則（足すのは `enter_else_condition` だけ）、A19 の `E1029` の挙動、既存の診断のコードと意味。
- **属性は表明**で、IR を変えない。**属性も `pure` もないプログラムでは `check` が要約を計算しない**（費用 0。D3）。名前・コードは D-55 のとおり固定（属性 `@no_alloc`・`@no_copy`・`@no_trap`・`@terminates`、診断 `E1031`・`E1032`・`W1008`）。新しい CLI・環境変数・予約語・std・crate・`unsafe` はない。
- **止める条件**は次節。特に、IR に確保があるのに規則が `alloc` でない関数が 1 件でも出たら、規則を**保守的な向きにだけ**直す（緩めない）。終わりの条件は「受け入れ条件」。
- 経路 A（A19 が done）か経路 B（未実装）かは、**着手前に `ls src/pure_rules.rs` で決める**（A19 が新規に作るファイル。あれば A）。

## 目的

関数ごとの**効果の要約**（D-53 の 14 個の語彙のビット集合）を型付き IR から推論し、4 つの属性を**表明として検査**する。

```tsuzuri
@no_alloc
@no_trap
def checksum :: ref [ubyte] -> i32u = \bytes ->
    let mut sum = 0i32u
    for i in 0 .. bytes.length - 1 do
        sum = sum * 31i32u + (bytes[i] as i32u)
    sum
```

（属性を外して HEAD で `check`・`run` に通した。`[1, 2, 3]` の値は 1026。試作の判定は効果なし。）

| 属性 | 約束 | 禁じる効果（要約に含まれてはならない） |
| --- | --- | --- |
| `@no_alloc` | ヒープの確保をしない | `alloc`・`extern`、および「未知の呼び出し」 |
| `@no_copy` | 長さに比例する暗黙の複製をしない | `copy`、および「未知の呼び出し」 |
| `@no_trap` | プログラムの論理によるトラップをしない | `trap`・`extern`、および「未知の呼び出し」 |
| `@terminates` | 停止する（構造的な十分条件） | `diverge`・`extern`、および「未知の呼び出し」 |

HEAD で測った事実（付録 H の再現コマンドと付録 E の数字）:

- 型付き IR の式は `TypedExprKind` が **64 variant**、組み込み関数は `Builtin` が **246 variant**（`Builtin::ALL` も 246。`#[cfg(test)]` の 3 件を除くと 243）。`TrapKind` は 16、`Type` は 30 variant。std の公開 `def` は 559 個。
- 生成 IR（`--emit llvm`）は `-O0` と `-O3` で**同一**（`cmp` で確認）。最適化の水準は clang だけが決めるので、「最適化の水準に依らない規則」は **`llvm::emit` が出す IR に対して**保守的であればよい。
- 試作（本チケットの規則の `src/effects.rs`。付録 I）を `tests/fixtures`・`examples` の **102 プログラム・11,910 関数**に当て、`--emit llvm` の IR と突き合わせた: 規則が `alloc` でないのに IR が確保する関数は **0 件**（規則の `alloc` 3,384、IR の確保 2,879。差の 505 件は安全側の偽陽性）。IR の論理トラップのうち規則が拾わなかったものは **0 件**（利用者のコード 1,101 箇所、std・組み込み・生成関数 848 件）。
- 走査の費用（付録 E.3。試作、release、同一機械）: `json` fixture（947 関数）で解析 248〜270 ms に対し走査 35 ms（13%）で、**走査の約 90% は `copies::sites`**。→ 属性も `pure` もないプログラムでは計算しない（D3）。

利用者の価値: 性能を主張するコード（組み込み・オーディオ・ゲームのフレームループ）で、「この関数は確保しない」を**コンパイラが保証**し、AI が書いたコードの約束も実行せずに `check` で確かめられる。

**非目標**: 属性を最適化・IR に使うこと／停止性の**証明**（`@terminates` は構造的な十分条件だけ）／資源の枯渇（確保の失敗・スタックの枯渇・参照カウントの桁あふれ）を `trap` に含めること（D-53）／契約（A21）・権限と `@trusted`（E15）・並列化の保証（F15）／ユーザー定義の属性、`@no_alloc` を `class` のメソッド・`instance`・ラムダ・ローカル関数に付けること。

## 着手条件と停止条件

### 着手条件

- 依存: A19 が `done`（経路 A）か、未実装（経路 B）。G22 は Phase 2 以降の診断で使う。**未実装でも着手できる**（縮退形を使う。後で G22 が入ったときの差し替え手順は「前提とする他チケットのインターフェース」）。
- 基準（GUIDE §2.3）: 手順 1 で `cargo build --release --locked && cargo test --locked` の通過件数と、全 fixture・example の IR を控える（以後の「IR が 1 byte も変わらない」の比較に使う）。`sh scripts/check-runtime-includes.sh` も通す。
- 読む: GUIDE §6.2・§6.5・§6.7・§6.9、D-45・D-51〜D-55、A19 の「検証のアルゴリズム」と「組み込み関数の分類」、本チケットの付録 A・B・C・D。

### 停止条件

次のどれかに当てはまったら、即興で回避せず作業を止めて報告する。

1. **規則が IR より保守的でない**: `tests/effects_alloc_conservative.rs` が、IR に確保（`@tz.alloc`・`@tz.realloc`・`@malloc`・`@realloc`・`@calloc`。付録 F.1 の記号）があるのに規則が `alloc` でない関数を 1 件でも見つけた。選択肢: (A) 規則を保守的な向きに足す（付録 C の表に行を足す。決定表と単体テストを同時に更新）。(B) その構文を `unknown` 扱いにする。**規則を緩めて通さない**。
2. IR の論理トラップ（付録 D の「論理」の種類）が、規則の `trap` の箇所（拒否でも免除でも）に 1 つも対応しない。選択肢: (A) 規則に箇所を足す。(B) 免除の理由を付録 D に足して `exempt` として記録する（理由なしの免除は禁止）。
3. 既存のテストの期待値や、`pure`・属性を使わないプログラムの生成 IR・診断・受理が 1 つでも変わった。
4. 解析のために、型検査・所有権検査・特殊化・`closures::lower` の意味を変える必要が出た。解析は型付き IR を読むだけにする。
5. 主要な std の関数（`Array.map`・`Array.fold`・`Array.sum`・`Array.get`・`Array.length`・`Math.*`・`Int.*`・`String.length`・`Maybe`・`Result` の操作）が、想定外に `alloc`・`trap`・`diverge` と判定された（付録 E の表と食い違う）。判定を緩めずに、連鎖を添えて報告する。
6. `ranges.rs` の既存の証明（`enter_condition`・`index_in_bounds`）を変えないと実装できない、または `enter_else_condition` を足すことで生成 IR が 1 byte でも変わった。
7. 要約の計算が「属性も `pure` もないプログラム」の `check` の時間を増やした（`check` の 2% 以内が目安。`tests/effects_summary.rs::no_attribute_means_no_summary` が計算されないことを固定する）。
8. 上限（到達する関数 4,096・式 2,097,152）を上げたくなった。
9. 新しい予約語・crate・`unsafe`・CLI・環境変数・既定の WASM の import が必要に見えた。
10. 診断のコードが `E1031`・`E1032`・`W1008` で足りない（D-55 の表に追記してから使う）。
11. A19 の経路が決められない（例: `pure_rules.rs` があるのに A19 の既存テストが緑でない）。
12. 同じ失敗を直す試みが 2 回続けて失敗した。

## 現状（HEAD `9dd0fbbc` で確認）

コードの所在（すべて HEAD に存在することを手順 1 の `grep` で確かめる）:

| 何 | 場所 |
| --- | --- |
| 型付き IR | `src/check.rs` の `TypedExpr`・`TypedExprKind`（64 variant）・`FunctionRef`・`CheckedFunction`・`FunctionOrigin`・`CheckedModule` |
| 子の走査 | `TypedExpr::children`（末尾が `_ => Vec::new()` の**非網羅**）・`try_for_each_child`・`try_for_each_child_mut` |
| 組み込み関数 | `src/check.rs` の `Builtin`（246）・`Builtin::ALL`・`name`・`scheme`。生成は `src/llvm.rs` の `emit_builtin`・`emit_typed_builtin` |
| パイプライン | `check_modules_collect` の末尾: `polymorph::specialize` → `closures::lower` → `ownership::check_all_with` → `gpu::validate_calls` → `validate_cpu_functions` → `diagnostics.check()?` |
| 組み込み関数の呼び出し | `closures::lower` が、**組み込み関数の参照をすべて `$builtin` の生成関数へ置き換える**（`Function(User(id))`）。`Call(Function(Builtin(..)))` はその生成関数の本体にだけ残る（`llvm.rs` の `call` が扱う） |
| 暗黙の複製 | `src/copies.rs` の `sites(module)`（`CopySite { function, span, ty, kind, cost }`。`cost` は `Length`・`Environment`） |
| 添字の証明 | `src/ranges.rs`（`mod ranges;`。`lib.rs` で非公開）の `analyze(module, function, intervals) -> RangeFacts`。`index_in_bounds` は `Type::Array` の添字だけを証明する。**LLVM に依存しない**（`llvm_ranges.rs` は `nsw`/`nuw` の書き込みだけ） |
| トラップの箇所 | `src/trap.rs` の `TrapKind`（16）。IR 側の発行は `FunctionEmitter::guard`・`emit_trap`（`src/llvm.rs`）。`--trap-info` の表は `llvm::emit_with_trap_info` |
| 属性 | `@cpu` は `Program.cpu_attributes: Vec<CpuAttribute>` に入り（`parser.rs` の `cpu_attribute`）、`check.rs` の `cpu_levels` から `FunctionOrigin.cpu` になる。`@literal`・`@alias`・`@checked`・`@json` が他の 4 つ（計 5） |

再現（`$WORK` はリポジトリの外の作業ディレクトリ。出力は実測）:

```sh
mkdir -p $WORK/attr && cd $WORK/attr
printf '@no_alloc\ndef f :: i64 -> i64 = \\n -> n + 1\n\ndef main :: unit -> i32 = \\() -> 0\n' > Main.tz
tsuzuri check . ; echo "exit=$?"
```

```text
./Main.tz:1:1: error[E0002]: expected an expression; '@checked' is the only attribute of an expression
  1 | @no_alloc
    | ^
exit=1
```

`copy` 効果の源の既存の報告は `--warn implicit-copy` の `W1006`（`./Main.tz:3:42: warning[W1006]: implicit copy of an array allocates and copies every element; borrow it with 'ref', or call 'Array.copy' to make the copy explicit`）。

IR の確保・トラップの実態は付録 C.2（`alloc`）・付録 D（`trap`）・付録 H.3〜H.5（実測のプローブ）。要点: フレームに載るリテラルは確保でなく、外へ出る・64 KiB を超える・再帰関数やタスクの中の連結と環境はヒープ。除数が定数の `/` と、網羅された `match` の `PatternMismatch` は、IR に守りが残るが論理でない（到達不能）ので免除する。

## 変えてはいけない意味

- `pure` も `@no_*`・`@terminates` も使わないプログラムの受理・拒否・診断・生成 IR・WASM の import は 1 byte も変わらない。属性の名前は**属性としてだけ**読み、`no_alloc` という名前の変数・関数・フィールドは今までどおり使える（`@` の直後だけが属性）。
- 属性を付けたプログラムも、**属性を外したプログラムと生成 IR が 1 byte も同じ**（検査だけ）。`FunctionOrigin` に足すフィールドは、`llvm*.rs` が読まない。
- `E1029`（A19）、`E1017` の既存の意味、既存の診断のコードとメッセージ。型検査・所有権・借用・特殊化・`closures::lower` の規則。
- `src/ranges.rs` の既存関数の挙動（`enter_condition`・`index_in_bounds`・`analyze`）。足すのは `enter_else_condition` だけで、`llvm*.rs` から呼ばない。
- 生成 IR・診断の順序・`related` の順序は決定的（集合は `BTreeSet`・`BTreeMap`・整列した `Vec`）。診断の既存の 5 キー（G22 の追加キーは値があるときだけ）。facts の schema は `tz-facts/1` のまま（足すだけ）。

## Phase 分割

| Phase | 内容 | 規模 | 前提 | 単独で出荷できる理由 |
| --- | --- | --- | --- | --- |
| 1 | 効果の要約の基盤: `Effect`・`EffectSet`・分類表・走査・強連結成分・`CheckedModule.effects`。経路 A は A19 の `pure` の走査を置き換え、経路 B は A19 の `pure` をこの基盤の上に実装する。新しい検査・診断は足さない。計算するのは 10 個（A19 の源）と「未知」 | L | A19 | 属性も `pure` もないプログラムは費用 0。A19 の全テストが緑 |
| 2 | `@no_copy`: 4 属性の構文・`FunctionOrigin.asserts`・`copy`（`copies::sites`）・`E1031`・`E1032`・`W1008`。ほかの 3 属性は `effect_attribute_unavailable` | M | 1 | 属性を使わないプログラムは不変 |
| 3 | `@no_alloc`: `alloc` の規則（付録 C）・`tests/effects_alloc_conservative.rs` | L | 2 | 同上。規則は IR より保守的（テストが固定） |
| 4 | `@no_trap`: `trap` の規則（付録 D）・`enter_else_condition` | L | 3 | 同上 |
| 5 | `@terminates`: `diverge` の規則（再帰・`while`・ブロッキングの組み込み関数。未知は Phase 1 から） | M | 4 | 同上 |
| 6 | `calls_params`: 関数型の引数を呼ぶ関数を、呼び出し地点の実引数の効果で検査する | L | 5 | それまで、関数値を取る関数は「未知」で拒否される（保守的）。Phase 6 は精密にするだけ |
| 7 | ツールと文書: docgen・hover・api-diff・facts の行・文書 | M | 5（6 は任意） | 表示と文書だけ |

経路 A: Phase 1 の手順 7 で `pure` を新基盤へ移す。経路 B: 手順 7 で A19 の手順 5（検証器）を本チケットの形で実装する。**着手する Phase は人間が指示する（指示がなければ Phase 1 だけ）。**

## 仕様

### 7.1 語彙と `EffectSet`

D-53 の 14 個で閉じている。ビットの順序と出力の順序は**名前の辞書順**（D-52 の `effects` と同じ）。

| ビット | 名前 | 何をしうるか | 源（Phase） | A19 の `pure` が禁じる |
| --- | --- | --- | --- | --- |
| 0 | `alloc` | ヒープの確保 | 7.3 の A（3） | — |
| 1 | `atomic` | 原子操作・`Mutex`・`Channel`・`ArenaNextId` | 組み込み関数（1） | ○ |
| 2 | `copy` | 長さに比例する暗黙の複製 | `copies::sites` の `Length`（2） | — |
| 3 | `debug` | `DebugPrintString`・`BenchConsume`（`Debug.__print_string`・`Bench.consume`） | 組み込み関数（1） | ○ |
| 4 | `diverge` | 停止しないかもしれない | 7.3 の V（5） | — |
| 5 | `extern` | `extern` の呼び出し | `HostCall`（1） | ○ |
| 6 | `gpu` | `Gpu.*` | 組み込み関数（1） | ○ |
| 7 | `io` | コンソール・`IO`・`Async` | 組み込み関数（1） | ○ |
| 8 | `net` | `Net.*` | 組み込み関数（1） | ○ |
| 9 | `os` | `File`・`Dir`・`Env`・`Time`・`Random`・`Process`・`Os.__*`・`Bench.now`・`Gen.__seed` | 組み込み関数（1） | ○ |
| 10 | `rc` | `Rc`・`Arc` | 組み込み関数（1） | ○ |
| 11 | `task` | `Task`・`Parallel` | 式・組み込み関数（1） | ○ |
| 12 | `trap` | 論理によるトラップ | 7.3 の T（4） | — |
| 13 | `user_drop` | 利用者の `Drop` の実行 | 式の型（1） | ○ |

`pure` が禁じる集合 `PURE_FORBIDDEN` は○の 10 個（A19 の源。件数 90 の組み込み関数は付録 B で、`dyn` の 1 個は「未知」へ）。各 Phase で計算できるビットが `SUPPORTED`（設計）で、`FunctionEffects.computed` はその部分集合: Phase 1 は○の 10 個、Phase 2 で `copy`、3 で `alloc`、4 で `trap`、5 で `diverge`。計算していないビットは常に 0 で、facts の `effects_complete: false` が示す（G26）。

### 7.2 要約の意味

関数 `f`（`module.functions` の添字）の要約 `Summary { effects: EffectSet, unknown: Option<Unknown>, calls_params: u64 }`:

- `effects(f)` = `f` の本体の**直接の源**と、`f` から**参照辺**でたどれる全関数の直接の源の和集合。辺は `Function(User(id))` と `Closure(id, _)` の**出現のすべて**（呼び出しでなくても。A19 の落とし穴）。組み込み関数の参照は `closures::lower` が `$builtin` の生成関数へ置き換えるので辺になり、その本体の `Function(Builtin(b))` が分類表（付録 B）を引く。
- 強連結成分の中の関数は**同じ集合**を持つ（互いに到達できるため。固定点の反復は要らない）。`diverge` は環のある成分の全関数が持つ。
- `unknown(f)` = 到達する関数のどれかが `dyn` か未解決の式を持つ、または（`f` が**開いている**かつ到達する関数のどれかが関数値を呼ぶ）。**開いている** = いずれかの引数の型が `types.reaches(ty, 関数型 | Dyn | Task | Owned.Function)`。根拠（D5）: 閉じた関数では、実行中に呼べる関数値はすべて到達する本体で作られ（その参照が辺になる）、外から入る道がない。
- Phase 6 で、`unknown` のうち「開いている引数を呼ぶ」ものを `calls_params`（呼ばれる引数の添字のビット集合）へ精密にする。Phase 5 までは `calls_params == 0`。
- 関数の**証拠**（`Witness`）: 効果 `e` ごとに `Direct { span, why }`（自分の本体）か `Via { callee, span }`（`span` は呼び元の中の参照の位置）。同じ成分では、直接の源を持つ関数から呼び元へ幅優先で `Via` を伝え、**鎖が必ず直接の源に着く**。位置は span の小さいもの（`(source, start)` の順）。

型の述語（式ごとに判定。利用者の `Drop` を持つ型があるプログラムだけ走査する）: `user_drop` は、式の型が `types.stored_all(ty, |t| !t.has_user_drop(types))` で偽になるとき。さらに `module.user_drops` の Drop 関数への辺を足す（`Drop.drop` の本体の効果が流れる）。

### 7.3 効果の源の決定表（全 variant・全組み込み関数は付録 A・B）

走査は式ごとに次の**文脈**を持つ: `read`（その場に残り、借用だけで読まれる位置）、`pattern`（`match` の腕の `steps`・`bindings` の中）、`If` の `then`/`else` での `RangeFacts` の入退場（`enter_condition`・`enter_else_condition`／`leave_condition`。`llvm.rs` の `If` と同じ順序: 条件を訪れ、`enter`、`then`、`leave`、`enter_else`、`else`、`leave`）。

**`alloc`（Phase 3）— 最適化の水準に依らない言語の規則（D7）**

| # | 型付き IR の形 | 結果 | 理由（IR の実測） |
| --- | --- | --- | --- |
| A1 | `NewArray`・`NewList`・`NewLiteral` | `alloc` | `new` は常にヒープ |
| A2 | `Closure(_, captures)` で `captures` が非空、または式の型が `Task` | `alloc` | 環境のスタック配置は `reserve_buffer` 次第（再帰関数・タスク・予算 4,096 超でヒープ） |
| A3 | `Call(callee, args)` で `callee.ty` が関数型で `args` が引数より少ない（部分適用） | `alloc` | 環境を作る |
| A4 | 文字列の `Binary(Add)`・`Interpolated`・`StructuralDisplay` | `alloc` | 連結・組み立て |
| A5 | `Construct` で payload があり、型が再帰的な union | `alloc` | ノードはヒープ |
| A6 | 型が `Array`／`List` の非空のリテラル（`Array`・`List`）と、非空の文字列リテラル（`String`） | **その場**なら `alloc` でない。そうでなければ `alloc` | 外へ出るとき（返す・値渡し・パターンで取り出す）ヒープへ昇格。空リテラルは確保しない |
| A7 | 「その場」= (a) 親が読み取りの位置、または (b) 不変の `Block` 束縛の初期化子で、その局所が `llvm::staying_locals` に入る。**かつ** `llvm::frame_bytes(式) ≤ MAX_VALUE_BYTES`（65,536） | A6 を満たす | `llvm_frame.rs`。72 KB のリテラルはヒープ |
| A8 | `copies::sites` の箇所（`cost` が `Length` でも `Environment` でも） | `alloc`（`Length` は `copy` も） | 暗黙の複製は確保 |
| A9 | `TaskRun`・`TaskParallel`・`TaskParallelResults`・`Parallel(..)` | `alloc`（と `task`） | |
| A10 | 組み込み関数 | 付録 B の列 | |
| A11 | 上以外 | `alloc` でない | レコード・タプル・非再帰 union・固定長配列・スカラー・借用・スライス（ビュー）・空リテラル・`Cast`・`Field`・`Index` の読み出し |

「読み取りの位置」（A7）: `Borrow(_, false)`・`BorrowOperand`・`Length`・`StringLength`・`Index` の基底・`Slice` の `value`・`ForEach` の `source`・比較演算の両辺・`Call` の `callee`。`Record`・`Tuple`・`Array`・`List` の要素、`If` の両腕、`Block` の `result`、`Match` の腕の本体は、親の `read` を引き継ぐ。それ以外の位置は `read = false`（`Call` の引数・`Match` の `value` を含む）。

**`trap`（Phase 4）— 論理によるトラップだけ（D8）**

| # | 型付き IR の形 | 結果 |
| --- | --- | --- |
| T1 | `Binary(Divide \| Remainder)` で整数 | `trap`。**免除**（`literal-divisor`）: 除数が `Int(v)`、`v ≠ 0`、かつ符号なし、または `v` が全ビット 1 でない |
| T2 | `Binary(Power)` で整数かつ**符号付き** | `trap`（負の指数）。免除: 指数が `Int(v)` で `v < 2^(bits-1)`。符号なしは免除 |
| T3 | `Index(a, i)` | `a.ty` が `FixedArray`: `Int(v)` かつ `v < N` なら免除（`fixed-literal-index`）。`pattern` 文脈の `Int` 添字は免除（`pattern-projection`。直前の長さの `Test` が保証）。`a.ty` が `Array`: `RangeFacts::index_in_bounds` が真なら免除（`proven-index`）。それ以外（`Vec`・`List`・`String`・`Utf8String`、証明できない `Array`）は `trap` |
| T4 | `Slice` | `trap`（範囲の証明は `Slice` に及ばない） |
| T5 | `ForRange` | `trap`（刻みが 0）。免除: 刻みが `Int(v)` で `v ≠ 0`（`literal-step`） |
| T6 | `NewArray(len, _)`・`NewList(len, _)` | `trap`（負の長さ）。免除: `len` が `Length`・`StringLength`、または `Int(v)` で `v < 2^63` |
| T7 | `Checked(_)` | `trap`（`Overflow`）。`try` の内側でも免除しない（保守） |
| T8 | `Reraise` | `trap` |
| T9 | `ListTail` | 免除（`pattern-projection`） |
| T10 | `Match`・`Try`・`RaisedException` | `trap` でない（網羅は `E1021` が保証。IR の `PatternMismatch` は到達不能） |
| T11 | `HostCall` | `trap` は付けず、免除の箇所 `host-call` を記録（`extern` が表明を禁じる） |
| T12 | 組み込み関数 | 付録 B の列（`assert`・`unreachable`・`Array.set`・`Int.clamp`…）。IR にトラップがあるが論理でないもの（ホストの状態検査・参照カウントの桁あふれ・到達不能な守り）は `exempt` の理由を持つ |
| T13 | `TrapKind` 16 個の分け方（各発行元は付録 D.1） | 論理 9: `Assert`・`IntegerDivisionByZero`・`IntegerDivisionOverflow`・`BoundsCheck`・`RangeStepZero`・`MatchFailure`（`unreachable`）・`Overflow`・`NumericRuntime`（負の指数・`Int.clamp` の条件）・`Encoding`（`Utf8Char.of_u32_unchecked`・`Utf8String.decode_at`）。資源 6（`trap` に含めない）: `AllocationSize`・`AllocationFailure`・`StringConcatOverflow`・`StackOverflow`・`DebugOutput`・`WasmRuntime`。到達不能 1: `PatternMismatch` |

**`copy`（Phase 2）**: `copies::sites` の `cost == Length` の箇所。明示の `Array.copy`・`List.copy`・`clone_string` は `copy` でない（W1006 と同じ。`alloc` ではある）。

**`diverge`（Phase 5）**: V1 `While`。V2 強連結成分が環（大きさ 2 以上、または自己辺。自己末尾再帰を含む）→ 成分の全関数。V3 未知の呼び出しは `unknown`（`@terminates` が禁じる）。V4 付録 B の `diverge` 列の組み込み関数（14 個: 外部の事象をタイムアウトなしで待つもの）。V5 `ForRange`（有界）・`ForEach`（配列・リスト・`Vec`・文字列だけ。`Seq` の `for` は `While` に下がる）・`Match`・`If` は `diverge` でない。

**未知の呼び出し（`unknown`）**: `dyn`（`DynDispatch`・`DynOf`）、未解決の式（`Error`・`GenericFunction`・`Method`・`TypeFunction`・`GenericInteger`・`GenericFloat`・`Lambda`・`CaseConstructor`。成功した `check` の後には残らない。既定で保守）、関数値の呼び出し（`Call` の callee が `Function`・`Closure` でない／`Binary(Pipe)` の右辺が `Function`・`Closure` でない／`callbacks` 列が ○ の組み込み関数（`Function(Builtin)` と `Parallel`）／`TaskRun`: 冷たい `Task` の実行）。

### 7.4 属性

```text
attribute      ::= "@no_alloc" | "@no_copy" | "@no_trap" | "@terminates"
declaration    ::= [doc] { attribute | cpu-attribute } ["private"] ["export"] ["const" | "pure"] "def" ["rec"] name "::" signature
```

- 4 つの属性と `@cpu [...]` は**任意の順序**で並べられ（改行は任意）、各属性は 1 回だけ。`def` の直前（`private`・`export`・`const`・`pure` の前）。`fn`・`and`・`let` の実装は、署名の属性を引き継ぐ（`and` の署名の属性は各自）。読み込み口は D-53 の `attributes()` の 1 か所で、E15 の `@trusted` もこの口に足される。`@no_*` は `pure def` と併用できる（`@cpu` との併用だけが `E0002`。A19 D12）。
- `no_alloc`・`no_copy`・`no_trap`・`terminates` は予約語でない。`@` の直後の名前としてだけ読む。`@` の直後が上の 4 つ以外の名前で、トップレベルなら従来どおり `E0002`。
- **意味**: 属性は表明。要約を置き換えず、推論した要約が禁じる効果（目的の表）を含まないことを検査する。特殊化した関数ごとに検査する。
- **置き場の誤り（`E1032`）**: ① 属性の次が `def` でない（`record`・`union`・`type`・`const`（値）・`fn`・`class`・`instance`・式）／`@literal` の定数／`export` だけ。② 同じ属性の重複。③ `extern def`・`extern type`（本体がない）。④ ラムダ・ローカル関数・`class` のメソッド・`instance` のメソッド（その位置では属性は読まれず、①になる）。式の位置の `@no_alloc 式` は `E1032`（今は `E0002`）。
- **ジェネリックな関数**: インスタンスごとに検査し、診断は (宣言, 属性, kind, 証拠の位置) で 1 件に集約。インスタンスが 1 つもなければ検査されない → `W1008`（`effect_attribute_unchecked_generic`）。
- **まだ検査できない属性（`E1032`）**: 属性の効果（`@no_alloc` は `alloc`、`@no_copy` は `copy`、`@no_trap` は `trap`、`@terminates` は `diverge`）を `effects::SUPPORTED` が含まないとき `effect_attribute_unavailable`。Phase 2 の出荷時は `@no_copy` だけが使え、Phase 3・4・5 が順に外す。**検査されない保証を黙って受理しない**（D19）。
- **`const def`**: 付けられる。実行時の本体を検査する。`W1008`（`effect_attribute_on_const_def`）を 1 件出す。
- `@cpu` と併用できる（`@cpu` の各版は同じ関数の別の機械語で、言語の規則は同じ）。

### 7.5 診断

| コード | kind | 条件 | 主スパン | メッセージの雛形（英語・小文字始まり・ピリオドなし） |
| --- | --- | --- | --- | --- |
| `E1031` | `effect_alloc_in_no_alloc` | `@no_alloc` の要約に `alloc` | 最初の証拠（同じ家族の中。外の関数の証拠は、家族から外へ出る最初の呼び出し式） | `'{fn}' is marked '@no_alloc' but {because}; {help}` |
| `E1031` | `effect_copy_in_no_copy` | `@no_copy` に `copy` | 同上 | `'{fn}' is marked '@no_copy' but {because}; {help}` |
| `E1031` | `effect_trap_in_no_trap` | `@no_trap` に `trap` | 同上 | 同上（`@no_trap`） |
| `E1031` | `effect_diverge_in_terminates` | `@terminates` に `diverge` | 同上 | 同上（`@terminates`） |
| `E1031` | `effect_extern_in_attribute` | `@no_alloc`・`@no_trap`・`@terminates` に `extern` | 家族から外へ出る最初の呼び出し式 | `'{fn}' is marked '@{attr}' but this call reaches the extern function '{name}', whose effects are not known; remove the call or the attribute` |
| `E1031` | `effect_unknown_callee` | いずれかの属性で `unknown` | 未知の呼び出し式 | `'{fn}' is marked '@{attr}' but it {why_unknown}; pass a named function or remove the attribute` |
| `E1032` | `effect_attribute_misplaced` | 7.4 の ①・④ | 属性の `@`（名前まで） | `'@{attr}' applies to the 'def' signature of a top-level function` |
| `E1032` | `effect_attribute_duplicate` | 7.4 の ② | 2 つ目の属性 | `'@{attr}' is written twice; remove one` |
| `E1032` | `effect_attribute_on_extern` | 7.4 の ③ | 属性 | `an extern function has no body to check; remove '@{attr}'` |
| `E1032` | `effect_attribute_unavailable` | 7.4 の「まだ検査できない属性」 | 属性 | `'@{attr}' cannot be checked by this compiler yet; remove it` |
| `W1008` | `effect_attribute_unchecked_generic` | インスタンスなし | 属性 | `'@{attr}' on '{fn}' is checked for each instantiation, and nothing instantiates it` |
| `W1008` | `effect_attribute_on_const_def` | `const def` | 属性 | `a const def is evaluated at compile time; '@{attr}' only checks its run-time body` |

終了コード: `E1031`・`E1032` はエラー（`check`・`build`・`run` が 1）。`W1008` は警告で、既定で表示され、検査を失敗させない（終了コード 0。`module.warnings`）。

`Why.code` の唯一の一覧とプレースホルダーの定義（`{because}`・`{what}`・`{help}`・`{why_unknown}`・`{fn}`・`{attr}`・`{callee}`・`{name}`・`{chain}`）、`{what}`・`{help}` の表（効果と `why` で決まる）は**付録 J**。`{because}` は、証拠が家族の中なら `this expression {what}`、外なら `this call reaches '{callee}', which {what}{chain}`。

`related`（G22。無ければ省略）: 呼び出しの連鎖を `ViaCall` で 1 件ずつ（最大 16。メッセージ `calls '{callee}'`、位置は呼び元の中の参照）、属性の位置に `RequiredBy`（`required by this attribute`）。`help` は上の表。`fixes` は空（機械的な安全な書き換えがない）。G22 が未実装の縮退: `related`・`help`・`kind` を省き、メッセージの末尾に ` (through 'A' -> 'B')` を足す（A19 の `{chain}`。最大 4 個）。診断は 1 つの宣言・属性につき**最初の 1 件**（ソース順）。

例（`E1031` 3 種・`W1008`・JSON。想定出力）は付録 J.4。

### 7.6 facts への出力（G26 の `effects`・`effect_sites`）

`FunctionEffects::rows(module)` が関数ごとに `EffectRow`（`id`・`effects`・`unknown`・`sites`・`exempt_sites`・`complete`。形は付録 J.5）を返し、G26 が `facts/<Module>.jsonl` へ写す。生成は決定的。

### 7.7 上限・決定性・互換

- 上限: 走査する関数 4,096・式の総数 2,097,152（A19 D11 と同じ）。超えたら `E1017`（`an effect summary reaches more than {n} functions or {m} expression nodes; split the function`）。保証なので、超えても黙って受理しない。
- 走査は明示のスタック（再帰しない。GUIDE §11.1）。強連結成分も反復（`recursion::check_graph` と同じ Kosaraju）。
- 互換（`docs/stability.md`）: `E1031`・`E1032`・`W1008` は新しいコードで、既存のコードの意味を変えない。IR・ABI・WASM の import は変わらず、edition に依らない（D-45・D-54）。属性を足すのは互換、外すのは非互換（api-diff）。`FRONTEND_FORMAT` を 4 から 5 へ上げる（A19 が先なら 6 以降。**先に入った方が上げる**）。構造体リテラルの追従: `CheckedModule` の構築 3、`Program` の構築 2・分解 2。

## 設計

### 新規ファイル `src/effects.rs`（`check` の子モジュール）

`src/check.rs` の `#[path = "recursion.rs"] mod recursion;` の隣に `#[path = "effects.rs"] pub mod effects;` を足す（`TypeContext::stored_all` が `pub(super)` なので `check` の子にする。A19 の `pure_rules` と同じ。公開名は `tsuzuri::check::effects`）。型と関数（名前と型を変えない。実装の全文は付録 I の試作を手本にする）:

```rust
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
#[repr(u8)]
pub enum Effect { Alloc, Atomic, Copy, Debug, Diverge, Extern, Gpu, Io, Net, Os, Rc, Task, Trap, UserDrop } // 辞書順
impl Effect { pub const ALL: [Effect; 14]; pub fn name(self) -> &'static str; pub fn from_name(&str) -> Option<Effect> }

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct EffectSet(u16); // EMPTY, PURE_FORBIDDEN, contains, insert, union, intersects, iter()（辞書順）, names()

pub enum Unknown { Dyn, Unresolved, ValueCall }
pub struct Why { pub code: &'static str, pub builtin: Option<Builtin> } // code は付録 J.1 の名前
pub enum Witness { Direct { span: Span, why: Why }, Via { callee: usize, span: Span } }
pub struct Summary { pub effects: EffectSet, pub unknown: Option<Unknown>, pub calls_params: u64 }

pub struct Options { pub scope: Scope, pub computed: EffectSet, pub sites: bool }   // sites が真のときだけ Site を残す（テストと facts）
pub enum Scope { Reachable, All }            // Reachable: 属性・pure を持つ関数から到達できる関数だけ
pub struct Site { pub effect: Effect, pub span: Span, pub why: &'static str, pub exempt: bool } // 自分の本体の直接の源。拒否も免除も。why は付録 J.1 の名前
pub struct FunctionEffects { pub summaries: Vec<Summary> /* 添字は module.functions */, pub computed: EffectSet, /* 非公開: locals, witnesses, sites */ }

pub(crate) struct BuiltinFacts { pub effect: Option<Effect>, pub alloc: bool, pub trap: bool, pub diverge: bool,
                                 pub callbacks: bool, pub dynamic: bool, pub exempt_trap: Option<&'static str> }
pub(crate) fn builtin_facts(builtin: Builtin) -> BuiltinFacts;      // wildcard なしの match。付録 B の 37 行がそのまま arm
pub const SUPPORTED: EffectSet;                                     // この版が計算できる効果。Phase 1: PURE_FORBIDDEN。Phase 2 で Copy、3 で Alloc、4 で Trap、5 で Diverge を足す
pub fn forbidden(kind: EffectKind) -> EffectSet;                    // 7.4 の禁じる集合（`unknown` は別）
impl Options {                                                      // 構造体のリテラルは書かない（E15 が `trusted` を足すため）。A21・E15 は `new` を使う
    pub fn checking(module: &CheckedModule) -> Options;             // scope: Reachable、sites: false、computed: SUPPORTED ∩ (pure があれば PURE_FORBIDDEN ∪ 各属性の forbidden)
    pub fn new(scope: Scope, computed: EffectSet) -> Options;       // sites: false
}
pub fn needed(module: &CheckedModule) -> bool;                      // origin.pure か origin.asserts が 1 つでもあるか
pub fn summarize(module: &CheckedModule, options: Options) -> Result<FunctionEffects, Diagnostic>; // E1017。summarize_with(.., Limits::default()) を呼ぶ
pub(crate) struct Limits { pub functions: usize, pub nodes: usize }  // 既定 4_096 と 2_097_152。単体テストが小さい値で境界を測る
pub(crate) fn summarize_with(module: &CheckedModule, options: Options, limits: Limits) -> Result<FunctionEffects, Diagnostic>;
pub struct Validation { pub errors: Vec<Diagnostic>, pub warnings: Vec<Diagnostic> }
pub fn validate(module: &CheckedModule, effects: &FunctionEffects) -> Validation; // errors: E1031・E1032（unavailable）、warnings: W1008
impl FunctionEffects { pub fn summary(&self, id: usize) -> &Summary; pub fn witness(&self, id: usize, e: Effect) -> Option<&Witness>;
                       pub fn chain(&self, id: usize, e: Effect) -> Vec<(usize, Span)>; pub fn rows(&self, module: &CheckedModule) -> Vec<EffectRow>;
                       pub fn local(&self, id: usize) -> EffectSet;   // 呼び先を含めない。自分の本体の、免除でない直接の源の効果の和
                       pub fn sites(&self, id: usize) -> &[Site]; }  // (source, start) の昇順。Options.sites が偽なら空
```

`BuiltinFacts` の `#[cfg(test)]` の 3 variant（`TestAdd`・`TestUnsigned`・`TestWiden`）は `#[cfg(test)]` を付けた arm にする（通常のビルドでは 243 個）。

### アルゴリズム `summarize`

1. **根**。`Scope::Reachable`: `origin.pure` か `origin.asserts != 0` の関数。`Scope::All`: すべて。根がなければ空の結果を返す。
2. **文脈**（1 回だけ作る）。`computed` に `Copy`／`Alloc` があれば `copies::sites(module)` を関数ごとに分ける。`module.user_drops` が空でなければ `user_drop` の判定を有効にする（型ごとに `BTreeMap<Type, bool>` で保存）。`Trap` が `computed` にあれば、関数ごとに `crate::ranges::analyze(module, function, false)`。
3. **局所の走査**（関数ごとに 1 回。到達した順に行う）。明示のスタックに `Item::Node(&TypedExpr, Ctx)`・`Item::Enter(&TypedExpr)`・`Item::EnterElse(&TypedExpr)`・`Item::Leave` を積み、`Node` を取り出すごとに (a) `user_drop`、(b) `match &expression.kind`（**wildcard なし**。付録 A）で直接の源・辺・未知・免除の箇所を `Local { effects, first: [Option<(Span, Why)>; 14], edges: Vec<(usize, Span)>, value_call, unknown, sites: Vec<Site>, nodes }` に記録し、(c) 子を、文脈（`read`・`pattern`）を決めて積む。`If` は `[condition][Enter(condition)][then][Leave][EnterElse(condition)][else][Leave]` の順に積む（`llvm.rs` の `If` と同じ順序）。`Match` は腕の `steps`・`bindings` を `pattern = true`、`guard`・本体を `false` で積む。`children()` は非網羅なので、**文脈を決める親（`If`・`Match`・`Block`・`Call`・`Index`・`Slice`・`ForEach`・`Borrow`・`BorrowOperand`・`Length`・`StringLength`・`Record`・`Tuple`・`Array`・`List`・比較の `Binary`）だけ手で積み**、ほかは `children()` を使う。式の総数が上限を超えたら `E1017`。
4. **強連結成分**。到達した関数の辺から Kosaraju（`recursion::check_graph` と同じ。反復）。成分を、呼び出される側が先になる順に処理する。
5. **要約**。成分ごとに、(a) 成分の全員の `Local.effects` の和、(b) 成分の外の呼び先の `Summary.effects` の和、(c) 成分が環（大きさ 2 以上か自己辺）なら、全員に `diverge`（`computed` にあるとき）。各関数の `diverge` の直接の源は、同じ成分の関数への辺のうち span が最小のもの（`why: cycle`。付録 J.1）。成分の全員が同じ `effects` を持つ。`unknown`: `dyn_reach`・`unresolved_reach`・`value_call_reach` を同じ和で求め、関数ごとに 7.2 の式（`open` は引数の型で判定し、型ごとに保存）。
6. **証拠**。効果 `e` ごとに、成分の各関数について、自分の直接の源があれば `Direct`、なければ成分の外の呼び先（`e` を持つもの）への最小 span の辺を `Via`。残りは、直接の源・外への `Via` を持つ関数から、成分の中の呼び元へ**幅優先**で `Via` を伝える（呼び元は添字の昇順、`Via` の `span` は呼び元の中のその呼び先への最小の参照）。
7. 結果を返す（`Scope::Reachable` は到達しなかった関数の `Summary` を空にし、`computed` はそのまま）。

計算量: 走査は O(式の総数)、成分と要約は O(辺 + 関数)。`copies::sites` は解析の 4〜24%（付録 E.3）なので、`computed` に `Copy`・`Alloc` がなければ呼ばない。

### `validate` のアルゴリズム

宣言の関数（`origin.asserts != 0`）ごと、属性ごと（`no_alloc`・`no_copy`・`no_trap`・`terminates` の順）に、まず属性の効果（`alloc`・`copy`・`trap`・`diverge`）が `SUPPORTED` になければ `E1032`（`effect_attribute_unavailable`）を出して次へ。そうでなければ、`forbidden(attr) ∩ effects(f)` か `unknown(f)` が空でないとき診断を 1 件作る。禁じる集合: `no_alloc` = {`alloc`, `extern`}、`no_copy` = {`copy`}、`no_trap` = {`trap`, `extern`}、`terminates` = {`diverge`, `extern`}。`unknown` はどの属性でも違反（kind `effect_unknown_callee`）。位置は禁じる効果のうち証拠の span が最小のもの。連鎖は `chain(f, e)` から作る。家族（宣言の関数とその `origin.parent` の連鎖で着く生成関数）の中に証拠があればその式、外の関数なら家族から外へ出る最初の呼び出し式。複数のインスタンスの診断は (宣言の位置, 属性, kind, 主スパン) で重複を除く。`W1008` は、属性の宣言にインスタンスが 1 つもないとき（`Program.effect_attributes` の関数名と `module.functions` の `asserts` を突き合わせる）。

### 段ごとの変更表

| Phase | ファイル | 場所 | 変更 |
| --- | --- | --- | --- |
| 1 | `src/effects.rs`（新規） | — | 上の型・`builtin_facts`・走査・成分・要約・証拠・`needed`・`summarize` |
| 1 | `src/check.rs` | `CheckedModule` と 2 つの構築（`check_modules_collect` の `let mut module = CheckedModule {`、`recovery_module`）、`mod` 宣言 | `pub effects: Option<effects::FunctionEffects>`（`None`）、`#[path = "effects.rs"] pub mod effects;` |
| 1 | `src/polymorph.rs` | `specialize` の `Ok(CheckedModule {` | `effects: None` |
| 1 | `src/check.rs` | `check_modules_collect`、`validate_cpu_functions` の直後 | `effects::needed(&module)` が真なら `effects::summarize(&module, effects::Options::checking(&module))` を呼ぶ。成功したら `effects::validate` の `errors` を `diagnostics` へ、`warnings` を `module.warnings` へ足して `(span.source, span.start)` で安定整列し直し、`module.effects = Some(..)`。`E1017` は `diagnostics` へ。`diagnostics.check()?` の前。経路 A は `pure_rules::validate` の呼び出しを `effects::validate` に統合 |
| 2 | `src/syntax.rs` | `Program`、`CpuAttribute` の隣 | `effect_attributes: Vec<EffectAttribute>`、`EffectAttribute { function: Ident, kind: EffectKind, span: Span }`、`EffectKind { NoAlloc, NoCopy, NoTrap, Terminates }`（`name()`・`bit()`） |
| 2 | `src/parser.rs` | `program_all`（`let cpu = self.cpu_attribute()?;`）、`Program` の構築 | `attributes()`（`@cpu` と 4 属性を任意の順序で読む。重複は `E1032`。D-53 の共通の読み込み口で、E15 の `@trusted` は後からこの口に足される。E15 が先に入っていれば `trusted_attribute()` の呼び出しも取り込む）に置き換える。署名の登録（`cpu_attributes.push` の箇所）に `effect_attributes.push`。属性の次が `def` でないとき `E1032`、`extern` の前なら `effect_attribute_on_extern` |
| 2 | `src/parse_control.rs` | `checked_expression` | `@` の次が 4 つの名前なら `E1032`（`effect_attribute_misplaced`）、それ以外は従来の `E0002` |
| 2 | `src/syntax_codec.rs` | `Program` の `put`/`get`、`impl Wire for EffectAttribute` | `cpu_attributes` の次に `effect_attributes`。`FRONTEND_FORMAT`（`src/frontend_cache.rs`）を上げる |
| 2 | `src/formatter.rs` | `canonicalize` | `effect_attributes` の `function` と `span` を正規化 |
| 2 | `src/api_diff.rs` | `Program` の分解（`cpu_attributes: _` の隣）、`Item` | `effect_attributes` を読み、`Item.asserts: u8`、`text()` に 4 属性の名前（`@no_alloc `・`@no_copy `・`@no_trap `・`@terminates `）の前置き、`compare_items` の `widened` に `old.asserts & !new.asserts == 0` |
| 2 | `src/check.rs` | `FunctionOrigin`、`source()`・`generated()`、ユーザー関数の構築（`cpu: cpu_levels...` の隣）、`cpu_levels` の隣 | `pub asserts: Asserts`（`#[derive(Clone, Copy, Debug, PartialEq, Eq)]`。`u8` の包み）。`source()` は 0、`generated()` は 0（`..self` の前に書く）。`asserts_by_decl` を `cpu_levels` と同じ形で作る |
| 2 | `src/polymorph.rs` | `instantiate`（`cpu` を保つ箇所） | `asserts` も保つ（ジェネリックなインスタンスが属性を引き継ぐ） |
| 3 | `src/llvm_frame.rs`・`src/llvm_escape.rs`・`src/llvm.rs` | `frame_bytes`・`staying_locals`、`mod escape;` の隣 | `pub(crate)` にし、`llvm.rs` に `pub(crate) use escape::staying_locals; pub(crate) use frame::frame_bytes;`（**この 3 行以外は変えない**。IR 不変） |
| 4 | `src/ranges.rs` | `enter_condition` の隣 | `enter_else_condition`・`negate`（7.3 T3。付録 D.3・I） |
| 4 | `src/driver.rs` | `Project::source_for` の隣 | `pub fn source_paths(&self) -> Vec<String>`（各 source の `relative_path`。`tests/effects_trap.rs` だけが使う。付録 I） |
| 7 | `src/docgen.rs`・`src/semantic.rs` | `render_declarations`、`semantic` の 2 箇所（`const ` を出す所） | 属性を `@no_alloc ` の形で前置き（`program.effect_attributes` を関数名で引く） |

## 前提とする他チケットのインターフェース

- **A19**（`FunctionOrigin.pure`・`Names.pure_functions`・`pure_rules.rs` の `own_violation`・`plain_part`・`validate`・`E1029` の `{reason}`・`{chain}`・`pure_functions.rs`。ファイルは A19 が `src/`・`tests/` に新規に作る）。**経路 A**（done）: `pure_rules::validate` の走査を `effects::summarize`（`Options::new(Scope::Reachable, PURE_FORBIDDEN)`）に置き換える。根が不純なのは `effects ∩ PURE_FORBIDDEN ≠ ∅` か `unknown ∈ {Dyn, Unresolved}`。位置・`{callee}`・`{chain}`・`{reason}` は A19 の規則のまま `Witness` から作る（`{reason}` は `Why.builtin` と効果で選ぶ。`Bench.now`・`Bench.consume`・`Gen.__seed` は「時計・乱数・計測」の文言）。`pure_rules.rs` には署名の規則と、D-53 の「純粋性の問い合わせの窓口」（`what`・`Impurity`・`impurity_of_roots`・`reachable_effect`・`builtin_impure`。B10・F15 が使う。中身は `effects::summarize` の `Witness` から作る薄い関数。名前と形は A19 のまま）が残り、A19 の単体テスト `every_builtin_has_a_verdict`（不純 90・純粋 156）と `honors_the_exact_limits` は `src/effects.rs` へ移す。**経路 B**（未実装）: 手順 7。
- **G22**（診断 v2。D-51）: `Diagnostic::with_kind(&'static str)`・`.related(RelatedRole::ViaCall | RequiredBy, impl Into<String>, Span)`・`.with_help(impl Into<String>)`。未実装なら、これらを呼ばず、メッセージ末尾に ` (through 'A' -> 'B')`（最大 4 個）を足す。`related` の JSON の `role` の文字列は G22 が決める（例では `required_by`）。G22 が入ったら、`validate` と `E1032`・`W1008` を作る箇所に `.with_kind(..)`・`.related(..)`・`.with_help(..)` を足すだけで、他は変わらない。
- **G26**（Facts）: `FunctionEffects::rows` の `EffectRow` を `facts/<Module>.jsonl` の `effects`・`effect_sites`・`complete`（= `effects_complete`）へ写す。`rows` は関数 id と位置だけを返し、ID の文字列化は G26。
- **E15**: `effects::forbidden(kind: EffectKind) -> EffectSet`（4 つの禁じる集合。7.4）を公開し、`@trusted` が `extern` を引ける。
- **A21**: 契約の式の純粋性は**式単位**の走査が要る。`summarize` の入口は関数単位に固定し、式単位は A21 が `scan_expression`（手順 4 で `scan_function` から切り出す）を再利用して足す。`Options` は `Options::new(Scope::All, PURE_FORBIDDEN)` で作る。
- **B10・A23**: 新しい組み込み関数（B10 の `DomainRun`＝`task`＋`alloc`・`DomainMessage`＝純粋、A23 の `Int.wrapping_*`＝純粋）の分類は、足すチケットが `builtin_facts` の `match` に足す（wildcard がなく、コンパイルが強制する）。

## 実装手順

各手順の最後で `cargo build --release --locked && cargo test --locked`（と、その手順のテスト）が緑。手順の前後で、全 fixture・example の `--emit llvm -O0` の IR が手順 1 の基準と **1 byte も変わらない**ことを確かめる（付録 H.1 のループを `-O0` で回し、結果を `cmp` する）。

### Phase 1（効果の要約の基盤）

### 手順 1: 基準
- 変更: なし
- 内容: `cargo build --release --locked && cargo test --locked` の件数と、全プロジェクトの IR（`Main.tz` を持つ 102 個）を保存する。`ls src/pure_rules.rs`（A19 が新規に作るファイル）で経路を決める（あれば A）。
- 確認: 付録 H.1 の件数コマンドと IR の同一性（`-O0` と `-O3`）の再現を実行し、出力が一致する（`TypedExprKind` 64・`Builtin` 246）。

### 手順 2: `Effect`・`EffectSet`
- 変更: `src/effects.rs`（新規）、`src/check.rs`（`mod` 宣言）
- 内容: 7.1 の表どおり。`Effect::ALL` は辞書順で 14 個、`name`・`from_name`。`EffectSet` の演算と `PURE_FORBIDDEN`（10 個）、`SUPPORTED`（Phase 1 は `PURE_FORBIDDEN` と同じ）。
- 確認: 単体テスト `effect_names_are_sorted_and_round_trip`（`Effect::ALL` の `name` が昇順、`from_name` が逆、14 個）、`pure_forbidden_has_ten_effects`。

### 手順 3: 組み込み関数の表
- 変更: `src/effects.rs`
- 内容: `BuiltinFacts` と `builtin_facts`。付録 B の 37 行をそのまま arm にする（B1 の `TestAdd`・`TestUnsigned`・`TestWiden` は `#[cfg(test)]` の別 arm に出す）。**wildcard を置かない**。
- 確認: 単体テスト `every_builtin_has_a_verdict`（`Builtin::ALL` を回し、`effect.is_some() || dynamic` が通常のビルドで 90、`cfg(test)` で 90 のまま（Test* は純粋）。件数 243／246）、`impure_builtins_are_pinned_by_name`（不純の 90 個の名前を固定。付録 B の列と一致）、`alloc_builtins_are_pinned`（49）、`trap_builtins_are_pinned`（26）、`diverge_builtins_are_pinned`（14）、`callback_builtins_are_pinned`（16）。

### 手順 4: 局所の走査
- 変更: `src/effects.rs`
- 内容: `Local`・`Item`・`Ctx`・`scan_function`（本体は `scan_expression(expression, ctx, &mut Local, &mut State)` へ切り出す）。`match &expression.kind` に**全 64 variant**を wildcard なしで書く（付録 A。Phase 1 で効果を付けるのは辺・10 個・`unknown`・`user_drop` だけ。`alloc`・`trap`・`copy`・`diverge` の arm は書くが `computed` にないビットは記録しない）。
- 確認: 単体テスト `scan_records_edges_for_every_reference`（`Interpolated` の `hole.method`・`StructuralCompare` の要素のメソッド・`Array.map` に渡す関数・`Closure` が辺になる）、`scan_marks_value_calls`（`Call` の callee が `Local`・`Pipe` の右辺が `Local`・`TaskRun`・`callbacks` の組み込み関数が値の呼び出しになり、callee が `Function`・`Closure` なら**ならない**）、`scan_marks_dyn_and_unresolved`。

### 手順 5: 成分・要約・証拠
- 変更: `src/effects.rs`
- 内容: 「アルゴリズム `summarize`」の 1・4・5・6・7。上限（`Limits`）と `E1017`。
- 確認: `tests/effects_summary.rs`（新規）に、`summary_of_a_scalar_function_is_empty`・`io_debug_and_extern_propagate_through_calls`・`mutual_recursion_shares_one_summary`・`closures_passed_to_std_functions_are_edges`・`open_function_that_calls_its_parameter_is_unknown`・`closed_function_with_local_lambdas_is_known`・`witness_chain_ends_in_a_direct_source`・`summaries_are_deterministic`（2 回の結果が等しい）。単体テスト `honors_the_exact_limits`（`summarize_with` に小さい `Limits` を渡して境界）。

### 手順 6: `check` への接続
- 変更: `src/check.rs`・`src/polymorph.rs`
- 内容: `CheckedModule.effects`（3 か所の構築に `None`）、`check_modules_collect` の呼び出し（変更表）。`needed` が偽ならば `summarize` を呼ばない。
- 確認: `tests/effects_summary.rs::no_attribute_means_no_summary`（属性も `pure` もないプログラムで `module.effects.is_none()`）。全 IR が基準と同一。`cargo test --locked` の件数が手順 1 と同じ（＋新規）。

### 手順 7: A19 との接続
- 変更: `pure_rules.rs`（経路 B は A19 の手順 1〜4 が触るファイルも。A19 のチケットのとおり）
- 内容: （経路 A）`pure_rules::validate` の走査を `effects` に置き換える（前節の A19 のインターフェース）。（経路 B）A19 の手順 1〜4 を A19 のチケットどおり実装してから、`pure_rules::validate` を `effects` の上に実装する。
- 確認: （経路 A）`cargo test --locked --test pure_functions` の通過件数が手順 1 と同じで、A19 の `E1029` の拒否例 `r_io`（`A19-pure-functions.md`）の診断のコードと位置が同じ。（経路 B）A19 の手順 5 の確認（`pure_functions.rs` の手順 5 の行）を通す。

### 手順 8: 文書と全体
- 変更: `docs/architecture.md`
- 内容: 表に `src/effects.rs` の行を足す（`src/closures.rs` の行の近く）。`docs/language.md`・言語リファレンスの更新は Phase 7 まで不要（Phase 1 は利用者から見える変更がない。`pure` の文書は A19）。
- 確認: `cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings`・GUIDE §3.1 の回帰テスト 4 本が通る。

### Phase 2（`@no_copy`）

### 手順 9: 構文
- 変更: `src/syntax.rs`・`src/parser.rs`・`src/parse_control.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`・`src/formatter.rs`・`src/api_diff.rs`・`tests/effects_attributes.rs`（新規）
- 内容: 変更表の Phase 2 の構文の行。この段では `check` は属性を無視する（手順 11 までの途中の状態で、単独では出荷しない）。
- 確認: `parses_effect_attributes_in_any_order_with_cpu`・`keeps_effect_attribute_names_as_identifiers`（`let no_alloc = 1` と `let terminates = 2` が従来どおり通る）・`rejects_misplaced_duplicate_and_extern_attributes`（`E1032`）・`formats_effect_attributes_and_round_trips`・`api_diff_treats_attributes_as_promises`、`syntax_codec` の既存の往復テスト、`node tests/frontend_cache.mjs target/release/tsuzuri`。IR 不変。

### 手順 10: `FunctionOrigin.asserts`
- 変更: `src/check.rs`・`src/polymorph.rs`
- 内容: `Asserts`・`asserts_by_decl`・`source()`/`generated()` の 0・`instantiate` で保つ。`needed` が `asserts != 0` を見る。
- 確認: `generic_instances_keep_the_attribute`・`lambdas_do_not_inherit_the_attribute`。

### 手順 11: `copy` と `E1031`・`E1032`・`W1008`
- 変更: `src/effects.rs`
- 内容: `SUPPORTED` に `Copy` を足し、`computed` に `Copy`（`copies::sites` の `Length`）。`validate`: `@no_copy` の `E1031`（診断の雛形は 7.5・付録 J）、効果が `SUPPORTED` にない属性の `E1032`（`effect_attribute_unavailable`）、`W1008` の 2 種。
- 確認: `no_copy_accepts_borrowing_code`・`no_copy_rejects_an_implicit_array_copy`（`E1031`、kind `effect_copy_in_no_copy`、位置は複製の式）・`no_copy_follows_calls`（連鎖）・`explicit_array_copy_is_not_a_copy_effect`・`unchecked_generic_warns`・`const_def_warns`・`unavailable_attributes_are_rejected_until_their_phase`（`@no_alloc`・`@no_trap`・`@terminates` が `E1032`。Phase 3・4・5 の手順で、その属性の行を受理の期待へ書き換える）。

### Phase 3（`@no_alloc`）

### 手順 12: 再公開（IR 不変）
- 変更: `src/llvm_frame.rs`・`src/llvm_escape.rs`・`src/llvm.rs`
- 内容: 変更表の Phase 3 の行（`frame_bytes`・`staying_locals` を `pub(crate)` にし、`llvm.rs` で再公開する）。
- 確認: 全 IR が基準と同一。
### 手順 13: `alloc` の規則
- 変更: `src/effects.rs`
- 内容: A1〜A11（7.3）。`copy` の箇所を `alloc` にも。`SUPPORTED` に `Alloc` を足す（`@no_alloc` が使える）。
- 確認: `tests/effects_alloc.rs`（新規）に付録 C の各行 1 例以上（受理と拒否）。**テスト先行**: 手順 14 のテストを先に書いて失敗させてから規則を足す。
### 手順 14: 規則が IR より保守的であること
- 変更: `tests/effects_alloc_conservative.rs`・`tests/common/ir_scan.rs`・`tests/fixtures/effects_sweep/Main.tz`（すべて新規）
- 内容: 付録 F・G のとおりにテストと掃引の fixture を書く。
- 確認: 付録 F の期待（違反 0）。違反が出たら**停止条件 1**。

### Phase 4（`@no_trap`）

### 手順 15: `enter_else_condition`
- 変更: `src/ranges.rs`
- 内容: 付録 I のコード（`enter_else_condition`・`negate`）を `enter_condition` の隣に足す。エミッターは呼ばない。
- 確認: `mod tests`（既存の `walk`・`verdicts` を手本に、else の前で `enter_else_condition`・後で `leave_condition` を呼ぶ `walk_else` を足す）に `else_branch_of_or_proves_the_index`（付録 H.5 の `a_guard2` の `xs[i]` が `true`）・`else_branch_keeps_weak_conditions_unproven`（`a_guard3` が `false`）、`emitter_checks_are_unchanged`（既存テストの結果と全 IR が不変）。
### 手順 16: `trap` の規則
- 変更: `src/effects.rs`
- 内容: T1〜T13。`RangeFacts` の入退場。`SUPPORTED` に `Trap` を足す（`@no_trap` が使える）。
- 確認: `tests/effects_trap.rs`（新規）に付録 D の各行の受理・拒否。`Array.get`・`Array.at`・`for` の範囲の添字の例。
### 手順 17: トラップの対応の検査
- 変更: `tests/effects_trap.rs`
- 内容: 付録 F.3 の (a)〜(c) のテストを書く。
- 確認: 違反 0（停止条件 2）。

### Phase 5（`@terminates`）

### 手順 18: `diverge` の規則
- 変更: `src/effects.rs`
- 内容: V1〜V5。`SUPPORTED` に `Diverge` を足す（`@terminates` が使える。以後 `effect_attribute_unavailable` は出ない）。
- 確認: `terminates_accepts_for_loops_and_acyclic_calls`・`terminates_rejects_while`・`terminates_rejects_self_tail_recursion_and_mutual_recursion`・`terminates_rejects_blocking_builtins`・`terminates_rejects_unknown_callees`。

### Phase 6（関数型の引数）

### 手順 19: 関数型の引数（`calls_params`）
- 変更: `src/effects.rs`
- 内容: `calls_params`。実引数の判定 `ArgEffect { Known(EffectSet), Param(usize), Unknown }` を、`call_specialization::target` の形（`Function(User)`・`Closure`・部分適用の `Call`・`Local` の束縛）を手本に `effects.rs` へ書く（`call_specialization.rs` は `llvm` の子で非公開）。属性つきの関数 `g` の呼び出し地点で、`g` の `calls_params` の各引数 `i` の実引数 `a` を判定する: `Known(E)` で `E ∩ forbidden ≠ ∅` → `E1031`（その呼び出し式。`related` に引数の関数の連鎖）／`Known(E)` で交わらない → 受理／`Param(p)`（呼び元の引数）で呼び元が同じ属性を持つ → 呼び元の `calls_params` に `p` を足す（義務が移る）／持たない → `E1031`（`effect_unknown_callee`、`help`: `add '@{attr}' to '{caller}' so that its callers are checked`）／`Unknown` → `E1031`（`effect_unknown_callee`）。強連結成分の中は `calls_params` を単調に反復（上限は成分の大きさ × 64 回）。
- 確認: `higher_order_no_alloc_accepts_a_pure_lambda`・`higher_order_no_alloc_rejects_an_allocating_lambda`・`forwarded_parameter_moves_the_obligation`・`dyn_call_is_unknown`。

### Phase 7（ツールと文書）

### 手順 20: ツールと文書
- 変更: `src/docgen.rs`・`src/semantic.rs`・`src/api_diff.rs`・文書
- 内容: 変更表の Phase 7 の行。hover は `module.effects` が `Some` のとき `inferred effects: alloc, trap` の行を足す。LSP は `@` の補完を持たないので足さない（HEAD で `grep -n '"cpu"' src/lsp.rs` が 0 件）。
- 確認: `docgen_shows_effect_attributes`・`hover_shows_attributes_and_inferred_effects`・文書の検証コマンド（「ドキュメント」）。

## テスト計画

名前は `tests/<file>.rs::<fn>`。`tests/effects_*.rs`・`tests/common/ir_scan.rs`・`tests/fixtures/effect_attributes/`・`tests/fixtures/effects_sweep/` はすべて新規。期待値は独立した参照（手計算・IR の実測）から作る。拒否は診断コードを照合する。helper は `tests/constants.rs` の `rejected(source)`・`tests/call_specialization.rs` の `body(ir, name)` を手本にする。

| ファイル | 名前（Phase） | 内容 |
| --- | --- | --- |
| `src/effects.rs` の単体 | `every_builtin_has_a_verdict`・`*_are_pinned`・`honors_the_exact_limits`・`effect_names_are_sorted_and_round_trip`（1） | 手順 2・3・5 |
| `tests/effects_summary.rs` | 手順 5・6 の名前（1） | 要約・辺・成分・未知・証拠・決定性・`no_attribute_means_no_summary` |
| `tests/effects_attributes.rs` | 手順 9・10・11 の名前（2） | 構文・`E1032`・`W1008`・`@no_copy`・ジェネリックのインスタンス・`api-diff`・整形 |
| `tests/effects_alloc.rs` | 付録 C の行ごと（3） | 受理と拒否。`closure_without_captures_is_not_alloc`・`large_literal_is_alloc`・`empty_string_literal_is_not_alloc`・`returned_array_literal_is_alloc` |
| `tests/effects_alloc_conservative.rs` | `rule_is_at_least_as_conservative_as_the_ir`・`std_public_functions_are_all_swept`・`false_positive_count_is_reported`（3） | 付録 F。違反 0 |
| `tests/effects_trap.rs` | 付録 D の行ごと、`array_get_is_trap_free`・`guard_with_or_proves_the_else_branch`・`ir_trap_sites_are_covered_by_the_rule`（4） | 付録 F |
| `tests/effects_terminates.rs`（新規） | 手順 18 の名前（5） | |
| `tests/effects_higher_order.rs`（新規） | 手順 19 の名前（6） | |
| `tests/features.mjs` の suite `effect_attributes`（2〜5） | fixture `tests/fixtures/effect_attributes/Main.tz`（新規）: `export def` の属性つき関数 6 個。期待値は JavaScript の BigInt で計算 | native/WASM × `-O0`/`-O3`、`live == 0`、WASM の import が空。IR 同一は `tests/effects_attributes.rs::attribute_does_not_change_ir`（同じソースから属性の行だけ除いた版と `llvm::emit` の出力が等しい） |
| `tests/api_diff.rs`・`tests/formatter.rs`・`tests/docs.rs`・`tests/lsp.rs` | 既存に行を足す（2・7） | 属性の差分・整形・docgen・hover |

**既存テストへの影響**: なし。根拠: 属性も `pure` もないプログラムは、構文（`Program` に空の `Vec` が足されるだけ）・`check`（`needed` が偽）・IR（`llvm*.rs` を変えない）が変わらない。`FRONTEND_FORMAT` を上げるので、形式の数を固定する期待値があれば直す（`grep -rn "FRONTEND_FORMAT\|frontend-format" tests src`）。A19 の全テスト（経路 A）は緑のまま。

**性能**: 属性なしのプログラムで `check` の時間が増えないこと（`needed` は `module.functions` の 1 回の走査）を、`tests/effects_summary.rs::no_attribute_means_no_summary` で固定する。性能の主張はしない。

## ドキュメント

GUIDE §8。`grep -rnE "@cpu|@literal|属性|E1028|E1017" _tsuzuri/language-reference docs README.md` で漏れを探す。

- `_tsuzuri/language-reference/values-and-functions/attributes.md`: 属性が 5 つから **9 つ**に。一覧表・mermaid・「ユーザー定義の属性はない」の列挙・まとめ。新しい節「効果の表明（`@no_alloc`・`@no_copy`・`@no_trap`・`@terminates`）」: 保証、禁じる効果の表、書ける位置と `E1032`、精密さの限界（付録 C の偽陽性）、`const def`・ジェネリック・`@cpu` との関係。例は `run=`（受理されるもの）と、拒否の例は text。
- `_tsuzuri/language-reference/compiler/diagnostics.md`: `E1031`・`E1032`・`W1008` の行、「`E1001`〜`E1028`」を実装時点の最大のコードまでに。`values-and-functions/functions.md`: 属性への導線（A19 の `pure def` の節があれば並べる）。`ownership-and-memory/stack-and-heap.md`: `@no_copy` の説明に `copy` の定義（W1006 との関係）を 1 段落。
- `docs/language.md`: 属性の節と診断の表。`docs/architecture.md`: `src/effects.rs` の行、`src/ranges.rs` の行に `enter_else_condition`。`docs/stability.md`: api-diff の分類（属性を足すのは互換、外すのは非互換）。`README.md`: 機能の一覧に 1 行。`_features/GUIDE.md` §4 のコードマップに `src/effects.rs`、D-55 の `W1008` の用途を確定（「効果の属性が効かない・検査されない」）。
- 検証: `node scripts/check-docs.mjs _tsuzuri/language-reference/values-and-functions/attributes.md _tsuzuri/language-reference/compiler/diagnostics.md`。

## 受け入れ条件

- [ ] **Phase 1**: `src/effects.rs` が `TypedExprKind` 64・`Builtin` 246 を wildcard なしで判定する。単体テストと `tests/effects_summary.rs` が通る。属性も `pure` もないプログラムで `check` が要約を計算しない。全 IR が基準と同一。経路 A は A19 の全テストが緑、経路 B は A19 の手順 5 の確認が緑。
- [ ] **Phase 2**: 4 つの属性が読め、`E1032`・`W1008`・`@no_copy` の `E1031` が出る（ほかの 3 属性は `effect_attribute_unavailable`）。属性を外した版と IR が同一。ジェネリックのインスタンスが属性を引き継ぐ。`api-diff` と整形が通る。
- [ ] **Phase 3**: 付録 C の規則。`tests/effects_alloc_conservative.rs` の違反が **0**。偽陽性の件数が付録 E と同じ桁（コーパスで 505 前後。大きく増えたら理由を報告）。
- [ ] **Phase 4**: 付録 D の規則。`enter_else_condition` が `llvm*.rs` から呼ばれず、IR 不変。`ir_trap_sites_are_covered_by_the_rule` の違反が 0。
- [ ] **Phase 5・6**: 手順 18・19 のテスト。文書に `@terminates` が構造的な十分条件だけと書いてある。**Phase 7**: 文書と `check-docs` が通る。docgen・hover。
- [ ] `cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings`・`cargo test --locked`・GUIDE §3.1 の回帰テスト 4 本・`sh scripts/check-runtime-includes.sh` が通る。`node tests/features.mjs target/release/tsuzuri effect_attributes`（件数を控える）。native/WASM × `-O0`/`-O3`。
- [ ] 完了報告を GUIDE §13.2 の書式でチケット末尾の `## 実装と検証（YYYY-MM-DD）` に書く。コミットは人間の指示まで作らない。

## 落とし穴

- **`TypedExpr::children` は非網羅**（末尾が `_ => Vec::new()`）。走査に使うが、**分類は wildcard なしの `match`**。新しい variant・`Builtin` は `match` がコンパイルエラーにする。
- **関数の参照はすべて辺**。`Call` の callee だけを辺にすると、`Interpolated` の `hole.method`・比較の要素のメソッド・`Array.map` に渡す関数を見逃す。
- **組み込み関数は `$builtin` の生成関数を通る**。`closures::lower` が参照をすべて置き換えるので、`Function(Builtin(..))` の分類は生成関数の本体でだけ効く（ユーザー関数の本体には残らない。`K::Parallel`・`TaskRun`・`TaskParallel`・`TaskParallelResults` は別）。
- **成分の和は固定点ではない**。環の中の全関数が同じ集合を持つ。深さ優先で関数ごとに結果を保存すると、循環の途中の祖先を取りこぼす（A19 の落とし穴）。
- **静的な呼びと動的な確保は違う**: `@tz.string.new(ptr, i64 0)` は確保しない。テストの IR 走査はこれを除く（付録 F）。規則側で `""` を `alloc` にしない。
- **IR のトラップには論理でないものが混じる**（参照カウントの桁あふれ・ホストの状態検査・到達不能な `PatternMismatch`・ランタイム補助関数の `NumericRuntime`）。比べるのは `--trap-info` の**実際の箇所**（`@tz.trap.report(i32 定数)`）と、継承された文脈の `add i32 %tz.context, KIND` の**種類**。呼び出し文脈の箇所は比べない（付録 F）。
- **`enter_condition` と `enter_else_condition` の入退場は対**。`then` と `else` の間で `leave` を忘れると、証明が `else` に漏れる（順序は `llvm.rs` の `If` と同じ）。`stable` でない局所（可変な添字）は証明に使わない。
- **`FunctionOrigin::generated` は `asserts` を 0 にする**。ラムダ・`$builtin`・`$case` が属性を持つと、std の本体に診断が出る。`instantiate` は宣言のインスタンスにだけ保つ。

## 対象外

- 属性を最適化・IR に使うこと、属性の引数（`@no_alloc(max = 4)` のような形）、ユーザー定義の属性。
- `@trusted`（E15）、契約（A21）、`Domain`（B10）、並列化の保証（F15）、確保数のテスト用 API（F14）。
- 停止性の証明（ループの上界の推論）、`try` による `Checked` の捕捉の精密化、`std` の関数への `@no_*` の付与（std を書き換えず、要約が推論する）。
- `alloc` の規則を IR の確保の有無に完全に一致させること（偽陽性は残る。付録 E）。
- `ranges.rs` の証明を `Vec`・`List`・`String` の添字や `Slice` へ広げること、エミッターが `enter_else_condition` を使うこと（別チケット。IR が変わる）。

## 決定事項

### D1: 属性は `Program.effect_attributes`（`@cpu` と同じ脇のリスト）に持つ
- 理由: `FunctionDecl` の構築 3 箇所と A19 の `pure` に触れず、`@cpu` の持ち回り（`cpu_levels`→`FunctionOrigin`）をそのまま使える。却下した案: `FunctionDecl.attributes`（種の案）。状態: 承認済み

### D2: `src/effects.rs` は `check` の子（`pub mod effects`）にする
- 理由: `stored_all` が `pub(super)`。A19 の `pure_rules` と同じ。却下した案: `lib.rs` の独立モジュール（`stored_all` を呼べない）。状態: 承認済み

### D3: 要約は**必要なときだけ**計算する（属性か `pure` があるとき、または消費者が呼ぶとき）
- 理由: 実測で走査は解析の 4〜27%（約 90% が `copies::sites`）。全プログラムに課さない。却下した案: 常に全関数（種の案。facts は `summarize(Scope::All)` で得られる）。状態: 承認済み

### D4: 成分は和（固定点なし）。証拠は幅優先
- 理由: 同じ成分の全員が互いに到達できる。却下した案: 関数ごとの反復。状態: 承認済み

### D5: 参照辺の規則と「開いている」規則で `unknown` を決める。Phase 6 で精密化
- 理由: 閉じた関数では外から関数値が入らない（A19 の論法の一般化）。却下した案: すべての関数値の呼び出しを未知（`Array.fold` に `\a x -> a + x` を渡せない）。状態: 承認済み

### D6: `extern` は `@no_alloc`・`@no_trap`・`@terminates` を、`unknown` はすべてを破る。`user_drop` は Drop 関数への辺で表す
- 理由: 外の C コードの性質は不明。E15 の `@trusted` が後で緩める。却下した案: `extern` を黙認。状態: 承認済み

### D7: `alloc` は位置規則（付録 C）。非空のリテラルは「その場」でなければ `alloc`、`frame_bytes > 65,536` も `alloc`。暗黙の複製は `alloc`
- 理由: IR の実測（昇格・64 KiB・`reserve_buffer` の条件）。規則は IR より保守的。却下した案: リテラルを常に `alloc`（実用にならない）、`frame_bytes` を無視（72 KB で不健全）。状態: 承認済み

### D8: `trap` は論理 9 種のみ。資源 6 種と到達不能 1 種は含めない。`Match` は `trap` でない。`Checked`・`Reraise` は `trap`。除数・刻み・添字の免除は記録して数える
- 理由: D-53。IR の種類別の実測（付録 D）。却下した案: `Checked` を `try` の内側で免除（捕捉の判定が要る）。状態: 承認済み

### D9: `ranges.rs` に `enter_else_condition` を足す（効果の解析だけが使う）
- 理由: `Array.get` の `if i < 0 || i >= len then .. else xs[i]` を証明できないと、`@no_trap` が std の安全な accessor を呼べない。却下した案: std の関数名の許可リスト（魔法）、エミッターにも使う（IR が変わる）。状態: 承認済み

### D10: `diverge` は `While`・環・未知・ブロッキングの組み込み 14 個。`for` と `ForEach` は有界。`TaskRun` は値の呼び出し（`unknown` の源）で、`diverge` でない
- 理由: 構造的な十分条件。`for` の範囲は型の端で止まる（言語リファレンス）。タスクの本体の停止は辺が運ぶ。却下した案: `Task.run`・`Task.parallel` を `diverge`。状態: 承認済み

### D11: ジェネリックはインスタンスごとに検査（診断は 1 件に集約）。インスタンスがなければ `W1008`
- 理由: 型で `alloc` が変わる。却下した案: 抽象本体の検査（型が不明）。状態: 承認済み

### D12: 属性と `@cpu` は併用でき、順序は任意
- 理由: `@cpu` の各版は同じ言語の規則。却下した案: 併用禁止（属性は IR を変えない）。状態: 承認済み

### D13: 属性は IR を変えない（外した版と IR が同一であることをテストする）
- 理由: D-53。却下した案: 属性を最適化に使う。状態: 承認済み

### D14: 経路 A（A19 done）は走査を置き換え、経路 B は A19 の手順 5 を本チケットで実装する
- 理由: A19 が未実装でも進められる。却下した案: A19 の完了を待つ。状態: 承認済み

### D15: `api-diff` は属性を足すのを互換、外すのを非互換とする
- 理由: 約束を足すだけ。却下した案: 常に非互換。状態: 承認済み

### D16: `frame_bytes`・`staying_locals` は `pub(crate)` の再公開で使う（再実装しない）
- 理由: エミッターの判断と**ずれない**。却下した案: 規則側に複製。状態: 承認済み

### D17: 検証は fixtures・examples・std の掃引（付録 F・G）。トラップは**実際の箇所**と**継承文脈の種類**で比べる
- 理由: 種類なしの `@llvm.trap` の有無は資源のトラップで汚れる。却下した案: 関数に `@llvm.trap` があるかだけ。状態: 承認済み

### D18: `E1031` は kind で区別（6 種）。属性ごとのコードは作らない。新しい CLI・環境変数・std・予約語・crate は足さない
- 理由: D-55 は 2 コードだけ割り当て。D-50。却下した案: 属性ごとに別コード、`tsuzuri effects`。状態: 承認済み

### D19: 効果を `SUPPORTED` が含まない属性は `E1032`（`effect_attribute_unavailable`）で拒否する
- 理由: Phase を単独で出荷しても、検査されない保証を受理しない。却下した案: 受理して無視（偽の安心を与える）。状態: 承認済み

## 付録

[A20-appendix.md](A20-appendix.md): 付録 A `TypedExprKind` 64 の分類／B `Builtin` 246 の分類／C `alloc` の規則／D `trap` の規則／E 実測／F 検証テストの設計／G std の掃引（116 関数）／H 再現コマンドと実出力／I 試作の骨格／J 診断の部品。


## 実装と検証（2026-10-29）

- **範囲**: Phase 1〜7 をすべて実装した（効果の要約の基盤・4 属性の構文と `E1031`／`E1032`／`W1008`・`alloc`・`trap`・`diverge`・関数型の引数（`calls_params`）・docgen／hover／api-diff と文書）。`SUPPORTED` は 14 個すべてで、`effect_attribute_unavailable` の経路は残したが発生しない。基盤は `main` の `a62c321f`。
- **経路 A の再検討（チケットからの逸脱）**: A19 は起票後に純粋な関数型 `pure (T -> U)`・特殊化前のテンプレート検査・信頼済みの callback 引数・`IO.run` の遅延へ進化しており、その走査を関数単位の要約で置き換えると `E1029` の受理・拒否が変わる（停止条件 3・11）。そこで `pure` の検査は `pure_rules.rs` のまま残し、`effects.rs` は属性のための独立した解析とした。組み込み関数の分類は、`effects::builtin_facts`・A19 の `pure_rules::builtin_impure`・G26 の `kg::effects::builtin_effect` の 3 表が一致することを単体テスト `every_builtin_has_a_verdict` で固定した（`kg` の `Bench.consume` を D-53 と A20 7.1 の `debug` にそろえた。G26 の既存テストは不変）。`needed` は属性だけを見る（`pure` だけのプログラムは A19 が検査する）。
- **HEAD との差の吸収**: `TypedExprKind` は 65（`SaturatingIndex` が増えた。効果なし）、`Builtin` は 252。増えた 6 個は `VecPushMut`（B6）・`VecView`（B1）・`AllocCounter`（B34、`debug`）・`VecSetCapacity`／`VecInsert`／`VecSplice`（新しい行 B38: `alloc`＋`trap`）。`VecTruncate` は負の個数で `BoundsCheck` するので B2（`dead_guard`）から B4（`trap`）へ移した。件数: 不純 91（効果 90＋`dyn` 1）・`alloc` 53・`trap` 30・免除 37・`diverge` 14・`cb` 16。`FRONTEND_FORMAT` は 9 → 10。
- **規則の精密化と保守化**（いずれも IR との突き合わせで健全性を確認）: ① 関数ごとの枠の上限（`MAX_FRAME_BYTES`）をリテラルの木が共有するので、その場に留まるリテラルの枠の合計が 64 KiB を超えたら `alloc`（チケットの式は 1 個ずつだけで、複数の大きいリテラルで不健全）。② `copies::sites` の `Environment`（関数値の環境の複製）は、呼び出しの callee（環境は借用で渡る）と、捕捉のないクロージャ・名前付き関数に束縛した局所なら `alloc` にしない。それ以外は why `environment-copy`（付録 J に追加。`{what}` は `copies a function value, whose environment may be heap allocated`）。③ `copy` は W1006 と同じく利用者のモジュールの暗黙の複製だけ（std の `Array.copy` 自身の本体の複製は明示の API）。④ `VALUE_CALL` は開いた（関数値を受け取る）呼び先からだけ伝える（閉じた関数の値の呼び出しは D5 により外から関数値が入らない）。⑤ Phase 6 の実引数の判定は関数型だけでなく関数値を運びうる型（レコード・リスト・`Maybe` など）にも及ぼし、値の呼び出しに渡す関数値を運ぶ引数も「呼ばれる」とみなす。
- **構文**: 読み込み口 `attributes()`（`@cpu` と 4 属性を任意の順序）。本体のない署名（`extern`・`def f :: T`）の直後の行で、宣言の列にある `@cpu`／効果の属性は、制約行の誤り（`E0002`）ではなく次の宣言の属性として読む（`@'a` の制約行の診断は不変）。
- **API の差**: `validate(module, Option<&FunctionEffects>, &[DeclaredAttribute])`（属性の位置と `const def` の判定に宣言の一覧が要るため）。`Project::source_paths` は足さず、既存の `Project::source_for_span` を使った。facts（G26）は Level B のまま: Level A（G26 付録 F2）への切り替えは brief カード・query・impact・図の 12 件のゴールデンを独立に作り直す必要があり、A20 の変更表（Phase 7 は docgen・semantic・api_diff）の外なので、G26 の追従として残す。A20 側は `FunctionEffects::rows`・`sites`・`computed` を提供済み。
- **検証**: `cargo fmt --all -- --check`・`cargo clippy --all-targets -- -D warnings` 成功。`cargo nextest run --locked` **2,484 passed / 4 skipped**（新規・変更テストを含む）。A20 の単体・統合テスト 70 件と、`tests/effects_alloc_conservative.rs`（**125 プログラム・18,230 関数**で違反 **0**、偽陽性 491。std の公開関数 **860 個すべて**がコーパスで実体化。掃引の fixture `tests/fixtures/effects_sweep` が 107 個を受け持つ）、`tests/effects_trap.rs`（論理トラップの箇所 **1,334**・継承文脈の論理トラップを持つ関数 **1,086** がすべて規則の箇所に対応）。コーパスの IR は build に加え、同じ解析結果の test・bench 実行器の IR も比べた。属性のない **123 プロジェクト**の `--emit llvm -O0` の IR は基準と byte 一致。`node tests/features.mjs target/debug/tsuzuri effect_attributes` **64 cases**（native／WASM × O0／O3）、`frontend_cache`・`docgen`・`lsp_sessions`・`e2e` 成功。`sh scripts/check-runtime-includes.sh` 58 files。
- **文書**: `values-and-functions/attributes.md`（属性 9 つ・効果の表明の節・実行例 2）、`compiler/diagnostics.md`（`E1031`・`E1032`・`W1008` と kind の表）、`values-and-functions/functions.md`、`ownership-and-memory/stack-and-heap.md`、`docs/language.md`・`docs/architecture.md`・`docs/stability.md`・`README.md`・`_features/GUIDE.md`（§4・D-55）、explain の `E1031`・`E1032`・`W1008`。`check-docs` 成功。
- **変更したファイル**: `src/effects.rs`（新規）、`src/check.rs`・`polymorph.rs`・`syntax.rs`・`parser.rs`・`parse_control.rs`・`syntax_codec.rs`・`frontend_cache.rs`・`formatter.rs`・`api_diff.rs`・`docgen.rs`・`semantic.rs`・`lsp.rs`・`ranges.rs`・`llvm.rs`・`llvm_frame.rs`・`llvm_escape.rs`・`diagnostic_kinds.rs`・`explain.rs`・`kg_capture.rs`・`kg/mod.rs`・`kg/effects.rs`、`src/explain/codes/{E1031,E1032,W1008}.md`、`tests/effects_{summary,attributes,alloc,alloc_conservative,trap,terminates,higher_order}.rs`・`tests/common/`・`tests/fixtures/{effect_attributes,effects_sweep}/`・`tests/{api_diff,formatter,docs,lsp,features.mjs}`。
