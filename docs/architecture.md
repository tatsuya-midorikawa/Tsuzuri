# コンパイラ構成

実装言語は Rust。binary リテラルの正確な丸めには `rustc_apfloat` を使い、
LLVM の C API／Rust バインディングには結合しません。
テキスト形式の LLVM IR と、バージョン差の小さい Clang／LLD の CLI を境界に使います。
`cargo build` に LLVM 開発ヘッダーや CMake は不要です。

```text
UTF-8 .tzr files in one directory (application entry: Main.tzr)
   -> driver -> sorted source files + filename-based module names
   -> lexer -> tokens + per-file byte spans
   -> parser -> syntax AST per file
   -> check -> module-scoped names + resolved, typed value IR
   -> ownership -> moves + loans + lifetime validation
   -> llvm -> deterministic LLVM IR
   -> Clang -O0..3 -> native executable / PIC object
                   -> wasm32 object -> LLD -> standalone .wasm
```

| ファイル | 責務 |
|---|---|
| `src/diagnostic.rs` | ソース ID とファイル内位置、安定コード、human／JSON 診断 |
| `src/syntax.rs` | トークン、構文木、構文資源上限 |
| `src/lexer.rs` | UTF-8 を壊さない字句走査、コメント、数値 |
| `src/parser.rs` | Pratt parser、宣言と式、トップレベルのエントリーコード、深さの制限 |
| `src/check.rs` | 全モジュールのシグネチャ収集、名前解決、単相型検査、レイアウト、公開 ABI |
| `src/numeric.rs` | プリミティブ名、整数・浮動小数点接尾辞、binary／decimal リテラルの丸めとエンコーディング |
| `src/ownership.rs` | 部分 move、借用の競合、最後の使用、分岐の合流、参照の寿命 |
| `src/llvm.rs` | SSA、phi、末尾ループ、所有値の解放、借用先、ホスト・ラッパー、C ヘッダー |
| `src/runtime/numeric.c` / `numeric.ll` | 多倍長整数による f16／f128／decimal 演算、比較、変換、表示 |
| `src/runtime/string.ll` / `heap-*.ll` | UTF-8 バッファ操作、ネイティブ確保、WASM の再利用・結合可能なヒープ |
| `src/runtime/wasm.ll` | 128-bit 乗除算・剰余・シフトの freestanding 補助 |
| `src/driver.rs` | ソースファイルの列挙、`Main.tzr` 選択、LLVM／LLD 起動、ステージング、出力保護 |
| `src/main.rs` | CLI オプションと診断表示 |

## 不変条件

**モジュール:** 1 ファイルに 1 モジュールを強制し、名前はファイル名から取得します。
同じディレクトリ直下の全 `.tzr` を名前順に処理し、サブディレクトリは探索しません。
`analyze_modules` はメモリ上の複数ソースを検査し、`analyze` は単独の `Main` モジュールとして検査します。
関数とレコードはモジュールで修飾した一意な名前を持ち、LLVM の内部シンボルにも修飾名を使います。
他モジュールの関数は修飾が必須で、レコードだけは自モジュール優先・他モジュールで一意なら
無修飾名を解決します。型付き IR の関数・レコード参照はプロジェクト全体で一意な ID です。
公開 ABI は従来の `tz_name` を維持し、エクスポート名の衝突は型検査で拒否します。

**エントリー:** `Main.tzr` のトップレベルコードを合成した非公開関数、または `Main.main` の
関数 ID を保持します。両者の併用は拒否し、他モジュールの `main` は入口に選びません。
トップレベルの `let` は通常のローカル束縛へ下げ、モジュールの共有状態は導入しません。
ライブラリ出力にはコンソール・ラッパーやトップレベルコードの自動実行を追加しません。

