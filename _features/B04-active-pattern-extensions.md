# B04: アクティブパターンの拡張（Option 返却・複数ケース）
| 項目 | 内容 |
|---|---|
| ID | B04 |
| 優先度 | P1 |
| 規模 | M |
| 依存 | B01, A02 |
| 後続 | A03, D01 |
| 状態 | todo |
| 主な影響ファイル | `src/syntax.rs`, `src/parser.rs`, `src/parse_control.rs`, `src/check.rs`, `src/control.rs`, `src/ownership_control.rs`, `src/llvm_control.rs`, `src/llvm.rs`, `tests/control.rs`, `tests/fixtures/control/`, `tests/control.mjs`, `docs/language.md`, `docs/architecture.md`, `README.md` |

## 目的

既存のアクティブパターンを次の 2 方向に拡張する。

1. 部分アクティブパターン `(|Name|_|)` が `bool` だけでなく `Option payload` を返せるようにし、成功時の payload を後続パターンへ渡す。
2. 全域の複数ケースアクティブパターン `(|Even|Odd|)` を追加し、recognizer が返す union case に応じてパターンケースを選ぶ。

これにより、D01 の `Parse`、字句分類、軽量な view pattern を、所有権と静的型検査を保ったまま表現できる。

## 現状

- `docs/language.md` の「アクティブパターン」:
  - 単一ケース全域 `(|Name|) :: input -> payload` は対応済み。
  - bool 部分 `(|Name|_|) :: input -> bool` は対応済み。
  - Option 返却の部分形式と複数ケース形式は未対応と明記。
- `src/parse_control.rs::function_name`:
  - `(|Name|)` または `(|Name|_|)` だけを受理する。
  - ケース名は大文字始まりを要求。
  - `ActivePattern { name, function, partial }` を `Parser.active_patterns` へ登録する。
  - 関数名は `$active.<Case>.<partial|total>` に変換する。
- `src/syntax.rs::ActivePattern` は `name: Ident`, `function: String`, `partial: bool` だけを持つ。
- `src/check.rs::check_modules`:
  - `program.active_patterns` を見て recognizer 関数 ID を `Names.active_patterns` に登録する。
  - `partial == true` の場合、戻り値が `Type::Bool` でなければ `E1003`。
- `src/control.rs`:
  - `Checker::active_recognizer` は名前から `(function_id, partial_bool)` を返す。
  - `Checker::active_pattern` は partial なら `PatternStep::Test(call)` だけを作り、payload は持てない。
  - total なら recognizer 結果を一時 local に `PatternStep::Bind` し、その payload pattern を `pattern_alternatives` で照合する。
- `src/ownership_control.rs::eval_match` と `src/llvm_control.rs::match_expression` は、失敗した `PatternStep::Bind` の一時所有値を drop/zero する既存処理を持つ。

### 前提とする他チケットのインターフェース

- **B01**: `Option 'a = None | Some of 'a` が std fallback で解決できる。
- **A02**: `Type::Union(usize, Vec<Type>)`、union case pattern、case tag test、payload projection、union clone/drop が実装済み。
- **A03**（後続）: match exhaustiveness。B04 は A03 に情報を提供するが、A03 未実装でも通常の上から順の match として動く。

### 他チケットへの提供インターフェース

- D01 は `def (|Parse|_|) :: &string -> Option i64` を使い、`| Parse n -> ...` と書ける。
- A03 は `ActivePatternInfo::TotalCases`（下記）を使って、同じ recognizer の全 case が guard なしで出ている場合に exhaustiveness の材料にできる。

## 仕様

### Option 返却部分アクティブパターン

```text
def (|Parse|_|) :: &string -> Option i64
fn (|Parse|_|) text = D01.parse_i64 text

match text with
| Parse n -> n + 1
| _ -> 0
```

意味:

- recognizer の追加引数を左から右に評価し、最後に matched input を渡す。
- 戻り値が `Some payload` なら成功し、payload を後続パターンに渡す。
- 戻り値が `None` なら失敗し、次の pattern alternative / 次の arm へ進む。
- `Parse n` の `n` は payload pattern。`Parse (n, rest)` のような任意の既存 pattern が使える。
- `Parse` だけで payload pattern を省略できるのは payload が `unit` の場合だけ。
- 既存の bool 部分 recognizer は互換維持:
  - `def (|Even|_|) :: i64 -> bool`
  - `| Even -> ...`
  - payload pattern は持てない。`Even x` は extras 引数と解釈されるため、bool partial の引数数が合わなければ `E1006`。

