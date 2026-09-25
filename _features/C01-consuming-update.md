# C01: 所有権に基づく関数的更新（一意所有時は in-place）
| 項目 | 内容 |
|---|---|
| ID | C01 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | E02 |
| 後続 | C02 |
| 状態 | todo |
| 主な影響ファイル | `std/Array.tz` または `src/check.rs` `Builtin`、`src/llvm.rs` `emit_builtin` / `FunctionEmitter::{expression_mode, clone_value, drop_value, checked_element_pointer}`、`src/llvm_frame.rs` `relocate`、`src/ownership.rs`、`src/call_specialization.rs`、`docs/language.md`、`docs/architecture.md`、`README.md`、`tests/arrays.rs`、`tests/fixtures/arrays/ConsumingUpdate.tz` |

## 目的

配列・連結リストを不変値として保ったまま、`Array.set xs i x` のような「値を受け取り新しい値を返す」更新 API を追加する。非 Copy コレクションは move され、Copy 要素だけを持つ配列・リストは構造的に Copy なので、古い値が後で観測される場合は既存規則どおり深いコピーを作る。実装が既存バッファをその場で書き換えてよいのは、呼び出し側 lowering が「単一使用の所有ローカル」を clone せずに transfer した場合だけである。ユーザーには関数的な値更新だけを見せ、性能上は一意所有時に O(1) の更新を可能にする。

このチケットは D-13 に従う。コレクションは引き続き immutable value であり、要素代入構文・可変要素借用は導入しない。`let mut xs` や `&mut [T]` による全体置換の既存仕様も変えない。

## 現状

- `src/check.rs` の `Type::Array(Box<Type>)` と `Type::List(Box<Type>)` は要素型だけを持ち、LLVM では `src/llvm.rs` の `llvm_type` により `%tz.array = { ptr, i64 }`、`%tz.list = { ptr, i64 }` へ下がる。
- `src/check.rs` の `Type::is_copy` は配列・リストを「要素が Copy なら Copy」とみなすが、`Type::needs_drop` は全配列・全リストで true。`src/ownership.rs` `Checker::read_places` も同じ構造的 Copy 判定を使うため、Copy 要素の配列・リストは値渡し後も旧値を観測できる。LLVM では旧値が観測可能なら `src/llvm.rs` `FunctionEmitter::clone_value` がバッファを独立に複製する。
- `src/llvm.rs` `FunctionEmitter::read_place` は `take=true` で場所を読むと、非 Copy は move、Copy は通常 clone する。ただし `FunctionEmitter::clones_on_take` は `src/call_specialization.rs` `single_use_locals` が唯一使用と判断した所有ローカルでは Copy でも clone しない。
- スタック上のリテラルは `src/llvm_frame.rs` `Frame` と `FunctionEmitter::frame_value` で entry alloca に置かれる。束縛から値を移す場合、`src/llvm.rs` `FunctionEmitter::read_place` が `src/llvm_frame.rs` `relocate` を呼び、スタック領域を指す `%tz.array` / `%tz.list` をヒープへ移してから外へ渡す。
- 配列の確保・添字は `src/llvm.rs` `FunctionEmitter::allocate_array`、`element_pointer`、`checked_element_pointer` が担当する。`checked_element_pointer` は `icmp ult i64 index, length` を生成して `guard` し、その後でデータポインタを `extractvalue` し GEP する。
- 連結リストは `src/llvm.rs` `list_node_type`、`list_element_pointer`、`list_builder`、`append_list`、`list_loop` で生成・走査・解放される。
- 現在の `src/check.rs` `Builtin` は `Sqrt` など 10 個で、`Builtin::signature` が「単一引数」前提の形になっている。E02 で標準ライブラリ名前空間と複数引数・制約付き builtin 署名が入ることを前提にする。
- 配列・リスト要素への代入は `src/check.rs` `Checker::value_expression` の `ExprKind::Assign` と `src/ownership.rs` `Checker::access` が `E1012` / `E1014` で拒否する。

## 仕様

### 前提とする他チケットのインターフェース

