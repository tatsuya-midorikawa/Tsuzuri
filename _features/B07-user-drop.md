# B07: ユーザー定義の解放処理（Drop）とリソース型

| 項目 | 内容 |
| --- | --- |
| ID | B07 |
| 優先度 | P1 |
| 規模 | L |
| 依存 | A06 |
| 後続 | E08, E12, B08, C10, F10 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善）。2026-09-29 実装者向けに詳細化（HEAD `f8dc655`） |
| 承認 | 要承認: D1（組み込みクラス `Drop` の追加と対象型の制限。GUIDE D-30 の仮割り当て）, D2（`drop` の引数を `ref mut` にする。起票時の既定案 `ref` からの変更）, D4（`Drop` 型からの move の禁止） |
| 改善する劣位 | C#/F# 比: `IDisposable`／`use` に相当する資源管理がない（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#cf-に対する劣位点)）／追加: ホスト資源を所有できない |
| 手本にする既存実装 | 利用者 instance を持つ組み込みクラス: `Eq`・`Display`（`src/polymorph.rs` の `BUILTIN_CLASSES`・`Classes`、`tests/fixtures/display_parse/Main.tz` の `instance Display<Label>`）。型性質: `src/check.rs` の `Type::is_copy`・`Type::needs_drop` と組み込み制約の評価（`src/polymorph.rs` の `"Copy" => ty.is_copy(types)`）。drop glue: `src/llvm.rs` の `FunctionEmitter::drop_value` の `Type::Record`・`Type::Union` 分岐と `FunctionEmitter::spill`。反復 drop: `src/llvm_recursive.rs` の `emit_helpers`（`drop_pending`）。ホストの記録: `tests/fixtures/host_imports/Main.tz` と `tests/host_imports.mjs` |
| 主な影響ファイル | `src/polymorph.rs`, `src/check.rs`, `src/derive.rs`, `src/ownership.rs`, `src/llvm.rs`, `src/llvm_recursive.rs`, `tests/user_drop.rs`（新規）, `tests/user_drop.mjs`（新規）, `tests/user_drop_runtime.c`（新規）, `tests/fixtures/user_drop/Main.tz`（新規）, `README.md`, `docs/language.md`, `docs/architecture.md`, `_docs/language-reference/ownership.md`, `_docs/language-reference/generics-and-typeclasses.md`, `_docs/guides/from-fsharp.md`, `_docs/feature-status.md`, `_features/README.md` |

## 目的

ファイル記述子・ソケット・GPU バッファ・ホストのハンドルなど、メモリ以外の資源を、所有値が終わる時点で一度だけ確実に解放できるようにする。
Rust の `Drop`、C++ の RAII、C# の `IDisposable` に相当し、OS API（E08）、不透明ハンドル（E12）、`dyn` 値の drop slot（A14 D10）の前提になる。

利用者は record・union に `instance Drop<T>` を書き、コンパイラは既存の drop glue（`FunctionEmitter::drop_value`）の先頭でその
`drop` を呼ぶ。新しい構文・予約語・ランタイム記号は足さない。`Drop` を書かないプログラムの IR は byte 単位で変えない。

## 着手条件と停止条件

### 着手条件

- A06（既定メソッド・スーパークラス）が `_features/README.md` の状態欄で done であること。
  確認: `grep -n "A06\|B07" _features/README.md`。
- D1・D2・D4 が承認済みであること。承認前はどの手順にも着手しない（手順 1 のベースラインだけは取ってよい）。
- GUIDE §2.3 の基準コマンドが成功し、手順 1 のベースライン（IR と stack-depth テスト）を保存していること。
- A14 は開始条件にしない。A14 の drop slot は `drop_value(T)` と同じ glue を呼ぶ（A14 D10）ので、どちらが先に入っても整合する。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- `BUILTIN_CLASSES` に `"Drop"` を足すと、既存の std・テスト・文書サンプルの型名 `Drop` と衝突する（組み込みクラスは record 型と
  名前空間を共有する）。HEAD の `std/` に `record Drop`・`union Drop` はない。
- 利用者 `drop` の特殊化を、ユーザーの非ジェネリック関数から辿れる型だけで決められない（std の generic を先回りして特殊化しないと
  要求が集まらない）。GUIDE §11.1 の特殊化予算（1,024）を消費する設計になった。
- `FunctionEmitter::drop_value` が値（SSA）ではなくポインターしか持たない経路、または `spill` できない経路（`@tz.rec.drop` の helper 内、
  `array_loop`・`list_loop` の本体）で、`ref mut` の引数を作れない。
- 利用者 `drop` の呼び出しが、再帰 union の反復 drop の待ちリスト（`drop_pending`）に積まれた node を二度解放しうる、
  または待ちリストの処理中に再帰呼び出しを要する。
- `src/runtime/` の C または生成 `.ll` の変更が必要になった（生成物は手で直さない。GUIDE §2.2）。
- 既存テストの期待値（IR・診断コード・メッセージ）を変える必要がある。`BUILTIN_CLASSES` の要素数の変更だけは除く。
- stack-depth の 3 テストか `honors_the_exact_specialization_limit` が失敗する。または上限・stack サイズを上げたくなった。
- `unsafe`、新しい crate、既定の WASM import が必要になった（E2E の `drop_log` はテスト fixture の `extern def` であり既定の import ではない）。

## 現状（HEAD `f8dc655` で確認）

- 組み込みクラスは `src/polymorph.rs` の `BUILTIN_CLASSES`（`[&str; 25]`。`Copy`・`Capture`・`Send`・`Display`・`Hash` など）で、
  `Drop` はない。コメントどおり record 型と名前空間を共有する。`Copy`・`Capture`・`Send` はメソッドのない性質クラスで、
  制約の評価は `"Copy" => ty.is_copy(types)`・`"Capture" => ty.can_capture(types)`・`"Send" => ty.can_send(types)` の形で
  型性質に委ねる。`"SimdVector" | "Copy" | "Capture" | "Send" => true` の分岐がこれらを特別扱いする。
- 利用者 instance の構文は `instance Display<Label> { fn to_string value = ... }` の形（`tests/fixtures/display_parse/Main.tz`、
  `tests/fixtures/typeclasses/Main.tz` の `instance Eq<'a> => Eq<Box<'a>>`）。instance の中身は `Classes` の `InstanceTemplate`
  （`class`・`head`・`constraints`・`methods`・`derived`）に集まる。
