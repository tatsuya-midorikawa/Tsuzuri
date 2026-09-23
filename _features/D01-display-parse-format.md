# D01: 表示・解析・書式化（Display／Parse／to_string）
| 項目 | 内容 |
|---|---|
| ID | D01 |
| 優先度 | P0 |
| 規模 | M |
| 依存 | E02, B01 |
| 後続 | A07, E07, G06 |
| 状態 | todo |
| 主な影響ファイル | `std/`（E02 後）, `src/check.rs`, `src/polymorph.rs`, `src/llvm.rs`, `src/runtime/numeric.c`, `src/runtime/generate.py`, `src/runtime/string.ll`, `tests/display_parse.rs`, `tests/display_parse.mjs`, `tests/primitives.mjs`, `tests/numeric_casts.mjs`, `docs/language.md`, `docs/architecture.md`, `README.md`, `docs/benchmarks.md` |

## 目的

表示・解析・文字列化を言語の基礎 API として定義する。
`Display` は A07 の deriving、E07 の Debug 出力、G06 のテスト失敗表示の前提である。
`Parse` は外部入力を安全に値へ戻すための最小変換 API であり、失敗は B01 の `Option` で表す。
`to_string` とコンソール出力は同じ数値フォーマットを使い、native と WASM で差を作らない。
WASM は既定でインポートなしなので、`printf`／libc／libm に依存しない実装へ揃える。
数値表示は性能上も頻出するため、整数は短い高速経路、浮動小数点は正確な往復性を両立する。

## 現状

`src/check.rs` の `Builtin` は `Sqrt`, `Floor`, `Ceil`, `Abs`, `ToFloat`, `ToInt`, `Assert`, `CloneString`, `TaskRun`, `TaskParallel` の 10 個だけを持つ。
`Builtin::ALL` は固定長配列 `[Self; 10]` で、追加時は長さも更新する必要がある。
`Builtin::signature` は `Signature { parameters: vec![parameter], result }` を返す一引数中心の形である。
`Checker::name` は無修飾名から `Builtin::ALL` を探し、`Task.run`/`Task.parallel` は `Checker::value_expression` の `ExprKind::Field` 分岐で特別解決する。
`check_modules` はユーザー関数名が `Builtin::ALL` の名前と一致すると `E1001` で拒否する。

`src/polymorph.rs` の `Classes::collect` は組み込みクラス `Add`, `Sub`, `Mul`, `Div`, `Rem`, `Eq`, `Ord`, `Bits`, `Neg`, `Integer`, `SignedInteger`, `Float`, `Numeric`, `Copy`, `Capture`, `Send` を登録する。
`Classes::intrinsic` は数値・string・bool・unit 等の組み込みインスタンスを判定する。
`Checker::builtin` は `builtin.signature().as_type()` を呼び、型変数を `Infer` に置換して `TypedExprKind::Function(FunctionRef::Builtin(...))` を返す。
`Specializer::intrinsic_function` は演算クラスの intrinsic method を通常関数に包むが、現在は `Operation::Binary` と `Operation::Unary` だけを想定している。

`src/llvm.rs` の `emit_builtin` は `ToFloat`, `ToInt`, `Assert`, `CloneString`, `Sqrt`, `Floor`, `Ceil`, `Abs` を内部関数 `@tz.builtin.<name>` として出す。
`FunctionEmitter::call` は `FunctionRef::Builtin` を見つけると `builtins: BTreeSet<Builtin>` へ登録し、`emit_target` が後で `emit_builtin` を出力する。
`FunctionEmitter::binary` は `string + string` を `@tz.string.concat`、`string == string` を `@tz.string.equal` へ下げる。
`FunctionEmitter::cast` は 8〜64-bit 整数と f32/f64 の直接変換、`llvm.fpto*i.sat`、それ以外の `tz_soft_cast` を使う。
`numeric_kind` は `Type::Binary(16/32/64/128)`, `Type::Decimal(32/64/128)`, `Type::Integer(bits, signed)` を runtime kind へ写像する。
`console_main` は i8〜i64/f32/f64/bool を native の `printf` で表示し、string と 128-bit/f16/f128/decimal は `@tz.console.write` と `tz_soft_format` を使う。
現行 f64 は `%.17g`、f32 は `%.9g` であり、`0.1` は `0.10000000000000001`、`0.1f32` は `0.100000001` と表示される。
現行の scratch 実行では `-0.0` は `-0`、`1.0 / 0.0` は `inf`、`0.0 / 0.0` は `nan` と表示された。

