# C10: 共有所有と循環構造（Arena・Handle・Rc／Arc）

| 項目 | 内容 |
| --- | --- |
| ID | C10 |
| 優先度 | P2 |
| 規模 | XL |
| 依存 | C02, (B07), (F10) |
| 後続 | A15 Phase 2 |
| 状態 | done |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | Phase 1 は不要（std 名 `Arena` は GUIDE D-30 の仮割り当てを使う）。Phase 2 は要承認: D13（参照カウントの導入。GUIDE D-30）, D14（std 名 `Rc`／`Arc` の割り当て） |
| 改善する劣位 | C#/F# 比: GC に任せられる共有データ・循環構造を所有権に沿って設計し直す必要がある（[なぜ Tsuzuri か](https://github.com/tatsuya-midorikawa/Tsuzuri/blob/c82c13e1e3dd1f02f78694aa1d26d39b3f793504/_docs/learn/why-tsuzuri.md#cf-に対する劣位点)） |
| 手本にする既存実装 | `std/Map.tz`（opaque 標準 record と Vec の消費的更新、`at` の `assert`、`iter_from` と `Seq.defer`）、`src/stdlib.rs` の `opaque_record`、`src/polymorph.rs` の `Checker::builtin`（`IO.__read_line` の std 専用制限）、`src/llvm.rs` の `emit_builtin`（`Builtin::Default`・`Builtin::Unreachable`）、`tests/map_set.rs`、`tests/fixtures/map_set/Main.tz` と `tests/features.mjs` の `map_set` suite |
| 主な影響ファイル | `std/Arena.tz`（新規）, `src/stdlib.rs`, `src/check.rs`, `src/polymorph.rs`, `src/llvm.rs`, `tests/arena.rs`（新規）, `tests/fixtures/arena/Main.tz`（新規）, `tests/features.mjs`, `docs/language.md`, `docs/architecture.md`, `_docs/library-reference/arena.md`（新規）, `_docs/library-reference/README.md`, `_docs/library-reference/api/Arena.md`（生成）, `_docs/guides/style-and-design.md`, `_docs/guides/from-fsharp.md`, `README.md`, `_docs/feature-status.md`, `_features/README.md` |

## 目的

グラフ・DAG・キャッシュのように複数の場所から同じ値を指す構造と、循環する構造を、所有権モデルのまま表す標準の手段を用意する。
Phase 1 は std の `Arena<'a>` と世代付きハンドル `Arena.Handle<'a>`（ともに新規）を追加する。Rust の `slotmap::DenseSlotMap` に相当し、言語の意味を変えずに循環グラフを構築・探索・解放できる。
Phase 2 は Rust の `Rc`／`Arc`／`Weak` に相当する参照カウントで、GUIDE D-30 により承認が必要なため設計方針だけを記す。

## 着手条件と停止条件

### 着手条件

- 依存: C02（`Vec`）が `_features/README.md` の状態欄で done であること。B07（利用者の `Drop`）と F10 は Phase 1 の着手条件ではない。B07 の完了後は、arena の要素の解放が既存の drop glue 経由で利用者の `Drop` も呼ぶ（C10 側の変更は不要）。
- 承認: Phase 1 は不要。std 名 `Arena` は GUIDE D-30 の仮割り当てを使い、完了報告で D-07 への移動を人間に依頼する。Phase 2 は D13・D14 の承認と、`Arc` については F10 の完了が条件。
- 範囲: 実装者は Phase 1 だけを行う。人間が明示的に依頼しない限り Phase 2 に着手しない。
- ベースライン（GUIDE §2.3）: 変更前に次がすべて成功することを確認する。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo test --locked --test map_set
cargo test --locked --test debug_output
cargo build --release --locked && node tests/features.mjs target/release/tsuzuri map_set
```

### 停止条件（実装をやめて報告する。GUIDE §13）

- `std/Arena.tz` の実装に `Arena.__next_id` 以外の builtin、新しい `Type` variant、`src/runtime/` の変更、`unsafe` Rust、新しい crate が必要になった。
- phantom 型引数を許可しても `record Node { value: i64, edges: Vec<Arena.Handle<Node>> }` が `E1010`／`E1017` になった。`src/recursive.rs` の `analyze` は record の field を型引数で置換して辿るため変更不要の想定で、修正が必要なら設計の前提が崩れている。
- 既存テストの期待値の変更が `src/stdlib.rs` の `reserves_the_d07_table`（20 → 21）以外に必要になった。
- 既定の WASM（threads なし）で `atomicrmw` を含む IR の clang が失敗した、または WASM の import が増えた。
- `TSUZURI_TSAN=1` の実行で `@tz.arena.next_id` へのデータ競合が報告された。
- `tests/polymorphism.rs` の `honors_the_exact_specialization_limit` が失敗した（std の Arena 関数を利用者コードから到達する前に特殊化している）。
- 仕様の API で表せない要求（要素の排他借用、複数要素の同時更新、並列の共有読み取り）が必要になった。Phase 1 の対象外として報告する。

## 現状（HEAD `f8dc655` で確認）

- 所有値は常に一意所有。docs/language.md「型とメモリ」は「GC、参照カウント、手動の解放操作は使わず、所有権と借用によりメモリを管理します」と定める。
- 再帰型は union を通る有限値だけ（A04、D-24）。`src/recursive.rs` の `analyze` は `Type::Vec` の要素にも入り、record／union は型引数で置換した field／payload を辿る。`record Node { value: i64, edges: Vec<Node> }` は `E1010`（`recursive value layout must pass through a union with a finite alternative`）。
- std に Rc／Arc／Weak・arena・handle はない。`src/stdlib.rs` の `RESERVED_MODULES` は 20 件で `Arena` を含まない。リポジトリに `Arena`／`arena` という名前のファイルはない。
- opaque 標準 record: `src/stdlib.rs` の `opaque_record` が `Map.Map`・`Map.Entry`・`Set.Set`・`Seq.Seq`・`Gpu.Device`・`Gpu.Buffer`・`IO.IO` を列挙する。`CheckedRecord::opaque`（`src/check.rs`）と `record_storage` が、定義モジュール外での構築・field 参照・pattern・update を `E1022`（`the representation of '{}' is opaque; use its module API`）で拒否する。record 宣言の検査ループは opaque 標準 record だけ公開 field 型の検査（`validate_public_type`）を免除する。
- 型引数を使わない record は拒否される。同じ検査ループが `E1024`（`type parameter '{}' is not used by any field; remove it or add a field that mentions it`）を返すため、phantom 型引数の `Handle<'a>` は今は宣言できない。
- std 専用 builtin の前例: `src/polymorph.rs` の `Checker::builtin` は、`IO.__read_line`／`IO.__write` を std の `IO` 以外から、`Debug.__print_string` を std の `Debug` 以外から呼ぶと `E1022` で拒否する（`tests/debug_output.rs` で確認）。builtin の定義は `src/llvm.rs` の `emit_builtin` が `@tz.builtin.<名前>` として出す。`Builtin::Default` は引数なしの定義文字列を直接返し、`Builtin::Unreachable` は `call void @llvm.trap()` を使う。引数なし builtin の呼び出しの前例は `Builtin::VecEmpty`（`Vec.empty()`）。
- コンパイラが生成する IR に `atomicrmw` はまだない（`src/` を grep して 0 件）。
- Vec の API（docs/language.md「Vec」）: `set`（旧要素を解放して置換）、`swap`、`pop`（`(残りの Vec, Maybe)`）、`push`、`get`（Copy 要素の `Maybe`）、`at`、`with_capacity`。容量は 4 から倍増する。
- union の payload を指す参照は返せない。`match ref slots[i] with | Occupied value -> Maybe.Some (ref value)` は `E1013`（`a borrowed value cannot outlive its pattern or iteration binding`）になる。slot を union で持つ設計では `get`／`at` を書けない。
- 試作で検証済み: 利用者モジュール `Arena.tz` で本チケットのアルゴリズムを書いた（Handle は非 generic、arena ID は引数で渡す版）。`target/release/tsuzuri run` の結果は手計算の期待値 `22220607751` と一致した。同じ試作で既存の診断も確認した。借用中の `insert` は `E1014`、`ref Arena` の task 捕捉は `E1013`（`tasks require owned values; ref Arena<string> contains a reference`）、消費後の使用は `E1012`（`use of moved or partially moved value 'arena'`）。
- `const values: Vec<i64> = Vec.empty()` は `E1026`（`this expression is not allowed in a phase-1 const; precompute it or use a runtime let`）。
- モジュールと同名の型は短縮名で書ける。利用者モジュール `Arena` の `record Arena<'a>` を `Arena<i64>` と書けた（`Map<'k, 'v>` と同じ規則）。
- 引数なし関数は `f()` で呼ぶ。`f ()` は unit を渡すため `E1006`（`cannot apply 1 arguments to a function accepting 0 arguments`）。

再現:

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p /tmp/tz-work-C10/phantom /tmp/tz-work-C10/recvec
printf "record Tag<'a> { id: i64 }\n0\n" > /tmp/tz-work-C10/phantom/Main.tz
printf 'record Node { value: i64, edges: Vec<Node> }\n0\n' > /tmp/tz-work-C10/recvec/Main.tz
target/release/tsuzuri check /tmp/tz-work-C10/phantom   # error[E1024]: type parameter 'a is not used by any field; ...
target/release/tsuzuri check /tmp/tz-work-C10/recvec    # error[E1010]: recursive value layout must pass through a union ...
```

## 仕様

### 段階

| 段階 | 内容 | 状態 |
| --- | --- | --- |
| Phase 1 | std の `Arena<'a>` と `Arena.Handle<'a>`。言語の意味は変えない | 実装対象 |
| Phase 2 | 参照カウント `Rc<'a>`／`Arc<'a>` と弱参照 | 要承認（D13・D14）。設計方針だけを記す |

### 前提とする他チケットのインターフェース

- C02（done）: `Vec` の builtin。`set` は旧要素を解放して置換し、`swap`／`pop`／`push` は所有 Vec を消費して返す。`get` は Copy 要素の `Maybe` を返す。
- C06（done）: opaque 標準 record の登録（`opaque_record`）と `record_storage` の `E1022`、`Map.at` の `assert` によるトラップ、所有値を消費して新しい値を返す API の形。
- C07（done）: `Seq.defer`・`Seq.empty()` と、`for (a, b) in seq do` のタプル pattern（試作で確認済み）。
- B07（未完了でよい）: 利用者の `Drop` は Vec の要素の drop glue から呼ばれる前提。C10 は B07 の API を使わない。
- F10（Phase 2 の `Arc` だけ）: atomic 操作と memory order の規則、`Sync`（仮称）。

### 方式の選択（D1）

- 世代付きの添字ハンドルを採用する。A09 の `ref {r} T` で arena の寿命に結び付けた参照は採用しない。
- 参照を返す方式では、参照が生きている間の `insert` が `E1014` になる。避けるには共有借用越しの確保（内部可変性。F10）が要る。
- 循環には確保後に参照を書き換える操作が要り、不変 record では書けない。region は検査専用で実行時に残らない（A09）ため、削除後の使用も検出できない。
- ハンドルは整数だけの Copy 値なので任意の値に格納できる。循環は `update` で後から辺を足して作る。削除後・別 arena の使用は世代と arena ID で実行時に検出する。

### 型

- `Arena<'a>`（`Arena` の短縮名）: opaque・非 Copy の所有値。`needs_drop` は真。`Send`・`Capture`・借用の保持は要素型 `'a` に従う（`Arena<ref string>` は `ref string` の寿命を超えられない。C06 の `Map` と同じ）。
- `Arena.Handle<'a>`: opaque・Copy・`Send`・`Capture`。field は整数だけなので、`'a` が参照型や非 Copy 型でも借用を保持しない。要素型は静的に照合し、異なる要素型の arena に渡すと `E1003`。
- `Arena.Slot`（新規、private）: arena 内部の Copy record。
- std は `Arena.Handle<'a>` に `Eq`・`Ord`・`Hash` の instance を与える（D5）。`Display`／`Debug` の instance は与えない。
- `Arena<'a>` を含む型の export は `E1008`（`Map` と同じ既存規則）。`Arena.Handle<'a>` の export の可否は既存の record 規則に従い、C10 では変更も検査もしない。ホストが作った値も実行時の検証を通るため安全性は変わらない。

### API

新 API（実装後に有効。未検証）。`'a` に制約はない。n は arena の要素数。