- E02 は標準ライブラリ module 名を予約し、`Array.set` のような修飾名付き builtin を `Builtin` に登録できるようにする。
- E02 後の `Builtin` は次を表現できること。
  - 複数引数の curried signature。
  - `Copy<'a> => ...` のような制約付き signature。
  - 修飾名 `"Array.set"` / `"List.tail"` を `Checker::name` と `ExprKind::Field` の module 解決から参照する仕組み。
- E02 はソース定義の `Array.set` があればそれを優先し、なければ builtin を解決する D-07 の規則を提供する。

### 他チケットへの提供インターフェース

- C02 は `Vec.set` / `Vec.push` で同じ「値渡し API だが、最後の使用なら storage transfer で O(1) 再利用できる」説明と検証方針を使う。
- C04 は `Array.append` / `Array.reverse` などで C01 の所有バッファ再利用条件を参照できる。ただし C04 の読み取り API は C03 の `&[T]` を優先する。

### API

標準ライブラリの利用者向け API は次の通り。`'a` は単相化後の具体型であり、配列長は型に含まない。

```text
Array.set    : ['a] -> i64 -> 'a -> ['a]
Array.update : ['a] -> i64 -> ('a -> 'a) -> ['a]
Array.swap   : ['a] -> i64 -> i64 -> ['a]

List.cons    : 'a -> [|'a|] -> [|'a|]
List.tail    : [|'a|] -> [|'a|]
```

- `Array.set xs i value` は `xs` を値渡しで受け取り、`i` の要素を `value` に置換した配列を返す。`xs` が非 Copy なら move、Copy なら通常の Copy 値渡しである。
- `Array.update xs i f` は `xs[i]` の旧要素を消費して `f old` を評価し、その結果を同じ位置に格納して返す。
- `Array.swap xs i j` は `xs[i]` と `xs[j]` を入れ替えた配列を返す。同じ添字なら旧要素の drop / clone を行わず `xs` を返す。
- `List.cons x xs` は `x` を先頭に持つリストを返す。`xs` が storage transfer された場合だけ既存ノード列を共有してよい。Copy で旧 `xs` が観測可能な場合は呼び出し側 clone 済みのノード列に対して処理する。
- `List.tail xs` は空でなければ先頭要素と先頭ノードだけを解放し、残りのノード列を所有するリストを返す。空ならトラップする。Copy list の旧値が観測可能なら、呼び出し側 clone 済みのリストに対して tail を取る。
- `List.set` / `List.update` はこのチケットでは提供しない。O(n) コピー API は C04 の `List` 一括操作で扱う。

### 構文

新しい構文はない。呼び出しは通常の curried function application。

```ebnf
array_update_call ::= "Array.set" expression expression expression
                    | "Array.update" expression expression expression
                    | "Array.swap" expression expression expression
list_update_call  ::= "List.cons" expression expression
                    | "List.tail" expression
```

### 型規則

- `Array.set` の第 1 引数は所有配列 `['a]`。`&['a]` ではない。
- `Array.set` の第 3 引数は配列要素型と同じ `'a`。暗黙変換はない。
- `Array.update` の関数引数は `('a -> 'a)`。旧要素を値として渡すため、`'a` が非 Copy でも受理する。
- `Array.swap` は要素型に制約を要求しない。ビット単位の一時退避と store で足り、clone は不要。
- `List.cons` / `List.tail` も要素型に Copy を要求しない。
- API は `export def` の公開 ABI には使えない配列・リストを含むため、ホスト境界には出さない。
- `Array.set (&xs) ...` は `&['a]` を `['a]` に渡すため `E1003`。`Array.set xs ...` 後に `xs` を使う場合、`xs` が非 Copy コレクションなら既存所有権規則で `E1012`、Copy コレクションなら旧値は意味論上コピーとして残る。

### 評価順序とトラップ

すべて通常の関数呼び出しとして、callee、引数を左から右に評価する。

1. `Array.set xs i value`: `Array.set` → `xs` → `i` → `value` を評価する。
2. すべての引数評価が終わってから、`0 <= i < xs.length` を検査する。
3. 検査成功後だけ GEP / load / store を行う。
4. 検査失敗は `llvm.trap`。トラップ時の巻き戻し・drop は従来通り保証しない。