### 複数ケース全域アクティブパターン

phase 1 では **recognizer の戻り値はユーザー宣言 union** とする。コンパイラは隠し union を合成しない。
A02 は同一モジュール内で union case と active-pattern case の同名宣言を `E1001` で拒否し、pattern 解決でも union case を recognizer より先に解決する。そのため、複数ケース active pattern の backing union case 名は active case 名と別名にし、宣言順で対応付ける。

```text
union ParityView =
    | ParityEven
    | ParityOdd

def (|Even|Odd|) :: i64 -> ParityView
fn (|Even|Odd|) n =
    if n % 2 == 0 then ParityEven else ParityOdd

match 41 with
| Even -> 0
| Odd -> 1
```

payload あり:

```text
union TokenView =
    | TokenInt of i64
    | TokenWord of string
    | TokenSymbol of ubyte

def (|Int|Word|Symbol|) :: &string -> TokenView
fn (|Int|Word|Symbol|) text = ...

match token with
| Int n -> n
| Word word -> word.length
| Symbol byte -> byte as i64
```

制約:

- `(|Case1|Case2|...|)` の各 case は PascalCase、大文字始まり。
- `_` を含む形式は部分 recognizer であり、複数ケースとは併用不可。`(|A|B|_|)` は `E1020`。
- recognizer の戻り union は、union case 数が active pattern の case 数と一致しなければ `E1020`。
- active case と backing union case は宣言順で対応する。上の例では `Even` ↔ `ParityEven`、`Odd` ↔ `ParityOdd`。
- active case と対応する backing union case の payload shape が一致していればよい。名前一致は要求しないし、同名は同一モジュールでは A02 により `E1001`。
- 各 active case pattern は、対応する backing union case の payload を後続 pattern へ渡す。
- payload のない case は `| Even ->` と書く。`| Even x ->` は `E1006`。
- backing union case に payload がある active case では payload pattern が必要。payload がない backing case に payload pattern を書くと `E1006`。

### 名前解決

- recognizer 定義:
  - `def (|Parse|_|)` は内部関数名 `$active.Parse.partial`。
  - `def (|Even|Odd|)` は内部関数名 `$active.Even.Odd.total`。
- pattern での解決:
  - A02 の名前解決表どおり union case を active recognizer より先に解決する。
  - active recognizer は origin-aware alias で解決する。利用者モジュールからの無修飾参照は「自モジュール → 利用者モジュールで一意 → 標準ライブラリ」。標準ライブラリからの参照は D-07 に従い「自 std モジュール → 標準ライブラリ」で、利用者宣言を見ない。
  - 同じ可視範囲に複数の active case alias があれば `E1004` で修飾を要求する。
- `Names.active_patterns` は case 名 alias ごとに function ID と case ID だけを持つ。payload `Type` は多相 recognizer の呼び出しごとに `self.function(id)` で fresh instantiate した signature から導く。

### 評価順序と一時値

- pattern は既存どおり上から順、左から右。
- active recognizer の追加引数は左から右に一度だけ評価し、最後に matched input を渡す。
- Option partial:
  1. recognizer call を評価し、一時 local に束縛する。
  2. tag が `Some` なら payload を pattern 照合する。
  3. tag が `None` なら一時 local を drop して次へ進む。
- multi-case total:
  1. recognizer call を評価し、一時 local に束縛する。
  2. tag が対象 case と等しいか test する。
  3. case が違えば一時 local を drop して次へ進む。
  4. case が一致すれば payload を pattern 照合する。
- 失敗した recognizer の一時所有値も解放する。これは既存 docs の「失敗した認識器やガードの一時所有値も解放」を拡張して維持する。
- bool partial は既存どおり `PatternStep::Test(call)` で、一時 payload local は作らない。

### 診断

新しい診断コードは追加しない。既存 `E1020` を active pattern 形式不正に使う。

