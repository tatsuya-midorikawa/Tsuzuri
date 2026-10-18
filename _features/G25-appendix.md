# G25 付録

[G25 本体](G25-typed-holes-and-find.md)の付録。A: 位置ごとの期待型と Phase 1 の試作の差分、B: 検証済みのコード、C: 未解決名の発行箇所と `pool`、D: テストの入力と期待値、E: 実測。

**試作と検証の方法**。HEAD（`a2313d19`、作業ツリー `9dd0fbbc`）のソースを `target/` の下の作業ディレクトリに複製し、そこだけに Phase 1〜3 の試作を入れ、`CARGO_TARGET_DIR` を別にして `cargo build --offline`（debug）でビルドして実測した（測定の後に削除した）。リポジトリのソース・テスト・文書は変えていない。B.1 は単独の `rustc --edition 2024 --test` で 3 件が通った。B.2・B.3 は複製の中でビルドでき、出力は付録 E のとおり。B.2 のコードは、ビルドした試作と空白・コメント・`Query` の置き場所を除いて同じ（機械的に比べた）。**実装では下のコードを使い、実測の数字は release ビルドで取り直す**。

## A. 位置ごとの期待型と Phase 1 の試作

### A.1 期待型の表（実測）

次の 1 つのソース（`tests/typed_holes.rs` の `expected_type_follows_the_position` が使う）に `tsuzuri check` をかけると、15 個の穴がそれぞれ 1 件の `E1030` になる。

```text
record P { x: i64, y: bool }
def g :: i64 -> bool -> i64 = \n b -> n
def p01 :: i64 -> i64 = \n -> n + ?
def p02 :: i64 -> i64 = \n -> ?
def p03 :: i64 -> i64 = \n -> g n ?
def p04 :: i64 -> i64 = \n -> if n > 0 then ? else 0
def p05 :: i64 -> P = \n -> P { x: ?, y: true }
def p06 :: i64 -> i64 = \n -> match n with | 0 -> ? | _ -> 1
def p07 :: bool -> bool = \b -> ? && b
def p08 :: i64 -> i64 = \n ->
    let a: i64 = ?
    a
def p09 :: i64 -> i64 = \n ->
    let b = ?
    n
def p10 :: i64 -> i64 = \n ->
    let c: [i64] = [1, ?]
    n
def p11 :: i64 -> i64 = \n ->
    let d = (1, ?)
    n
def p12 :: i64 -> i64 = \n ->
    let h = \z -> ?
    n
def p13 :: [i64] -> i64 = \xs -> Array.length ?
def p14 :: i64 -> i64 = \n -> g ? true
def p15 :: i64 -> i64 = \n -> ? n
p01 1
```

| 行 | 位置 | `detail.expected` |
| --- | --- | --- |
| 3 | 二項演算の右辺（`n + ?`。左が `i64`） | `i64` |
| 4 | 関数本体の末尾（結果 `i64`） | `i64` |
| 5 | 呼び出しの引数（`g n ?`。2 番目は `bool`） | `bool` |
| 6 | `if` の枝 | `i64` |
| 7 | record の field（`x: i64`） | `i64` |
| 8 | `match` の腕（結果 `i64`） | `i64` |
| 9 | `? && b` の左辺 | `_` |
| 11 | 注釈つきの `let` | `i64` |
| 14 | 注釈のない `let` | `_` |
| 17 | 注釈つきの配列の要素 | `i64` |
| 20 | タプルの要素 | `_` |
| 23 | 無名関数の本体（結果が未確定） | `_` |
| 25 | `Array.length ?`（引数は `ref [_]`） | `ref [_]` |
| 26 | 呼び出しの引数（`g ? true`。1 番目は `i64`） | `i64` |
| 27 | 呼び出しの関数の位置（`? n`） | `_` |

`scope` の例: 行 23 の穴は、`z`（型は未確定で `_`）と `n: i64` を見る。行 25 は `xs: [i64]`。

### A.2 Phase 1 の試作の差分（そのまま使える）

`syntax.rs`:

```diff
@@ TokenKind
     At,
     Hash,
+    Question,
     Arrow,
@@ ExprKind の `visit` の葉の群
             | Break
             | Continue
+            | Hole(_)
             | Name(_)
@@ enum ExprKind
     Break,
     Continue,
+    /// `?` or `?name`: a typed hole.
+    Hole(Option<Ident>),
     Name(Ident),
```

`lexer.rs`（`Lexer::symbol`）:

```diff
             '#' => Hash,
+            '?' => Question,
             '=' => Equal,
```

`parser.rs`:

```diff
@@ `Parser::space_argument` の「引数になりうる token」の列
-                | TokenKind::LeftList => true,
+                | TokenKind::LeftList
+                | TokenKind::Question => true,
@@ `Parser` の impl（`array_literal` などの隣）
+    fn hole(&mut self) -> Result<Expr, Diagnostic> {
+        let question = self.take().span;
+        let mut end = question;
+        let mut name = None;
+        if matches!(self.current().kind, TokenKind::Ident(_)) && self.current().span.start == question.end {
+            let ident = self.ident()?;
+            end = ident.span;
+            name = Some(ident);
+        }
+        self.make(ExprKind::Hole(name), question.through(end), 1)
+    }
@@ `Parser::primary` の `match`（`TokenKind::True | TokenKind::False` の次）
+            TokenKind::Question => return self.hole(),
```

`space_argument` と `primary` の腕の後は `ExprKind::Hole` を使う全箇所（`syntax.rs` の `visit`・`computation.rs`・`warnings.rs`・`formatter.rs`・`syntax_codec.rs`）が網羅の誤りで見つかる（`cargo build` が教える）。

`formatter.rs`:

```diff
@@ 記号の間隔の規則
-    if matches!(previous.kind, At | Hash) {
+    if matches!(previous.kind, At | Hash | Question) {
         return separated;
     }
@@ `Canonical` の式の走査の `match`（`DynDispatch { .. } => {}` の次）
+            Hole(name) => {
+                if let Some(name) = name {
+                    self.ident(name);
+                }
+            }
```

`syntax_codec.rs`（tag は `ExprKind` の次の空き番号。HEAD は 46）:

```diff
@@ `ExprKind` の `put`（末尾の腕）
+            Hole(name) => {
+                out.tag("ExprKind", 46);
+                name.put(out)
+            }
@@ `ExprKind` の `get`
+            46 => variant(input, |input| Ok(Hole(Option::<Ident>::get(input)?))),
@@ 件数のテスト
-            ("ExprKind", 46),
+            ("ExprKind", 47),
```

`computation.rs`・`warnings.rs`（式の葉の群）: `| ExprKind::Hole(_)`（`computation.rs`）・`| Hole(_)`（`warnings.rs`）を `Continue` の次に足す。`frontend_cache.rs` の `FRONTEND_FORMAT` を 1 上げる（試作では未変更。他のチケットと「先に入った方が上げる」）。

`parse_type`・`parse_type_query`（Phase 3。`parser.rs` の `parse_with_source_all` の次に置く。試作で動作確認済みの `parse_type_query` に `parse_type` を足す）:

```rust
/// Reads `text` as one type. Nothing may follow it.
pub fn parse_type(text: &str) -> Result<TypeExpr, Diagnostic> {
    let tokens = lex(text, Edition::LATEST)?;
    let mut parser = Parser::new(text, tokens);
    let ty = parser.type_expr()?;
    if !parser.at(&TokenKind::End) {
        return Err(parser.error("unexpected input after the type"));
    }
    Ok(ty)
}

/// Reads `text` as a type, the way `signature` reads a `def` signature: the parameters of a
/// function type and its result.
pub fn parse_type_query(text: &str) -> Result<(Vec<TypeExpr>, TypeExpr), Diagnostic> {
    let ty = parse_type(text)?;
    let (mut parameters, mut result) = match ty.kind {
        TypeExprKind::Function(parameters, result) => (parameters, *result),
        _ => (Vec::new(), ty),
    };
    while !parameters.is_empty()
        && matches!(&result.kind, TypeExprKind::Function(parameters, _) if !parameters.is_empty())
    {
        let TypeExprKind::Function(more, tail) = result.kind else {
            unreachable!()
        };
        parameters.extend(more);
        result = *tail;
    }
    Ok((parameters, result))
}
```