| API | 型 | 計算量 | 無効なハンドル |
| --- | --- | --- | --- |
| `Arena.empty()` | `Arena<'a>` | O(1)、確保なし | – |
| `Arena.with_capacity count` | `i64 -> Arena<'a>` | O(1)、3 バッファを確保 | –（負の count はトラップ） |
| `Arena.length arena` | `ref Arena<'a> -> i64` | O(1) | – |
| `Arena.contains arena handle` | `ref Arena<'a> -> Arena.Handle<'a> -> bool` | O(1) | `false` |
| `Arena.get arena handle` | `ref Arena<'a> -> Arena.Handle<'a> -> Maybe<ref 'a>` | O(1) | `None` |
| `Arena.at arena handle` | `ref Arena<'a> -> Arena.Handle<'a> -> ref 'a` | O(1) | トラップ |
| `Arena.insert arena value` | `Arena<'a> -> 'a -> (Arena<'a> * Arena.Handle<'a>)` | 償却 O(1) | – |
| `Arena.remove arena handle` | `Arena<'a> -> Arena.Handle<'a> -> (Arena<'a> * Maybe<'a>)` | O(1) | 変更しない arena と `None` |
| `Arena.update arena handle change` | `Arena<'a> -> Arena.Handle<'a> -> ('a -> 'a) -> Arena<'a>` | O(1)＋`change` | トラップ（`change` は呼ばない） |
| `Arena.iter arena` | `ref Arena<'a> -> Seq<(Arena.Handle<'a> * ref 'a)>` | 全体で O(n) | – |

- ハンドルが有効なのは、同じ arena（同じ arena ID）で、`insert` が返してから `remove` されるまでの間だけ。`update` は同じハンドルを有効なまま保つ。
- std 内部の builtin `Arena.__next_id()`（新規）は `i64` を返す。std の `Arena` モジュール以外から呼ぶと `E1022`。

### 評価順序・所有権・借用

- 引数は通常の関数呼び出しと同じく左から一度ずつ評価する。
- `insert` は arena と value を消費し、value を dense 領域の末尾へ move する。Vec の伸長時の再配置以外に複製はない。
- `remove` は arena を消費し、有効なら要素の所有権を呼び出し元へ返す（arena は解放しない）。無効なら arena を変更せずに返す（確保・move なし）。
- `update` はまず検証し、無効ならトラップする。有効なら要素を取り出して `change` を一度だけ呼び、結果を同じ位置へ戻す。ハンドルの添字・世代と他の要素の位置は変わらない。`change` は arena を参照できない（`update` が消費している）ため再入は起きない。
- `get`／`at`／`iter` の結果は `ref Arena` 引数の loan を持つ。その間 arena の消費（`insert`／`remove`／`update`／move）は `E1014`。
- drop: arena の drop は dense 領域の要素を位置の昇順（0 から n − 1）に既存の drop glue で解放し、続いて整数のバッファを解放する。ハンドルごとの処理はない。循環グラフも arena の drop で完結し、再帰呼び出しがないため深さの制限を受けない。
- 関数値の複製: arena を捕捉した関数値を複製すると、既存の record clone で独立した snapshot になり、arena ID も複製される。同じハンドルは両方の snapshot で有効（C06 の `map_capture` と同じ経路）。
- Task: `'a` が `Send` なら arena を task へ move できる。ハンドルは常に Copy で捕捉できる。`ref Arena` を task が捕捉すると `E1013`。Phase 1 には arena を複数の task で共有して読む手段はない。

### ハンドルの検証とトラップ

- 無効の定義: `handle.arena != arena.id`、添字が slot の範囲外、または slot の世代が `handle.generation` と異なる（削除済み・退役済み・別の値へ再利用済み）。
- 世代は slot ごとの i64 で 0 から始まり、`remove` のたびに 1 増える。削除時の世代が `9223372036854775807`（i64 の最大値）なら slot を退役させる（世代を −1 にし、空き slot の列へ入れない）。トラップにはしない（D8）。
- トラップ: `at`／`update` の無効なハンドル（std の `assert`）、`with_capacity` の負の容量と Vec の容量・バイト数の overflow（既存）、確保失敗（既存）、arena ID の枯渇（2^63 − 1 個を超える生成。実用上は到達しない）。
- native と WASM の意味は同じ。arena ID の採番は native と `--wasm-feature threads` では atomic 命令、既定の WASM（atomics 機能なし）では LLVM が通常の load／store へ下げる。WASM の import は増えない。

### 決定性

- 公開 API の結果（ハンドルの添字と世代、`iter` の順序、空き slot の再利用）は API 呼び出しの列だけで決まる。
- 空き slot は後入れ先出し（最後に削除した slot から再利用する）。`iter` は dense 位置の順で、`remove` は末尾の要素を削除位置へ移す（swap remove）。
- arena ID の数値は生成の全体順に依存し、並列 task では実行ごとに変わり得る。数値は観測できない。`Eq`・`Ord`・`Hash` に含めず（D5）、取得 API も表示もない。観測できるのは「同じ arena か」だけで、ID が一意なので実行順に依存しない。
- 生成 IR は決定的（カウンタの名前と定義は固定で、使われたときだけ一度出す）。

### F13 との違い

- F13 は全体の確保先（`--allocator host` など）をランタイムで差し替える。Arena は確保方式を変えず、値を Vec のバッファ（`@tz.alloc`）に置く型付きコンテナで、論理的な識別子（ハンドル）と世代の検証を与える。
- F13 の allocator は Arena のバッファにもそのまま適用される。互いに依存しない。

### 診断

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| `E1022` | std の `Arena` 外での `Arena`／`Arena.Handle`／`Arena.Slot` の構築・field 参照・pattern・update | `the representation of 'Arena.Handle' is opaque; use its module API`（引用部は該当 record 名。既存） | 構築・field・pattern・update の式 |
| `E1022` | std の `Arena` 外からの `Arena.__next_id` | `the arena id primitive is private to the standard Arena module; create arenas with Arena.empty or Arena.with_capacity`（新規） | 名前 |
| `E1024` | 利用者 record の未使用の型引数（変更なし） | `type parameter 'a is not used by any field; remove it or add a field that mentions it` | 型引数 |
| `E1003` | 要素型の異なる arena とハンドル | 既存の型不一致 | 引数 |
| `E1012` | 消費した arena の使用 | `use of moved or partially moved value 'arena'` | 使用箇所 |
| `E1014` | `get`／`at`／`iter` の借用中に arena を消費 | `access conflicts with a live borrow; use the reference or end its last use before moving, replacing, or borrowing exclusively` | 消費する引数 |
| `E1013` | `ref Arena` を task が捕捉 | `tasks require owned values; ref Arena<string> contains a reference` | task 式 |
| `E1026` | const の初期化に Arena | `this expression is not allowed in a phase-1 const; precompute it or use a runtime let` | 初期化式 |
| `E1008` | `Arena<'a>` を含む export | 既存（`Map` と同じ） | 宣言 |

新しいメッセージは `Arena.__next_id` の 1 件だけで、新しい診断コードはない。

### 資源上限

- 要素数は Vec の上限に従う（容量・バイト数の overflow はトラップ）。WASM では 16 MiB のヒープ上限を 3 本のバッファで共有する。
- 要素あたりの追加メモリは owners の 8 byte と slot の 16 byte。ハンドルは 24 byte（D12）。`Arena<'a>` の値は 88 byte（ID 8、Vec 記述子 24 × 3、空き列の先頭 8）。
- 特殊化: std の Arena 関数は利用者コードから到達した要素型ごとに特殊化され、既存の 1,024 件の上限に数える。`Arena.__next_id` は単相で 1 件。
- arena ID は 1 プロセスで 2^63 − 1 個まで。

### 例

受理される例。新 API（実装後に有効。未検証）。結果は `143`（1 + 20 + 1 + (1 + 100 + 20)）で、関数の終わりで循環グラフ全体が解放される。

```tsuzuri
record Node { value: i64, edges: Vec<Arena.Handle<Node>> }

def link :: Arena.Handle<Node> -> Node -> Node
fn link target node =
    match node with
    | Node { value = value, edges = edges } -> Node { value: value, edges: Vec.push edges target }

def cycle :: i64
fn cycle =
    let empty: Arena<Node> = Arena.empty()
    match Arena.insert empty (Node { value: 1, edges: Vec.empty() }) with
    | (with_a, a) ->
        match Arena.insert with_a (Node { value: 20, edges: Vec.push (Vec.empty()) a }) with
        | (with_b, b) ->
            let graph = Arena.update with_b a (link b)
            let first = Arena.at (ref graph) a
            let second = Arena.at (ref graph) first.edges[0]
            let back = Arena.at (ref graph) second.edges[0]
            let mut total = 0
            for (handle, node) in Arena.iter (ref graph) do
                total = total + node.value
                if handle == a then total = total + 100
            first.value + second.value + back.value + total
```

拒否される例とトラップ。新 API（実装後に有効。未検証）。`E1014`／`E1013`／`E1012` は試作で同じ形を確認済み。

| 例 | 結果 |
| --- | --- |
| `let arena: Arena<i64> = Arena.empty()` の後の `arena.values` | `E1022` |
| `Arena.Handle { arena: 0, index: 0, generation: 0 }` | `E1022` |
| `insert` が返したハンドルの `handle.index` | `E1022` |
| 利用者コードの `Arena.__next_id()` | `E1022` |
| `Arena<string>` のハンドルを `let numbers: Arena<i64>` に渡す `Arena.get (ref numbers) handle` | `E1003` |
| `let text = Arena.at (ref filled) handle` の後、`text` を使う前に `Arena.insert filled "y"` | `E1014` |
| `let view = ref arena` の後の `Task.run (task { return Arena.length view })` | `E1013` |
| `let moved = Arena.insert arena 5` の後の `Arena.length (ref arena)` | `E1012` |
| `const bad: Arena<i64> = Arena.empty()` | `E1026` |
| 利用者コードの `record Tag<'a> { id: i64 }` | `E1024`（従来どおり） |
| `remove` 後の古いハンドルで `Arena.at` | トラップ |
| 別の arena のハンドルで `Arena.update` | トラップ |

### Phase 2: 参照カウント（要承認。設計方針。Phase 1 では実装しない）

- 承認（D13・D14）前は着手しない。`Arc` は F10 の完了後。原案の `Weak<'a>` は atomic の有無が異なるため `Rc.Weak<'a>` と `Arc.Weak<'a>` に分ける。
- 型: `Rc<'a>`（非 atomic）、`Rc.Weak<'a>`、`Arc<'a>`（atomic）、`Arc.Weak<'a>`。モジュール名 `Rc`／`Arc` は割り当てが必要（D14）。
- 表現: 新しい `Type` variant（仮名 `Type::Shared(Box<Type>, bool)`。bool は atomic）。LLVM では `ptr` で、ヒープの `{ i64 strong, i64 weak, T value }` を指す。strong 全体で weak を 1 つ保持する（Rust と同じ）。
- 所有権: 非 Copy で、共有の増加は明示的な `Rc.share (ref rc)`（A15 の方針。暗黙の Copy にしない）。`Rc` は `Send` ではない。関数値の複製は `share` として扱う。`Arc` は `'a` が `Send`（F10 導入後は `Sync` も）なら `Send`。内容は共有借用だけで読み、内部可変性は F10 に委ねる。

新 API（承認・実装後に有効。未検証）。`Arc` は同名・同型で `Rc` を `Arc` に置き換える。

```text
Rc.new          : 'a -> Rc<'a>
Rc.share        : ref Rc<'a> -> Rc<'a>
Rc.get          : ref Rc<'a> -> ref 'a
Rc.strong_count : ref Rc<'a> -> i64
Rc.try_unwrap   : Rc<'a> -> Result<'a, Rc<'a>>
Rc.downgrade    : ref Rc<'a> -> Rc.Weak<'a>
Rc.upgrade      : ref Rc.Weak<'a> -> Maybe<Rc<'a>>
```

