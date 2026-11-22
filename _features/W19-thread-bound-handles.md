# W19: スレッドに結び付くハンドル（`extern type local`）と、固定された文脈でだけ使える検査

| 項目 | 内容 |
| --- | --- |
| ID | W19 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | [W18](W18-copy-extern-type.md)（`HandleKind` と修飾の読み方） |
| 後続 | [W25](W25-extern-interface-com.md)（`extern interface` の既定が `local`）、[W27](W27-win32-package-from-winmd.md)（スレッドに結び付くハンドルを `local` に生成） |
| 状態 | todo |
| 起票 | 2026-11-22（D-63。利用者の依頼「X: D13 を狭く改める方針で決定。実現向けたチケットを @_features に W プリフィックスのチケットを作成して」と「リスクについては修正が必要なものは修正用チケットを、改善が必要なものはあなたが考える中で最も良いと考えられる方法での改善チケットを作成して」から起票。HEAD `fa40eede` で確認） |
| 承認 | 要承認（D-55、効果の語彙 D-53）。台帳は [GUIDE D-63](GUIDE.md#d-63-tsuzuri-だけで-win32-のアプリを書くe12-d13-の狭い改訂w01w30) |
| 改善する劣位 | リスク「スレッドへの固定」: M:N の tzroutine は、固定しなければ待つたびに（POSIX では横取りでも）別の OS スレッドで続く。HDC・STA の COM・OpenGL の文脈のように、受け取ったスレッドで使い続けなければならないホストの値を tzroutine で取り出し、使い、解放しても、今のコンパイラは何も言わない（`extern type` は Send で、`Routine.with_locked_thread` を使うかは利用者の注意だけ） |
| 手本にする既存実装 | `src/domain_rules.rs`（特定の組み込みに渡した関数を型付き IR で探し、要約で判定する）、`src/effects.rs` の `Effect`・`EffectSet`・本体の走査（`self.local.edges`）・`builtin_facts`（wildcard のない分類）、`Routine.with_locked_thread`（[TZ08](_completed/TZ08-blocking-calls-netpoller.md) Phase 3）、W18 の `HandleKind` |
| 主な影響ファイル | `src/syntax.rs`・`src/parser.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs`・`src/check.rs`・`src/effects.rs`・`src/pinned_rules.rs`（新規）・`src/polymorph.rs`・`src/diagnostic_kinds.rs`・`src/diagram/calls.rs`・`src/api_diff.rs` などの見出し（W18 と同じ所）・`tests/thread_bound_handles.rs`（新規）・`tests/fixtures/thread_bound/Main.tz`（新規）・`tests/ffi_extensions.mjs`・`tests/ffi_extensions_host.c`・`tests/windows.mjs`・`src/effects.rs` の単体テスト `effect_names_are_sorted_and_round_trip`（語彙の数 14 を固定）・`spec/rules/TSK.md`・文書 |

## 実装者向け早見

- 範囲: `extern type local Hdc` を Copy でない・捕捉できない・Send でない・Sync でないハンドルにする。さらに、`local` の値に触れるコード（新しい効果 `pinned`）を、OS スレッドが固定された文脈の外で走らせうる関数値を `E1013` で拒む。生成コードは 1 byte も変えない（検査だけ）。
- 最初に読む: ① 仕様 3（固定された文脈と閉じ込めの規則）② 現状の「ランタイム」と「効果の要約」③ `src/domain_rules.rs` ④ `src/effects.rs` の `K::Closure`・`K::Function` の辺と `user_drop`。
- 最初に書くテスト: `tests/thread_bound_handles.rs` の `spawned_code_may_not_touch_local_handles`（下の例の 2 つ目の `tzroutine` だけが拒まれる）。
- 触ってはいけない: `local` を宣言しないプログラムの IR・診断・facts・hover（要約を計算しない）。ランタイム（`tzroutine.c`）。修飾なし・`copy` のハンドルの規則。
- 完了の見分け方:

```tsuzuri
extern type copy Hwnd
extern type local Hdc
@trusted
extern "GetDesktopWindow" def desktop :: unit -> Hwnd
@trusted
extern "GetDC" def get_dc :: Hwnd -> Hdc
@trusted
extern "ReleaseDC" def release_dc :: Hwnd -> Hdc -> i32

def touch :: Hwnd -> i32 = \h -> release_dc h (get_dc h)

def main :: unit -> i32 = \() ->
    let h = desktop ()
    tzroutine {
        let _ok = Routine.with_locked_thread (\() -> touch h)   // 通る
    }
    tzroutine {
        let _bad = touch h                                       // E1013（pinned_code_spawned）
    }
    touch h                                                      // 通る（main のスレッド）
```

## 目的

`!Send` だけではスレッドに固定されない。値をほかの tzroutine へ渡せなくても、tzroutine の中で `GetDC` から `ReleaseDC` までの間に待てば（POSIX では横取りされれば）、G は別の M で続き、ホストの契約（同じスレッドで解放する、STA のスレッドでだけ呼ぶ）を破る。このチケットは、スレッドに結び付くハンドルを型で宣言でき、それに触れるコードが固定された文脈でだけ走ることを、コンパイル時に保証する。利用者にとっては、`Routine.with_locked_thread` の付け忘れが実行時の不具合でなく `E1013` になる。

## 着手条件と停止条件

### 着手条件

- W18 が done（`grep -n "enum HandleKind" src/syntax.rs` が 1 件、`Type::Handle(_, _)` が種類を持つ）。承認（決定事項 D1〜D3）。
- `grep -n "pub const ALL: \[Effect; 14\]" src/effects.rs` が 1 件で、語彙が 14 個のままであること（違えば現在値から数える。GUIDE-LITE §11）。
- `node scripts/repo-kb.mjs brief W19`、D-53・D-61 を読む。

### 停止条件

- **S1** 効果の要約に `pinned` を足すと、`local` を宣言しないプログラムの `check --json`・facts・hover・`--emit llvm` が 1 byte でも変わる。
- **S2** `local` を使わない既存のプログラム（`tests/fixtures/*`・`examples/*`・std の例）に、閉じ込めの規則（仕様 3）が 1 件でも誤りを出す（`thread_bound` が偽なら何も検査しないはず）。
- **S3** `dyn` の vtable のメソッド（`CheckedModule::vtables`）か、値の drop が走らせる `Drop`（`effects.rs` の `user_drop` の辺）を型付き IR から求められず、閉じ込めに穴が残る。穴のまま出さない。
- **S4** 要約の資源上限（`effects::Limits`）に、`local` を使う現実の大きさのプログラム（`tests/fixtures/thread_bound`）が届く。
- **S5** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `fa40eede` で確認）

### ランタイム（`src/runtime/tzroutine.c`）

- G を M に固定する手段は `tzr_lockm`（`g->lockedm` を立てる）だけで、呼ぶのは `tsuzuri_tz_callback_enter`（ブロッキングの `extern` の中で呼ばれたコールバック・export）と `tsuzuri_tz_lock_thread`（`Routine.with_locked_thread`）の 2 つ。
- スケジューラーの M でないスレッド（`main` のスレッド、export を呼んだホストのスレッド）は擬似 G（`tzr_pseudo`）で、待つときは自分の OS スレッドを止める。`tsuzuri_tz_lock_thread` の注釈は「A pseudo-G is its thread already」（D-61 の実行モデルも同じ）。
- `extern` の呼び出しの中では G は M を離れない（`tzr_entersyscall` の注釈「the G never leaves its M inside the call」）。待ちから戻った G は別の M で続きうる（`routine.md`「tzroutine は待つたびに別の OS スレッドで続くことがある」）。POSIX の横取り（TZ10 Phase 2）は生成コードの任意の命令で G を止め、「on whichever thread the G runs by then」で続ける。Windows には信号による横取りがない（`routine.md`）。
- tzroutine を起動する native のプログラムでは、`src/llvm.rs` の `with_blocking_externs` がすべての `extern` の呼び出しを `tsuzuri_tz_blocking_enter`／`_exit` で挟み、コールバックと export の本体を `tsuzuri_tz_callback_enter`／`_exit` で挟む。

### 型と組み込み

- `Type::can_send` は `Type::Handle(..)` を真にする（E12 D3）。`Routine.with_locked_thread :: (unit -> 'a) -> 'a` は `'a` に制約がない（`src/check.rs` の `RoutineWithLockedThread` の scheme）。
- 開発版（`target/debug/tsuzuri.exe check`）での再現: 次のプログラムは終了コード 0 で通る。`Hdc` を `tzroutine { }` へ move する、tzroutine の中で `GetDC`・`ReleaseDC` を呼ぶ関数を呼ぶ、`Routine.with_locked_thread` から `Hdc` を返す、の 3 つとも、`Hdc` を `local` にすればこのチケットの後は `E1013` になる（テストの拒否の例に使う）。

```tsuzuri
extern type Hwnd
extern type Hdc
@trusted
extern "GetDesktopWindow" def desktop :: unit -> Hwnd
@trusted
extern "GetDC" def get_dc :: ref Hwnd -> Hdc
@trusted
extern "ReleaseDC" def release_dc :: ref Hwnd -> Hdc -> i32

def paint :: unit -> i32 = \() ->
    let h = desktop ()
    let dc = get_dc (ref h)
    release_dc (ref h) dc

def main :: unit -> i32 = \() ->
    let h = desktop ()
    let dc = get_dc (ref h)
    tzroutine {
        let _r = release_dc (ref h) dc
    }
    tzroutine {
        let _r = paint ()
    }
    let back = Routine.with_locked_thread (\() -> get_dc (ref (desktop ())))
    release_dc (ref (desktop ())) back
```

- `polymorph.rs` の `validate_instance` は、Send でない型のうち `Rc` も `Arc` も持たないものを「values that move to another tzroutine or thread must be owned; … contains a reference」（kind `task_requires_owned_value`）で報告する。Send でないハンドルがあると、この文が誤った理由を言う。

### 効果の要約（`src/effects.rs`、A20・D-53）

- `Effect` は名前順の 14 個（`Effect::ALL: [Effect; 14]`）、`EffectSet(u16)`、内部の `Bits` は 17 ビット（`DYN = 14`・`UNRESOLVED = 15`・`VALUE_CALL = 16`）。単体テストが 14 を固定している。
- 関数の参照（`K::Function(FunctionRef::User(id))`）とクロージャの生成（`K::Closure`）は、呼ばなくても辺を張る（`docs/language.md` の効果の節「関数値として渡すだけでも」）。値の drop は `user_drop` が `Drop` の実装へ辺を張る。
- A20 の要約は、効果の属性があるとき（`effects::needed`）と、属性を持つ関数の hover（`src/lsp.rs` の `inferred_effects`。属性のない関数には効果を出さない）のときだけ計算する。facts の効果は `src/kg/facts.rs` が `src/kg/effects.rs`（Level B）で別に数える。`builtin_facts` は wildcard のない表で、`RoutineWithLockedThread` は `callbacks: true`。
- `src/kg/effects.rs`（Level B）は語彙の一部だけを数え、`effects_complete` は偽。`src/diagram/calls.rs` の `IO_EFFECTS` は 10 個の名前を並べる。

## 変えてはいけない意味

- 生成コード・ランタイム・ABI・C ヘッダー・WASM。`local` は値の表し方を変えず（E12 D4）、検査は拒むだけである。
- `local` を宣言しないプログラムの診断・facts・hover。`pure`・効果の属性が禁じる集合（`pinned` を足さない）。
- 固定の範囲（TZ08 D7: `with_locked_thread` の関数の間だけ）とコールバックの P の扱い。

## 仕様

### 1. 構文

`extern type local Name`（`private` も可）。読み方は W18 の仕様 1 と同じ（`type` の直後で同じ行に識別子が続くときだけの文脈キーワード）。`extern type copy local X`・`extern type local copy X` は `E0002`（`extern_type_modifier_conflict`）。行末で終わる `extern type local copy` は、名前 `copy` の `local` ハンドルである（`extern type copy local` も同じく名前 `local` の `copy` ハンドル）。

### 2. 型規則

| 性質 | `local` |
| --- | --- |
| Copy・捕捉・Send・Sync | すべて偽。それを持つ record・union・配列・タプルも同じ |
| ABI の位置・表現・WASM | 修飾なしのハンドルと同じ（E12 D4・D5）。WASM でも使える |
| 平たいデータ・`const`・instance | 修飾なしと同じ（置けない、持たない） |

- 誤りの文（既存の kind に新しい枝。`local` を持つ型だけが通る）: tzroutine・`Channel` などへ移す → `E1013` `task_requires_send`「values that move to another tzroutine or thread must be Send; {ty} holds a thread-bound extern handle ('extern type local'), which must stay on the OS thread that received it; use it there, or do the work inside 'Routine.with_locked_thread'」。関数値への捕捉 → `E1005` `capture_unsendable_in_function_value`「cannot capture {ty} in a function value; function values may move to other tzroutines or threads, and a thread-bound extern handle ('extern type local') must stay on its OS thread; pass it as an argument」。共有（`Sync`）は今の `task_requires_sync` の文のまま。

### 3. 固定された文脈と効果 `pinned`

- **直接の源**: 特殊化とクロージャの下げの後の関数のうち、引数・捕捉・結果の型か、本体のどれかの式の型が `local` のハンドルに届く（`Type::holds_thread_bound`）もの。理由の名前は `local-handle`。`with_locked_thread` が `local` を返せば、その呼び出しの式が呼んだ側の源になる（固定の外へ持ち出せない）。
- **伝播**: 今の要約の規則（呼び出し・参照・`user_drop` の辺、強連結成分）。ただし次の **固定する位置** から張る辺は `pinned` 以外を運ぶ:
  (a) `Routine.with_locked_thread` の直接の関数引数（ラムダか関数名）、(b) コールバックを取る `extern` の呼び出しのコールバックの位置の関数名、(c) W22 の `FnPtr.of name`（W22 が入ったら）。
  要約は強連結成分ごとに 1 つなので、固定する位置の辺を成分の中に残すと、`f` が `with_locked_thread (\() -> g ())` を呼び、`pinned` な `g` が `f` を呼ぶ再帰で `f` まで `pinned` になる（誤った拒否）。そこで `pinned` のビットだけは、固定する位置の辺を除いたグラフで強連結成分と不動点を求め直す。ほかの効果は今どおりすべての辺で求める。
- **固定された文脈**: 関数値を通らずに到達するコード。D-61 の境界（トップレベルのコードと `def main`・`export def`・`test`・`bench`）は擬似 G で走り、(a)〜(c) の本体は固定されて走る（`tzr_lockm`）。
- **閉じ込めの規則**: 要約に `pinned` を持つ関数の値（クロージャ、値として使う関数名、部分適用、`dyn` の vtable）は、次の位置でだけ作れる。
  1. 直接の呼び出しの呼ばれる側（`f x`・`x |> f`・その場で呼ぶラムダ）。
  2. 固定する位置 (a)〜(c)。
  3. **同期の位置**: 呼んだ G の上で戻る前に呼び、保持しない組み込みの直接の関数引数（`pinned_rules::builtin_position` の表。`Mutex.with_lock`・`RwMutex.with_read`・`with_write`・`OnceValue.get`・`Cond.wait_with`・`List.map`・`Array.sort_by` など。W23 が `Anchor.read`・`update` を足す）。ここからの辺は `pinned` を運ぶ（呼んだ側も固定された文脈でなければならない）。
  それ以外（`let` の束縛、record の field、利用者と std の関数の引数、`Owned.function`、`tzroutine { }` の本体、`Routine.go`・`WaitGroup.go`・`Routine.scope`・`Time.after_func`・`Parallel.*` の関数引数、`>>`、保持する部分適用、`Dyn.of`）で作れば `E1013`。
- **診断**（`E1013`、G23 の登録簿に 2 つ）:
  - `pinned_code_spawned`（作った位置が `tzroutine { }` の本体か、`Builtin::spawns()` または `Parallel.*` の関数引数）: 「this starts code that uses the thread-bound extern type '{H}' on another tzroutine or thread, which may run on any OS thread; do that work inside 'Routine.with_locked_thread' in the new tzroutine, or run it on the calling thread」。
  - `pinned_function_value`（それ以外）: 「this function value runs code that uses the thread-bound extern type '{H}', and a function value may reach another tzroutine or thread; call the function directly, pass it straight to 'Routine.with_locked_thread', or use 'for' or 'match' instead of a callback」。
  - どちらも位置は作った式。`related`（D-51）に源の位置を、`help` に源までの関数の連なり（`FunctionEffects` の `Witness`）を付ける。
- 要約が上限（`effects::Limits`）を超えたら、今の要約と同じ `E1017` を返す（検査を緩めない）。検査は target に依らない（native と WASM で同じプログラムを受け入れる）。

## 設計

| ファイル | 変更 |
| --- | --- |
| `src/syntax.rs`・`src/parser.rs`・`src/syntax_codec.rs`・`src/frontend_cache.rs` | `HandleKind::Local`、修飾の語に `local`、`FRONTEND_FORMAT` を 1 上げる |
| `src/check.rs` | `is_copy`・`can_capture`（2 つの枝）・`can_send`・`sync_here` の `Local` を偽。`Type::holds_thread_bound(&self, types) -> bool`（`types.reaches` で `Handle(_, Local)` を探す）。`CheckedModule` に `thread_bound: bool`（`local` を宣言したモジュールがあるか）。`check` の段の `domain_rules::validate` の後に `pinned_rules::validate` |
| `src/effects.rs` | `Effect::Pinned`（名前 `pinned`、名前順で `Os` と `Rc` の間）。`ALL` を 15、`EffectSet::ALL`、`BITS = 18`（`DYN`・`UNRESOLVED`・`VALUE_CALL` を 1 ずつ後ろへ）。`Pinned` の源は `module.thread_bound` のときだけ数える（数えないなら走査も型の照合もしない）。辺に「`pinned` を運ばない」印を足し、固定する位置 (a)〜(c) の辺をそれで張る。`computed` が `Pinned` を含むときは、印の付いた辺を除いたグラフで `pinned` のビットだけの強連結成分と不動点を別に求める（仕様 3 の再帰の例）。`PURE_FORBIDDEN` と `forbidden` は変えない |
| `src/pinned_rules.rs`（新規） | `validate(module) -> Vec<Diagnostic>`: `module.thread_bound` でなければ空（時間の段 `crate::timings::Phase::start("pinned")` もそのときだけ始める）。`effects::summarize(module, Options::new(Scope::All, EffectSet::of(&[Effect::Pinned])))` の後、利用者と std の関数の本体（インスタンス）を走査し、関数の値の生成（`Closure`・呼ばれる側でない `Function(User)`・部分適用）とその位置を集め、`pinned` を持つものが位置 1〜3 になければ診断。`x |> f` の `f`（`Binary(BinaryOp::Pipe, ..)` の右）は、`domain_rules::uses` と同じく呼ばれる側として扱う。`module.vtables` の slot に `pinned` があれば、その stored type を包む `Dyn.of` の呼び出しの位置で同じ診断。`builtin_position(Builtin) -> Position { Pinning, Inline, Escaping }` は `builtin_facts` と同じく wildcard のない `match` |
| `src/polymorph.rs` | `validate_instance` の Send・Capture の枝の前に `holds_thread_bound` の枝（仕様 2 の文） |
| `src/diagram/calls.rs` | `IO_EFFECTS` に `pinned`（ホストに結び付く関数を `io` の色に） |
| 見出し（W18 の表の所） | `extern type local {name}`。`api-diff` は `local` を足すのを互換でない、外すのを互換とする |

- `src/kg/effects.rs`（Level B）は `pinned` を数えない（語彙の一部だけの段で、`effects_complete` は今も偽）。
- 実行時の費用はない（生成コードを変えない）。性能の主張はしない。

## 前提とする他チケットのインターフェース

- W18: `syntax::HandleKind`（`Owned`・`Copy`）、`Type::Handle(Box<str>, HandleKind)`、`ExternTypeDecl::kind`・`kind_span`、修飾の語を 1 つ読む `Parser::extern_declaration`、`E0002` の kind `extern_type_modifier_conflict`。名前が違えば止めて報告する（GUIDE-LITE §11）。

## 他チケットへ提供するインターフェース

- `HandleKind::Local` と `Type::holds_thread_bound`。W25 は `extern interface` の値（既定で `local`）をこの述語に足し、Send・Sync・捕捉と `pinned` の源を同じ規則にする。
- `effects::Effect::Pinned` と `pinned_rules::builtin_position`。W22 は `FnPtr.of` を `Pinning`、W23 は `Anchor.read`・`Anchor.update` を `Inline`、W25・W26 は COM の呼び出しの組み込みを分類して足す。
- `E1013` の kind `pinned_code_spawned`・`pinned_function_value`。

## 実装手順

1. **変更**: ブランチ `feat/w19-thread-bound-handles`。**内容**: 着手条件。`tests/fixtures/*`・`examples/*` の `check --json`・`--emit llvm`（native と wasm32）と、効果の属性を持つ fixture の hover（`tests/lsp.rs` の形。hover が効果を出すのは属性のある関数だけ）を `target/scratch/w19/before/` に保存。**確認**: 組の数を控える。
2. **変更**: 構文と型規則（設計の 1・2 行目、`polymorph.rs`）。**内容**: 仕様 1・2（`local` の読み方、4 つの性質、Send・捕捉の誤りの文の枝）。**確認**: `cargo test --locked --test thread_bound_handles local_handles_` の型規則の件（`running N tests` を見る）。
3. **変更**: `src/effects.rs`（`Pinned`・ビット・辺の印）。**内容**: 仕様 3 の源と伝播（固定する位置の辺を除いたグラフでの `pinned` の伝播）。**確認**: `effect_names_are_sorted_and_round_trip` の 14 を 15 に直して通し、`cargo test --locked --test effects_summary --test effects_higher_order --test effects_attributes` が変わらず通る。手順 1 の保存と hover・`check --json` が一致（S1）。
4. **変更**: `src/pinned_rules.rs` と `check` の段。**内容**: 仕様 3。**確認**: `tests/thread_bound_handles.rs` の全件。手順 1 の保存と全組が一致（S1・S2）。
5. **変更**: `tests/fixtures/thread_bound/Main.tz`・`tests/ffi_extensions_host.c`・`tests/ffi_extensions.mjs`・`tests/windows.mjs`。**内容**: テスト計画の E2E（スレッドの ID を照合するホストの関数と 64 個の tzroutine、Windows の早見の例）。**確認**: E2E（テスト計画）。
6. **変更**: 文書・`spec/rules/TSK.md`・`tests/conformance/TSK/`・見出し・`vsc/syntaxes/tsuzuri.tmLanguage.json`（`local` を W18 の正規表現に足す）。**内容**: 「ドキュメント」の節と `api-diff` の互換の判定。**確認**: T2。PR に `ci:full`。

## テスト計画

### Rust（`tests/thread_bound_handles.rs`、新規）

| テスト | 受理 | 拒否（コードと kind） |
| --- | --- | --- |
| `local_handles_are_not_copy_send_sync_or_captured` | `ref` での使用、`main` の中の move | 2 回目の使用 `E1012`、捕捉 `E1005` `capture_unsendable_in_function_value`、`tzroutine { }` へ move・`Channel.make` の要素 `E1013` `task_requires_send`、`Routine.scope (ref dc)` `E1013` `task_requires_sync`、`extern type copy local X`・`extern type local copy X` `E0002` `extern_type_modifier_conflict`（行末の `extern type local copy` は受理） |
| `spawned_code_may_not_touch_local_handles` | 早見の 1 つ目と 3 つ目、`export def` の本体、コールバックの位置の `pinned` な関数を tzroutine の中の `extern` に渡す、`f` が `with_locked_thread (\() -> g ())` を呼び `pinned` な `g` が `f` を呼ぶ相互再帰の `f` を tzroutine から呼ぶ（仕様 3 の再帰の例） | 早見の 2 つ目、`Parallel.map` の関数、`Routine.scope` の関数、`tzroutine` の中で `with_locked_thread` が返した `Hdc` を使う（いずれも `pinned_code_spawned`） |
| `pinned_function_values_stay_where_they_run` | `Mutex.with_lock` に `pinned` なラムダ（`main` の中）、直接の呼び出し、`x |> touch_dc` | `let f = \() -> touch h`、`Owned.function (\() -> touch h)`、利用者の高階関数・`Maybe.map` へ渡す、`dyn` の vtable のメソッドが `pinned`（いずれも `pinned_function_value`）、`copy` ハンドルだけを持ち `Drop` が `pinned` な record を tzroutine へ移す（`pinned_code_spawned`） |
| `programs_without_local_handles_compute_nothing` | `local` のない tzroutine のプログラムで要約が計算されない（`TSUZURI_TIME_PASSES=1` の出力に段 `pinned` がない） | – |

- 期待のコードと kind は仕様から書く（今の出力を写さない。GUIDE-LITE §2 規則 10）。

### E2E

- `tests/ffi_extensions_host.c` に `w19_ctx_new`（作ったスレッドの ID を記録）・`w19_ctx_same`（同じスレッドなら 1）・`w19_ctx_free`。`tests/fixtures/thread_bound/Main.tz` は、64 個の tzroutine がそれぞれ `Routine.with_locked_thread` の中で文脈を作り、`Routine.yield ()` と `Time.sleep` を挟んで 8 回 `w19_ctx_same` を呼び、解放する。`main` も `Channel` で待つ前後で同じことをする。期待は合計 `64 * 8 + 8`（手計算）。native の `-O0`・`-O3`、`TSUZURI_MAXPROCS=1` と `4`、5 回続けて通る。WASM は型と検査だけ（tzroutine の段は native）。
- `tests/windows.mjs`: 早見の 1 つ目と 3 つ目だけの版を `-l user32` を付けて MSVC で build・実行し、終了コード 1（`ReleaseDC` の 1）。2 つ目を含む版は `check` が `E1013`（`pinned_code_spawned`）。

### 不変

- 手順 1 の保存と、`local` を使わない全プロジェクトの `check --json`・IR・facts・hover が一致する。

## ドキュメント

- `docs/language.md`: 「不透明ハンドル」に `local`、「効果の表明」の語彙を 15 個に（`pinned` の意味と、属性が禁じないこと）、tzroutine の節に固定された文脈。D-53・D-55 の台帳（オーケストレーターと相談）。
- 言語リファレンス: `compiler/native-interop.md`（「不透明ハンドル」と「スレッドとホストの責任」の `with_locked_thread` の段落を `local` と `E1013` の説明に）、`built-in-types-and-modules/routine.md`（`Routine.with_locked_thread`）、`async-tasks-and-lazy/tzroutine.md`、`types-and-type-inference/constraints.md`、`values-and-functions/attributes.md`（hover の効果）、`compiler/kg-format.md`（効果の名前）、`compiler/diagnostics.md`（3 つの kind。kind の表は `<!-- generated:diagnostic-kinds -->` の生成の部分なので手で書かず、`src/diagnostic_kinds.rs` に登録してから `node scripts/gen-docs.mjs`）。
- `spec/rules/TSK.md` に 1 つ（番号は着手時の最大 + 1。案「Code that uses a value of an `extern type local` MUST NOT run in a function value that may start on another tzroutine or thread (E1013).」）と `tests/conformance/TSK/` の例。`docs/architecture.md` の A20 の節に `pinned` と `pinned_rules`。

## 受け入れ条件

- [ ] 仕様 1〜3 のとおりに動き、Rust のテストと E2E（native の 2 つの `TSUZURI_MAXPROCS`、`-O0`・`-O3`）が通る。
- [ ] `local` を使わないプログラムの IR・object・header・診断・facts・hover が 1 byte も変わらない（記録した）。
- [ ] 語彙の数（14 → 15）とビットの位置を直した単体テストが通り、`FRONTEND_FORMAT` を 1 上げた。
- [ ] 文書・`spec/rules/TSK.md` を更新し、`node scripts/check-docs.mjs <変更したページ>` が通る。
- [ ] T2 が通り、PR に `ci:full` を付けて `windows.yml`・`vscode.yml` が通った。

## 落とし穴

- **参照の辺**: 要約は関数名・クロージャを値として作るだけで辺を張る。固定する位置の辺に印を付け忘れると、早見の 1 つ目の `tzroutine`（`Routine.with_locked_thread (\() -> touch h)`）が誤って拒まれる。
- **未知の呼び出しに頼らない**: `VALUE_CALL`・`DYN` の未知は `pinned` を表さない。安全は「`pinned` な関数の値は固定・同期の位置でしか生まれない」ことで保つので、関数の値を作る形（クロージャ、値として使う関数名、部分適用、`>>`、`Dyn.of`）を 1 つでも見落とすと穴になる。`TypedExprKind` の全 variant（`grep -n "^    [A-Z][A-Za-z]*[({,]" src/check.rs` の `enum TypedExprKind` の範囲）を見て、関数の値を作る形をすべてテストに入れる。
- **drop**: 値を tzroutine へ移すと、その `Drop` は tzroutine で走る。`user_drop` の辺がそれを運ぶことを、`copy` ハンドルの record の例で確かめる。
- **ビットの位置**: `Pinned` を名前順に入れると `Rc` から後ろのビットが 1 つずれる。`EffectSet` の数値を保存・比較している所がないこと（起票時は `src/lsp.rs` の hover だけが集合として使う）を確かめる。
- **このチケットの窮屈さ**: `pinned` なラムダは `Maybe.map`・`IO.map` などの std の関数へ渡せない。文書に `match`・`for`・トップレベルの `def` で書く例を載せる。

## 対象外

- 緩和（後続チケットの候補）: (1) 利用者と std の高階関数の引数の逃げの解析（A20 Phase 6 の `calls_params` と同じ不動点で `escaping_params` を求め、呼ぶだけで保持も起動もしない引数を同期の位置にする）、(2) 呼ばれる側にしか使わない不変の局所に束縛したラムダ（`effects.rs` の `aliases`）。W25・W27 の例で必要が分かったら起票する。
- 実行時の検査（決定事項 D4 の代替案）、ハンドルを作ったスレッドとホストの API の対応の検査（ホストの契約）。
- `copy` ハンドルの規則（W18）、`extern interface` の宣言（W25）、KG の Level B の `pinned`。

## 決定事項

| ID | 決定 | 理由 | 状態 |
| --- | --- | --- | --- |
| D1 | 修飾 `extern type local Name`。読み方と衝突の扱いは W18 の D1 と同じ | 予約語を足さない（D-55）。[共通の綴り](README.md#共通の綴り起票時に決めた契約)のとおり | 要承認（承認前は該当する段に着手しない） |
| D2 | `local` のハンドルは Copy でない・捕捉できない・Send でない・Sync でない。E12 D3 の「Task へは move できる」を `local` に限って改める | 受け取ったスレッドを離れてはいけない値を、移す・共有する・関数値で運ぶ経路を型で閉じる | 要承認（承認前は該当する段に着手しない） |
| D3 | D-53 の語彙に 15 個目の効果 `pinned`（固定された OS スレッドを要る）を足す。`local` を宣言したプログラムでだけ数え、`pure` と効果の属性は禁じない。効果の属性を持つ関数の hover（`inferred effects`）には出るが、facts（KG の Level B）には出ない | 呼び出しの連なりで伝わる性質で、A20 の要約の走査・強連結成分の不動点・`Witness`（`E1013` の `help` に出す源までの連なり）・資源上限（`effects::Limits`）をそのまま使える。共通の綴りもこれを効果として決めている。語彙の外に別の解析を作る案は、同じ不動点と上限を二重に持つ | 要承認（承認前は該当する段に着手しない） |
| D4 | 静的な検査を既定にする: `pinned` な関数の値を固定・同期の位置にだけ閉じ込める（仕様 3） | 案 R1: `local` を取る `extern` の呼び出しごとに「固定されているか」を実行時に確かめてトラップする案は、通った経路しか見ず、呼び出しごとに費用を足し、`with_locked_thread` の中で作って外で（別の固定の中で）使う誤りを見逃す。案 R2: 作ったスレッドの ID をハンドルと一緒に持つ案は、ハンドルの表現と ABI（E12 D4）を変える。案 U: 起動する場所で未知の呼び出しを全部拒む案は、`local` を 1 つ宣言しただけで `ErrGroup`・関数値の `Routine.go` を使う既存のコードを拒む。案 N: 源を「`local` を名指す `extern` の呼び出し」だけにする案は、`with_locked_thread` の結果と W25 のスロット呼び出しに別の規則が要る | 既定案（実装者はこの案に従う） |
| D5 | 固定された文脈は「関数値を通らずに到達するコード」と、`with_locked_thread` の直接の引数・コールバック（と W22 の `FnPtr.of`）の本体。D-61 の境界の本体（`main` のほか `export def`・`test`・`bench`）を含む | 擬似 G は自分のスレッドで待つ（D-61 の実行モデル、`tzr_pseudo`）。共通の綴りは境界の本体として `main` だけを挙げるが、`export` も同じ擬似 G で、検査は関数の値の閉じ込めとして実装するので、境界を列挙する必要がない。`test`・`bench` が擬似 G で走る（tzroutine の中で呼ばれない）ことは、着手時に `src/test_runner.rs` が作る入口で確かめる | 既定案（実装者はこの案に従う） |
| D6 | 同期の位置は組み込みの表（wildcard なし）で決め、このチケットでは利用者と std の関数の引数を同期の位置にしない | 保持するか・別の G で呼ぶかは組み込みなら表で確かに分かる。利用者の関数は逃げの解析が要り、規模を超える（対象外の緩和） | 既定案（実装者はこの案に従う） |
| D7 | 新しい診断は `E1013` の kind `pinned_code_spawned`・`pinned_function_value` と、既存の kind（`task_requires_send`・`capture_unsendable_in_function_value`）の `local` の文の枝 | 新しいコードを割り当てない（D-51。W チケットの共通の規則）。既存の文は `local` を持たない型では変わらない | 既定案（実装者はこの案に従う） |
