# G44: 複数ケースのアクティブパターン（`-> 'T`）の本体が、最初のエラーしか報告しない

| 項目 | 内容 |
| --- | --- |
| ID | G44 |
| 優先度 | P3（複数ケースのアクティブパターンの本体に 2 つ以上のエラーがあるときだけ。報告されないエラーは、最初のエラーを直した次の `tsuzuri check` で出る。`tsuzuri fix` は内部のラウンドを繰り返すので、確かな修正案のあるエラーは 1 回の実行で直る） |
| 規模 | S（`src/check.rs` の `infer_active_result` とその呼び出し側、回帰テスト） |
| 依存 | – |
| 後続 | – |
| 状態 | todo |
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