| 条件 | コード |
|---|---|
| `(|A|B|_|)` | `E1020` |
| case 名が小文字始まり | `E1020` |
| Option partial で戻り値が `Option` でも `bool` でもない | `E1003` |
| bool partial に payload pattern を指定 | `E1006` |
| Option partial の payload pattern 数が不正 | `E1006` |
| multi-case total の戻り値が union でない | `E1003` |
| multi-case total の active case 数と backing union case 数が一致しない | `E1020` |
| 同一モジュール内で active case と backing union case が同名 | `E1001` |
| 無修飾 active case が複数の可視 recognizer に一致する | `E1004` |
| mutable reference input を要求 | 既存どおり `E1014` |
| 非 Copy matched input を値で消費し、次 arm が必要 | 既存どおり `E1014` |

## 設計

### 構文データ

`src/syntax.rs`:

```rust
#[derive(Clone, Debug)]
pub enum ActivePatternKind {
    TotalSingle,
    Partial,
    TotalCases(Vec<Ident>),
}

#[derive(Clone, Debug)]
pub struct ActivePattern {
    pub cases: Vec<Ident>,
    pub function: String,
    pub kind: ActivePatternKind,
}
```

互換のため `cases[0]` が従来の `name` に相当する。

`src/check.rs` 側の名前表:

```rust
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct ActivePatternRef {
    function: usize,
    case: ActiveCaseRef,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ActiveCaseRef {
    BoolPartial,
    OptionPartial,
    TotalSingle,
    TotalCase { case_index: usize },
}

struct ActivePatternAliases {
    qualified: BTreeMap<String, ActivePatternRef>,
    user_aliases: BTreeMap<String, Vec<(String, ActivePatternRef)>>,
    std_aliases: BTreeMap<String, Vec<(String, ActivePatternRef)>>,
}
```

`ActivePatternRef` には payload `Type` を保存しない。`(|TryParse|_|) :: &string -> Option 'a` のような多相 recognizer では、呼び出しごとに `Checker::function(id)` が fresh な `Infer` を含む signature を返すため、その fresh signature から `Option` payload 型や backing union の具体 payload 型を導く。

### Parser

`src/parse_control.rs::function_name` を拡張する。

受理:

```text
(|Name|)
(|Name|_|)
(|Case1|Case2|)
(|Case1|Case2|Case3|)
```

拒否:

```text
(|A|B|_|)
(|_|)
(|a|B|)
```

アルゴリズム:

1. `(` `|` を読んだ後、`Ident` case を 1 個以上読む。
2. 各 case の後に `|` を要求。
3. 次 token が `_` なら partial。partial は case が 1 個だけのときだけ許可し、続く `|` と `)` を読む。
4. 次 token が `RightParen` なら終了。
5. それ以外なら次 case を読む。
6. 内部関数名:
   - partial: `$active.<Case>.partial`
   - total single: `$active.<Case>.total`
   - total cases: `$active.<Case1>.<Case2>...total`

### Check

`src/check.rs::check_modules` の active pattern 登録を変更する。

- partial:
  - recognizer は少なくとも input parameter を 1 つ持つ。
  - result が `bool` なら `BoolPartial`。
  - result が `Option payload` なら `OptionPartial`。payload 型はここで cache しない。
  - それ以外は `E1003` `"a partial active recognizer must return bool or Option payload"`。
- total single:
  - 既存と同じ。result は任意 payload。
- total cases:
  - result は `Type::Union(union_id, args)`。
  - A02 の `CheckedUnion` から backing union case 数と payload shape を取得する。
  - active case 数と backing union case 数が一致すること。case 名一致は要求しない。
  - active cases は declaration order で backing union cases に対応する。payload の有無・タプル構造などの shape が pattern 側の期待と一致するかは `active_pattern` で fresh instantiated result 型から検査し、不一致は `E1006` または通常の型エラーにする。
  - `Names.active_patterns` に各 active case 名の alias を登録する。同じモジュール内で union case/record/union/active case 名が重複したら A02 と同じく `E1001`。

resolver:

```rust
impl Names {
    fn active_pattern(
        &self,
        requester: &str,
        name: &Ident,
        requester_is_std: bool,
    ) -> Result<Option<ActivePatternRef>, Diagnostic> {
        // 1. qualified Module.Case: exact qualified lookup
        // 2. unqualified: requester module
        // 3. if requester is user: unique visible user alias
        // 4. std alias
        // ambiguity => E1004
    }
}
```

`src/control.rs::active_recognizer` は `Option<_>` ではなく `Result<Option<ActivePatternRef>, Diagnostic>` を返す。`pattern_alternatives` の binding 名判定も、この resolver の `Ok(Some(_))` だけを active pattern とみなす。`E1004` ambiguity を `unknown active pattern` に潰さない。

### Control lowering

`src/control.rs::active_recognizer` は `Result<Option<ActivePatternRef>, Diagnostic>` を返す。

`Checker::active_pattern` の分岐:

#### bool partial

既存維持:

```rust
steps: vec![PatternStep::Test(call)]
bindings: []
```

#### Option partial

入力:

```text
| Parse payload_pattern -> body
```

生成:

1. recognizer call: `call : Option payload_type`
2. 一時 local `$recognizerN : Option payload_type`
3. `PatternStep::Bind(local, call)`
4. `PatternStep::Test(tag_is_some(local))`
5. payload projection `Some` の payload を `payload_pattern` へ `pattern_alternatives`

A02 に union case test/projection helper があるならそれを使う。なければ `TypedExprKind` に union tag/payload projection がある前提で追加するのは A02 の範囲。`payload_type` は `self.function(id)` の fresh signature の result から取り出す。

#### total multi-case

入力:

```text
| Even -> body
| Odd -> body
```

生成:

1. recognizer call: `call : ParityView`
2. 一時 local `$recognizerN : Parity`
3. `PatternStep::Bind(local, call)`
4. `PatternStep::Test(tag_eq(local, backing_case_index))`
5. 対応する backing union case に payload があるなら、その payload projection を後続 pattern へ渡す。payload が unit/none なら pattern なし。
6. backing union case の concrete payload 型は `self.function(id)` の fresh signature result（`Type::Union(union_id, args)`）から A02 の substitution helper で導く。alias 登録時の古い `Type` を使い回さない。

### Ownership

既存 `src/ownership_control.rs::eval_match` は `PatternStep::Bind` の一時 local を `temporaries` に入れ、`PatternStep::Test` 失敗時に `finish_control_scope(&temporaries, ...)` する。Option partial / multi-case total はこの経路へ載せる。

確認点:

- `Some string` payload を pattern が失敗した場合、一時 `Option string` が drop される。
- `Int n when false` の guard 失敗でも recognizer result が drop される。
- bool partial は一時 payload がないため従来通り。
- matched input が `string` など非 Copy で recognizer が値取りを要求する場合、次 arm のために入力を保持できず `E1014`。`&string` を使うよう診断する。

### LLVM

`src/llvm_control.rs::match_expression` は既に:

- `PatternStep::Bind` を一時 slot に store。
- `PatternStep::Test` が false の場合 `clear_pattern_temporaries` で一時値を drop/zero。
- guard 失敗時も `clear_pattern_temporaries`。

Option partial / multi-case total は `PatternStep::Bind + Test` へ落とすため、既存形を拡張しやすい。

A02 の union tag test の代表 IR:

```llvm
%tag = extractvalue %"tz.union.Main.Parity" %value, 0
%ok = icmp eq i32 %tag, 0
br i1 %ok, label %case, label %next
```

payload projection は A02 のレイアウト helper を使い、inactive case の payload を読まない。

### Exhaustiveness (A03 連携)

B04 は A03 を実装しないが、以下の情報を提供する。

- active pattern case が `TotalCase { case_index }` のとき、同じ recognizer function に属する active case ID 集合を取得できる。
- A03 は guard なし、同じ matched input、同じ extras 引数、同じ recognizer の全 case が揃う場合に exhaustiveness を満たすと扱える。
- Option partial は部分 pattern なので exhaustiveness には寄与しない。
- bool partial も同じ。

## 実装手順

1. **構文データを拡張する**
   - `ActivePattern` を `cases`/`kind` ベースへ変更。
   - 既存テストが壊れないよう total single / bool partial を同じ意味で保持。
   - 確認: `cargo test --locked --test control active_recognizers_are_typed_calls_with_ordered_patterns`（`running N tests` の N が 0 でないこと）。
