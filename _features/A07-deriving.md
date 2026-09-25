# A07: deriving（Eq／Ord／Display／Hash／Default の自動導出）

| 項目 | 内容 |
|---|---|
| ID | A07 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | A11, A06, A02, D01 |
| 後続 | C06 |
| 状態 | todo |
| 主な影響ファイル | `src/syntax.rs`, `src/lexer.rs`, `src/parser.rs`, `src/check.rs`, `src/polymorph.rs`, `src/control.rs`, `src/ownership.rs`, `src/llvm.rs`, `src/llvm_frame.rs`, `docs/language.md`, `docs/architecture.md`, `README.md`, `tests/polymorphism.rs`, `tests/control.rs`, `tests/primitives.mjs`, `tests/fixtures/*` |

## 目的

レコードと A02 の判別共用体に対し、構造的な `Eq`/`Ord`/`Display`/`Hash`/`Default` インスタンスを自動生成する。標準ライブラリの `Map`/`Set`/`Result`/`Option` と利用者定義データ型を実用的に組み合わせるため、A06 の条件付きインスタンスと D01 の `Display` 基盤を使う。

生成コードは通常の Tsuzuri コードと同じ型検査・所有権・単相化・LLVM lowering を通す。実行時リフレクション、辞書、GC は導入しない。

## 現状

- `src/syntax.rs` の `RecordDecl` は `name` と `fields` だけを持つ。A02 後の `UnionDecl` も deriving 情報を持たない前提。
- `src/lexer.rs` に `deriving` キーワードはない。GUIDE D-15 で予約済み。
- `src/parser.rs` の `Parser::program` は `record Name { ... }` を読み終えたら即 `program.records.push` する。宣言後 clause を読む場所はここ。
- `src/polymorph.rs` の `Classes::collect` は組み込みクラスとして `Add`, `Sub`, `Eq`, `Ord`, `Copy`, `Capture`, `Send` 等を登録するが、`Hash`/`Default` はない。
- `Classes::instances` は user-written `InstanceDecl` から `$instance.<id>.<method>` 関数を合成して `function_declarations` へ追加する。この流れを deriving の合成 AST に再利用できる。
- `Type::is_copy` は構造的 Copy を既に持つが、`Eq`/`Ord` はレコード/配列/リスト/関数に組み込みではない。A06 で配列/リスト/タプルの条件付き `Eq`/`Ord` が追加される前提。
- D01 は `Display<'a> { def display :: &'a -> string }` と `to_string` を提供する前提。

## 仕様

### 構文

```text
record-declaration ::=
    "record" type-name type-parameters? "{" field-list? "}" deriving-clause?

union-declaration ::=
    "union" type-name type-parameters? "=" union-cases deriving-clause?

type-parameters ::= "<" type-variable ("," type-variable)* ","? ">"

deriving-clause ::=
    "deriving" "(" derive-class ("," derive-class)* ","? ")"

derive-class ::= "Eq" | "Ord" | "Display" | "Hash" | "Default"
```

例:

```text
record Point { x: i64, y: i64 } deriving (Eq, Ord, Display, Hash, Default)

union Shape =
    | Circle of f64
    | Rect of f64 * f64
    | Empty
    deriving (Eq, Display)
```

`deriving` は宣言の直後だけに書ける。別行でもよいが、次のトップレベル宣言の前に現れなければならない。

### 生成する意味

#### Eq

- レコード: フィールド宣言順に左から右へ比較し、すべて true なら true。短絡する。
- union: tag が違えば false。同じ case なら payload を比較。payload なし case は true。
- 浮動小数点は既存 `==` と同じ。NaN は等しくなく、`-0 == +0`。
- 生成インスタンスは各 component に `Eq` を要求する。関数値や `Task` など `Eq` がない component は `E1025`。

#### Ord

- `Ord` deriving は同時に `Eq` を要求する。A06 で `Ord : Eq` の superclass が入っている前提。
- レコード: フィールド宣言順の辞書式。最初に `lt`/`gt` が決まったフィールドで結果を返す。
- union: case 宣言順を tag 順とし、異なる case は tag で比較。同じ case は payload を辞書式。
- 浮動小数点は既存 `< <= > >=` と同じ。NaN を含む比較はすべて false になり得る。総順序は提供しない。

#### Display

- D01 の `Display.display : &'a -> string` を使う。
- レコード形式: `Point { x: 1, y: 2 }`
- union 形式:
  - payload なし: `Empty`
  - 単一 payload: `Some 42`
  - タプル payload: `Rect (1, 2)`
  - record payload: `Named Point { x: 1, y: 2 }`
