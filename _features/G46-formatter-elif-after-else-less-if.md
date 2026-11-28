# G46: `tsuzuri fmt` が、`if`・`elif` の枝の最後が `else` のない `if` で、その後に `elif` が続くファイルを `E2000` で拒否する

| 項目 | 内容 |
| --- | --- |
| ID | G46 |
| 優先度 | P2（正しいプログラムのファイルを 1 つも整形できなくなる。`fmt` はファイル単位で成否が決まるので、該当する形が 1 か所あるだけで `fmt`・`fmt --check` がそのファイルで失敗する。出力は書き換えないので、意味の変わったファイルが残ることはない。回避策はある: 内側の `if` に `else ()` を足す、内側の `if` の後に文を置く、外側の `elif` を `else` と入れ子の `if` に書き直す） |
| 規模 | S（`src/formatter.rs` の `else`・`elif` の持ち主の決め方、`tests/formatter.rs` の回帰テストとコーパス） |
| 依存 | – |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-28（[NC10](_completed/NC10-random-distributions-and-generators.md) の実装中に `std/Random.tz` の `binomial_btpe` で当たり、その PR（#153）では書き方を変えて避けた。利用者の依頼「作業中に見つけた既存の不具合 2 件は `_features/` にチケットを起票」で起票。`main` の `7252006b` で再現） |
| 承認 | 不要（整形器が、今 `E2000` で拒否している正しいファイルを整形できるようにする修正。受け入れているファイルの整形結果は 1 byte も変えない） |
| 改善する劣位 | 不具合: 構文も意味も正しいファイルに `tsuzuri fmt` が `error[E2000]: formatter would change program semantics; please report a bug` を出し、整形しない |
| 手本にする既存実装 | `src/formatter.rs` の `Canonical::expression`（`ExprKind::If`・`ExprKind::IfLet` が `Hints::else_owners`・`body_owners` を記録する所）、字下げを決める所の `let owner = if matches!(token.kind, TokenKind::Else \| TokenKind::Elif)`、`format_source_with_edition` の整形前後の AST の比較、`tests/formatter.rs` の `corpus_is_idempotent_and_semantically_unchanged` |
| 主な影響ファイル | `src/formatter.rs`, `tests/formatter.rs`, `std/Random.tz`（回避のために書き直した `binomial_btpe` を元の形に戻すかは D2）, `_features/README.md` |

## 実装者向け早見

- 範囲: `if`／`elif` の連鎖の枝の本体が、`else` を持たない `if`（または `if let`）で終わり、その枝の次が外側の `elif` である形。整形器はこの `elif` を内側の `if` のものとして字下げする（推定。「原因の候補」）ので、整形後の AST が変わり、整形前後の比較が `E2000` を出す。
- 最初に読む: `src/formatter.rs` の `format_source_with_edition`（比較）→ `Hints::else_owners` を書く 4 か所（`ExprKind::If`・`ExprKind::IfLet`・計算ブロックの `If`・`IfLet`）→ それを読む `TokenKind::Else | TokenKind::Elif` の所。
- 最初にすること: 比較の前の整形結果（`print_tokens` の戻り値）を「現状」の a で出して、外側の `elif` がどの字下げで出ているかを確かめる（原因の確認。今は確かめていない）。
- 最初に書くテスト: `tests/formatter.rs` に「現状」の表の a・b・d・e・i・k・m・n を足し、整形が成功して結果が入力と同じ（すでに整形済みの形）であることを確かめる。
- 触ってはいけない: 今受け入れているファイルの整形結果（`tests/formatter.rs` のコーパスと golden）。整形前後の AST の比較そのもの（安全網）を弱めない。
- 完了の見分け方: 「現状」の表の全行で `tsuzuri fmt --check` が終了コード 0 になり、コーパスの検査が通る。

## 目的

`tsuzuri fmt` は、整形の前後で AST が同じであることを確かめ、違えば何も書かずに `E2000` を出す。
この安全網のおかげで意味が変わることはないが、次の正しいプログラムは整形できない。

```tsuzuri
def f :: i64 -> i64
fn f x =
    let mut r = 0
    if x == 0 then
        r = 1
    elif x == 1 then
        r = 2
        if x > 0 then
            r = 3
            r = 4
    elif x == 2 then
        r = 5
    r

f 1
```

`tsuzuri run` はこのプログラムを受け入れて `4` を出す。`tsuzuri fmt` は次を出して終了コード 1 で終わる。

```text
Main.tz:1:1: error[E2000]: formatter would change program semantics; please report a bug
```

