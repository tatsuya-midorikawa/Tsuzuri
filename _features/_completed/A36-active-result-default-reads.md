# A36: edition 2027 で、ペイロードを持つジェネリックな複数ケースのアクティブパターンを誤って拒否する

| 項目 | 内容 |
| --- | --- |
| ID | A36 |
| 優先度 | P2（正しいプログラムを `E1003` で拒否する。edition 2027 は既定の edition で、ジェネリックな入力の複数ケースのアクティブパターンがペイロードに入力を入れると必ず起きる） |
| 規模 | S（`src/check.rs` の `infer_active_result` と関数本体の引数の準備の共通化、回帰テスト） |
| 依存 | G44（done） |
| 後続 | – |
| 状態 | done |
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

## 実装と検証（2026-11-21）

`main` `4c36bd1a` で着手した。

- 関数本体の引数の準備（束縛、`mut` の引数と、`reads` が印を付けた引数（edition 2027 の既定の読み取り、`pure def` の借用、クラスの読み取り）のうち型変数や関数に届くものの `read_value_parameters`、`default_reads` のヒント、A32 の注釈）を `Checker::bind_function_parameters` にまとめ、関数本体の経路と `infer_active_result` の両方で使う。`infer_active_result` は、その関数の `default_reads` を受け取る（引数が 8 つになったので、ほかの検査器の入口と同じく `clippy::too_many_arguments` を許す）。
- 観測のプログラムは edition 2027 で通り、`tsuzuri run` は 2026 と同じ `42` を `-O0`・`-O3` で出す。同じ理由で、`pure def` の複数ケースのアクティブパターン（ジェネリックな入力）も、どの edition でも通るようになった（`main` では `E1003`）。

### 振る舞いの変化（意図どおり）

edition 2027 で、ジェネリックな入力をそのままペイロードに入れる複数ケースのアクティブパターン（`(|First|Second|) :: 'a -> 'T` の `First value`）は、`main` では推論が引数を読まなかったのでペイロードが `ref 'a`（入力の借用）になり、非 Copy の入力（`"abc"`）でも通っていた。今は関数本体と同じく読み取った値（`'a`）なので、非 Copy の入力では使う側で `E1005`（`Copy<string>`）になり、リストや配列の入力は暗黙の複製になる。edition 2026 と普通の関数の本体は前からこの振る舞いで、言語リファレンス（`generics-functions.md` の edition 2027 の読み取り規約、`active-pattern.md` の所有権と評価）もこのとおりに書いてある。借用のまま渡すなら、シグネチャに `ref 'a` と書く。

### テスト（`tests/control.rs`）

- `generic_active_patterns_read_their_parameters_in_edition_2027`: 観測のプログラム、入力から計算したペイロード、2 つの型での使用が 2026・2027 で通る。ペイロードの型の誤りは、両方の edition で同じ文（`expected 'number, found string`）の `E1003`（`main` の 2027 では別の文になる）。`pure def` の形。非 Copy の入力の `E1005` と、`ref 'a` の形が通ること。
- `generic_active_patterns_run_the_same_in_editions_2026_and_2027`: マニフェストで edition 2026 にしたプロジェクトと、マニフェストのないプロジェクト（2027）で、`tsuzuri run` が `-O0`・`-O3` とも `42` を出す。

### レビュー

独立のレビュー（コンパイラ・文書とテストの 2 つの観点と、指摘ごとの反証）で、テストの弱さ（実行しない、`main` でも通る検査、`pure def` と非 Copy の変化を固定しない）と、補助関数の doc comment の言い過ぎを直した。2026 の結果・診断・警告はすべて `main` と同じで、2027 で両方が受け付けるプログラムの `--emit llvm` も同じだった。言語リファレンスは変える必要がなかった。

### 検証（macOS arm64、dev ビルド）

- `cargo fmt --all -- --check`、`cargo clippy --locked --all-targets -- -D warnings`（ホストと `x86_64-pc-windows-msvc`）。
- `cargo nextest run --locked`（全体）、`tests/control.rs`（34 件）。
