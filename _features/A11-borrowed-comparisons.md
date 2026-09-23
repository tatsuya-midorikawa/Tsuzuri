# A11: 比較演算の非消費化（Eq／Ord の借用シグネチャ）

| 項目 | 内容 |
|---|---|
| ID | A11 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | – |
| 後続 | A06, A07, C04, C06 |
| 状態 | todo |
| 主な影響ファイル | `src/polymorph.rs`, `src/check.rs`, `src/ownership.rs`, `src/call_specialization.rs`, `src/llvm.rs`, `src/llvm_frame.rs`, `docs/language.md`, `docs/architecture.md`, `README.md`, `tests/polymorphism.rs`, `tests/types_ownership.rs`, `tests/fixtures/polymorphism/*`, `examples/polymorphism/*` |

## 目的

`==`、`!=`、`<`、`<=`、`>`、`>=` を全型で非消費にする。現状は `string` の `==`/`!=` だけが特例で非消費だが、ユーザー定義 `Eq`/`Ord`、A06 の条件付き `Eq ['a]` / `Ord (Option 'a)`、A07 の deriving、C04 の `Array.contains` / `Array.sort`、C06 の Map key 比較では、比較のためだけに非 Copy 値を move してしまう。このチケットで `Eq`/`Ord` の組み込みシグネチャを借用へ変更し、演算子 lowering と所有権検査をそれに合わせる。

この変更は比較だけを対象にする。`Add`/`Sub`/`Mul`/`Div`/`Rem`/`Bits`/`Neg` は従来どおり値を受け取り、`string + string` も両辺を消費する。

## 現状

- `src/polymorph.rs` `Classes::collect` は組み込み binary method を一括生成し、`Eq.eq`/`Eq.ne` と `Ord.lt`/`le`/`gt`/`ge` の `Signature.parameters` を `vec![a.clone(), a]` にしている。比較 result だけ `bool` で、引数は値型である。
- `src/check.rs` `Checker::binary_type` は左右の型を一致させ、`polymorph::binary_class(operator)` の制約を追加する。ここでは所有権を扱わず、比較演算子も他の演算子と同じ `TypedExprKind::Binary` のまま。
- `src/polymorph.rs` `Classes::instances` は class method の `method.signature.parameters` を `substitute` し、`type_expression` で instance method の `FunctionDecl.parameters` を作る。したがって class signature を `&'a -> &'a -> bool` に変えると、ユーザー instance の method 引数型も自動的に `&Concrete` になる。
- `src/polymorph.rs` `Specializer::lower` は非 intrinsic な binary operator を `operator_call` に置換する。現在の `operator_call` は `Call(method, [left, right])` を作るだけなので、非 Copy 値の比較は user instance method へ値を move する。
- `src/polymorph.rs` `intrinsic_function` は `Eq.eq` のような intrinsic method value 用に wrapper を合成し、現在は `TypedExprKind::Binary(operator, Local(0), Local(1))` を body にする。比較 method value の型が借用になると、wrapper body では dereference してから intrinsic `Binary` を使う必要がある。
- `src/ownership.rs` `Checker::eval_value` の `E::Binary` 分岐は `string` の `==`/`!=` だけ `Use::Read` にし、それ以外は `Use::Consume` にする。左 operand の読み取り中の loan は `held` に入れて右 operand の評価中も保持する。
- `src/ownership.rs` `Checker::place` は source-level `&temporary` を `E1013` `"borrow requires a local place; bind the temporary with 'let' first"` で拒否する。A11 の演算子内部の一時値借用は、この source-level 制約とは別の compiler-internal materialization として実装する。
- `src/call_specialization.rs` `read_only` は `string` `==`/`!=` だけ `Access::Read` として扱う。他の比較演算子が user instance call へ下がると、借用 worker 判定で「読むだけ」の情報が失われる。
- `src/llvm.rs` `FunctionEmitter::binary` は `string` なら `read_operand`/`take_operand` を使い、`==`/`!=` では `@tz.string.equal` 後に `release_operand(left)`、`release_operand(right)` の順に一時値を解放する。数値/bool/unit は `icmp`/`fcmp` 等へ直接下がる。
- `src/llvm_frame.rs` `Frame::read_operand` は場所なら所有者を残したまま `read_place`、場所でない式なら `frame_value` で一時フレームへ materialize する。`release_operand` は場所でない一時値だけ `drop_framed` する。
- `tests/types_ownership.rs` は `fn f() -> bool { let s = "x"; s == s && { let t = "x"; t } == s }` を受理し、`fn f() -> bool { let s = "x"; s == { let t = s; t } }` を借用競合で拒否する。これは string equality の現行非消費規則の回帰テストである。
- `tests/polymorphism.rs` は user class / instance、method value、operator instance、数値 representation の operator specialization を検査するが、非 Copy record の `Eq`/`Ord` user instance と演算子非消費は未検査。
- `tests/fixtures/polymorphism/Main.tz` と `examples/polymorphism/Main.tz` は `Add Point` と borrowed user class method value を使うが、`Eq Point`/`Ord Point` はない。
- `docs/language.md` は「既存の string の `==`／`!=` は引き続き非消費の比較です。`Eq.eq` のような通常のメソッド適用ではシグネチャどおり引数の所有権を渡します。」と書いている。A11 後はこの文を削除し、比較全体の非消費規則と `Eq`/`Ord` の借用シグネチャへ更新する。