`Array.update xs i f` は `f` の関数値を境界検査前に評価するが、`f old` の適用は境界検査後にだけ行う。旧要素を load する前に必ず境界検査する。

`Array.swap xs i j` は `xs`、`i`、`j` を評価し、`i` の境界検査、`j` の境界検査の順に行う。どちらかが失敗したら要素 load は行わない。

`List.tail xs` は `xs.length > 0` を検査し、成功後に head node を読む。空リストの head ポインタに対する load / GEP は生成しない。

### 所有権・借用・性能契約

- API は値を値渡しで受け取り、新しい所有値を返す。非 Copy コレクションでは move、Copy コレクションでは通常の Copy 値渡しである。
- 真の in-place 書き換え（既存バッファの storage transfer）が観測不能な条件:
  - 実引数が場所にある所有ローカルで、`src/call_specialization.rs` `single_use_locals` が単一使用と判定している。
  - そのローカルが `src/llvm.rs` `FunctionEmitter.borrowed_locals` に含まれていない。
  - `src/llvm.rs` `FunctionEmitter::clones_on_take` が false になり、`FunctionEmitter::read_place(..., take=true, relocate=true)` が clone せずに読み出す。
  - スタック literal 由来の data / node を指しうる場合は、書き換え前に `src/llvm_frame.rs` `relocate` でヒープ storage へ移している。
- O(1) になる条件:
  - 配列・リストの descriptor が既にヒープ領域を所有している。
  - 上記 storage transfer 条件をすべて満たす。
  - `Array.update` の `f` が既知 worker かどうかは in-place の条件ではない。関数呼び出しコストだけに影響する。
- O(n) fallback:
  - Copy 配列・リストの旧値が後で観測可能、または共有借用が生存しているため、呼び出し側が `clone_value` で独立バッファを作ってから builtin に渡す場合。この場合 `Array.set` しても旧 `xs` や `&xs[0]` は元の値を読む。
  - `let xs = [1,2,3]; Array.set xs 0 9` のようにスタック literal をスコープ外へ move する場合、`src/llvm_frame.rs` `relocate` がヒープコピーを作る。このあと builtin 内部では追加の全体コピーをしない。
- 借用中の非 Copy 配列・リストを move する呼び出しは既存 `E1014` のまま拒否する。Copy 配列・リストの共有借用が生存している場合は、呼び出し側が clone するため受理され、旧値は preserved される。
- 既存の `xs[i] = value`、`&mut xs[i]` は引き続き拒否する。

## 設計

### データ構造

E02 後の `src/check.rs` `Builtin` に次を追加する。

```rust
pub enum Builtin {
    // existing ...
    ArraySet,
    ArrayUpdate,
    ArraySwap,
    ListCons,
    ListTail,
}
```

E02 の複数引数署名 API に合わせ、概念上は次を返す。

```rust
BuiltinScheme {
    signature: Signature {
        parameters: vec![
            Type::Array(Box::new(Type::Variable("a".into()))),
            Type::I64,
            Type::Variable("a".into()),
        ],
        result: Type::Array(Box::new(Type::Variable("a".into()))),
    },
    variables: vec!["a".into()],
    constraints: vec![],
}
```

`Array.update` は第 3 引数を `Type::function(vec![a.clone()], a.clone())` にする。`Array.swap` は `['a], i64, i64 -> ['a]`。

`List.cons` は `a, [|a|] -> [|a|]`、`List.tail` は `[|a|] -> [|a|]`。

AST / typed IR に新しい式 variant は不要。E02 の canonical path に従い、通常の `TypedExprKind::Call(Function(FunctionRef::Builtin(BuiltinInstance { ... })), args)` として扱う。

### 型検査

- `src/check.rs` `Builtin::ALL`、`Builtin::name`、E02 後の署名関数へ追加。
- `Checker::name` / `ExprKind::Field` の修飾 builtin 解決は E02 の規則に従う。`Array` / `List` という user module と衝突した場合は E02 の予約 module 診断 `E1011`。
- 型不一致は既存 `E1003`、引数数不一致は `E1006`、未知 module/function は `E1002`。
- 可変参照格納禁止や配列要素 move 禁止は既存と同じ。

