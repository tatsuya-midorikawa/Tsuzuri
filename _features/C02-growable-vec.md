# C02: 伸縮可能な配列 Vec
| 項目 | 内容 |
|---|---|
| ID | C02 |
| 優先度 | P1 |
| 規模 | L |
| 依存 | E02, C01, B01 |
| 後続 | C06, D02 |
| 状態 | todo |
| 主な影響ファイル | `std/Vec.tz` または `src/check.rs` `Type` / `Builtin`、`src/parser.rs` / `src/polymorph.rs` 型適用、`src/llvm.rs` `llvm_type` / `drop_value` / `clone_value` / `expression_mode` / `emit_target`、`src/llvm_frame.rs`、`src/ownership.rs`、`src/call_specialization.rs`、`src/runtime/heap-native.ll`、`src/runtime/heap-wasm.ll`、`docs/language.md`、`docs/architecture.md`、`README.md`、`tests/vec.rs`、`tests/fixtures/vec/Vec.tz` |

## 目的

固定長の不変配列 `[T]` に加えて、構築時に効率よく伸ばせる所有コレクション `Vec 'a` を導入する。`Vec` は「伸縮可能な作業バッファ」であり、最終的な不変配列へ `Vec.to_array` で O(1) に変換できる。C01 の消費する更新と同じく、API は所有値を受け取り新しい所有値を返し、一意所有時に内部バッファを再利用する。

D-13 に従い、`[T]` の `%tz.array = { ptr, i64 }` は変更しない。`Vec` は別の組み込み型であり、暗黙に `[T]` へ変換しない。

## 現状

- `src/check.rs` `Type` に `Array` と `List` はあるが `Vec` はない。
- 型適用は D-02/A01 で `TypeExprKind::Apply` を導入予定。現状 `Vec i64` のようなユーザー定義型適用は未実装。
- LLVM の配列 descriptor は `src/llvm.rs` `emit_target` 先頭で `%tz.array = type { ptr, i64 }` として出力される。
- 配列 clone/drop は `src/llvm.rs` `FunctionEmitter::clone_value` / `drop_value` が要素ごとに処理する。
- 確保は `src/runtime/heap-native.ll` の `@tz.alloc` / `@tz.free` と、`src/runtime/heap-wasm.ll` の 16 MiB 上限 free-list allocator。
- WASM allocator はアドレス順の free list と隣接結合を持つが、現状 `@tz.realloc` はない。
- `for...in` は `src/control.rs` `Checker::control_expression` が配列・リスト・string・整数範囲だけを受理し、`src/llvm_control.rs` `FunctionEmitter::for_each` が `%tz.array` / `%tz.list` を走査する。

## 仕様

### 前提とする他チケットのインターフェース

- E02: `std/Vec` module の同梱、`Vec.empty` など修飾 builtin の解決、複数引数 signature。
- C01: 所有値消費 API の O(1) / O(n) fallback の説明と test helper。
- B01: `Option 'a`。`Vec.pop` と `Vec.get` の戻り値に使う。`Some` / `None` ケース名は D-08 準拠。
- A01/A02 が未完了でも、組み込み型 `Vec 'a` は `Type::Vec(Box<Type>)` として先行実装してよい。ただし一般の型適用構文がない場合は C02 内で `Vec` だけを `resolve_type` に特例追加し、A01 後に `TypeExprKind::Apply` へ移行する。

### 他チケットへの提供インターフェース

- C06 は `Map.to_array` / `Set.to_array` の構築バッファとして `Vec` を使える。
- D02 は `Vec ubyte` と `String.from_bytes : Vec ubyte -> string` を文字列構築の推奨経路にできる。
- F02 は `Vec.to_array` 後の `[T]` / `&[T]` を並列 API 入力にする。

### 型と表現

Surface type:

```text
Vec 'a
```

内部型:

```rust
pub enum Type {
    // existing ...
    Vec(Box<Type>),
}
```

LLVM:

```llvm
%tz.vec = type { ptr, i64, i64 } ; data, len, cap
```

意味:

- `ptr` は `cap` 個分の要素領域。`cap == 0` なら `ptr == null`。
- `len` は初期化済み要素数。
- `cap` は確保済み要素数。`0 <= len <= cap`。
- `len..cap` の領域は未初期化。drop / clone / to_array は `len` 個だけを見る。
- `Vec 'a` は常に non-Copy。要素が Copy でも descriptor の共有を避けるため暗黙 Copy しない。
- 明示 clone は `Vec.clone : Copy 'a => &Vec 'a -> Vec 'a` として提供する。`string` など非 Copy 要素の複製 API は将来の `Clone` クラスまで対象外。

`Type` の性質:

```rust
Type::Vec(element):
    is_copy = false
    needs_drop = true
    contains_reference = element.contains_reference()
    contains_mutable_reference = element.contains_mutable_reference()
    carries_loans = element.carries_loans(records)
    can_capture = element.can_capture(records)
    can_send = element.can_send(records)
    exportable = false
```

`layout_size(Type::Vec)` と `validate_size(Type::Vec)` は保守的に 32 bytes とする。LLVM 実体は 24 bytes だが、集約フィールドでは 16 byte 境界に切り上げる既存規則に合わせる。

### API

```text
Vec.empty         : Vec 'a
Vec.with_capacity : i64 -> Vec 'a
Vec.length        : &Vec 'a -> i64
Vec.capacity      : &Vec 'a -> i64
Vec.is_empty      : &Vec 'a -> bool

Vec.push          : Vec 'a -> 'a -> Vec 'a
Vec.pop           : Vec 'a -> (Vec 'a * Option 'a)
Vec.reserve       : Vec 'a -> i64 -> Vec 'a
Vec.truncate      : Vec 'a -> i64 -> Vec 'a
Vec.clear         : Vec 'a -> Vec 'a

Vec.set           : Vec 'a -> i64 -> 'a -> Vec 'a
Vec.swap          : Vec 'a -> i64 -> i64 -> Vec 'a
Vec.get           : Copy 'a => &Vec 'a -> i64 -> Option 'a
Vec.at            : &Vec 'a -> i64 -> &'a
Vec.clone         : Copy 'a => &Vec 'a -> Vec 'a

Vec.of_array      : ['a] -> Vec 'a
Vec.to_array      : Vec 'a -> ['a]
```

補足:

- `Vec.empty` は引数なし関数。呼び出しは既存の引数なし関数と同じ `Vec.empty()`。
- `Vec.with_capacity n` は `n < 0` または要素サイズ積 overflow で trap。
- `Vec.reserve v additional` は `additional < 0`、`len + additional` overflow、byte size overflow で trap。
- `Vec.truncate v new_len` は `new_len < 0` で trap。`new_len >= len` なら何もしない。短くする場合は削除要素を添字昇順で drop する。
- `Vec.clear v` は `Vec.truncate v 0`。
- `Vec.pop v` は空なら `(v, None)`、非空なら最後の要素を move して `(shorter, Some value)`。
- `Vec.get` は範囲外なら `None`、範囲内なら Copy 要素を複製して `Some value`。
- `Vec.at` は範囲外なら trap。戻り値は `&'a` への共有借用で、寿命は `&Vec 'a` 入力に結び付く。
- `Vec.of_array` は配列の所有バッファを O(1) で `Vec { ptr, len, cap=len }` にする。ただし `len == 0` の runtime array は `allocate_array` により non-null 1-byte allocation を持ちうるため、`Vec` の invariant `cap == 0 => ptr == null` を優先し、incoming pointer が non-null なら `@tz.free(ptr)` して `%tz.vec zeroinitializer` を返す。
- `Vec.to_array` は O(1) で `Array { ptr, len }` を返し、`cap` は捨てる。未初期化領域 `len..cap` は drop しない。free は元ポインタに対して行われるため安全。

### 構文

Phase 1 では新しいリテラル構文はない。`Vec` は関数で作る。

```ebnf
vec_type       ::= "Vec" type_atom
vec_expression ::= "Vec.empty" "(" ")"
                 | "Vec.with_capacity" expression
                 | "Vec.push" expression expression
                 | "Vec.to_array" expression
```