## 仕様

### 組み込みクラスのシグネチャ

`Eq` と `Ord` だけを次へ変更する。

```text
class Eq 'a {
    def eq :: &'a -> &'a -> bool
    def ne :: &'a -> &'a -> bool
}

class Ord 'a {
    def lt :: &'a -> &'a -> bool
    def le :: &'a -> &'a -> bool
    def gt :: &'a -> &'a -> bool
    def ge :: &'a -> &'a -> bool
}
```

`Add` 等は変更しない。

```text
class Add 'a {
    def add :: 'a -> 'a -> 'a
}
```

メソッド値もこの型を持つ。例えば `Eq.eq` は制約付きの `&'a -> &'a -> bool`、`Ord.lt` は `&'a -> &'a -> bool` であり、次のように使う。

```text
def same_by_method :: Eq 'a => &'a -> &'a -> bool
fn same_by_method left right =
    let f: &'a -> &'a -> bool = Eq.eq;
    f left right
```

### 演算子の所有権

比較演算子 `==`、`!=`、`<`、`<=`、`>`、`>=` は、全型で左右 operand を消費しない。

```text
record User { name: string, score: i64 }

instance Eq User {
    fn eq left right = left.name == right.name && left.score == right.score
    fn ne left right = !(Eq.eq left right)
}

def compare_twice :: User -> bool
fn compare_twice user = {
    let same = user == user;
    same && user.name.length > 0
}
```

上の `user == user` は `user` を move せず、比較後も `user` を使える。

左右 operand は左から右に評価する。operand が場所（local、field、array/list element、dereference など）ならその場所を共有借用する。operand が一時値なら compiler が内部一時スロットへ materialize して共有借用する。比較呼び出し後、内部一時値は左 operand、右 operand の順に解放する。これは既存の `string` equality が `read_operand(left)`, `read_operand(right)`, `@tz.string.equal`, `release_operand(left)`, `release_operand(right)` の順に動くことと一致させるためである。

比較中の loan は右 operand の評価と method 呼び出しが終わるまで生存する。したがって、左 operand を比較用に読んだあと、右 operand の評価で同じ所有者を move/置換/排他借用しようとすると従来どおり `E1014` になる。

```text
fn bad() -> bool {
    let mut text = "x";
    text == { text = "y"; text } // E1014
}
```

### ユーザー instance body

instance method の仮引数は `&T` になる。既存の field access は `Checker::autoderef` によりそのまま通る。例えば `left.x == right.x` は `left` / `right` を field access のために自動参照外ししてから、field 値を比較する。

一方で、比較演算子自体は参照を自動 dereference しない。`src/check.rs` `Checker::binary_type` は左右の型をそのまま一致させて `Eq` / `Ord` 制約を課すだけで、`left == right` のような `&R` 同士の比較は `Eq (&R)` を要求する。`Eq (&R)` は存在しないため、instance method 内で同じ型の値全体を比較したい場合は `Eq.eq left right` / `Ord.lt left right` のように class method を呼ぶ。

```text
record Point { x: i64, y: i64 }

instance Eq Point {
    fn eq left right = left.x == right.x && left.y == right.y
    fn ne left right = !(Eq.eq left right)
}
```

ただし、仮引数そのものを値として消費するコードは壊れる。移行では、値渡し関数を借用版へ変更するか、必要な field だけを読む。