`src/runtime/numeric.c` は `tzrt_big`, `tzrt_format`, `tzrt_number` を使い、ホスト浮動小数点を使わずに `tz_soft_op`, `tz_soft_cmp`, `tz_soft_cast`, `tz_soft_format` を実装する。
`tz_soft_format` は現在「最大 36 桁程度を出す」実装で、binary の最短往復表示ではない。
`src/runtime/generate.py` は `numeric.c` を Clang `-ffreestanding -fno-builtin` で LLVM IR にし、`src/runtime/numeric.ll` を生成する。
`numeric.ll` は手編集禁止である。
`src/runtime/string.ll` は `@tz.string.copy`, `@tz.string.allocate`, `@tz.string.new`, `@tz.string.concat`, `@tz.string.equal` だけを提供する。

`tests/primitives.mjs` は decimal と binary128 の参照ケース、文字列表示、現行コンソール出力を検査する。
同ファイル末尾には `1.0000000000000000000000000000000002f128` の出力として現行 `1.00000000000000000000000000000000019` を期待する箇所がある。
`tests/numeric_casts.mjs` は数値変換の境界値を JS BigInt/Number 参照で照合し、native/WASM `-O0`/`-O3` と WASM imports 空を確認する。

## 仕様

### 前提とする他チケットのインターフェース

E02 は `std/` の同梱、標準モジュール名の予約、修飾名付き builtin 解決、複数引数 builtin、制約付き builtin signature を提供している前提にする。
E02 の builtin interface は次を唯一の前提にする。

```rust
pub enum BuiltinType {
    Var(&'static str),
    Concrete(Type),
    Std { module: &'static str, name: &'static str, args: Vec<BuiltinType> },
    Array(Box<BuiltinType>),
    List(Box<BuiltinType>),
    Tuple(Vec<BuiltinType>),
    Reference(Box<BuiltinType>, bool),
    Function(Vec<BuiltinType>, Box<BuiltinType>),
    UnsignedOf(Box<BuiltinType>),
    WidenOf(Box<BuiltinType>),
}

pub struct BuiltinScheme {
    pub parameters: Vec<BuiltinType>,
    pub result: BuiltinType,
    pub variables: Vec<&'static str>,
    pub constraints: Vec<BuiltinConstraint>,
}
```

型付き IR の builtin 参照は `FunctionRef::Builtin(BuiltinInstance { builtin, types })` だけを使う。
このチケットで独自の `BuiltinSignature`、`BuiltinSignatureTemplate`、`FunctionRef::Builtin(Builtin, Vec<Type>)` を定義しない。
B01 の `Option 'a` は `BuiltinType::Std { module: "Option", name: "Option", args: vec![BuiltinType::Var("a")] }` で表す。
builtin-backed API 関数は `Builtin` entry としてだけ存在し、std source に同名 `def` を置かない。
B01 は `Option 'a` を `std/Option.tc` に `union Option 'a = None | Some of 'a` として提供している前提にする。
B01 の `Option.Some`/`Option.None` は std から常に解決でき、builtin lowering が値を構築できること。
A08 は `Type::Char` 相当の `char` 型、Unicode スカラーリテラル、`Char.to_u32`, `Char.of_u32 : i32u -> Option char` を提供する前提にする。
A08 が未完了の段階では char 用 `Display` instance と `Parse char` はこのチケット内で実装せず、スタブも置かない。
C03 の `&[T]` slice と C02 の `Vec` は D01 の必須依存ではないが、フェーズ 2 の string interpolation 実装で効率的な builder に使える。

### 他チケットへの提供インターフェース

このチケットは組み込みクラス `Display` と `Parse`、および `to_string` を提供する。
A07 は deriving `Display` の生成先として `Display.display` を実装し、数値・bool・unit・string・char の組み込み instance を上書きしない。
E07 は `Debug.print`/`trace` の既定表示に `Display.display` または明示的 `Debug` を選べる。
G06 はテスト失敗時の値表示に `to_string` を使える。
D02 は `String` モジュール内部で `to_string` を呼ばないが、string interpolation のフェーズ 2 は D01 の desugar 規則に従う。

### 構文

このチケットで新しいユーザー構文は必須追加しない。
フェーズ 2 で string interpolation を入れる場合だけ次を追加する。

```text
InterpolatedString ::= '$"' InterpolatedPart* '"'
InterpolatedPart   ::= RawStringText | '{{' | '}}' | '{' Expression '}'
```

`{{` は `{`、`}}` は `}` を表す。
穴 `Expression` は通常の式で、`}` までを既存 parser の式として読む。
穴の中の `let`/ブロック/if/match は既存の式構文に従う。
フェーズ 1 では interpolation を実装しない。

### 型クラスと関数

組み込みクラスを次の形で追加する。

```text
class Display 'a {
    def display :: &'a -> string
}

class Parse 'a {
    def parse :: &string -> Option 'a
}

def to_string :: Display 'a => 'a -> string
```

`Display.display` は値を消費せず、共有借用から `string` を新しく返す。
`to_string` は値を受け取り、内部で一時共有借用して `Display.display` を呼び、その後に受け取った値を通常どおり drop する。
`Parse.parse` は入力文字列を消費せず、成功なら `Some value`、失敗なら `None` を返す。
`Parse` の失敗はトラップではない。

