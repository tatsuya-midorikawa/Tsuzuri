# Tsuzuri (綴り) 言語リファレンス

## index

### 概要

- [Tsuzuri の特徴](./languages/how-about-tsuzuri.md)
- [Tsuzuri 言語の戦略](./languages/strategy.md)
- [なぜ Tsuzuri なのか](./languages/why-tsuzuri.md)

### リファレンス

#### コードの整理

- [名前空間](./organizing-tsuzuri/namespaces.md)
- [モジュール](./organizing-tsuzuri/modules.md)
- [パッケージ](./organizing-tsuzuri/packages.md)
- [using 宣言](./organizing-tsuzuri/using-declarations.md)
- [アクセス制御](./organizing-tsuzuri/access-control.md)
- [ドキュメント コメント](./organizing-tsuzuri/documentation-comment.md)

#### リテラルと文字列

- [リテラル](./literals-and-strings/literals.md)
- [文字列](./literals-and-strings/strings.md)
- [補間文字列](./literals-and-strings/interpolated-strings.md)

#### 値と関数

- [値](./values-and-functions/values.md)
- [キーワード](./values-and-functions/keywords.md)
- [演算子と式](./values-and-functions/op-and-expressions.md)
- [ステートメント](./values-and-functions/statements.md)
- [特殊文字](./values-and-functions/tokens.md)
- [関数 / 高階関数 / 再帰関数](./values-and-functions/functions.md)
- [ジェネリック関数と型パラメータ制約](./values-and-functions/generics-functions.md)
- [ラムダ式](./values-and-functions/lambda-expressions.md)
- [属性](./values-and-functions/attributes.md)

#### ループと条件

- [if 式](./loops-and-conditionals/if.md)
- [for...in 式](./loops-and-conditionals/for-in.md)
- [for...to 式](./loops-and-conditionals/for-to.md)
- [while 式](./loops-and-conditionals/while.md)

#### パターンマッチング

- [パターンマッチング](./pattern-matching/pattern-matching.md)
- [match 式](./pattern-matching/match.md)
- [Active パターン](./pattern-matching/active-pattern.md)

#### 例外処理

- [例外処理](./exception-handling/exception-handling.md)
- [try 式](./exception-handling/try-with-finally.md)
- [use キーワード](./exception-handling/use.md)
- [assert 式](./exception-handling/assert.md)

#### 型

- [型](./types-and-type-inference/types.md)
- [基本型](./types-and-type-inference/basic-types.md)
- [Unit 型](./types-and-type-inference/unit-type.md)
- [型エイリアス](./types-and-type-inference/alias.md)
- [型推論](./types-and-type-inference/type-inference.md)
- [型キャスト](./types-and-type-inference/cast.md)
- [ジェネリック](./types-and-type-inference/generics.md)
- [型クラス](./types-and-type-inference/type-classes.md)
- [制約 と 属性](./types-and-type-inference/constraints.md)

#### 所有権とメモリ

- [所有権とムーブ](./ownership-and-memory/ownership.md)
- [借用と参照](./ownership-and-memory/borrowing.md)
- [ライフタイムと region](./ownership-and-memory/lifetimes.md)
- [スタックとヒープ](./ownership-and-memory/stack-and-heap.md)
- [Drop とリソースの解放](./ownership-and-memory/drop.md)

#### 組み込み型 / 組み込みモジュール

- [Record](./built-in-types-and-modules/record.md)
- [Union](./built-in-types-and-modules/union.md)
- [Tuple](./built-in-types-and-modules/tuple.md)
- [String](./built-in-types-and-modules/string.md)
- [Utf8String](./built-in-types-and-modules/utf8string.md)
- [Char](./built-in-types-and-modules/char.md)
- [Utf8Char](./built-in-types-and-modules/utf8char.md)
- [Array](./built-in-types-and-modules/array.md)
- [List](./built-in-types-and-modules/list.md)
- [Vec](./built-in-types-and-modules/vec.md)
- [Map](./built-in-types-and-modules/map.md)
- [Set](./built-in-types-and-modules/set.md)
- [HashMap](./built-in-types-and-modules/hashmap.md)
- [HashSet](./built-in-types-and-modules/hashset.md)
- [Arena](./built-in-types-and-modules/arena.md)
- [Seq](./built-in-types-and-modules/seq.md)
- [Maybe](./built-in-types-and-modules/maybe.md)
- [Result](./built-in-types-and-modules/result.md)
- [Math](./built-in-types-and-modules/math.md)
- [Int](./built-in-types-and-modules/int.md)
- [BigInt](./built-in-types-and-modules/bigint.md)
- [Debug](./built-in-types-and-modules/debug.md)
- [Parallel](./built-in-types-and-modules/parallel.md)
- [Simd](./built-in-types-and-modules/simd.md)
- [Test](./built-in-types-and-modules/test.md)
- [Gpu](./built-in-types-and-modules/gpu.md)
- [IO](./built-in-types-and-modules/io.md)
- [Owned](./built-in-types-and-modules/owned.md)
- [File](./built-in-types-and-modules/file.md)
- [Dir](./built-in-types-and-modules/dir.md)
- [Path](./built-in-types-and-modules/path.md)
- [Env](./built-in-types-and-modules/env.md)
- [Time](./built-in-types-and-modules/time.md)
- [Random](./built-in-types-and-modules/random.md)
- [Os](./built-in-types-and-modules/os.md)
- [Process](./built-in-types-and-modules/process.md)
- [Format](./built-in-types-and-modules/format.md)
- [Exception](./built-in-types-and-modules/exception.md)

#### 非同期処理

- [Async 式](./async-tasks-and-lazy/async.md)
- [Task 式](./async-tasks-and-lazy/task.md)
- [Lazy 式](./async-tasks-and-lazy/lazy.md)

#### コンピュテーション式

- [コンピュテーション式](./computation-expressions/computation-expressions.md)

#### コンパイラ

- [コンパイラの使い方](./compiler/usage.md)
- [コンパイラ オプション](./compiler/option.md)
- [コンパイラ ディレクティブ](./compiler/directives.md)
- [診断メッセージとエラーコード](./compiler/diagnostics.md)
- [WebAssembly への出力](./compiler/webassembly.md)
- [ネイティブ連携 (C ABI)](./compiler/native-interop.md)