```text
def consume :: Point -> i64
fn consume point = point.x

instance Eq Point {
    fn eq left right = consume left == consume right // E1003: left/right は &Point
    fn ne left right = !(Eq.eq left right)
}
```

`*left` による非 Copy 値の move も `E1012` で拒否する。

### intrinsic 組み込みインスタンス

数値、`bool`、`unit`、`string`、A08 の `char`、D02 の `string Ord` は intrinsic のままにする。

- 演算子としての `1i64 == 2i64`、`x < y` は従来どおり `icmp`/`fcmp`/`@tz_soft_cmp` へ直接 lower し、参照引数や wrapper call を生成しない。
- `string == string`/`!=` は従来どおり `@tz.string.equal`。D02 の `string < string` などは `@tz.string.compare` を直接呼ぶ intrinsic lowering とし、参照 traffic は生成しない。
- A08 の `char` は `i32` scalar として `icmp` へ lower し、operator codegen は A11 前後で同じ形を維持する。
- `Eq.eq` や `Ord.lt` を method value として取得した場合だけ、借用シグネチャに合う monomorphic wrapper を生成する。この wrapper 内では参照を dereference してから同じ intrinsic `Binary` を使う。

### 診断

新しい診断コードは追加しない。

| 条件 | 期待コード |
|---|---|
| source-level `&temporary` | 既存 `E1013` |
| 比較中に operand 所有者を move/置換/排他借用 | 既存 `E1014` |
| `Eq`/`Ord` instance body で borrowed parameter を値関数へ渡す | `E1003` |
| borrowed parameter を dereference して非 Copy 値を move | `E1012` |
| `Eq`/`Ord` instance がない | 既存 `E1005` |
| 重複/上書き instance | 既存 `E1016` |

### 前提とする他チケットのインターフェース

なし。A11 は既存 compiler に対する先行修正として実装できる。

### 他チケットへの提供インターフェース

- A06 は `Eq ['a]`、`Ord (Option 'a)`、superclass default method を借用シグネチャで書く。`fn le x y = Ord.lt x y || Eq.eq x y` の `x`/`y` は `&'a`。
- A07 は derived `Eq`/`Ord` を非消費に生成できる。field/payload の比較は借用を渡し、非 Copy component を move しない。
- C04 は `Array.equal`、`contains`、`index_of`、`binary_search`、`sort` の comparator で要素を借用して比較できる。値を返す API だけ `Copy` 等を要求する。
- C06 は key comparison のためだけの `Copy 'k` を外せる。`Map.get` の値返却や `Map.keys` の配列生成など、値を返す操作に必要な `Copy` は残す。

## 設計

### `Classes::collect`

`src/polymorph.rs` `Classes::collect` の binary method 生成を分岐する。

```rust
let parameter = Type::Variable("a".into());
let parameters = if matches!(
    operator,
    Equal | NotEqual | Less | LessEqual | Greater | GreaterEqual
) {
    vec![
        Type::Reference(Box::new(parameter.clone()), false),
        Type::Reference(Box::new(parameter.clone()), false),
    ]
} else {
    vec![parameter.clone(), parameter.clone()]
};
```

`result` は比較なら `Type::Bool`、それ以外は従来どおり `parameter`。`variables(signature.as_type())` は引き続き `["a"]` であり、class method 検査は通る。

### `Classes::instances`

`Classes::instances` は現在の `type_expression(&substitute(ty, &substitutions), records, span)` を維持する。A11 後は `Eq Point` の method parameter が `&Point` になるため、ユーザーが `fn eq left right = left.x == right.x` と書いた場合、生成 `FunctionDecl` の parameter type が `&Point` になる。

注意点:

- method arity は変えない。`eq`/`lt` は従来どおり 2 引数。
- `definition.parameters.len()` の検査は現状維持。
- default method を持つ A06 後も、class signature から生成される parameter type は借用になる。

### `Checker::binary_type`

`src/check.rs` `Checker::binary_type` の外部仕様はほぼ変更しない。左右の値型は同じでなければならず、比較演算子は `Eq T` または `Ord T` constraint を追加し、結果は `bool`。

ここで operand type を `&T` に変えないことが重要である。演算子の surface type は `T == T -> bool` のままに見えるが、所有権と lowering が内部的に借用へ変換する。

### operator lowering

`src/polymorph.rs` `Specializer::lower` では、intrinsic でない比較演算子だけ borrowed operator call へ下げる。