### 所有権検査

変更なしを原則とする。理由:

- `Array.set xs i v` は通常の関数呼び出しなので、`src/ownership.rs` `Checker::eval_composed` の `E::Call` が callee と引数を左から右に consume する。
- `src/ownership.rs` `Checker::read_places` は `Checker::is_copy` を見て、非 Copy コレクションだけを move として扱う。Copy コレクションは `Use::Read` 相当になり、後続使用や共有借用と両立する。
- 非 Copy の `xs` を借用中に consume すれば `Checker::access` が `E1014`。
- 非 Copy の `xs` を呼び出し後に使えば `E1012`。Copy の `xs` は旧値が残るため拒否しない。

ただしテストで次を確認する。

- Copy array の唯一使用では IR に `clone_value` 相当の `@tz.alloc` が余分に出ない。
- Copy array の複数使用では更新前に clone が入り、元配列が保持される。

### LLVM lowering

E02 の builtin lowering をそのまま使う。`FunctionRef::Builtin` は `BuiltinInstance` を保持し、単相化後の具体型を失わない。`FunctionEmitter::call` は builtin instance を `builtins: BTreeSet<BuiltinInstance>` に登録し、`@tz.builtin.Array.set.<mangled-types>` のような決定的な内部シンボルを呼ぶ。`emit_builtin(instance, module, intrinsics, ...)` は concrete 型と `CheckedModule` を受け取り、既存の `FunctionEmitter` / builtin emitter helper（`ty`、`value`、`guard`、`drop_value`、`clone_value`、`allocate_array`、`element_pointer`、`checked_element_pointer`、`list_*`、closure application helper）を使って本体 IR を生成する。

禁止する実装:

- `$builtin.Array.set.<id>` のような synthetic `CheckedFunction` を作り、通常関数 body hook で特別扱いする方式。
- `Builtin::signature()` の単一引数前提や `FunctionRef::Builtin(Builtin)` を残したまま、型を文字列から推測して IR を生成する方式。

`Array.set` / `Array.update` / `Array.swap` は partial application と関数値化が通常 builtin と同じに動く必要がある。たとえば `let set_at_zero = Array.set xs 0` は `xs` と `0` を捕捉した関数値を作り、最後の引数を受け取った段階で builtin instance を呼ぶ。

生成 IR 形状（`Array.set`、要素型 `T`）:

```llvm
define internal %tz.array @tz.builtin.Array.set.<T>(%tz.array %xs, i64 %index, T %new) nounwind {
entry:
  %len = extractvalue %tz.array %xs, 1
  %ok = icmp ult i64 %index, %len
  br i1 %ok, label %in_bounds, label %trap
trap:
  call void @llvm.trap()
  unreachable
in_bounds:
  %data = extractvalue %tz.array %xs, 0
  %slot = getelementptr inbounds T, ptr %data, i64 %index
  %old = load T, ptr %slot
  ; if T.needs_drop(records): drop_value(T, %old)
  store T %new, ptr %slot
  ret %tz.array %xs
}
```

`Array.update`:

- 境界検査後に旧要素を load。
- `old` を `f` に値渡しで適用する。`f` が既知関数でも未知 closure でも、通常の closure application path（`FunctionEmitter::apply_value` 相当、または E02 後の builtin emitter に公開された同等 helper）を使い、評価順序・捕捉環境・drop を通常呼び出しと揃える。
- 結果を同じ slot に store。
- 旧要素の drop は呼び出し先 `f` の所有権に任せるため、store 前に `drop_value(old)` しない。
- `f` 自体は builtin が所有する引数なので、適用後に関数値の環境を `drop_value(&f.ty, f_value)` で解放する。既知 worker への特殊化を使う場合も、所有権上必要な cleanup を省かない。

`Array.swap`:

```llvm
%same = icmp eq i64 %i, %j
br i1 %same, label %return, label %check_j
```

