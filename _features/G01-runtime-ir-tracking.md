# G01: ランタイム IR ファイルの Git 追跡漏れの修正
| 項目 | 内容 |
|---|---|
| ID | G01 |
| 優先度 | P0 |
| 規模 | S |
| 依存 | なし |
| 後続 | 全チケット（新規 clone / fresh worktree が buildable であること） |
| 状態 | todo |
| 主な影響ファイル | `.gitignore`, `src/runtime/string.ll`, `src/runtime/heap-native.ll`, `src/runtime/heap-wasm.ll`, `src/runtime/console.ll`, `src/runtime/numeric.ll`, `src/runtime/generate.py`, `src/llvm.rs`, `src/driver.rs`, `tests/runtime_ir_tracking.rs` または CI スクリプト |

## 目的

新しい clone や fresh worktree で `cargo build` が失敗しないように、コンパイラが
`include_str!` で埋め込むランタイム LLVM IR ファイルを Git で追跡する。

このチケットは機能追加ではなく、開発環境の再現性修正である。G02 以降のすべての作業は、
ランタイム `.ll` が存在する作業ディレクトリを前提にしないこと。

## 現状

調査時点コミット: `19d8cdd`。

`.gitignore` は `*.ll` を無視し、現時点の例外は次だけである。

```gitignore
*.ll
!src/runtime/wasm.ll
!src/runtime/closure.ll
!src/runtime/task-wasm.ll
```

そのため次のファイルは作業ツリーには存在するが、Git 追跡対象ではない。

| ファイル | 役割 | 現在の ignore 理由 |
|---|---|---|
| `src/runtime/string.ll` | `@tz.string.copy/allocate/new/concat/equal` | `.gitignore:27:*.ll` |
| `src/runtime/heap-native.ll` | native の `@tz.alloc` / `@tz.free` | `.gitignore:27:*.ll` |
| `src/runtime/heap-wasm.ll` | WASM のヒープ allocator | `.gitignore:27:*.ll` |
| `src/runtime/console.ll` | console entry 用 `@tz.console.write` | `.gitignore:27:*.ll` |
| `src/runtime/numeric.ll` | `numeric.c` から生成する soft numeric runtime | `.gitignore:27:*.ll` |

確認コマンド（修正前の期待値）:

```sh
git --no-pager ls-files src/runtime
# src/runtime/closure.ll
# src/runtime/generate.py
# src/runtime/numeric.c
# src/runtime/task-wasm.ll
# src/runtime/task.c
# src/runtime/wasm.ll

git check-ignore --no-index -v \
  src/runtime/string.ll \
  src/runtime/heap-native.ll \
  src/runtime/heap-wasm.ll \
  src/runtime/console.ll \
  src/runtime/numeric.ll
# .gitignore:27:*.ll  src/runtime/string.ll
# .gitignore:27:*.ll  src/runtime/heap-native.ll
# .gitignore:27:*.ll  src/runtime/heap-wasm.ll
# .gitignore:27:*.ll  src/runtime/console.ll
# .gitignore:27:*.ll  src/runtime/numeric.ll
```

実コードでは、これらのファイルは存在を前提に `include_str!` される。

| 参照元 | 関数 / 箇所 | 埋め込むファイル |
|---|---|---|
| `src/llvm.rs` | `emit_target` | `runtime/task-wasm.ll`, `runtime/numeric.ll`, `runtime/closure.ll`, `runtime/string.ll`, `runtime/heap-wasm.ll`, `runtime/heap-native.ll` |
| `src/llvm.rs` | `Globals::default` | `runtime/numeric.ll`（metadata ID の開始値計算） |
| `src/llvm.rs` | `console_main` | `runtime/console.ll` |
| `src/driver.rs` | `build` | `runtime/wasm.ll`, `runtime/task.c` |

fresh clone では untracked な `.ll` が存在せず、Rust の `include_str!` がコンパイル時に
ファイル不在エラーを出す。

## 仕様

1. 既存の `*.ll` ignore 方針は維持する。
2. `src/runtime/` のうち、コンパイラが直接 `include_str!` するランタイム `.ll` だけを
   `.gitignore` の例外で追跡対象にする。
