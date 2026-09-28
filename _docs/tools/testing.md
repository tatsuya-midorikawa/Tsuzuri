# 言語内テスト

[ドキュメントのトップ](../README.md)

test 宣言は通常のソースと同じ型・所有権検査を受ける unit 型の計算です。通常の成果物には出力せず、test コマンドで各テストを別プロセスとして実行します。

## テストを書く

Main のないライブラリでも、例えば `Checks.tz` に置けます。

```tsuzuri project=checks file=Checks.tz
private def add :: i64 -> i64 -> i64
fn add left right = left + right

test "adds integers" = assert (add 20 22 == 42)

test "compares text" =
    let actual = "ready"
    let expected = "ready"
    Test.equal actual expected
```

test は `.tz` / `.tc` の宣言部分に置きます。`.tt` と埋め込み std では E1018 です。private / export / rec は付けず、通常の関数名空間にも入りません。同じモジュールの private helper は利用できます。

名前は妥当な Unicode 文字列です。空名・重複名も許可し、モジュールと宣言順の index で識別します。運用上は区別しやすい名前にすると filter が使いやすくなります。

## 実行する

テストを置くディレクトリを `target/test-demo` とする例です。

```sh
./target/release/tsuzuri test target/test-demo
./target/release/tsuzuri test target/test-demo --filter "Checks.adds"
./target/release/tsuzuri test target/test-demo --target wasm32 -O3
./target/release/tsuzuri test target/test-demo --json
```

既定は native / O0 です。O0 から O3、native / wasm32 を指定できます。WASM 実行には Node.js が必要です。cpu、emit、output のオプションは使えません。

Main は不要で、現在のプロジェクト loader でソースを探索します。filter は `Module.テスト名` の部分一致で、正規表現ではありません。除外したテストも型検査自体から除かれるわけではありません。

## アサーション

assert と Test.equal / Test.not_equal / Test.is_true を使います。比較は借用するので所有値を消費しません。失敗はトラップです。カスタム assertion message や詳細な値の差分表示はまだありません。

エラーを返す API は match で成功・失敗を検査し、通常の Error とトラップを混同しないようにします。

## 隔離と実行順

各テストは別プロセスです。CPU 数、32、選択件数の最小値まで並列に実行し、結果は宣言順に報告します。あるテストのトラップで、ほかのテストの結果収集を中止しません。

トラップ、非ゼロ終了、30 秒の timeout は失敗です。timeout 設定の CLI はありません。テスト内に停止しないループを作っても無制限には待ちません。

## JSON と終了コード

test ごとの結果と最後の summary は stdout に一行一 JSON オブジェクトで出します。コンパイルなどの診断は stderr です。両ストリームを一つの JSON 配列として解析しないでください。

全成功または選択件数 0 は終了コード 0、失敗は 1 と E2006 です。filter で除外された件数は ignored です。CI で実行漏れを検出したい場合は、終了コードだけでなく選択件数も確認します。

## 通常ビルドとの関係

check / build でもテストを検査しますが、テスト本体とテスト専用 lambda / 特殊化は通常出力へ含めません。テスト用にしか呼ばない関数が本番コードへ到達するとは限りません。

コンパイラ自身の Rust テスト・E2E と、利用者プログラムの test 宣言は別です。前者の実行手順は[リポジトリの検証手順](../../README.md#検証と性能測定)を参照してください。

## 関連項目

- [Test API](../library-reference/builtins.md)
- [診断](diagnostics.md)
- [WASM ホスト](../guides/webassembly.md)