- 型性質は `src/check.rs` の `Type::is_copy`・`Type::needs_drop`・`Type::can_capture`・`Type::can_send`（どれも `&TypeContext<'_>` を取る）。
  record・union は `TypeContext::record_fields_all`・`union_payloads_all` で field・payload を調べる。再帰型の判定は
  `src/recursive.rs` の `recursive`。所有権検査は `src/ownership.rs` の `Checker::is_copy` で Copy を判断する。
- drop glue は `src/llvm.rs` の `FunctionEmitter::drop_value(ty, value)` だけで、`value` は SSA 値である。
  - `Type::Record`: `record_fields` の宣言順に、`needs_drop` な field を `extractvalue` して再帰的に drop する。
  - `Type::Union`（非再帰）: `owning_cases` と `case_switch` で case ごとに payload を drop する。General 配置では `spill` した slot から読む。
  - 再帰 union: `self.globals.recursive_types` に登録し、`drop_pending` があれば `@tz.rec.enqueue(ptr, ptr pending)`、なければ
    `@tz.rec.drop(ptr)` を呼ぶ。helper 本体は `src/llvm_recursive.rs` の `emit_helpers` が型ごとに作り、その中では
    `drop.drop_pending = Some("%pending".into())` にして子を待ちリストへ積む（反復 drop。スタックを消費しない）。
  - `Array`・`Vec`・`List` は `array_loop`・`list_loop` で要素ごとに `drop_value` を呼ぶ。
  - scope の終わりは `FunctionEmitter::drop_scope(scope: &[(String, Type)])`。複製は `FunctionEmitter::clone_value`。
- `use` 束縛は `src/parser.rs` が `"use bindings are not supported; bind the owned value with let and rely on lexical drop"` で拒否する（E1018）。
  本チケットはこれを変えない。
- `std/` に値を明示的に破棄する関数（`drop`・`dispose` など）はない。早期解放は、値を消費する関数に渡すか内側の block で終わらせる。
- docs/architecture.md の「初版の次に必要な設計」は任意の destructor を未対応とする。`Gpu.Device` などの opaque・非 Copy 型は
  コンパイラ登録の特別扱いで、利用者は同様の資源型を定義できない。

### 再現（検証済み）

`/tmp/tz-work-B07/` の scratch で 2026-09-29 に確認した。`Main.tz` の中身は各行の左に書いたものだけを変えた。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/release/tsuzuri check /tmp/tz-work-B07/inst    # instance Drop<Resource> { fn drop value = () }
target/release/tsuzuri check /tmp/tz-work-B07/derive  # record Resource { handle: i64 } deriving (Copy)
target/release/tsuzuri check /tmp/tz-work-B07/copy    # instance Copy<Resource> {}
```

結果は順に `error[E1016]: unknown type class 'Drop'`、`error[E1025]: only Eq, Ord, Display, Hash and Default can be derived`、
`error[E1016]: built-in instances and marker classes cannot be overridden`。後の 2 つは本チケット後も同じで、`deriving (Drop)` も
同じ E1025 になる（新しい診断は要らない）。

関数値は Copy で、非 Copy の値を捕捉した関数値も複製できる。`let f = \_ -> text.length` と `let g = f` を含むプログラム
（`/tmp/tz-work-B07/capture`）は検査を通り、IR に `@tz.env.clone.*` が出る。つまり関数値の複製は捕捉した値を `clone_value` で深く複製する。
D5 はこの事実から決めた。

## 仕様

実装者は Phase 1 だけを実装する。Phase 2 は人間が求めた場合だけ着手する。

### 前提とする他チケットのインターフェース

- A14（任意）: `dyn C` の drop slot は `FunctionEmitter::drop_value(T)` と同じ glue を呼んでから data を解放する（A14 D10）。
  本チケットは A14 に何も要求しない。A14 が先に入っていれば、手順 9 の E2E に `dyn` 値の場合を 1 件足す。

### 構文

新しい構文はない。`Drop` はソースに宣言のない組み込みクラスで、次の宣言と同じ意味を持つ（表示用。`class` として書くことはできない）。

```text
class Drop<'a> {
    def drop :: ref mut 'a -> unit
}
instance Drop<Resource> { fn drop value = ... }            -- 型引数のない record・union
instance Drop<Handle<'a>> { fn drop value = ... }          -- ジェネリック型は全体を覆う
```

### 型規則

1. `Drop` instance の head は、ソースで宣言した record・union（std の型は不可）に、互いに異なる型変数をすべての型引数へ当てたもの。
   制約付き instance（`instance C<'a> => Drop<Handle<'a>>`）は書けない。違反は E1016（「診断」）。
2. `Drop` instance を持つ型（以下「Drop 型」）は Copy ではない。Drop 型を field・payload・要素に持つ型も、既存の構造的な規則で Copy でない。
   `Copy` を要求する制約・推論（`ownership.rs` の `require_copy` を含む）は満たせない。
3. Drop 型は `needs_drop` が真（field に解放の要るものがなくても）。
4. Drop 型を含む型は `Capture` を満たさない（D5）。`Send` は field・payload の規則のままで、Task へは送れる。
5. `Drop.drop` を式で参照する（呼び出しを含む）と E1016。instance の本体の中でも同じ。
6. 制約 `Drop<'a>` は特別扱いしない。呼べるメソッドがないので書いても意味はない。

### 評価順序・所有権・借用

- Drop 型の値の drop glue は、利用者 `drop` を一回呼び、その後 field・payload を `drop_value` の既存順序（宣言順）で drop する。
  `drop` が `ref mut` で field を書き換えた場合は、書き換え後の値の field を drop する（glue は呼び出し後に値を読み直す）。
- drop の時点と回数は既存の `drop_value` の呼び出し位置そのもの: scope の終わり（`drop_scope` は束縛の逆順）、`break`・`continue`・
  `return` による脱出、代入で置き換わる古い値、捨てた一時値、コレクションの要素、未開始の Task の環境、再帰 union の node。
  move 済みの値では呼ばない（既存の move 追跡のまま）。
