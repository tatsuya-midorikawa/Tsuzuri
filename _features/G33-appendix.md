# G33 付録 A・B: 検証の方法と、OWN・NUM・EVL・CTL・TYP の規則 128 件

本文: [G33-spec-rule-ids-conformance.md](G33-spec-rule-ids-conformance.md)。残り 10 領域は [G33-appendix-areas.md](G33-appendix-areas.md)、ハーネスの詳細は [G33-appendix-harness.md](G33-appendix-harness.md)。

## 付録 A: 検証の方法と、行からファイルへの変換

### A.1 検証の方法（実機で確かめた規則だけを載せた）

1. 候補の規則（273 件）を、`docs/language.md` と言語リファレンスの「この記事のポイント」・表・例から選ぶ。
2. 規則ごとに最小のプログラム（`Main.tz`。必要なら補助ファイル）を書き、`target/release/tsuzuri`（`9dd0fbbc` のビルド。macOS arm64）で実行する。コマンドは本文 §5 の表のとおり: `check DIR --json`（診断は stderr の JSON Lines）、`run DIR -O0`・`-O3`、`test DIR --json --target native|wasm32 -O0|-O3`、cli は `check`・`run`・`build`・`new`・`fmt --check`・`api-diff`・`test --list`・`test --filter`。
3. 実機の結果が文書の 1 文と一致したものだけを、ID・文・出典・テストの本体と期待にして載せた。一致しないもの・1 文で言い切れないもの・確かめきれなかったものは載せていない。
4. 載せた 228 件を、本文 §3 の手順で**ディレクティブつきの `.tz`**にして、本文 §4〜§6 を実装した試作のハーネスに通した。結果は `tests: 228, failed: 0`。所要時間は `--jobs 4` で 3 回とも 3 分 42 秒〜7 分 6 秒（他のビルドや検査と同時だと遅い。最後の全件の実行は 5 分 44 秒）。
5. 出典は 43 ファイル（言語リファレンス 163 件（40 ページ）・`docs/language.md` 63 件・`docs/stability.md` 1 件・`docs/architecture.md` 1 件）。`§` つきの 79 件が 24 組の（ファイル，見出し）を引き、見出しはすべて実在し、そのファイルで**完全一致で 1 つだけ**ある（Doc の `#slug` の元）。`§` のない 149 件はページ単位の出典で、Doc に `#slug` を付けない。

期待の種類の内訳（行は領域。`cli` は cli モード、`det` は cli モードの `deterministic`）:

| 領域 | check-ok | check-error | check-warning | stdout | trap | trap + stdout | tests-pass | cli | det | 計 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| OWN | 0 | 18 | 0 | 12 | 0 | 0 | 0 | 0 | 0 | 30 |
| NUM | 0 | 1 | 0 | 4 | 4 | 0 | 21 | 0 | 0 | 30 |
| EVL | 0 | 0 | 0 | 19 | 0 | 1 | 0 | 0 | 0 | 20 |
| CTL | 0 | 7 | 1 | 14 | 4 | 0 | 0 | 0 | 0 | 26 |
| TYP | 0 | 12 | 0 | 10 | 0 | 0 | 0 | 0 | 0 | 22 |
| LEX | 0 | 5 | 0 | 5 | 0 | 0 | 0 | 0 | 0 | 10 |
| SYN | 0 | 6 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 10 |
| STR | 0 | 1 | 0 | 8 | 1 | 0 | 0 | 0 | 0 | 10 |
| MOD | 0 | 6 | 0 | 4 | 0 | 0 | 0 | 0 | 0 | 10 |
| TSK | 0 | 3 | 0 | 7 | 0 | 0 | 0 | 0 | 0 | 10 |
| DIA | 0 | 1 | 1 | 0 | 0 | 0 | 0 | 8 | 0 | 10 |
| CLI | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 9 | 1 | 10 |
| ABI | 0 | 2 | 0 | 0 | 0 | 0 | 0 | 8 | 0 | 10 |
| STD | 0 | 0 | 0 | 10 | 0 | 0 | 0 | 0 | 0 | 10 |
| EDN | 2 | 5 | 0 | 0 | 0 | 0 | 0 | 3 | 0 | 10 |
| 計 | 2 | 67 | 2 | 97 | 9 | 1 | 21 | 28 | 1 | 228 |

WASM を通るのは `tests-pass` の 21 件（すべて NUM）だけ。`stdout`・`trap` は native の `-O0`・`-O3` だけ。

### A.2 行からファイルへ（本文 §3 の手順の例）

表の列は「ID・規則の文・出典・`本体` → `ディレクティブ`」。`⏎` は改行、`LR/` は `_tsuzuri/language-reference/`、`(empty)` は空の本体、`[名前]⏎` は複数ファイルの始まり。ディレクティブの項目は ` ⏎ ` で分かれ、各項目の先頭に `// ` を付けて、`// rule: <ID>` の次に並べる。

**例 1: 1 ファイル**（OWN-001）。`tests/conformance/OWN/OWN-001-let-binding-from-non.tz`:

```
let first = "data"
let second = first
first.length

// rule: OWN-001
// expect: check-error E1012 line=3
```

**例 2: 複数ファイル**（MOD-002）。`[Main.tz]⏎…⏎[Geometry.tz]⏎…` なのでディレクトリ形。`tests/conformance/MOD/MOD-002-public-member-must-be/Main.tz` の末尾にディレクティブ、同じ場所に `Geometry.tz`:

```
Geometry.area 3 4

// rule: MOD-002
// expect: stdout "12\n"
```

```
def area :: i64 -> i64 -> i64 = \w h -> w * h
```

**例 3: `Main.tz` が行にない・本体が空**（CLI-010 と CLI-008）。`Main.tz` は本体なしで、ディレクティブだけを置く。`CLI-010` は `old/Lib.tz` と `new/Lib.tz` も同じディレクトリに置く:

```
// rule: CLI-010
// cli: api-diff $DIR/old $DIR/new
// expect: exit 1
// expect: stdout-has "two"
```

### A.3 食い違いに当たったとき

- 文書の 1 文と実機の結果が違う → その規則は載せない（足さない）。報告に、候補の文・出典（ファイルと見出し）・実行したコマンドと出力を書く。文書を直すか、コンパイラのバグかは別の作業。
- 同じプログラムが `-O0` と `-O3`、または native と WASM で違う → コンパイラのバグの候補。テストを弱めずに報告する。
- 付録の行どおりに作ったテストが落ちる → 本文の停止条件 1。先に、行の写し間違い（`⏎` の位置・`line=` の数え方・`\n` のエスケープ）を疑う。

## 付録 B: OWN・NUM・EVL・CTL・TYP の規則（128 件）

各行はすべて実機で確かめた。出典の `§` の見出しは完全一致。`line=N` はファイルの行（本体の行番号）。

### B.1 OWN（所有権と借用。30 件）

