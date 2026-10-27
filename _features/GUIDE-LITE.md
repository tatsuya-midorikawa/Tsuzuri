# GUIDE-LITE: 小さいモデル向けの抜粋

[GUIDE.md](GUIDE.md)（約 14 万字）の、実装担当が着手前に知っておく規則だけを集めた抜粋です。**正は GUIDE.md** で、食い違ったら GUIDE.md が優先です。
第3期のチケット（[D-50](GUIDE.md#d-50-第3期計画ai堅牢性性能27-チケットの起票)）は、この文書とそのチケットだけを読めば着手できるように書いてあります。第1・2期のチケットも、まずこの文書とチケットを読み、チケットが指定した GUIDE.md の節だけを足します。GUIDE.md のほかの節は読みません。

## 1. 読む順序

1. このファイル。
2. 対象チケットの「実装者向け早見」→「着手条件と停止条件」→「仕様」→「設計」→「実装手順」。
3. チケットが「前提とする他チケットのインターフェース」に挙げたチケットの、その節だけ。
4. チケットが指す [GUIDE.md](GUIDE.md) の D-n（決定台帳）と節。第3期の共通契約は D-51（診断）・D-52・D-60（Facts と KG）・D-53（効果と属性）・D-54（edition）・D-55（名前の割り当て）・D-56（AI 向け成果物）・D-57（権限）・D-58（規則 ID）。

## 2. 絶対に守る規則（GUIDE §1 の要約）

1. 言語の意味を変える最適化をしない（整数の折り返し・飽和・丸め・NaN・符号付きゼロ・評価順序（左から右）・短絡評価・トラップ・所有権・借用）。fast-math・再結合・暗黙の FMA・二重丸めは禁止。
2. 暗黙の変換・既定値・読み飛ばしをしない。未対応の構文や型は別の意味に置き換えず、コンパイルエラーにする。
3. 生成 IR は決定的にする（名前の集合は `BTreeMap`／`BTreeSet`。`HashMap` の反復順で出力を決めない）。intrinsic 宣言は重複させない。
4. WASM は既定でインポートなし。インポートが要る機能は明示的なオプトインにする。
5. 共有 CI に速度の合否の閾値を入れない。性能の主張は実測と生成コードの確認を伴う。実装済みと計画を区別して書く。
6. 新しい解析には資源上限と `E1017` を設ける（構文・式の深さ 128、特殊化 65,536 など。解析が最適化にすぎない場合は、上限超過で結果を捨てて保守的な経路へ戻してよい）。
7. 型付き IR の不変条件は型検査で保証する。コード生成時のキャストや `unreachable!()` で辻褄を合わせない。
8. `unsafe` は禁止（`Cargo.toml` の lint）。新しい crate を足さない（使えるのは `serde_json` と標準ライブラリだけ）。
9. 同じ PR で `_tsuzuri/language-reference/` の関連ページと `docs/language.md` を（動作が固まったところで一度）最新にし、`node scripts/check-docs.mjs <変更したページ>` を通す。README と `docs/architecture.md` は、書いてあることが変更で誤りになるときだけ直す。
10. テストの期待値は、独立した参照（JavaScript の BigInt・Python・C・手で書いた値）から作る。**コンパイラの現在の出力を期待値に写さない。** 通らないテストを、期待値の書き換えで通さない。
11. コミットは自分のブランチに段ごとに作ってよい（1 チケット 1 ブランチ・1 PR）。`main` にはコミットも push もしない（`AGENTS.md`）。

## 3. 作業環境と基準コマンド

- 必要: Rust 1.85 以降、Clang／LLD 17 以降、Node.js 20 以降（BigInt を多用する suite は Node 24）、Python 3.9 以降。macOS は `brew install llvm lld` の後、`export PATH="$(brew --prefix llvm)/bin:$(brew --prefix lld)/bin:$PATH"`。
- 着手前に全テストは回さない。基準は `main` の最新の `ci.yml` の結果（`gh run list --branch main --workflow ci.yml --limit 1`）で、`main` が通る検査を自分の変更で落としたら、それは自分の変更の影響。`main` が赤のときだけ、赤い検査を手元で再現し、自分の変更と無関係であることを記録してから着手する。
- 回す範囲は README の「検証と性能測定」の T0〜T3（編集のたび／手順の 1 段ごと／完了の報告の前／CI）に従う。「変更の種類 → テスト」の表も README にある。小さい範囲から実行し、最後に全体を実行する:

```sh
cargo test --locked --test <tests/ のファイル名から .rs を除いたもの>   # 1 ファイル
cargo test --locked <テスト関数名の一部>                                # 注意: 0 件でも成功する。出力の `running N tests` の N を必ず確かめる
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo nextest run --locked                                                # 全 Rust テスト（T1）。無ければ cargo test --locked
cargo build && node tests/features.mjs target/debug/tsuzuri <suite 名>    # E2E は直前にそのバイナリをビルドする。dev（opt-level 1）で足りる。release は性能や大きさを見る検査だけ
node scripts/check-docs.mjs _tsuzuri/language-reference/<変更したページ>.md
sh scripts/check-runtime-includes.sh                                      # src/runtime/ のファイルを足したとき
```

- 手元の動作確認は、リポジトリの外の一時ディレクトリで行う: `mkdir -p /tmp/tz-try && printf 'let x = 40\nx + 2\n' > /tmp/tz-try/Main.tz && target/debug/tsuzuri run /tmp/tz-try`。
- コード生成や数値に触れる変更は、native と WASM の `-O0`／`-O3` をすべて確かめる（`tests/features.mjs` が確保の追跡 `live == 0` と WASM の import なしも検査する）。

## 4. 一段ずつ進める

- チケットの手順は「変更 → 内容 → 確認」の順に**1 段ずつ**進める。複数段をまとめて書いてから一度にテストしない。段ごとに T1（`cargo nextest run --locked`。無ければ `cargo test --locked`）と、その段に書かれたテストを実行し、通った件数を控える（完了報告に使う）。
- 既存の似た機能（`NewLiteral`・`Task`・`ConstDef`・`W1006` など、チケットが「手本」に挙げたもの）を**必ず読み**、同じ形で書く。新しい抽象化を発明しない。
- `match` の `_ =>` が新しい variant を黙って無視する箇所が多い。新しい variant や field を足したら、近い既存の名前で全体を `grep`（例: `grep -rn "ExprKind::RecordUpdate" src/`）し、出てきた match をすべて確かめる（GUIDE §6.9）。
- 迷ったら、チケットの「決定事項」→ GUIDE の台帳 → 最も保守的な選択（拒否・何もしない）の順に従う。

## 5. テストの型

Rust のテスト（`tests/<機能>.rs`。LLVM 不要）:

```rust
use tsuzuri::{analyze, analyze_modules, llvm};
fn accepts(source: &str) {
    let module = analyze(source).unwrap_or_else(|e| panic!("{source}\n{}: {}", e.code, e.message));
    let ir = llvm::emit(&module, llvm::Entry::Library).unwrap();
    assert_eq!(ir, llvm::emit(&module, llvm::Entry::Library).unwrap()); // IR は決定的
}
fn rejects(source: &str, code: &str) {
    let e = analyze(source).expect_err(source);
    assert_eq!(e.code, code, "{source}\n{}", e.message);
}
```

- 複数ファイルは `analyze_modules(&[("Main.tz", main), ("Shapes.tz", shapes)])`。拒否の例は**診断コードを必ず照合**する（メッセージ全文は固定しない。固定してよいのは、チケットが「メッセージ」として決めたものだけ）。
- E2E は `tests/features.mjs` の suite として足す: fixture を `tests/fixtures/<suite>/`（入口 `Main.tz`、検査する関数は `export def`）に置き、`const suites = { ... }` に `cases`（`["<export 名>", [引数], 期待値]`。引数と期待値は BigInt）と `traps` を足す。
  一つだけ実行: `node tests/features.mjs target/release/tsuzuri <suite 名>`。
- CLI の出力（JSON・テキスト）を検査するテストは、`std::process::Command` で `env!("CARGO_BIN_EXE_tsuzuri")` を呼ぶ（手本: `tests/api_diff.rs`・`tests/diagnostics.rs`）。

## 6. 診断を足すとき（GUIDE §6.7 と D-51）

- 既存のコードで表せるなら既存のコードを使う。新しいコードは D-55 の表に割り当てられたものだけを使う。表にないコードが要るときは、**表に追記してから**使う。
- メッセージは英語・小文字始まり・末尾にピリオドなし・修正方法を含める（例: `an empty array needs a type annotation, for example '[i64]'`）。位置は問題の本体（名前・式）の `Span`。別ファイルのモジュールでも元ファイルの位置を保つ（`Span::in_source`）。
- 診断 v2 の追加キー（`kind`・`related`・`help`・`fixes`・`expected`／`found`・`candidates`・`scope`）は D-51 の形に従う。値のない診断の JSON は 1 byte も変えない。
- **診断を作る API は、実装する時点の HEAD で決まる。** `grep -n "pub fn of" src/diagnostic.rs` が 1 件なら G23 は済んでいて、`Diagnostic::new`・`warning` はもうない: 新しい診断は `Diagnostic::of(DiagnosticKind::…, …)` で作り、kind を先に G23 の登録簿（`diagnostic_kinds` モジュール）へ足す
  （手順は [G23](_completed/G23-diagnostic-kinds-explain-hints.md) の仕様 S1「新しい kind を足す手順」。**kind は診断を足すチケット自身が足す**）。0 件で `grep -n "pub fn with_kind" src/diagnostic.rs` が 1 件（G22 だけ）なら、チケットのとおり `Diagnostic::new(…).with_kind("…")` と書く（G23 があとで登録簿へ移す）。どちらもなければ kind を付けない（コード・位置・メッセージは同じ。D-51）。kind の文字列はどの場合も同じ。チケットが `with_kind` の形で書いていても、G23 のあとなら `of` の形に読み替える。
- `_tsuzuri/language-reference/compiler/diagnostics.md` と `docs/language.md` の「診断」の表を更新する。

## 7. 新しい構文・型・組み込み関数を足すとき

- 新しい予約語は足さない（文脈キーワードか属性にする）。字句・構文の変更は `Edition`（D-45・D-54）を引数に取る入口を通す。`src/syntax_codec.rs` の符号化と `FRONTEND_FORMAT`（AST の符号化を変えるたびに 1 上げる）を忘れない。
- 新しい組み込み関数は `Builtin::ALL`・`name`・`signature`・生成（`emit_builtin`）・効果の分類（A19・A20 の表）を全部更新する。
- std や組み込みに関数・モジュールを足すときは、操作する値（対象）を最後の引数にする（D-59）。新しい `std/` のファイルは `tests/std_argument_order.rs` の `MODULES` に分類する（足さないと赤になる）。資源のハンドルを先に取る例外は、理由を同じ表に書く。
- 新しい CLI オプションは `src/main.rs` の引数解析と、`rejects_ambiguous_or_unused_arguments` のテスト、`driver.rs` の `BuildOptions::validate`（使えない組み合わせは黙って無視せず `E2000`）、README の CLI 表と `_tsuzuri/language-reference/compiler/option.md` を更新する（G35 の `check-cli-docs` が `--help` の旗を両方に求める）。
- std に公開関数・モジュールを足すときは、G34 の Phase 3 が済んでいる木（`ls tests/std_examples.rs`）なら、足した公開関数の `///` に G34 の形式の例を付け、新しいモジュールをそのファイルの床の表（FLOORS）に足す。G35 の Phase 2 が済んでいる木（`ls tests/std_tags.rs`）で、完全にタグ付けしたモジュールの一覧（TAGGED）にあるモジュール（`Array`・`Vec`・`Map`・`Set`・`HashMap`・`HashSet`・`List`・`String`）へ公開関数を足すときは、`@complexity` も付ける（新しい関数には `@since` も）。足さないと、それぞれのテストが赤になる。
- 新しい `.ll` を足したら `.gitignore` の `!src/runtime/<name>.ll` と `scripts/check-runtime-includes.sh`。`numeric.ll`・`math.ll` は生成物で手で直さない。

## 8. 文書の更新（GUIDE §8）

- 文書は日本語（です・ます）、コード識別子と診断メッセージは英語。AI 向け成果物（`guide/`・`AGENTS.md` のひな形・`llms.txt`・brief カード）は英語（D-56）。
- 利用者向けの仕様は `docs/language.md`、内部の不変条件は `docs/architecture.md`、概要と実行例は `README.md`、性能は `docs/benchmarks.md`。食い違いを残さない。
- 機能の名前・キーワード・診断コード・CLI オプションで `_tsuzuri/language-reference` 全体を `grep` し、いちばん目立つページだけで終わらせない。計画中の注記・既知の不具合の警告・チケットへのリンクは、実際の振る舞いの説明に置き換える。
- 実装していない最適化や将来計画を、実装済みのように書かない。

## 9. 止めて報告する条件（GUIDE §13.1）と完了報告

次のどれかに当てはまったら、即興で回避せず、作業を止めて状況・原因の候補・選択肢を報告する。

- チケットの記述と HEAD のコードが食い違い、意図を保ったまま合わせる方法が一つに決まらない。
- `unsafe`・新しい crate・既定の WASM の import・意味を変える最適化が必要に見える。
- 既存のテストの期待値を変えないと通らない（チケットの「既存テストへの影響」に書かれたものを除く）。
- GUIDE §3.1 の回帰テスト（stack の深さ・特殊化の上限）が失敗する、または上限や stack の大きさを上げたくなった。
- 生成 IR が非決定的になった、または native と WASM、`-O0` と `-O3` で結果が違う。
- 同じ失敗を直す試みが 2 回続けて失敗した。

完了したら、チケットの末尾に `## 実装と検証（YYYY-MM-DD）` の節を足す: 実装した Phase と手順・実行したコマンドと結果（通過した件数）・チケットから外れた判断・更新した文書のページ・変更したファイルの一覧。

## 10. よくある落とし穴（GUIDE §11.1・§12 から）

- **stack の深さ**: テストのスレッドは既定で 2 MiB の stack で動く（dev profile は opt-level 1 で frame が小さくなったが、再帰の深さに注意する点は変わらない）。`syntax` の enum の variant を小さく保ち（大きい field は `Box`）、再帰する補助関数は入れ子の数を必ず増減する。
- **特殊化の予算**: std の generic 関数を先に特殊化すると利用者の予算を消費する。std に API を足したら `honors_the_exact_specialization_limit`（`src/polymorph.rs`）を実行する。
- **`cargo test <pattern>` は 0 件でも成功**。`running N tests` の N を確かめる。
- **Node の版**: BigInt を多用する suite は Node 20 で異常終了することがある。`npx --yes --package=node@24 node tests/<suite>.mjs target/release/tsuzuri`。
- **テストプロジェクトは 1 つずつ別の一時 root に置く**（ディレクトリ内のソースは再帰的に読み込まれる）。
- **整数の cast** は、整数から整数ならビットを保つ（切り捨て・符号拡張）。飽和するのは浮動小数点から整数だけ。
- **Tsuzuri の構文の落とし穴**: `pair.0` は不可（`match pair with | (a, _) -> a`）、`(5: i64)` は不可（`let x: i64 = 5`）、`Display.to_string` はなく `to_string`（消費）か `Display.display`（借用）、`new` は予約語、
  `for x in a .. b` は**両端を含む**（スライス `xs[a..b]` は半開）、複数行の条件は `if`／`elif`／`else`、明示ブロックの文の区切りは `;`、`abs` は f64 だけで整数は `Int.abs`、無名関数は `\x -> body`。
  定義は `def name :: Type = \args -> body`、再帰は `def rec`、可変な局所は `let mut`、借用は `ref`／`ref mut`／`deref`。

## 11. 第3期のチケットの共通の約束

- 全 Phase の設計が書いてあるが、**着手する Phase は人間の指示に従う**（指示がなければ Phase 1 だけ）。Phase はこの順に独立して実装・検証・マージできる。
- 各チケットの「決定事項」は承認済み（D-50）。A18 の D1〜D4 と全 Phase も、2026-10-23 に利用者が承認しました。edition の例外と現行契約の照合は README.md にあります。
- 前提のチケットが未実装なら、チケットの「前提とする他チケットのインターフェース」に書かれた名前を**そのまま**使って実装する（インターフェースは D-51〜D-53 に固定されている）。名前を変えたくなったら、止めて報告する。
- **共有の数値は現在の値から数える。** `FRONTEND_FORMAT`・`src/syntax_codec.rs` の tag と `("ExprKind", N)` の件数・`TrapKind` の個数と番号・`Builtin::ALL` の長さ・`RESERVED_MODULES` の件数・診断の kind の個数・テストの通過件数は、第3期の別チケットが先に変えているかもしれない。
  チケットに書かれた具体的な数（`4 → 5`・`16 → 17`・`running 11 tests` など）は**書いた時点の例**であり、着手時に現在の値を読んで（手順 1 のベースラインで控える）、そこから+1 する。食い違っても止まらない（現在の値を優先する）。
- 手順の「確認」にある通過件数は「手順 1 の件数 + このチケットが足したテストの数」の意味で読む。