- Drop 型から non-Copy の field・payload を move すること（`r.name` の消費、値による pattern 束縛の消費）は E1012。Copy の field の読み出し、
  `ref` による借用、値全体の move は許す。record 更新 `{ r with ... }` の基が Drop 型なら E1012。
- `drop` 本体の中で引数全体を置き換える代入は E1012（置き換えは古い値の drop、つまり同じ `drop` を再び呼ぶので無限再帰になる）。
  field への代入は許す。
- 再帰 union の node: 反復 drop の helper が node ごとに利用者 `drop` を呼んでから子を待ちリストへ積む。利用者 `drop` の呼び出しは
  再帰せず、100 万 node でもスタックを消費しない。
- 関数値の複製は捕捉値を複製するので、Drop 型は関数値に捕捉できない（D5）。Task の環境は複製されない（clone pointer が `null`）ので、
  Drop 型を `task { ... }` に捕捉して別スレッドへ送れる。drop は環境の所有者が終わる所（Task の実行後、または未開始の Task の破棄）で走る。

### 数値・トラップ・native と WASM の差

- trap は巻き戻さない。trap の後、未実行の利用者 `drop` は走らない（既存の方針）。`drop` 本体の trap は通常の trap で、`--trap-info` の
  位置は `drop` 本体を指す。
- native と WASM で drop の順序・回数は同じ。並列の Task で走る `drop` 同士の順序は保証しない（E2E は順序に依存しない値だけを見る）。

### 診断

| コード | 条件 | メッセージ | 位置 |
| --- | --- | --- | --- |
| E1016 | head が record・union でない、または std の型 | `only records and unions declared in this program can implement Drop` | instance のクラス名 |
| E1016 | head の型引数が互いに異なる型変数でない | `a Drop instance must cover every instantiation; write every type parameter as a distinct type variable` | instance のクラス名 |
| E1016 | 制約付きの `Drop` instance | `Drop instances cannot have constraints; drop must work for every instantiation` | instance のクラス名 |
| E1016 | `Drop.drop` の参照 | `'Drop.drop' runs automatically when a value is dropped; let the value go out of scope or pass it to a function that consumes it` | メソッド名 |
| E1016（既存） | `Drop` instance の重複 | `overlapping instance for Drop<...>` | instance のクラス名 |
| E1016（既存） | `instance Copy<T>` | `built-in instances and marker classes cannot be overridden` | instance のクラス名 |
| E1025（既存） | `deriving (Drop)`・`deriving (Copy)` | `only Eq, Ord, Display, Hash and Default can be derived` | deriving のクラス名 |
| E1012 | Drop 型から non-Copy の field・payload を move | `cannot move a field or payload out of a value whose type implements Drop; borrow it with 'ref' instead` | move する式 |
| E1012 | record 更新の基が Drop 型 | `cannot update a value whose type implements Drop; construct a new value instead` | record 更新の式 |
| E1012 | `drop` 本体で引数全体へ代入 | `cannot replace the whole value inside Drop.drop; assign its fields instead` | 代入の式 |
| E1005（既存） | Drop 型を関数値に捕捉 | `cannot capture {型} in a reusable function; fully apply exclusive borrows and keep single-use tasks in task blocks`（既存の文のまま） | 関数値の式 |

### 資源上限

- 利用者 `drop` の特殊化は既存の特殊化上限（`MAX_SPECIALIZATIONS` = 1,024）に数える。超えれば既存の E1017。
- 新しい上限は足さない。反復 drop の待ちリストは既存の helper のまま。

### 例

新 API（実装後に有効。未検証）。`drop_log` は E06 のホスト関数で、ホストが呼び出し順を記録する。

```tsuzuri
extern def drop_log :: i64 -> unit

record Resource { id: i64, name: string }

instance Drop<Resource> {
    fn drop value = drop_log value.id
}

export def scopes :: i64
fn scopes =
    let first = Resource { id: 1, name: "a" }
    let second = Resource { id: 2, name: "b" }
    first.id + second.id
```

`scopes` は `3` を返し、ホストの記録は `[2, 1]`（`drop_scope` は束縛の逆順）。次は拒否される例で、同じく新 API（実装後に有効。未検証）
である（実装前は E1016 `unknown type class 'Drop'` になる）。

```tsuzuri
fn take_name value = value.name                  -- E1012: Drop 型から string の field を move
fn rename value = { value with name = "c" }      -- E1012: 基が Drop 型の record 更新
fn close value = Drop.drop (ref mut value)       -- E1016: Drop.drop の参照
instance Drop<i64> { fn drop value = () }        -- E1016: record・union でない
```

### Phase 2（設計方針）

- B05 で未対応の計算式 `use`／`use!` の糖衣化（lexical drop と同じ意味）と、`use` 束縛（HEAD は E1018）の再検討。
- 明示的な早期解放 API（std の関数）。Phase 1 は値を消費する関数に渡すか内側の block で終わらせる。
- 関数値への捕捉（D5）。複製できない関数値の型が要る。

## 設計

### データ構造

```rust
// src/check.rs
pub struct CheckedRecord { /* 既存の field */ pub user_drop: bool /* （新規） */ }
pub struct CheckedUnion { /* 既存の field */ pub user_drop: bool /* （新規） */ }
pub struct CheckedModule {
    /* 既存の field */
    /// Concrete Drop type -> specialized `Drop.drop` function id. Empty without Drop instances.
    pub user_drops: BTreeMap<Type, usize>, // （新規）
}

impl Type {
    /// Record or union declared with a user `Drop` instance (any type arguments).
    pub(crate) fn has_user_drop(&self, types: &TypeContext<'_>) -> bool; // （新規）
}

// src/ownership.rs
struct Place {
    /* 既存の root, fields */
    through_drop: bool, // （新規）a field or payload step starts at a Drop type
}
```

`user_drop` は型構成子ごとの印で、ジェネリック型でも一つ。`has_user_drop` は `Type::Record(id, _)` なら `types.records[id].user_drop`、
`Type::Union(id, _)` なら `types.unions[id].user_drop`、それ以外は false。`Place::through_drop` は同じ root・fields なら常に同じ値に
なる（型から決まる）ので、`Place` の比較・`overlaps` の意味は変わらない。

### 段ごとの変更

