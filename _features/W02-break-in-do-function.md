# W02: `do!` を含む関数の `while` の中の `break` が `E1023` になる

| 項目 | 内容 |
| --- | --- |
| ID | W02 |
| 優先度 | P2（回避策はある: ループを別の関数に出すか、`do!` のない関数に分ける。Win32 のメッセージループのように、IO を挟むループを `main` に書くと必ず当たる） |
| 規模 | S |
| 依存 | – |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 不要（文書の規則「コンピュテーション式の境界を越えると `E1023`」（`loops-and-conditionals/while.md`）と `docs/language.md` の「ビルダー自身の `For`／`While` を越えて脱出しようとした場合」に実装を合わせる修正。境界でない所で拒否していたものを受け入れるだけで、受け入れていたプログラムの意味は変えない。台帳は [GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30)） |
| 手本にする既存実装 | `src/exceptions.rs` の `try` の検査（`finally` があるときだけ `normal_loop_depth` を `take` し、ないときはそのまま使う）、`src/closures.rs` の `lambda`（関数の境界で `take` する） |
| 主な影響ファイル | `src/computation.rs`（`check_implicit`・`Checker::implicit_root`）、`tests/control.rs`、`tests/fixtures/control/Main.tz`、`tests/control.mjs`、`spec/rules/CTL.md`、`tests/conformance/CTL/`、`_tsuzuri/language-reference/loops-and-conditionals/while.md`・`for-in.md`・`for-to.md`、`computation-expressions/computation-expressions.md`、`built-in-types-and-modules/io.md`、`async-tasks-and-lazy/tzroutine.md`、`docs/language.md` |

## 実装者向け早見

- 範囲: 結果型にビルダーがなく、`let!`・`do!` をその場で実行する本体（以下「直接実行の本体」: `def main :: unit -> i32`、`unit` を返す関数、`tzroutine { }` の本体、ループの本体のブロック、`IO` を直接実行するトップレベルのコード）の中の `while`・`for` に、同じ本体の `break`・`continue` を書けるようにする。
- 原因: `src/computation.rs` の `check_implicit` は、`$implicit.body.*` 以外のすべての暗黙の計算で `normal_loop_depth` を 0 にする。直接実行の本体は `direct_block` が通常のブロックに下げ、その中の `while`・`for` の本体、`if` の枝、`match!` の腕を `direct_computation`（`$implicit.root.$direct` の暗黙の計算）で包むので、ループの本体に入るたびに深さが 0 に戻る。
- 直し方: 直接実行と最初から分かる暗黙の計算（`$direct`、または強制されたビルダーがなく `direct_context(expected)` が真）では、`normal_loop_depth` を 0 にしない。ビルダーで組み立てる計算は今どおり 0 にする。
- 最初に読む: `src/computation.rs` の `check_implicit` → `Checker::implicit_root` の先頭の 3 行 → `direct_block` の `While`・`For`・`If` の枝と `direct_computation` → `src/check.rs` の `value_expression` の `ExprKind::Break | ExprKind::Continue`。
- 最初に書くテスト: `tests/control.rs` の `break_continue_in_bodies_that_run_io_directly`。
- 触ってはいけない: ビルダーで組み立てる計算（`IO { }`、`Maybe { }`、結果型が `IO<unit>` の関数の暗黙の計算）の `while`・`for` の中の `break`・`continue` は今どおり `E1023`。`E1023` の文と位置。
- 完了の見分け方: 次のプログラムが `n=5` を出して終了コード 5 で終わる（native と WASM、`-O0` と `-O3`）。

```tsuzuri
def main :: unit -> i32 = \() ->
    let mut n = 0
    while true do
        n = n + 1
        if n == 5 then break
    do! IO.writeln $"n={n}"
    n as i32
```

## 目的

`break`・`continue` は、ループの外、ラムダ式、`tzroutine { }` の本体、コンピュテーション式、`finally` つきの `try` の境界を越えるときだけ `E1023` になる、と文書は定める。直接実行の本体は IO の値を作らず、`let!`・`do!` を書いた場所で順に実行する通常のコードで、どの境界でもない。それなのに、関数のどこかに `do!` があるだけで、同じ関数の `while` の中の `break` が拒否される。`do!` の行を消すと通るので、利用者には理由が分からない。