- 循環: 内部可変性のない Tsuzuri では、値ができる前にその値への `Rc` を作れないため、Phase 2 単独では `Rc` の循環を作れず漏れない。F10 の可変セルと組み合わせると循環を作れるため、`Weak` を使わない循環は解放されないことを文書化する。サイクルコレクターは作らない。
- drop: strong を 1 減らし、0 なら値を drop してから weak を 1 減らし、0 ならブロックを解放する。`Arc` は `atomicrmw sub ... release` の後、0 に達したら `fence acquire` を置いてから drop する。`Rc` の長い鎖（連結リスト）は `src/llvm_recursive.rs` と同じ反復 drop にする（D-24）。
- overflow: strong／weak の計数が i64 の最大値を超えるとトラップ。
- 変更範囲: GUIDE §6.3 の `Type` 追加の全項目（`is_copy`・`needs_drop`・`can_send`・`can_capture`・`contains_reference`・layout・`src/polymorph.rs`・`src/ownership.rs`・`FunctionEmitter::drop_value`／`clone_value`）。docs/language.md の「GC、参照カウント…は使わず」の記述を改めるため、言語の意味の変更として承認が要る。
- 使い分け: Arena は単一の所有者が持つ循環グラフ、`Rc` は複数の所有者が共有する不変の DAG（永続データ、キャッシュ）、`Arc` は task 間の不変データの共有（A15 の深い複製の置き換え）。
- テスト案: `tests/rc.rs`、`tests/fixtures/rc/Main.tz`、E2E の `live == 0`、`Arc` の TSan、既定 WASM での atomic の下げ、100 万要素の `Rc` 鎖の drop。

## 設計

### データ構造

std の内部表現（D2）。新 API（実装後に有効。未検証）。

```tsuzuri
private record Slot { position: i64, generation: i64 }
record Arena<'a> { id: i64, values: Vec<'a>, owners: Vec<i64>, slots: Vec<Slot>, free: i64 }
record Handle<'a> { arena: i64, index: i64, generation: i64 }
```

- `values`: 生存要素の dense 領域（長さ n）。
- `owners[p]`: `values[p]` を持つ slot の添字（長さ n）。
- `slots[i]`: 使用中なら `position` は `owners[position] == i` を満たす dense 位置、`generation` は現在の世代。空きなら `position` は次の空き slot（−1 は終端）、`generation` は次に再利用するときの世代。退役済みなら `generation` は −1 で、空き列に入らない。
- `free`: 最初の空き slot（−1 は空きなし）。
- 不変条件: 使用中の全 slot i で `owners[slots[i].position] == i`。`Vec.length values == Vec.length owners` は使用中の slot 数に等しい。返すハンドルの世代は 0 以上。

Rust 側の変更（すべて新規の行）:

```rust
// src/check.rs
pub enum Builtin {
    // ...
    ArenaNextId,
}
// Builtin::ALL に Self::ArenaNextId を追加する（長さ付き配列なら長さも 1 増やす）。
// Builtin::name:   Self::ArenaNextId => "Arena.__next_id",
// Builtin::scheme: Self::ArenaNextId => (Vec::new(), Concrete(Type::I64), Vec::new()),

// src/polymorph.rs の Checker::builtin（IO と Debug の制限の直後）
if builtin == Builtin::ArenaNextId
    && !(self.module == "Arena" && self.names.origin(self.module) == ModuleOrigin::Std)
{
    return Err(Diagnostic::new(
        "E1022",
        "the arena id primitive is private to the standard Arena module; create arenas with Arena.empty or Arena.with_capacity",
        span,
    ));
}

// src/check.rs の record 宣言ループ（CheckedRecord を組み立てる前）
let opaque = names.origin(module) == ModuleOrigin::Std && crate::stdlib::opaque_record(&qualified);
// 既存の公開 field 型の検査の条件を `record.visibility == Visibility::Public && !opaque` にする。
// 未使用型引数の E1024 を `if !opaque { ... }` で囲む。
```

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| std ソース | `std/Arena.tz`（新規） | `Arena`・`Handle`・`Slot` と API 関数 | 「アルゴリズム」の実装 |
| std 登録 | `src/stdlib.rs` | `SOURCES` | `("std/Arena.tz", include_str!("../std/Arena.tz"))` を先頭（`std/Array.tz` の前）に追加 |
| std 登録 | `src/stdlib.rs` | `RESERVED_MODULES`、`tests::reserves_the_d07_table` | 末尾に `"Arena"` を追加し、件数の期待値を 20 から 21 にする |
| std 登録 | `src/stdlib.rs` | `opaque_record` | `matches!` の候補に `"Arena.Arena"`・`"Arena.Handle"`・`"Arena.Slot"` を追加 |
| 宣言検査 | `src/check.rs` | record 宣言ループ（`is not used by any field` の分岐） | opaque 標準 record の判定を一度だけ計算し、公開 field 型の検査（既存）と未使用型引数の `E1024`（新規）の免除に使う |
| builtin | `src/check.rs` | `Builtin`・`Builtin::ALL`・`Builtin::name`・`Builtin::scheme` | `ArenaNextId`（新規）。名前 `"Arena.__next_id"`、引数なし、結果 `i64`、制約なし |
| builtin | `src/check.rs` ほか | `Builtin` への網羅的な `match` | `cargo build` の non-exhaustive エラーが出た箇所だけ、`Builtin::VecEmpty`（引数なし）と同じ扱いの arm を足す。`_ =>` に落ちる match は変更しない |
| 呼び出し制限 | `src/polymorph.rs` | `Checker::builtin` | std の `Arena` モジュール以外からの `ArenaNextId` を `E1022` |
| コード生成 | `src/llvm.rs` | `emit_builtin` | `Builtin::ArenaNextId` の arm。大域カウンタと定義を一つの文字列で返す（次節） |
| E2E | `tests/fixtures/arena/Main.tz`（新規）、`tests/features.mjs` | `suites.arena`（新規） | テスト計画のとおり |
| Rust テスト | `tests/arena.rs`（新規） | テスト計画の 7 関数 | テスト計画のとおり |

`src/recursive.rs`・`src/ownership.rs`・ランタイムは変更しない。

### 生成 IR とランタイム

`emit_builtin` の `Builtin::ArenaNextId` の arm は次の文字列を返す（シンボルは `builtin_symbol(instance, module)` の値。`tests/debug_output.rs` の `@tz.builtin.Debug.__print_string` と同じ規則で `@tz.builtin.Arena.__next_id` になる想定）。

```llvm
@tz.arena.next_id = internal global i64 0, align 8

define internal i64 @tz.builtin.Arena.__next_id() nounwind {
entry:
  %previous = atomicrmw add ptr @tz.arena.next_id, i64 1 monotonic, align 8
  %id = add i64 %previous, 1
  %valid = icmp sgt i64 %id, 0
  br i1 %valid, label %done, label %exhausted
exhausted:
  call void @llvm.trap()
  unreachable
done:
  ret i64 %id
}
```

- `monotonic` で足りる。一意性だけが必要で、カウンタを通じて他のメモリを公開しない。非 atomic の加算は `Task.parallel` の worker 間でデータ競合になるため使わない。
- builtin は単相なので instance は 1 個で、大域の定義も 1 回だけ出る。Arena を使わないプログラムの IR は変わらない。
- `@llvm.trap` の宣言は `Builtin::Unreachable` と同じ経路に任せる（重複宣言がないことをテストで確認する）。
- 既定の WASM（atomics 機能なし）では LLVM の WebAssembly backend が atomic 命令を通常の load／add／store へ下げる。`--wasm-feature threads` では `i64.atomic.rmw.add` になり、共有メモリ上のカウンタを worker が共有する。どちらも import は増えない。
- `Arena<'a>` と `Arena.Handle<'a>` は既存の record 表現で下げる。新しい `%tz.*` 型、ランタイムのファイル、リンクの条件は増えない。

### アルゴリズム

`std/Arena.tz` の参照実装。新 API（実装後に有効。未検証）。`Handle` の型引数と `Arena.__next_id` を除いた同形の試作は、HEAD の release コンパイラで実行して期待値と一致した。

```tsuzuri
instance Eq<Handle<'a>> {
    fn eq left right = left.index == right.index && left.generation == right.generation
    fn ne left right = !(Eq.eq left right)
}

instance Ord<Handle<'a>> {
    fn lt left right = left.index < right.index || (left.index == right.index && left.generation < right.generation)
    fn le left right = !(Ord.lt right left)
    fn gt left right = Ord.lt right left
    fn ge left right = !(Ord.lt left right)
}

instance Hash<Handle<'a>> {
    fn hash handle =
        let key = (handle.index, handle.generation)
        Hash.hash (ref key)
}

def empty :: Arena<'a>
fn empty = Arena { id: Arena.__next_id(), values: Vec.empty(), owners: Vec.empty(), slots: Vec.empty(), free: -1 }

def with_capacity :: i64 -> Arena<'a>
fn with_capacity count = Arena { id: Arena.__next_id(), values: Vec.with_capacity count, owners: Vec.with_capacity count, slots: Vec.with_capacity count, free: -1 }

def length :: ref Arena<'a> -> i64
fn length arena = Vec.length (ref arena.values)

private def position :: ref Arena<'a> -> Handle<'a> -> i64
fn position arena handle =
    if handle.arena == arena.id then
        match Vec.get (ref arena.slots) handle.index with
        | Maybe.Some slot -> if slot.generation == handle.generation then slot.position else -1
        | Maybe.None -> -1
    else -1

def contains :: ref Arena<'a> -> Handle<'a> -> bool
fn contains arena handle = position arena handle >= 0

def get :: ref Arena<'a> -> Handle<'a> -> Maybe<ref 'a>
fn get arena handle =
    let index = position arena handle
    if index >= 0 then Maybe.Some (ref arena.values[index]) else Maybe.None

def at :: ref Arena<'a> -> Handle<'a> -> ref 'a
fn at arena handle =
    let index = position arena handle
    assert (index >= 0)
    ref arena.values[index]

def insert :: Arena<'a> -> 'a -> (Arena<'a> * Handle<'a>)
fn insert arena value =
    match arena with
    | Arena { id = id, values = values, owners = owners, slots = slots, free = free } ->
        let next_position = Vec.length (ref values)
        if free < 0 then
            let index = Vec.length (ref slots)
            let result = Arena { id: id, values: Vec.push values value, owners: Vec.push owners index, slots: Vec.push slots (Slot { position: next_position, generation: 0 }), free: free }
            (result, Handle { arena: id, index: index, generation: 0 })
        else
            let slot = Maybe.get (Vec.get (ref slots) free)
            let result = Arena { id: id, values: Vec.push values value, owners: Vec.push owners free, slots: Vec.set slots free (Slot { position: next_position, generation: slot.generation }), free: slot.position }
            (result, Handle { arena: id, index: free, generation: slot.generation })

def remove :: Arena<'a> -> Handle<'a> -> (Arena<'a> * Maybe<'a>)
fn remove arena handle =
    let target = position (ref arena) handle
    if target < 0 then (arena, Maybe.None)
    else
        match arena with
        | Arena { id = id, values = values, owners = owners, slots = slots, free = free } ->
            let last = Vec.length (ref values) - 1
            let moved = owners[last]
            let retired = handle.generation == 9223372036854775807
            let moved_slot = Maybe.get (Vec.get (ref slots) moved)
            let relinked = Vec.set slots moved (Slot { position: target, generation: moved_slot.generation })
            let vacated = Vec.set relinked handle.index (Slot { position: (if retired then -1 else free), generation: (if retired then -1 else handle.generation + 1) })
            match Vec.pop (Vec.swap values target last) with
            | (remaining, Maybe.Some value) ->
                match Vec.pop (Vec.swap owners target last) with
                | (remaining_owners, _) -> (Arena { id: id, values: remaining, owners: remaining_owners, slots: vacated, free: (if retired then free else handle.index) }, Maybe.Some value)
            | (_, Maybe.None) -> unreachable ()

def update :: Arena<'a> -> Handle<'a> -> ('a -> 'a) -> Arena<'a>
fn update arena handle change =
    let target = position (ref arena) handle
    assert (target >= 0)
    match arena with
    | Arena { id = id, values = values, owners = owners, slots = slots, free = free } ->
        let last = Vec.length (ref values) - 1
        match Vec.pop (Vec.swap values target last) with
        | (remaining, Maybe.Some value) -> Arena { id: id, values: Vec.swap (Vec.push remaining (change value)) target last, owners: owners, slots: slots, free: free }
        | (_, Maybe.None) -> unreachable ()

private def handle_at :: ref Arena<'a> -> i64 -> Handle<'a>
fn handle_at arena dense =
    let index = arena.owners[dense]
    Handle { arena: arena.id, index: index, generation: arena.slots[index].generation }

private def rec iter_from :: ref Arena<'a> -> i64 -> Seq<(Handle<'a> * ref 'a)>
fn rec iter_from arena index = Seq.defer (\() ->
    if index < length arena then (iter_from arena (index + 1), Maybe.Some (handle_at arena index, ref arena.values[index]))
    else (Seq.empty(), Maybe.None))

def iter :: ref Arena<'a> -> Seq<(Handle<'a> * ref 'a)>
fn iter arena = iter_from arena 0
```