組み込み `Display` instance は次を持つ。
`i8`, `i16`, `i32`, `i64`, `i128`, `i8u`, `i16u`, `i32u`, `i64u`, `i128u`。
`f16`, `f32`, `f64`, `f128`。
`d32`, `d64`, `d128`。
`bool`, `unit`, `string`。
A08 完了後は `char`。

組み込み `Parse` instance は次を持つ。
全整数型。
全 binary float 型。
全 decimal float 型。
`bool`。
A08 完了後に `char` を追加してよいが、D01 フェーズ 1 の必須ではない。
`unit` と `string` の `Parse` はこのチケットでは提供しない。
`string` は「任意の入力が成功する」ため `Parse` の意味が薄く、必要なら D02 以降で `String.clone` として扱う。

### 表示形式

数値表示は `to_string` と `console_main` で完全に一致させる。
**決定（2026-09-23 承認、GUIDE D-11）:** f32/f64 の `%.9g`/`%.17g` 互換をやめ、binary float（f16/f32/f64/f128）は最短 round-trip 形式に切り替える。コンソール出力も同じ形式に変える。
理由は、`0.1` を `0.1` と表示する通常のユーザー期待と、D-11 の「数値の文字列表現を統一する」要求を両立できるため。
Ryu/Grisu-exact/Dragon4 のいずれかのアルゴリズムを、既存 `numeric.c` の多倍長整数を使って同梱実装する。
第三者の libm/printf/Ryu ソースをコピーしない。
実装は論文・仕様に基づく自前実装とし、ライセンス不明コードを取り込まない。

出力の字句 grammar は次とする。

```text
display-int      ::= "-"? Digit+
display-bool     ::= "true" | "false"
display-unit     ::= "()"
display-string   ::= raw UTF-8 bytes of the string
display-char     ::= one UTF-8 encoded Unicode scalar value
display-special  ::= "-inf" | "inf" | "nan"
display-float    ::= "-0" | "-"? finite-float | display-special
finite-float     ::= decimal-plain | decimal-scientific
decimal-plain    ::= Digit+ ("." Digit+)?
decimal-scientific ::= Digit ("." Digit+)? "e" Sign? Digit+
Sign             ::= "+" | "-"
Digit            ::= "0" | ... | "9"
```

整数は常に十進、接頭辞・桁区切りなし。
符号付き整数の最小値は通常の負十進で表示する。
符号なし整数に `-` は付かない。
bool は小文字 `true`/`false`。
unit は `()`。
string の display は文字列の **同じ UTF-8 バイト列**を返す。
したがって NUL や改行もエスケープせず含む。
これは `console_main` の string 出力と同じで、デバッグ用の引用符付き表現ではない。
char は A08 の Unicode スカラーを UTF-8 エンコードした 1 文字の string にする。

binary float の有限値は、同じ型へ `Parse.parse` したときに元の値と同一 bit pattern へ戻る最短十進表現とする。
同じ長さの候補が複数ある場合は数値的に近いもの、完全同距離なら末尾の十進仮数が偶数のものを選ぶ。
NaN は例外で、すべての NaN encoding を `nan` と表示し、parse すると canonical quiet NaN に戻す。
したがって display→parse の bit-exact round trip は **非 NaN 値だけ**に要求し、NaN は分類（NaN であること）だけを比較する。
指数表記を使う条件は `decimal_exponent < -6 || decimal_exponent >= significant_digits + 6` とする。
ここで `decimal_exponent` は正規化した `d.ddd * 10^e` の `e`。
指数の `e` は小文字で、符号は負なら `-`、正なら `+` を必ず付ける。
指数の leading zero は付けない。
通常表記では不要な末尾 `.0` は付けない。
`1.0` は `1`、`1000000.0` は `1000000`、`10000000.0` も threshold 上は通常表記を許す（`10000000`）。
負のゼロは `-0` と表示する。
正のゼロは `0`。
NaN は常に `nan` で、符号と payload は表示しない。
無限大は `inf`/`-inf`。
subnormal は同じ規則で最短 round-trip 表示にする。

decimal float は十進エンコーディングの値を十進で正確に表示する。
decimal は quantum の末尾ゼロを表示しない正規化形式にする。
`0.10d128` と `0.1d128` はどちらも `0.1`。
decimal の負のゼロは `-0`。
decimal の NaN は `nan`、Infinity は `inf`/`-inf`。
decimal の exponent 表記条件も binary と同じ `decimal_exponent < -6 || decimal_exponent >= significant_digits + 6` に揃える。
decimal は parse/display の往復で値の数値同値と符号付きゼロ・special を保つ。
NaN payload/sign と quantum の違いまで保持することはこのチケットの表示契約に含めない。