**フロントエンド:** 全ファイルのシグネチャを先に収集するため、宣言順・ファイル順に依存しません。
ローカル束縛は一意な ID に解決します。コード生成時に名前解決や型推測をやり直しません。
暗黙変換、未知の名前への既定値、未対応構文の読み飛ばしは行いません。
決定性が必要な名前集合は順序付きコレクションです。
Span はソース ID とファイル内バイト位置を保ち、字句・構文・型・レイアウトのエラーを
元のファイルに対応付けます。ソースの連結や診断オフセットの書き換えは行いません。

**数値:** 各整数幅・符号、各浮動小数点形式は別の型です。byte／ubyte だけは i8／i8u の別名です。
リテラルは型の文脈または接尾辞で決定し、変数を暗黙変換しません。
binary リテラルを f64 に落としてから f128 に拡大するような二重丸めは禁止です。
decimal は BID の有限値／非正規化数／符号付きゼロ／無限大／NaN を保持します。
ソフトウェア演算は基数 2 または 10 の整数係数と指数を復号し、多倍長整数で計算して、
目的の精度へ一度だけ最近接・偶数丸めします。binary64 による近似代用は行いません。

**所有権:** 型検査済みのローカル ID とフィールド経路に対して move と loan を検査します。
引数などの一時的な loan も、後続オペランドの評価が終わるまで保持します。
再借用は元の loan を親として追跡し、子の生存中に元の排他参照を使用・移動できません。
参照を結果・外側の束縛へ移すときは、参照先の所有者が生存していることを検証します。
借用を返す関数は単一の借用入力に寿命を結び付けます。参照フィールドと名前付きライフタイムは未対応です。
レコード／配列の Copy は構造的に決まり、文字列を含む値は所有権を移動します。

**LLVM:** 整数の算術に `nsw`／`nuw` を付けません。除算／剰余はゼロと符号付き MIN/-1 を検査し、
シフト数を幅ごとにマスクし、浮動小数点→整数には飽和変換を使います。
負の添字を含め、配列の範囲検査を GEP／load の前に行います。
浮動小数点に fast-math フラグを付けません。
借用可能な値の領域は entry に置きます。所有値を move すると移動元をゼロにし、
スコープ終了時に残った文字列フィールドだけを解放します。レコードの部分 move も同じ仕組みです。

**末尾再帰:** 直接の自己末尾呼び出しは引数評価の後で loop に分岐し、
全引数を phi のバックエッジで同時更新します。
所有するローカル・配列アクセス・数値演算の一時 alloca は必ず entry に置きます。
これをループ内に置くと、ソースでは末尾再帰でもスタックを消費し続けるためです。
反復前に未移動の所有値を解放します。借用を含む引数がある場合はフレーム再利用を行いません。

**WASM:** bulk-memory を有効化し、集約値のコピーにホストの memcpy を要求しません。
LLVM の閉形式ループ最適化は i64 プログラムにも i128 の乗算／除算を導入します。
同梱の乗算は i64／32-bit limb、除算は固定シフトと減算で実装し、
その補助自身が wide multiply/divide libcall に再変換される循環を避けます。
LLVM 23 は limb の乗算式を再び i128 乗算として認識するため、乗算補助だけは
`noinline optnone` で InstCombine から保護します。可変シフトの limb 補助も同様に保護します。
アプリケーション関数の最適化は維持します。
weak/hidden な補助はオブジェクト間で重複を許容し、未使用なら LLD が削除します。
WASM ヒープはアドレス順の空き領域リストを使い、分割・隣接領域の結合を行います。
memory.grow の失敗・16 MiB 上限超過ではトラップし、成功した形の無効ポインターを返しません。

**ホスト:** Tsuzuri 関数には外部関数をインポートしません。
コンソールの printf／putchar は生成したエントリー・ラッパーだけが持ちます。
公開 ABI は 64-bit 以下の整数／f32／f64／正規化した bool／void に限定します。
8／16-bit 整数のホスト ABI は 32-bit に正規化します。128-bit 値、ソフトウェア浮動小数点、
文字列・参照の所有権やホスト依存レイアウトを公開しません。
GUI、入力、永続化、非同期処理はホストの境界で扱います。