`polymorph.rs`:

```diff
-fn map_type(ty: &Type, f: &mut impl FnMut(&Type) -> Type) -> Type {
+pub(super) fn map_type(ty: &Type, f: &mut impl FnMut(&Type) -> Type) -> Type {
@@ `Classes`
     pub(super) fn holds(&self, name: &str, ty: &Type, types: &TypeContext<'_>) -> bool {
+        self.holds_class(self.names[name], ty, types)
+    }
+
+    pub(super) fn holds_class(&self, class: usize, ty: &Type, types: &TypeContext<'_>) -> bool {
         let constraint = Constraint {
-            class: self.names[name],
+            class,
             ty: ty.clone(),
             span: Span::default(),
         };
         self.normalize(&constraint, types)
             .is_ok_and(|residual| residual.is_empty())
     }
+
+    pub(super) fn class_name(&self, class: usize) -> &str {
+        &self.declarations[class].name
+    }
```

### A.3 穴の検査の補助（試作）

```rust
fn hole_type(&self, ty: &Type) -> String {              // `hole_type`: Infer と Error を `_` にして表示する
    let ty = self.inference.resolve(ty);
    polymorph::map_type(&ty, &mut |t| match t {
        Type::Infer(_) | Type::Error => Type::Handle("_".into()),
        _ => t.clone(),
    })
    .display(&self.types)
}

/// 探索用の `Inference` に型を写す。`Infer(k)` は `map` を通して新しい変数に、`Error` は新しい変数にする。
fn import_type(ty: &Type, fresh: &mut Inference, map: &mut BTreeMap<usize, Type>) -> Type {
    polymorph::map_type(ty, &mut |t| match t {
        Type::Infer(k) => map.entry(*k).or_insert_with(|| fresh.fresh()).clone(),
        Type::Error => fresh.fresh(),
        _ => t.clone(),
    })
}
```

「分かる」の判定（試作）: `expected_type = expected.map(|ty| self.inference.resolve(ty))` が `Some(ty)` で、`!matches!(ty, Type::Infer(_)) && !ty.contains_error()`。

### A.4 `typed_hole` の骨格（Phase 1・2。コンパイルは未検証。部品は試作で動いた A.3 と B.3）

```rust
fn typed_hole(&mut self, name: Option<&Ident>, expected: Option<&Type>, span: Span) -> Diagnostic {
    let wanted = expected
        .map(|ty| self.inference.resolve(ty))
        .filter(|ty| !matches!(ty, Type::Infer(_)) && !ty.contains_error());
    let label = name.map_or_else(|| "'?'".to_owned(), |name| format!("'?{}'", name.text));
    let (message, help, shown) = match &wanted {
        Some(ty) => {
            let shown = self.hole_type(ty);
            (
                format!("typed hole {label} of type '{shown}'; replace it with an expression of that type"),
                format!("fill the hole with a value of type '{shown}'"),
                shown,
            )
        }
        None => (
            format!("typed hole {label} of unknown type; replace it with an expression"),
            "fill the hole with an expression; its type is not known here".to_owned(),
            "_".to_owned(),
        ),
    };
    // scope: 内側のフレームから集め、隠された名前と生成された局所を除き、宣言の順にして、
    // 期待型と単一化できるものを先にする（`import_type` で写した型の `unify` の成否だけを見る）。
    let mut seen = BTreeSet::new();
    let mut locals: Vec<&Local> = Vec::new();
    for frame in self.scopes.iter().rev() {
        for (local_name, local) in frame {
            if local.provenance == Provenance::User
                && !local_name.starts_with('$')
                && seen.insert(local_name.clone())
            {
                locals.push(local);
            }
        }
    }
    locals.sort_by_key(|local| local.id);
    let fits = |local: &Local| {
        wanted.as_ref().is_some_and(|wanted| {
            let mut search = Inference::default();
            let mut map = BTreeMap::new();
            let goal = Self::import_type(wanted, &mut search, &mut map);
            let ty = Self::import_type(&self.inference.resolve(&local.ty), &mut search, &mut map);
            search.unify(&ty, &goal, &self.types, Span::default()).is_ok()
        })
    };
    let (first, rest): (Vec<&Local>, Vec<&Local>) = locals.into_iter().partition(|local| fits(local));
    let scope: Vec<(String, Type)> = first
        .into_iter()
        .chain(rest)
        .take(32)
        .map(|local| (local.name.clone(), self.inference.resolve(&local.ty)))
        .collect();
    let mut diagnostic = Diagnostic::new("E1030", message, span).with_kind("typed_hole").with_help(help);
    diagnostic.detail.expected = Some(shown);                 // `expected_found` は使わない（`found` がない）
    let mut diagnostic = diagnostic.with_scope(
        scope.iter().map(|(name, ty)| ScopeEntry { name: name.clone(), ty: self.hole_type(ty) }).collect(),
    );
    if let Some(wanted) = &wanted
        && self.holes_searched < find::MAX_HOLE_SEARCHES
    {
        self.holes_searched += 1;
        let (candidates, fixes) = self.hole_candidates(wanted, &scope, span);   // find.rs。B.3 の本体
        diagnostic = diagnostic.with_candidates(candidates);
        for fix in fixes {
            diagnostic = diagnostic.fix(fix.title, fix.applicability, fix.edits);
        }
    }
    diagnostic
}
```

## B. 検証済みのコード

### B.1 `suggest`（Phase 4。`rustc --edition 2024 --test` で 3 件が通った）

`src/find.rs` にそのまま入れる（`tests` は単体テストの本体）。`Pooled.group`: 0 局所、1 関数・定数・組み込み、2 case、3 module、4 型・field・active pattern。

```rust
use std::cmp::Ordering;

/// Damerau-Levenshtein distance (optimal string alignment): insert, delete, replace, and swap of
/// two neighbors each cost 1.
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut rows = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in rows.iter_mut().enumerate() {
        row[0] = i;
    }
    for j in 0..=b.len() {
        rows[0][j] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (rows[i - 1][j] + 1)
                .min(rows[i][j - 1] + 1)
                .min(rows[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(rows[i - 2][j - 2] + 1);
            }
            rows[i][j] = best;
        }
    }
    rows[a.len()][b.len()]
}

/// The most edits that still count as similar: 1 for a name of up to 4 characters, else 2.
fn limit(target: &str) -> usize {
    if target.chars().count() <= 4 { 1 } else { 2 }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Pooled {
    /// The text to write in place of the target.
    pub text: String,
    /// The name that is compared with the target (the last segment of a qualified `text`).
    pub name: String,
    pub group: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Suggestion {
    pub text: String,
    /// 0 when `name` equals the target and `text` only qualifies it.
    pub distance: usize,
    pub group: u8,
}

pub(super) fn suggest(target: &str, pool: &[Pooled]) -> Vec<Suggestion> {
    let mut found: Vec<Suggestion> = Vec::new();
    for item in pool {
        if item.text == target {
            continue;
        }
        let distance = if item.name == target {
            0
        } else if target.chars().count() < 2 {
            continue;
        } else {
            let distance = distance(target, &item.name);
            if distance > limit(target) {
                continue;
            }
            distance
        };
        found.push(Suggestion {
            text: item.text.clone(),
            distance,
            group: item.group,
        });
    }
    found.sort_by(|a, b| {
        (a.distance, a.group, &a.text)
            .cmp(&(b.distance, b.group, &b.text))
            .then(Ordering::Equal)
    });
    found.dedup_by(|a, b| a.text == b.text);
    found.truncate(10);
    found
}

/// The one text a `maybe` fix may write: the only suggestion, or one that is strictly closer
/// than the next.
pub(super) fn best(found: &[Suggestion]) -> Option<&Suggestion> {
    match found {
        [only] => Some(only),
        [first, second, ..] if (first.distance, first.group) < (second.distance, second.group) => {
            Some(first)
        }
        _ => None,
    }
}
```

単体テスト 3 件（`distances_are_damerau_levenshtein`・`qualified_names_come_first_and_win_when_alone`・`near_names_are_limited_by_length_and_ranked`）:

