# E13: ホスト言語バインディングと Web glue の生成

| 項目 | 内容 |
|---|---|
| ID | E13 |
| 優先度 | P2 |
| 規模 | L |
| 依存 | E05, E12, (F06), (F11) |
| 後続 | B08 Phase 2 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | C#/F# 比: .NET からの利用手段（[なぜ Tsuzuri か](../_docs/learn/why-tsuzuri.md#cf-に対する劣位点)）／追加: WASM ホストの手書き glue と、ブラウザー向け threads glue がない |
| 主な影響ファイル | `src/main.rs`, `src/driver.rs`, `src/llvm_abi.rs`, `src/bindings.rs`（新規）, `src/runtime/wasm-threads.mjs`, `examples/web/`, `docs/language.md`, `tests/bindings.mjs`（新規） |

## 目的

Rust の wasm-bindgen、Emscripten の glue、C# の P/Invoke 生成に相当する型付きバインディングを生成し、ホスト側の手書き変換コードをなくす。
F06 で対象外としたブラウザー向けの WASM threads の本番 glue もここで提供する。

## 現状

- `--emit header` は C header（`tz_*` の prototype、`tsuzuri_*_buffer`、レコードの typedef、`tsuzuri_alloc`／`tsuzuri_free`）を生成する。
- JavaScript 側は手書き: out descriptor（offset 0 の i32 pointer、offset 8 の i64 length、size 16）の読み取り、memory grow 後の view の再作成、BigInt／Number の変換、`tsuzuri_free` の呼び出し（docs/language.md 公開 ABI、`examples/web/simulation.mjs`）。
- threads は Node 用の `src/runtime/wasm-threads.mjs` だけで、「Browser本番glueは未実装です」（`examples/web/README.md`）。
- C#・C++ 向けの生成物はない。native の共有ライブラリ出力（`.so`／`.dylib`／`.dll`）はない。

## 仕様

### Phase 1: TypeScript／JavaScript（実装対象）

- `tsuzuri build ... --target wasm32 --emit bindings-ts -o dir/`（または `tsuzuri bind --lang ts`）で ESM の `name.mjs` と型定義 `name.d.ts` を生成する。
- `load(wasmBytes, imports?)` が instance を作り、各 export を型付き関数として返す:
  string は `string`、utf8string は `string`（TextEncoder/Decoder で厳密変換、不正は例外）、所有バッファ結果は `BigInt64Array`／`Float64Array`／`Uint8Array` へコピーして解放、
  i64 は `bigint`、スカラーレコードはオブジェクト。extern（E06/E12）の import も型付きで受け取る。
- 大きな配列入力のコピーを避ける `withBorrowed(array, callback)` 形式の API（呼び出し中だけ有効な view）を用意する。
- トラップは `WebAssembly.RuntimeError` をそのまま伝え、`--trap-info` の表があれば理由と位置を付けた `TsuzuriTrap` にする。

### Phase 2（設計方針）

- ブラウザーの threads glue: Worker 起動スクリプト、COOP/COEP の検査（不備は明示的な例外で逐次へ黙って切り替えない）、共有 memory の初期化。Node 用ホストと同じ protocol を使う。
- C#: `LibraryImport` による宣言、所有バッファの `SafeHandle`、借用入力の `ReadOnlySpan<T>`。native の共有ライブラリ出力（`--emit shared`）を併せて追加する。
- C++: C header の上の RAII ラッパー（`std::span`、`std::u16string_view`、`tsuzuri_free` を deleter にした `unique_ptr`）。

## 設計

- 公開 ABI の型情報（`src/abi.rs`、`src/llvm_abi.rs` の header 生成と同じモデル）から各言語のコードを生成する新しい `src/bindings.rs` を置く。ABI の意味は増やさない。
- 生成物は決定的にし、ヘッダーの先頭にコンパイラの版と ABI の版を記録する。
- 出力保護は既存の build と同じ規則（ソースの上書き・symlink を拒否）。

## 実装手順

1. **型モデルの共通化**: header 生成と同じ ABI モデルを取り出す。確認: 既存 header の出力が不変。
2. **TypeScript**: 生成と Node での実行。確認: `tests/bindings.mjs` で全 ABI 型の往復、`tsc --noEmit` による型検査（利用可能な場合）。
3. **トラップと借用 view**: 確認: memory grow 後の view 再作成、呼び出し後の view 無効化。
4. **Phase 2 の設計レビュー**: ブラウザー threads、C#、C++、共有ライブラリ出力の順序。

## テスト計画

- E2E: Node で WASM × `-O0`/`-O3` の全 export 型、確保追跡（`tsuzuri_free` の呼び忘れがない）、例外の型。
- ブラウザーは既存の headless Chrome 検査（`TSUZURI_BROWSER`）を再利用する。

## ドキュメント

- `_docs/guides/webassembly.md`、`_docs/guides/wasm-threads.md`、`_docs/guides/native-interop.md`、`docs/language.md` の公開 ABI、`examples/web/README.md`。

## 受け入れ条件

- [ ] 生成した TypeScript バインディングで、手書きの descriptor 操作なしに全 ABI 型を呼べる。
- [ ] 生成物が決定的で、既存の header 出力が変わらない。

## 落とし穴

- memory grow で古い `ArrayBuffer` の view が無効になる。生成コードは呼び出しごとに view を作り直す。
- i64 の符号付き・符号なしの解釈（`BigInt.asUintN`）を型ごとに正しく選ぶ。

## 対象外

- WebAssembly component model（WIT）の完全対応、GUI フレームワークとの統合。

## 未決事項

- **CLI の形**: 既定案は `--emit bindings-ts`。言語が増える場合は `tsuzuri bind --lang` に移す。
- **共有ライブラリ出力**: 既定案は C# 対応と同時に Phase 2 で追加し、G10 の Windows DLL 方針と揃える。