直接実行の本体を、ループの深さについても通常のコードとして扱う。ビルダーで組み立てる計算は境界のまま残す。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief W02`。「現状」の再現を HEAD で繰り返し、同じ結果になることを確かめる。食い違えば、このチケットを直してから始める。
- `grep -n "normal_loop_depth" src/*.rs` の結果が「現状」の一覧と同じであること（ほかに 0 にする所が増えていれば、その意図を確かめる）。

### 停止条件

- **S1** 修正の後、ビルダーで組み立てる計算・ラムダ・`tzroutine { }`・`finally` を越える `break`・`continue` のどれかが受け入れられる（「仕様」の拒否の表）。
- **S2** 今受け入れているプログラム（`tests/fixtures`・`examples`・`tests/conformance`・std の例）のどれかで、IR か診断が変わる。とくに、直接実行の本体の深い入れ子のループの中の、列やチャンネルの `for` が `E1017`（`SequenceLoopDepthLimit`）になる（「落とし穴」）。
- **S3** 受け入れた `break`・`continue` の生成コードが、native と WASM、`-O0` と `-O3` で違う結果を出すか、`live != 0` になる。
- **S4** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

### 再現

```sh
mkdir -p target/scratch/w02/repro
# target/scratch/w02/repro/Main.tz に「実装者向け早見」のプログラムを書く
target/debug/tsuzuri.exe check target/scratch/w02/repro
#   ...Main.tz:5:24: error[E1023]: break can only target an enclosing for or while loop in the same
#   function, tzroutine, or computation; return a value across boundaries
```

- `do! IO.writeln $"n={n}"` の行を消すと `check` は通る。
- 配布物 0.1.6 の `tsuzuri.exe` でも同じ位置で `E1023`（文の「tzroutine」が「task」の古い文）。

### 形ごとの結果

`target/scratch/w-w01w02/` で確認し、確認後に消した。「直接」は直接実行の本体、「ビルダー」はビルダーで組み立てる計算。

| 形 | 本体 | 今の結果 | 修正後 |
| --- | --- | --- | --- |
| 再現（`while` と `break`、後ろに `do!`） | 直接（`unit -> i32`） | `E1023` | 受け入れる |
| `while` と `continue`（後ろに `do!`） | 直接 | `E1023` | 受け入れる |
| `for i in 1 ..= 10` と `break` | 直接 | `E1023` | 受け入れる |
| 配列の `for` と `continue`・`break` | 直接 | `E1023` | 受け入れる |
| ループの本体の中に `do!`、その後に `break` | 直接 | `E1023` | 受け入れる |
| 複数行の `if` の中に `do!` と `break`、`else` に `continue` | 直接 | `E1023` | 受け入れる |
| `match` の腕の `break` | 直接 | `E1023` | 受け入れる |
| 先頭に `let!`、その後のループの `break` | 直接 | `E1023` | 受け入れる |
| `def run :: unit -> unit` の中 | 直接 | `E1023` | 受け入れる |
| `do!` のない関数の、本体に `do!` を持つループの `break` | ループの本体が直接 | `E1023` | 受け入れる |
| `tzroutine { }` の中のループの `break`（本体に `do!`） | 直接 | `E1023` | 受け入れる |
| トップレベルの `do!` の後のループの `break` | 直接（`$entry` の `IO`） | `E1023` | 受け入れる |
| プロジェクトに利用者のビルダー（文書の `Identity.tc`）があるときの再現 | 直接（`Identity` は選ばれず、IR に `Identity.Bind` が出ない） | `E1023` | 受け入れる |
| `def run :: unit -> IO<unit>` の中 | ビルダー（`IO`） | `E1023` | `E1023`（変えない） |
| `do! IO { ... while ... break ... }` | ビルダー | `E1023` | `E1023` |
| `def pick :: unit -> Maybe<i64>` の中 | ビルダー（`Maybe`） | `E1023` | `E1023` |
| ループの中の `tzroutine { do! ...; break }` | `tzroutine` の境界 | `E1023` | `E1023` |
| ループの中の `let f = \() -> break` | ラムダの境界 | `E1023` | `E1023` |
| ループの中の `let r = try break with \| e -> e finally ()`（後ろに `do!`） | `finally` の境界 | `E1023` | `E1023` |
| ループの外の `break`（`do!` の後） | – | `E1023` | `E1023` |

### 原因（コード）

- `src/check.rs` の `Checker::value_expression`: `ExprKind::Break | ExprKind::Continue` は `self.normal_loop_depth == 0` なら `E1023`（kind `jump_outside_loop`）。
- `src/control.rs` の `Checker::control_expression`: `while` と `for` の本体を検査する間だけ `normal_loop_depth` を 1 増やす。
- 深さを 0 にする所: `src/closures.rs` の `lambda`（ラムダと `tzroutine { }` の本体。`src/check.rs` の `ExprKind::Routine` は `self.lambda` を通る）、`src/check.rs` の `Checker::expression` の `ExprKind::ComputationBoundary`（明示したビルダーの計算）、`src/exceptions.rs`（`finally` のある `try`）、そして `src/computation.rs` の `check_implicit` の `$implicit.root.*` の枝（`std::mem::take(&mut checker.normal_loop_depth)`）。
- `src/computation.rs` の `Checker::implicit_root`: `forced == Some("$direct") || (forced.is_none() && self.direct_context(expected))` なら `direct_root`、`$entry` でビルダーが `IO` なら `direct_root`、それ以外はビルダーで組み立てる（`Delay` のクロージャで包む）。`direct_context` は、期待する結果型を作るビルダーが無いとき真。
- `src/computation.rs` の `direct_block`: `let!` を `let x = IO.run io`、`do!` を `IO.run io` にした通常のブロックを作る。`ComputationStatementKind::While`・`For` は `ExprKind::While`・`For` にし、本体を `direct_computation`（`$implicit.root.$direct`）で包む。`If` の両方の枝と `Match`（`match!`）の腕も同じ。クロージャは作らない。
- そのため、直接実行の本体の `while` の本体に入ると、`control_expression` が 1 にした深さを、`check_implicit` が `$direct` のところで 0 に戻す。`if n == 5 then break` の `if` の枝も `$direct` なので、`while` の枝だけ直しても足りない。
- 経緯: 深さを 0 にする行は暗黙の計算を入れたコミット `16d4fa9a`（2026-09-28。暗黙の計算は常にビルダーで組み立てていた）のもので、直接実行（`direct_computation`）はその後の `38344d72`（2026-10-04）で同じ `check_implicit` を通るように足された。直接実行で深さを残す判断を書いた記録はない。
- ビルダーで組み立てる計算では、`while cond do C` は `B.While (\() -> cond) (B.Delay (\() -> C))` に下がる（`computation-expressions.md` の表、`src/computation.rs` の `Lowering` の `While` の枝）。ループはビルダーの `While` の中にあり、`C` は別のクロージャなので、`C` の中の `break` が指せるループは Tsuzuri のコードの中にない。`docs/language.md` の「制御構文」はこれを「ビルダー自身の `For`／`While` を越えて脱出しようとした場合」の `E1023` として書いている。

## 変えてはいけない意味

- 受け入れていたプログラムの意味と IR。修正は、今 `E1023` で拒否しているプログラムの一部を受け入れるだけ。
- `break` は最も内側のループを抜け、`continue` は次の反復へ進む。ジャンプの前に、評価を終えた一時値と、抜けるスコープの所有値を決まった順で解放する（`docs/language.md` の「制御構文」）。直接実行の本体でも同じ。
- 境界: ラムダ、`tzroutine { }`、明示したビルダーの計算、ビルダーで組み立てる暗黙の計算、`finally` つきの `try`。

## 仕様

- 直接実行の本体は、`break`・`continue` にとって境界ではない。その本体（入れ子の `if`・`match`・ループの本体を含む）の中の `break`・`continue` は、同じ本体の中で最も内側の `while`・`for` を指す。指すループがなければ今どおり `E1023`。
- 直接実行の本体かどうかは、今の `implicit_root` の判定のまま（結果型を作るビルダーが無い、または `direct_block` が作った入れ子の本体）。トップレベルの `$entry` はもともとループの中に現れないので、扱いを変えない（深さは 0 のまま）。
- ビルダーで組み立てる計算の中の `while`・`for` の文の本体の `break`・`continue` は `E1023` のまま（ビルダーの `While`・`For` を越えるため）。その計算の中の通常の式のループ（`Maybe { let value = { while true do break; 42 }; return value }`）は今どおり受け入れる。
- 診断の文・kind・位置は変えない。

## 設計

`src/computation.rs`:

```rust
impl Checker<'_> {
    /// Whether an implicit computation runs its IO binds where it is written (`direct_root`): `forced`
    /// is the builder name after `$implicit.root.`. Such a body is ordinary code of the enclosing
    /// function, so a loop around it stays the target of its `break` and `continue` (W02).
    fn runs_directly(&self, forced: Option<&str>, expected: Option<&Type>) -> bool {
        forced == Some("$direct") || (forced.is_none() && self.direct_context(expected))
    }
}

pub(super) fn check_implicit(...) -> Result<TypedExpr, Diagnostic> {
    ...
    } else {
        let forced = builder.text.strip_prefix("$implicit.root.");
        // A built computation is a boundary: its statements end up in the builder's closures.
        let outer_loop = if checker.runs_directly(forced, expected) {
            checker.normal_loop_depth
        } else {
            std::mem::take(&mut checker.normal_loop_depth)
        };
        ...（今のまま）
        checker.normal_loop_depth = outer_loop;
        result
    };
```

`Checker::implicit_root` の先頭の判定も `if self.runs_directly(forced, expected)` に置き換え、2 か所が同じ述語を使うようにする。`check_implicit` は `direct_context` を `scopes.push` の前に、`implicit_root` はその後に呼ぶが、どちらも推論の状態を変えないので同じ答えになる。

### 判定を検査の前に決める理由（採らない案）

- **ビルダーが決まってから深さを 0 にする**: `implicit_root` は、ビルダーが期待する型から決まらないとき、`implicit_prefix` で先頭の通常の文を検査してから、最初の `let!`・`do!` の値の型でビルダーを選ぶ。`Delay` を持つビルダーなら、その先頭の文もクロージャの中に入る。後から 0 にすると、先頭の文の `break` が、クロージャを越えて外のループを指したまま受け入れられる。
- **`direct_block` の `While` の枝だけを直す**: `if` の枝・`match!` の腕も `$direct` なので足りない。ループの本体のブロックが自分で `$implicit` の根になる形（「現状」の表の「`do!` のない関数の…」）も残る。
- **ビルダーの計算の `while` も、本体に `!` の文がなければ通常のループに下げる**: `computation-expressions.md` の対応表（`while` は常に `B.While`）を変え、利用者のビルダーの `While` の意味（数える、ログを取るなど）と IR を変える。このチケットの範囲を超える（要るなら別のチケットで、要承認）。

### 生成

変更なし。直接実行の本体の型付きの木は、`IO.run` の呼び出しを除けば、同じ形の通常の本体と同じ（`While` の本体の `Block` の中の `If` の `Break`）で、`src/llvm.rs` の `emit_loop_jump` は通常のループと同じに下げる。

## 前提とする他チケットのインターフェース

- なし。

## 他チケットへ提供するインターフェース

- なし（W06・W12 などの Win32 のメッセージループの例は、`main` の中に `while` と `break` と `do!` を一緒に書いてよい）。

## 実装手順

1. **変更**: ブランチ `fix/w02-break-in-direct-io-bodies`。基準の記録だけ。
   **内容**: 着手条件。`tests/fixtures`・`examples` の `Main.tz` を HEAD のコンパイラで `--emit llvm -O1`（native と `--target wasm32`）にし、`check --json` と一緒に `target/scratch/w02/before/` へ保存する。`node scripts/conformance.mjs target/debug/tsuzuri --area CTL` の件数を控える。
   **確認**: 保存した数を控える。
2. **変更**: `tests/control.rs` の新しいテスト（テスト計画）を先に書く。
   **内容**: 受け入れる形が今は `E1023` で失敗し、拒否の形は通ること。
   **確認**: `cargo test --locked --test control break_continue_in_bodies` が `running 1 test` で失敗する。
3. **変更**: `src/computation.rs`（設計）。
   **内容**: `runs_directly` と、`check_implicit`・`implicit_root` の 2 か所。
   **確認**: 手順 2 のテストが通る。T0。`cargo test --locked --test control`・`--test tzroutine`・`--test specs` の全件。手順 1 の IR と `check --json` が `cmp` で一致する（S2）。
4. **変更**: E2E（`tests/fixtures/control/Main.tz`・`tests/control.mjs`）と適合性テスト（`spec/rules/CTL.md`・`tests/conformance/CTL/`）。
   **内容**: テスト計画のとおり。
   **確認**: `node tests/control.mjs target/debug/tsuzuri`（native と WASM の `-O0`・`-O3`、`live == 0`）。`node scripts/conformance.mjs target/debug/tsuzuri --rule CTL-027`（番号は着手時の次の番号）。
5. **変更**: 文書（下）。
   **確認**: `node scripts/check-docs.mjs` に変更したページ、`node scripts/check-spec.mjs --require-markers`、`node scripts/gen-docs.mjs --check`。T1、T2。

## テスト計画

### Rust（`tests/control.rs`）

`break_continue_in_bodies_that_run_io_directly`: 既存の `accepts`（native と wasm32 の IR を作り、決定的であることを見る）と `rejects`。

受け入れる（`break` と `continue` の両方で書けるものは両方）:

- 早見の再現。`continue` 版（`while n < 10 do` の中で `if n % 2 == 0 then continue`、後ろに `do!`）。
- `for i in 1 ..= 10 do` の `break`、`for i in [1, 2, 3, 4, 5] do` の `continue` と `break`（後ろに `do!`）。
- ループの本体の中の `do! IO.writeln ...` の後の `break`。複数行の `if` の中の `do!` と `break`、`else` の `continue`。`match n with | 5 -> break | _ -> ()`。
- 先頭の `let! limit = IO.pure 7` の後のループ。`def run :: unit -> unit` の中。`do!` のない関数の、本体に `do!` を持つループ。`tzroutine { }` の本体の中のループ（本体に `do!`）。トップレベルの `do! IO.writeln "start"` の後のループと、最後の式。
- 入れ子: 外の `for` と内の `while` の両方の本体に `do!` があり、内の `break` が内だけを抜ける形。
- 利用者のビルダーを持つプロジェクト（`analyze_modules` に文書の `Identity.tc` と再現の `Main.tz`。`tests/control.rs` の既存のテストも `("Arrays.tc", ...)`・`("Flow.tc", ...)` の形でビルダーのファイルを渡している）。

拒否する（`E1023`。今と同じ）:

- `def run :: unit -> IO<unit>` の中の `while` の `break`（ビルダー `IO`）。`do! IO { ... while true do ... break ... }`。`def pick :: unit -> Maybe<i64>` の中のループの `break`。
- ループの中の `tzroutine { do! IO.writeln "x"; break }`。直接実行の本体のループの中の `let f = \() -> break`。直接実行の本体のループの中の `let r = try break with | e -> e finally ()`（`tests/specs.rs` の形）。直接実行の本体のループの外の `break`。

既存の `break_continue_type_contexts`・`break_continue_ir_shapes`・`break_continue_ownership` と `tests/tzroutine.rs`・`tests/specs.rs` の `E1023` の検査は、そのまま通ること。

### E2E（`tests/control.mjs`、fixture `tests/fixtures/control/Main.tz`）

WASM で import が増えないよう、`IO` は `IO.pure` だけを使う（`let! start = IO.pure 1` を持つ `export def` が wasm32 で import なしに動くことを、`target/scratch` で確かめた）。期待値は JS の参照（手で書いたループ）で作る。

| export | 内容 | 期待（参照） |
| --- | --- | --- |
| `direct_break :: own i64 -> i64` | `let! start = IO.pure 0`、`n` を 1 ずつ増やし、`n >= limit` で `break`、`n` を返す | `limit >= 1` なら `limit`、それ以外は 1（`limit` は −3・0・1・5・4096） |
| `direct_continue_owned :: own i64 -> i64` | `let! base = IO.pure 0`、`for i in 0 ..< count do` の中で `let text = $"item-{i}"`（所有値）、`i % 3 == 0` で `continue`、`i == 10` で `break`、それ以外で `total = total + text.length` | JS で同じ規則（`("item-" + i).length`）。`count` は 0・1・4・20 |
| `direct_nested :: i64` | `let! seed = IO.pure 1`、`for i in 1 ..= 3` の中の `while true` で `j` を増やし、`j > i` で `break`、そうでなければ `do! IO.pure ()` の後 `total = total + seed * 10`、内のループの後で `total = total + 1` | 63（`11 + 21 + 31`） |

- 各ケースの後の `live == 0`（native）で、`continue`・`break` の前の所有値（`text`）の解放を確かめる。
- CLI の確認を 1 つ: 早見の再現を一時ディレクトリに書いて `tsuzuri run` し、標準出力が `n=5\n`、終了コードが 5。

### 適合性テスト

- `spec/rules/CTL.md` に次の番号の規則（HEAD では `CTL-027`）: 「`break` and `continue` in a body that runs `let!` and `do!` in place (its result type has no computation builder) MUST target the loops of that body; the body of a `while` or `for` statement of a computation built by a builder MUST NOT be such a body (E1023).」
- `tests/conformance/CTL/CTL-027-break-in-direct-io-body.tz`（早見の再現の最後の式を `0` にした `def main`。`// expect: stdout "n=5\n"`）と `CTL-027-break-in-built-computation.tz`（`def run :: unit -> IO<unit>` のループの `break`、`// expect: check-error E1023 line=N`）。トップレベルのコードで「ループの後に `do!`」と書くと、[W31](W31-top-level-loop-before-do.md) が入るまでは「対象外」の `E1018` に当たるので使わない（W31 の後はこの注意は要らない）。
- 文書の規則の文の直前に `<!-- rule: CTL-027 -->`（`while.md` の該当の段落）。

## ドキュメント

- `_tsuzuri/language-reference/loops-and-conditionals/while.md`（105 行目付近）・`for-in.md`（347 行目付近）・`for-to.md`（220 行目付近）: 「コンピュテーション式」の境界の文に、「`let!`・`do!` をその場で実行する本体（`def main :: unit -> i32` など、結果型にビルダーのない本体と `tzroutine { }` の本体）は境界ではなく、その中のループには `break`・`continue` を書ける。`IO { }`・`Maybe { }` や結果型が `IO<T>` の本体のように、ビルダーで組み立てる計算の `while`・`for` の文は、ビルダーの `While`・`For` になるので、その本体からの `break`・`continue` は `E1023`」を足す。`while.md` に `run=` の例（早見の再現。標準出力 `n=5`）を 1 つ。
- `computation-expressions/computation-expressions.md`: 直接実行の段落（260 行目付近）に「その本体の `while`・`for` は通常のループで、`break`・`continue` を書ける」、「まとめ」（407 行目付近）の `while` の行に同じ注記。
- `built-in-types-and-modules/io.md`: 「main とコンピュテーション式」（64 行目付近）に同じ 1 文。
- `async-tasks-and-lazy/tzroutine.md`（76 行目付近）: 「本体の中のループには `break`・`continue` を書ける。外のループへは出られない（`E1023`）」に直す。
- `docs/language.md` の「制御構文」（2821 行目付近）: 直接実行の本体が境界でないことを 1 文。`E1023` の表（5093 行目付近）と `compiler/diagnostics.md`（306 行目付近）は、書いてあることが誤りにならないので、文を変えない（必要なら「ビルダーで組み立てる計算の `For`／`While`」と明確にする）。
- `node scripts/gen-docs.mjs`（`compiler/conformance.md` の生成の表が規則を数えるなら）。README は変えない。

## 受け入れ条件

- [ ] 「現状」の表の「受け入れる」の形がすべて受け入れられ、「`E1023`（変えない）」の形はすべて同じ文・位置の `E1023` のまま。
- [ ] `tests/control.rs` の新しいテストと、`tests/control.rs`・`tests/tzroutine.rs`・`tests/specs.rs` の既存の全件が通る。
- [ ] `node tests/control.mjs target/debug/tsuzuri` が native と WASM の `-O0`・`-O3` で通り、`live == 0`。早見の再現の `tsuzuri run` が `n=5` と終了コード 5。
- [ ] 今受け入れているプログラム（`tests/fixtures`・`examples` のすべての `Main.tz`）の IR・object・診断が 1 byte も変わらない（手順 1 との比較）。新しい `E1017` がない。
- [ ] 適合性テスト（`CTL-027`）、`check-spec --require-markers`、変更した全ページの `check-docs` が通る。
- [ ] T2 が通る。

## 落とし穴

- **`$direct` だけを直さない**: 関数の本体には `do!` がなく、ループの本体にだけ `do!` がある形では、ループの本体のブロックが自分で `$implicit` の根になり、期待する型 `unit` で直接実行になる（「現状」の表の「`do!` のない関数の…」）。`forced.is_none() && direct_context(expected)` も含める。
- **`direct_context` の答えは利用者のビルダーで変わる**: 期待する結果型を作るビルダーがあれば、その本体はビルダーで組み立てる計算になり、境界のままでよい（`implicit_root` と同じ述語を使えば、検査と下げ方が食い違わない）。2 か所に同じ式を書き写さず、`runs_directly` を共有する。
- **深さの上限の見積もりも変わる**: `src/control.rs` の `channel_loop`・`sequence_loop` は `body.depth + 12 * (normal_loop_depth + 1)` を `MAX_NESTING`（128）と比べ、超えれば `E1017`（`SequenceLoopDepthLimit`）。今は直接実行の本体の入れ子のループごとに深さが 0 に戻るので、見積もりが実際より小さい。修正後は、同じコードを `do!` のない本体に書いたときと同じ見積もりになる。届くのは、列かチャンネルの `for` の外に 10 段ほどのループがある形だけ。コーパスに当たるものがあれば S2 で止める。
- **`$entry` は 0 のまま**: トップレベルのコードはループの中に現れない。`$entry` でビルダーが `IO` に決まるのは `implicit_prefix` の後なので、そこで判定を変えようとしない。
- **`IO.run` の後の `break`**: 直接実行の `do!` は `IO.run io` の呼び出しで、クロージャではない。`break` の前に解放する一時値に `IO` の値が残らないことを、E2E の `live == 0` で確かめる。

## 対象外

- ビルダーで組み立てる計算の `while`・`for` の文の本体で `break`・`continue` を使えるようにすること（ビルダーの `While`・`For` の意味を変える。「設計」の採らない案）。`E1023` の文に、その場合の `help`（「このループはビルダーの `While`。ループを関数に出すか、ビルダーのない本体に書く」）を足すことも、診断の基準を動かすので別にする。
- **トップレベルのコードで、最初の `let!`・`do!` より前にループがあると `E1018` になる**: `let mut n = 0` → `while n < 3 do n = n + 1` → `do! IO.writeln $"n={n}"` → `n` は、`break` がなくても `E1018`（cannot infer the computation builder）。`implicit_root` の `implicit_prefix` が `let` と式の文で止まり、`while` の文の後ろの `do!` を見ないため。ループを `do!` の後に置けば通る。W02 とは別の不具合で、[W31](W31-top-level-loop-before-do.md) が直す。変える行は重ならず、どちらが先でもよい（W02 を先に入れることを推奨）。後から入る方の PR が、組み合わせの形（トップレベルの `while true do` のループの中に `if n == 3 then break`、ループの後に `do!`、式で終える）をテストに足す（W31 の D6）。
- `break` に値やラベルを付けること。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 直接実行の本体は `break`・`continue` の境界にしない。判定は `implicit_root` と同じ述語（`runs_directly`）で、本体を検査する前に決める | 直接実行の本体は IO の値を作らず、クロージャもない通常のコードで、文書の境界（ラムダ・`tzroutine`・コンピュテーション式・`finally`）のどれでもない。後から決めると、`Delay` のクロージャに入る先頭の文の `break` を誤って受け入れる | 既定案（実装者はこの案に従う） |
| D2 | ビルダーで組み立てる計算の `while`・`for` の文の本体の `break`・`continue` は `E1023` のまま | `while` は常に `B.While` で、本体は `Delay` のクロージャ（`computation-expressions.md` の表、`docs/language.md` の「ビルダー自身の `For`／`While`」）。変えると利用者のビルダーの意味と IR が変わる | 既定案（実装者はこの案に従う） |
| D3 | `E1023` の文・kind・位置は変えない | 受け入れる範囲を直すだけで、残る拒否の理由は今の文で正しい。診断の基準を動かさない | 既定案（実装者はこの案に従う） |
| D4 | 規則 `CTL-027`（着手時の次の番号）と 2 つの適合性テストを足す | 境界の範囲は文書の規則で、実装が外れたことを CI（`conformance.mjs`）で捕まえるため（D-58 の形式） | 既定案（実装者はこの案に従う） |