既存から変わる代表例:
`0.1` は `0.10000000000000001` から `0.1` へ変わる。
`0.1f32` は `0.100000001` から `0.1` へ変わる。
`1.0000000000000000000000000000000002f128` は現在の 36 桁丸め表示ではなく、f128 へ戻る最短表現へ変わる。
`tests/primitives.mjs` の現行出力期待値と `docs/language.md` の「コンソール表示」説明を更新する。

### 解析形式

`Parse.parse` は入力文字列全体を使う。
前後の空白は許可しない。
空文字列は失敗。
成功時だけ `Some` を返す。
失敗理由を診断にはしない。

整数の入力 grammar:

```text
parse-int ::= Sign? (Decimal | Hex | Binary)
Decimal   ::= Digit ("_"? Digit)*
Hex       ::= "0x" HexDigit ("_"? HexDigit)*
Binary    ::= "0b" BinDigit ("_"? BinDigit)*
Sign      ::= "+" | "-"
```

`+` は許可する。
符号なし整数に負号が付いたら `None`。
`_` は桁と桁の間だけ許可し、先頭・末尾・連続・prefix 直後は `None`。
十進以外の prefix は小文字だけを必須にする。
大文字 `0X`/`0B` はフェーズ 1 では `None`。
値が対象幅の範囲を超えたら `None`。
符号付き整数の最小値は `-128i8` 相当の文字列を受理する。

float の入力 grammar:

```text
parse-float ::= Sign? (FiniteFloat | "inf" | "nan")
FiniteFloat ::= DecimalDigits ("." DecimalDigits?)? Exponent?
              | "." DecimalDigits Exponent?
DecimalDigits ::= Digit ("_"? Digit)*
Exponent ::= ("e" | "E") Sign? DecimalDigits
Sign ::= "+" | "-"
```

`inf`, `nan` は小文字だけを必須にする。
`+inf` と `-inf` は受理する。
`+nan` と `-nan` は受理し、結果の NaN 符号は公開しないのでどちらも canonical NaN にしてよい。
NaN payload 構文は提供しない。
hex float は提供しない。
`1.` は受理する。
`.5` は受理する。
`1e`、`_1`、`1__0`、`1_` は `None`。
有限値の parse は対象型へ一度だけ最近接・偶数丸めする。
binary は f64 等の中間値を経由しない。
decimal は decimal の precision/exponent へ一度だけ丸める。
範囲外で infinity へ丸められる有限入力は `None` とする。
`inf` だけが infinity を生成する。
subnormal へ丸められる値は受理する。
0 へ underflow する値も、丸め結果が 0 なら符号付きゼロを保って受理する。

bool の入力は厳密に `true` または `false`。
大文字や空白は `None`。

char の入力は A08 完了後に「UTF-8 でちょうど 1 Unicode scalar value」を受理し、空文字列、複数 scalar、不正 UTF-8 は `None`。

### 評価順序と所有権

`to_string value` は `value` を左から右の通常規則で評価し、その所有値を受け取る。
内部の `display (&value)` は `value` の drop より前に実行する。
`display` が返した string は独立した所有値である。
`value` が非 Copy の場合、`to_string value` の後に元の束縛は使えない。
`Display.display (&value)` を直接呼べば値は消費されない。
`Parse.parse (&text)` は text を消費しない。
`string` の display は `clone_string` と同じく独立した所有 string を返す。
戻り string は呼び出し元が所有し、通常どおり drop される。

## 設計

### データ構造

E02 の builtin signature 拡張を使い、`src/check.rs` の `Builtin` に次を追加する。

```rust
pub enum Builtin {
    // existing variants...
    ToString,
    Display,
    Parse,
}
```

`Builtin::ALL` は 13 要素へ更新する。
`Builtin::name` は `"to_string"`, `"$builtin.display"`, `"$builtin.parse"` を返す。
ユーザーに直接見せる名前は `to_string` のみとし、`"$builtin.*"` は `Specializer::intrinsic_function` から生成する内部関数名に使う。
E02 の仕様により、`"$"` を含む名前はソースから定義できない。
`to_string` の scheme は `BuiltinScheme { parameters: [Var("a")], result: Concrete(Type::String), variables: ["a"], constraints: [Display Var("a")] }`。
`Display.display` と `Parse.parse` は class method から生成する `BuiltinInstance` としてだけ参照し、std source に同名の `def` を置かない。
`Parse.parse` の result は `BuiltinType::Std { module: "Option", name: "Option", args: vec![Var("a")] }`。

`src/polymorph.rs` の `Classes::collect` で組み込みクラス `Display` と `Parse` を追加する。
`Display` には method `display`、signature `&'a -> string`、operation `Operation::Builtin(Builtin::Display)` を持たせる。
`Parse` には method `parse`、signature `&string -> Option 'a`、operation `Operation::Builtin(Builtin::Parse)` を持たせる。
現在の `Operation` は `Binary`/`Unary` だけなので次に拡張する。