- `remove` は移動する要素の slot を先に付け替え（`relinked`）、その後で削除した slot を空きにする（`vacated`）。末尾の要素を削除するとき（`target == last`）は `moved == handle.index` なので、この順序でなければ削除した slot が使用中に戻る。
- `update` は `Vec.set` を使わない。`set` は旧要素を解放するため、`change` に渡す値がなくなる。
- 検証（`position`）は確保せず、`Vec.get` の範囲検査・比較 2 回・添字だけで終わる。

## 実装手順

各段で tree がコンパイルでき、既存テストが通る状態を保つ。コマンドはリポジトリ直下で実行する。`cargo test --locked <filter>` は 0 件でも成功するため、出力の `running N tests` の N が 0 でないことを毎回確かめる。

1. **ベースライン**
   - 変更: なし。
   - 内容: 「着手条件」のコマンドを実行し、結果を控える。
   - 確認: `cargo test --locked --test map_set`・`cargo test --locked --test debug_output` が成功し、`node tests/features.mjs target/release/tsuzuri map_set` が全ケース成功する。
2. **builtin `Arena.__next_id`**
   - 変更: `src/check.rs`（`Builtin`・`Builtin::ALL`・`Builtin::name`・`Builtin::scheme`）、`src/polymorph.rs`（`Checker::builtin`）、`src/llvm.rs`（`emit_builtin`）。
   - 内容: 「データ構造」と「生成 IR とランタイム」のとおり。non-exhaustive の `match` には `Builtin::VecEmpty` と同じ扱いの arm を足す。
   - 確認: `cargo build --locked` が警告なしで成功し、`cargo test --locked --test debug_output` が成功する。
3. **std 登録と phantom 型引数**
   - 変更: `src/stdlib.rs`（`SOURCES`・`RESERVED_MODULES`・`opaque_record`・`reserves_the_d07_table` の 21）、`src/check.rs`（record 宣言ループ）、`std/Arena.tz`（新規。3 つの record、`Eq`／`Ord`／`Hash` の instance、`empty`・`with_capacity`・`length` だけ）。
   - 内容: opaque の判定を一度だけ計算し、公開 field 型の検査と未使用型引数の `E1024` の両方を免除する。
   - 確認: `cargo test --locked --lib stdlib` が成功（N ≥ 1）し、`cargo test --locked --test map_set` と `cargo test --locked --test polymorphism honors_the_exact_specialization_limit`（N = 1）が成功する。
4. **Rust テストの第 1 群**
   - 変更: `tests/arena.rs`（新規）。
   - 内容: `arena_id_primitive_is_private`・`phantom_parameters_are_std_opaque_only`・`arena_ir_has_one_atomic_counter` と、`arena_types_are_opaque_noncopy_owned_values` のうち record の構築・field・update・export・const の部分を書く。
   - 確認: `cargo test --locked --test arena` が 4 件成功する。
5. **検証と挿入**
   - 変更: `std/Arena.tz`（`position`・`contains`・`get`・`at`・`insert`）、`tests/arena.rs`。
   - 内容: 「アルゴリズム」のとおり。`handles_are_typed_copy_and_send` と `arena_borrows_follow_ownership_rules` を追加し、pattern と `handle.index` の `E1022` を第 1 群に足す。
   - 確認: `cargo test --locked --test arena` が 6 件成功する。
6. **削除と空き slot の列**
   - 変更: `std/Arena.tz`（`remove`）、`tests/arena.rs`。
   - 内容: swap remove、後入れ先出しの空き列、世代の加算と退役。`arena_api_programs_emit_for_both_targets` を追加し、削除 → 再挿入 → 古いハンドルの `contains` が false のプログラムを native と WASM で出力する。
   - 確認: `cargo test --locked --test arena` が 7 件成功する。
7. **更新**
   - 変更: `std/Arena.tz`（`update`）、`tests/arena.rs`。
   - 内容: 「例」の `cycle` と、無効なハンドルでの `update` を含むプログラムを `arena_api_programs_emit_for_both_targets` に足す。
   - 確認: `cargo test --locked --test arena` が 7 件成功する。
8. **反復**
   - 変更: `std/Arena.tz`（`handle_at`・`iter_from`・`iter`）、`tests/arena.rs`。
   - 内容: `for (handle, value) in Arena.iter (ref arena) do` を使うプログラムを足す。
   - 確認: `cargo test --locked --test arena` が 7 件成功し、`cargo test --locked --test polymorphism honors_the_exact_specialization_limit` が成功する。
9. **E2E**
   - 変更: `tests/fixtures/arena/Main.tz`（新規）、`tests/features.mjs`（`map_set` の後に `arena` suite。GUIDE §7.4）。
   - 内容: テスト計画の export、ケース、トラップ、IR 検査。
   - 確認: `cargo build --release --locked && node tests/features.mjs target/release/tsuzuri arena` が全ケース・全トラップで成功する（native／WASM × `-O0`／`-O3`、`live == 0`、WASM の import なしはハーネスが確認する）。
10. **並列・WASM threads・決定性**
    - 変更: なし（失敗したら停止条件に従う）。
    - 内容: TSan、threads 版 WASM のビルド、IR の 2 回出力の比較。
    - 確認: 次のコマンドがすべて成功し、`cmp` が差分を出さない。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
TSUZURI_TSAN=1 node tests/features.mjs target/release/tsuzuri arena
rm -rf /tmp/tz-work-C10/fixture && mkdir -p /tmp/tz-work-C10 && cp -R tests/fixtures/arena /tmp/tz-work-C10/fixture
target/release/tsuzuri build /tmp/tz-work-C10/fixture --target wasm32 --wasm-feature threads -o /tmp/tz-work-C10/arena-threads.wasm
target/release/tsuzuri build /tmp/tz-work-C10/fixture --emit llvm -o /tmp/tz-work-C10/first.ll
target/release/tsuzuri build /tmp/tz-work-C10/fixture --emit llvm -o /tmp/tz-work-C10/second.ll
cmp /tmp/tz-work-C10/first.ll /tmp/tz-work-C10/second.ll
```

11. **ドキュメント**
    - 変更: 「ドキュメント」の全ファイル。
    - 内容: API 表、無効なハンドル、決定性、drop、Task、F13 との違い、グラフの設計指針。
    - 確認: `node scripts/check-docs.mjs _docs/library-reference/arena.md _docs/library-reference/README.md _docs/guides/style-and-design.md _docs/guides/from-fsharp.md` が成功する（`arena.md` の `run=143` の例も実行される）。
12. **全体検証**
    - 変更: なし。
    - 内容: GUIDE §10 の完了の定義を確認する。
    - 確認: `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test --locked` が成功し、`node tests/features.mjs target/release/tsuzuri` が全 suite で成功する。

## テスト計画

### Rust テスト

`tests/arena.rs`（新規）。書き方は `tests/map_set.rs` と同じく `tsuzuri::{analyze, llvm}` を使い、拒否は `analyze(source).unwrap_err().code` を比べる。

| テスト関数 | 確認すること |
| --- | --- |
| `arena_id_primitive_is_private` | `Arena.__next_id()` を利用者コードの式と `def f :: i64` の本体で呼ぶと `E1022` で、メッセージに `private to the standard Arena module` を含む |
| `phantom_parameters_are_std_opaque_only` | `record Tag<'a> { id: i64 }` は `E1024`（`is not used by any field`）。`record Node { value: i64, edges: Vec<Arena.Handle<Node>> }` と `Arena<Node>` を使うプログラムは成功。`record Bad { value: i64, edges: Vec<Bad> }` は `E1010` |
| `arena_types_are_opaque_noncopy_owned_values` | `Arena { ... }` の構築、`arena.values`、`{ arena with free = 0 }`、全 field の `Arena { ... }` pattern、`Arena.Handle { arena: 0, index: 0, generation: 0 }`、`handle.index` がすべて `E1022`。消費後の使用は `E1012`、`export def bad :: Arena<i64>` は `E1008`、`const bad: Arena<i64> = Arena.empty()` は `E1026`。2 つの arena を使うプログラムの IR が native と WASM でそれぞれ 2 回の出力で一致する |
| `handles_are_typed_copy_and_send` | ハンドルを別の束縛へコピーした後も両方で `contains` が真、task がハンドルを捕捉して比較できる。`Arena<string>` のハンドルを `Arena<i64>` に渡すと `E1003` |
| `arena_borrows_follow_ownership_rules` | 「例」の表の `E1014`・`E1013`・`E1012` の 3 ケース（コードとメッセージの先頭） |
| `arena_ir_has_one_atomic_counter` | 2 つの arena を作るプログラムの IR（native と WASM）で、`@tz.arena.next_id = internal global i64 0` と `define internal i64 @tz.builtin.Arena.__next_id()` がそれぞれ 1 回、`atomicrmw add ptr @tz.arena.next_id, i64 1 monotonic` を含み、`declare void @llvm.trap` が 2 回以上現れない。Arena を使わないプログラムの IR は `tz.arena` を含まない |
| `arena_api_programs_emit_for_both_targets` | `insert`・`get`・`at`・`contains`・`remove`（再挿入を含む）・`update`・`iter`・`with_capacity` と「例」の `cycle` を使うプログラムが解析され、native と WASM の IR を出力できる |

`src/stdlib.rs` の `reserves_the_d07_table` は 21 件を期待し、`embedded_sources_use_reserved_flat_modules` は `std/Arena.tz` も検査する。

### E2E

fixture は `tests/fixtures/arena/Main.tz`（新規）、suite は `tests/features.mjs` の `arena`（新規。fixture のディレクトリ名と同じ）。ハーネスが native／WASM × `-O0`／`-O3`、各呼び出し後の `live == 0`、WASM の import なしを確認する。fixture は次の型を使う。新 API（実装後に有効。未検証）。

```tsuzuri
record Node { value: i64, edges: Vec<Arena.Handle<Node>> }
record Link { value: i64, next: Maybe<Arena.Handle<Link>> }
```

| export | 引数 | 内容 | 期待値（独立計算） |
| --- | --- | --- | --- |
| `arena_cycle` | – | 「例」の `cycle` | `143` |
| `arena_ring` | n = 1, 2, 3, 1000 | 値 i・辺なしのノードを n 個挿入し、`update` で辺 i → (i + 1) mod n を足す。`handles[0]` から n 歩たどり、歩数 s（0 始まり）で `値 × (s + 1)` を足す。最後に `assert (cursor == handles[0])` | `(n − 1) × n × (n + 1) / 3` |
| `arena_chain` | n = 0, 1, 50000 | ノード k（値 k、k = 0 は `next` が None、それ以外は直前のハンドル）を順に挿入し、最後のノードから None までたどって値を足す | `n × (n − 1) / 2` |
| `arena_churn` | count = 0, 1, 2, 3, 100, 1000 | 値 3i を挿入し、i % 3 == 0 のハンドルを削除して削除値の和 R を取る。削除数 r だけ `1000000 + k` を再挿入し、古いハンドルで `contains` が偽の数 s と `iter` の値の和 L を取る | `L × 3 + R × 5 + s × 7 + length × 11` |
| `arena_order` | – | 0〜4 を挿入し、値 1 のハンドルを削除して 5 を挿入する。`iter` の値を 10 進で連結し、`handles[1] < fifth && fifth < handles[2]` なら末尾に 1、偽なら 0 を付ける | `42351` |
| `arena_strings` | count = 0, 1, 10, 10000 | `to_string i` を挿入し、i % 4 == 1 を削除（長さの和 R）、i % 4 == 2 を `\text -> text + "!"` で更新し、`iter` で長さの和 L を取る | `L × 1000 + R` |
| `arena_foreign` | – | 同じ要素型の 2 つの arena で、添字と世代が同じハンドル ha・hb を作る。`contains (ref b) ha` が真なら 1、`ha == hb` なら 10、`get (ref b) ha` が None なら 100 を足す | `110` |
| `arena_capture` | – | 42 を入れた arena を関数値 `\() -> deref (Arena.at (ref arena) handle)` が捕捉し、`read () + read ()` | `84` |
| `arena_task` | – | arena を task へ move し、`Task.run` で要素を読む | `42` |
| `arena_parallel` | count = 0, 1, 1000 | `Task.parallel` の 4 task（添字 i）がそれぞれ arena を作り、`i × count + k`（k < count）を挿入して和を返す。結果配列の和 | `2 × count × (4 × count − 1)` |
| `arena_borrowed_values` | – | `Arena<ref string>` に `ref text`（`"borrowed"`）を入れ、`Arena.at` の結果の `length` | `8` |