同じ添字なら要素 load/store を行わず `%xs` を返す。異なる場合は両 slot を境界検査後に load し、相互 store する。要素の clone/drop は不要。

`List.cons`:

- 新しい 1 node を `@tz.alloc` で確保。
- node.next に旧 head、node.value に `x` を store。
- 返す `%tz.list` は `{ node, old_len + 1 }`。
- `old_len == i64::MAX` は `add` overflow になるため、`old_len != i64::MAX` を guard してから `add i64 old_len, 1`。

`List.tail`:

- `%len > 0` を guard。
- `%head = extractvalue %tz.list xs, 0`。
- `%next = load ptr, ptr %head` を先に読む。
- head element を `drop_value`。
- `call void @tz.free(ptr %head)`。
- 返す `%tz.list { next, len - 1 }`。

### 決定性

- 具体型ごとの builtin wrapper 名は `Builtin` 名 + mangle した `Type::display` ではなく、単相化済み関数 ID または `BTreeMap<(Builtin, TypeVec), usize>` の決定的 ID にする。
- 追加する intrinsic はない。既存 `intrinsics: BTreeSet<String>` の決定性に影響しない。

## 実装手順

1. **E02 前提確認**
   - `_features/E02-standard-library-infrastructure.md` の実装が完了し、修飾 builtin・複数引数 signature が使えることを確認する。
   - 確認: `Array.length` など E02 の代表 builtin が `analyze_modules` と driver の両方で解決できる。

2. **Builtin 定義追加**
   - `src/check.rs` `Builtin` に 5 variant を追加する。
   - `Builtin::ALL`、`Builtin::name`、E02 後の `BuiltinScheme` / `BuiltinInstance` table を更新する。
   - 確認: `tests/consuming_update.rs` に `Array.set [1] 0 2` が型検査される受理テストを追加。

3. **Array 専用 builtin lowering**
   - `emit_builtin(instance, module, intrinsics, ...)` の per-instance 分岐として `emit_array_set`、`emit_array_update`、`emit_array_swap` を追加する。
   - `FunctionRef::Builtin(BuiltinInstance)`、partial application、function value 化は E02 の通常経路だけを使う。
   - `drop_value` / `clone_value` を再利用し、要素型ごとの所有値処理を重複実装しない。
   - 確認: Rust test で IR が `icmp ult` より前に `getelementptr` を含まないこと、`Array.set` が `%tz.array` を返すことを文字列検査する。

4. **List 専用 builtin lowering**
   - `emit_builtin(instance, module, intrinsics, ...)` の per-instance 分岐として `emit_list_cons`、`emit_list_tail` を追加し、`list_node_type` / `list_element_pointer` を使う。
   - `List.tail` は link を読んでから head node を free する。`drop_value` の list loop と同じ理由で順序を守る。
   - 確認: IR に `load ptr, ptr %head` が `@tz.free(ptr %head)` より前にある。

5. **所有権最適化の確認**
   - コード変更は最小にし、`FunctionEmitter::read_place` / `clones_on_take` / `src/call_specialization.rs` `single_use_locals` の既存経路を使う。
   - 必要なら builtin call が通常 call と同じ argument evaluation path を通るようにする。
   - 確認: `let xs = new [i64](3, i -> i); Array.set xs 1 9` で更新用の配列全体 clone が発生しない。

6. **ドキュメントと fixture**
   - `tests/fixtures/arrays/ConsumingUpdate.tz` を追加し、native/WASM E2E で境界・所有値・drop を確認する。
   - README / docs を更新する。
   - 確認: 指定テストと E2E が通る。

## テスト計画

### Rust unit tests: `tests/consuming_update.rs`

受理:

- `def f :: [i64]\nfn f = Array.set [1,2,3] 1 9`
- `def f :: i64\nfn f = (Array.set [1,2,3] 1 9)[1]`
- `def f :: [string]\nfn f = { let xs = new [string](2, i -> "a"); Array.set xs 0 "b" }`
- `def f :: [string]\nfn f = Array.update (new [string](2, i -> "a")) 1 (s -> s + "x")`
- `def f :: [i64]\nfn f = Array.swap [1,2,3] 0 2`
- `def f :: [|i64|]\nfn f = List.cons 1 [|2,3|]`
- `def f :: [|string|]\nfn f = List.tail (List.cons "x" (new [|string|](2, i -> "y")))`
- Copy array の複数使用: `let xs = [1,2]; let ys = Array.set xs 0 9; xs[0] + ys[0]`
- Copy array の共有借用 preserved: `def f :: i64\nfn f = { let xs = [1]; let r = &xs[0]; let ys = Array.set xs 0 2; *r + ys[0] }` は 3。`r` が旧 `xs` を読むため、呼び出し側は更新対象を clone する。
- Partial application / function value: `let set_at_zero = Array.set xs 0; set_at_zero 9`、`let tail = List.tail; tail list`。

拒否:

- `Array.set (&[1,2]) 0 9` → `E1003`
- `Array.set [1,2] true 9` → `E1003`
- `Array.set [1,2] 0 true` → `E1003`
- `let xs = new [string](2, i -> "x"); let ys = Array.set xs 0 "y"; xs.length` → `E1012`
- `let xs = new [string](1, i -> "x"); let r = &xs[0]; let ys = Array.set xs 0 "y"; r.length` → `E1014`
- `List.tail [||]` はコンパイル受理、実行時 trap。コンパイル拒否にしない。

IR 検査:

- `Array.set` wrapper body で `icmp ult i64` と `llvm.trap` が `getelementptr` より前。
- `Array.swap` 同一添字 case に要素 `load` がない。
- Copy かつ唯一使用の `new [i64]` を `Array.set` に渡した場合、更新前の `clone_value` 用余分な `@tz.alloc` がない。
- Copy だが共有借用または後続使用がある `[i64]` を `Array.set` に渡した場合、更新対象用の clone が入り、旧配列から作った参照は元の値を読む。
- スタック literal を返す更新では `src/llvm_frame.rs` `relocate` 由来の `@tz.alloc` が 1 回あり、builtin 内の全体 clone はない。

検証コマンドは GUIDE §3 に従い、Rust は `cargo test --locked --test consuming_update` のように `--test <file>` を使って `running N tests` の `N > 0` を確認する。Node E2E の直前には必ず `cargo build --release --locked` を実行し、`target/release/tsuzuri` を更新してから `node tests/*.mjs target/release/tsuzuri` を走らせる。

### E2E fixture: `tests/fixtures/arrays/ConsumingUpdate.tz`

公開関数と独立期待値:

- `array_set_i64(n, i, value)` は JS BigInt で `sum([0..n-1] with i=value)` を計算。
- `array_update_string()` は `"a"`, `"b"` の長さ合計を整数で返す。期待値は JS 文字列長から計算。
- `array_swap(n, i, j)` は JS 配列で swap した checksum。
- `list_cons_tail(n)` は `n + (n-1) + ...` を BigInt で計算。
- `drop_old_elements(count)` は `string` 要素の置換を大量に行い、native heap tracking で `live == 0`。
- `bounds_set(n, index)`、`bounds_swap(n, i, j)`、`tail_empty()` は子プロセスで trap を確認。

ターゲット:

- native `-O0` / `-O3`
- WASM `-O0` / `-O3`
- WASM imports empty、memory 16 MiB 以下。
- native heap tracking で各 exported call 後 `live == 0`。

### 性能確認

- `benchmarks/` に速度閾値は入れない。
- `target/release/tsuzuri build /tmp/... --emit llvm -O3` と `clang -O3 -S -emit-llvm` で `Array.set` が全体 copy loop になっていないことを確認する。
- 比較は `Array.set` vs 手書き「new array + copy + replace」を同じ要素数で測り、`docs/benchmarks.md` に環境・中央値・生データを任意記録する。CI 合否にはしない。

## ドキュメント