```rust
fn pooled(text: &str, group: u8) -> Pooled {
    let name = text.rsplit('.').next().unwrap().to_owned();
    Pooled { text: text.to_owned(), name, group }
}

#[test]
fn distances_are_damerau_levenshtein() {
    assert_eq!(distance("cnt", "count"), 2);
    assert_eq!(distance("lenght", "length"), 1);
    assert_eq!(distance("Strng", "string"), 2);
    assert_eq!(distance("Circl", "Circle"), 1);
    assert_eq!(distance("abc", "abc"), 0);
    assert_eq!(distance("", "ab"), 2);
    assert_eq!(limit("cnt"), 1);
    assert_eq!(limit("count"), 2);
}

#[test]
fn qualified_names_come_first_and_win_when_alone() {
    let pool = vec![pooled("Array.length", 1), pooled("String.length", 1), pooled("count", 0)];
    let found = suggest("length", &pool);
    assert_eq!(found.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(), ["Array.length", "String.length"]);
    assert!(best(&found).is_none());
    let found = suggest("length", &pool[..1]);
    assert_eq!(best(&found).unwrap().text, "Array.length");
}

#[test]
fn near_names_are_limited_by_length_and_ranked() {
    let pool = vec![pooled("count", 0), pooled("cnt2", 1), pooled("counts", 1), pooled("cut", 1)];
    let found = suggest("cnt", &pool);
    assert_eq!(found.iter().map(|s| (s.text.as_str(), s.distance)).collect::<Vec<_>>(), [("cnt2", 1), ("cut", 1)]);
    assert!(suggest("cnt", &pool[..1]).is_empty());
    let found = suggest("countt", &pool);
    assert_eq!(found[0].text, "count");
    assert_eq!(best(&found).unwrap().text, "count");
    let found = suggest("x", &[pooled("y", 0), pooled("M.x", 1)]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].text, "M.x");
}
```

### B.2 索引と照合（Phase 2・3。複製でビルドして動作確認済み）

`src/find.rs`（`check` の子モジュール）の本体。試作と違うのは、`E2000` の `find_unknown_module` に `.with_kind("find_unknown_module")` を付けること（G22 Phase 1 が main にあるとき）だけ。`debug_run` は試作の確認用で、実装では `run`（設計）に置き換える。

```rust
use super::polymorph::{self, Classes, Inference, Scheme};
use super::*;

/// The module whose point of view resolves the names of a query and decides what is visible.
pub const REQUESTER: &str = "Main";
pub const MAX_ENTRIES_EXAMINED: usize = 5_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Def,
    Const,
    Builtin,
    Field,
    Case,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Def => "def",
            Self::Const => "const",
            Self::Builtin => "builtin",
            Self::Field => "field",
            Self::Case => "case",
        }
    }
}

/// One declaration with its type, in the form that holes and `tsuzuri find` search.
#[derive(Clone, Debug)]
pub struct Entry {
    pub kind: Kind,
    /// The name as source code writes it from the root namespace.
    pub name: String,
    /// The key of the owning module (the part before the first `.` for a builtin).
    pub module: String,
    pub public: bool,
    pub origin: ModuleOrigin,
    /// The function id of a `def` or `const`.
    pub id: Option<usize>,
    pub variables: Vec<String>,
    pub parameters: Vec<Type>,
    pub result: Type,
    /// `(class, type)` pairs over `variables`.
    pub constraints: Vec<(usize, Type)>,
    /// `Numeric<'a> => ref ['a] -> 'a`
    pub text: String,
}

impl Entry {
    pub fn visible_from(&self, requester: &str) -> bool {
        self.public || self.module == requester
    }
}

fn render(
    types: &TypeContext<'_>,
    classes: &Classes,
    parameters: &[Type],
    result: &Type,
    constraints: &[(usize, Type)],
) -> String {
    let function = if parameters.is_empty() {
        result.display(types)
    } else {
        Type::function(parameters.to_vec(), result.clone()).display(types)
    };
    if constraints.is_empty() {
        return function;
    }
    let prefix: Vec<String> = constraints
        .iter()
        .map(|(class, ty)| {
            format!("{}<{}>", type_display(classes.class_name(*class)), ty.display(types))
        })
        .collect();
    format!("{} => {function}", prefix.join(", "))
}

pub(super) fn build(
    names: &Names,
    types: TypeContext<'_>,
    signatures: &[Scheme],
    classes: &Classes,
) -> Vec<Entry> {
    let mut entries = Vec::new();
    for (key, info) in &names.functions {
        let scheme = &signatures[info.id];
        if key.contains('$') || key.contains("__") || scheme.is_poisoned() {
            continue;
        }
        let constraints: Vec<(usize, Type)> = scheme
            .constraints
            .iter()
            .map(|constraint| (constraint.class, constraint.ty.clone()))
            .collect();
        let kind = if names.constants.contains(&info.id) { Kind::Const } else { Kind::Def };
        entries.push(Entry {
            kind,
            name: declaration_display(key),
            module: info.module.clone(),
            public: info.visibility == Visibility::Public,
            origin: names.origin(&info.module),
            id: Some(info.id),
            variables: scheme.variables.clone(),
            parameters: scheme.signature.parameters.clone(),
            result: scheme.signature.result.clone(),
            text: render(&types, classes, &scheme.signature.parameters, &scheme.signature.result, &constraints),
            constraints,
        });
    }
    for builtin in Builtin::ALL {
        let name = builtin.name();
        if name.contains('$') || name.contains("__") {
            continue;
        }
        // A throwaway checker: `names`, `types` and `signatures` are shared references, so nothing
        // of the checker that is running is touched.
        let mut checker = Checker::new(REQUESTER, names, types, signatures, classes);
        let Ok((_, ty)) = checker.builtin(*builtin, Span::default()) else {
            continue;
        };
        let mut letters: BTreeMap<usize, String> = BTreeMap::new();
        let mut letter = |k: usize| -> String {
            let next = letters.len();
            letters
                .entry(k)
                .or_insert_with(|| ((b'a' + (next % 26) as u8) as char).to_string())
                .clone()
        };
        let named = polymorph::map_type(&ty, &mut |t| match t {
            Type::Infer(k) => Type::Variable(letter(*k)),
            _ => t.clone(),
        });
        let constraints: Vec<(usize, Type)> = checker
            .constraints
            .iter()
            .map(|constraint| {
                let ty = polymorph::map_type(&constraint.ty, &mut |t| match t {
                    Type::Infer(k) => Type::Variable(letter(*k)),
                    _ => t.clone(),
                });
                (constraint.class, ty)
            })
            .collect();
        let (parameters, result) = match named {
            Type::Function(parameters, result) => (parameters, *result),
            other => (Vec::new(), other),
        };
        entries.push(Entry {
            kind: Kind::Builtin,
            name: name.to_owned(),
            module: name.split_once('.').map_or_else(String::new, |(prefix, _)| prefix.to_owned()),
            public: true,
            origin: ModuleOrigin::Std,
            id: None,
            variables: polymorph::variables(&Type::function(parameters.clone(), result.clone())),
            text: render(&types, classes, &parameters, &result, &constraints),
            parameters,
            result,
            constraints,
        });
    }
    for (key, info) in &names.records {
        if key.contains('$') {
            continue;
        }
        let record = &types.records[info.id];
        let arguments: Box<[Type]> =
            record.parameters.iter().map(|parameter| Type::Variable(parameter.clone())).collect();
        let whole = Type::Record(info.id, arguments);
        for (field, ty) in &record.fields {
            entries.push(Entry {
                kind: Kind::Field,
                name: format!("{}.{field}", type_display(key)),
                module: info.module.clone(),
                public: info.visibility == Visibility::Public,
                origin: names.origin(&info.module),
                id: None,
                variables: record.parameters.clone(),
                parameters: vec![whole.clone()],
                result: ty.clone(),
                constraints: Vec::new(),
                text: render(&types, classes, &[whole.clone()], ty, &[]),
            });
        }
    }
    for (key, case) in &names.cases {
        let union = &types.unions[case.info.id];
        let arguments: Box<[Type]> =
            union.parameters.iter().map(|parameter| Type::Variable(parameter.clone())).collect();
        let whole = Type::Union(case.info.id, arguments);
        let parameters: Vec<Type> = union.cases[case.case].1.clone().into_iter().collect();
        entries.push(Entry {
            kind: Kind::Case,
            name: declaration_display(key),
            module: case.info.module.clone(),
            public: case.info.visibility == Visibility::Public,
            origin: names.origin(&case.info.module),
            id: None,
            variables: union.parameters.clone(),
            text: render(&types, classes, &parameters, &whole, &[]),
            parameters,
            result: whole,
            constraints: Vec::new(),
        });
    }
    entries
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Exact,
    Borrow,
    Partial,
}