2. **parser を拡張する**
   - `function_name` で `(|A|B|)` を読む。
   - 不正形を `E1020`。
   - 確認: `cargo test --locked --test control active_pattern_extensions_parse`（`running N tests` の N が 0 でないこと）で `(|A|B|_|)`、小文字 case を拒否。
3. **check_modules の登録を拡張する**
   - partial result が `bool` または `Option payload` か検査。
   - total multi-case result が union か検査。
   - active case 数と backing union case 数の一致を検査する。同名 case は A02 の重複検査で `E1001`。
   - origin-aware alias resolver と `E1004` ambiguity を `tests/control.rs` に追加する。
   - 確認: `cargo test --locked --test control active_pattern_extensions_resolve`（`running N tests` の N が 0 でないこと）。
4. **control lowering を拡張する**
   - `active_pattern` で `OptionPartial` と `TotalCase` を `PatternStep::Bind + Test + payload pattern` へ展開。
   - extras 引数と payload pattern の個数を検査。
   - 確認: `cargo test --locked --test control active_pattern_extensions_payloads`（`running N tests` の N が 0 でないこと）で payload destructuring、追加引数、guard を Rust テスト。
5. **ownership/LLVM の一時値解放を検査する**
   - 既存処理で不足があれば `clear_pattern_temporaries` と `finish_control_scope` の対象を調整。
   - 確認: `cargo test --locked --test control active_pattern_extensions_ownership`（`running N tests` の N が 0 でないこと）と E2E で owned string payload の失敗・guard false・次 arm で `live == 0`。
6. **E2E を追加する**
   - `tests/fixtures/control` に Option partial / multi-case total の export 関数を追加。
   - `tests/control.mjs` に expected values/traps を追加。
7. **ドキュメント更新**
   - 未対応記述を削除し、新構文と制約を追加。

## テスト計画

### Rust 受理

Option partial:

```text
def (|Parse|_|) :: &string -> Option i64
fn (|Parse|_|) text =
    if text.length == 2 then Some 42 else None

match "42" with
| Parse n -> n
| _ -> 0
```

Option partial payload destructuring:

```text
def (|Parts|_|) :: i64 -> Option (i64 * i64)
fn (|Parts|_|) n = if n > 0 then Some (n, n + 1) else None
match 20 with
| Parts (a, b) -> a + b
| _ -> 0
```

追加引数:

```text
def (|Divisible|_|) :: i64 -> i64 -> Option unit
fn (|Divisible|_|) divisor n =
    if n % divisor == 0 then Some () else None
match 42 with
| Divisible (1 + 2) -> 1
| _ -> 0
```

multi-case:

```text
union ParityView = ParityEven | ParityOdd
def (|Even|Odd|) :: i64 -> ParityView
fn (|Even|Odd|) n = if n % 2 == 0 then ParityEven else ParityOdd
match 41 with
| Even -> 0
| Odd -> 1
```

multi-case payload:

```text
union NumberView = ViewSmall of i64 | ViewLarge of i64
def (|Small|Large|) :: i64 -> NumberView
fn (|Small|Large|) n = if n < 10 then ViewSmall n else ViewLarge n
match 12 with
| Small n -> n
| Large n -> n + 30
```

### Rust 拒否

| プログラム | 期待コード |
|---|---|
| `def (|Parse|_|) :: i64 -> i64 ...` | `E1003` |
| `def (|A|B|_|) :: i64 -> bool ...` | `E1020` |
| `def (|a|B|) :: i64 -> ParityView ...` | `E1020` |
| `union P = X; def (|A|B|) :: i64 -> P ...` | `E1020` |
| `union Bad = A | B; def (|A|B|) :: i64 -> Bad ...` in the same module | `E1001` |
| `def (|Even|_|) :: i64 -> bool ...; match 2 with | Even n -> n` | `E1006` |
| `def (|Bad|_|) :: &mut i64 -> Option unit ...` | `E1014` |
| `def (|Bad|_|) :: string -> Option unit ...; match "x" with | Bad -> 1 | _ -> 0` | `E1014` |