| ID | 規則 | 出典 | テスト（本体 → ディレクティブ） |
| --- | --- | --- | --- |
| OWN-001 | A `let` binding from a non-Copy value (such as a `string`) MUST move the value, and a later read of the source binding MUST be rejected with E1012. | LR/ownership-and-memory/ownership.md §ムーブ後の使用エラー（E1012） | `let first = "data"⏎let second = first⏎first.length` → `expect: check-error E1012 line=3` |
| OWN-002 | A `let` binding from a Copy value (such as an `i64`) MUST NOT move the source, so both bindings stay readable. | docs/language.md §Ownership / Borrowing | `let a = 5⏎let b = a⏎a + b` → `expect: stdout "10\n"` |
| OWN-003 | Passing a non-Copy value to a by-value parameter MUST move it into the callee; a later read in the caller MUST be rejected with E1012. | docs/language.md §Ownership / Borrowing | `def take :: string -> i64 = \s -> s.length⏎let s = "abc"⏎let n = take s⏎s.length` → `expect: check-error E1012 line=4` |
| OWN-004 | Passing an owned value to a `ref T` parameter MUST borrow it implicitly without moving it. | docs/language.md §呼び出し引数の暗黙の借用 | `def length_of :: ref string -> i64 = \s -> s.length⏎let s = "abc"⏎let n = length_of s⏎n + s.length` → `expect: stdout "6\n"` |
| OWN-005 | Any number of shared borrows (`ref T`) of the same owner MAY be live at the same time. | LR/ownership-and-memory/borrowing.md §共有借用（ref） | `def both :: ref string -> ref string -> i64 = \a b -> a.length + b.length⏎let s = "abcd"⏎both s s` → `expect: stdout "8\n"` |
| OWN-006 | An exclusive borrow (`ref mut T`) MUST be rejected with E1014 while a shared borrow of the same owner is still used later. | LR/ownership-and-memory/borrowing.md §借用の競合と不変条件（E1014） | `def replace :: ref mut string -> unit = \text -> { deref text = "next"; }⏎def main2 :: unit -> i64 = \() -> {⏎    let mut text = "abc";⏎    let shared = ref text;⏎    replace ref mut text;⏎    shared.length⏎}` → `expect: check-error E1014 line=5` |
| OWN-007 | A borrow MUST end after its last use, so the owner is usable again afterwards. | docs/language.md §Ownership / Borrowing | `def length :: ref string -> i64 = \text -> text.length⏎def identity :: ref string -> ref string = \text -> text⏎def replace :: ref mut string -> unit = \text -> { deref text = "next"; }⏎def example :: unit -> string = \() -> {⏎    let mut text = "before";⏎    let shared = identity text;⏎    let size = length shared;⏎    replace text;⏎    let owned = text;⏎    owned⏎}⏎example ()` → `expect: stdout "next\n"` |
| OWN-008 | A value moved on any path MUST NOT be used after the paths join (E1012). | LR/ownership-and-memory/ownership.md §ムーブ後の使用エラー（E1012） | `def take :: string -> i64 = \s -> s.length⏎def f :: bool -> i64 = \flag -> {⏎    let s = "abc";⏎    if flag then { take s } else { 0 };⏎    s.length⏎}⏎f true` → `expect: check-error E1012 line=5` |
| OWN-009 | Moving one field out of a record MUST leave the other fields usable. | docs/language.md §Ownership / Borrowing | `record Pair { left: string, right: string }⏎let p = Pair { left: "ab", right: "cde" }⏎let l = p.left⏎l.length + p.right.length` → `expect: stdout "5\n"` |
| OWN-010 | Moving a non-Copy value out through a reference MUST be rejected with E1012. | docs/language.md §呼び出し引数の暗黙の借用 | `def steal :: ref string -> string = \r -> deref r⏎0` → `expect: check-error E1012 line=1` |
| OWN-011 | An immutable `let` binding MUST NOT be reassigned. | LR/values-and-functions/values.md | `let x = 1⏎x = 2⏎x` → `expect: check-error E1014 line=2` |
| OWN-012 | A record whose fields are all Copy MUST be Copy, so a `let` from it does not move. | docs/language.md §Ownership / Borrowing | `record Point { x: i64, y: i64 }⏎let p = Point { x: 3, y: 4 }⏎let q = p⏎p.x + q.y` → `expect: stdout "7\n"` |
| OWN-013 | A record with a non-Copy field MUST NOT be Copy, so a `let` from it moves. | docs/language.md §Ownership / Borrowing | `record Named { name: string, id: i64 }⏎let p = Named { name: "a", id: 1 }⏎let q = p⏎p.id` → `expect: check-error E1012 line=4` |
| OWN-014 | A function MUST NOT return a reference to a local owner (E1013). | LR/ownership-and-memory/lifetimes.md §ライフタイムの基本原則 | `def bad :: unit -> ref string = \() -> {⏎    let s = "abc";⏎    ref s⏎}⏎0` → `expect: check-error E1013 line=3` |
| OWN-015 | Moving a value inside a loop body MUST be rejected when the value is used again in a later iteration (E1012). | LR/loops-and-conditionals/while.md §所有権とメモリ管理 | `def take :: string -> i64 = \s -> s.length⏎let s = "abc"⏎for i in 0 .. 2 do⏎    let n = take s⏎0` → `expect: check-error E1012 line=4` |
| OWN-016 | Two exclusive borrows of the same owner MUST NOT be live at once (E1014). | LR/ownership-and-memory/borrowing.md §排他借用（ref mut）と排他性規則 | `def both :: ref mut i64 -> ref mut i64 -> unit = \a b -> { deref a = 1; deref b = 2; }⏎def f :: unit -> i64 = \() -> {⏎    let mut n = 1;⏎    both (ref mut n) (ref mut n);⏎    n⏎}⏎f ()` → `expect: check-error E1014 line=4` |
| OWN-017 | A function taking `ref mut T` MUST be able to update the caller's `let mut` owner through `deref`. | LR/ownership-and-memory/borrowing.md §排他借用（ref mut）と排他性規則 | `def inc :: ref mut i64 -> unit = \r -> { deref r = deref r + 1; }⏎def f :: unit -> i64 = \() -> {⏎    let mut n = 1;⏎    inc ref mut n;⏎    inc ref mut n;⏎    n⏎}⏎f ()` → `expect: stdout "3\n"` |
| OWN-018 | The `+` operator on two strings MUST consume its left operand. | LR/literals-and-strings/strings.md §所有権 | `let a = "x"⏎let b = a + "y"⏎a.length` → `expect: check-error E1012 line=3` |
| OWN-019 | A shared reference `ref T` MUST be Copy, so one reference MAY be bound to several names. | docs/language.md §Ownership / Borrowing | `let s = "abcd"⏎let r = ref s⏎let a = r⏎let b = r⏎a.length + b.length` → `expect: stdout "8\n"` |
| OWN-020 | A shared borrow MUST prevent moving its owner while the borrow is still used later (E1014). | LR/ownership-and-memory/borrowing.md | `let s = "abc"⏎let r = ref s⏎let t = s⏎r.length` → `expect: check-error E1014 line=3` |
| OWN-021 | Assigning to an owner MUST be rejected while a borrow of it is still used later (E1014). | LR/ownership-and-memory/borrowing.md | `let mut s = "abc"⏎let r = ref s⏎s = "xyz"⏎r.length` → `expect: check-error E1014 line=3` |
| OWN-022 | A variable moved in a loop MUST be usable again after it is assigned a new value before the next iteration. | LR/loops-and-conditionals/while.md §所有権とメモリ管理 | `def take :: string -> i64 = \s -> s.length⏎let mut s = "abc"⏎let mut total = 0⏎for i in 0 .. 2 do⏎    total = total + take s⏎    s = "de"⏎total` → `expect: stdout "7\n"` |
| OWN-023 | A `Drop` instance MUST run once when the owner leaves its scope, and owners MUST be dropped in reverse order of declaration. | LR/ownership-and-memory/drop.md | `record Ticket { id: i64 }⏎⏎instance Drop<Ticket> {⏎    fn drop value = do! IO.write_line ("drop " + to_string value.id)⏎}⏎⏎def main :: unit -> i32 = \() ->⏎    let first = Ticket { id: 1 }⏎    let second = Ticket { id: 2 }⏎    do! IO.write_line (to_string (first.id + second.id))⏎    0` → `expect: stdout "3\ndrop 2\ndrop 1\n"` |
| OWN-024 | A value moved to another owner MUST NOT be dropped by its original owner. | LR/ownership-and-memory/drop.md | `record Ticket { id: i64 }⏎⏎instance Drop<Ticket> {⏎    fn drop value = do! IO.write_line ("drop " + to_string value.id)⏎}⏎⏎def consume :: Ticket -> unit = \t -> ()⏎⏎def main :: unit -> i32 = \() ->⏎    let t = Ticket { id: 7 }⏎    consume t⏎    do! IO.write_line "after"⏎    0` → `expect: stdout "drop 7\nafter\n"` |
| OWN-025 | Assigning a new value to a `let mut` owner MUST drop the old value immediately. | LR/ownership-and-memory/drop.md | `record Ticket { id: i64 }⏎⏎instance Drop<Ticket> {⏎    fn drop value = do! IO.write_line ("drop " + to_string value.id)⏎}⏎⏎def main :: unit -> i32 = \() ->⏎    let mut t = Ticket { id: 1 }⏎    t = Ticket { id: 2 }⏎    do! IO.write_line "assigned"⏎    0` → `expect: stdout "drop 1\nassigned\ndrop 2\n"` |
| OWN-026 | A type with a `Drop` instance MUST NOT be Copy, so a `let` from it moves. | docs/language.md §利用者定義の解放（Drop） | `record Ticket { id: i64 }⏎⏎instance Drop<Ticket> {⏎    fn drop value = do! IO.write_line "drop"⏎}⏎⏎let a = Ticket { id: 1 }⏎let b = a⏎a.id` → `expect: check-error E1012 line=9` |
| OWN-027 | User code MUST NOT call `Drop.drop` directly (E1016). | LR/ownership-and-memory/drop.md | `record Ticket { id: i64 }⏎⏎instance Drop<Ticket> {⏎    fn drop value = do! IO.write_line "drop"⏎}⏎⏎def f :: unit -> unit = \() -> {⏎    let mut t = Ticket { id: 1 };⏎    Drop.drop (ref mut t)⏎}⏎0` → `expect: check-error E1016 line=9` |
| OWN-028 | A reference MUST NOT be bound to a `let` with a `ref` annotation by implicit borrowing; implicit borrowing applies to call arguments only. | docs/language.md §呼び出し引数の暗黙の借用 | `let s = "abc"⏎let r: ref string = s⏎r.length` → `expect: check-error E1003 line=2` |
| OWN-029 | Passing a reference to a parameter that takes a non-Copy value by value MUST be rejected (E1012). | docs/language.md §呼び出し引数の暗黙の借用 | `def take :: string -> i64 = \s -> s.length⏎def f :: ref string -> i64 = \r -> take r⏎0` → `expect: check-error E1012 line=2` |
| OWN-030 | A tuple that contains a non-Copy element MUST NOT be Copy, so a `let` from it moves. | docs/language.md §Ownership / Borrowing | `let p = ("ab", 1)⏎let q = p⏎match p with⏎\| (s, _) -> s.length` → `expect: check-error E1012 line=3` |