```rust
enum Operation {
    Binary(BinaryOp),
    Unary(UnaryOp),
    Builtin(Builtin),
}
```

`Classes::intrinsic` は次を返す。
`Display`: 数値、bool、unit、string、A08 の char。
`Parse`: 数値、bool、A08 の char。
`Specializer::intrinsic_function` は `Operation::Builtin` の場合、method signature を具体型へ substitute し、body を `TypedExprKind::Call(FunctionRef::Builtin(builtin), locals...)` 相当に生成する。
既存の `TypedExprKind::Function(FunctionRef::Builtin(_))` と `TypedExprKind::Call` だけで表せるなら新しい `TypedExprKind` は不要。
新しい variant を足す場合は `TypedExpr::children`/`children_mut` と GUIDE 6.2 の全箇所を更新する。

`src/llvm.rs` の `emit_builtin` に `ToString`, `Display`, `Parse` を追加する。
ただし `Display`/`Parse` は型ごとに戻り型が異なるので、E02 の `FunctionRef::Builtin(BuiltinInstance { builtin, types })` を使って対象型を運ぶ。
既存 builtin も `BuiltinInstance` に統一する。
`Builtin::Display` と `Builtin::Parse` は `types[0]` に対象型を保持する。
`Builtin::ToString` は `types[0]` に `'a` の具体型を保持する。
`builtins` collection は `BTreeSet<BuiltinInstance>` にする。
IR シンボル名は型を決定的に mangling する。
例: `@tz.builtin.to_string.i64`, `@tz.builtin.display.f64`, `@tz.builtin.parse.i128u`。

### ランタイム

`numeric.c` に parse 関数を追加し、`generate.py` で `numeric.ll` を再生成する。

```c
int tz_soft_format(char *out, const unsigned char *input, int kind);
int tz_soft_parse(unsigned char *out, const char *input, unsigned long long length, int kind);
```

`tz_soft_format` は既存の formatter symbol であり、これを単一の数値表示入口として拡張する。
別名 `tz_format_value` を追加しない。
戻り値は書いた byte 数。
失敗しない。
出力 buffer は少なくとも 128 bytes とする。
最大長は i128u 39 桁、binary128 shortest round-trip 約 46 桁、decimal128 約 43 桁、符号・指数込みでも 128 bytes に収まる。

`tz_soft_parse` は成功なら 1、失敗なら 0 を返す。
成功時は `out` に対象型の bit 表現を書き込む。
整数 overflow、構文不正、有限 float overflow は 0。
内部で `__builtin_trap` してはならない。
作業 buffer の上限を設け、長さが 4,096 bytes を超える入力は 0 を返す。
これは parse の資源上限で、診断ではなく `None`。

binary shortest formatter は既存 `tzrt_big` を使う Dragon4 系で実装する。
最短判定は対象 float の隣接値の中点区間を多倍長整数で表し、十進候補が区間内にあるかで決める。
tie-to-even の境界は IEEE の偶数 significand を使う。
Ryu 相当の高速表生成を追加する場合も、表は生成スクリプトで作り、決定的な C 配列として `numeric.c` に埋め込む。
表の出典と生成手順を `docs/architecture.md` に書く。

整数 formatter は `u128` の繰り返し除算を使う高速経路にする。
8〜64-bit でも native `printf` は使わない。
`console_main` は全数値で同じ formatter を使い、native/WASM 差を消す。
bool/unit/string は `numeric.c` ではなく `llvm.rs`/`string.ll` 側で処理する。

`string.ll` には「既存 buffer から string を作る」`@tz.string.new` があるので、display は一時 stack buffer から `@tz.string.new` で所有 string を作る。
parse は `&string` の descriptor から data pointer と length を取り、`tz_soft_parse` を呼ぶ。
`Option.Some`/`None` の構築は B01 の union lowering を使う。

### LLVM IR の形

`to_string i64` の代表 IR:

```llvm
define internal %tz.string @tz.builtin.to_string.i64(i64 %x) nounwind {
entry:
  %slot = alloca i64, align 16
  %buffer = alloca [128 x i8], align 16
  store i64 %x, ptr %slot
  %count = call i32 @tz_soft_format(ptr %buffer, ptr %slot, i32 20)
  %n = sext i32 %count to i64
  %s = call %tz.string @tz.string.new(ptr %buffer, i64 %n)
  ret %tz.string %s
}
```

`Display.display (&i64)` は `ptr` から load して同じ formatter を呼ぶ。
`to_string` は non-Copy 値の場合、呼び出し側で通常どおり引数を move し、builtin 内では `Display.display` 相当を実行後に `drop_value` される形へする。
実装しやすい場合は `to_string` を型付き IR で `display (&tmp)` へ展開し、`tmp` の drop を既存 lowering に任せる。
`to_string string` は値を消費し、そのまま返してよい。
ただし `Display.display (&string)` は clone する。
この差を仕様に明記し、所有権テストで確認する。