| 段 | ファイル | 関数・型 | 変更内容 |
| --- | --- | --- | --- |
| クラス | `src/polymorph.rs` | `BUILTIN_CLASSES` | `"Drop"` を足し、配列長を 26 にする |
| クラス | `src/polymorph.rs` | `Classes::collect` | `Display`・`Hash` と同じく `Method { name: "drop", signature: Ok(Signature { parameters: vec![Type::Reference(Box::new(a), true)], result: Type::Unit }), operation: None, default: None }` を push する |
| クラス | `src/polymorph.rs` | `Classes::intrinsic` | 変更なし（`_ => false` で組み込み instance はない） |
| instance | `src/polymorph.rs` | `Classes::instances`（`"built-in instances and marker classes cannot be overridden"` を出す分岐の後） | クラスが `Drop` のとき head と制約を型規則 1 で検査し、E1016 の 3 メッセージを出す |
| 参照 | `src/polymorph.rs` | `Classes::method` | 解決したクラスが `Drop` なら E1016（`method.span`） |
| 印 | `src/check.rs` | `Classes::collect` を呼ぶ関数（`classes.instances(...)` の直後） | Drop instance の head の id で `user_drop` を立てる（アルゴリズム 1） |
| 性質 | `src/check.rs` | `Type::is_noncopy_record` | `\|\| self.has_user_drop(types)` を足す。呼び出し元 3 か所（`Type::is_copy`、`ownership.rs` の `Checker::is_copy`・`require_copy`）が自動で従う |
| 性質 | `src/check.rs` | `Type::needs_drop` | 先頭で `if self.has_user_drop(types) { return true; }` |
| 性質 | `src/check.rs` | `Type::can_capture` | 再帰型の分岐と通常の分岐の前で `if self.has_user_drop(types) { return false; }`（field 経由は既存の構造的な走査が拾う） |
| 性質 | `src/check.rs` | `Type::can_send` | 変更なし |
| 特殊化 | `src/polymorph.rs` | `specialize`, `Specializer` | 要求の処理が終わった後に Drop 型を集めて `drop` を要求し、`user_drops` を作る（アルゴリズム 2） |
| 所有権 | `src/ownership.rs` | `Checker::place` と `PAYLOAD`・field 番号を `Place::fields` へ積む全箇所 | 積む前の型が Drop 型なら `through_drop = true` |
| 所有権 | `src/ownership.rs` | `Checker::read_places` | `moving && place.through_drop` なら E1012（move out） |
| 所有権 | `src/ownership.rs` | `Checker::eval_value` の `E::Record(_) \| E::RecordUpdate { .. }` | `RecordUpdate` で式の型が Drop 型なら E1012 |
| 所有権 | `src/ownership.rs` | `Use::Write` の検査（`Checker::access`） | 検査中の関数が `user_drops` の値で、place が第 1 引数の root そのもの（`fields` が空）なら E1012 |
| LLVM | `src/llvm.rs` | `FunctionEmitter::drop_value` | 非再帰の Drop 型で利用者 `drop` を呼び、値を読み直してから既存の分岐へ（「生成 IR」） |
| LLVM | `src/llvm_recursive.rs` | `emit_helpers` の drop helper | `case_switch` の前で node の利用者 `drop` を呼ぶ |
| LLVM | `src/llvm.rs` | `FunctionEmitter::clone_value` | Drop 型では `unreachable!("Drop types are never cloned")`。到達したら停止条件 |
| LLVM | `src/llvm_frame.rs` | — | 変更なし（解放は `drop_value` 経由）。手順 8 で `grep -n "drop" src/llvm_frame.rs` を見て独自の解放がないことを確かめる |
| 派生 | `src/derive.rs` | — | 変更なし（`deriving (Drop)` は既存の E1025） |

### 生成 IR とランタイム

非再帰の Drop 型（record `Main.Resource`）の `drop_value`。`spill` は `slot` を使うので alloca は関数の入口に置かれる。

```llvm
store %"tz.record.Main.Resource" %v, ptr %slot            ; FunctionEmitter::spill
call void @<Drop.drop の特殊化の記号>(ptr %slot)             ; 利用者関数の直接呼び出しと同じ記号・引数規約
%v.1 = load %"tz.record.Main.Resource", ptr %slot
%name = extractvalue %"tz.record.Main.Resource" %v.1, 1    ; 以降は既存の field の drop
```

- 呼び出しは、式の `FunctionRef::User(id)` を直接呼ぶ既存の経路（`grep -n "FunctionRef::User" src/llvm.rs`）と同じ記号・引数規約で出す。
  hidden 引数・`sret` が要るなら停止条件。
- 再帰 union: helper の `%node` は node への `ptr`。`slot(ty)` に `%node` を store し、利用者 `drop` を呼び、`load ptr` で読み直した node で
  既存の `case_switch` 以降を行う。`drop_pending` はそのまま（子は待ちリストへ積まれる）。
- ランタイム（`src/runtime/`）・ヘッダー・`Globals` の固定部分は変えない。`user_drops` が空なら生成 IR は HEAD と byte 単位で同じ。

### アルゴリズム

1. 印（check）: `classes.instances(...)` の後、クラスが `Drop` の instance の head（`Type::Record(id, _)`・`Type::Union(id, _)`）の
   id で `records[id].user_drop`・`unions[id].user_drop` を立てる。`types` が records を借用していて書けないなら、印を立ててから
   `types` を作り直す（`TypeContext` は `Copy` の 2 参照で安い）。`Classes::collect` から印を立てるまでの間に Drop 型の `is_copy`・
   `needs_drop` を使う箇所があれば停止条件。
2. 特殊化（polymorph）: Drop instance が一つもなければ何もしない。あれば次を固定点まで繰り返す。

```text
done = {}; scanned = {}
loop
  found = BTreeSet()
  for f in 特殊化済みの関数 (id 順) で scanned にないもの:
    scanned += f
    for t in f の引数・結果・全 TypedExpr の ty（walk で集める）:
      for u in components(t):            # t 自身、record の field、union の payload、tuple・Array・List・Vec の要素、Task の結果
        if u.has_user_drop() and u not in done: found += u
  if found が空: break
  for u in found:                          # BTreeSet 順で決定的
    (function, arguments) = classes.resolved_method(drop_class, 0, u, types)   # head が型全体を覆うので必ずある
    user_drops[u] = request(function, arguments, instance の span); done += u
  既存の要求処理を再び回し、新しい特殊化を終える
```