3. `src/runtime/numeric.ll` は引き続き手編集禁止で、`python3 src/runtime/generate.py` による生成物を
   そのままコミットする。
4. 追跡漏れが再発した場合、CI または専用テストで明示的に失敗させる。
5. 新しいランタイム `.ll` を追加する今後のチケットは、同じ例外行と追跡ガードを更新する。

## 設計

### `.gitignore` の正確な変更

`*.ll` の直後の例外ブロックを次のようにする。順序はファイル名の種類ごとにまとめ、
既存の `wasm.ll` / `closure.ll` / `task-wasm.ll` は残す。

```gitignore
*.ll
!src/runtime/wasm.ll
!src/runtime/closure.ll
!src/runtime/task-wasm.ll
!src/runtime/string.ll
!src/runtime/heap-native.ll
!src/runtime/heap-wasm.ll
!src/runtime/console.ll
!src/runtime/numeric.ll
```

既存ファイルを `git add -f` で強制追加するのではなく、`.gitignore` を直してから通常の
`git add` で追跡できる状態にする。

### 追加して追跡するファイル

次の 5 ファイルを Git に追加する。

```text
src/runtime/string.ll
src/runtime/heap-native.ll
src/runtime/heap-wasm.ll
src/runtime/console.ll
src/runtime/numeric.ll
```

すでに追跡されている次のファイルは変更しない。

```text
src/runtime/wasm.ll
src/runtime/closure.ll
src/runtime/task-wasm.ll
src/runtime/task.c
src/runtime/numeric.c
src/runtime/generate.py
```

### 退行防止ガード

追跡状態は Git の属性なので、通常の Rust unit test だけでは完全には検査できない。
このチケットでは CI 用の読み取り専用スクリプトを追加する方針にする。

推奨ファイル: `scripts/check-runtime-includes.sh`。

```sh
#!/usr/bin/env sh
set -eu

files=$(
  grep -hE 'include_str!\("runtime/[^"]+"\)' src/llvm.rs src/driver.rs |
    sed -E 's/.*include_str!\("runtime\/([^"]+)"\).*/src\/runtime\/\1/' |
    sort -u
)

for file in $files; do
  test -f "$file"
  git ls-files --error-unmatch "$file" >/dev/null
  if git check-ignore --no-index -q "$file"; then
    echo "tracked runtime include is ignored by .gitignore: $file" >&2
    exit 1
  fi
done
```

CI がまだ存在しない場合は、最小の代替として `tests/runtime_ir_tracking.rs` を追加し、
`include_str!("../src/runtime/...")` の存在だけを検査する。ただしこの Rust test は「存在」だけで、
「Git 追跡」を検査できないため、最終受け入れ条件は上記スクリプトを手元または CI で実行すること。

スクリプトは `src/llvm.rs` と `src/driver.rs` の `include_str!("runtime/...")` だけを対象にする。
`include_str!` のパス形式を変える場合は、スクリプトも同時に更新する。

### `numeric.ll` の再現性

`src/runtime/generate.py` は次の条件で `src/runtime/numeric.c` から `numeric.ll` を生成する。

| 項目 | 現状 |
|---|---|
| Python | `python3` |
| Clang | `${TSUZURI_CLANG:-clang}` |
| target | `--target=x86_64-unknown-linux-gnu` |
| C 標準 | `-std=c11` |
| 最適化 | `-O1` |
| freestanding | `-ffreestanding -fno-builtin -fno-stack-protector` |
| 正規化 | `source_filename`, `target`, `!llvm.*`, target attrs, new IR attrs などを削除 |

検証は byte-identical とし、生成後に `git diff --exit-code -- src/runtime/numeric.ll` が通ること。
Clang の新バージョンで新しい属性や IR 構文が増えた場合、`numeric.ll` を手編集せず、
`generate.py` の正規化規則を更新してから再生成する。

検証ログには次を記録する。

```sh
python3 --version
${TSUZURI_CLANG:-clang} --version
python3 src/runtime/generate.py
git --no-pager diff --exit-code -- src/runtime/numeric.ll
```

### 各コンパイラ段への影響