### E2E

追加 export:

- `active_option_parse()` -> 42
- `active_option_none()` -> 0
- `active_option_owned(count)` -> `count * 5`（失敗 recognizer が作った owned string payload を解放）
- `active_multi_parity(n)` -> `0/1`
- `active_multi_payload(n)` -> JS 参照で計算
- `active_multi_guard()` -> guard false 後に一時 payload 解放、次 arm 成功
- traps:
  - recognizer 内で明示トラップする入力は従来どおり trap。
  - pattern 不一致で次 arm に進む場合、後続の `1 / 0` が選択されなければ trap しない。

検証:

- `cargo build --release --locked && node tests/control.mjs target/release/tsuzuri`
- native/WASM × `-O0`/`-O3`
- `live == 0`
- WASM imports なし
- IR 決定性

## ドキュメント

- `docs/language.md`:
  - 「Option／Choice 型が未導入のため未対応」を削除。
  - bool partial、Option partial、multi-case total の構文・型・例を追加。
  - 失敗 recognizer の一時所有値も解放する規則を維持して明記。
- `docs/architecture.md`:
  - `PatternStep::Bind + Test` で Option partial/multi-case を表すこと。
  - A03 が使える active pattern metadata を記載。
- `README.md`:
  - 制御構文とパターンの節に短い `Parse` 例を追加。

## 受け入れ条件

- [ ] `(|Name|_|) :: input -> bool` の既存コードがそのまま動く。
- [ ] `(|Name|_|) :: input -> Option payload` で `Name payload_pattern` が使える。
- [ ] `None` の場合に次 arm へ進み、一時 payload/recognizer result を解放する。
- [ ] `(|A|B|) :: input -> BackingUnionWithSameCaseCount` が宣言順対応で case ごとの pattern として使える。
- [ ] multi-case の active case 数と backing union case 数の不一致を `E1020` で拒否する。
- [ ] 同一モジュール内で active case と union case が同名なら A02 と同じく `E1001`。
- [ ] active recognizer resolver が current module → unique visible user → std、std requester は user を見ない、曖昧なら `E1004` を満たす。
- [ ] 多相 recognizer の payload 型は各使用箇所の fresh instantiated signature から導き、alias 登録時の `Type` を cache しない。
- [ ] mutable input や非 Copy consuming input の既存安全性を破らない。
- [ ] native/WASM × `-O0`/`-O3`、heap tracking、WASM import なし、IR 決定性を確認した。
- [ ] A03 が利用できる recognizer/case metadata を保持する。

## 落とし穴

- Option partial は `Some payload` の payload を pattern に渡す。`Option (unit)` の場合だけ payload pattern 省略を許す。
- bool partial と Option partial を同じ `partial: bool` だけで表すと payload を扱えない。enum に分ける。
- multi-case で隠し union を合成しない。型表示・ABI・exhaustiveness が複雑になるため phase 1 はユーザー宣言 union。
- recognizer result の union payload は inactive case で読まない。
- pattern 失敗・guard 失敗・次 alternative への移動で一時 union result を必ず drop/zero する。
- active pattern case と union case の名前衝突は同一モジュールでは `E1001`。別モジュールで衝突する場合も pattern 解決では union case が優先し、active recognizer は resolver で曖昧性を判定する。

## 対象外

- コンパイラによる hidden `Choice`/active-choice union の自動生成。
- .NET/F# の完全互換、`ValueOption`、型テスト、null pattern。
- active pattern の exhaustiveness 本体。A03 の担当。
- 複数ケース部分 active pattern `(|A|B|_|)`。
- recognizer の結果を cache して複数 arm 間で一度だけ評価する最適化。

## 未決事項

- **既定案:** multi-case total の戻り値はユーザー宣言 union とし、active case と backing union case は宣言順で対応する。case 名一致は要求せず、同一モジュールで同名なら A02 により `E1001`。
- hidden union を合成する案は、型表示・ドキュメント・A03 連携・LLVM 型名の決定性が増えるため、このチケットでは採用しない。
- active pattern alias の cross-module 曖昧性は `E1004` で修飾要求する。resolver は `Result<Option<_>, Diagnostic>` とし、曖昧性を unknown pattern に潰さない。