`components` は訪問済み集合を持ち、再帰 union でも止まる。関数型の引数・結果は所有しないので辿らない。std の generic を先回りして
特殊化しない（既に特殊化された関数の型だけを見る）。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test --locked <filter>` は 0 件でも成功するので、
`running N tests` の N が期待どおりかを必ず見る（GUIDE §3.1）。Rust テストの名前と中身は「テスト計画」の表に従う。

### 手順 1: ベースラインを取る

- 変更: なし。
- 内容: GUIDE §2.3 の基準コマンドを実行し、`Drop` を使わない fixture の IR を保存する（手順 10 で byte 比較する）。
- 確認: 次がすべて成功し、stack-depth の 3 テストと特殊化上限のテストはそれぞれ `1 passed`。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
cargo build --release --locked
mkdir -p /tmp/tz-b07/before
for f in recursive_types tasks map_set unions host_imports; do
  target/release/tsuzuri build tests/fixtures/$f --emit llvm -o /tmp/tz-b07/before/$f.ll || break
done
cargo test --locked --test polymorphism bounds_type_growing_polymorphic_recursion
cargo test --locked --lib bounds_recursive_and_flat_expression_depth
cargo test --locked --test computations bounds_nested_builder_expansion_not_just_source_syntax
cargo test --locked honors_the_exact_specialization_limit
```

### 手順 2: クラス `Drop` と instance の検査

- 変更: `src/polymorph.rs` の `BUILTIN_CLASSES`・`Classes::collect`・`Classes::instances`、`tests/user_drop.rs`（新規）。
- 内容: 「段ごとの変更」のクラス・instance の行。テストファイルには `tests/polymorphism.rs` の `accepts`（IR を 2 回出して一致を確認）と
  `rejects` を写し、`CheckedModule` を返すだけの `checks(source)` を足す。先に `grep -rn "record Drop\|union Drop\|type Drop" std tests examples _docs`
  が空であることを見る（空でなければ停止条件）。
- 確認: `cargo test --locked --test user_drop` が `7 passed`。`cargo test --locked` が成功する。

### 手順 3: `Drop.drop` の参照を拒否する

- 変更: `src/polymorph.rs` の `Classes::method`。
- 内容: `position` で method を見つけた後、クラス名が `Drop` なら E1016（`method.span`）。
- 確認: `cargo test --locked --test user_drop` が `8 passed`。

### 手順 4: 印と型性質

- 変更: `src/check.rs` の `CheckedRecord`・`CheckedUnion`（`user_drop`）、`Type::has_user_drop`（新規）、`Type::is_noncopy_record`、
  `Type::needs_drop`、`Type::can_capture`、`Classes::collect` を呼ぶ関数（アルゴリズム 1）。`CheckedRecord {`・`CheckedUnion {` の構築箇所すべて
  （`grep -n "CheckedRecord {\|CheckedUnion {" src/*.rs`）に `user_drop: false` を足す。
- 確認: `cargo test --locked --test user_drop` が `11 passed`。`cargo test --locked` が成功する（Drop のないプログラムでは印が立たない）。

### 手順 5: `drop` の特殊化

- 変更: `src/check.rs` の `CheckedModule`（`user_drops`。構築箇所すべてに `BTreeMap::new()`）、`src/polymorph.rs` の `specialize`・`Specializer`。
- 内容: アルゴリズム 2。Drop instance がなければ走査もしない。
- 確認: `cargo test --locked --test user_drop` が `13 passed`。`cargo test --locked honors_the_exact_specialization_limit` が `1 passed`。

### 手順 6: 所有権の規則

- 変更: `src/ownership.rs` の `Place`（`through_drop`）、`Checker::place` と `PAYLOAD`・field 番号を積む全箇所（`grep -n "PAYLOAD\|fields.push" src/ownership.rs`）、
  `Checker::read_places`、`Checker::eval_value` の record 更新、`Checker::access` の `Use::Write`。
- 内容: 「段ごとの変更」の所有権の 4 行。E1012 の 3 メッセージは「診断」の表の文と一字一句同じにする。
- 確認: `cargo test --locked --test user_drop` が `17 passed`。`cargo test --locked --test types_ownership` と `cargo test --locked` が成功する。

### 手順 7: drop glue

- 変更: `src/llvm.rs` の `FunctionEmitter::drop_value`・`FunctionEmitter::clone_value`。
- 内容: 非再帰の Drop 型なら `spill`、利用者 `drop` の直接呼び出し、`load` で読み直し、既存の `match` へ読み直した値を渡す。
  `FunctionEmitter::slot` が alloca を関数の入口に置くことを IR で確かめる（ループ内に alloca が出たら停止条件）。
- 確認: `cargo test --locked --test user_drop` が `18 passed`。`cargo test --locked` が成功する。

### 手順 8: 再帰 union の反復 drop

- 変更: `src/llvm_recursive.rs` の `emit_helpers`（drop helper）。`src/llvm_frame.rs` は `grep -n "drop" src/llvm_frame.rs` で見るだけ。
- 内容: helper で `%node` を slot へ store し、利用者 `drop` を呼び、node を読み直してから `case_switch`。payload のない case の値にも呼ぶ。
- 確認: `cargo test --locked --test user_drop` が `19 passed`。`cargo test --locked --test recursive_types` と `cargo test --locked` が成功する。

### 手順 9: E2E `tests/user_drop.mjs`

- 変更: `tests/fixtures/user_drop/Main.tz`（新規）、`tests/user_drop.mjs`（新規）、`tests/user_drop_runtime.c`（新規）。
- 内容: `tests/host_imports.mjs` と `tests/host_imports_runtime.c` を写す（header の出力、`@malloc` などを `@tracked_*` へ置き換えた IR、
  object を link した実行、WASM の import 名の確認）。ホスト関数は native が `tsuzuri_host_Main_drop_log`、WASM が `"Main.drop_log"`。
  C のプロトタイプは生成した header に合わせる。両ホストとも最初の 64 件の値、件数、合計を記録する。