Indexing:

```text
v[i]
&v[i]
```

- `v[i]` は `Vec.at (&v) i` を dereference した値として扱う。値として使う場合、要素型に Copy が必要。非 Copy 要素の move は `E1012`。
- `&v[i]` は要素への共有借用。`&mut v[i]` は `E1014`。
- `for x in v do` / `for x in &v do` を受理し、配列と同じく全体を読み取り借用して順序走査する。

### 評価順序と traps

- すべての API は通常の関数呼び出し順: callee → 引数左から右。
- `Vec.push v value`: `v` と `value` を評価後、必要なら capacity grow を行い、その後 `value` を `data[len]` へ store する。
- `Vec.set v i value`: `v`、`i`、`value` を評価後、bounds check、旧要素 drop、store。
- `Vec.pop v`: `v` 評価後、`len == 0` を分岐。非空の場合だけ `data[len-1]` を load。
- `Vec.to_array v`: `v` 評価後は trap しない。
- allocation failure、WASM 16 MiB 上限超過、capacity overflow は `llvm.trap`。

### 成長ポリシー

- `required = len + additional`。
- `required` の計算は `additional < 0` を先に trap し、`len + additional` を `llvm.uadd.with.overflow.i64` または `additional > i64::MAX - len` で検査する。overflow したら trap。
- `cap >= required` なら realloc しない。
- `cap == 0` なら `new_cap = max(required, 4)`。
- それ以外は `new_cap = cap` から `new_cap *= 2` を繰り返し、`new_cap >= required` にする。
- doubling の各反復で `new_cap > i64::MAX / 2` なら trap。`required` へ直接ジャンプして overflow check を迂回してはいけない。実装は `llvm.umul.with.overflow.i64(new_cap, 2)` を使ってもよい。
- old/new byte count は手書き乗算ではなく、既存 `src/llvm.rs` `FunctionEmitter::allocation_size` と同じ guard を通す。`new_cap * element_size` や `old_cap * element_size` が `i64::MAX` を超える場合は trap。
- `element_size == 0` は既存 `allocation_size` と同じく stride 1 として扱う。`Vec<unit>` でも capacity 分 1 byte stride の領域を確保してポインタ一貫性を保つ。
- `reserve` は shrink しない。shrink-to-fit は対象外。

## 設計

### 型検査・型適用

`src/check.rs`:

- `Type::Vec(Box<Type>)` を追加。
- `Type::display` は `Vec T`。関数型など曖昧な型は `Vec (T -> U)` のように括る。
- `resolve_type` は D-02 が入っていれば `TypeExprKind::Apply("Vec", [element])` を `Type::Vec` へ解決する。D-02 前なら `TypeExprKind::Named("Vec")` の後続型 atom を読む暫定 parser を C02 内に置かず、C02 は A01/E02 完了後に着手する既定案。
- `polymorph.rs` `map_type`、`substitute`、`bounded_type`、`Inference::resolve`、`Inference::unify`、`type_expression` に `Vec` を追加。
- `Classes::intrinsic` は `Copy` では常に false、`Capture` / `Send` は element 再帰。

### Builtin

`src/check.rs` `Builtin` に `VecEmpty` などを追加する。E02 後は以下を data-driven に持つ。

```rust
pub enum Builtin {
    VecEmpty,
    VecWithCapacity,
    VecLength,
    VecCapacity,
    VecIsEmpty,
    VecPush,
    VecPop,
    VecReserve,
    VecTruncate,
    VecClear,
    VecSet,
    VecSwap,
    VecGet,
    VecAt,
    VecClone,
    VecOfArray,
    VecToArray,
}
```

`Vec.pop` の型は B01 の `Option` union と `Tuple` を使う。

### LLVM 型と header

`src/llvm.rs`:

```rust
Type::Vec(_) => "%tz.vec".into()
```

`emit_target` header:

```llvm
%tz.vec = type { ptr, i64, i64 }
```

`abi_type` / `c_type` は unreachable のまま。`Type::exportable` は false。

### drop / clone