- `docs/language.md`
  - 「配列・連結リストとレコード」に `Array.set` / `Array.update` / `Array.swap` と `List.cons` / `List.tail` を追加。
  - 「Ownership / Borrowing」に「非 Copy コレクションでは更新 API への値渡し後に元値を使えないが、Copy コレクションでは旧値が preserved される」例を追加。
  - トラップ順序: 引数評価後、境界検査後に要素アクセス。
- `docs/architecture.md`
  - 「コレクションの不変性」に in-place が観測不能な最適化である条件を追記。
  - 「LLVM」に `checked_element_pointer` と同じ bounds-before-GEP を維持することを追記。
- `README.md`
  - 配列不変性の節に関数的更新の短い例を追加。
- `docs/benchmarks.md`
  - 測定した場合だけ結果を追記。未測定なら計画として書かない。

## 受け入れ条件

- [ ] `Array.set` / `Array.update` / `Array.swap` / `List.cons` / `List.tail` が修飾名で解決される。
- [ ] すべての API が値渡しで動作し、非 Copy コレクションの move 後使用は `E1012`、非 Copy コレクションの借用競合は `E1014`、Copy コレクションの旧値観測は clone により preserved。
- [ ] 境界検査または空リスト検査が GEP / load より前にある。
- [ ] 旧要素の drop が `Array.set` と `List.tail` で漏れない。
- [ ] `Array.update` は旧要素を `f` に move し、二重 drop しない。
- [ ] Copy array の複数使用では元配列が変わらない。
- [ ] Copy array の共有借用が生存する場合も受理され、借用は旧値を読む。
- [ ] 唯一所有の heap array では全体 clone なしに同じバッファを返す。
- [ ] native/WASM × `-O0`/`-O3` E2E が通り、WASM imports は空、native heap tracking は `live == 0`。
- [ ] 生成 IR が決定的で、追加 wrapper 名順が `BTreeMap` / `BTreeSet` 由来。
- [ ] README / docs を更新済み。

## 落とし穴

- `Array.set` の「すべての引数評価後に境界検査」を崩さない。`value` の評価を bounds check 後へ動かすと副作用と trap 順序が変わる。
- `Array.update` で旧要素を `drop_value` してから `f old` に渡すと use-after-free / 二重解放になる。
- `Array.set` で新値を store してから旧値を drop すると、旧値が新値と同じ所有バッファを含む場合に壊れる。必ず旧値を load して drop してから store する。
- `List.tail` は head node の next を free 前に読む。`drop_value` の list loop と同じ理由。
- `@tz.free(null)` は native `free(null)` で安全だが、空でない検査なしに null head を load してはいけない。
- Stack literal の O(n) `relocate` は仕様上必要。これを「in-place でない」と誤って最適化しない。
- `single_use_locals` はループ内使用を複数使用扱いにする。ループ内の一回使用を O(1) 更新の根拠にしない。
- Copy 配列・リストを「値渡しでも必ず move」と説明しない。`Type::is_copy` と `ownership.rs` `read_places` により、旧値が観測可能なら意味論上 copy である。
- builtin を synthetic `CheckedFunction` にしない。E02 の `BuiltinInstance` を通さないと concrete 型、partial application、到達性 pruning が壊れる。

## 対象外

- 配列・リスト要素代入構文 `xs[i] = v`。
- `&mut xs[i]`。
- `Array.insert` / `Array.remove` / `Array.resize`。伸縮は C02 `Vec`。
- O(n) の `List.set` / `List.update`。C04 で別途検討する。
- 並列更新、SIMD 更新、GPU 更新。
- トラップ時の巻き戻し・destructor 保証。

## 未決事項

- `Array.update` の関数引数を `('a -> 'a)` にする既定案を採用する。`&'a -> 'a` だと非 Copy 要素の置換で旧要素の所有権が曖昧になる。
- `List.cons` の引数順は `value -> list -> list` を既定案にする。パイプライン重視の `list -> value -> list` は `List.cons_to` など別 API が必要になった時に検討する。
- `Array.set` などを std Tsuzuri ソースでラップするか、すべて builtin にするかは E02 の実装形に合わせる。既定案は「公開名は std module、実体は compiler builtin」。
- 台帳の見直し提案: なし。D-13 と整合する。