- 確認: `cargo build --release --locked && node tests/user_drop.mjs target/release/tsuzuri` が `user drop: native/WASM -O0 passed` と
  `-O3 passed` の 2 行を出す。

### 手順 10: Drop のないプログラムの IR が変わらない

- 変更: なし。
- 内容: 手順 1 と同じ 5 fixture の IR を出して byte 比較する。
- 確認: 次の `cmp` がすべて無出力で成功する。差があれば停止条件。

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
mkdir -p /tmp/tz-b07/after
for f in recursive_types tasks map_set unions host_imports; do
  target/release/tsuzuri build tests/fixtures/$f --emit llvm -o /tmp/tz-b07/after/$f.ll && cmp /tmp/tz-b07/before/$f.ll /tmp/tz-b07/after/$f.ll || break
done
```

### 手順 11: 文書

- 変更: 「ドキュメント」の全ファイル。
- 確認: `node scripts/check-docs.mjs docs/language.md docs/architecture.md _docs/language-reference/ownership.md _docs/language-reference/generics-and-typeclasses.md _docs/guides/from-fsharp.md _docs/feature-status.md`
  が成功する。

### 手順 12: 最終確認

- 確認: GUIDE §3 の整形・lint コマンド、`cargo test --locked`、
  `node tests/user_drop.mjs target/release/tsuzuri`、`node tests/host_imports.mjs target/release/tsuzuri`、
  `node tests/features.mjs target/release/tsuzuri tasks`、手順 1 の stack-depth 3 テストがすべて成功する。

## テスト計画

### Rust テスト

`tests/user_drop.rs`（新規）。手順の列は、そのテストを足す手順。

| 手順 | テスト | 内容と期待 |
| --- | --- | --- |
| 2 | `accepts_drop_instances_for_records_and_unions` | record と union の instance を受理し、IR が 2 回とも同じ |
| 2 | `accepts_generic_drop_instance_covering_the_type` | `instance Drop<Handle<'a>>` を受理 |
| 2 | `rejects_drop_for_builtin_and_std_types` | `i64`・`[i64]`・`i64 * i64`・`Option<'a>` の head がそれぞれ E1016（records and unions の文） |
| 2 | `rejects_partial_or_repeated_type_arguments` | `Handle<i64>` と `Pair<'a, 'a>` が E1016（every instantiation の文） |
| 2 | `rejects_constrained_drop_instance` | `instance Eq<'a> => Drop<Handle<'a>>` が E1016（constraints の文） |
| 2 | `keeps_existing_copy_and_deriving_errors` | `instance Copy<Resource> {}` が E1016、`deriving (Drop)` が E1025（「再現」の文） |
| 2 | `rejects_duplicate_drop_instance` | 同じ型への 2 つの instance が E1016 `overlapping instance for Drop<...>` |
| 3 | `rejects_direct_drop_calls` | 関数本体と instance 本体の `Drop.drop (ref mut value)` がどちらも E1016 |
| 4 | `drop_types_are_not_copy` | `i64` だけを持つ Drop 型を 2 回使うと E1012 `use of moved or partially moved value 'r'` |
| 4 | `drop_types_cannot_be_captured_by_function_values` | `\_ -> r.id` が E1005 で、文が `cannot capture ` で始まり ` in a reusable function;` を含む |
| 4 | `drop_types_can_be_sent_to_tasks` | `task { return r.id }` を受理 |
| 5 | `specializes_drop_once_per_concrete_type` | `Handle<i64>`・`Handle<string>` を使うと `user_drops.len() == 2` |
| 5 | `programs_without_drop_have_no_user_drops` | Drop instance のないプログラム（record・union・Task を含む）で `user_drops.is_empty()` |
| 6 | `rejects_moving_fields_out_of_drop_types` | string field の消費、`match` の値束縛で payload の消費がどちらも E1012（move out の文） |
| 6 | `allows_copy_fields_and_borrows_of_drop_types` | `r.id` の読み出し、`ref r.name`、値全体の move を受理 |
| 6 | `rejects_record_update_of_drop_types` | `{ r with name = "c" }` が E1012（update の文） |
| 6 | `rejects_replacing_the_whole_value_inside_drop` | `drop` 本体の引数全体への代入が E1012、field への代入は受理 |
| 7 | `drop_glue_calls_user_drop_before_fields` | IR で、利用者 `drop` の call が同じ関数内の field の `@tz.free` より前にあり、IR が 2 回とも同じ |
| 8 | `recursive_drop_calls_user_drop_before_enqueue` | 再帰 union の drop helper の本文で、利用者 `drop` の call が `@tz.rec.enqueue` より前にある |

### E2E

`tests/fixtures/user_drop/Main.tz`（新規）と `tests/user_drop.mjs`（新規）。native（`-O0`・`-O3`、IR を clang に渡す経路と object を link する経路）と
WASM（`-O0`・`-O3`）で同じ期待値を確かめる。期待値は下の定義から手で計算した値で、コンパイラの出力から写さない。各呼び出しの前に記録を空にする。

| export | 定義 | 返り値 | 記録 |
| --- | --- | --- | --- |
| `scopes` | `Resource` 1、2 の順に束縛し id の和を返す | 3 | `[2, 1]` |
| `fields` | `Outer { id: 1, first: R10, second: R11 }`（Outer も Drop 型）の `id` | 1 | `[1, 10, 11]` |
| `reassign` | `let mut r = R1`、`r = R2`、`r.id` | 2 | `[1, 2]` |
| `consume` | R7 を消費する private 関数に渡し、その関数が id を返す | 7 | `[7]` |
| `loop_break` | i = 0 から毎回 R i を作り、i == 3 で `break`。作った数を返す | 4 | `[0, 1, 2, 3]` |
| `array` | `[R1, R2, R3]` の長さ | 3 | `[1, 2, 3]` |
| `map_remove` | key 1〜3 に R1〜R3 を入れ、key 2 を `Map.remove`、`drop_log 100`、残りの件数 | 2 | 先頭 2 件が `[2, 100]`、残り 2 件を整列すると `[1, 3]` |
| `generic` | `Handle<i64> { id: 4 }`、`Handle<string> { id: 5 }` の順に束縛し id の和 | 9 | `[5, 4]` |
| `chain` | `Link (R1, Link (R2, Link (R3, End)))`（`union Chain = End \| Link of Resource * Chain`、Chain は Drop 型でない）の長さ | 3 | `[1, 2, 3]` |
| `long_chain n` | Drop 型 `union Counted = Stop \| More of Counted`（drop は `drop_log 1`）を n = 1,000,000 段作って長さを返す | 1000000 | 件数 1,000,001、合計 1,000,001 |
| `task_owned` | R7 を `task { return r.id }` に捕捉して `Task.run` | 7 | `[7]` |
| `task_unstarted` | R8 を捕捉した Task を作り、実行せずに 0 を返す | 0 | `[8]` |
| `tasks_parallel` | 16 個の Task がそれぞれ R i（i = 0..15）を所有し id を返す。`Task.parallel` の結果の和 | 120 | 件数 16、合計 120（順序は見ない） |
| `drop_allocates` | drop 本体で `to_string value.id` を作り、その長さを記録する型の値 R5 | 5 | `[1]` |
| `trap_after_create` | R9 を作ってから `assert false` | trap | なし |