`drop_value(Type::Vec(element), value)`:

1. `%data = extractvalue %tz.vec value, 0`
2. `%len = extractvalue %tz.vec value, 1`
3. `for index in 0..len`: if `element.needs_drop`, load and drop element.
4. `call void @tz.free(ptr %data)`

`clone_value(Type::Vec, ...)` は通常の暗黙 Copy では呼ばれない設計にする。関数環境の clone 等で到達しうるため、`Vec` を捕捉可能にするなら明示 clone ではなく `clone_value` も正しく実装する必要がある。

既定案:

- `Vec` は `can_capture` が element に従って true なので関数環境に捕捉できる。
- 関数値 clone では `clone_value(Type::Vec)` を呼ぶため、ここでは要素が Copy かどうかに関係なく既存 `clone_value(element)` で deep clone する。これは関数値の独立 snapshot を守る内部複製であり、ユーザーの Copy とは別。
- `Vec.clone` API は `Copy 'a` に限定する。

### realloc runtime

追加シンボル:

```llvm
declare / define internal ptr @tz.realloc(ptr %old, i64 %old_size, i64 %new_size)
```

契約:

- `%old == null` なら `@tz.alloc(new_size)` と同じ。
- `%new_size == 0` は `%old` を free し `null` を返す。C02 の通常 grow では呼ばないが truncate future-proof。
- 失敗は trap。null 成功値を返さない。
- `%old_size` は copy byte 数の上限。WASM fallback の `memcpy` に使う。

Native (`heap-native.ll`):

- `declare ptr @realloc(ptr, i64)` を追加。
- `@tz.realloc` は `realloc` を呼び、`new_size != 0` で null なら trap。

WASM (`heap-wasm.ll`):

- null / zero は header を読む前に分岐する。
  - `%old == null && new_size == 0` は `null` を返す。
  - `%old == null && new_size != 0` は `@tz.alloc(new_size)`。
  - `%old != null && new_size == 0` は `@tz.free(old)` 後 `null`。
- `new_size <= 16_777_184` を検査し、超過なら trap。これは既存 `@tz.alloc` の payload 上限と同じ。
- `new_total = align16(new_size + 16)` を計算する。既存 allocator と同じ IR 形にするなら、`new_size` を i32 に trunc した後、`%rounded = add i32 %small, 31; %new_total = and i32 %rounded, -16`。`new_size <= 16_777_184` を先に検査するため trunc は安全。
- old payload から `old_block = ptrtoint(old) - 16`、`old_header = inttoptr old_block` を得て、`old_total = load i32, ptr old_header`。
- `new_total <= old_total` ならその場で `%old` を返す。Phase 1 では shrink split をしない。
- 隣接 free block を探す。`adjacent = old_block + old_total` とし、アドレス順 free list を predecessor link 付きで走査して、`block == adjacent` の free block を見つける。
- 隣接 block が見つかり、`old_total + next_total >= new_total` なら in-place grow:
  - predecessor link を隣接 block の next へ向けて unlink する。
  - `remainder = old_total + next_total - new_total`。
  - `remainder >= 32` なら `split = old_block + new_total` に free header を作り、size=remainder、next=old_next を書き、predecessor link を split へ向ける。old header size は `new_total`。
  - `remainder < 32` なら whole-block absorb とし、old header size は `old_total + next_total`、predecessor link は old_next のまま。
  - payload pointer は元の `%old` を返す。
- 隣接 block が無い、または足りない場合は fallback:
  - `new = @tz.alloc(new_size)`。
  - `copy = min(old_size, new_size)` bytes を `llvm.memcpy.p0.p0.i64` でコピー。
  - `@tz.free(old)`。
  - `new` を返す。
- `llvm.memcpy.p0.p0.i64` を runtime IR 内で宣言する。WASM bulk-memory 前提は既存と同じ。
- 16 MiB 上限・memory.grow failure は既存 `@tz.alloc` と同じ trap。

WASM allocator tests must cover:

- null realloc: `@tz.realloc(null, 0, n)` behaves like alloc。
- zero realloc: `@tz.realloc(p, old, 0)` frees and returns null。
- in-place split: adjacent free block is larger than needed and leaves a remainder `>= 32` that remains reusable。
- whole-block absorb: remainder `< 32` is absorbed and not left as an invalid tiny free block。
- fallback copy: adjacent block absent or too small, data preserved after alloc+copy+free。
- post-realloc coalescing: freeing the grown block coalesces with following/previous free blocks under the existing `@tz.free` rules。

### Vec operations lowering

専用 helper:

```rust
fn vec_parts(&mut self, value: &str) -> (String, String, String); // data, len, cap
fn make_vec(&mut self, data: &str, len: &str, cap: &str) -> String;
fn vec_element_pointer(&mut self, element: &Type, data: &str, index: &str) -> String;
fn vec_reserve_capacity(&mut self, element: &Type, vec: &str, required: &str) -> String;
```

`Vec.push` IR shape:

```llvm
%len = extractvalue %tz.vec %v, 1
%cap = extractvalue %tz.vec %v, 2
%required = add i64 %len, 1       ; overflow checked with llvm.uadd.with.overflow or icmp
%grow = icmp ugt i64 %required, %cap
br i1 %grow, label %reserve, label %store
reserve:
  ; compute new_cap by loop, call @tz.realloc(old, old_bytes, new_bytes)
store:
  %slot = getelementptr inbounds T, ptr %data2, i64 %len
  store T %value, ptr %slot
  %new_len = add i64 %len, 1
  ret %tz.vec { data2, new_len, cap2 }
```

`reserve` / `push` growth loop details:

- For `push`, compute `required = len + 1` with overflow check before any store。
- For `reserve additional`, reject `additional < 0` before converting to unsigned。Then compute `required = len + additional` with overflow check。
- In the doubling loop, before `new_cap * 2` check `new_cap <= i64::MAX / 2`; otherwise trap. Alternatively use `llvm.umul.with.overflow.i64(new_cap, 2)` and trap on the overflow bit。
- Compute both old byte size (`cap`) and new byte size (`new_cap`) through `allocation_size(element, cap)` / `allocation_size(element, new_cap)` so negative/overflow byte counts use the same trap path as arrays。

`Vec.pop`:

- empty: return tuple `{ v, Option.None }`。
- non-empty:
  - `%new_len = sub i64 %len, 1`
  - `%slot = gep data, new_len`
  - `%value = load T, ptr %slot`
  - slot は未初期化扱いにする。必要なら `store T zeroinitializer` して二重 drop 防止。ただし drop は len のみ見るため必須ではない。ASan friendliness のため zero store を推奨。
  - return `{ vec(data,new_len,cap), Option.Some value }`

`Vec.to_array`:

- return `%tz.array { ptr=data, i64 len }`。
- `cap` は捨てる。呼び出し後の `Vec` は move 済みなので drop されない。

`Vec.of_array`:

- `%data = extractvalue %tz.array array, 0`
- `%len = extractvalue %tz.array array, 1`
- if `%len == 0`: if `%data != null` then `call void @tz.free(ptr %data)`; return `%tz.vec zeroinitializer`
- else return `%tz.vec { data, len, len }`
- 入力 array は move 済み。drop されない。

### Index / for-in

`src/check.rs` `ExprKind::Index`:

- `Type::Vec(element)` を受理し、結果型は `element`。
- 既存配列と同じく ownership が非 Copy 要素の move を `E1012` で拒否する。

`src/ownership.rs`:

- `Checker::place` / `is_place` の `Index` 条件に `Type::Vec(_)` を追加。
- collection loan の `usize::MAX` sentinel を Vec にも使う。

`src/control.rs`:

- `for...in` source に `Type::Vec(element)` と `Type::Reference(Vec, false)` を追加。`&Vec` は C03 がなくても通常 reference として autoderef で扱える。

`src/llvm_control.rs`:

- `for_each` は source.ty が Vec の場合、`extractvalue %tz.vec 0/1` で data/len を得る。要素 pointer は `element_pointer(element, data, index)`。

### String builder 連携

D02 との既定インターフェース:

```text
String.from_bytes : Vec ubyte -> string
String.to_bytes   : &string -> [ubyte]
```

- `String.from_bytes` は `Vec.to_array` 相当の buffer transfer ではなく UTF-8 validation / string ownership に従う。Phase 1 では ASCII/UTF-8 validation を D02 に任せ、C02 は `Vec ubyte` を効率よく作れることだけを保証する。

## 実装手順

1. **型表現追加**
   - `Type::Vec` と各再帰関数を追加。
   - 確認: `Vec i64` の annotation を含む最小テストが `Type::display == "Vec i64"` になる。

2. **LLVM 型・drop/clone**
   - `%tz.vec` を IR header に追加。
   - `llvm_type`、`drop_value`、`clone_value`、`llvm_frame.rs` `stack_size` / `field_types` を更新。
   - 確認: `fn f :: Vec i64` を emit して `%tz.vec` が一度だけ定義される。

3. **Runtime realloc**
   - `heap-native.ll` / `heap-wasm.ll` に `@tz.realloc` を追加。
   - `emit_target` の runtime include 条件を `@tz.realloc` でも heap runtime が入るようにする。
   - WASM は上記の exact algorithm（null/zero before header read、16 MiB check、in-place split/absorb、fallback copy）を実装する。
   - 確認: scratch IR で native/WASM とも `@tz.realloc` 定義が連結され、WASM imports は増えない。allocator unit/E2E で split/absorb/fallback/null/zero/coalescing を確認する。

4. **基本 builtin**
   - `Vec.empty`、`with_capacity`、`length`、`capacity`、`is_empty` を実装。
   - 確認: `tests/vec.rs` で型検査と IR 決定性。

5. **push/reserve/truncate/drop**
   - `len + additional`、doubling、old/new byte counts の overflow guard を実装。
   - `truncate` で削除要素の drop を確認。
   - 確認: `Vec.push` で capacity 0→4→8、`truncate` 後 heap tracking `live == 0`。

6. **pop/get/at/set/swap**
   - B01 `Option` と tuple を使った戻り値を生成。
   - `Vec.at` の借用 result は `Signature::validate_borrows` を満たすよう `&Vec` 入力 1 つに結び付ける。
   - 確認: 範囲外 trap / `None` の双方を E2E。

7. **array transfer / for-in / indexing**
   - `Vec.of_array` / `Vec.to_array` O(1) transfer。
   - `ExprKind::Index` と `ForEach` に Vec を追加。
   - 確認: IR に element copy loop がない transfer test。

8. **Docs / benchmarks**
   - docs 更新と `benchmarks/vec` を追加する場合は速度閾値なし。
   - 確認: targeted Rust test、Node E2E native/WASM `-O0/-O3`。

## テスト計画

### Rust: `tests/vec.rs`

受理:

- `def f :: Vec i64\nfn f = Vec.empty()`
- `def f :: Vec i64\nfn f = Vec.with_capacity 10`
- `def f :: i64\nfn f = Vec.length (&(Vec.push (Vec.empty()) 1))`
- `def f :: [i64]\nfn f = Vec.to_array (Vec.push (Vec.of_array [1,2]) 3)`
- `def f :: (Vec i64 * Option i64)\nfn f = Vec.pop (Vec.push (Vec.empty()) 1)`
- `def f :: i64\nfn f = { let v = Vec.push (Vec.empty()) "x"; (Vec.at (&v) 0).length }`
- `for x in v do ...` と `for x in &v do ...`

拒否:

- `let v = Vec.push (Vec.empty()) "x"; let w = Vec.push v "y"; v` → `E1012`
- `Vec.push (&v) 1` → `E1003`
- `Vec.get (&v) 0` where element is `string` → `E1005` no `Copy string`
- `&mut v[0]` → `E1014`
- `record R { v: Vec &string }` または借用を含む Vec field → `E1013`
- `export def f :: Vec i64` → `E1008`

IR:

- `Vec.to_array (Vec.of_array xs)` に element loop がない。
- `Vec.of_array []` と runtime zero-length array 由来の `Vec.of_array` は `%tz.vec zeroinitializer` を返し、incoming non-null allocation を free する。
- `Vec.push` に `@tz.realloc` が現れる。
- `Vec.truncate` は dropped range だけ loop し、残存 len を更新する。
- `for x in v` の loop 内に `alloca` がない。
- Growth overflow: `Vec.reserve v (-1)`, `Vec.reserve v i64::MAX`, `Vec.with_capacity i64::MAX`, `Vec.reserve v 4611686018427387904` など near-half-max / max cases が trap する。
- WASM realloc runtime unit fixtures: split, whole-block absorb, fallback copy, null realloc, zero realloc, post-realloc coalescing。

Native heap tracking for realloc:

- IR 置換は `@malloc`→`@tracked_alloc_raw` ではなく、heap runtime の `declare ptr @malloc`, `declare void @free`, `declare ptr @realloc` をそれぞれ payload-aware tracker に差し替える。
- 既存の tracked allocator が `uint64_t` header 2 words を付けて `p + 2` を返す場合、`tracked_realloc` は payload pointer をそのまま libc `realloc` に渡してはいけない。`((uint64_t*)payload) - 2` に戻してから realloc する。

```c
static uint64_t live, peak, allocations;
static const uint64_t MAGIC = UINT64_C(0x51a110ca7e);

void *tracked_malloc(uint64_t n) {
    uint64_t *p = malloc((size_t)n + 16);
    assert(p);
    p[0] = n;
    p[1] = MAGIC;
    live += n;
    if (live > peak) peak = live;
    allocations++;
    return p + 2;
}

void tracked_free(void *payload) {
    if (!payload) return;
    uint64_t *p = ((uint64_t *)payload) - 2;
    assert(p[1] == MAGIC);
    live -= p[0];
    p[1] = 0;
    free(p);
}

void *tracked_realloc(void *payload, uint64_t n) {
    if (!payload) return tracked_malloc(n);
    if (n == 0) {
        tracked_free(payload);
        return NULL;
    }
    uint64_t *old = ((uint64_t *)payload) - 2;
    assert(old[1] == MAGIC);
    uint64_t old_size = old[0];
    uint64_t *p = realloc(old, (size_t)n + 16);
    assert(p);
    p[0] = n;
    p[1] = MAGIC;
    live = live - old_size + n;
    if (live > peak) peak = live;
    allocations++;
    return p + 2;
}
```

### E2E: `tests/fixtures/vec/Vec.tz`

公開関数:

- `vec_push_sum(n)` → JS BigInt で `0..n-1` sum。
- `vec_growth(n)` → push 後 capacity が 2 倍系で `>= len`。値は checksum。
- `vec_pop_order(n)` → LIFO の合計。
- `vec_set_swap()` → JS 配列 reference。
- `vec_to_array_no_copy(n)` → native allocation count を `Vec.with_capacity + pushes` と比較。速度閾値ではなく「transfer 後 live == 0」。
- `vec_strings(n)` → string 要素 push/pop/truncate で leak なし。
- `vec_bounds(index)` → `Vec.at` trap。
- `vec_get(index)` → `Option` の `None` / `Some` checksum。

Native/WASM:

- `-O0` / `-O3` 両方。
- WASM imports empty、16 MiB 上限。
- native heap tracking `live == 0` after each call。
- `Vec.with_capacity(20_000_000)` などは子プロセスで trap。
  `Vec.reserve` の negative / `i64::MAX` / near-half-max overflow trap も子プロセスで確認する。

検証コマンドは GUIDE §3 に従い、Rust は `cargo test --locked --test vec` で `running N tests` の `N > 0` を確認する。Node E2E の直前には必ず `cargo build --release --locked` を実行し、`target/release/tsuzuri` を更新してから `node tests/*.mjs target/release/tsuzuri` を走らせる。

### 性能・IR確認

- `/tmp` scratch で `Vec.push` loop を `--emit llvm -O3` → `clang -O3 -S -emit-llvm` に通し、push hot path が len/cap branch + store になっていることを確認。
- `Vec.to_array` は O(1) transfer であることを IR で確認し、速度の CI 閾値は設けない。
- D02 連携時は `Vec ubyte` で byte builder を測るが、UTF-8 validation cost と分けて報告する。