`Parse.parse i64` の代表 IR:

```llvm
define internal %tz.union.Option[i64] @tz.builtin.parse.i64(ptr %text) nounwind {
entry:
  %s = load %tz.string, ptr %text
  %data = extractvalue %tz.string %s, 0
  %len = extractvalue %tz.string %s, 1
  %slot = alloca i64, align 16
  %ok = call i32 @tz_soft_parse(ptr %slot, ptr %data, i64 %len, i32 20)
  %is_ok = icmp ne i32 %ok, 0
  br i1 %is_ok, label %some, label %none
some:
  %value = load i64, ptr %slot
  ; B01/A02 の Option.Some 構築
none:
  ; B01/A02 の Option.None 構築
}
```

IR は `BTreeSet`/`BTreeMap` を使って決定的に出す。
intrinsic 宣言を追加する場合も `intrinsics: BTreeSet<String>` に入れる。
数値表示・解析では fast-math、再結合、暗黙 FMA を使わない。
`emit_target` は現状 `output.contains("@tz_soft_")` のときだけ `numeric.ll` を連結する。
formatter は既存 `@tz_soft_format`、parser は `@tz_soft_parse` にするため、この predicate で parse-only/to_string-only の両方が確実に連結される。
もし `@tz_soft_` 以外の symbol 名を選ぶ場合は、`emit_target` の inclusion predicate を同時に拡張し、parse-only/to_string-only の link test を追加する。

### コンソール出力

`console_main` は数値について `printf` を使わない。
native でも `@tz.console.write` を使う。
そのため `Entry::Console` で数値を表示する場合は `runtime/console.ll` と `runtime/numeric.ll` が連結される。
`printf` 宣言は bool の `%s` 表示にも使わず、bool は `@tz.console.write` で `true`/`false` を書く。
unit は何も出力しない。
最後に改行を出す既存挙動を維持する。
string は既存どおり bytes を書いてから改行する。

### string interpolation（フェーズ 2）

フェーズ 2 で `$"x = {x}"` を実装する場合の desugar は次とする。
`$"a{x}b{y}"` は `(((("a" + to_string x) + "b") + to_string y) + "")` に相当する。
評価順序は左から右。
各 hole 式は一度だけ評価する。
hole の値は `to_string` に渡すため move される。
hole 後に元の非 Copy 値を使うと通常の move エラー。
literal segment は string literal と同じ UTF-8 bytes。
`{{` と `}}` は literal brace。
空 segment は実装時に省略してよいが、評価順序を変えてはならない。
大量結合の性能は D02/C02 の builder API で改善するまで対象外。

## 実装手順

1. E02/B01 の完了状態を確認する。
   確認: `_features/README.md` で E02 と B01 が `done` であること。
2. `Display`/`Parse` を `Classes::collect` に追加する。
   確認: `tests/display_parse.rs` に `let x = 42; Display.display (&x)` と `let text = "42"; Parse.parse (&text)` の型検査受理を追加する。
3. `Classes::intrinsic` に組み込み instance 判定を追加する。
   確認: 数値・bool・unit・string の `Display` 制約が通り、record の `Display` 制約は `E1005` で拒否される。
4. `Operation::Builtin` と `Specializer::intrinsic_function` を拡張する。
   確認: `Display.display` を関数値として渡す小テストが `llvm::emit` まで通り、IR が決定的であること。
5. E02 の `FunctionRef::Builtin(BuiltinInstance { builtin, types })` を使い、builtin set と symbol mangling を更新する。
   確認: 既存 builtin `sqrt`, `to_int`, `clone_string`, `Task.run` の IR 名とテストが壊れていないこと。
6. `to_string` を制約付き builtin として追加する。
   確認: `to_string 42`, `to_string true`, `to_string "x"` の所有権テストを追加する。
7. `numeric.c` の `tz_soft_format` に整数 fast path を実装する。
   確認: i8/i16/i32/i64/i128 と unsigned の min/max を native/WASM `-O0`/`-O3` で BigInt 期待値と照合する。
8. `numeric.c` に binary shortest formatter を実装する。
   確認: f16 は全 bit pattern、f32 は代表境界、f64/f128 は Python `decimal`/`fractions` 参照で round-trip と最短性を検査する。
9. `numeric.c` に decimal formatter を既存 decimal decode/pack と同じ意味で実装する。
   確認: d32/d64/d128 の 0、-0、subnormal、最大有限、quantum 末尾ゼロを Python decimal 参照と照合する。
10. `tz_soft_parse` の整数 parser を実装する。
    確認: prefix、underscore、overflow、符号なし負号、最小値を `Option` 結果で検査する。
11. `tz_soft_parse` の binary/decimal float parser を実装する。
    確認: ties-to-even、subnormal、underflow to signed zero、finite overflow failure、`inf`/`nan` を検査する。