- 構造内の string/char は **リテラル風に quote する**。standalone `Display<string>` が raw 文字列を返す D01 仕様と区別し、derived structural display では曖昧さを避ける。
  - string は前後に `"` を付ける。各 Unicode scalar を順に処理し、`"` は `\"`、`\` は `\\`、LF/CR/TAB/NUL は `\n`/`\r`/`\t`/`\0`、その他の `U+0000..U+001F` と `U+007F` は uppercase hex の `\u{HEX}`、それ以外は UTF-8 のまま出力する。
  - char は前後に `'` を付ける。`'` は `\'`、`\` は `\\`、LF/CR/TAB/NUL は `\n`/`\r`/`\t`/`\0`、その他の `U+0000..U+001F` と `U+007F` は `\u{HEX}`、それ以外はその scalar の UTF-8 を出力する。
- quote 処理は A07 が所有する compiler-internal builtin として実装する。利用者からは見えない `$builtin.display_quoted_string : &string -> string` と `$builtin.display_quoted_char : &char -> string` を追加し、derived Display だけが呼ぶ。D02 の文字列 API には依存しない。
- 文字列連結は左から右。中間 string の所有権は生成コードが通常通り move/drop する。

#### Hash

このチケットで組み込みクラスを追加する。

```text
class Hash<'a> {
    def hash :: &'a -> i64u
}
```

アルゴリズムは target-independent な 64-bit FNV-1a とする。

```text
offset = 14695981039346656037i64u
prime  = 1099511628211i64u
mix(h, byte) = (h ^ byte) * prime   // i64u wrapping multiplication
```

`Hash.hash` は呼び出しごとに `offset` から開始する。入れ子 component は親の状態を共有せず、component 自身を `offset` から hash した `i64u` を親の canonical stream に 8 byte little-endian で混ぜる。structural delimiter（型 tag、field index、case index、length）もすべて 8 byte little-endian。primitive 値の payload 幅は下表のとおり固定し、target の endian/padding/pointer を使わない。

型ごとの canonical stream:

| 型 | stream |
|---|---|
| bool | `tag(0x01)`, `value(0 or 1)`。どちらも 8 byte little-endian |
| unit | `tag(0x02)` の 8 byte |
| 符号付き/符号なし整数 | `tag(0x10 + bit_width_code)` 8 byte、値の 2 の補数 little-endian を幅どおり（1/2/4/8/16 byte） |
| f16/f32/f64/f128 | `tag(0x20 + bit_width_code)` 8 byte、IEEE bits little-endian。ただし `+0` と `-0` は同じ `+0` bits に正規化 |
| d32/d64/d128 | `tag(0x30 + bit_width_code)` 8 byte、BID bits little-endian。ただし decimal zero は符号を正規化 |
| string | `tag(0x53)` 8 byte、byte length 8 byte、UTF-8 bytes |
| char | `tag(0x43)` 8 byte、Unicode scalar value を i32u little-endian 4 byte |
| tuple | `tag(0x54)` 8 byte、要素数 8 byte、各要素 index 8 byte と component hash 8 byte |
| array/list | A06 の条件付き Hash instance として、`tag(0x41)`/`tag(0x4C)` 8 byte、長さ 8 byte、各要素 index 8 byte と component hash 8 byte |
| record | `tag(0x52)` 8 byte、フィールド数 8 byte、フィールド宣言順に field index 8 byte と component hash 8 byte |
| union | `tag(0x55)` 8 byte、case index 8 byte、payload なしなら `0xffff_ffff_ffff_ffffi64u` 8 byte、payload ありなら component hash 8 byte |

`Hash` は安定した内部データ構造用で、HashDoS 耐性・ランダム seed は対象外。

#### Default

このチケットで組み込みクラスを追加する。

```text
class Default<'a> {
    def default :: 'a
}
```

`Default.default()` のように 0 引数メソッドとして呼ぶ。現在の `Signature::as_type`/`call_signature` は空 parameter の関数型を扱えるため、実装時に regression test を追加する。

生成規則:

- 数値: 0
- bool: false
- unit: `()`
- string: `""`
- char: `'\0'`（A08 後）
- tuple/array/list: tuple は要素 default、array/list は空
- record: 全フィールド default
- union: 最初に宣言された case。payload があれば payload default が必要
- function/task/reference: `Default` なし。component に含まれたら `E1025`

### 所有権・評価順序