- `arena_order` の期待値の導出: 削除で値 4 が位置 1 へ移り（0, 4, 2, 3）、5 は末尾に入る（0, 4, 2, 3, 5 → 4235）。5 は空き slot 1 を世代 1 で再利用するため `handles[1] < fifth`（添字が同じで世代が大きい）と `fifth < handles[2]`（添字 1 < 2）が真で、末尾は 1。
- `arena_chain` を 50000 にするのは WASM の 16 MiB 上限のため（要素 40 byte × 容量 65536 と整数バッファ、倍増時の旧領域を含めて 9 MiB 程度）。drop に再帰がないことは構造上の性質で、件数を増やしても確認の強さは変わらない。
- トラップ: `arena_at_removed`（削除後のハンドルで `at`）、`arena_at_foreign`（別の arena のハンドルで `at`）、`arena_update_stale`（削除後のハンドルで `update`）、`arena_negative_capacity`（`Arena.with_capacity (-1)`）。

`tests/features.mjs` への追加（期待値は JavaScript の BigInt で独立に計算する）:

```javascript
  arena: {
    cases: [
      ["arena_cycle", [], 143n], ["arena_order", [], 42351n], ["arena_foreign", [], 110n],
      ["arena_capture", [], 84n], ["arena_task", [], 42n], ["arena_borrowed_values", [], 8n],
      ...[1n, 2n, 3n, 1000n].map((n) => ["arena_ring", [n], (n - 1n) * n * (n + 1n) / 3n]),
      ...[0n, 1n, 50000n].map((n) => ["arena_chain", [n], n * (n - 1n) / 2n]),
      ...[0n, 1n, 1000n].map((count) => ["arena_parallel", [count], 2n * count * (4n * count - 1n)]),
      ...[0n, 1n, 2n, 3n, 100n, 1000n].map((count) => {
        let kept = 0n, removed = 0n, stale = 0n;
        for (let i = 0n; i < count; i++) if (i % 3n === 0n) { removed += i * 3n; stale++; } else kept += i * 3n;
        let reinserted = 0n;
        for (let k = 0n; k < stale; k++) reinserted += 1000000n + k;
        return ["arena_churn", [count], (kept + reinserted) * 3n + removed * 5n + stale * 7n + count * 11n];
      }),
      ...[0n, 1n, 10n, 10000n].map((count) => {
        let live = 0n, removed = 0n;
        for (let i = 0n; i < count; i++) {
          const length = BigInt(String(i).length);
          if (i % 4n === 1n) removed += length; else live += length + (i % 4n === 2n ? 1n : 0n);
        }
        return ["arena_strings", [count], live * 1000n + removed];
      }),
    ],
    traps: [["arena_at_removed", []], ["arena_at_foreign", []], ["arena_update_stale", []], ["arena_negative_capacity", []]],
    inspect(ir) {
      const lookups = [...ir.matchAll(/^define internal [^\n]*@tz\.fn\.Arena\.(?:position|contains|get|at)[^\n]*\{([\s\S]*?)^\}/gm)];
      assert.ok(lookups.length > 0);
      for (const [, body] of lookups) assert.doesNotMatch(body, /@tz\.(?:alloc|realloc)\(/);
      assert.equal(ir.match(/^@tz\.arena\.next_id = internal global i64 0/gm)?.length, 1);
      assert.match(ir, /atomicrmw add ptr @tz\.arena\.next_id, i64 1 monotonic/);
    },
  },
```

`arena_ring` の形。新 API（実装後に有効。未検証）。`link` は「例」と同じ。

```tsuzuri
export def arena_ring :: i64 -> i64
fn arena_ring count =
    let mut graph: Arena<Node> = Arena.with_capacity count
    let mut handles: Vec<Arena.Handle<Node>> = Vec.with_capacity count
    let mut index = 0
    while index < count do
        match Arena.insert graph (Node { value: index, edges: Vec.empty() }) with
        | (next, handle) ->
            graph = next
            handles = Vec.push handles handle
        index = index + 1
    index = 0
    while index < count do
        graph = Arena.update graph handles[index] (link handles[(index + 1) % count])
        index = index + 1
    let start = handles[0]
    let mut cursor = start
    let mut total = 0
    let mut step = 0
    while step < count do
        let node = Arena.at (ref graph) cursor
        total = total + node.value * (step + 1)
        cursor = node.edges[0]
        step = step + 1
    assert (cursor == start)
    total
```

### 既存テストへの影響

- `src/stdlib.rs` の `reserves_the_d07_table`: 予約モジュールが 1 件増えるため、期待値を 20 から 21 にする。
- それ以外はなし。Arena を使わないプログラムの IR は変わらない（std の generic 関数は利用者コードから到達したときだけ特殊化され、カウンタは builtin を使ったときだけ出る）。
- `tsuzuri doc` の std 出力に `Arena.md` が増える。std の API ページの一覧を固定する既存テストがあれば、その一覧の更新だけを許す。

### 性能

- 性能の優位は主張しない。計算量（検証 O(1)、挿入は償却 O(1)、削除 O(1)、反復は連続領域の走査）だけを文書化する。
- 比較するときは `docs/benchmarks.md` の手順に従い、Rust の `slotmap::DenseSlotMap` と同じ操作列・同じ要素型で計測し、生成コードを確認してから記録する。

## ドキュメント

- `docs/language.md`: `### Map / Set` の直後に `### Arena`（新規）を置く（API 表、無効なハンドル、世代と退役、決定性、drop、Task、トラップ）。「型とメモリ」の「GC、参照カウント、手動の解放操作は使わず」の段落と、「ノード共有・循環した実行時グラフ・GC・参照カウントは導入しません」の文に、循環するグラフは std の Arena とハンドルで表すことを添える。`## 診断` の `E1022` の説明に std 専用 primitive の直接呼び出しがなければ追記する。
- `docs/architecture.md`: opaque 標準 record を説明する段落（`grep -n "opaque" docs/architecture.md` で探す）に `Arena`・`Arena.Handle`・`Arena.Slot` を追加し、builtin の大域状態として `@tz.arena.next_id`（atomic、import なし）を記す。
- `_docs/library-reference/arena.md`（新規）: `map-set.md` と同じ構成。基本例は `tsuzuri run=143` として「例」の `cycle` を載せる。`_docs/library-reference/README.md` の一覧にリンクを足す。
- `_docs/library-reference/api/Arena.md`: `_docs/library-reference/api/Map.md` と同じ手順（README の `tsuzuri doc` の節）で生成する。
- `_docs/guides/style-and-design.md`: グラフの設計指針（所有する木は union、共有・循環は Arena とハンドル、参照カウントは未導入）。`_docs/guides/from-fsharp.md`: 参照の共有・循環する F# のデータ構造の置き換え方。
- `README.md`: 未実装の一覧や循環構造の記述があれば更新する。
- `_docs/feature-status.md` の C10 行と `_features/README.md` の状態欄: Phase 1 の完了を反映し、Phase 2 が未着手（要承認）であることを書く。

## 受け入れ条件

- [x] Phase 1 の API がすべて仕様どおりに動き、`cargo test --locked --test arena` の 7 件が成功する。
- [x] 循環を含むグラフを Arena とハンドルで構築・探索・解放でき、`node tests/features.mjs target/release/tsuzuri arena` が native／WASM × `-O0`／`-O3`、`live == 0`、WASM の import なしで成功する。
- [x] 削除済み・別の arena・範囲外のハンドルを、`get`／`contains`／`remove` は `None`／偽で、`at`／`update` はトラップで検出する。
- [x] ハンドルの `Eq`／`Ord`／`Hash` が arena ID を含まない（`arena_foreign` が `110`）。
- [x] 利用者 record の未使用型引数は引き続き `E1024`、利用者コードからの `Arena.__next_id` は `E1022`。
- [x] IR にカウンタの大域定義が 1 回だけあり、`atomicrmw ... monotonic` を使う。Arena を使わないプログラムの IR は変わらない。
- [x] `TSUZURI_TSAN=1` で競合の報告がなく、`--wasm-feature threads` のビルドが成功し、IR が 2 回の出力で一致する。
- [x] `honors_the_exact_specialization_limit` が成功する。
- [x] ~~Phase 2 は未着手で、D13・D14 が要承認のまま残っている。~~ 2026-10-07 に利用者が D13・D14 を承認し、全 Phase の実装を依頼した（「実装と検証」を参照）。
- [x] ドキュメントを更新し、`node scripts/check-docs.mjs` が成功する。
- [x] GUIDE §10 の完了の定義を満たす（`_features/README.md`・GUIDE の台帳・`_completed/` への移動は coordinator が行う）。

## 落とし穴

- union の payload への参照は返せない（`E1013` `a borrowed value cannot outlive its pattern or iteration binding`）。slot を `Vec<Maybe<'a>>` や union で持つと `get`／`at` が書けないため、値は dense な `Vec<'a>` に置く。
- `remove` で slot の付け替えと空き化の順序を逆にすると、末尾の要素を削除したときに削除済みの slot が使用中に戻り、古いハンドルが有効と判定される。`arena_churn` の count = 1 と `arena_order` が検出する。
- `update` に `Vec.set` を使うと旧要素が解放され、`change` に渡す値がなくなる（または二重解放を誘う）。swap・pop・push・swap の順を守る。
- カウンタを非 atomic に加算すると `Task.parallel` の worker 間でデータ競合になる。`arena_parallel` を TSan で実行して確認する。
- `Handle` に `deriving (Eq, Ord, Hash)` を付けると arena ID が比較と Hash に入り、並列 task で結果が実行ごとに変わり得る。instance は手書きし、`arena_foreign` で確認する。`Display`／`Debug` の instance も作らない。
- phantom 型引数の免除を全 record に広げると、利用者の誤った宣言を見逃す。免除は opaque 標準 record だけにし、`phantom_parameters_are_std_opaque_only` で確認する。
- std の Arena 関数を先行して特殊化すると、利用者の 1,024 件の特殊化予算を消費する（`honors_the_exact_specialization_limit`）。`Map` と同じく利用者コードからの到達に任せる。
- 引数なしの呼び出しは `Arena.empty()`。`Arena.empty ()` は unit を渡して `E1006` になる。fixture とドキュメントの例で間違えやすい。
- `get`／`at` の結果を保持したまま `insert`／`update` を呼ぶと `E1014`。必要な整数やハンドル（Copy）を先に取り出し、借用を終えてから消費する。
- 関数値の複製で arena の snapshot を作ると ID も複製されるため、同じハンドルが両方で有効になる。別の arena の検出は「同じ系統か」の判定で、値ごとの一意性ではない。文書に書く。
- WASM の 16 MiB 上限は 3 本のバッファと倍増時の旧領域の合計に効く。E2E の件数を増やすと WASM だけ確保失敗のトラップになる。
- ハンドルは値の寿命を延ばさない。arena を drop した後のハンドルは、どの arena でも無効になる（別の arena では ID が違う）。

## 対象外

- Phase 2（D13・D14 の承認まで）。
- region 付き参照（A09／A12）、チャンク化した保存、要素の排他借用（`ref mut`）、複数要素の同時借用・同時更新。
- 複数 task での共有読み取り、`Arena.clone`・`clear`・`retain`・`drain`・`is_empty`（`length` で足りる）・`fold`（`iter` と `for` で足りる）。
- ハンドルの圧縮（16 byte 以下）と、世代の退役の実行時検証（2^63 回の再利用が必要なため、コードの確認だけにする）。
- 確保方式の変更（F13）、GC、サイクルコレクター、内部可変性（F10）。

## 決定事項

### D1: Phase 1 の方式

- 決定: 世代付きの添字ハンドル（`Arena.Handle<'a>`）を返す。arena の寿命に結び付けた region 付き参照は採用しない。
- 理由: 参照方式は内部可変性（F10）なしでは挿入と共存できず、循環も作れず、削除後の使用も検出できない。ハンドルなら `unsafe`・GC・新しいランタイムなしで std のコードだけで書ける。
- 状態: 既定案（実装者はこの案に従う）

### D2: 保存形式

- 決定: dense な `values: Vec<'a>`、`owners: Vec<i64>`、`slots: Vec<Slot>`、空き列の先頭 `free` で表す（`DenseSlotMap` 方式）。チャンク化しない。
- 理由: union の slot では payload への参照を返せない（`E1013`）。借用中は arena を消費できないため、伸長時の再配置が生きた参照を壊すことはなく、チャンクは不要。連続領域は反復が速い。
- 状態: 既定案（実装者はこの案に従う）