12. `emit_builtin` に `Display`/`Parse`/`ToString` の型別 IR を実装する。
    確認: `llvm::emit_target(..., wasm=true)` の IR に `@printf`、libm、未宣言の libcall がないことを文字列検査する。
13. `console_main` を formatter 経由へ変更する。
    確認: scratch ではなく `tests/display_parse.mjs` で console の `0.1`, `-0`, `inf`, `nan`, string を検査する。
14. `python3 src/runtime/generate.py` で `numeric.ll` を再生成する。
    確認: `numeric.ll` にホスト triple 依存・libcall 宣言・`llvm.memcmp` 等がないことを grep する。
15. 既存テスト期待値を新表示へ更新する。
    確認: `tests/primitives.mjs` の f128 表示期待を独立参照実装で再計算して置き換える。
16. docs を更新する。
    確認: `docs/language.md` の組み込み関数表、数値表示、診断表、README のコンソール説明が一致する。

## テスト計画

### Rust frontend/IR tests

`tests/display_parse.rs` を追加する。
受理例:
`def f :: string; fn f = to_string 42`。
`def f :: string; fn f = { let x = 42i8; Display.display (&x) }`。
`def f :: string; fn f = { let x = true; Display.display (&x) }`。
`def f :: string; fn f = { let x = (); Display.display (&x) }`。
`def f :: string; fn f = { let text = "abc"; Display.display (&text) }`。
`def f :: Option i64; fn f = { let text = "42"; Parse.parse (&text) }`。
`def f :: Option f64; fn f = { let text = "0.1"; Parse.parse (&text) }`。
`let show = Display.display; let x = 1i64; show (&x)`。
`let parse_i64: &string -> Option i64 = Parse.parse; let text = "1"; parse_i64 (&text)`。

拒否例:
`record R { x: i64 }; fn f(r: R) -> string { to_string r }` は `E1005`。
`fn f() -> Option string { let text = "x"; Parse.parse (&text) }` は `E1005`。
`fn f() -> string { fn local = 1; to_string local }` のような不正構文は既存 `E0002`。
`fn to_string() -> i64 { 1 }` は builtin 名衝突で `E1001`。
`instance Display i64 { fn display x = "" }` は組み込み instance 上書きで `E1016`。

IR 不変条件:
同じ module を `llvm::emit` 2 回して完全一致。
`to_string 1i64` の IR は `@tz_soft_format` を含み、`@printf` を含まない。
`let text = "x"; Display.display (&text)` は `@tz.string.new` を含む。
`to_string "x"` は余分な clone をしないことを IR の `@tz.string.new` 回数で検査する。
WASM target IR に未知の `@fma`, `@sin`, `@memcmp`, `@printf` 宣言がないこと。

### Node E2E

`tests/display_parse.mjs` を追加する。
native `-O0`/`-O3`、WASM `-O0`/`-O3` を実行する。
WASM は `WebAssembly.Module.imports(module)` が空であること。
native は malloc/free 追跡で各 exported call 後 `live == 0`。
IR を 2 回生成して byte-for-byte 一致。

表示ケース:
i8/i16/i32/i64/i128 の min/max/0/-1。
unsigned 全幅の max。
f16 の全 65,536 bit pattern の display→parse round-trip。
f32 は 0、-0、NaN、±inf、min subnormal、max subnormal、min normal、1±ulp、max finite、ランダム 10,000 bit pattern。
f64 は上記境界とランダム 10,000 bit pattern。
f128 は Python `fractions` で生成した境界・ランダム 2,000 pattern。
d32/d64/d128 は Python decimal context で生成した 0、-0、最大有限、最小非ゼロ、丸め境界。
bool、unit、string（NUL、改行、UTF-8 emoji）。
NaN は payload/sign を期待値にせず、display が常に `nan`、parse 結果が NaN 分類であることだけを検査する。
非 NaN だけ bit-exact round-trip を要求する。
f16/f128/decimal/i128 は公開 ABI で直接 export できないため、Tsuzuri 内部で bool/i64 checksum または 64-bit limb 分割を返す exported helper を使う。
f16 全 bit pattern の網羅は、`tz_soft_format`/`tz_soft_parse` を C/LLVM runtime harness から raw bytes で直接呼ぶ test-only harness で行い、公開 ABI を拡張しない。

解析ケース:
整数 grammar の valid/invalid を全幅で BigInt 参照と照合。
float grammar の valid/invalid を Python `decimal`/`fractions` 参照と照合。
finite overflow は `None`。
`inf`, `-inf`, `nan` は `Some`。
前後空白、空文字、大文字 `INF`, `NaN`, `0X10`, 不正 underscore は `None`。
parse-only program（`Parse.parse` だけを使い `to_string`/console 数値表示を使わない）と to_string-only program を native/WASM `-O0`/`-O3` で build/link し、`numeric.ll` が確実に連結されることを検査する。