概念上の lowering:

```text
left == right
```

を次へ変換する。

```text
Call(
  Function(instance_eq),
  [Borrow(left_place_or_temp), Borrow(right_place_or_temp)]
)
```

実装では source-level `TypedExprKind::Borrow` と区別するため、compiler-internal shared borrow operand を表す小さな IR を追加する。

```rust
pub enum TypedExprKind {
    // existing ...
    Borrow(Box<TypedExpr>, bool),          // source `&` / `&mut`
    BorrowOperand(Box<TypedExpr>),         // A11: compiler-generated shared borrow for comparison
}
```

`BorrowOperand` の型は `Type::Reference(Box::new(inner.ty.clone()), false)`。`children`/`children_mut`、`expression_types`、`walk`、`closures::lower_expression`、`recursion::references`、`count_uses`、`call_specialization::all_children` へ必ず追加する。

`Specializer::operator_call` は比較 class の場合だけ `arguments` を `BorrowOperand(argument)` に包む。算術/ビット/単項 operator は従来どおり値引数。

```rust
let borrowed = matches!(class_name, "Eq" | "Ord");
let arguments = if borrowed {
    arguments.into_iter().map(borrow_operand).collect()
} else {
    arguments
};
```

`BorrowOperand` は source syntax から作らない。これによりユーザーが書いた `&"temporary"` は引き続き `E1013` で拒否できる。

### intrinsic method value wrapper

`src/polymorph.rs` `Specializer::intrinsic_function` は method signature を substitution した後に parameters を作る。A11 後、`Eq.eq i64` の wrapper parameters は `&i64`, `&i64` になる。`method.operation` が comparison なら body operand を dereference してから `TypedExprKind::Binary` を作る。

```rust
let argument_value = |id: usize, expected: &Type| {
    let local = TypedExpr { kind: Local(id), ty: parameters[id].ty.clone(), span };
    if matches!(parameters[id].ty, Type::Reference(_, false)) && !matches!(expected, Type::Reference(_, _)) {
        TypedExpr { kind: Dereference(Box::new(local)), ty: expected.clone(), span }
    } else {
        local
    }
};
```

operator lowering では intrinsic class を `None` のまま残すため、この wrapper は method value / first-class use にだけ使われる。

### ownership

`src/ownership.rs` の変更:

- `Checker::eval_value` の `E::Binary` 分岐で、`Equal | NotEqual | Less | LessEqual | Greater | GreaterEqual` は全型 `Use::Read` にする。`string` 特例を削除し、比較 operator 特例へ一般化する。
- 左 operand の `Value` は現状どおり `held.push` し、右 operand 評価中も loan を保持する。これにより `left == { mutate left }` を `E1014` で拒否する。
- `BorrowOperand` の eval は `Use::Read` と同じ。場所なら shared loan を作り、一時値なら「比較呼び出し中だけ生きる所有一時」として評価し、その loan を result に入れる。source-level `Borrow` の `place()` 制約は変えない。
- loan は比較 call の評価後に終わる。`BorrowOperand` の loan を外側の `Value` result へ漏らさない。

`uses`/`count_uses` は `BorrowOperand` の子を数える。これにより一時値や local の live set は既存の operand 評価順序と同じになる。

### call specialization

`src/call_specialization.rs` の変更:

- `read_only` の `Binary(Equal | NotEqual)` string 特例を、全比較演算子の `Access::Read` へ一般化する。
- `BorrowOperand(value)` は `read_only(value, local, Access::Read, module)` として扱う。
- `may_mutate` と `non_escaping` は `all_children` 経由で `BorrowOperand` をたどる。
- `binary_operation` は intrinsic wrapper body が `Dereference(Local)` を含むようになるため、tail recursion の arithmetic snapshot 判定に誤って比較 wrapper を含めない。加減算だけを扱う既存 filter は維持する。

`FunctionEmitter::borrowed_call` / `prepare_borrowed_call` は既存の「関数値の捕捉を呼び出し中だけ貸す」最適化であり、A11 の比較 operand borrow とは別概念である。名前や helper を混同しない。

### LLVM lowering

`src/llvm.rs` / `src/llvm_frame.rs` の変更:

- intrinsic 比較 operator は従来の `FunctionEmitter::binary` を使う。数値/bool/unit/char は `self.expression(left)`, `self.expression(right)` から `icmp`/`fcmp`、string は `read_operand` と `@tz.string.equal` / D02 の `@tz.string.compare`。
- user instance 比較は `Call(method, [BorrowOperand(left), BorrowOperand(right)])` を通常 call として emit する。ただし `BorrowOperand` は値ではなく `ptr` を返す。
- `BorrowOperand` の codegen は専用 helper `prepare_comparison_borrow` で行う。`FunctionEmitter::read_operand` / `llvm_frame.rs::frame_value` は **SSA value と `Frame` metadata** を返すだけで slot pointer を返さないため、その戻り値を直接借用 pointer として使ってはいけない。
- `prepare_comparison_borrow(expression)` の規則:
  1. `FunctionEmitter::is_place(expression)` が true なら `place(expression)` を返し、cleanup は空。`FunctionEmitter::place` は temporary を受け付けないので、この分岐以外で呼ばない。
  2. place でなければ `let (value, frames) = frame_value(expression)` で一時値を評価する。
  3. `let slot = slot(&expression.ty)` で entry-block alloca を作る。`slot` は既存どおり `self.allocas` へ追加するため、末尾再帰の entry alloca invariant を保つ。
  4. `store <ty> value, ptr slot` を emit し、借用先 pointer として `slot` を返す。
  5. cleanup として `(expression.ty.clone(), value, frames)` ではなく `(expression.ty.clone(), slot, value, frames)` 相当を保持する。drop 時は `drop_framed(&ty, &value, &frames)` を左から右に呼び、必要なら `store zeroinitializer` で slot を無効化する。`drop_framed` は SSA value と frame metadata に対して呼び、slot pointer 自体を value と誤認しない。
- call 後に、`BorrowOperand` が作った一時値だけを上記 cleanup で解放する。解放順は左 operand、右 operand の順。既存 call cleanup が右から左に drop する箇所へ混ぜず、comparison borrowed operand cleanup は専用の左から右順にする。
- 生成 IR は決定的にする。一時 slot 名は既存 `slot()` / `value()` の連番に従い、BTreeSet/BTreeMap を使う箇所は現状維持。

実装方法の既定案:

1. `FunctionEmitter::call` で arguments を評価するとき、`BorrowOperand` を検出して `(ptr, cleanup)` を返す `prepare_comparison_borrow` を使う。
2. known direct call と function value apply の両方で、borrowed operand cleanup を call/apply の直後に左から右で実行する。
3. `BorrowOperand` は comparison lowering からしか生成しないため、一般 API として露出しない。

### recursion checks

`src/recursion.rs` は `Classes::recursion_targets` を通じて `TypedExprKind::Binary` / `TypedExprKind::Method` の参照を集める。A11 後、user instance comparison は単相化で `Call(Function(instance), [BorrowOperand...])` になるが、再帰検査は単相化前に走るため次を守る。

- `Classes::recursion_targets` は `Binary(Equal | Less ...)` の class/method を引き続き候補に含める。
- `BorrowOperand` を `children` に含めることで、comparison operand 内の関数参照も漏らさない。
- instance method body が `Eq.eq left right` / `Ord.lt left right` で同じ instance method へ再帰する場合は、従来どおり `rec` が必要。`left == right` のような参照同士の演算子比較は `Eq (&T)` を要求するため、この用途には使わない。

### tail recursion

`src/llvm.rs` `tail_arguments` は native で整数 `+`/`-` の nontrapping wrapping arithmetic だけを latch へ遅延する。A11 後も比較 call は末尾引数の遅延対象にしない。

- intrinsic numeric comparison は `Binary(Equal | Less ...)` なので `binary_operation` で見えても、既存 filter `matches!(operator, Add | Subtract)` により遅延されない。
- user comparison は `Call` になるが、tail recursion 引数として現れた場合は通常どおり `self.expression(argument)` で評価し、借用一時値の cleanup を終えてから back edge に入る。
- 借用を含む引数がある関数では `is_self` が tail loop 化を避ける既存条件を維持する。

### 各コンパイラ段への変更