## ドキュメント

- `docs/language.md`
  - 型表に `Vec 'a` を追加。
  - 配列節に「固定長 immutable array」と「伸縮可能な作業バッファ Vec」の違い。
  - API 一覧、trap 条件、`Vec.to_array` / `Vec.of_array` の O(1) transfer。
- `docs/architecture.md`
  - `%tz.vec = { ptr, i64 len, i64 cap }`。
  - `@tz.realloc` native/WASM 契約。
  - non-Copy だが function environment clone は deep clone する理由。
- `README.md`
  - 未実装一覧から伸縮可能なコレクションを外す。
  - 短い例: push で作り、to_array で読み取り API に渡す。
- `docs/benchmarks.md`
  - 測定した場合だけ環境・中央値・生データを記録。

## 受け入れ条件

- [ ] `Vec 'a` が型注釈・関数引数・戻り値・レコード field に使える。ただし借用を含む field は既存通り拒否。
- [ ] `Vec` は暗黙 Copy されない。
- [ ] `Vec.push` / `reserve` が overflow と allocation failure を trap する。
- [ ] growth は `len + additional`、doubling、old/new byte counts の全てで overflow を trap する。
- [ ] `Vec.pop` / `get` が B01 `Option` と正しく連携する。
- [ ] `Vec.at` の借用寿命が `&Vec` 入力に結び付き、所有者より長生きしない。
- [ ] `Vec.to_array` / `Vec.of_array` が要素 copy なし。
- [ ] `Vec.of_array` は `len == 0` の incoming array allocation を free し、`%tz.vec zeroinitializer` に正規化する。
- [ ] `drop_value` が `len` 個だけ drop し、未初期化 capacity を読まない。
- [ ] `@tz.realloc` が native/WASM で実装され、WASM imports を増やさない。
- [ ] native/WASM × `-O0`/`-O3` E2E、heap tracking `live == 0`。
- [ ] 生成 IR が決定的。

## 落とし穴

- `cap` 個を drop すると未初期化領域を読む。必ず `len` 個。
- `Vec.to_array` 後に元 Vec を drop すると二重 free。所有値 move 済みとして扱う。
- `Vec.of_array` 後に元 Array を drop すると二重 free。同上。
- `realloc` 後、古い data pointer を使って store しない。
- WASM `realloc` の in-place grow は free list のリンク更新を間違えると allocator 全体が壊れる。`tz.free` の隣接結合と同じ address-order invariant を守る。
- WASM `realloc` は null/zero を header read より前に処理する。`old == null` や `new_size == 0` で `old - 16` を読まない。
- native heap tracking の `tracked_realloc` は payload pointer ではなく tracker header へ戻してから libc `realloc` を呼ぶ。
- `Vec.get` の `Option 'a` で非 Copy 要素を返すと共有借用から move になり危険。Phase 1 は `Copy 'a` 制約。
- `Vec.at` を `Option &'a` にすると borrowed union の寿命表現が B01/A02 に依存しすぎる。Phase 1 は trap する `&'a` 返却。

## 対象外

- `Vec` リテラル構文。
- `Vec.insert` / `Vec.remove` / `Vec.sort`。C04/C06 以降。
- shrink-to-fit。
- SmallVec / stack inline buffer。
- Thread-safe shared mutable vector。
- GPU / SIMD 専用 builder。
- Public C/WASM ABI で `Vec` を渡すこと。

## 未決事項

- `Vec` の暗黙 Copy は導入しない既定案。要素が Copy でも大きな allocation を隠すため。
- `Vec.clone` は `Copy 'a` 制約に限定する既定案。将来 `Clone` / `Default` クラスが入れば拡張する。
- `Vec.at` は範囲外 trap、`Vec.get` は `Option` の既定案。`Option &'a` は borrowed union の仕様が固まるまで入れない。
- 台帳の見直し提案: なし。D-13 の `Vec 'a` と `[T]` descriptor 不変に従う。