**出力:** 入力全体の検査後、出力先と同じファイルシステムの専用ディレクトリでビルドします。
全ツールが成功した後にだけ rename で成果物を公開します。
出力保護はルートだけでなく読み込んだ全ソースに適用し、Unix ではソースへのハードリンクも拒否します。
他のプラットフォームでも atomic replacement が
別のハードリンクの内容を書き換えることはありません。
出力ファイルを先に truncate しないでください。

## 開発と検証

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
node tests/e2e.mjs target/release/tsuzuri
node tests/primitives.mjs target/release/tsuzuri
node tests/examples.mjs target/release/tsuzuri
```

Rust のテストは LLVM なしで走ります。字句・型・失敗例・レイアウト・IR の不変条件・
2,000 パターンの決定的なソース変異を検査します。
所有権では正常な move／共有借用／排他借用に加え、move 後の使用、部分 move、
分岐・短絡評価、再借用、寿命切れ、オペランド評価中の参照先の無効化を検査します。
Node の E2E は本物の Clang／LLD、ネイティブ C ホスト、WebAssembly エンジンを使い、
JavaScript の BigInt／Number の参照結果と照合します。
最適化無効／有効の両方を実行し、百万回の末尾再帰、トラップ、
キャリーを含む全幅 128-bit 補助を検査します。外部ツール不足をスキップして成功扱いにはしません。
例の検証には HTTP 経由の WASM 読み込み、Python デスクトップ・ホストの headless 実行も含みます。
複数ファイルでは名前の分離、修飾された高階関数・レコード、相互再帰、
`Main.tzr` の選択、ファイルごとの診断、依存ソースの出力保護を検査します。
`tests/primitives.mjs` は decimal の結果を Python の IEEE 用 decimal コンテキストと照合し、
ネイティブの確保／解放を追跡して解放漏れ・二重解放を検出します。
WASM では累積の確保量がメモリ上限を超える反復を実行し、空き領域の再利用を確認します。

数値ランタイムの変更時は `src/runtime/numeric.c` を編集し、
`python3 src/runtime/generate.py` で `numeric.ll` を再生成します。
生成時だけ Clang を使い、通常の Cargo ビルド・型検査・LLVM IR 出力には LLVM のインストールを要求しません。
生成器はホストの target triple／データレイアウト／CPU 属性と新しい IR 限定の属性を除き、
対応する little-endian ネイティブ／wasm32 で同じ整数アルゴリズムを使います。
生成済み IR は直接手編集せず、C の変更と一緒に更新してください。

任意の実ブラウザー検証:

```sh
TSUZURI_BROWSER="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
  node tests/examples.mjs target/release/tsuzuri
```

専用の一時プロファイルで headless Chrome を起動し、読み込み成功と UI の有効化を確認します。
既存のブラウザー・プロファイルは使いません。

新機能では構文、型、LLVM、ホスト ABI、エラー、資料の関連面を同時に更新してください。
言語の値の意味を変える最適化は不可です。特に短絡評価、トラップ、符号付きゼロ、
overflow、評価順序を両ターゲットで確認します。
型付き IR に必要な不変条件は、コード生成時のキャストではなく型検査で保証します。

## 初版の次に必要な設計

動的コレクション、キャプチャ付きクロージャー、代数的データ型、ジェネリクス、
外部パッケージ、階層モジュール、効果の型付け、デバッグ情報、IDE／LSP は未実装です。
所有権は文字列と不変集約値を対象に実装していますが、名前付きライフタイム、
借用フィールド、再帰的なヒープ型、任意の destructor、ホストをまたぐ所有権は未対応です。
これらを追加するときも、寿命・ホスト境界・失敗モデルを型検査と一緒に設計する必要があります。