### D3: ハンドルの型付けと arena の識別

- 決定: 要素型は `Arena.Handle<'a>` の型引数で静的に照合し、arena の識別は実行時の arena ID で行う。コンパイル時の arena 識別（branded type）は採用しない（原案どおり）。
- 理由: branded type は A09 の region を型の同一性へ持ち込み、特殊化の数と推論を複雑にする。実行時の照合は比較 1 回で済む。
- 状態: 既定案（実装者はこの案に従う）

### D4: arena ID の採番

- 決定: std 専用 builtin `Arena.__next_id`（新規）が大域カウンタ `@tz.arena.next_id` を `atomicrmw add ... monotonic` で増やし、1 から単調に採番する。枯渇（i64 の最大値を超える）はトラップ。std の `Arena` 以外からの呼び出しは `E1022`。
- 理由: std のコードだけでは一意な値を作れない。原案の「小さな builtin」に当たり、`IO.__read_line` と同じ std 専用の制限を再利用する。atomic は `Task.parallel` での競合を防ぎ、WASM の import は増えない。
- 状態: 既定案（実装者はこの案に従う）

### D5: ハンドルの比較と Hash

- 決定: `Eq`・`Ord`・`Hash` は添字と世代だけを使う（`Ord` は添字、次に世代の辞書順）。arena ID は含めない。
- 理由: arena ID の数値は並列 task の実行順に依存する。比較や Hash に入れると、結果が実行ごとに変わり得る。
- 状態: 既定案（実装者はこの案に従う）

### D6: phantom 型引数

- 決定: 未使用型引数の `E1024` を、opaque 標準 record（`opaque_record` に登録された std の record）に限って免除する。利用者の record は従来どおり `E1024`。
- 理由: `Arena.Handle<'a>` は要素型を型だけで運ぶ。免除を std の opaque record に限れば、利用者に見える言語規則は変わらない。`src/recursive.rs` は field を置換して辿るため再帰判定にも影響しない。
- 状態: 既定案（実装者はこの案に従う）

### D7: 無効なハンドルの扱い

- 決定: `get` は `None`、`contains` は偽、`remove` は変更しない arena と `None`。`at` と `update` はトラップ。
- 理由: `Map.get`／`Map.at` と `Vec.get`／`Vec.at` の使い分けに揃える。`update` は値を返せないため、黙って無視せずトラップにする。
- 状態: 既定案（実装者はこの案に従う）

### D8: 世代の上限

- 決定: 世代は i64。削除時の世代が i64 の最大値なら slot を退役させ（世代 −1、空き列に入れない）、トラップにしない（原案どおり）。
- 理由: 到達には 1 つの slot の 2^63 回の再利用が必要で、実用上は起きない。起きても容量の消費で吸収でき、古いハンドルが有効に戻ることはない。
- 状態: 既定案（実装者はこの案に従う）

### D9: drop と要素の解放

- 決定: arena の drop は生存要素を dense 位置の昇順に既存の drop glue で解放する。`remove` は要素を呼び出し元へ返し、`update` は要素を `change` へ渡すため、どちらも arena は要素を解放しない。
- 理由: Vec の既存の drop をそのまま使え、循環グラフも再帰なしで一度に解放できる。B07 の利用者 `Drop` も同じ経路で呼ばれる。
- 状態: 既定案（実装者はこの案に従う）

### D10: API の範囲

- 決定: 原案の `empty`・`insert`・`get`・`remove`・`length`・`iter` に、`with_capacity`・`contains`・`at`・`update` を加える。`iter` の要素は `(Arena.Handle<'a> * ref 'a)`。
- 理由: 循環を作るには既存要素の更新（`update`）が要る。トラップで検出する読み取り（`at`）と確保の予約（`with_capacity`）は `Map`／`Vec` の API に揃える。グラフの走査にはハンドル付きの反復が要る。
- 状態: 既定案（実装者はこの案に従う）

### D11: Task の境界

- 決定: `Arena<'a>` の `Send`／`Capture` は要素型に従い、ハンドルは常に Copy・`Send`。複数の task での共有読み取りは Phase 1 では提供しない。
- 理由: 既存の record の性質の計算をそのまま使え、新しい並行性の規則が要らない。共有読み取りは Phase 2 の `Arc` か F10 で扱う。
- 状態: 既定案（実装者はこの案に従う）

### D12: ハンドルの大きさ

- 決定: 3 つの i64（arena ID・添字・世代）の 24 byte とする。
- 理由: 変換なしで Vec の添字に使え、実装が単純。i32 の添字・世代で 16 byte にする圧縮は、計測で必要性を示してから別チケットで扱う。
- 状態: 既定案（実装者はこの案に従う）

### D13: Phase 2 の参照カウントの導入

- 決定: 「Phase 2」の設計方針（`Rc.share` による明示的な共有、`Arc` は F10 の後、循環は `Weak` で断つ、反復 drop）を案とし、Phase 1 の利用実績を見てから人間が判断する。
- 理由: GUIDE D-30 が承認対象とし、docs/language.md の「参照カウントは使わず」という言語の方針と `Type` を変える。
- 状態: 承認済み（2026-10-07。全 Phase の実装の依頼）。実装した設計は「実装と検証」の Phase 2 にある

### D14: Phase 2 の std 名

- 決定: モジュール `Rc`／`Arc`（型 `Rc<'a>`・`Rc.Weak<'a>`・`Arc<'a>`・`Arc.Weak<'a>`）を提案する。
- 理由: GUIDE D-30 の仮割り当ては `Arena` だけで、`Rc`／`Arc` は未割り当て。予約モジュール名の追加は D-07 の変更に当たる。
- 状態: 承認済み（2026-10-07）。予約モジュール `Rc`・`Arc` を足し、型名 `Rc`・`Arc` も `Vec` と同じく予約した（「実装と検証」）

## 実装と検証（2026-10-07）

利用者の依頼（全 Phase の実装、判断が要る点は最善の選択で実装）を D13・D14 の承認として扱った。着手時の HEAD は `2ee813f`（チケットの詳細化時の `f8dc655` から、`_docs/` が `_tsuzuri/language-reference/`（以下 LR）へ移り、予約モジュールが 36 件になっている）。
作業機: Apple M1 Max、macOS、Apple clang 21、rustc 1.98.1、Node v20.19.6。性能の改善は主張しない。

### Phase 1: Arena とハンドル

#### 実装

- 組み込み関数 `Builtin::ArenaNextId`（`Arena.__next_id :: i64`、`Builtin::ALL` の `Ignore` の後）。`Checker::builtin` が std の `Arena` モジュール以外からの使用を `E1022`（`the arena id primitive is private to the standard Arena module; create arenas with Arena.empty or Arena.with_capacity`）で拒否する。
  `emit_builtin` はチケットの「生成 IR」のとおりの定義と `@tz.arena.next_id = internal global i64 0, align 8` を一つの文字列で返す（単相なので一度だけ出る。`@llvm.trap` は既存の宣言を使う）。
- `src/stdlib.rs`: `SOURCES` の先頭に `std/Arena.tz`、`RESERVED_MODULES` の末尾に `Arena`（`reserves_the_d07_table` は 36 → 37。Phase 2 で 39）、`opaque_record` に `Arena.Arena`・`Arena.Handle`・`Arena.Slot`。
- `src/check.rs` の record 宣言ループ: 不透明な標準 record の判定 `opaque` を宣言ごとに一度だけ計算し、公開フィールド型の検査と未使用型引数の `E1024` の両方を免除する。利用者の record は従来どおり `E1024`。
- `std/Arena.tz`: チケットの「アルゴリズム」のとおり（`Slot`・`Arena`・`Handle`、手書きの `Eq`／`Ord`／`Hash`、`empty`・`with_capacity`・`length`・`contains`・`get`・`at`・`insert`・`remove`・`update`・`iter`）。公開する宣言には文書コメントを付けた（`tsuzuri doc` と LSP の hover に出る）。
- テスト: `tests/arena.rs`（7 件。チケットの表の内容に加え、ハンドルの pattern 分解の `E1022`、`let next = Arena.__next_id` の `E1022`、予約モジュール `Arena` の `E1011`、`Map`／`HashMap` のキーとしてのハンドル）。
  `tests/fixtures/arena/Main.tz` と `tests/features.mjs` の suite `arena`（`map_set` の後。26 ケースとトラップ 4 つ、`inspect` は検索関数が確保しないこと・カウンターの定義と `define` が 1 回ずつ・`atomicrmw ... monotonic`）。
- 文書: LR に `built-in-types-and-modules/arena.md`（新規。`map.md` と同じ構成、基本例は `run=143` の `cycle`）と `index.md` の項目。共有や循環を「できない」「計画中」と書いていた
  `ownership-and-memory/drop.md`・`ownership.md`、`languages/why-tsuzuri.md`・`how-about-tsuzuri.md`・`strategy.md`、`built-in-types-and-modules/union.md`・`record.md`、`compiler/diagnostics.md`（`E1022`）を直した。
  `docs/language.md`（`### Arena`、「型とメモリ」と再帰型の節の追記、診断表の `E1022`）、`docs/architecture.md`（Arena の段落）、`README.md`（コレクションの一覧と予約名）。

#### チケットから外れた判断

1. 文書の置き場所は GUIDE §8.1 で読み替えた（`_docs/library-reference/arena.md` → LR の `arena.md`）。生成 API の snapshot（`api/Arena.md`）と `_docs/feature-status.md` は置き換え先がないので作らない。`_features/README.md` と GUIDE は coordinator が更新する。
2. `docs/language.md` の `### Arena` は `### Map / Set` の直後ではなく、その後に続く `### HashMap / HashSet` の後に置いた（チケットの詳細化の後に HashMap の節ができ、Map／Set と HashMap／HashSet の間を割らないため）。
3. 「Arena を使わないプログラムの IR は変わらない」は、std の関数の追加による生成 id の一様なずれを除いて満たす。instance の関数の id は全関数の数の後から採番するため（`polymorph.rs` の `function_id = functions.len()`）、std に関数を足すと `$instance.N`・`$intrinsic.<class>.<method>.N`・`$builtin.to_string.N` の番号がずれる（C08・A15 と同じ）。
   `2ee813f` の release コンパイラと比べ、`tests/fixtures/*` の 57 個（`arena` を除く）のうち 32 個は byte 一致、25 個はこの番号を正規化すると一致した（差分は番号だけ）。`tz.arena` と `atomicrmw` は Arena を使わない IR に現れない（`arena_ir_has_one_atomic_counter`）。
4. ハンドルの `Display`／`Debug` は作らない（チケットどおり）。

#### 確認

- `cargo test --locked --test arena`（7 passed）、`--lib stdlib`（5 passed）、`--test map_set`（2）、`--test debug_output`（3）、`--test polymorphism honors_the_exact_specialization_limit`（1）。
- `node tests/features.mjs target/release/tsuzuri arena`: 26 ケースとトラップ 4 つが native／WASM × `-O0`／`-O3` で成功（`live == 0`、WASM の import なし、IR の 2 回の出力が一致、宣言の重複なし）。`TSUZURI_TSAN=1` でも成功（`arena_parallel` の 4 task が並列に `Arena.__next_id` を呼ぶ）。`map_set` も成功。
- `tsuzuri build tests/fixtures/arena --target wasm32 --wasm-feature threads` が成功し、`llvm-objdump` で `i64.atomic.rmw.add` を確認した。既定の wasm32 では atomic 命令が通常の加算に下がる（import なし）。native と wasm32 の IR はそれぞれ 2 回の出力で一致した。
- `node scripts/check-docs.mjs`（変更した LR の 10 ページ）が成功。`tsuzuri doc std` は `Arena.md` を出し、`tsuzuri fmt` で整形した fixture も `check` を通る。
- `cargo fmt --all -- --check`、`cargo clippy --locked --all-targets -- -D warnings`、`RUST_MIN_STACK=4194304 cargo test --locked`（72 個のテストバイナリで 743 passed、失敗なし）、GUIDE §3.1 の回帰テスト 4 件（既定の stack で個別に実行）が成功。

### Phase 2: 参照カウント（Rc／Arc／Weak）

#### 具体的な設計