既存変更:
`tests/primitives.mjs` の現行 console output expectations を更新する。
`tests/numeric_casts.mjs` は直接の変換仕様を変えないため期待値は変更しない。
ただし `run` の console 出力が formatter 経由になる場合、`builtin_float` の `42` 出力は引き続き `42`。

### 参照実装

整数期待値は JS BigInt で計算する。
binary float の round-trip は Python `fractions` で隣接値区間を計算する。
`mpmath` が使える環境では f128 の追加確認に使ってよいが、必須参照は標準 Python `decimal`/`fractions` にする。
decimal は Python `decimal.Context(prec, Emin, Emax, ROUND_HALF_EVEN, clamp=1)` を使う。
期待値はコンパイラ出力から生成しない。

### 実装完了時の検証コマンド

GUIDE §3 に従い、小さい範囲から次を実行する。

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked --test display_parse
cargo test --locked
cargo build --release --locked
node tests/display_parse.mjs target/release/tsuzuri
node tests/primitives.mjs target/release/tsuzuri
node tests/numeric_casts.mjs target/release/tsuzuri
```

数値の表示・parse lowering を変えるため、`tests/display_parse.mjs` は native/WASM × `-O0`/`-O3`、WASM imports 空、parse-only/to_string-only link、非 NaN bit round-trip と NaN 分類比較を必ず含める。

## ドキュメント

`docs/language.md` の組み込み関数表へ `to_string`、`Display`、`Parse` を追加する。
同じ節で `sqrt`/`floor`/`ceil`/`abs` が D03 後に Math へ移る予定であることを重複なく更新する。
数値仕様のコンソール表示を「最短 round-trip」へ変更する。
`Parse` grammar と `Option` 返却を B01 の節から参照する。
`docs/architecture.md` に formatter/parser が `numeric.c`/`numeric.ll` 由来で、WASM import なしであることを書く。
`README.md` のコンソール説明と実行例を更新する。
`docs/benchmarks.md` は性能主張をしない限り必須ではない。
もし shortest formatter の高速化を主張するなら、整数 fast path と binary formatter の測定条件だけを記録し、CI に速度閾値を置かない。

## 受け入れ条件

- [ ] `Display`/`Parse`/`to_string` の型と所有権規則が仕様どおり。
- [ ] `to_string` と `console_main` が全数値で同じ文字列を返す。
- [ ] f16/f32/f64/f128 は非 NaN の display→parse が bit pattern を保つ。NaN は `nan` と canonical NaN の分類で検査する。
- [ ] decimal は非 NaN の display→parse が数値、符号付きゼロ、special を保つ。NaN payload/sign は保持しない。
- [ ] `Parse` は構文不正・overflow を `None` にし、トラップしない。
- [ ] native と WASM、`-O0` と `-O3` で結果が同じ。
- [ ] WASM imports は空。
- [ ] `printf`/libc/libm に依存しない。
- [ ] parse-only と to_string-only の native/WASM link test があり、`@tz_soft_parse`/`@tz_soft_format` で `numeric.ll` が連結される。
- [ ] `numeric.ll` は `generate.py` で再生成され、手編集されていない。
- [ ] 既存の数値演算・変換テストが壊れていない。
- [ ] docs と README の表示仕様が実装と一致する。

## 落とし穴

`%.17g` は round-trip だが最短ではない。
f64 で作ってから f128 を表示・parse すると二重丸めになる。
`nan` の payload は表示しないが、NaN 比較の仕様は変えない。
`-0` を `0` にしてはいけない。
decimal の末尾ゼロを保持する表示はユーザーには冗長であり、このチケットでは正規化表示を選ぶ。
`Display.display (&string)` と `to_string string` の所有権差を混同しない。
`Parse.parse` は前後空白を trim しない。
WASM で LLVM が `memcmp` や `printf` を導入したら失敗であり、ランタイムへ同梱するか IR を避ける。
`numeric.c` で host `double` を使うと f128/decimal の仕様違反になる。
`Builtin::ALL` の配列長更新漏れはコンパイルエラーになるが、`children` 系の漏れはならない。

## 対象外

汎用 `Debug` 表示。
レコード・union の deriving `Display`。
`Parse` の詳細エラーを返す `Result`。
locale、桁区切り、任意 radix の表示オプション。
format string API。
printf 互換。
NaN payload の文字列表現。
string interpolation のフェーズ 1 実装。

## 未決事項

最短 formatter の具体アルゴリズムは Dragon4 を既定案にする。
Ryu 相当の高速表を入れる場合は、生成スクリプトと表のライセンスを確認し、第三者コードをコピーしない。
decimal の quantum を display/parse round-trip で保存するかは未決。
既定案は保存しない正規化表示。
`Parse string` を提供するかは未決。
既定案は提供しない。
char の `Parse` を D01 に含めるかは A08 の完了時期次第。
既定案は A08 側または D02 側で追加し、D01 は hook だけ用意する。

台帳の見直し提案: なし。