| 段 | 変更 |
|---|---|
| lexer | 変更なし |
| parser | 変更なし |
| syntax | source AST 変更なし。typed IR に `BorrowOperand` を追加する既定案 |
| computation | 変更なし。展開後の比較演算子に通常どおり型検査がかかる |
| check | `Checker::binary_type` は制約収集を維持。`TypedExpr::children` / `children_mut` に新 IR を追加 |
| polymorph | `Classes::collect` の Eq/Ord signature、`Classes::instances` の結果確認、`Specializer::operator_call`、`intrinsic_function` |
| control | pattern constant equality は `Binary(Equal, ...)` のままで、所有権で非消費になる |
| closures | `BorrowOperand` の子を lower。捕捉は作らない |
| recursion | `BorrowOperand` の子を走査。`Classes::recursion_targets` は維持 |
| ownership | 比較 `Binary` と `BorrowOperand` を `Use::Read` として扱う。一時 loan を call 後に終える |
| call_specialization | 比較を read-only として扱う。新 IR を all_children に含める |
| llvm | intrinsic operator は直接 lowering 維持。user instance comparison call は `prepare_comparison_borrow` で場所または entry-block 一時 slot を借用し、左から右に cleanup |
| llvm_frame | `frame_value` の SSA value + `Frame` metadata を使って temporary を評価する。slot pointer は `llvm.rs` 側で entry-block alloca を作って用意する |
| runtime | 変更なし。D02 の `@tz.string.compare` は D02 側で追加 |
| driver / main | 変更なし |

## 実装手順

1. **型クラス signature の変更**
   - `Classes::collect` で `Eq`/`Ord` method parameters を `&'a`, `&'a` にする。
   - `tests/polymorphism.rs` に `let eq: &i64 -> &i64 -> bool = Eq.eq` を追加し、旧 `i64 -> i64 -> bool` が `E1003` になることを確認する。
   - 確認: 既存の `Add.add` / `Mul.mul` method value tests は型を変えず通る。
2. **intrinsic method wrapper**
   - `Specializer::intrinsic_function` で borrowed comparison parameters を dereference して intrinsic `Binary` を作る。
   - 確認: `Eq.eq (&x) (&y)`、`Ord.lt (&x) (&y)` が scalar/string で通り、operator codegen は wrapper を呼ばない。
3. **operator lowering**
   - `BorrowOperand`（または同等の internal borrowed operand）を追加する。
   - `Specializer::operator_call` で user `Eq`/`Ord` operator だけ borrowed operands を渡す。
   - 確認: `record R { s: string }` の user `Eq R` で `r == r` が型検査を通る。
4. **所有権**
   - `E::Binary` の比較を全型 `Use::Read` にする。
   - `BorrowOperand` の loan と一時値寿命を実装する。
   - 確認: 比較後に左右の非 Copy 値を使える。比較中に右 operand が左所有者を変更する例は `E1014`。
5. **LLVM cleanup**
   - borrowed operand materialization を `prepare_comparison_borrow` で実装する。場所は `place(expr)`、temporary は `frame_value` の SSA value を entry-block `slot` に store してその slot を借用する。
   - call 後 cleanup は左から右。
   - 確認: native heap tracking で record/string 比較 loop が `live == 0`、二重 free なし。
6. **call specialization / tail recursion 回帰**
   - `read_only` を全比較へ一般化し、新 IR の traversal を追加する。
   - 確認: `tests/call_specialization.rs` の既存ケースと、比較を含む callback が clone/drop を増やさない代表 IR。
7. **fixtures / docs**
   - polymorphism fixture に non-Copy `Eq`/`Ord` record、method value、評価順序、再帰検査を追加。
   - `docs/language.md` の該当文と class table を更新する。

## テスト計画

### Rust: `tests/polymorphism.rs`

受理:

```text
record Box { value: string }

instance Eq Box {
    fn eq left right = left.value == right.value
    fn ne left right = !(Eq.eq left right)
}

def main :: bool
fn main = {
    let box = Box { value: "x" };
    let same = box == box;
    same && box.value.length == 1
}
```

```text
record Pair { left: string, right: string }

instance Ord Pair {
    fn lt a b = a.left < b.left || (a.left == b.left && a.right < b.right)
    fn le a b = !(Ord.lt b a)
    fn gt a b = Ord.lt b a
    fn ge a b = !(Ord.lt a b)
}
```

```text
def method_value :: Eq 'a => &'a -> &'a -> bool
fn method_value left right =
    let f: &'a -> &'a -> bool = Eq.eq;
    f left right
```

拒否:

| プログラム | 期待 |
|---|---|
| `let f: string -> string -> bool = Eq.eq` | `E1003` |
| `record R { s: string } instance Eq R { fn eq left right = left == right; fn ne left right = !(Eq.eq left right) }` | `E1005` (`left`/`right` are `&R`; operators do not autoderef references, so this asks for `Eq (&R)`) |
| `instance Eq R { fn eq a b = consume a == consume b ... }` where `consume : R -> i64` | `E1003` |
| `instance Eq R { fn eq a b = consume (*a) == consume (*b) ... }` for non-Copy `R` | `E1012` |
| `let mut x = R { ... }; x == { x = R { ... }; x }` | `E1014` |
| recursive `Eq` instance method without `rec` | `E1019` |

IR:

- `fn eq_i64(x: i64, y: i64) -> bool { x == y }` の IR は wrapper call や `alloca` 経由の参照引数を含まず、A11 前と同じ `icmp` 形。
- `fn lt_f64(x: f64, y: f64) -> bool { x < y }` は `fcmp olt` で fast-math flags なし。
- `fn eq_string(x: string, y: string) -> bool { x == y }` は `@tz.string.equal` を直接呼ぶ。
- user `Eq Box` の `box == box` は `$instance` method call になり、operand materialization の一時 drop が call 後にある。

### Rust: `tests/types_ownership.rs`

追加受理:

```text
record R { s: string }
instance Eq R {
    fn eq a b = a.s == b.s
    fn ne a b = !(Eq.eq a b)
}
fn f() -> string {
    let r = R { s: "x" };
    let _ = r == r;
    r.s
}
```

追加拒否:

```text
record R { s: string }
instance Eq R {
    fn eq a b = a.s == b.s
    fn ne a b = !(Eq.eq a b)
}
fn f() -> bool {
    let r = R { s: "x" };
    r == { let moved = r; moved }
}
```

期待 `E1014`。左 operand の read loan が右 operand 評価中に保持されることを確認する。

### fixtures / E2E

`tests/fixtures/polymorphism/BorrowedComparisons.tz` を追加する。

内容:

- `record Box { value: string }` の `Eq`。
- `record Key { name: string, rank: i64 }` の `Ord`。
- `export def compare_loop :: i64 -> i64`:
  - `new [Box](n, i -> Box { value: ... })` 相当の owned records を作る。
  - loop 内で同じ record を複数回比較し、比較後に `.value.length` を読む。
  - checksum を返す。
- `export def method_value_scalar :: i64 -> i64 -> bool` は `Eq.eq` method value を `&i64` で呼ぶ。
- `export def method_value_record :: bool` は user `Eq Box` の method value を `&Box` で呼ぶ。
- `export def ordered :: i64` は `tick (&mut counter)` を左右 operand に使い、評価順序が左→右であることを checksum で確認する。ただし同じ owner を比較中に変更するケースは拒否テストへ分離する。

Node E2E:

- native `-O0` / `-O3`。
- WASM `-O0` / `-O3`。
- IR 2 回出力が一致。
- `@malloc`/`@free` を tracking 版へ置換し、各 exported call 後 `live == 0`。
- WASM imports は空。
- record/string 比較を 10,000 回以上繰り返し、leak / double free / stack growth がないことを確認する。

### 検証コマンド

GUIDE §3 に従い、変更した Rust test target を個別に実行し、各コマンドの出力で **0 tests** になっていないことを確認する。filter 名を追加する場合も同様で、0 件なら filter を直すか target 全体を実行する。

```sh
cargo test --locked --test polymorphism
cargo test --locked --test types_ownership
cargo test --locked --test call_specialization
```

E2E は release binary を使うため、Node の前に必ず release build を作り直す。

```sh
cargo build --release --locked
node tests/e2e.mjs target/release/tsuzuri
```

fixture を別の Node suite に接続した場合は、その suite も `cargo build --release --locked` の後に実行する。例:

```sh
node tests/primitives.mjs target/release/tsuzuri
```

### docs examples

`examples/polymorphism` に `Eq Point` または `Eq Box` を追加し、`point == point` 後に `point` を使う例を入れる。

## ドキュメント

- `docs/language.md`
  - 組み込みクラス表の `Eq`/`Ord` method を `&'a -> &'a -> bool` に変更。
  - 「演算クラスのメソッドは値を受け取り...」「`Eq.eq` では所有権を渡す」文を、比較は借用・算術は値受け取りへ書き換える。
  - Ownership / Borrowing 節に「比較演算子は全型で非消費。operand の loan は比較式の末尾で終わる」を追加。
  - `string` 節の `==`/`!=` 特例説明を「比較全体の規則の一例」に変更。D02 後の `string Ord` も非消費だが direct intrinsic であることを記載。