- 型: `Type::Shared(Box<Type>, SharedKind)`。`SharedKind` は `Rc`・`RcWeak`・`Arc`・`ArcWeak`（`atomic()`・`weak()`・`strong()`・`weakened()`）。ソースの型名は `Rc<T>`・`Rc.Weak<T>`・`Arc<T>`・`Arc.Weak<T>`（`std::` 付きも可）で、`resolve_type` が `Vec` と同じく宣言の解決より先に読む（引数が 1 つでなければ `E1004`）。std のソースはなく、予約モジュール `Rc`・`Arc` を足した（`reserves_the_d07_table` は 39）。
- API（組み込み関数 18 個。`Arc` 版は同名・同型）: `new :: 'a -> Rc<'a>`、`share :: ref Rc<'a> -> Rc<'a>`、`get :: ref Rc<'a> -> ref 'a`、`strong_count`／`weak_count :: ref Rc<'a> -> i64`（`weak_count` は `Weak` の数で、強い所有者全体の 1 を含まない）、`ptr_eq :: ref Rc<'a> -> ref Rc<'a> -> bool`、`try_unwrap :: Rc<'a> -> Result<'a, Rc<'a>>`、`downgrade :: ref Rc<'a> -> Rc.Weak<'a>`、`upgrade :: ref Rc.Weak<'a> -> Maybe<Rc<'a>>`。`weak_count` と `ptr_eq` は追加した。
- 表現: 値は `ptr`。ブロックは `{ i64 strong, i64 weak, T }` で、強い所有者全体で弱い数を 1 持つ（Rust と同じ）。`T` が再帰型を格納する（`Type::reaches_recursive`）ときだけ、`%tz.rec.header` の `next`・`action` を前に置いた `{ ptr, ptr, i64, i64, T }` にする（遅延ブロック）。
- drop: ヌル（ムーブ済み）なら何もしない。強い数を減らして 0 なら、値を drop してから弱い数を減らし、0 ならブロックを解放する。遅延ブロックは値を drop せず、action `@"tz.shared.drop.{rc,arc}.<T>"` を書いて `@tz.rec.enqueue`（`drop_pending` があるとき）か `@tz.rec.drop` に積む。action は `drop_pending` 付きで値を drop して弱い数を減らす。再帰型のノードと同じ待ちリストなので、`Rc`／`Arc` を通る鎖も再帰しない（D-24 と同じ方式）。action の型は `Globals::shared_types` に集め、`llvm_recursive::emit_helpers` が再帰型の helper と交互に不動点まで定義する。
- `Arc` の順序: 増加は `atomicrmw add monotonic`、減少は `atomicrmw sub release`、0 にしたタスクは値とブロックを壊す前に `fence acquire`。`try_unwrap` は `cmpxchg 1 → 0 monotonic monotonic` の後に `fence acquire`、`upgrade` は強い数が 0 でない間 `cmpxchg n → n + 1 acquire monotonic` を繰り返す。読み出しは `load atomic monotonic`。`Rc` は通常のロード・ストアだけ。
- overflow: 強い数・弱い数の増加が `i64::MAX` を超えるとトラップ（`TrapKind::NumericRuntime`。`Rc` は書き込む前に検査するので計数は壊れない）。
- 性質: 非 Copy、`needs_drop`。`contains_reference`・`carries_loans`・所有権の `owned` は中の値に従う。`Rc`／`Rc.Weak` は `Send` でなく、`Arc<'a>` は `'a` が `Send` で、かつ `shareable`（下の「レビュー対応」4）なら `Send`。
- 捕捉（チケットの「関数値の複製は share」から外れた判断）: 関数値の型は捕捉した値を表さず、どの関数値も `Send` として task へ渡せる（借用だけを ownership が追跡する）。`Rc` を関数値に入れると非 atomic な計数が task をまたぐので、`Rc`／`Rc.Weak` とそれを持つ値は `Capture` を満たさない（`E1005`）。`Arc<'a>` は `'a` が `shareable`（`Rc`・`Rc.Weak`、extern ハンドル、Copy でない dyn、`Owned.Function` を格納しない。レビュー対応で `thread_confined` から改めた）なら捕捉でき、関数値の複製（`clone_value`）は `share` と同じく強い数を 1 増やす（弱参照は弱い数）。
- 中身: 排他参照を含む中身は `E1005`（`Validation::check`）。共有参照を含む中身（`Rc<ref string>`）は参照先の寿命に縛られる。
- 再帰型（チケットからの拡張）: `record Node { value: i64, children: Vec<Rc<Node>> }` のような DAG のノードを書けるように、`src/recursive.rs` は共有ポインタを格納のグラフに含め（`visit`・辺・`stored_all`・`reaches`）、共有ポインタを通る循環には union を求めない（`Graph::shared`）。値が有限かの判定では `Shared(T)` は `T` と同じなので、`record Loop { next: Rc<Loop> }` は `E1010`。共有ポインタを通る循環の中の union も再帰型のノードになるため、`union List = Nil | Cons of (i64 * Rc<List>)` は要素ごとに 2 回確保する（ノードと `Rc` のブロック）。再帰型の性質の計算は既存の `stored_all` を通るので無限再帰しない。
- 名前: 型名 `Rc`・`Arc` は `Vec` と同じく利用者のレコード・union・型エイリアス・extern type・型クラス・case に使えない（`E1001`）。`new` は予約語のまま、parser の `dot_member` がドットの後ろでだけメンバー名にする（`Rc.new`。`union` の前例と同じ。GUIDE §12 の表の `Atomic.new` は `E0002` ではなく `E1002 unknown value 'Atomic'` になる）。
- 診断: 新しいコードはない。task へ渡す `Rc` は `E1013 tasks require Send values; Rc<i64> holds an Rc or Rc.Weak, whose counts are not atomic; share values across tasks with Arc`、捕捉は `E1005 cannot capture Rc<i64> in a function value; function values may move to other tasks, and Rc counts its owners without atomic operations; capture an Arc, or pass the Rc as an argument`、中身の排他参照は `E1005 Rc cannot hold a mutable reference; ...`。`export`／`extern` は `E1008`、const は `E1026`。
- `Eq`・`Ord`・`Hash`・`Display` の instance は作らない（`Rc.get` で値を比べる）。高カインドの構築子にも使えない。
- 循環: 値ができる前にその値の `Rc` は作れず、共有した値は変更できないので、Phase 2 だけでは `Rc`／`Arc` の循環を作れない（解放漏れの経路はない）。サイクルコレクターは作らない。**F10 が内部可変性を導入するときは、`Arc<'a>` の `Send` に `Sync`（共有参照を複数の task へ渡せること）を要求し、`Weak` を使わない循環が解放されないことを文書化する。**

#### 実装したファイル

- `src/check.rs`（`Type::Shared`・`SharedKind`・性質・`shareable`（当初は `thread_confined`）・`holds_rc`・`holds_unshareable_arc`・`reaches_recursive`、`resolve_type` と `builtin_type_head`、予約型名、`Builtin` の 18 個と `SharedOperation`、`BuiltinType::Shared`、`Validation`／`Layouts`）、`src/polymorph.rs`（`map_type`・`resolve`・`unify`・`type_expression`・`bounded_type`・`drop_components`・`builtin_type`、拒否のメッセージ）、`src/recursive.rs`、`src/ownership.rs`（`owned`）、`src/closures.rs`、`src/constants.rs`・`src/warnings.rs`・`src/call_specialization.rs`（型の走査）、`src/parser.rs`（`dot_member`）、`src/stdlib.rs`。
- `src/llvm_shared.rs`（新規）、`src/llvm.rs`（`llvm_type`・`canonical_type`・`storage_layout`・`drop_value`・`clone_value`・組み込みの振り分け、`Globals::shared_types`、`emit_typed_builtin` が本体の登録を共有の `Globals` へ移す。これまで組み込み関数の本体だけが構築・解放する再帰型は helper が出ない可能性があった）、`src/llvm_recursive.rs`（helper の不動点）、`src/llvm_debug.rs`。
- テスト: `tests/rc.rs`（8 件）、`tests/fixtures/rc/Main.tz` と suite `rc`（35 ケース、native だけの 100 万要素 3 件、トラップ 1 つ、WASM だけのトラップ 2 つ）。suite の後に `--wasm-feature threads` の WASM を `createThreadPool`（3 worker）で `-O0`／`-O3` 実行し、heap が worker の stack だけに戻ることを確かめる（`rcWasmThreadsChecks`）。
- 文書: LR の `built-in-types-and-modules/rc.md`（新規）と `index.md`、`ownership-and-memory/ownership.md`・`drop.md`・`stack-and-heap.md`、`async-tasks-and-lazy/task.md`、`values-and-functions/lambda-expressions.md`・`keywords.md`、`types-and-type-inference/types.md`、`built-in-types-and-modules/list.md`・`union.md`・`record.md`、`languages/why-tsuzuri.md`・`how-about-tsuzuri.md`・`strategy.md`、`compiler/diagnostics.md`。`docs/language.md`（「型とメモリ」の規則を「GC と手動の解放は使わず、参照カウントは std の `Rc`／`Arc` を明示的に使った値だけ。サイクルコレクターはない」に改め、型の表、`### Rc / Arc`、閉包・task・再帰型・予約名・`new`・診断）、`docs/architecture.md`、`README.md`。

#### 確認

- `cargo test --locked --test rc`（8 passed）、`--test arena`（7 passed）。
- `node tests/features.mjs target/release/tsuzuri rc`: 35 ケースとトラップが native／WASM × `-O0`／`-O3` で成功（`live == 0`、WASM の import なし、IR の 2 回の出力が一致）。100 万要素の `Rc`／`Arc` の鎖（`rc_chain`・`rc_long_drop`・`arc_chain`）は native の `-O0`／`-O3` で解放でき、WASM の既定の 16 MiB では 100 万要素が確保の失敗でトラップする（`wasmTraps`）。WASM で収まる最大はおよそ `rc_chain` 162,946 要素、`rc_long_drop` 139,532 要素、`arc_chain` 162,946 要素（二分探索で測った）で、両 target のケースは 100,000 要素にした。
- `TSUZURI_TSAN=1 node tests/features.mjs target/release/tsuzuri rc` が成功（`arc_parallel`・`arc_weak_parallel`、最後の所有者が worker で解放する `arc_parallel_last`・`arc_chain_parallel`）。検出力の確認として、`Arc` の減少を非 atomic にした build では TSan が data race を報告し、`fence acquire` だけを外した build では報告しなかった（このハーネスでは acquire の欠落を検出できない。順序は C++ のメモリモデルで決めた）。
- `--wasm-feature threads` の build が成功し、`i64.atomic.rmw.add`・`i64.atomic.rmw.sub`・`i64.atomic.rmw.cmpxchg`・`atomic.fence` を確認した。既定の wasm32 は atomic 命令を含まない。threads の WASM を 3 worker で実行した結果も一致した。
- `-g`（`TSUZURI_LLVM_LINK` を設定）、`--trap-info`、`--trap-mode return`、`--allocator counting` の build が成功。`tsuzuri fmt` の往復と `tsuzuri doc`、LSP の意味索引（hover の型 `Rc<string>`）も確かめた。
- `2ee813f` の release コンパイラとの IR の比較（`tests/fixtures/*` の 57 個）は Phase 1 と同じく 32 個が byte 一致、25 個が生成 id のずれだけ。Phase 2 は Rc／Arc を使わない IR を変えない。
- 全体: `cargo fmt --all -- --check`、`cargo clippy --locked --all-targets -- -D warnings`（rustc／clippy 1.98.1）、`RUST_MIN_STACK=4194304 cargo test --locked`（73 個のテストバイナリで 751 passed、失敗なし）、GUIDE §3.1 の回帰テスト 4 件（既定の stack で個別に実行）、`cargo build --release --locked`、`sh scripts/check-runtime-includes.sh`（29 files）が成功。
  Windows の型検査 `cargo clippy --locked --all-targets --target x86_64-pc-windows-msvc -- -D warnings`（と `aarch64-pc-windows-msvc`）は、rustup の toolchain が 1.96.1 で、既存の `src/lsp.rs`・`src/parser.rs` の `clippy::nonminimal_bool` 3 件だけを報告した（`2ee813f` でも同じ 3 件）。この lint を除くと両 target とも警告なし。
- E2E（release）: features の `arena`・`rc`（`TSUZURI_TSAN=1` も）・`recursive_types`・`map_set`・`hash_map`・`vec`・`parallel`・`dyn_dispatch`・`iteration_protocol`・`unions`・`generic_records`・`typeclasses`・`consuming_update`・`borrowed_records`、`tests/tasks.mjs`・`tests/primitives.mjs`・`tests/wasm_threads.mjs`・`tests/user_drop.mjs` が成功。`node scripts/check-docs.mjs`（変更した LR の 17 ページ）が成功。

