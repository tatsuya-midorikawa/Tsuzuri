# G44: 複数ケースのアクティブパターン（`-> 'T`）の本体が、最初のエラーしか報告しない

| 項目 | 内容 |
| --- | --- |
| ID | G44 |
| 優先度 | P3（複数ケースのアクティブパターンの本体に 2 つ以上のエラーがあるときだけ。報告されないエラーは、最初のエラーを直した次の `tsuzuri check` で出る。`tsuzuri fix` は内部のラウンドを繰り返すので、確かな修正案のあるエラーは 1 回の実行で直る） |
| 規模 | S（`src/check.rs` の `infer_active_result` とその呼び出し側、回帰テスト） |
| 依存 | – |
| 後続 | – |
| 状態 | done |
| 起票 | 2026-11-21（E17 の PR のレビューで見つかった、E17 より前からある制限。`feat/e17-io-function-names` の `2ffa444f` と `main` の `b6bbdcb5` の両方で再現） |
| 承認 | 不要（ほかの関数の本体と同じエラー回復にそろえる直し） |
| 改善する劣位 | 不具合: 1 回の `tsuzuri check` が、ほかの関数の本体ならすべて報告するエラー（E17 の `E1040` の旧名を含む）を、複数ケースのアクティブパターンの本体では最初の 1 つしか報告しない |
| 手本にする既存実装 | 関数の本体の検査（`src/check.rs` の `checker.recovering = true` の 2 か所と `recovered`・`recovered_functions`） |
| 主な影響ファイル | `src/check.rs`, アクティブパターンの既存のテスト（`tests/control.rs` など） |

## 観測

```tsuzuri
def (|Small|Large|) :: i64 -> 'T = \value ->
    let a = 1 + "x"
    let b = 2 + "y"
    if value < 10 then Small value else Large value

def main :: unit -> i32 = \() ->
    match 42 with
    | Small value -> 1
    | Large value -> 2
    | _ -> 0
```

`tsuzuri check` は `2:13: error[E1003]: expected string, found i32` だけを出し、3 行目の同じ `E1003` を出さない。同じ 2 つの束縛を普通の関数（`def f :: i64 -> i64`）に書けば、2 つとも出る。本体に E17 の旧名（`IO.write_line`・`IO.write_error`）を並べても、最初の 1 つしか出ない（`main` の中の旧名は出る）。

原因: `infer_active_result`（`src/check.rs`）は本体を検査する `Checker` を作るが、`recovering` を立てないので、最初のエラーが `Err` で返る。呼び出し側はその 1 つを積み、シグネチャを `Scheme::poisoned()` にするので、本体は回復ありで検査し直されない。

## 進め方

1. `infer_active_result` の `Checker` に `recovering = true` を立て、`recovered` に積まれた診断も呼び出し側へ返す（たとえば戻り値を `Result<Option<Type>, Vec<Diagnostic>>` にする）。
2. 呼び出し側は、硬いエラーか回復した診断のどちらかがあればシグネチャを毒にし、すべての診断を積む。同じ位置の診断を二重に積まない（`Diagnostics` の重複除去を確かめる）。
3. 回帰テスト: 観測のプログラムで 2 つの `E1003` が出ること、旧名を 2 つ並べた本体で 2 つの `E1040` が出ること、`tsuzuri fix` の 1 ラウンドで両方が直ること。

## 受け入れ条件

- [ ] 観測のプログラムの `tsuzuri check` が、2 行目と 3 行目の `E1003` を 1 回で報告する。
- [ ] 本体のエラーがないアクティブパターンの型推論と、既存のテストの結果が変わらない。

## 対象外

- 単一ケース・部分アクティブパターン（`infer_active_result` を通らず、普通の関数と同じ経路で検査される）。

## 実装と検証（2026-11-21）

`main` `52155373` で着手した。

- `infer_active_result`（`src/check.rs`）の `Checker` に `recovering = true` を立て、`recovered` に積まれた診断と硬いエラーをまとめて返す（戻り値は `Result<Option<Type>, Vec<Diagnostic>>`）。呼び出し側は、どちらかがあればすべてを積んでシグネチャを毒にする（1 つのときの今までの扱いと同じ）。回復した診断がないときだけ `finish` を通す。
- 回復した診断がなく、本体が別の毒の関数を使うだけ（`checker.poisoned` だけが立つ）のときは、`main` と同じく `finish` を通して結果を推論する（レビューで、そのときに診断なしで毒にしてしまう最初の実装を直した）。
- 本体の所有権・借用のエラー（`E1012` など）は、型のエラーがなくなってからの検査で報告する。普通の関数の本体は、型のエラーがあっても `ownership::check_recovered` でムーブとアクセスのエラーを一緒に報告するが、アクティブパターンの本体は推論に失敗するとシグネチャを毒にして本体を検査し直さないので、ここは同じにならない。文書（`active-pattern.md`・`docs/language.md`）はこの範囲だけを書いた。式の検査の後で確かめるエラー（`finish` の書式指定の型など）も、回復したエラーがあると `finish` を通さないので後から出る（ふつうの関数の本体でも同じ。Copilot のレビューで文言を絞った）。
- テスト（`tests/control.rs`）: 観測のプログラムで 2 つの `E1003` が 1 回の解析で出ること、本体の E17 の旧名 2 つとプログラムの旧名 1 つが 3 つの `E1040` になること、エラーのない本体の推論が変わらないこと、`tsuzuri fix --json` が 3 つの修正を 1 ラウンドで適用してエラーが残らないこと。

### 外した変更と、別のチケットにしたもの

- 実装中に、毒になった認識器を使う節でペイロードの名前が `E1002` の連鎖になることに気づき、ペイロードを `pattern_alternatives` に通して束縛する直し（D1）を足したが、レビューで panic（`Maybe.Some q`・レコードのペイロード）、ペイロードでない引数の束縛、エラー型にならない束縛が見つかったので外した。連鎖は G44 より前からある振る舞いで、直し方の案と一緒に [G45](../G45-poisoned-recognizer-payload-cascade.md) にした。
- レビューで、edition 2027 の既定の読み取りが `infer_active_result` で効かず、ペイロードを持つジェネリックな複数ケースのアクティブパターンを誤って拒否する不具合（G44 より前から）が見つかった。[A36](../A36-active-result-default-reads.md) にした。

### 検証（macOS arm64、dev ビルド）

- `cargo fmt --all -- --check`、`cargo clippy --locked --all-targets -- -D warnings`（ホストと `x86_64-pc-windows-msvc`）。
- `cargo nextest run --locked`（全体）、`tests/control.rs`（32 件）。
- `node scripts/check-docs.mjs _tsuzuri/language-reference/pattern-matching/active-pattern.md`。
- 独立のレビュー（コンパイラ・文書とテストの 2 つの観点と、指摘ごとの反証）。