| 段 | 変更 |
|---|---|
| lexer | 変更なし |
| parser | 変更なし |
| check / polymorph / control / closures / ownership | 変更なし |
| llvm | ソース変更なし。`src/llvm.rs` の既存 `include_str!` が fresh clone で解決できるようにする |
| runtime | 既存 `.ll` ファイルを追跡対象にする。内容は生成物 / 既存実装のまま |
| driver | ソース変更なし。`src/driver.rs` の既存 `include_str!` が fresh clone で解決できるようにする |
| main | 変更なし |
| tests / CI | runtime include 追跡ガードを追加 |
| docs | README / architecture の「ランタイム IR は同梱済み」と検証手順を必要最小限で更新 |

## 実装手順

1. **現状を確認する。**
   - `git --no-pager ls-files src/runtime` を実行し、追跡済み `.ll` が
     `closure.ll`, `task-wasm.ll`, `wasm.ll` だけであることを確認する。
   - `git check-ignore --no-index -v src/runtime/string.ll ...` で 5 ファイルが `*.ll` により ignore されることを確認する。
   - 確認のみで、まだファイル内容は変更しない。

2. **`.gitignore` に例外を追加する。**
   - `*.ll` の直後に 5 行を追加する。
   - 既存の generated output 全般（例: `*.wasm`, `*.o`, `*.ll`）の ignore は維持する。
   - 確認:
     ```sh
     git check-ignore --no-index -v src/runtime/string.ll || true
     git check-ignore --no-index -v src/runtime/heap-native.ll || true
     git check-ignore --no-index -v src/runtime/heap-wasm.ll || true
     git check-ignore --no-index -v src/runtime/console.ll || true
     git check-ignore --no-index -v src/runtime/numeric.ll || true
     ```
     いずれも出力なしになること。

3. **5 つの `.ll` を追加する。**
   - 通常の `git add src/runtime/string.ll ...` が成功することを確認する。
   - `git --no-pager ls-files --error-unmatch <file>` が 5 ファイルすべてで成功すること。
   - `numeric.ll` は編集しない。

4. **退行防止ガードを追加する。**
   - CI スクリプトを採用する場合:
     - `scripts/check-runtime-includes.sh` を追加し、実行権限を付ける。
     - CI がある場合は `cargo test --locked` の前に実行する。
   - Rust test のみを採用する場合:
     - `tests/runtime_ir_tracking.rs` に `include_str!("../src/runtime/<name>.ll")` の存在検査を追加する。
     - 確認は `cargo test --locked --test runtime_ir_tracking` で実行し、GUIDE §3 の通り `--test <ファイル名>` を使う。
     - チケット末尾または PR 説明に「Git 追跡は手動コマンドで確認した」と明記する。
   - 推奨は CI スクリプト。追跡状態の検査に Git を使うことは、通常の unit test ではなく CI 入口の責務とする。

5. **`numeric.ll` の再生成再現性を確認する。**
   - fresh worktree または一時 worktree で実行する。
   - `python3 src/runtime/generate.py` の後、`git diff --exit-code -- src/runtime/numeric.ll` が通ること。
   - 差分が出た場合は、Clang のバージョン差か `generate.py` の正規化漏れを調べる。
   - 差分の内容を直接手で取り込まない。

6. **fresh worktree で検証する。**
   - `git worktree add ... HEAD` は `HEAD` に含まれるファイルだけを検証するため、post-commit または CI の checkout で行う。
     実装者の通常ワークフローが commit 前レビューなら、ここでは commit を要求せず、追跡状態の検査と通常 build を先に実施する。
   - post-commit / CI での例:
     ```sh
     git worktree add /tmp/tsuzuri-g01-fresh HEAD
     cd /tmp/tsuzuri-g01-fresh
     git --no-pager ls-files --error-unmatch \
       src/runtime/string.ll \
       src/runtime/heap-native.ll \
       src/runtime/heap-wasm.ll \
       src/runtime/console.ll \
       src/runtime/numeric.ll
     scripts/check-runtime-includes.sh
     cargo build --locked
     ```
   - 検証後に `git worktree remove /tmp/tsuzuri-g01-fresh` で掃除する。

## テスト計画

### 読み取り専用の事前確認