#### 既知の制限

- `Rc`／`Arc` を通って自分自身を含む union は再帰型のノードで表すので、`Cons` 1 つにつき確保が 2 回になる（Rust は 1 回）。一つにまとめる表現は別の改善として残す。
- 関数値・dyn・task の環境を何重にも通る鎖の解放は、D-24 と同じく反復化の対象外（`Rc` は関数値に入らないので、`Rc` の鎖では起きない）。
- `Rc`／`Arc` の `Eq`・`Ord`・`Hash`・`Display`、高カインドの構築子、内部可変性（F10）はない。
- TSan のハーネスは計数の非 atomic 化を検出するが、`fence acquire` の欠落は検出しない（上の確認）。
- GUIDE §12 の誤りの表の `Atomic.new`（`E0002`）は、`.new` をメンバー名として読むようになったので `E1002 unknown value 'Atomic'` になる（GUIDE は coordinator が更新する）。

### レビュー対応（2026-10-08）

統合ブランチへの merge 後のコードレビューで見つかった 4 件を `wt/c10` で直した。どれも回帰テストを足し、直す前の build でそのテストが失敗することを確かめた。

1. **共有ポインタの中だけに現れる名前付き型の定義漏れ（HIGH）。** `src/llvm.rs` の `named_types::visit` が `Type::Shared` を辿らず、`Vec<Rc<Maybe<string>>>`、`Maybe<Rc<Pair<i64>>>`（`Pair<'a>` は利用者の generic record）、`Maybe<Arc<(i64 * Maybe<Arc<i64>>)>>` のように共有ブロックの中にしか現れない record／union の具体化に `%"tz.union…"`／`%"tz.record…"` の定義が出ず、clang が `base element of getelementptr must be sized` で拒否していた。`Vec`・`Task`・参照と同じ再帰の腕に `Type::Shared` を足した。§6.9 の他の型の走査も見直し、`literal_match` の `closed`（`src/check.rs`）も `Vec` と同じく共有ポインタの中を辿るようにした（`Rc<{integer}>` の値は参照ではないので、引数の借用の結果は変わらない）。`llvm_abi` の handle の typedef、`polymorph` の捕捉、region のラベルは共有ポインタを含みえない（`E1008`、捕捉は値ごと）ので変えていない。
   テスト: `tests/rc.rs` の `named_types_reached_only_through_shared_pointers_are_defined`（IR が使う引用符付きの名前付き型がすべて `= type` で定義されることを native／wasm32 で確かめる。直す前は `"tz.union.Maybe.Maybe[string]" has no definition` で失敗）。E2E は suite `rc` の `shared_named_types`（3 つの型を実際に作って読む）。
2. **arena の複製の間で、ハンドルが空き slot に一致する（MEDIUM）。** 空き slot は「次の世代」と「空き列の次」を持ち、`position` は世代だけを比べていたので、関数値の複製で arena ID ごと複製された 2 つの arena の一方で作ったハンドルが、他方の空き slot に一致した（`contains`／`get` が無関係な値を返し、`remove` が別の値を swap-remove して空き列を自己ループにする）。`std/Arena.tz` で、空き slot の世代を次の世代 `g` から `-1 - g`（-2 以下）として持つように改めた。退役は従来どおり -1、ハンドルの世代は 0 以上なので、世代の比較だけで使用中の slot に限られる。`insert` の再利用は `-1 - slot.generation` で世代を戻し、`remove` は `-2 - handle.generation` を書く（`i64::MAX` の退役は先に判定するのであふれない）。詰めた位置・空き列・API の意味は変わらない。
   テスト: `tests/arena.rs` の `handles_never_match_free_slots_of_another_copy`（5 値の arena を捕捉した関数値から 2 つの複製を作り、一方で 2 つ削除、他方で削除と挿入をして、他方のハンドルを一方で `contains`／`get`／`remove` し、その後一方へ 3 値を足して詰めた順を読む。CLI の `run` で `-O0`／`-O3` を実行し、`4050300708090111` を確かめる。直す前は `40300708091221`）。E2E は suite `arena` の `arena_snapshot`（同じ手順。期待値は詰めた順 40, 50, 30, 7, 8, 9 から独立に組み立てる）。
3. **弱い親リンクで多相再帰の `E1017` が誤って出る（MEDIUM）。** `src/recursive.rs` は、解析の経路上にある同じ宣言の具体化より型引数の「重さ」が減らない具体化を `E1017` にしていた。`Rc.Weak` と `Rc` の重さが同じなので、`record TreeNode { value: i64, parent: Maybe<Rc.Weak<TreeNode>>, children: Vec<Rc<TreeNode>> }` で `Rc.upgrade` が返す `Maybe<Rc<TreeNode>>` を解析すると、経路上の `Maybe<Rc<TreeNode>>` から `Maybe<Rc.Weak<TreeNode>>` に出会って拒否した（`record Node { link: Maybe<Rc<Node>> }` と `Maybe.Some (Rc.downgrade …)`、共有ポインタと関係のない `record Node { value: i64, a: Maybe<[Node]> }` と `Maybe<Vec<Node>>` も同じ）。重さで比べるのをやめ、多相再帰を宣言ごとの性質として検査する。ジェネリックな宣言を自身の型パラメーターで解析するとき（`Graph::generic_root`）だけ、同じ宣言の別の具体化に到達したら `E1017`（引数の変化）。具体型を根とする解析では比較しない。ほかの根の解析が先に到達した「自身のパラメーターのままの宣言」の結果はキャッシュしないので、宣言の順序によらず各宣言は自分の解析で検査される（直さないと `record First<'a, 'b> { swap: Maybe<Swap<'a, 'b>> }` の後の `Swap` が素通りした）。増え続ける展開はノード 4096・深さ 128 の上限で止まり、経路に同じ宣言の別の具体化があれば `E1017`（引数の変化）、なければ従来の上限の `E1017`。
   以前と同じく拒否するもの: `Bad<['a]>`、`Swap<'b, 'a>`、`Pairs<('a * 'a)>`、`record Grow<'a> { next: Vec<Grow<['a]>> }`、相互再帰の `Ping<'a>`／`Pong<['a]>`、`record Pair<'a> { value: 'a, other: Maybe<Rc<Pair<i64>>> }`（他の宣言の後でも）。関数の多相再帰の上限（`bounds_type_growing_polymorphic_recursion`）は変わらない。
   テスト: `tests/rc.rs` の `weak_back_links_are_not_polymorphic_recursion`（`TreeNode`、`Node` の弱参照、`Arc` の双方向リスト、generic な `Node<'t>`）、`tests/recursive_types.rs` に拒否 5 件と受理 1 件を追加。E2E は suite `rc` の `rc_weak_parent`（親を解放した後の `upgrade` が `None`）。
4. **`Arc` で extern ハンドルを複数の task が同時に使える（MEDIUM）。** `Arc<H>`（`H` は `extern type`）が `Send` で捕捉もできたので、複数の task が `Arc.get` で同じホストのハンドルに extern 関数を同時に呼べた（C10 以前はハンドルを持てる task は 1 つだった）。F10 が `Sync` を入れるまでは、`Type::shareable`（格納グラフに `Rc`／`Rc.Weak`、`Type::Handle`、Copy でない dyn（ハンドルを隠しうる）、`Owned.Function`（ハンドルを捕捉しうる。`Owned.call` は借用で呼ぶ）を持たない）を満たさない値の `Arc`／`Arc.Weak` は、それを持つレコード・union・`Arc` も含めて `Send` でも捕捉可能でもない。task へ渡すと `E1013 tasks require Send values; Arc<Main.Counter> shares an extern handle, a dyn value that is not Copy, or an Owned.Function through an Arc, and several tasks could then use it at once; give the value to one task instead`、関数値の捕捉は `E1005 cannot capture … in a function value; function values may move to other tasks, and …; pass it as an argument`（`holds_unshareable_arc` で選ぶ。`Owned.function` の捕捉も `E1013` の同じ文面）。同じ task の中の `Arc.share`、ハンドルそのものを 1 つの task へ移すこと、Copy な dyn や関数値の `Arc` は従来どおり。`thread_confined` は `shareable` に置き換えた（Copy でない dyn は `Send` でも共有できない）。**F10 は内部可変性とともに、この `shareable` を `Sync` に置き換える。**
   テスト: `tests/rc.rs` の `arc_does_not_share_host_handles_between_tasks`（task への `Arc<H>`・`Arc.Weak<H>`・`Arc<Arc<H>>`・`Arc<H>` を持つ record と再帰 union・`Arc<H>` を受け取って task を返す関数・`Owned.function` の捕捉・`Arc<Owned.Function>`・`Arc<dyn (Shape, Send)>` の `E1013`、関数値の捕捉の `E1005` 2 件、受理 4 件）。拒否なので E2E は足していない。
   文書: LR の `rc.md`（ポイント、Arc とタスク、送信と捕捉の規則の表と箇条、弱い親リンクの木の実行例 `parent=7 after=-1`）、`async-tasks-and-lazy/task.md`・`values-and-functions/lambda-expressions.md`・`compiler/native-interop.md`・`compiler/diagnostics.md`（ハンドルを持つ値の `Arc`）、`arena.md`（複製の間のハンドル）、`union.md`（`E1017` の判定）、`docs/language.md`（Arena の空き slot の符号化、`Rc / Arc` の `Send`／捕捉、再帰型の `E1017`）、`docs/architecture.md`（Arena の slot の符号化、`shareable`、`generic_root`、`named_types`）。

確認（レビュー対応の後）:

- `cargo test --locked --test rc`（11 passed）、`--test arena`（8 passed）、`--test recursive_types`（4 passed）。
- `RUST_MIN_STACK=4194304 cargo test --locked`: 755 passed（Phase 2 の 751 に今回の 4 件）、失敗なし。GUIDE §3.1 の回帰テスト 4 件（`bounds_type_growing_polymorphic_recursion`・`bounds_recursive_and_flat_expression_depth`・`bounds_nested_builder_expansion_not_just_source_syntax`・`honors_the_exact_specialization_limit`）を既定の stack で個別に実行して成功。
- `cargo fmt --all -- --check`、`cargo clippy --locked --all-targets -- -D warnings` が成功。Windows の型検査 `cargo clippy --locked --all-targets --target x86_64-pc-windows-msvc -- -D warnings`（rustup 1.96.1）は従来の `clippy::nonminimal_bool` 3 件だけで、この lint を除くと警告なし（プラットフォーム固有のコードは変えていない）。
- E2E（release）: features の `rc`（37 ケース、WASM threads の検査を含む）・`arena`（27 ケース）・`recursive_types`・`parallel`・`dyn_dispatch`・`unions`・`generic_records`・`typeclasses`・`vec`・`map_set` が native／WASM × `-O0`／`-O3` で成功。`TSUZURI_TSAN=1` の `rc`・`arena`、`tests/tasks.mjs` も成功。`node scripts/check-docs.mjs`（変更した LR の 7 ページ）が成功。

### 統合後の変更（opt-in std モジュール。D-40）

- 6 チケットの統合で、`Arena` を opt-in std モジュールにした。利用者のソースが識別子 `Arena` を含むときだけ読み込み、`Arena.Handle` などの宣言は利用者のコードから修飾した名前でだけ見える。
  そのため「Arena を使わないプログラムの IR は変わらない」は、生成 id のずれも含めて byte 単位で満たす（`2ee813f` の release コンパイラと比べ、既存の fixture と例の
  native／wasm32 の IR 132 個がすべて byte 一致）。
  無修飾の `Handle` は従来どおり `File.Handle` を指す。`Rc`／`Arc` は組み込みの型で std のソースを持たないので、この仕組みの対象外。

### PR #17 の Copilot のレビュー（2026-10-08）

- 「ハンドルの `Eq`・`Ord`・`Hash` が arena ID を含まず、別の arena のハンドルが `Map`／`HashMap` のキーとして同一視される」という指摘には、D5 を保った。
  arena ID の数値は並列の task で実行ごとに変わり得るので、`Ord`・`Hash` に入れるとキー順とハッシュが決定的でなくなり、`Eq` だけに入れると `Ord` と食い違う。
  Rust の `slotmap` のキーと同じ比較であることと、表のキーには 1 つの arena のハンドルだけを入れること（複数の arena では利用者の区別をキーに含めること）を
  `arena.md` と docs/language.md に明記した。arena への読み書きは別の arena のハンドルを常に検出する。