- `docs/architecture.md`
  - 多相性 / 所有権 / LLVM の不変条件に、`Eq`/`Ord` は borrowed signatures、operator surface は `T op T`、user instance lowering は borrowed call、intrinsic operator lowering は direct のまま、を追加。
  - `prepare_comparison_borrow`、`frame_value` の SSA value + `Frame` metadata、entry-block temporary slot、一時値の左から右 cleanup を記載。
- `README.md`
  - 型クラス例に borrowed `Eq`/`Ord` の短い説明を追加。

## 受け入れ条件

- [ ] `Eq.eq`/`Eq.ne`/`Ord.lt`/`le`/`gt`/`ge` の class signature が `&'a -> &'a -> bool`。
- [ ] `==`/`!=`/`<`/`<=`/`>`/`>=` は全型で operand を消費しない。
- [ ] user `Eq`/`Ord` instance operator call は borrowed operands を渡し、comparison 後に一時値を左から右へ解放する。
- [ ] source-level `&temporary` は引き続き拒否され、operator 内部一時借用だけが許可される。
- [ ] intrinsic numeric/bool/unit/string/char comparisons は direct `icmp`/`fcmp`/`@tz.string.equal`/D02 の `@tz.string.compare` を維持し、scalar operator IR に参照 traffic を増やさない。
- [ ] `Eq.eq`/`Ord.lt` method value は `&T -> &T -> bool` として使える。
- [ ] non-Copy record/string を含む値の比較 loop が native/WASM × `-O0`/`-O3` で leak/double free なし。
- [ ] 比較中の借用競合は既存 `E1014`、borrowed parameter の誤用は既存 `E1003`/`E1012` で安定する。
- [ ] A06/A07/C04/C06 の ticket が A11 を hard dependency として参照し、比較のためだけの `Copy` 要求を残さない。

## 落とし穴

- `Checker::binary_type` で operand type を `&T` に変えると surface type と制約推論が壊れる。借用化は lowering/ownership の責務。
- source-level `Borrow` をそのまま一時値許可へ広げると、`&"temporary"` が受理されて既存仕様を変えてしまう。compiler-generated borrow と source borrow を区別する。
- intrinsic operator を method wrapper call へ変えると scalar IR が悪化し、`Add.add` 等との性能経路一貫性にも影響する。operator は direct lowering のまま。
- user instance method body の `left.x` は autoderef で通るが、`left` 自体を値として使うコードは壊れる。migration guidance と診断テストを用意する。
- `BorrowOperand` の cleanup を通常 scope drop へ任せると右から左になる。A11 では既存 string equality と同じ左から右 cleanup に固定する。
- `call_specialization::read_only` を更新しないと、比較しかしていない captured 値が consume 扱いになり、不要な clone/drop や最適化漏れが起きる。
- recursion check は単相化前に走る。comparison を早く `Call` へ置換しすぎると instance method の再帰検出を漏らす。

## 対象外

- 算術・ビット演算 method の借用化。
- `Clone` class の導入。
- `Eq`/`Ord` の law 検査、総順序 float、NaN の total ordering。
- A06 の条件付き instance / superclass / default method の実装。
- A07 の deriving 実装。
- C04/C06 の API 実装。
- source-level temporary borrow の一般解禁、寿命延長、名前付き lifetime。

## 未決事項

- **決定済み（2026-09-23 承認、GUIDE D-20）:** 比較演算は全型で非消費とし、`Eq`/`Ord` のメソッドは借用を受け取る。言語仕様の変更として `docs/language.md` を更新する。

- **既定案: compiler-internal IR は `BorrowOperand(Box<TypedExpr>)` を追加する。** 既存 `Borrow` に flag を足す案も可能だが、source-level temporary borrow の誤許可を避けるため別 variant を既定にする。
- **既定案: borrowed operand cleanup は左から右。** 既存 `string` equality の `release_operand(left)` → `release_operand(right)` と同じにする。
- **既定案: intrinsic method value wrapper は参照を dereference して direct `Binary` を使う。** operator lowering では wrapper を使わないため、scalar operator IR は unchanged を受け入れ条件にする。
- 台帳の見直し提案: なし。GUIDE D-20 に従う。