- 生成された比較・表示・hash は component を左から右、宣言順に評価する。
- `Display`/`Hash` は借用を受け取り、component も借用して処理する。
- `Eq`/`Ord` は A11 により `&'a -> &'a -> bool` の借用シグネチャであり、演算子も非消費である。derived body は field/payload を借用して比較し、非 Copy component を move しない。
- 生成コードは短絡を維持する。`Eq` は最初の false で残りを評価しない。`Ord` は最初に大小が決まった field/payload で返す。

### 診断

| コード | 条件 |
|---|---|
| `E1025` | deriving 対象外クラス、component に必要 instance がない、function/task/reference 等が含まれる、union がないのに union 専用規則を要求 |
| `E1001` | `deriving (Eq, Eq)` の重複 |
| `E1016` | synthesized instance が既存 user instance と overlap |
| `E1017` | deriving 展開数・生成式深さが上限超過 |

## 設計

### データ構造

`src/syntax.rs`:

```rust
pub enum TokenKind {
    // ...
    Deriving,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeriveClass {
    Eq,
    Ord,
    Display,
    Hash,
    Default,
}

pub struct RecordDecl {
    pub name: Ident,
    pub fields: Vec<Parameter>,
    pub derives: Vec<(DeriveClass, Span)>,
}
```

A02 の `UnionDecl` にも同じ `derives` を追加する。

`src/polymorph.rs` の builtin class 初期化へ `Hash`, `Default` を追加する。`Hash.hash` は `&'a -> i64u`、`Default.default` は `fn() -> 'a` ではなく `def default :: 'a` として扱い、呼び出し側は `Default.default()`。

### 合成方針

`check_modules` の名前収集後、`Classes::collect` と `Classes::instances` の前に deriving を `InstanceDecl` 相当へ展開する。

```rust
fn synthesize_derives(
    modules: &[(&str, &Program)],
    names: &Names,
    records: &[CheckedRecord],
    unions: &[CheckedUnion], // A02
) -> Result<Vec<(String, InstanceDecl)>, Diagnostic>
```

または `Classes::instances` に `derived_instances` を渡して、user instance と同じ overlap 検査を通す。合成 method 名は `$derive.<type-id>.<class>.<method>` ではなく、既存 `$instance.<function_id>.<method>` に最終的に揃える。

生成 AST は `ExprKind`/`Definition` で作る。Rust 側で直接 `TypedExpr` を組み立てない。これにより `computation::expand`、型推論、所有権、再帰、closures、polymorph、LLVM の既存経路を使える。

### 生成コードの形

レコード `Point { x: i64, y: i64 } deriving (Eq)`。A11 により `left`/`right` は `&Point`:

```text
instance Eq<Point> {
    fn eq left right = Eq.eq (&left.x) (&right.x) && Eq.eq (&left.y) (&right.y)
    fn ne left right = !(Eq.eq left right)
}
```

generic record `Box<'a> { value: 'a } deriving (Eq)` は A06 を使う。`left`/`right` は `&(Box<'a>)`:

```text
instance Eq<'a> => Eq<Box<'a>> {
    fn eq left right = Eq.eq (&left.value) (&right.value)
    fn ne left right = !(Eq.eq left right)
}
```

`Ord` も同じく field/payload を明示的に借用して既存 method を呼ぶ。例えば `le` の default を使わずに生成する場合、`fn le left right = Ord.lt left right || Eq.eq left right` のように borrowed parameter 自体を渡し、逆向き比較は `Ord.lt right left` を使う。field 比較では `Ord.lt (&left.x) (&right.x)`、等価判定では `Eq.eq (&left.x) (&right.x)` を使い、`left == right` のように `&Record` 自体へ演算子を適用しない。

union `Option<'a> = None | Some of 'a deriving (Default)`:

```text
instance Default<Option<'a>> {
    fn default = None
}
```

`Some` が最初の case で payload `'a` を持つなら `Default<'a> => Default<Option<'a>>` とし、`Some Default.default()` を生成する。

### LLVM IR 形

- derived method は通常の internal function `@tz.fn.<Module>.$instance.N.method`。
- レコード field access は `extractvalue` または slot 経由の `getelementptr` に下がる。
- union tag/payload は A02 の lowering に従う。異なる case 比較は tag の `icmp`/`switch`。
- `Hash` の primitive は typed LLVM integer ops:
  - byte mix: `xor i64`, `mul i64`（`nuw/nsw` なし、wrapping）
  - float zero normalize: `fcmp oeq value, 0.0` 後 select で +0 bits、または bitcast 後 zero check
  - string bytes は既存 string descriptor `{ptr, i64}` を length loop で読む