この形を、入力の字下げのまま（意味を変えずに）整形できるようにする。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief G46`。「現状」の表を HEAD で繰り返し、同じ結果になることを確かめる。食い違えば、このチケットを直してから始める。

### 停止条件

- **S1** 修正の後、`tests/formatter.rs` のコーパス（`corpus_is_idempotent_and_semantically_unchanged` が歩くディレクトリ）のどれかのファイルの整形結果が 1 byte でも変わる。
- **S2** 直すために、整形前後の AST の比較を外すか緩める必要があるように見える。
- **S3** GUIDE-LITE §9 の共通の条件。

## 現状（`main` の `7252006b` で確認）

`target/debug/tsuzuri fmt --check <dir>/Main.tz` で確かめた。作業は `target/scratch/bug-tickets/` で行い、確認後に消した。
どのプログラムも `tsuzuri check` は通る。本体は `def f :: i64 -> i64` / `fn f x =` / `let mut r = 0` / （表の `if` の連鎖）/ `r` で、行は ` / ` で区切り、字下げの深さを `>` の数で書く。

| # | `if` の連鎖 | 今の結果 |
| --- | --- | --- |
| a | `if x == 0 then` / `> r = 1` / `elif x == 1 then` / `> r = 2` / `> if x > 0 then` / `>> r = 3` / `>> r = 4` / `elif x == 2 then` / `> r = 5` | `E2000` |
| b | a の内側の `if` の本体を 1 文（`>> r = 3`）にした形 | `E2000` |
| i | a の内側の `if` を 1 行（`> if x > 0 then r = 3`）にした形 | `E2000` |
| e | a の `elif x == 1` の本体を内側の `if` だけにした形 | `E2000` |
| d | 内側の `if` が最初の `if` の枝の最後にあり、次が `elif` の形（`if x == 0 then` / `> r = 1` / `> if x > 0 then` / `>> r = 3` / `>> r = 4` / `elif x == 2 then` / `> r = 5`） | `E2000` |
| k | `if x == 0 then` / `> if x > 0 then` / `>> r = 3` / `elif x == 2 then` / `> r = 5` | `E2000` |
| m | k の後に `else` / `> r = 6` を足した形 | `E2000` |
| n | 内側が `if let Maybe.Some v = Maybe.Some x then` / `>> r = v` で、次が `elif` の形 | `E2000` |
| c | a の最後の `elif x == 2 then` を `else` にした形 | 成功 |
| j | `if x == 0 then` / `> if x > 0 then` / `>> r = 3` / `else` / `> r = 5` | 成功 |
| l | k の内側の `if` に `>> else` / `>>> r = 4` を足した形 | 成功 |
| g | a の内側の `if` の後に `> r = 6` を足した形（内側の `if` が枝の最後でない） | 成功 |
| h | a の内側の `if` を `while r < 5 do` / `>> r = r + 1` / `>> r = r + 1` にした形 | 成功 |
| o | 内側の `if` が `match` の腕の中にあり、その `match` が枝の最後の形 | 成功 |

まとめると、拒否されるのは「`if`／`elif` の枝の最後の文が `else` のない `if`（`if let` を含む）で、その枝の直後が外側の `elif`」の形だけである。
直後が外側の `else` なら成功する（c・j）。内側の `if` が `else` を持つ（l）、枝の最後でない（g）、別の構文に包まれている（o）場合も成功する。

`--json` の診断は `{"code":"E2000","kind":"formatter_changed_semantics","span":{"start":0,"end":0,...}}` で、位置はファイルの先頭である。どの `if` が原因かは診断から分からない。

### 原因の候補（確かめていない）

- `src/formatter.rs` の `format_source_with_edition` は、`print_tokens` の結果をもう一度構文解析し、`canonicalize` した整形前の AST と比べる。拒否の文はここで出る。構文エラーではない（`formatter produced invalid syntax` ではない）ので、整形結果は構文として正しく、`if` の連鎖の組み方だけが違うはずである。外側の `elif` が内側の `if` の字下げで出れば、ちょうどそうなる。
- `else`・`elif` の字下げは、`Hints::else_owners`（キーは `then` の枝の終わりの位置、値は「`else` の枝の始まりの位置」と持ち主の `if` の始まり）を、トークンの位置以下で最後のキーから引いて決める。`else` のない内側の `if` も `else_owners` に項目を入れる（`else` の枝は構文解析が補う `unit`）。内側の `if` が枝の最後のとき、外側の `then` の枝の終わりと内側の `then` の枝の終わりは同じ位置になりうる。同じキーなら後から入れたほうが残る。
- 外側の次が `else` のときに成功する理由は、この説明だけでは決まらない。手順 1 で、`elif` と `else` の両方について、キーと値を実際に出して確かめる。

## 仕様

1. 「現状」の表の a・b・d・e・i・k・m・n を、`tsuzuri fmt` が成功として整形する。整形結果は、外側の `elif` を外側の `if` と同じ字下げに置く（表のプログラムは、すでにその形なので変更なしになる）。
2. 整形前後の AST の比較は今のまま残す。比較が失敗したら、今と同じ `E2000`（kind `formatter_changed_semantics`）を出す。
3. 今成功しているファイルの整形結果は変えない。
4. `E2000` の位置をファイルの先頭から原因の構文へ移すことは、このチケットではしない（「対象外」）。

## 設計

- `else`・`elif` のトークンの持ち主を、「トークンの位置以下で最後のキー」ではなく、「`then` の枝の終わりがトークンより前にあり、`else` の枝の始まりがトークン以後にある項目のうち、いちばん外側ではなく、そのトークンを実際に `else` として持つ `if`」で決める。構文解析が補った `unit` の `else` の枝を持つ `if`（`else` のない `if`）は、`else_owners` に入れないのが最も単純である（その `if` には字下げを決める `else`・`elif` のトークンがない）。
- `else` のない `if` かどうかは、`canonicalize` の時点の AST で分かる必要がある。構文解析が補った `unit` を見分ける方法（幅 0 の span、専用の印など）は、手順 1 で `src/parser.rs` の `if` の解析を読んで決める。見分けられなければ、キーの衝突を避ける形（値を、持ち主の `if` の始まりが最も小さいものに保つ、など）にする。
- 計算ブロックの `If(condition, yes, Some(no))` は、`else` があるときだけ項目を入れている（`Some(no)`）。式の `If` を同じ形にそろえる。

## 実装手順

1. **変更**: なし（調査）。**内容**: a と c について、比較の前の整形結果と、`else_owners` のキー・値、`elif`／`else` のトークンの位置を出して、「原因の候補」を確かめる。違っていれば、このチケットの「原因の候補」と「設計」を直してから進む。**確認**: 整形結果の a の `elif` の字下げ。
2. **変更**: `tests/formatter.rs`。**内容**: 「現状」の表の 14 個を足す（8 個は今は失敗する）。**確認**: `cargo test --locked --test formatter`（8 個が赤）。
3. **変更**: `src/formatter.rs`。**内容**: 「設計」の直し。**確認**: T0、`cargo test --locked --test formatter`（全部緑。コーパスの検査を含む）、T1。
4. **変更**: `std/Random.tz`（D2 で戻すと決めたとき）、文書。**内容**: `binomial_btpe` を元の形に戻すなら、`tests/random_distributions.rs` と `features.mjs random` で出力が変わらないことを確かめる。`docs/language.md` と Tsuzuri-Reference（`docs/compiler/cli.md` の `fmt`）に、この形についての記述があれば直す（今は無い）。**確認**: T2 の `gen-docs --check`・`check-cli-docs`。

## テスト計画

- `tests/formatter.rs`: 「現状」の表の 14 個（成功・変更なし）。`if let` の形（n）と、計算ブロックの中の同じ形（`IO { ... }` の中の `if` の連鎖）を 1 個ずつ。字下げを崩した入力（外側の `elif` を内側の深さに置いた別のプログラム）は、今と同じく内側の `if` の `elif` として整形されること。
- コーパス: `corpus_is_idempotent_and_semantically_unchanged` が通り、整形結果が修正の前後で同じであること（S1）。

## 受け入れ条件

- 「現状」の表の全行で `tsuzuri fmt --check` が終了コード 0。
- `tests/formatter.rs` のコーパスの整形結果が修正の前後で 1 byte も変わらない。
- T1 と、T2 の `gen-docs --check`・`check-cli-docs` が通る。

## 対象外

- `E2000`（`formatter_changed_semantics`）の位置を、原因の構文に付けること。
- `else`・`elif` 以外のトークンの字下げの規則。

## 決定事項

- **D1（既定案）**: 整形前後の AST の比較は残し、字下げの決め方だけを直す。
- **D2（既定案）**: `std/Random.tz` の `binomial_btpe` は、NC10 が回避のために書き直した形（`check = 条件` の代入）のままにする。戻すと乱数の列の検査を全部やり直すことになり、得るものがない。回帰テストは `tests/formatter.rs` の最小の形で持つ。
