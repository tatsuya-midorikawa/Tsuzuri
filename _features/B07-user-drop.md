# B07: ユーザー定義の解放処理（Drop）とリソース型

| 項目 | 内容 |
|---|---|
| ID | B07 |
| 優先度 | P1 |
| 規模 | L |
| 依存 | A06 |
| 後続 | E08, E12, B08, C10, F10 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | C#/F# 比: `IDisposable`／`use` に相当する資源管理がない（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#cf-に対する劣位点)）／追加: ホスト資源を所有できない |
| 主な影響ファイル | `src/check.rs`, `src/polymorph.rs`, `src/ownership.rs`, `src/llvm.rs`, `src/llvm_recursive.rs`, `src/llvm_frame.rs`, `docs/language.md`, `docs/architecture.md`, `tests/drop.rs`（新規） |

## 目的

ファイル記述子・ソケット・GPU バッファ・ホストのハンドルなど、メモリ以外の資源を所有値のスコープ終了時に一度だけ確実に解放できるようにする。
Rust の `Drop`、C++ の RAII、C# の `IDisposable` に相当し、OS API（E08）と不透明ハンドル（E12）の前提になる。

## 現状

- 解放はコンパイラ生成だけ。`Type::needs_drop`（`src/check.rs`）と `FunctionEmitter::drop_value`（`src/llvm.rs`）が組み込み型・record・union・コレクション・関数値・Task を扱う。
- 再帰 union は `src/llvm_recursive.rs` の反復 drop、コレクション要素は一つの走査で解放する（D-24）。
- `use` 束縛と `try` 式は `src/parser.rs` が `E1018`（`"use bindings are not supported; bind the owned value with let and rely on lexical drop"`）で拒否する。
- docs/architecture.md「初版の次に必要な設計」: 任意の destructor は未対応。
- `Gpu.Device` などの opaque・non-Copy 型はコンパイラ登録の特別扱いで、利用者は同様の型を定義できない。

## 仕様

### Phase 1（実装対象）

- 組み込みクラス `Drop<'a> { def drop :: ref 'a -> unit }` を追加する。instance は利用者宣言の record・union（ジェネリック可）にだけ書ける。
  数値・配列などの組み込み型や型別名への instance は `E1016`。
- `Drop` instance を持つ型は Copy にならない（自動 Copy 推論から除外）。Copy 制約を要求する API に渡すと従来どおり型エラー。
- 呼び出し時点: 値がスコープ終了・置換（代入）・未使用の破棄で解放されるとき、フィールド・payload の解放より前に一度だけ呼ぶ。move 済みの値では呼ばない。
- `Drop` 型からのフィールドの部分 move と、レコード更新 `{ value with ... }` による取り出しは `E1012` で拒否する（drop は常に完全な値を見る）。
- drop 本体は通常の関数で、トラップは通常どおり。トラップ時の未実行 drop は保証しない（既存の方針）。
- Task・Parallel・`parallel_results` の未開始タスクの捕捉値、`Vec.pop`／`truncate`、Map/Set の値、再帰 union の payload でも同じ規則で呼ぶ。

### Phase 2（設計方針）

- 可変な後処理が必要な場合の `ref mut 'a` 版、B05 で未対応の計算式 `use`／`use!` の糖衣化（lexical drop と同じ意味）を検討する。

## 設計

- `Classes` に `Drop` を追加し、instance の有無を `Type::is_copy`／`needs_drop` に反映する。GUIDE §6.3 の「性質」を全経路で確認する。
- `drop_value` の該当型の分岐で、フィールド解放の前に `$instance.<id>.drop` を借用付きで呼ぶ。
- 反復 drop（`llvm_recursive.rs`）とコレクション要素の解放ループにも同じ呼び出しを入れる。drop 本体が確保しても、反復 drop の待ちリストの不変条件を壊さない。
- 所有権検査は drop 呼び出しを暗黙の共有借用として扱い、フレーム上の値（`llvm_frame.rs`）の relocate 後にも一度だけ呼ぶ。

## 実装手順

1. **クラスと検査**: `Drop` の宣言・instance の制約・Copy 除外。確認: `tests/drop.rs` の受理・拒否（`E1016`、`E1012`）。
2. **スコープ終了と置換**: ローカル・引数・代入・分岐・ループ・`break`／`continue`。確認: 呼び出し回数を記録する extern を使う E2E。
3. **コレクション・再帰型**: 配列・リスト・Vec・Map・再帰 union の要素。確認: 100 万ノードの反復 drop でもスタックを消費しない。
4. **Task 境界**: 未実行タスク・`parallel_results` の未開始分。確認: 各経路で一度だけ呼ばれる。
5. **文書**: 解放順序と保証しない場合（トラップ）。

## テスト計画

- Rust: 宣言・Copy 除外・部分 move 拒否・ジェネリック instance の具体化。
- E2E: native/WASM × `-O0`/`-O3` で drop 回数・順序をホスト関数のカウンターで照合し、`live == 0` を確認する。ASan で二重解放がないことを確認する。

## ドキュメント

- `docs/language.md` の Ownership・型クラス、`docs/architecture.md` の「初版の次に必要な設計」、`_docs/language-reference/ownership.md`、`_docs/guides/from-fsharp.md`（`use` との対応）。

## 受け入れ条件

- [ ] `Drop` instance を持つ値が、正常終了のすべての解放経路でちょうど一回 drop される。
- [ ] 既存型の IR と性能が変わらない（`Drop` を使わないプログラムの IR が同一）。
- [ ] 部分 move・Copy の誤用を拒否する。

## 落とし穴

- `clone_value` が `Drop` 型を複製しないこと（非 Copy なので本来到達しないが、ジェネリック経路を確認する）。
- drop 本体からの自身の置換・move を許すと再帰 drop になる。共有借用に限定する理由である。
- `parallel_results` の「開始済み環境を二重に drop しない」既存の不変条件と組み合わせる。

## 対象外

- トラップ時の巻き戻しと drop、finalizer、GC との連携、drop 順序の利用者指定。

## 未決事項

- **drop の引数**: 既定案は `ref 'a`。可変処理が必要な資源は後続段階で `ref mut 'a` を検討する。
- **`use` 構文**: 既定案は Phase 1 では追加せず、lexical drop だけを使う。
