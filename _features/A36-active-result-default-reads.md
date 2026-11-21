# A36: edition 2027 で、ペイロードを持つジェネリックな複数ケースのアクティブパターンを誤って拒否する

| 項目 | 内容 |
| --- | --- |
| ID | A36 |
| 優先度 | P2（正しいプログラムを `E1003` で拒否する。edition 2027 は既定の edition で、ジェネリックな入力の複数ケースのアクティブパターンがペイロードに入力を入れると必ず起きる） |
| 規模 | S（`src/check.rs` の `infer_active_result` と関数本体の引数の準備の共通化、回帰テスト） |
| 依存 | G44（done） |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-21（G44 の PR のレビューで見つかった、G44 より前からある不具合。`main` の `52155373` で再現） |
| 承認 | 不要（関数本体と同じ引数の扱いにそろえる直し） |
| 改善する劣位 | 不具合: edition 2027 の既定の読み取り（値で受けるジェネリックな引数は `ref 'a` として読み取る）が、複数ケースのアクティブパターンの結果の型の推論だけで効かない |
| 手本にする既存実装 | 関数本体の検査の引数の準備（`src/check.rs` の `checker.default_reads`・`checker.read_value_parameters` を埋める部分。`checker.recovering = true` の少し前） |
| 主な影響ファイル | `src/check.rs`, `tests/control.rs` |

## 観測

```tsuzuri
def (|Even|Odd|) :: 'number -> 'T
    @'number : Integer, Copy = \value ->
    | remainder == 0 -> Even value
    | otherwise -> Odd value
    where
        remainder = value % 2

match 42i64 with | Even number -> number | Odd _ -> 0 | _ -> -1
```

マニフェストなし（edition 2027）の `tsuzuri check` は `6:29: error[E1003]: expected ref 'number, found i32` と `8:35: error[E1002]: unknown value 'number'` を出す。`Tsuzuri.toml` の `edition = "2026"` では通る。同じ本体の普通の関数（`def f :: 'number -> 'number @'number : Integer, Copy = \value -> if value % 2 == 0 then value else value + 1`）と、ペイロードを持たない形（`| value % 2 == 0 -> Even`）は 2027 でも通る。`tests/control.rs` の `lambda_guards_accept_at_constraints` の同じ例が通るのは、`analyze` が edition 2026 で解析するため。

原因: `infer_active_result` は引数を `checker.bind` するだけで、関数本体の経路が行う既定の読み取りの準備（`default_reads`・`read_value_parameters`）をしない。

## 進め方

1. 関数本体の経路の引数の準備（束縛、`parameter_conventions`、読み取り、`closures::parameter_annotations`）を補助関数にまとめ、`infer_active_result` からも呼ぶ。
2. 回帰テスト: 観測のプログラムを `analyze_with_edition(…, Edition::E2027)`（または edition 2027 の既定）で受け付け、実行結果が 2026 と同じこと。2026 の結果が変わらないこと。

## 受け入れ条件

- [ ] 観測のプログラムが edition 2027 で通り、`tsuzuri run` が 2026 と同じ値を返す。
- [ ] 既存のアクティブパターンのテストと、普通の関数の既定の読み取りのテストが変わらない。

## 対象外

- 既定の読み取りの規則そのもの（edition 2027 の仕様）。
