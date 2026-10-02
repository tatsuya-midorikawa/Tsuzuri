# 実践ガイド

[ドキュメントのトップ](../README.md)

言語機能を組み合わせて、計算をホストへ組み込むためのガイドです。外部 I/O はホスト、値の計算と所有する状態は Tsuzuri に分けます。

| 記事 | 内容 |
| --- | --- |
| [F# からの移行](from-fsharp.md) | 似た構文の違い、所有権、task、非互換機能 |
| [API 設計とスタイル](style-and-design.md) | 型と所有権の選択、データ構造、失敗、テスト |
| [C ホスト連携](native-interop.md) | object / header、export、extern、リンク名、ハンドル、コールバック、ホストのリンク指定、バッファ、信頼境界 |
| [WebAssembly](webassembly.md) | Number / BigInt、memory、文字列、SIMD opt-in |
| [WASM threads](wasm-threads.md) | Node Worker pool、shared memory、失敗、ブラウザーの条件 |
| [GPU](gpu.md) | CPU 参照、strict WGSL、WebGPU host、現在の限界 |
| [性能](performance.md) | O0 / O3、CPU dispatch、コピー、順序、再現可能な比較 |

## 動く例を探す

[アプリケーションの実装例](../examples/README.md)には、料金見積もり、単語カウンター、TODO リスト、Node.js / WASM によるファイル集計の完成コードとテストがあります。

[hello](../../examples/hello/Main.tz)、[C ホスト](../../examples/native/main.c)、[Web ホスト](../../examples/web/README.md)、[GPU ホスト](../../examples/gpu/run.mjs)を参照できます。ブラウザー用の通常 WASM と Node 向け threads は異なるホストです。

実験的機能や未検証ターゲットの境界は[対応状況](../feature-status.md)に記載します。詳しい構文は[言語リファレンス](../language-reference/README.md)、関数の引数順は[ライブラリ](../library-reference/README.md)で確認してください。