### B.2 NUM（数値。30 件。21 件の `tests-pass` は WASM の 4 組も通る）

| ID | 規則 | 出典 | テスト（本体 → ディレクティブ） |
| --- | --- | --- | --- |
| NUM-001 | An integer literal without a suffix and without any typing context MUST have type `i32`. | LR/types-and-type-inference/type-inference.md §文脈が決まらない場合の既定型 | `let x = 2147483647⏎x + 1` → `expect: stdout "-2147483648\n"` |
| NUM-002 | Integer `+` MUST wrap modulo 2^width. | docs/language.md §数値仕様 | `def add32 :: i32 -> i32 -> i32 = \a b -> a + b⏎def add8 :: i8 -> i8 -> i8 = \a b -> a + b⏎⏎test "i32 max + 1" = Test.equal (ref (-2147483648)) (ref (add32 2147483647 1))⏎test "i8 max + 1" = Test.equal (ref (-128)) (ref (add8 127 1))` → `expect: tests-pass 2` |
| NUM-003 | Integer `-` MUST wrap modulo 2^width. | docs/language.md §数値仕様 | `def sub8 :: i8 -> i8 -> i8 = \a b -> a - b⏎def sub8u :: i8u -> i8u -> i8u = \a b -> a - b⏎⏎test "i8 min - 1" = Test.equal (ref 127) (ref (sub8 (-128) 1))⏎test "i8u 0 - 1" = Test.equal (ref 255i8u) (ref (sub8u 0i8u 1i8u))` → `expect: tests-pass 2` |
| NUM-004 | Integer `*` MUST wrap modulo 2^width. | docs/language.md §数値仕様 | `def mul32 :: i32 -> i32 -> i32 = \a b -> a * b⏎⏎test "65536 * 65536" = Test.equal (ref 0) (ref (mul32 65536 65536))⏎test "i32 max * 2" = Test.equal (ref (-2)) (ref (mul32 2147483647 2))` → `expect: tests-pass 2` |
| NUM-005 | Unary minus of the minimum signed value MUST wrap to the same value. | docs/language.md §数値仕様 | `def neg :: i32 -> i32 = \a -> -a⏎⏎test "neg i32 min" = Test.equal (ref (-2147483648)) (ref (neg (-2147483648)))⏎test "neg 5" = Test.equal (ref (-5)) (ref (neg 5))` → `expect: tests-pass 2` |
| NUM-006 | Integer `/` MUST truncate toward zero. | docs/language.md §数値仕様 | `def div :: i64 -> i64 -> i64 = \a b -> a / b⏎⏎test "-7 / 2" = Test.equal (ref (-3)) (ref (div (-7) 2))⏎test "7 / -2" = Test.equal (ref (-3)) (ref (div 7 (-2)))⏎test "-7 / -2" = Test.equal (ref 3) (ref (div (-7) (-2)))` → `expect: tests-pass 3` |
| NUM-007 | Integer `%` MUST take the sign of the dividend. | docs/language.md §数値仕様 | `def rem :: i64 -> i64 -> i64 = \a b -> a % b⏎⏎test "-7 % 2" = Test.equal (ref (-1)) (ref (rem (-7) 2))⏎test "7 % -2" = Test.equal (ref 1) (ref (rem 7 (-2)))⏎test "-7 % -2" = Test.equal (ref (-1)) (ref (rem (-7) (-2)))` → `expect: tests-pass 3` |
| NUM-008 | Integer division by zero MUST trap. | docs/language.md §数値仕様 | `def div :: i64 -> i64 -> i64 = \a b -> a / b⏎⏎let zero = 0⏎div 10 zero` → `expect: trap kind=div_zero` |
| NUM-009 | Signed integer division of the minimum value by -1 MUST trap. | docs/language.md §数値仕様 | `def div :: i64 -> i64 -> i64 = \a b -> a / b⏎⏎let minimum = -9223372036854775807l - 1l⏎div minimum (-1)` → `expect: trap kind=div_overflow` |
| NUM-010 | Signed integer remainder of the minimum value by -1 MUST trap. | docs/language.md §数値仕様 | `def rem :: i64 -> i64 -> i64 = \a b -> a % b⏎⏎let minimum = -9223372036854775807l - 1l⏎rem minimum (-1)` → `expect: trap kind=div_overflow` |
| NUM-011 | A shift count MUST be masked with `bit_width - 1`. | docs/language.md §数値仕様 | `def shl :: i32 -> i32 -> i32 = \a n -> a <<< n⏎⏎test "1 <<< 33 on i32" = Test.equal (ref 2) (ref (shl 1 33))⏎test "1 <<< 32 on i32" = Test.equal (ref 1) (ref (shl 1 32))` → `expect: tests-pass 2` |
| NUM-012 | `>>>` MUST be an arithmetic shift on signed integers and a logical shift on unsigned integers. | docs/language.md §数値仕様 | `def sar :: i32 -> i32 -> i32 = \a n -> a >>> n⏎def shr :: i8u -> i8u -> i8u = \a n -> a >>> n⏎⏎test "signed" = Test.equal (ref (-4)) (ref (sar (-8) 1))⏎test "unsigned" = Test.equal (ref 100i8u) (ref (shr 200i8u 1i8u))` → `expect: tests-pass 2` |
| NUM-013 | `**` MUST be right-associative. | LR/values-and-functions/op-and-expressions.md | `let a = 2 ** 3 ** 2⏎a` → `expect: stdout "512\n"` |
| NUM-014 | Unary minus MUST bind tighter than `**`, so `-2 ** 2` is `(-2) ** 2`. | LR/values-and-functions/op-and-expressions.md | `let b = -2 ** 2⏎b` → `expect: stdout "4\n"` |
| NUM-015 | An integer `**` with a negative exponent MUST trap. | docs/language.md §数値仕様 | `def pow :: i64 -> i64 -> i64 = \a n -> a ** n⏎⏎let n = -1⏎pow 2 n` → `expect: trap kind=numeric_runtime` |
| NUM-016 | The bitwise operators `&&&`, `\|\|\|`, `^^^` and `~~~` MUST work on the bits of the operand type. | LR/values-and-functions/op-and-expressions.md | `def bits :: i32 -> i32 -> i32 = \a b -> ((a &&& b) \|\|\| 8) ^^^ 1⏎def inv :: i32 -> i32 = \a -> ~~~a⏎⏎test "and or xor" = Test.equal (ref 11) (ref (bits 6 3))⏎test "not" = Test.equal (ref (-11)) (ref (inv 10))` → `expect: tests-pass 2` |
| NUM-017 | There MUST be no implicit conversion between integer types of different widths. | LR/types-and-type-inference/cast.md | `let x: i64 = 10i32⏎x` → `expect: check-error E1003 line=1` |
| NUM-018 | Narrowing integer `as` MUST keep the low bits. | LR/types-and-type-inference/cast.md | `def narrow :: i64 -> i8 = \a -> a as i8⏎⏎test "255 as i8" = Test.equal (ref (-1)) (ref (narrow 255))⏎test "256 as i8" = Test.equal (ref 0) (ref (narrow 256))` → `expect: tests-pass 2` |
| NUM-019 | Widening integer `as` MUST sign-extend signed values and zero-extend unsigned values. | LR/types-and-type-inference/cast.md | `def wide :: i8 -> i64 = \a -> a as i64⏎def wideu :: i8u -> i64 = \a -> a as i64⏎⏎test "signed" = Test.equal (ref (-1)) (ref (wide (-1)))⏎test "unsigned" = Test.equal (ref 255) (ref (wideu 255i8u))` → `expect: tests-pass 2` |
| NUM-020 | Float to integer `as` MUST truncate toward zero. | LR/types-and-type-inference/cast.md | `def cut :: f64 -> i64 = \a -> a as i64⏎⏎test "3.99" = Test.equal (ref 3) (ref (cut 3.99))⏎test "-3.99" = Test.equal (ref (-3)) (ref (cut (-3.99)))` → `expect: tests-pass 2` |
| NUM-021 | Float to integer `as` MUST saturate at the minimum and maximum of the target type. | LR/types-and-type-inference/cast.md | `def cut :: f64 -> i8 = \a -> a as i8⏎⏎test "high" = Test.equal (ref 127) (ref (cut 1e30))⏎test "low" = Test.equal (ref (-128)) (ref (cut (-1e30)))` → `expect: tests-pass 2` |
| NUM-022 | A NaN converted to an integer by `as` MUST become 0. | LR/types-and-type-inference/cast.md | `def cut :: f64 -> i64 = \a -> a as i64⏎def nan :: f64 -> f64 = \z -> z / z⏎⏎test "nan" = Test.equal (ref 0) (ref (cut (nan 0.0)))` → `expect: tests-pass 1` |
| NUM-023 | Float division by zero MUST NOT trap; it MUST yield an infinity or NaN. | LR/types-and-type-inference/basic-types.md | `def div :: f64 -> f64 -> f64 = \a b -> a / b⏎⏎test "1/0" = Test.is_true (div 1.0 0.0 > 1e308)⏎test "-1/0" = Test.is_true (div (-1.0) 0.0 < -1e308)⏎test "0/0" = Test.is_true (div 0.0 0.0 != div 0.0 0.0)` → `expect: tests-pass 3` |
| NUM-024 | Every ordering and equality comparison with a NaN operand MUST be false, except `!=`, which MUST be true. | docs/language.md §数値仕様 | `def nan :: f64 -> f64 = \z -> z / z⏎⏎test "eq" = Test.is_true (not (nan 0.0 == nan 0.0))⏎test "ne" = Test.is_true (nan 0.0 != nan 0.0)⏎test "lt" = Test.is_true (not (nan 0.0 < 1.0))⏎test "ge" = Test.is_true (not (nan 0.0 >= 1.0))` → `expect: tests-pass 4` |
| NUM-025 | Negative zero MUST compare equal to zero and MUST keep its sign through division. | docs/language.md §数値仕様 | `def neg :: f64 -> f64 = \a -> -a⏎def div :: f64 -> f64 -> f64 = \a b -> a / b⏎⏎test "eq" = Test.is_true (neg 0.0 == 0.0)⏎test "sign survives" = Test.is_true (div 1.0 (neg 0.0) < 0.0)` → `expect: tests-pass 2` |
| NUM-026 | `f64` arithmetic MUST be IEEE 754 round-to-nearest-even without fused or reassociated operations. | docs/language.md §数値仕様 | `def add :: f64 -> f64 -> f64 = \a b -> a + b⏎⏎test "sum" = Test.is_true (add 0.1 0.2 == 0.30000000000000004)⏎test "not 0.3" = Test.is_true (add 0.1 0.2 != 0.3)` → `expect: tests-pass 2` |
| NUM-027 | Integer to float `as` MUST round to nearest, ties to even. | LR/types-and-type-inference/cast.md | `def conv :: i64 -> f64 = \a -> a as f64⏎⏎test "tie goes to even" = Test.is_true (conv 9007199254740993l == 9007199254740992.0)⏎test "next tie" = Test.is_true (conv 9007199254740995l == 9007199254740996.0)` → `expect: tests-pass 2` |
| NUM-028 | `f32` arithmetic MUST round to `f32` after each operation, without an `f64` intermediate. | docs/language.md §数値仕様 | `def add :: f32 -> f32 -> f32 = \a b -> a + b⏎⏎test "2^24 + 1" = Test.is_true (add 16777216f 1f == 16777216f)⏎test "2^24 + 2" = Test.is_true (add 16777216f 2f == 16777218f)` → `expect: tests-pass 2` |
| NUM-029 | An integer `+` inside `@checked` MUST raise `OverflowException` on overflow instead of wrapping. | LR/exception-handling/exception-handling.md | `def safe_add :: i32 -> i32 -> Result<i32, Exception> = \x y ->⏎    try⏎        @checked (x + y)⏎    with⏎    \| e is OverflowException -> e⏎⏎let text =⏎    match safe_add 2147483647 1 with⏎    \| Ok _ -> "ok"⏎    \| Error _ -> "overflow"⏎text` → `expect: stdout "overflow\n"` |
| NUM-030 | A suffix MUST fix the literal type: `l` is `i64`, `y` is `i8`, and `u` is `i32u`. | docs/language.md §数値仕様 | `def big :: i64 -> i64 = \a -> a⏎⏎test "l" = Test.equal (ref 3000000000) (ref (big 3000000000l))⏎test "y wraps as i8" = Test.equal (ref (-128)) (ref (127y + 1y))⏎test "u is unsigned" = Test.equal (ref 4294967295u) (ref (0u - 1u))` → `expect: tests-pass 3` |