- `$builtin.display_quoted_string`/`$builtin.display_quoted_char` は runtime IR の byte/scalar loop で lowering する。native/WASM とも libc の `printf`、locale、Unicode library を呼ばず、必要な長さを計算して `@tz.alloc` で出力 string を確保し、ASCII escape と `\u{HEX}` を自前で書く。WASM imports は増やさない。
- intrinsic 宣言は不要。追加する場合は `FunctionEmitter.intrinsics: BTreeSet<String>` で dedup。

### 変更ステージ

| ステージ | 変更 |
|---|---|
| lexer | `deriving` を `TokenKind::Deriving` として予約 |
| parser | record/union 宣言後に `deriving (...)` を読む。重複 derive は parser か check で `E1001` |
| syntax | `DeriveClass`, `derives` field |
| computation::expand | 変更なし。合成 AST も通常式として展開される |
| check | deriving を synthesized instance に変換し、component requirements を作る。`Hash`/`Default` builtin class を登録 |
| polymorph | A06 の conditional instance と default method 解決を利用。`Hash`/`Default` primitive/conditional intrinsic を追加 |
| control | union tag match を A02 の `PatternStep` と同じ表現で使う。変更は A02 の API に合わせる |
| closures | 変更なし |
| recursion | 合成 method も通常関数として検査 |
| ownership | 比較/表示/hash/default の生成コードを通常検査。非 Copy component を二重 move しないテストを追加 |
| call_specialization | 変更なし |
| llvm | `Hash` primitive helper と structural generated code の lowering。`Display` は D01 の string API と A07 の quoted display builtin を使う |
| llvm_control | union tag switch があれば A02 の lowering を再利用。Eq/Ord の短絡分岐は通常 if/&& |
| llvm_frame | Default で生成される空配列/空リスト/文字列が既存 storage 規則に従う。変更なし |
| runtime | Hash/Display に必要な追加ランタイムは原則なし。D01 の string formatting runtime があれば使用 |
| driver/main | 変更なし |

### 前提とする他チケットのインターフェース

- A11: `Eq`/`Ord` の借用シグネチャと非消費の比較演算子。
- A06: conditional instance、superclass、default method、組み込み配列/リスト/タプル `Eq`/`Ord`。
- A02: `Type::Union(usize, Vec<Type>)`、union case metadata、tag/payload lowering、case constructor/pattern。
- D01: `Display<'a> { def display :: &'a -> string }`、string 連結/formatting helper、`to_string`。
- A08: `char` がある場合、Hash/Default の primitive instance と quoted char display builtin の入力型を追加する。standalone `Display<char>` は D01 が所有する。

### 他チケットへの提供インターフェース

- C06 は `Hash`/`Ord`/`Eq` を Map/Set key の制約として使える。
- D01 は derived Display の quote helper を公開しなくてもよいが、A07 が同じ escaping を使える内部関数を共有するのが望ましい。

## 実装手順

1. **構文追加**
   - `TokenKind::Deriving`, `Lexer::identifier`, `DeriveClass`, `RecordDecl.derives`, A02 の `UnionDecl.derives` を追加。
   - `Parser::program` の record/union 分岐で `deriving` clause を読む。
   - 確認: `record Point { x: i64 } deriving (Eq, Display)` の parse テスト。
2. **Hash/Default クラス追加**
   - `Classes::collect` の builtin class 配列に `Hash`, `Default` を追加。
   - `Default.default` の 0 引数 method value/call をテスト。
   - 確認: `def f :: i64\nfn f = Default.default()` が primitive instance 後に通る。
3. **derive synthesis**
   - record/union metadata から `InstanceDecl` を生成。
   - generic 型は A06 の context を作る。component ごとに必要 class を列挙し、重複を BTreeSet で deterministic にする。
   - 確認: 合成 instance と user instance の overlap が `E1016`。
4. **Eq/Ord**
   - field/case/payload 順で AST を生成。短絡 `&&`/`||` と `if` を使い、評価順を固定。
   - 確認: NaN、signed zero、case order、field order。
5. **Display**
   - D01 の API を使って構造文字列を構築。
   - `$builtin.display_quoted_string : &string -> string` と `$builtin.display_quoted_char : &char -> string` を追加し、escape grammar と native/WASM lowering を実装する。
   - 確認: `Point { x: 1, y: "a\n" }` が期待文字列。
6. **Hash/Default**
   - primitive Hash/Default と structural derive を実装。
   - `Hash` は native/WASM で同じ i64u を返す。
   - 確認: JS BigInt の独立 FNV 実装と照合。
7. **E2E と文書**
   - fixtures と Node tests を追加し、native/WASM × `-O0`/`-O3`、heap `live == 0`。

## テスト計画

### 受理