```sh
git --no-pager ls-files src/runtime
git check-ignore --no-index -v \
  src/runtime/string.ll \
  src/runtime/heap-native.ll \
  src/runtime/heap-wasm.ll \
  src/runtime/console.ll \
  src/runtime/numeric.ll
```

### 追跡状態の検査

```sh
git --no-pager ls-files --error-unmatch \
  src/runtime/string.ll \
  src/runtime/heap-native.ll \
  src/runtime/heap-wasm.ll \
  src/runtime/console.ll \
  src/runtime/numeric.ll

scripts/check-runtime-includes.sh
```

### 生成物再現性

```sh
python3 --version
${TSUZURI_CLANG:-clang} --version
python3 src/runtime/generate.py
git --no-pager diff --exit-code -- src/runtime/numeric.ll
```

### buildable fresh clone / worktree

post-commit または CI checkout で実行する。commit 前の通常確認では、この代わりに現在の作業ツリーで
`git --no-pager ls-files --error-unmatch ...` と `cargo build --locked` を実行する。

```sh
git worktree add /tmp/tsuzuri-g01-fresh HEAD
cd /tmp/tsuzuri-g01-fresh
cargo build --locked
```

このチケット自体は言語意味や LLVM 生成を変えないため、native/WASM × `-O0`/`-O3` の E2E は不要。
ただし `cargo build --locked` が fresh worktree で通ることを必須にする。

## ドキュメント

- `README.md` のビルド節に、同梱 runtime `.ll` は Git 管理される生成物であることを短く追記する。
- `docs/architecture.md` の数値ランタイム節に、`numeric.ll` は `generate.py` で再生成し、
  byte-identical であることを確認する手順を維持 / 補強する。
- `_features/GUIDE.md` はこのチケットの実装では変更しない。実装完了後、別途運用として
  「ランタイム `.ll` 追加時は `.gitignore` 例外とガード更新」を反映してよい。

## 受け入れ条件

- [ ] `.gitignore` が 5 つの runtime `.ll` 例外を持つ。
- [ ] `src/runtime/string.ll`, `heap-native.ll`, `heap-wasm.ll`, `console.ll`, `numeric.ll` が Git 追跡対象である。
- [ ] `git check-ignore --no-index -v` が上記 5 ファイルを ignore しない。
- [ ] `src/llvm.rs` と `src/driver.rs` の `include_str!("runtime/...")` 対象がすべて存在し、追跡されている。
- [ ] 退行防止ガードが CI または明示スクリプトとして追加されている。
- [ ] `python3 src/runtime/generate.py` 後に `src/runtime/numeric.ll` が byte-identical である。
- [ ] fresh worktree で `cargo build --locked` が通る。
- [ ] ランタイム IR の内容をこのチケットで意味変更していない。

## 落とし穴

- `git add -f` だけで済ませると、次の runtime `.ll` 追加時に同じ問題が再発する。
- Rust unit test は「ファイルがあること」は検査できるが、「Git 追跡」は検査できない。
- `numeric.ll` は 195 KB 程度あり、手編集の差分をレビューしづらい。必ず `numeric.c` と `generate.py` を基準にする。
- Clang のバージョン差で IR 属性が増えることがある。差分が出たら `generate.py` の正規化を更新する。
- `.gitignore` の例外は、親ディレクトリが ignore されていない前提で効く。今回は `src/runtime/` 自体は ignore されていない。
- fresh worktree の検証で、未コミットの untracked `.ll` が混ざらないようにする。

## 対象外

- ランタイム IR のアルゴリズム変更。
- `numeric.c` の数値実装変更。
- Clang / LLVM のバージョン固定導入。
- build script (`build.rs`) で runtime IR を生成する方式への変更。
- Cargo build 時に Python / Clang を必須にする変更。

## 未決事項

- **CI スクリプトの置き場所。** 既定案は `scripts/check-runtime-includes.sh`。既存 CI がない場合も、
  手元検証コマンドとして追加しておく。
- **`numeric.ll` の byte identity を保証する Clang 範囲。** 既定案は LLVM/Clang 17 以降を対象にし、
  新しい Clang で差分が出たら `generate.py` の正規化を更新する。
- **台帳の見直し提案:** なし。