### B.3 EVL（評価順序。20 件）

| ID | 規則 | 出典 | テスト（本体 → ディレクティブ） |
| --- | --- | --- | --- |
| EVL-001 | The operands of a binary operator MUST be evaluated left to right in source order, even when precedence groups the operators differently. | LR/values-and-functions/op-and-expressions.md | `def say :: i64 -> i64 = \n ->⏎    do! IO.write_line (to_string n)⏎    n⏎⏎say 1 + say 2 * say 3` → `expect: stdout "1\n2\n3\n7\n"` |
| EVL-002 | The arguments of a call MUST be evaluated left to right. | docs/language.md §呼び出し引数の暗黙の借用 | `def say :: i64 -> i64 = \n ->⏎    do! IO.write_line (to_string n)⏎    n⏎⏎def sum3 :: i64 -> i64 -> i64 -> i64 = \a b c -> a * 100 + b * 10 + c⏎sum3 (say 1) (say 2) (say 3)` → `expect: stdout "1\n2\n3\n123\n"` |
| EVL-003 | All arguments MUST be evaluated before the callee body runs. | docs/language.md §呼び出し引数の暗黙の借用 | `def say :: i64 -> i64 = \n ->⏎    do! IO.write_line (to_string n)⏎    n⏎⏎def body :: i64 -> i64 -> i64 = \a b ->⏎    do! IO.write_line "body"⏎    a + b⏎body (say 1) (say 2)` → `expect: stdout "1\n2\nbody\n3\n"` |
| EVL-004 | `&&` MUST NOT evaluate its right operand when the left operand is false. | LR/values-and-functions/op-and-expressions.md | `def say :: i64 -> i64 = \n ->⏎    do! IO.write_line (to_string n)⏎    n⏎⏎let r = false && (say 1 == 1)⏎r` → `expect: stdout "false\n"` |
| EVL-005 | `\|\|` MUST NOT evaluate its right operand when the left operand is true. | LR/values-and-functions/op-and-expressions.md | `def say :: i64 -> i64 = \n ->⏎    do! IO.write_line (to_string n)⏎    n⏎⏎let r = true \|\| (say 1 == 1)⏎r` → `expect: stdout "true\n"` |
| EVL-006 | A trapping operation in a short-circuited operand MUST NOT execute. | LR/values-and-functions/op-and-expressions.md | `let zero = 0⏎let safe_and = false && (1 / zero == 0)⏎let safe_or = true \|\| (1 / zero == 0)⏎$"{not safe_and} {safe_or}"` → `expect: stdout "true true\n"` |
| EVL-007 | `if` MUST evaluate only the selected branch. | LR/loops-and-conditionals/if.md | `def say :: i64 -> i64 = \n ->⏎    do! IO.write_line (to_string n)⏎    n⏎⏎let r = if say 1 == 1 then say 10 else say 20⏎r` → `expect: stdout "1\n10\n10\n"` |
| EVL-008 | `match` MUST evaluate its scrutinee exactly once, whatever the number of arms tried. | LR/pattern-matching/match.md | `def say :: i64 -> i64 = \n ->⏎    do! IO.write_line (to_string n)⏎    n⏎⏎let r =⏎    match say 3 with⏎    \| 1 -> 100⏎    \| 2 -> 200⏎    \| n -> n⏎r` → `expect: stdout "3\n3\n"` |
| EVL-009 | The elements of an array literal MUST be evaluated in written order. | docs/language.md §式と評価順序 | `def say :: i64 -> i64 = \n ->⏎    do! IO.write_line (to_string n)⏎    n⏎⏎let xs = [say 1, say 2, say 3]⏎xs.length` → `expect: stdout "1\n2\n3\n3\n"` |
| EVL-010 | The fields of a record literal MUST be evaluated in written order, not in declaration order. | docs/language.md §式と評価順序 | `def say :: i64 -> i64 = \n ->⏎    do! IO.write_line (to_string n)⏎    n⏎⏎record Pair { a: i64, b: i64 }⏎let p = Pair { b: say 1, a: say 2 }⏎p.a * 10 + p.b` → `expect: stdout "1\n2\n21\n"` |
| EVL-011 | The elements of a tuple MUST be evaluated left to right. | docs/language.md §式と評価順序 | `def say :: i64 -> i64 = \n ->⏎    do! IO.write_line (to_string n)⏎    n⏎⏎let p = (say 1, say 2)⏎match p with⏎\| (a, b) -> a + b` → `expect: stdout "1\n2\n3\n"` |
| EVL-012 | An assignment MUST evaluate its right-hand side before it determines the target place. | docs/language.md §呼び出し引数の暗黙の借用 | `def say :: i64 -> i64 = \n ->⏎    do! IO.write_line (to_string n)⏎    n⏎⏎def pick :: ref mut i64 -> i64 -> ref mut i64 = \r n ->⏎    do! IO.write_line ("place " + to_string n)⏎    r⏎def f :: unit -> i64 = \() ->⏎    let mut x = 0⏎    deref (pick (ref mut x) 1) = say 2⏎    x⏎f ()` → `expect: stdout "2\nplace 1\n2\n"` |
| EVL-013 | The bounds of `for i = a to b` MUST each be evaluated exactly once, left to right, before the first iteration. | LR/loops-and-conditionals/for-to.md | `def bound :: i32 -> i32 = \n ->⏎    do! IO.write_line ("bound " + to_string n)⏎    n⏎⏎let mut sum = 0⏎for i = bound 1 to bound 3 do⏎    sum = sum + i⏎sum` → `expect: stdout "bound 1\nbound 3\n6\n"` |
| EVL-014 | A `while` condition MUST be evaluated before every iteration, including the final failing check. | LR/loops-and-conditionals/while.md | `def below :: i64 -> bool = \n ->⏎    do! IO.write_line "check"⏎    n < 2⏎⏎let mut i = 0⏎while below i do⏎    i = i + 1⏎i` → `expect: stdout "check\ncheck\ncheck\n2\n"` |
| EVL-015 | `x \|> f \|> g` MUST evaluate `x` first and then apply `f` and `g` in order. | LR/values-and-functions/op-and-expressions.md | `def say :: i64 -> i64 = \n ->⏎    do! IO.write_line (to_string n)⏎    n⏎⏎def f :: i64 -> i64 = \n -> say (n + 10)⏎def g :: i64 -> i64 = \n -> say (n * 2)⏎say 1 \|> f \|> g` → `expect: stdout "1\n11\n22\n22\n"` |
| EVL-016 | A function-valued expression MUST be evaluated before its arguments. | docs/language.md §呼び出し引数の暗黙の借用 | `def say :: i64 -> i64 = \n ->⏎    do! IO.write_line (to_string n)⏎    n⏎⏎def pick :: unit -> (i64 -> i64) = \() ->⏎    do! IO.write_line "pick"⏎    \n -> n + 1⏎(pick ()) (say 5)` → `expect: stdout "pick\n5\n6\n"` |
| EVL-017 | The holes of an interpolated string MUST be evaluated left to right. | LR/literals-and-strings/interpolated-strings.md | `def say :: i64 -> i64 = \n ->⏎    do! IO.write_line (to_string n)⏎    n⏎⏎$"{say 1}-{say 2}"` → `expect: stdout "1\n2\n1-2\n"` |
| EVL-018 | Binary operators other than `**` MUST be left-associative. | LR/values-and-functions/op-and-expressions.md | `let a = 10 - 4 - 3⏎let b = 100 / 10 / 5⏎$"{a} {b}"` → `expect: stdout "3 2\n"` |
| EVL-019 | An operation that traps MUST stop the program at its own evaluation point: earlier side effects are visible and later operands are not evaluated. | LR/exception-handling/exception-handling.md | `def say :: i64 -> i64 = \n ->⏎    do! IO.write_line (to_string n)⏎    n⏎⏎let zero = 0⏎say 1 + (10 / zero) + say 2` → `expect: trap kind=div_zero stdout "1\n"` |
| EVL-020 | A chain `a && b && c` MUST stop at the first false operand. | LR/values-and-functions/op-and-expressions.md | `def say :: i64 -> i64 = \n ->⏎    do! IO.write_line (to_string n)⏎    n⏎⏎let r = (say 1 == 1) && (say 2 == 3) && (say 3 == 3)⏎r` → `expect: stdout "1\n2\nfalse\n"` |