- native: 各呼び出しの後で `live == 0`。`trap_after_create` は mode 引数で別プロセスとして実行し、終了状態が 0 でないこと、`drop_log` が
  その mode でだけ stdout に書く `dropped` が出ないことを確かめる。
- WASM: `WebAssembly.Module.imports` が `["Main.drop_log"]` だけ。`trap_after_create` は `WebAssembly.RuntimeError` を投げ、記録は空のまま。
  `scopes` を 10,000 回呼んだ後の `memory.buffer.byteLength` が 16 MiB 以下（`tests/host_imports.mjs` と同じ形）。
- A14 が done なら、`dyn` 値に入れた R6 の drop で `[6]` になる場合を足す。

### 既存テストへの影響

なし。`BUILTIN_CLASSES` の要素数が変わる。Drop のないプログラムの IR・診断は変わらない（手順 10）。

### 性能

Drop のないプログラムは IR が同じなので変化しない。Drop 型の drop は `store`・直接呼び出し・`load` が増えるだけで、`-O3` では slot が
消えることを手順 7 の IR で見る。時間の計測はしない。

## ドキュメント

- `docs/language.md`: 組み込みクラスの説明（`grep -n "Capture" docs/language.md` で見つかる箇所）に `Drop` と型規則 1〜6、所有権の drop の説明に
  drop の時点・順序、E1012 の 3 条件、trap で走らないこと。診断表の E1016・E1012 の説明に `Drop` を足す。
- `docs/architecture.md`: 「初版の次に必要な設計」から任意の destructor を外し、drop glue（利用者 `drop` → field）と `CheckedModule::user_drops`、
  反復 drop での呼び出し位置を書く。
- `_docs/language-reference/ownership.md`: 資源型の例（「例」の `scopes`）と E1012 の例。
- `_docs/language-reference/generics-and-typeclasses.md`: 組み込みクラスの一覧に `Drop`。
- `_docs/guides/from-fsharp.md`: `IDisposable`／`use` と lexical drop の対応。`use` 束縛はまだ E1018 であること。
- `README.md`: 検証コマンドの `node tests/host_imports.mjs target/release/tsuzuri` の次に `node tests/user_drop.mjs target/release/tsuzuri`。
- `_docs/feature-status.md` と `_features/README.md` の B07 の状態。

## 受け入れ条件

- [ ] D1・D2・D4 が承認済みで、Phase 1 だけが実装されている。
- [ ] Drop 型の値が、正常終了のすべての解放経路（E2E の表の全行）で、native と WASM、`-O0` と `-O3` のすべてでちょうど一回 drop される。
- [ ] 利用者 `drop` が field・payload より先に走り、field は宣言順に drop される。
- [ ] 100 万段の再帰 union の drop が成功し、利用者 `drop` が 1,000,001 回呼ばれる。
- [ ] 「診断」の表の全行を Rust テストで確かめている。
- [ ] Drop のないプログラムの IR が byte 単位で変わらない（手順 10）。WASM の既定 import は増えない。
- [ ] native の全 E2E で `live == 0`。
- [ ] GUIDE §10 の完了の定義を満たす。

## 落とし穴

- `needs_drop` が偽だった型（`i64` だけの record）が真になる。`needs_drop` で drop を省いている箇所（scope への登録、コレクションの要素の
  ループ）が自動で従うことを E2E の `scopes`・`array` で確かめる。`needs_drop` を見ずに型の形で drop を省く箇所が見つかったら直す。
- `ownership.rs` の `Checker::is_copy` は引数付きの record・union を field から判断するので、`Type::is_copy` だけを変えると generic な
  Drop 型が Copy に見える。`is_noncopy_record` を変えるのはこのためで、3 か所すべてがそれを先に見る。
- `Place::through_drop` を `Checker::place` だけで立てると、`match` の値束縛の place（`PAYLOAD` を積む箇所）が漏れ、payload の move が通る。
  `rejects_moving_fields_out_of_drop_types` の `match` の場合が落ちたらこれを疑う。
- drop glue の `spill` を `array_loop`・`list_loop` の本体で呼ぶ。alloca が入口にないとループのたびにスタックが伸びる。
- 反復 drop の helper は `module.functions[0]` を文脈に作られる。利用者関数の call に debug 位置が要る設定（debug 情報付きの build）で
  LLVM の検証が失敗したら、既存の helper の call と同じ扱いにする。直らなければ停止条件。
- 特殊化の走査を Drop instance がないときにも走らせると、コンパイル時間が増え、`user_drops` の順序次第で IR が揺れる。印がなければ走らない、
  集合は `BTreeSet`・`BTreeMap` で持つ。
- `parallel_results` は開始済みの環境を二重に drop しない不変条件を持つ。`tasks_parallel` の件数 16 が 17 以上になったらここを疑う。
- E2E の期待値をコンパイラの出力から写さない。`[2, 1]`（逆順）、`[1, 10, 11]`（利用者 drop が先）は仕様から決まる。
- `cargo test --locked user_drop` は 0 件でも成功する。`running N tests` を見る。

## 対象外

- trap 時の巻き戻しと drop、finalizer、GC との連携、drop 順序の利用者指定。
- `use` 束縛・計算式の `use`／`use!`、明示的な早期解放 API（Phase 2）。
- 組み込み型・std の型への `Drop`、関数値への Drop 型の捕捉（D5）。
- 非同期（B08）、共有所有（C10）、並行プリミティブ（F10）での資源の扱い。