```text
record Point { x: i64, y: i64 } deriving (Eq, Ord, Display, Hash, Default)
def main :: bool
fn main = {
    let p = Point { x: 1, y: 2 };
    let q = Point { x: 1, y: 3 };
    p != q && p < q
}
```

```text
union Option<'a> = None | Some of 'a deriving (Eq, Display, Default)
def main :: bool
fn main = Some 42 == Some 42 && Default.default() == None
```

```text
record Box<'a> { value: 'a } deriving (Eq, Hash)
def same :: Eq<'a> => Box<'a> -> Box<'a> -> bool
fn same x y = x == y
```

### 拒否

| プログラム | 期待 |
|---|---|
| `record R { f: i64 -> i64 } deriving (Eq)` | `E1025` |
| `record R { t: Task<i64> } deriving (Default)` | `E1025` |
| `record R { x: i64 } deriving (Eq, Eq)` | `E1001` |
| user `instance Eq<R>` と `deriving (Eq)` の併用 | `E1016` |
| component に `Display` がない型で `deriving (Display)` | `E1025` |
| union 全 case が payload default 不可で `Default` | `E1025` |

### E2E 期待値

GUIDE §3 に従い、Node E2E の直前に必ず `cargo build --release --locked` を実行し、古い `target/release/tsuzuri` を使わない。

- `hash_point(1,2)` は JS の FNV-1a 実装で `tag 0x52`, field 0/1, little-endian i64 を mix して期待値を計算。
- quoted Display は `"\n\t\"\\\0"` と `'\''`, `'\\'`, `'\u{1F600}'` を含む fixture で、A07 builtin の出力を期待文字列と照合する。native/WASM とも libc なし、WASM imports 空。
- `display_shape()` は `"Rect (1, 2)"`。
- `default_checksum()` は default record/union を作り、フィールド和を返す。
- float Eq: `nan_point_eq()` は false、`zero_point_hash()` は +0/-0 で同値。
- native/WASM × `-O0`/`-O3`、WASM imports 空、IR 2 回一致、heap `live == 0`。

## ドキュメント

- `docs/language.md`
  - `deriving` 構文、各クラスの構造的意味、Hash アルゴリズム、Default 規則。
  - 診断表に `E1025`。
- `docs/architecture.md`
  - deriving は AST 合成であり、型付き IR を直接作らないこと。
  - Hash の target-independent contract。
- `README.md`
  - 型クラス/データ型の例へ `deriving` を追加。

## 受け入れ条件

- [ ] record/union で `deriving (Eq, Ord, Display, Hash, Default)` を parse できる。
- [ ] 合成 instance が user instance と同じ coherence 検査を受ける。
- [ ] generic record/union が A06 の conditional instance を生成する。
- [ ] Eq/Ord の field/case 順、float semantics、短絡が仕様通り。
- [ ] Display 文字列が仕様通り quote/escape される。
- [ ] `$builtin.display_quoted_string` と `$builtin.display_quoted_char` が native/WASM で同じ escaping を行い、D02 に依存しない。
- [ ] Hash が canonical stream 仕様通り native/WASM で一致し、JS 独立実装と一致する。
- [ ] Default が 0 引数 method として動作する。
- [ ] native/WASM × `-O0`/`-O3`、heap `live == 0`、IR 決定性。

## 落とし穴

- Rust 側で `TypedExpr` を直接組み立てると、`children`/`children_mut`、所有権、単相化の漏れを作りやすい。必ず AST 合成を基本にする。
- Hash で platform endian やポインター値を混ぜない。必ず little-endian の値表現。
- `Default.default` は `unit -> 'a` ではない。`Default.default()` で 0 引数関数として呼ぶ。
- Display の string quote と standalone string Display を混同しない。
- derived Eq/Ord は A11 の借用 comparison を使う。非 Copy component を複数回 move しないよう、field/payload の借用と短絡順序を検査する。

## 対象外

- `deriving Copy`。Copy は既存の構造的性質。
- ユーザー定義 derive macro。
- HashDoS 耐性、ランダム seed、暗号学的 hash。
- 総順序 float、NaN 正規化順序。
- private field の表示抑制。E01 後も同一モジュールの合成コードとして全 field を使う。

## 未決事項

- **D-20/A11 で決定済み:** `Eq`/`Ord` は借用シグネチャで、derived comparison は呼び出し元の値を消費しない。A07 では追加の台帳見直しを提案しない。
- **既定案: union Default は最初の case。** 別 case を指定する構文は導入しない。
- **既定案: derived Display は structural context で string/char を quote する。** D01 の standalone `Display<string>` は raw のまま。