### B.4 CTL（制御とパターン。26 件）

| ID | 規則 | 出典 | テスト（本体 → ディレクティブ） |
| --- | --- | --- | --- |
| CTL-001 | The condition of `if` MUST have type `bool`; there is no implicit conversion from numbers or strings. | LR/loops-and-conditionals/if.md | `let n: i64 = 1⏎if n then 1 else 2` → `expect: check-error E1003 line=2` |
| CTL-002 | All branches of an `if` MUST have the same type. | LR/loops-and-conditionals/if.md | `let flag = true⏎if flag then 1 else "one"` → `expect: check-error E1003 line=2` |
| CTL-003 | An `elif` chain MUST select the first branch whose condition is true. | LR/loops-and-conditionals/if.md | `def rate :: i64 -> i64 = \amount ->⏎    if amount >= 10000 then 20⏎    elif amount >= 5000 then 10⏎    elif amount >= 2000 then 5⏎    else 0⏎⏎$"{rate 12000} {rate 6000} {rate 2500} {rate 100}"` → `expect: stdout "20 10 5 0\n"` |
| CTL-004 | `for x in a .. b` MUST include both bounds. | LR/loops-and-conditionals/for-in.md | `let mut sum = 0⏎for i in 1 .. 4 do⏎    sum = sum + i⏎sum` → `expect: stdout "10\n"` |
| CTL-005 | `for x in a .. step .. b` MUST visit a, a+step, ... while the value has not passed b, and a negative step MUST count down. | LR/loops-and-conditionals/for-in.md | `let mut result = 0⏎for n in 10 .. -2 .. 4 do⏎    result = result + n⏎result` → `expect: stdout "28\n"` |
| CTL-006 | A range with a zero step MUST trap. | LR/loops-and-conditionals/for-in.md | `let step = 0⏎let mut count = 0⏎for n in 1 .. step .. 5 do⏎    count = count + 1⏎count` → `expect: trap kind=range_step` |
| CTL-007 | `for` over an array MUST visit the elements in index order. | LR/loops-and-conditionals/for-in.md | `let mut text = ""⏎for n in [3, 1, 2] do⏎    text = text + to_string n⏎text` → `expect: stdout "312\n"` |
| CTL-008 | `for i = a to b` MUST include both bounds, use `i32` for `i`, and run zero times when `a > b`. | LR/loops-and-conditionals/for-to.md | `let mut up = 0⏎for i = 1 to 4 do⏎    up = up + i⏎let mut none = 0⏎for i = 5 to 1 do⏎    none = none + 1⏎$"{up} {none}"` → `expect: stdout "10 0\n"` |
| CTL-009 | `continue` MUST skip to the next iteration and `break` MUST leave the innermost loop. | LR/loops-and-conditionals/for-in.md | `let mut sum = 0⏎for n in 1 .. 10 do⏎    if n % 2 == 1 then⏎        continue⏎    if n > 6 then⏎        break⏎    sum = sum + n⏎sum` → `expect: stdout "12\n"` |
| CTL-010 | `break` outside a loop MUST be rejected (E1023). | docs/language.md §制御構文 | `def f :: i64 -> i64 = \n ->⏎    break⏎    n⏎0` → `expect: check-error E1023 line=2` |
| CTL-011 | A `while` loop whose condition is initially false MUST run its body zero times. | LR/loops-and-conditionals/while.md | `let mut n = 0⏎while n > 0 do⏎    n = n + 100⏎n` → `expect: stdout "0\n"` |
| CTL-012 | The condition of `while` MUST have type `bool`. | LR/loops-and-conditionals/while.md | `let mut n: i64 = 3⏎while n do⏎    n = n - 1⏎n` → `expect: check-error E1003 line=2` |
| CTL-013 | `match` MUST try the arms in order and run the first arm whose pattern matches and whose guard holds. | LR/pattern-matching/match.md | `def check :: i64 -> string = \age ->⏎    match age with⏎    \| n when n < 0 -> "invalid"⏎    \| n when n < 20 -> "minor"⏎    \| _ -> "adult"⏎⏎$"{check (-3)} {check 15} {check 25}"` → `expect: stdout "invalid minor adult\n"` |
| CTL-014 | A `match` that does not cover every value MUST be rejected (E1021). | LR/pattern-matching/match.md | `union Status = Success of i64 \| NotFound \| Error of string⏎⏎def handle :: Status -> i64 = \status ->⏎    match status with⏎    \| Success code -> code⏎    \| NotFound -> 404⏎0` → `expect: check-error E1021 line=4` |
| CTL-015 | An arm with a `when` guard MUST NOT count towards exhaustiveness. | LR/pattern-matching/match.md | `def sign :: i64 -> i64 = \n ->⏎    match n with⏎    \| v when v >= 0 -> 1⏎    \| v when v < 0 -> -1⏎0` → `expect: check-error E1021 line=2` |
| CTL-016 | An arm fully covered by earlier arms MUST produce warning W1003. | LR/pattern-matching/match.md | `def f :: bool -> i64 = \b ->⏎    match b with⏎    \| true -> 1⏎    \| false -> 0⏎    \| _ -> 2⏎f true` → `expect: check-warning W1003 line=5` |
| CTL-017 | All arms of a `match` MUST have the same type. | LR/pattern-matching/match.md | `let n = 1⏎match n with⏎\| 1 -> "one"⏎\| _ -> 0` → `expect: check-error E1003 line=4` |
| CTL-018 | A union pattern MUST bind the payload of the matched case. | LR/pattern-matching/match.md | `union Status = Success of i64 \| NotFound \| Error of string⏎⏎def handle :: Status -> i64 = \status ->⏎    match status with⏎    \| Success code -> code⏎    \| NotFound -> 404⏎    \| Error _ -> 500⏎⏎$"{handle (Success 200)} {handle NotFound} {handle (Error "x")}"` → `expect: stdout "200 404 500\n"` |
| CTL-019 | An or-pattern MUST match when any alternative matches and every alternative MUST bind the same names. | LR/pattern-matching/pattern-matching.md | `union NumberOrPair = Single of i64 \| Pair of i64 * i64⏎⏎def extract :: NumberOrPair -> i64 = \val ->⏎    match val with⏎    \| Single n \| Pair (n, _) -> n⏎⏎$"{extract (Single 7)} {extract (Pair (42, 10))}"` → `expect: stdout "7 42\n"` |
| CTL-020 | A record pattern MUST bind fields by name and MAY ignore the others with `_`. | LR/pattern-matching/pattern-matching.md | `record Person { name: string, age: i64 }⏎⏎let person = Person { name: "Alice", age: 30 }⏎match person with⏎\| Person { name = n, age = _ } -> n.length` → `expect: stdout "5\n"` |
| CTL-021 | A tuple pattern in `for` MUST destructure each element. | LR/pattern-matching/pattern-matching.md | `let points = [(1, 2), (3, 4)]⏎let mut sum = 0⏎for (x, y) in points do⏎    sum = sum + x * y⏎sum` → `expect: stdout "14\n"` |
| CTL-022 | A refutable pattern in `for` MUST NOT be checked for exhaustiveness and a non-matching element MUST trap. | LR/loops-and-conditionals/for-in.md | `let items = [Some 1, None]⏎let mut sum = 0⏎for Some x in items do⏎    sum = sum + x⏎sum` → `expect: trap kind=pattern_mismatch` |
| CTL-023 | A failed `assert` MUST trap and MUST NOT be caught by `try`. | LR/exception-handling/exception-handling.md | `def f :: i64 -> i64 = \n ->⏎    assert (n > 0)⏎    n⏎let zero = 0⏎f zero` → `expect: trap kind=assert` |
| CTL-024 | `try` MUST NOT catch traps such as division by zero. | LR/exception-handling/exception-handling.md | `def div :: i32 -> i32 -> Result<i32, Exception> = \x y ->⏎    try⏎        @checked (x / y)⏎    with⏎    \| e is OverflowException -> e⏎⏎let zero = 0⏎match div 10 zero with⏎\| Ok v -> v⏎\| Error _ -> -1` → `expect: trap kind=div_zero` |
| CTL-025 | `let!` in a `Maybe` computation MUST stop at the first `None` and return `None`. | LR/exception-handling/exception-handling.md | `def say :: i64 -> i64 = \n ->⏎    do! IO.write_line (to_string n)⏎    n⏎⏎def f :: unit -> Maybe<i64> = \() ->⏎    let none: Maybe<i64> = None⏎    Maybe {⏎        let! a = Some (say 1)⏎        let! b = none⏎        let! c = Some (say 3)⏎        return a + b + c⏎    }⏎⏎match f () with⏎\| Some v -> v⏎\| None -> -1` → `expect: stdout "1\n-1\n"` |
| CTL-026 | `let!` in a `Result` computation MUST stop at the first `Error` and return it. | LR/exception-handling/exception-handling.md | `def parse_qty :: i64 -> Result<i64, string> = \qty ->⏎    if qty > 0 then Ok qty else Error "quantity must be positive"⏎⏎def line_total :: i64 -> i64 -> Result<i64, string> = \price qty ->⏎    Result {⏎        let! count = parse_qty qty⏎        return price * count⏎    }⏎⏎def show :: Result<i64, string> -> string = \result ->⏎    match result with⏎    \| Ok n -> to_string n⏎    \| Error e -> e⏎⏎show (line_total 120 3) + " / " + show (line_total 120 0)` → `expect: stdout "360 / quantity must be positive\n"` |