impl Tier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Borrow => "borrow",
            Self::Partial => "partial",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Request {
    pub parameters: Vec<TypeExpr>,
    pub result: Option<TypeExpr>,
    /// `--result` and `--takes`: the parameters are matched in any order and the missing side is free.
    pub loose: bool,
    pub module: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Hit {
    pub tier: Tier,
    pub extra: usize,
    pub kind: Kind,
    pub name: String,
    pub module: String,
    pub origin: ModuleOrigin,
    pub text: String,
}

#[derive(Clone, Debug, Default)]
pub struct Outcome {
    pub hits: Vec<Hit>,
    pub searched: usize,
}

fn strip(ty: &Type) -> Type {
    match ty {
        Type::Reference(inner, _) => (**inner).clone(),
        other => other.clone(),
    }
}

/// Whether `ty` has no unresolved variable, so that a class constraint over it can be decided.
pub(super) fn ground(ty: &Type) -> bool {
    let mut ground = true;
    polymorph::map_type(ty, &mut |t| {
        if matches!(t, Type::Infer(_) | Type::Variable(_) | Type::Error) {
            ground = false;
        }
        t.clone()
    });
    ground
}

pub(super) fn instantiate(
    entry: &Entry,
    search: &mut Inference,
) -> (Vec<Type>, Type, Vec<(usize, Type)>) {
    let substitutions: BTreeMap<String, Type> =
        entry.variables.iter().map(|variable| (variable.clone(), search.fresh())).collect();
    (
        entry.parameters.iter().map(|ty| polymorph::substitute(ty, &substitutions)).collect(),
        polymorph::substitute(&entry.result, &substitutions),
        entry
            .constraints
            .iter()
            .map(|(class, ty)| (*class, polymorph::substitute(ty, &substitutions)))
            .collect(),
    )
}

pub(super) fn constraints_hold(
    constraints: &[(usize, Type)],
    search: &Inference,
    types: &TypeContext<'_>,
    classes: &Classes,
) -> bool {
    constraints.iter().all(|(class, ty)| {
        let resolved = search.resolve(ty);
        !ground(&resolved) || classes.holds_class(*class, &resolved, types)
    })
}

/// A query with its variables and wildcards replaced by fresh inference variables.
struct Wanted {
    parameters: Vec<Type>,
    result: Option<Type>,
}

pub(super) struct Query {
    pub parameters: Vec<Type>,
    pub result: Option<Type>,
    pub loose: bool,
}

fn instantiate_query(query: &Query, search: &mut Inference) -> Wanted {
    let mut all = BTreeSet::new();
    for ty in query.parameters.iter().chain(query.result.iter()) {
        all.extend(polymorph::variables(ty));
    }
    let substitutions: BTreeMap<String, Type> =
        all.into_iter().map(|variable| (variable, search.fresh())).collect();
    Wanted {
        parameters: query
            .parameters
            .iter()
            .map(|ty| polymorph::substitute(ty, &substitutions))
            .collect(),
        result: query.result.as_ref().map(|ty| polymorph::substitute(ty, &substitutions)),
    }
}

/// `(tier, extra parameters)` of the entry for the query, if it matches.
pub(super) fn tier(
    entry: &Entry,
    query: &Query,
    types: &TypeContext<'_>,
    classes: &Classes,
) -> Option<(Tier, usize)> {
    let span = Span::default();
    let count = entry.parameters.len();
    let wanted_count = query.parameters.len();
    if query.loose {
        for (tier, borrow) in [(Tier::Exact, false), (Tier::Borrow, true)] {
            let pick = |ty: &Type| if borrow { strip(ty) } else { ty.clone() };
            let mut search = Inference::default();
            let (parameters, result, constraints) = instantiate(entry, &mut search);
            let wanted = instantiate_query(query, &mut search);
            if let Some(expected) = &wanted.result
                && search.unify(&pick(&result), &pick(expected), types, span).is_err()
            {
                continue;
            }
            let mut used = vec![false; parameters.len()];
            let mut all = true;
            for want in &wanted.parameters {
                let mut matched = false;
                for (index, parameter) in parameters.iter().enumerate() {
                    if used[index] {
                        continue;
                    }
                    let mut trial = search.clone();
                    if trial.unify(&pick(parameter), &pick(want), types, span).is_ok() {
                        search = trial;
                        used[index] = true;
                        matched = true;
                        break;
                    }
                }
                if !matched {
                    all = false;
                    break;
                }
            }
            if all && constraints_hold(&constraints, &search, types, classes) {
                return Some((tier, count.saturating_sub(wanted_count)));
            }
        }
        return None;
    }
    if count == wanted_count {
        for (tier, borrow) in [(Tier::Exact, false), (Tier::Borrow, true)] {
            let pick = |ty: &Type| if borrow { strip(ty) } else { ty.clone() };
            let mut search = Inference::default();
            let (parameters, result, constraints) = instantiate(entry, &mut search);
            let wanted = instantiate_query(query, &mut search);
            let same = parameters
                .iter()
                .zip(&wanted.parameters)
                .all(|(a, b)| search.unify(&pick(a), &pick(b), types, span).is_ok());
            let expected = wanted.result.as_ref().expect("a signature query has a result");
            if same
                && search.unify(&pick(&result), &pick(expected), types, span).is_ok()
                && constraints_hold(&constraints, &search, types, classes)
            {
                return Some((tier, 0));
            }
        }
    }
    if count >= wanted_count {
        let mut search = Inference::default();
        let (parameters, result, constraints) = instantiate(entry, &mut search);
        let wanted = instantiate_query(query, &mut search);
        let expected = wanted.result.as_ref().expect("a signature query has a result");
        if search.unify(&strip(&result), &strip(expected), types, span).is_err() {
            return None;
        }
        let mut used = vec![false; parameters.len()];
        for want in &wanted.parameters {
            let mut matched = false;
            for (index, parameter) in parameters.iter().enumerate() {
                if used[index] {
                    continue;
                }
                let mut trial = search.clone();
                if trial.unify(&strip(parameter), &strip(want), types, span).is_ok() {
                    search = trial;
                    used[index] = true;
                    matched = true;
                    break;
                }
            }
            if !matched {
                return None;
            }
        }
        if constraints_hold(&constraints, &search, types, classes) {
            return Some((Tier::Partial, count - wanted_count));
        }
    }
    None
}

/// Renames each `_` of a query to its own type variable, which `resolve_type_with_kinds` accepts.
fn name_wildcards(ty: &mut TypeExpr, next: &mut usize) {
    match &mut ty.kind {
        TypeExprKind::Named(name) if name == "_" => {
            ty.kind = TypeExprKind::Variable(format!("_{next}"));
            *next += 1;
        }
        TypeExprKind::Apply(_, arguments) => {
            for argument in arguments.iter_mut() {
                name_wildcards(argument, next);
            }
        }
        TypeExprKind::Regions(inner, _)
        | TypeExprKind::Quantified(_, inner)
        | TypeExprKind::Array(inner)
        | TypeExprKind::ArrayView(inner)
        | TypeExprKind::List(inner)
        | TypeExprKind::Task(inner)
        | TypeExprKind::Reference(inner, _) => name_wildcards(inner, next),
        TypeExprKind::FixedArray(element, length) => {
            name_wildcards(element, next);
            name_wildcards(length, next);
        }
        TypeExprKind::Tuple(elements) => {
            for element in elements {
                name_wildcards(element, next);
            }
        }
        TypeExprKind::Function(parameters, result) => {
            for parameter in parameters {
                name_wildcards(parameter, next);
            }
            name_wildcards(result, next);
        }
        _ => {}
    }
}

/// `--module` accepts the module's full name and its last part.
fn module_matches(entry: &Entry, wanted: &str) -> bool {
    namespace_display(&entry.module) == wanted || module_stem(&entry.module) == wanted
}

pub(super) fn search(
    request: &Request,
    names: &Names,
    types: TypeContext<'_>,
    signatures: &[Scheme],
    classes: &Classes,
    entries: &[Entry],
) -> Result<Outcome, Diagnostic> {
    if let Some(wanted) = &request.module
        && !names.modules.keys().any(|key| namespace_display(key) == *wanted || module_stem(key) == wanted)
        && !entries.iter().any(|entry| entry.kind == Kind::Builtin && entry.module == *wanted)
    {
        return Err(Diagnostic::new(
            "E2000",
            format!("no loaded module is named '{wanted}'"),
            Span::default(),
        ));
        // G22 Phase 1 があれば `.with_kind("find_unknown_module")` を付ける。
    }
    let _ = signatures;
    let kinds = BTreeMap::new();
    let mut next = 0;
    let mut resolve = |expression: &TypeExpr| -> Result<Type, Diagnostic> {
        let mut expression = expression.clone();
        name_wildcards(&mut expression, &mut next);
        resolve_type_with_kinds(&expression, REQUESTER, names, &kinds)
    };
    let mut parameters = Vec::new();
    for parameter in &request.parameters {
        parameters.push(resolve(parameter)?);
    }
    let result = request.result.as_ref().map(&mut resolve).transpose()?;
    let query = Query { parameters, result, loose: request.loose };
    let mut hits = Vec::new();
    let mut searched = 0;
    for entry in entries {
        if !entry.visible_from(REQUESTER)
            || request.module.as_ref().is_some_and(|wanted| !module_matches(entry, wanted))
        {
            continue;
        }
        searched += 1;
        if searched > MAX_ENTRIES_EXAMINED {
            break;
        }
        if let Some((tier, extra)) = tier(entry, &query, &types, classes) {
            hits.push(Hit {
                tier,
                extra,
                kind: entry.kind,
                name: entry.name.clone(),
                module: namespace_display(&entry.module),
                origin: entry.origin,
                text: entry.text.clone(),
            });
        }
    }
    hits.sort_by(|a, b| (a.tier, a.extra, &a.name, a.kind).cmp(&(b.tier, b.extra, &b.name, b.kind)));
    hits.dedup_by(|a, b| a.name == b.name && a.kind == b.kind);
    Ok(Outcome { hits, searched })
}
```

`name_wildcards` と `module_matches` の単体テストは、`TypeExpr` を `parse_type` で作って `Variable("_0")`・`Variable("_1")` になること、`Entry`（`module: "Geometry.Point"`）が `Geometry::Point`・`Point` に合い `Geometry` に合わないことを確かめる。

### B.3 穴の探索（Phase 2。試作で動作確認済み。付録 E の出力）

試作の `Checker::typed_hole` の中の、`Entry` から候補を作る部分。実装では (1) 結果を `Candidate`（`name`・`signature`・`how`）にして整列キー `(順位, 結果が型変数 1 つか, ? の数, name, text)` で並べる、(2) 順位 1（`local.field`）と、局所の値（順位 0。下の最後のループ）を足す、(3) `OnceLock` の索引を使う、(4) 上限（16 個の穴・5,000 個の `Entry`・200 文字）を足す、(5) `text` が同じものを 1 つにする、(6) 順位を下の表にする。試作の `rank` は引数なし 4・全部埋まる 2・一部 3・どれも埋まらない 5（局所は 0）で、本文の順位表と同じ。

```rust
let entries = find::build(self.names, self.types, self.signatures, self.classes);   // 実装は names.search_index.get_or_init(..)
for entry in &entries {
    if !entry.visible_from(self.module) || (entry.id.is_some() && entry.id == self.function) {
        continue;
    }
    let mut search = Inference::default();
    let (parameters, result, constraints) = find::instantiate(entry, &mut search);
    let mut map = BTreeMap::new();
    let goal = Self::import_type(wanted, &mut search, &mut map);
    if search.unify(&result, &goal, &self.types, Span::default()).is_err() {
        continue;
    }
    if !find::constraints_hold(&constraints, &search, &self.types, self.classes) {
        continue;
    }
    let mut filled: Vec<Option<String>> = Vec::new();
    for parameter in &parameters {
        let parameter = search.resolve(parameter);
        let mut hit = None;
        for (local_name, local_type) in &scope {
            if matches!(local_type, Type::Infer(_)) {
                continue;       // 型が未確定の局所は引数を埋めない
            }
            let mut trial = search.clone();
            let mut local_map = map.clone();
            let local_type = Self::import_type(local_type, &mut trial, &mut local_map);
            let mutable = self.local(local_name).is_some_and(|l| l.mutable);
            let text = match (&parameter, &local_type) {
                (Type::Reference(inner, wants_mut), Type::Reference(local_inner, local_mut)) => {
                    (wants_mut == local_mut
                        && trial.unify(local_inner, inner, &self.types, Span::default()).is_ok())
                    .then(|| local_name.clone())
                }
                (Type::Reference(inner, wants_mut), _) => {
                    ((!*wants_mut || mutable)
                        && trial.unify(&local_type, inner, &self.types, Span::default()).is_ok())
                    .then(|| format!("({} {local_name})", if *wants_mut { "ref mut" } else { "ref" }))
                }
                _ => trial
                    .unify(&local_type, &parameter, &self.types, Span::default())
                    .is_ok()
                    .then(|| local_name.clone()),
            };
            // 取り込んだ後に制約が壊れる局所は採らない（`Int.count_ones xs` を防ぐ）
            if text.is_some() && find::constraints_hold(&constraints, &trial, &self.types, self.classes) {
                search = trial;
                map = local_map;
                hit = text;
                break;
            }
        }
        filled.push(hit);
    }
    let all = filled.iter().all(Option::is_some);
    let rank: u32 = if parameters.is_empty() { 4 }
        else if all { 2 }
        else if filled.iter().any(Option::is_some) { 3 }
        else { 5 };
    let unfilled = filled.iter().filter(|f| f.is_none()).count() as u32;
    let generic = u32::from(matches!(entry.result, Type::Variable(_)));
    let key = rank * 10_000 + generic * 1_000 + unfilled;       // 実装は (rank, generic, unfilled, name, text) のタプル
    let args: Vec<String> = filled.iter().map(|f| f.clone().unwrap_or_else(|| "?".into())).collect();
    // text = entry.name、引数があれば " " と args を連ねる
}
// 局所の値（順位 0）:
for (local_name, local_type) in &scope {
    let mut search = Inference::default();
    let mut map = BTreeMap::new();
    let goal = Self::import_type(wanted, &mut search, &mut map);
    let local_type = Self::import_type(local_type, &mut search, &mut map);
    if search.unify(&local_type, &goal, &self.types, Span::default()).is_ok() { /* 順位 0 の候補 local_name */ }
}
```

## C. 未解決名の発行箇所と `pool`（Phase 4）

各行の「目印」で `grep -n` して箇所を見つける（行番号は変わる）。`target` は比べる名前、`置換先` は `maybe` の fix が書き換える span。`pool` の `group` と `signature` は本文の表（局所 0・関数と定数と組み込み 1・case 2・module 3・型と field と active pattern 4）。

| # | 目印（`grep -n`） | 関数 | `target` | `pool`（`Pooled { text, name, group }`） | 置換先 |
| --- | --- | --- | --- | --- | --- |
| 1 | `unknown value '{}'` | `Checker::name` | `name.text` | (a) `self.scopes` の全フレームの局所（`provenance == User`、`$` で始まらない）: `text = name = 局所名`、group 0、`signature = "{名前}: {hole_type(型)}"`。(b) `names.functions` の `info.module == self.module` の関数（`$`・`__` を含まない）: `text = name = key の最後の部分`、group 1、`function`（定数は `constant`）。(c) `Builtin::ALL` の `.` を含まない名前: group 1、`builtin`。(d) `names.case_aliases` のキー: group 2、`union case`。(e) `target` が大文字で始まるとき `names.modules` のキーの最後の部分: group 3、`module`。(f) **修飾名**: `names.functions` で `visible_from(self.module)` かつ最後の部分が `target` に等しいもの（(b) 以外）は `text = declaration_display(key)`、`name = target`、group 1。同じく `names.cases` のキー（`case.info.visible_from(self.module)`）は group 2。`Builtin::ALL` の `X.target` も group 1 | `name.span` |
| 2 | `has no function or union case` | `Checker::value_expression` の `ExprKind::Field(value, field)` の腕の最後の `else`（`Task` と `::` の枝は対象外） | `field.text` | `key = names.module_path(self.module, &spelled)`。`key` があれば `names.functions` の `info.module == key` と `names.cases` の `case.info.module == key` の最後の部分（group 1・2）。`key` がなければ `Builtin::ALL` の `"{spelled}."` で始まる名前の後ろ（group 1）。`text = name = メンバー名` | `field.span` |
| 3 | `unknown function '{}'` | `Checker::qualified_function` | `name.text` 全体 | 見える関数（`names.functions`、`visible_from(self.module)`）の `declaration_display(key)` を group 1、見える case を group 2、`.` を含む組み込み名を group 1。`text = name = 表示名`（全体を比べる） | `name.span` |
| 4 | `unknown record, union, or type alias '{name}'` | `Names::named_type`（`self.type_result(choice, name, span)` の `Err` で、コードが `E1004` かつ message が `unknown record, union, or type alias` で始まるときだけ。`namespace_spelling` の枝は対象外） | `name` | `record_aliases`・`union_aliases`・`type_alias_names`・`handle_aliases` のキー（group 4、`type`）と、`numeric::NUMERIC_NAMES` に `bool`・`unit`・`string`・`utf8string`・`char`・`utf8char`・`bigint` を足したもの（group 4、`type`）。`name` が `.`・`:` を含むなら fix なし | `span` |
| 5 | `unknown field '{}'` と `has no field '{}'`（`record '{}' has no field`） | `Checker::field_access`・`record_literal`・`record_update` | `field.text` | その record（`self.types.records[id]`）の `fields` の名前（group 4、`field`） | `field.span` |
| 6 | `unknown union case or active pattern '{}'` | `src/control.rs` の `Checker::active_pattern` の `ok_or_else` | `name.text` | `names.case_aliases` のキー（group 2、`union case`）と `names.active_aliases` のキー（group 2、`active pattern`） | `name.span` |

呼び出しの形（6 か所で共通。`Checker` のメソッドに 1 つ書く）:

```rust
fn with_suggestions(&self, diagnostic: Diagnostic, target: &str, replace: Span, pool: &[(find::Pooled, String)]) -> Diagnostic
// 1. find::suggest(target, プールの Pooled) → 候補がなければ diagnostic をそのまま返す
// 2. Candidate { name: text, signature: プールの signature の文字列, how: local | qualify | similar }
// 3. help: 候補が 1 件、または find::best が Some → "did you mean '{先頭}'?"、そうでなければ "did you mean one of: '{1}', '{2}', '{3}'?"（最大 3 つ）
// 4. find::best(..) が Some かつ target が '.'・':' を含まない（4 だけ）→ fix("Replace '{target}' with '{先頭}'", Applicability::Maybe, vec![Edit { span: replace, text: 先頭 }])
// 呼び出し: diagnostic.with_candidates(..).with_help(..) と .fix(..)（G22 の builder。help は後勝ち）
```

`Names` のメソッド（4）は `Checker` を持たないので、同じ処理を `Names::suggest_type(&self, diagnostic, name, span)`（`pool` が別名表だけ）として書き、`named_type` から呼ぶ。

## D. テストの入力と期待値

テストの共通の書き方。ライブラリ経由のテストは `tsuzuri::analyze_all(source).expect_err("...").diagnostics`（`DiagnosticSet.diagnostics`。各要素は G22 の `Diagnostic`。`code`・`kind`・`message`・`span`・`help`・`fixes`・`detail.expected`・`detail.scope`（`name`・`ty`）・`detail.candidates`（`name`・`signature`・`how`）を読む）。CLI のテストは `std::process::Command::new(env!("CARGO_BIN_EXE_tsuzuri"))` で、一時ディレクトリは `std::env::temp_dir()` の下に一意の名前で作り、終わりに消す（既存の `tests/api_diff.rs` と同じ）。`\n` はソースの改行。

### D.1 `tests/typed_holes.rs`（Phase 1: 8 件、Phase 2: 9 件）

| # | テスト名 | 入力 | 期待 |
| --- | --- | --- | --- |
| 1 | `a_hole_is_one_error_and_does_not_cascade` | `def f :: i64 -> i64 = \n -> n + ?\nf 1\n` | 診断 1 件。`E1030`・kind `typed_hole`・span 32..33・message `typed hole '?' of type 'i64'; replace it with an expression of that type`・help `fill the hole with a value of type 'i64'`・`expected` が `Some("i64")`・`scope` が `[("n","i64")]` |
| 2 | `a_named_hole_keeps_its_name_and_reports_unknown_type` | `let x = ?foo\nx\n` | 1 件。span 8..12・message `typed hole '?foo' of unknown type; replace it with an expression`・help `fill the hole with an expression; its type is not known here`・`expected` が `Some("_")`・`scope` と `candidates` が空 |
| 3 | `holes_and_independent_errors_are_all_reported` | `def f :: i64 -> i64 = \n ->\n    let a: i64 = ?\n    let b: bool = 1\n    let c = a + ?\n    n\nf 1\n` | 3 件。コードが `E1030`・`E1003`・`E1030` の順。行・列が 2:18・3:19・4:17。2 件目の message が `expected bool, found i32`。3 件目の `expected` が `Some("_")`（`a` は誤りの回復で型が未確定） |
| 4 | `expected_type_follows_the_position` | A.1 のソース | 診断が 15 件（穴の数）。行 → `expected` が A.1 の表（3: `i64`、4: `i64`、5: `bool`、6: `i64`、7: `i64`、8: `i64`、9: `_`、11: `i64`、14: `_`、17: `i64`、20: `_`、23: `_`、25: `ref [_]`、26: `i64`、27: `_`） |
| 5 | `scope_lists_user_locals_in_a_fixed_order` | (a) `def f :: i64 -> bool -> i64 = \n flag ->\n    let s = "a"\n    let n = 1.5\n    match flag with\n    \| true -> ?\n    \| false -> 0\nf 1 true\n`、(b) `def g :: string -> i64 -> i64 = \s k ->\n    let t = "b"\n    let m = 2\n    ?\ng "a" 1\n`、(c) `def h :: i64 -> i64 = \n ->` に `let v0 = 0` … `let v39 = 39` を並べて最後に `?` | (a) `expected` が `Some("i64")`、`scope` の名前が `["flag","s","n"]`・型が `["bool","string","f64"]`（`$match` の局所と、隠された `n: i64` は出ない）。(b) 名前が `["k","m","s","t"]`（期待型 `i64` と単一化できる `k`・`m` が先で、各群は宣言の順）。(c) `scope` が 32 件で、先頭が `n`、最後が `v30` |
| 6 | `a_program_with_a_hole_is_not_built` | `def f :: i64 -> i64 = \n -> n + ?\nf 1\n` を `Main.tz` に置き、`tsuzuri build DIR -o DIR/out` | 終了コードが 0 でない。標準エラーに `E1030`。`DIR/out` が存在しない |
| 7 | `fmt_keeps_holes` | `let x = f   ?a ?\nx\n` を `Main.tz` に置き `tsuzuri fmt DIR` | 終了コード 0。ファイルが `let x = f ?a ?\nx\n`。続けて `tsuzuri fmt --check DIR` が終了コード 0 |
| 8 | `question_mark_outside_an_expression_is_a_syntax_error` | `match 1 with\n\| ? -> 0\n\| _ -> 1\n` と `let y: ? = 1\ny\n` | どちらも先頭の診断が `E0002`（`expected a pattern`・`expected an identifier`） |
| 9 | `candidates_use_locals_and_are_ranked` | `def f :: [i64] -> i64 = \xs -> ?\nf [1, 2]\n` | `candidates` の先頭 4 件の `name` が `["Array.length (ref xs)","Array.product (ref xs)","Array.sum (ref xs)","Parallel.sum (ref xs)"]`、`how` がすべて `call`、先頭の `signature` が `Array.length :: ref ['a] -> i64`。`candidates` が 10 件 |
| 10 | `the_function_being_checked_is_not_a_candidate` | 9 と同じ | どの `name` も `Main.f ` で始まらない。`Int.count_ones xs` もない（`Integer<[i64]>` を満たさない） |
| 11 | `constraints_remove_candidates` | 9 と同じ | どの `name` も `Array.sum_pairwise` で始まらない（`Float<i64>` を満たさない）。`Array.sum (ref xs)` はある |
| 12 | `one_local_of_the_type_gives_a_maybe_fix` | `def f :: i64 -> i64 = \n -> ?\nf 1\n` | `candidates[0]` が `name` `n`・`how` `value`・`signature` `n: i64`。`fixes` が 1 件: title `Replace '?' with 'n'`・`Maybe`・edit が穴の span を `n` に |
| 13 | `two_locals_of_the_type_give_no_fix` | `def f :: i64 -> i64 -> i64 = \a b -> ?\nf 1 2\n` | `candidates` の先頭 2 件が `a`・`b`（`how` `value`）。`fixes` が空 |
| 14 | `union_cases_are_candidates` | `union Shape = Circle of f64 \| Square of f64 \| Empty\nlet s: Shape = ?\ns\n` | `candidates` の先頭 3 件が `Main.Empty`（`value`）・`Main.Circle ?`（`call`）・`Main.Square ?`（`call`） |
| 15 | `candidates_are_capped_and_deterministic` | 9 のソースを `tsuzuri check --json DIR` に `TSUZURI_THREADS=1` と `=8` で 2 回 | 標準エラーが 2 回とも同じ。`candidates` が 10 件 |
| 16 | `only_the_first_sixteen_holes_of_a_function_get_candidates` | `def f :: i64 -> i64 = \n ->` の本体に `let a0: i64 = ?` … `let a16: i64 = ?`（17 個）と最後に `n` | 診断が 17 件。先頭 16 件の `candidates` が空でなく、17 件目は空 |
| 17 | `an_unknown_expected_type_has_no_candidates` | `let x = ?\nx\n` | `candidates` と `fixes` が空 |

### D.2 `tests/find.rs`（15 件。CLI を `Command` で実行）

std の内容に依って変わる件数は照合しない（行の「含む」と相対の順序だけ）。

| # | テスト名 | 入力 | 期待 |
| --- | --- | --- | --- |
| 1 | `std_search_ranks_exact_before_partial` | `find 'ref [i64] -> i64'` | 終了コード 0。行に `exact   Array.length :: ref ['a] -> i64`・`exact   Array.sum :: Numeric<'a> => ref ['a] -> 'a`・`partial Array.count :: (ref 'a -> bool) -> ref ['a] -> i64  (+1)`。`Array.sum_pairwise` を含む行がない。すべての `exact` の行が `partial` の行より前。各段の中が名前の昇順 |
| 2 | `borrow_tier_ignores_one_outer_ref` | `find 'string -> i64'`、`find 'ref string -> string -> bool'` | 前者に `borrow  String.length :: ref string -> i64` と `borrow  String.char_count :: ref string -> i64`。後者に `borrow  String.contains`・`String.ends_with`・`String.starts_with` があり、`exact` の行がない |
| 3 | `wildcards_and_type_variables` | `find '_ -> bool'`、`find "'a -> 'a"`、`find "(ref 'a -> bool) -> ref ['a] -> i64"` | 1 つ目に `exact   Array.is_empty :: ref ['a] -> bool`。2 つ目に `exact   Char.to_ascii_upper :: char -> char`。3 つ目の先頭の行が `exact   Array.count :: (ref 'a -> bool) -> ref ['a] -> i64` |
| 4 | `constraints_exclude_types_that_do_not_satisfy_them` | `find 'ref [string] -> string'`、`find 'ref [f64] -> f64'` | 前者に `Array.sum ::` の行がない。後者に `exact   Array.sum ::` と `exact   Array.sum_pairwise ::` がある |
| 5 | `result_and_takes_options` | `find --result bool`、`find --takes string --result i64`、`find --takes 'ref string' --result i64` | 1 つ目に `exact   Array.is_empty`。2 つ目に `borrow  String.length`。3 つ目に `exact   String.length` |
| 6 | `module_filter_and_unknown_module` | `find --module Array 'ref [i64] -> i64'`、`find --module Arrey i64` | 前者はすべての結果の名前が `Array.` で始まる。後者は終了コード 2、標準エラーに `E2000` と `no loaded module is named 'Arrey'` |
| 7 | `limit_and_the_truncation_line` | `find --limit 2 'ref [i64] -> i64'`、`--limit 0`・`--limit 101`・`--limit x` | 前者は標準出力が 3 行で、最後が `... N more; raise --limit (at most 100)`（`N` は整数）。後 3 つは終了コード 2 |
| 8 | `json_output_has_result_events_and_a_summary` | `find --json --limit 3 'ref [i64] -> i64'` | 標準出力の各行が JSON。先頭が `{"event":"result","tier":"exact","kind":"def","name":"Array.length","module":"Array","origin":"std","signature":"ref ['a] -> i64"}`。最後が `event` `summary`・`query` `ref [i64] -> i64`・`limit` 3・`shown` 3・`matches` が 4 以上・`searched` が 500 より大きい |
| 9 | `no_matches_exits_zero` | `find 'bool -> bool -> bool -> bool -> bool -> bool -> bool -> bool -> bool -> bool -> bool -> bool -> bool'`（引数 12 個） | 標準出力が `no matches\n`、終了コード 0 |
| 10 | `query_errors_exit_one_with_the_query_as_source` | `find 'i64 ->'`、`find 'Strng -> i64'` | どちらも終了コード 1。前者の標準エラーが `<query>:1:` で始まり `E0002`。後者に `E1004` と `unknown record, union, or type alias 'Strng'` |
| 11 | `usage_errors_exit_two` | `find`、`find a b`、`find --bogus x`、`find --limit`、`find --module A --module B x`、`find --result i64 x` | すべて終了コード 2、標準エラーに `E2000` と本文の使い方の誤りの表の message。`--json` を付けると `"kind":"find_bad_arguments"`（G22 Phase 1 のとき） |
| 12 | `project_declarations_are_searched` | `Main.tz`: `record Point { x: i64, y: i64 }\nunion Shape = Circle of f64 \| Empty\nconst limit: i64 = 10\ndef double :: i64 -> i64 = \n -> n * 2\ndouble limit\n`、`Helper.tz`: `def shared :: i64 -> i64 = \n -> n\nprivate def secret :: i64 -> i64 = \n -> n\n` を置いて `find --project DIR --limit 100 ...`（宣言はエントリの式より前に書く） | `'i64 -> i64'` に `exact   Main.double :: i64 -> i64` と `exact   Helper.shared :: i64 -> i64`、`Helper.secret` はない。`'i64'` に `exact   Main.limit :: i64`（`--json` の `kind` が `const`）。`'Point -> i64'` に `exact   Main.Point.x :: Main.Point -> i64`（`kind` が `field`）。`'f64 -> Shape'` に `exact   Main.Circle :: f64 -> Main.Shape`（`kind` が `case`）。`--json` の `origin` が `user` |
| 13 | `body_errors_do_not_stop_the_search` | 12 の `Main.tz` の `def double` の次の行に `def bad :: i64 -> i64 = \n -> n + true\n` を足す | `find --project DIR --limit 100 'i64 -> i64'` が終了コード 0。結果に `Main.double` と `Main.bad`。標準エラーが空 |
| 14 | `declaration_errors_fail_the_search` | `Main.tz`: `def bad :: Strng -> i64 = \n -> 0\n` | 終了コード 1。標準エラーに `E1004` |
| 15 | `output_is_deterministic_across_thread_counts` | 12 の project に `find --project DIR --limit 100 'i64 -> i64'` を `TSUZURI_THREADS=1` と `=8` で 2 回ずつ | 4 回の標準出力が同じ |

### D.3 `tests/name_suggestions.rs`（10 件。ライブラリ経由）

| # | テスト名 | 入力 | 期待 |
| --- | --- | --- | --- |
| 1 | `a_near_local_gets_a_help_and_a_maybe_fix` | `let total = 1\nlet y = totl + 1\ny\n` | `E1002`・message `unknown value 'totl'`。`candidates[0]` が `total`・`how` `local`・`signature` `total: i64`。help `did you mean 'total'?`。`fixes` が 1 件: title `Replace 'totl' with 'total'`・`Maybe`・edit が `totl` の span を `total` に |
| 2 | `no_near_name_leaves_the_diagnostic_unchanged` | `let count = 1\nlet y = cnt + 1\ny\n` | `candidates` と `fixes` が空。`--json` に `candidates`・`fixes` のキーがない。message は `unknown value 'cnt'` |
| 3 | `a_bare_member_name_gets_qualified_candidates` | `let xs = [1, 2]\nlength (ref xs)\n` | `E1002`・`unknown value 'length'`。`candidates` が `Array.length` を含み、その `how` が `qualify`。`String.length` も含む。`fixes` が空。help が `did you mean one of: 'Array.length'` で始まる |
| 4 | `a_module_member_typo_gets_the_member_name` | `let xs = [1, 2]\nArray.lenght (ref xs)\n` | `E1002`・`module 'Array' has no function or union case 'lenght'`・span は `lenght`。`candidates[0]` が `length`（`how` `similar`・`signature` `function`）。`fixes` が 1 件: title `Replace 'lenght' with 'length'`・edit の span が `lenght` |
| 5 | `a_module_name_typo_gets_the_module` | `let xs = [1, 2]\nAray.length (ref xs)\n` | `E1002`・`unknown value 'Aray'`。`candidates[0]` が `Array`（`signature` `module`）。fix が `Aray` を `Array` に |
| 6 | `an_unknown_type_gets_a_near_type` | `record Point { x: i64 }\nlet p: Pointt = Point { x: 1 }\np\n` | `E1004`・`unknown record, union, or type alias 'Pointt'`。`candidates[0]` が `Point`（`signature` `type`）。fix が `Pointt` を `Point` に |
| 7 | `an_unknown_field_gets_a_near_field` | (a) `record P { count: i64 }\nlet p = P { count: 1 }\np.cont\n`、(b) `record P { count: i64 }\nlet p = P { cont: 1 }\np\n` | (a) `E1007`・`unknown field 'cont'`、(b) `E1007`・`record 'Main.P' has no field 'cont'`。どちらも `candidates[0]` が `count`（`signature` `field`）。fix が `cont` を `count` に |
| 8 | `an_unknown_union_case_gets_a_near_case` | `union Shape = Circle of f64 \| Empty\nlet s = Empty\nmatch s with\n\| Circl r -> r\n\| Empty -> 0.0\n` | `E1020`・`unknown union case or active pattern 'Circl'`。`candidates[0]` が `Circle`（`signature` `union case`）。fix が `Circl` を `Circle` に |
| 9 | `an_ambiguous_best_gets_no_fix` | `let aa = 1\nlet ab = 2\nlet y = ac + 1\ny\n` | `candidates` が `aa`・`ab`。`fixes` が空。help が `did you mean one of: 'aa', 'ab'?` |
| 10 | `messages_codes_and_spans_do_not_change` | 1・3・4・5・6・7(a)・7(b)・8 の入力 | 各 `message`・`code`・主スパンが HEAD と同じ（上の表の message、span は `grep` した token の位置） |

### D.4 単体テスト

| ファイル | テスト名 | 内容 |
| --- | --- | --- |
| `src/lexer.rs` | `question_mark_is_a_token` | `?x` が `[Question, Ident("x")]`（そのあとに `End`） |
| `src/formatter.rs` | `holes_keep_their_spacing` | `f   ?a ?` → `f ?a ?`、2 回整形しても同じ |
| `src/parser.rs` | `parse_type_query_flattens_curried_functions` | `i64 -> (i64 -> bool)` が引数 2・結果 `bool` |
| `src/parser.rs` | `parse_type_query_rejects_trailing_input` | `i64 -> bool extra` が `E0002` |
| `src/parser.rs` | `parse_type_query_reads_wildcards_and_variables` | `_ -> 'a` が引数 1・結果 `Variable` |
| `src/parser.rs` | `parse_type_reads_one_type` | `i64 -> i64` が 1 つの `Function` の型。空文字列は `E0002` |
| `src/find.rs` | `wildcards_get_their_own_variables` | `_ -> _` を `name_wildcards` に通すと `Variable("_0")` と `Variable("_1")` |
| `src/find.rs` | `modules_match_by_full_name_or_last_part` | `module: "Geometry.Point"` が `Geometry::Point`・`Point` に合い、`Geometry` に合わない |
| `src/find.rs` | B.1 の 3 件 | `distances_are_damerau_levenshtein`・`qualified_names_come_first_and_win_when_alone`・`near_names_are_limited_by_length_and_ranked` |
| `src/main.rs` | `find_arguments_*` 4 件 | `find_arguments_read_a_query`（`['ref [i64] -> i64']`）、`find_arguments_read_options`（`--module Array --limit 5 --json --project d q`）、`find_arguments_reject_unknown_options`、`find_arguments_reject_a_bad_limit_and_repeats` |

## E. 実測

測定は、HEAD の複製に試作を入れた debug ビルド（測定の後に削除）と、HEAD の `target/release/tsuzuri`（2026-10-17 22:00 のビルド）。release ビルドで取り直すこと。

| 項目 | 結果 |
| --- | --- |
| `?` の現状 | `E0001`・`unexpected character '?'; identifiers use ASCII` |
| std | 47 モジュール、374,185 bytes、`def` 系の行 815 |
| 索引の大きさ（std 全部。非公開を含む） | 1,449 件 |
| 索引の公開された件数（std 全部／opt-in なし） | 981 件（`def` 559・`builtin` 192・`case` 123・`field` 107）／562 件（`def` 311・`builtin` 174・`case` 41・`field` 36） |
| 組み込み関数 | `Builtin::ALL` 246（`#[cfg(test)]` を除くと 243）のうち、索引に入るのは 192（残りは内部用で `E1022` か名前に `$`・`__`） |
| 索引の構築・照合の時間（debug） | 構築 約 5〜7 ms、照合 約 3〜13 ms（981 件） |
| `check` の時間（release、自明なプログラム） | opt-in なし 約 0.04 秒、std 全部 約 0.16 秒 |
| 問い合わせ `ref [i64] -> i64` | 16 件（`exact` 4・`partial` 12） |
| `string -> i64` | 8 件（`borrow` 2・`partial` 6） |
| `ref string -> string -> bool` | 3 件（すべて `borrow`） |
| `(ref 'a -> bool) -> ref ['a] -> i64` | 1 件（`exact`） |
| `_ -> bool`／`'a -> 'a`／`f64 -> f64`／`i64 -> i64 -> i64` | 114／271／52／13 件（すべて `exact` 中心。照合は変数の両側束縛なので `'a -> 'a` が多い） |
| 穴の候補（`xs: [i64]`・期待 `i64`） | 82 件を調べて上位 10 件を採る。先頭は `Array.length (ref xs)`・`Array.product (ref xs)`・`Array.sum (ref xs)`・`Parallel.sum (ref xs)` |
| 穴の候補（`Shape` の穴・局所なし） | 先頭は `Main.Empty`（順位 4）、`Main.Circle ?`・`Main.Square ?`（順位 5）。計 36 件 |
| 診断の再現 | `?` 15 か所の期待型は A.1。複数の穴と独立した誤りの共存（D.1 の 3）を確認。`tsuzuri build` は穴で何も作らない。`tsuzuri fmt` は `f   ?a ?` を `f ?a ?` にする |
| 未解決名の現状 | D.3 の 10 個の入力が HEAD で `E1002`（5 件）・`E1004`・`E1007`（2 件）・`E1020` を出すことを確認 |