## 決定事項

### D1: クラス `Drop` と対象型

- 決定: 組み込みクラス `Drop`（`BUILTIN_CLASSES` に追加）で、メソッドは `drop` だけ。instance はソースで宣言した record・union に、型全体を覆う
  head で、制約なしに書く。Drop 型は Copy でない。`deriving (Drop)` は既存の E1025。
- 理由: 型クラスの既存の仕組み（instance の収集、特殊化、E1016 の検査）をそのまま使え、新しい構文が要らない。型全体を覆う head に限ると
  drop の有無が型構成子ごとに決まり、`is_copy` を型引数から独立に決められる。
- 状態: 要承認（承認前は手順 2 以降に着手しない。GUIDE D-30 の仮割り当て）

### D2: `drop` の引数

- 決定: `drop :: ref mut 'a -> unit`。glue は呼び出し後に値を読み直し、書き換え後の field を drop する。
- 理由: 資源の後処理では field の更新（閉じた印、バッファの flush 位置）が要る。A14 D10 の drop slot もこの形を前提にする。起票時の既定案 `ref` の
  懸念（自身の置換による再帰）は、引数全体への代入を E1012 にすることで除いた（D4）。
- 状態: 要承認（承認前は手順 2 以降に着手しない）

### D3: drop glue の順序

- 決定: 利用者 `drop` を一回呼び、その後 field・payload を `drop_value` の既存順序（宣言順）で drop する。scope の中の順序は `drop_scope` のまま
  （束縛の逆順）。
- 理由: `drop` は完全な値を見られる。既存の順序を変えないので Drop のないプログラムの IR が同じになる。A14 の drop slot も同じ glue を通る。
- 状態: 既定案（実装者はこの案に従う）

### D4: Drop 型からの move と置換

- 決定: Drop 型から non-Copy の field・payload を move すること、Drop 型を基にした record 更新、`drop` 本体での引数全体への代入を E1012 で拒否する。
  Copy の field の読み出し、借用、値全体の move は許す。
- 理由: 部分 move を許すと `drop` が欠けた値を見る。record 更新は基を分解するので同じ問題になる。引数全体の代入は古い値の drop、つまり同じ
  `drop` を再び呼んで無限再帰になる。
- 状態: 要承認（承認前は手順 6 に着手しない）

### D5: 関数値と Task

- 決定: Drop 型を含む型は `Capture` を満たさない（関数値に捕捉すると既存の E1005）。`Send` は変えず、Task へは捕捉して送れる。
- 理由: 関数値は Copy で、複製は `@tz.env.clone.*` で捕捉値を複製する（「再現」で確認）。Drop 型を複製すると同じ資源を二度解放する。
  Task の環境は複製されない。調整役の方針（`can_capture`・`can_send` の既存の規則に任せる）をこの事実に当てはめた結果である。
- 状態: 既定案（実装者はこの案に従う）

### D6: 特殊化と `CheckedModule::user_drops`

- 決定: 通常の特殊化の後、特殊化済みの関数に現れる具体型から Drop 型を集め、`Classes::resolved_method` と `Specializer::request` で
  `drop` を特殊化する。固定点まで繰り返し、結果を `CheckedModule::user_drops`（`BTreeMap<Type, usize>`）に置く。
- 理由: LLVM の段では特殊化を要求できない。既に特殊化された関数の型だけを見るので、std の generic を先回りして特殊化せず、1,024 の予算を
  余計に使わない。Drop instance がなければ何もしないので IR も時間も変わらない。
- 状態: 既定案（実装者はこの案に従う）

### D7: 型性質の表し方

- 決定: `CheckedRecord::user_drop`・`CheckedUnion::user_drop` と `Type::has_user_drop`。Copy でないことは `Type::is_noncopy_record` に足して伝える。
- 理由: `is_noncopy_record` は Copy 判定の 3 か所すべてが最初に見る唯一の関数で、`Seq.Seq`・`Gpu.Device` と同じ扱いになる。`TypeContext` に
  field を足すと構築箇所がすべて変わる。
- 状態: 既定案（実装者はこの案に従う）

### D8: 再帰型

- 決定: 反復 drop の helper が node ごとに利用者 `drop` を呼び、その後で子を待ちリストへ積む。payload のない case の値にも呼ぶ。
- 理由: 再帰呼び出しをせず、100 万段でもスタックを消費しない。Drop 型のすべての値で一回という規則を表現によらず守る。
- 状態: 既定案（実装者はこの案に従う）

### D9: trap

- 決定: trap は巻き戻さず、未実行の `drop` は走らない。`drop` 本体の trap は通常の trap。
- 理由: trap を巻き戻さない既存の方針を変えない。巻き戻しは native と WASM の両方で大きな基盤を要する。
- 状態: 既定案（実装者はこの案に従う）

### D10: 直接呼び出しと早期解放

- 決定: `Drop.drop` の参照は E1016。`std/` に破棄の関数はないので、早期解放は値を消費する関数に渡すか内側の block で終わらせる。std の API は
  Phase 2 とする。
- 理由: 直接呼べると同じ値の二重 drop になる。E1016 はクラスの誤用の既存コードで、新しいコードが要らない。
- 状態: 既定案（実装者はこの案に従う）

### D11: E2E の観測方法

- 決定: fixture の `extern def drop_log :: i64 -> unit`（E06 のホスト関数。検査を通ることを確認済み）で native の C ハーネスと WASM の JS ハーネスが
  記録する。専用の `tests/user_drop.mjs` にし、`tests/features.mjs` の suite にはしない。
- 理由: 呼び出し順と回数をホストで独立に観測できる。import は fixture だけのもので、既定の WASM import は増えない（D-18）。
- 状態: 既定案（実装者はこの案に従う）

### D12: A14 との関係

- 決定: A14 の drop slot は `drop_value(T)` と同じ glue を呼ぶ（A14 D10）。B07 は A14 を開始条件にせず、A14 が done なら E2E に 1 件足す。
- 理由: glue を一つにすれば、どちらの順で入っても整合する。
- 状態: 既定案（実装者はこの案に従う）