### B.5 TYP（型。22 件）

| ID | 規則 | 出典 | テスト（本体 → ディレクティブ） |
| --- | --- | --- | --- |
| TYP-001 | A value whose type differs from the annotated type of its binding MUST be rejected (E1003); there is no implicit conversion. | LR/types-and-type-inference/types.md | `let s: string = 5⏎s` → `expect: check-error E1003 line=1` |
| TYP-002 | Records MUST be nominal: two record types with identical fields MUST be different types. | LR/types-and-type-inference/types.md | `record A { x: i64 }⏎record B { x: i64 }⏎⏎let a: A = B { x: 1 }⏎a.x` → `expect: check-error E1003 line=4` |
| TYP-003 | A function type MUST be right-associative and functions MUST be curried, so partial application yields a function. | LR/types-and-type-inference/types.md | `def add :: i64 -> i64 -> i64 = \a b -> a + b⏎⏎let inc = add 1⏎inc 41` → `expect: stdout "42\n"` |
| TYP-004 | An unsuffixed integer literal MUST take its type from a later use, in both directions. | LR/types-and-type-inference/type-inference.md | `def accept_i64 :: i64 -> i64 = \x -> x + 2⏎⏎let a = 40⏎accept_i64 a` → `expect: stdout "42\n"` |
| TYP-005 | A decimal literal without any typing context MUST have type `f64`. | LR/types-and-type-inference/type-inference.md | `let third = 1.0 / 3.0⏎third == 0.3333333333333333` → `expect: stdout "true\n"` |
| TYP-006 | A `let` whose type cannot be determined MUST be rejected; an empty array needs a type annotation (E1004). | LR/types-and-type-inference/type-inference.md | `let xs = []⏎0` → `expect: check-error E1004 line=1` |
| TYP-007 | A generic function MUST be usable at several types in one program. | LR/types-and-type-inference/generics.md | `def id :: 'a -> 'a = \x -> x⏎⏎$"{id 41 + 1} {id "s"}"` → `expect: stdout "42 s\n"` |
| TYP-008 | A constraint used by a generic body MUST be inferred from the operators it uses, so `x + x` requires `Add`. | LR/types-and-type-inference/type-inference.md | `def double :: 'a -> 'a = \x -> x + x⏎⏎let i = double 21⏎let f = double 2.5⏎$"{i}:{f}"` → `expect: stdout "42:5\n"` |
| TYP-009 | A generic function MUST be rejected at a type that lacks the required instance (E1005). | LR/types-and-type-inference/constraints.md | `record P { x: i64 }⏎⏎def double :: 'a -> 'a = \x -> x + x⏎⏎let p = double (P { x: 1 })⏎0` → `expect: check-error E1005 line=5` |
| TYP-010 | A declared type parameter MUST be used by a field or a payload (E1024). | LR/types-and-type-inference/generics.md | `record Box<'a> { v: i64 }⏎0` → `expect: check-error E1024 line=1` |
| TYP-011 | A generic record literal MUST infer its type arguments from the field values. | LR/types-and-type-inference/type-inference.md | `record Box<'a> { value: 'a }⏎⏎let boxed = Box { value: 42 }⏎boxed.value` → `expect: stdout "42\n"` |
| TYP-012 | A type alias MUST be interchangeable with its target type. | LR/types-and-type-inference/alias.md | `type Id = i64⏎⏎def next :: Id -> i64 = \n -> n + 1⏎⏎let n: i64 = 41⏎next n` → `expect: stdout "42\n"` |
| TYP-013 | An assignment expression MUST have type `unit`. | LR/values-and-functions/values.md | `let mut x = 0⏎let r: i64 = (x = 1)⏎r` → `expect: check-error E1003 line=2` |
| TYP-014 | A record without a derived or declared `Eq` instance MUST NOT support `==` (E1005). | LR/types-and-type-inference/type-classes.md | `record P { x: i64 }⏎⏎let a = P { x: 1 }⏎let b = P { x: 1 }⏎a == b` → `expect: check-error E1005 line=5` |
| TYP-015 | `deriving (Eq)` MUST give a record structural equality. | LR/types-and-type-inference/type-classes.md | `record P { x: i64, y: string } deriving (Eq)⏎⏎let a = P { x: 1, y: "a" }⏎let b = P { x: 1, y: "a" }⏎let c = P { x: 1, y: "b" }⏎$"{a == b} {a == c}"` → `expect: stdout "true false\n"` |
| TYP-016 | A cast between `char` and an integer with `as` MUST be rejected (E1005); `Char.to_u16` MUST be used. | LR/types-and-type-inference/cast.md | `let c = 'a'⏎let n = c as i64⏎n` → `expect: check-error E1005 line=2` |
| TYP-017 | A recursive record that contains itself without indirection MUST be rejected (E1010). | docs/language.md §再帰的なデータ型 | `record Node { next: Node, value: i64 }⏎0` → `expect: check-error E1010 line=1` |
| TYP-018 | A union case constructor MUST check its payload type (E1003). | LR/types-and-type-inference/types.md | `union Status = Active of i64 \| Done⏎⏎let s = Active "x"⏎0` → `expect: check-error E1003 line=3` |
| TYP-019 | A `let` MAY shadow a name in the same scope, including with a different type. | LR/values-and-functions/values.md | `let x = 1⏎let x = "a"⏎x` → `expect: stdout "a\n"` |
| TYP-020 | An unknown type name MUST be rejected at compile time. | LR/types-and-type-inference/types.md | `let x: Foo = 1⏎x` → `expect: check-error E1004 line=1` |
| TYP-021 | A type class instance MUST be unique per class and type (E1016). | LR/types-and-type-inference/type-classes.md | `[Main.tz]⏎record Amount { value: i64 }⏎⏎instance Traits.Score<Amount> {⏎    fn score amount = amount.value⏎}⏎⏎instance Traits.Score<Amount> {⏎    fn score amount = amount.value + 1⏎}⏎0⏎[Traits.tt]⏎class Score<'value> {⏎    def score :: ref 'value -> i64⏎}` → `expect: check-error E1016 line=7` |
| TYP-022 | A class declared in a `.tt` file MUST be usable by an instance in a `.tz` file, and the method type MUST come from the class. | LR/types-and-type-inference/type-classes.md | `[Main.tz]⏎record Amount { value: i64 }⏎⏎instance Traits.Score<Amount> {⏎    fn score amount = amount.value⏎}⏎⏎let amount = Amount { value: 5 }⏎Traits.Score.score ref amount⏎[Traits.tt]⏎class Score<'value> {⏎    def score :: ref 'value -> i64⏎}` → `expect: stdout "5\n"` |
