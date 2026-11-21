# G45: 毒になった認識器を使う節で、ペイロードの名前が `E1002` の連鎖になる

| 項目 | 内容 |
| --- | --- |
| ID | G45 |
| 優先度 | P3（認識器の宣言にエラーがあるときだけ。本当のエラーは宣言の側に出ていて、宣言を直せば連鎖も消える） |
| 規模 | S〜M（`src/control.rs` の `active_pattern`、認識器の引数の数の記録、回帰テスト） |
| 依存 | G44（done） |
| 後続 | – |
| 状態 | done |
| 起票 | 2026-11-21（G44 の実装中に見つけた、G44 より前からある振る舞い。G44 で試した直し方はレビューで崩れたので外し、ここに記録する） |
| 承認 | 不要（診断の連鎖を減らす直し。受け付けるプログラムは変えない） |
| 改善する劣位 | 不具合: 本体やシグネチャのエラーでシグネチャが毒になった認識器を使う節では、ペイロードのパターンが名前を束縛しないので、節の本体のその名前が `unknown value`（`E1002`）になる |
| 手本にする既存実装 | `src/control.rs` の `active_pattern`（`ty.contains_error()` の分岐）と `pattern_alternatives` |
| 主な影響ファイル | `src/control.rs`, `src/check.rs`（認識器の登録）, `tests/control.rs` |

## 観測

```tsuzuri
def (|Small|Large|) :: i64 -> 'T = \value ->
    let a = 1 + "x"
    if value < 10 then Small value else Large value

match 42 with | Small value -> value | Large value -> value + 1 | _ -> 0
```

`tsuzuri check` は本体の `2:13: error[E1003]` のほかに、`5:32: error[E1002]: unknown value 'value'` と `5:55: error[E1002]: unknown value 'value'` を出す（`main` の `52155373` でも同じ）。後の 2 つは宣言のエラーの連鎖で、宣言を直すと消える。

## G44 で試して外した直し方（同じ失敗をしないために）

毒の分岐で、最後の引数をペイロードとして `pattern_alternatives(payload, TypedExpr::error(..))` に通し、その束縛を残した。レビューで次が分かり、外した。

1. **panic**: ペイロードが値を持つ共用体のケース（`Maybe.Some q`）や修飾したレコード（`Point { x = a; y = b }`）だと、`union_case_pattern`（`unreachable!("the pattern type was unified with a union")`）や `pattern_alternatives` のレコードの分岐（`unreachable!("the pattern type was unified with a record")`）に入る。`Error` との単一化は成功するが、`matched.ty` は `Error` のままだから。G44 より前に毒になる認識器（シグネチャの型の誤りなど）でも起きる。
2. **ペイロードでない引数**: 真偽値の部分パターンや、追加の引数を取りペイロードのないケースでは、最後の引数は値の式（外の変数）なので、それを新しい名前として束縛して外の変数を隠し、本当のエラーを消したり、別の `E1007` を足したりする。毒のシグネチャには引数の数がないので、`arguments.last()` がペイロードかは分からない。
3. **型がエラー型にならない**: タプル・配列・リストの要素や、入れ子のジェネリックな認識器のペイロードは、新しい推論変数で型が付き、`Error` との単一化では決まらないので、正しい節に別の `E1007` を出す。or パターン（`Small (a | a)`）は選択肢が 2 つで束縛が残らず、型テストのペイロードは失敗する。

## 進め方（案）

1. 認識器を登録するとき（`src/check.rs` の `active_patterns` の登録）、宣言に書いた引数の数を記録する（毒になっても残る）。
2. 毒の分岐では、真偽値の部分パターンなら束縛しない。ほかは引数の数が「追加の引数 + 1」のときだけ、最後の引数をペイロードとして扱う。
3. ペイロードのパターンを字句の木として歩き、束縛する名前（`Binding`・`As`・タプル・レコードの欄・ケースや認識器のペイロード・配列・リスト・`::`・注釈・or パターンの最初の選択肢）を集め、ケース名・認識器・リテラルを除いて、それぞれを `TypedExpr::error(name.span)` に束縛する。`pattern_alternatives` は通さない。
4. 回帰テスト: 観測のプログラムで `E1002` が出ないこと。ペイロードが `Maybe.Some q`・修飾したレコード・入れ子のケース・タプル・or パターン・型テストのとき panic せず `E1002` も出ないこと。真偽値の部分パターンと追加の引数の節で外の変数が隠れないこと（`main` と同じ診断）。宣言のエラーは必ず残ること。

## 受け入れ条件

- [ ] 観測のプログラムの `tsuzuri check` が、本体の `E1003` だけを出す。
- [ ] 上の形のどれでも panic しない。誤ったプログラムを受け付けない（宣言のエラーが残る）。
- [ ] 毒でない認識器の節の検査と、既存のテストが変わらない。

## 実装と検証（2026-11-21）

`main` `4c36bd1a` で着手した。

- `DeclaredFunction` に `bool_partial`（部分パターンの認識器で、宣言した結果が `bool` に解決する。別名を含む）を足し、認識器を登録するところ（`src/check.rs`）で `resolve_type` から決める。シグネチャのほかの部分が毒になっても残る。
- `src/control.rs` の `active_pattern` の毒の分岐で、`poisoned_payload`（宣言の引数の数と同じ数の引数を書いたときの最後の引数。`bool_partial` ならなし）を `payload_binders`（パターンの字句から束縛する名前を集める。ケース名・認識器・修飾した名前・値の引数は除く。or パターンは左側、入れ子の認識器は同じ規則でペイロードだけ）で歩き、それぞれを `TypedExpr::error(name.span)` に束縛する。`pattern_alternatives` は通さない（G44 で外した方法の panic を避ける）。
- テスト（`tests/control.rs` の `a_poisoned_recognizer_still_binds_its_payload_names`）: 観測のプログラム、`Maybe.Some q`、修飾したレコード、タプル、or パターン、`::`、入れ子のケース、型テスト、入れ子の認識器（`Even`・`n & Even`・`as`）、配列、注釈のペイロードで、宣言の `E1003` だけが出て、束縛しない名前（`missing`）は `E1002` のまま。シグネチャで毒になった部分パターン（`ref Missing -> Maybe<Maybe<i64>>`）、`bool` を返す単一ケースの全域の認識器（ペイロードを束縛する）、`bool`（と別名）を返す部分パターン（`Even x` の `x` は束縛しない）、ペイロードの前の値の引数（外の `k` のまま）。
- 文書: `active-pattern.md`、`docs/language.md`。

### レビュー

独立のレビュー（コンパイラ・文書とテストの 2 つの観点と、指摘ごとの反証）で、最初の実装が `bool` の規則を全域の単一ケースの認識器にも当てていた誤り（`(|IsBig|) :: … -> bool` のペイロードを束縛しなかった）、`bool` の別名を見ないこと、テストの不足（型テスト・入れ子の認識器・配列・`as`・注釈、`bool` の規則だけで決まる例、値の引数の検査が弱いこと）、文書の言い過ぎ（「報告するのは宣言のエラーだけ」、複数ケースだけに見える置き場所）を直した。panic・誤って受け付けるプログラム・外の変数を隠す束縛は見つからなかった。

### 検証（macOS arm64、dev ビルド）

- `cargo fmt --all -- --check`、`cargo clippy --locked --all-targets -- -D warnings`（ホストと `x86_64-pc-windows-msvc`）。
- `cargo nextest run --locked`（全体）、`tests/control.rs`（33 件）。
- `node scripts/check-docs.mjs _tsuzuri/language-reference/pattern-matching/active-pattern.md`。
