# E14: 埋め込み時のトラップ境界とスタック枯渇の報告

| 項目 | 内容 |
|---|---|
| ID | E14 |
| 優先度 | P1 |
| 規模 | L |
| 依存 | G04, E05, (B06) |
| 後続 | F13 Phase 2 |
| 状態 | todo |
| 起票 | 2026-09-29（第2期・比較劣位の改善。未レビュー） |
| 改善する劣位 | 追加（why-tsuzuri 未記載）: native ホストに組み込んだ関数のトラップでホストのプロセス全体が終了し、スタック枯渇は理由なしに異常終了する |
| 主な影響ファイル | `src/llvm_traps.rs`, `src/trap.rs`, `src/llvm.rs`, `src/llvm_abi.rs`, `src/runtime/`（新規 trap ランタイム）, `src/runtime/task.c`, `src/driver.rs`, `docs/language.md`, `tests/trap_locations.rs`, `tests/trap_boundary.mjs`（新規） |

## 目的

C/C++ のサーバーやデスクトップアプリへ Tsuzuri の計算を組み込んだとき、一回の呼び出しの失敗でホスト全体を失わない選択肢を用意する。
Rust の `catch_unwind`、C# の例外に相当する「呼び出し単位の失敗」をホスト API として返し、言語の意味（トラップは Result に変換しない）は変えない。
あわせて、スタック枯渇を理由付きのトラップとして報告する。

## 現状

- トラップは `@llvm.trap()`（`src/llvm_traps.rs`）。native では AArch64 の `brk`、x86 の `ud2` になり、プロセスはシグナルで終了する。
- `--trap-info` では `@tz.trap.report(site)` が native の stderr に理由を書き、`@tz.trap.latest` に ID を保存する（`tsuzuri_trap_site()`）。巻き戻し・setjmp/longjmp・signal handler はない。
- WASM ではホストが `WebAssembly.RuntimeError` を捕捉できるが、トラップ後の heap の整合と解放は保証しない。threads ではトラップ後の pool を再利用しない（F06）。
- スタック枯渇: native は OS のガードページで SIGSEGV となり理由を報告しない。WASM はエンジンの例外。検出機構はない（`sigaltstack`／`sigaction` の使用なし）。
- D-10: 例外・巻き戻しは導入しない。「失敗時のスタック巻き戻し・捕捉値の解放・兄弟タスクのキャンセルは保証しません」（docs/language.md タスク）。

## 仕様

### Phase 1: スタック枯渇の報告（実装対象）

- Tsuzuri が生成する native 実行ファイル（`build --emit exe`、`run`、`test`）だけ、runtime が代替シグナルスタックとハンドラーを登録する（POSIX は `sigaltstack` + SIGSEGV/SIGBUS、Windows は vectored exception handler）。
- 故障アドレスが主スレッド・worker のスタックのガード範囲なら `stack overflow` を stderr に書いて abort し、`tsuzuri run` は新しい TrapKind `StackOverflow` として `E2005` で報告する。
- ハンドラーは async-signal-safe な処理（`write` と `abort`）だけを行う。ガード範囲外の故障は既定の動作へ戻す。
- object・ライブラリ出力ではホストのシグナル設定を変更しない。

### Phase 2: 呼び出し単位のトラップ境界（opt-in、実装対象）

- `--trap-mode return`（native の object 出力）で、既存の `tz_name` に加えて `int32_t tz_try_name(tsuzuri_trap_info *trap, ...)` を生成する。0 は成功、非 0 はトラップ（理由と site ID を書く）。既存の `tz_name` の ABI・挙動は変えない。
- 実装: `tz_try_*` の入口で setjmp し、トラップ site は abort の代わりに thread-local の境界へ longjmp する。
- 呼び出し中の確保は thread-local の確保リストに登録し、トラップ時に一括解放する（drop は実行しない。入力は借用なので影響しない）。成功時はリストを破棄するだけ。
- `Task.parallel` の worker でのトラップは、B06 の失敗伝播と同じく group を失敗状態にし、全 worker の join 後に呼び出し元の境界へ戻る。worker 自身は longjmp で境界を越えない。
- extern のホスト関数のフレームを跨ぐ longjmp は起こらない（トラップは Tsuzuri のフレームでだけ発生する）。E12 Phase 2 のコールバックとは併用できない（`E2000`）。

## 設計

- trap の lowering を一か所（`llvm_traps.rs`）に集め、`--trap-mode` により `llvm.trap` と `tsuzuri_trap_raise(site)` を切り替える。成功経路には呼び出しを追加しない。
- 確保リストは `@tz.alloc`／`@tz.free` の境界モード版で管理し、既定モードの allocator は変更しない。
- シグナルハンドラーと境界は C ランタイム（`src/runtime/`）に置き、既存の runtime 連結条件で到達時だけ連結する。

## 実装手順

1. **スタック枯渇**: POSIX のハンドラー、主スレッドと worker のガード範囲。確認: 深い非末尾再帰で `E2005 stack overflow`、ASan とは別ジョブで検査。
2. **trap lowering の切り替え**: `--trap-mode return` の IR。確認: 既定の IR が不変。
3. **境界と確保リスト**: `tz_try_*`、一括解放。確認: C ホストから 1 万回トラップさせても RSS が増えず、成功呼び出しの結果が正しい。
4. **並列 group**: worker のトラップの伝播。確認: TSan、全 worker の join、pool の再利用。
5. **性能**: 境界モードの確保のオーバーヘッドを計測して記録する。

## テスト計画

- E2E: native × `-O0`/`-O3`、C ホスト（`tests/*_runtime.c` と同じ形式）で成功・トラップ・連続呼び出し、ASan/UBSan/TSan。
- WASM は変更しないことを import と IR の比較で確認する。

## ドキュメント

- `docs/language.md` のトラップ位置・公開 ABI、`_docs/tools/debugging.md`、`_docs/guides/native-interop.md`、GUIDE D-10 への補足（例外ではなくホスト API の失敗状態であること）。

## 受け入れ条件

- [ ] native 実行ファイルのスタック枯渇が理由付きで報告される。
- [ ] opt-in の境界モードで、トラップした呼び出しがホストへ失敗として戻り、メモリを回収して次の呼び出しを続けられる。
- [ ] 既定モードの IR・ABI・性能が変わらない。

## 落とし穴

- シグナルハンドラー内で malloc・stdio を使わない。
- worker スレッドのガード範囲は pthread の属性から求める。取得できない場合は報告せず既定動作に戻す。
- 確保リストに登録しないランタイム内部の確保（C runtime）があると漏れる。全確保経路を確認する。

## 対象外

- drop を実行する巻き戻し、言語内での失敗の捕捉、WASM のトラップ後の heap 再初期化。

## 未決事項

- **方式**: 既定案は setjmp/longjmp。LLVM の invoke／landingpad による巻き戻しは D-10 に反するため採用しない。
- **Windows**: 既定案は G10 の完了後に SEH で同じ境界を実装する。
