# M13: デスクトップの OS サービスの公式パッケージ（クリップボード・ファイルダイアログ・通知・設定・バンドルの資源。std に入れない）

| 項目 | 内容 |
| --- | --- |
| ID | M13 |
| 優先度 | P3（今は `Process.run` で `pbcopy`・`osascript`・`defaults` を呼ぶか、ObjC の実行時の C の API を ABI の知識で呼べば書ける（現状 3）。M07 の `apple` パッケージが出れば、利用者は生成した宣言で直接書ける。このチケットが減らすのは、書く量と誤りの入る余地である） |
| 規模 | XL（Phase 1〜3。人間が求めない限り Phase 1 だけを実装する） |
| 依存 | [M07](M07-apple-sdk-bindgen-and-package.md), ([M15](M15-macos-app-bundle.md)), ([M04](M04-objc-blocks.md)), ([M09](M09-main-thread-dispatch-and-handles.md))（M15 は Phase 2 の通知の bundle の識別子） |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-26（D-67。利用者の依頼「「どのチケットにもない壁」と「調査中に見つけたコンパイラの不具合（自分で再現済み）」のそれぞれについて @_features/ に開発チケットまたは修正チケットを作成して」から起票。調査で報告した「壁 5（std にクリップボード・ダイアログ・通知・設定・バンドルの資源がなく、チケットもない）」に当たる。HEAD `804b7970` で確認） |
| 承認 | [GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22) の分岐 5（公式パッケージの配布の形）と分岐 1（X′）は承認済み（2026-11-26）。W27 の D3・D9 は W27 自身の決定として残る。分岐 1 との関係は決定事項 D10 に写した（承認済み）。要承認: 置き場（`apple` のリポジトリ）、パッケージの API の名前（モジュールの道筋、型と関数の名前、直接の関数にする形）と、GUI フレームワークを同梱しないという非目標との関係の記述。分岐 5 と置き場は決定事項 D1（分岐 5 は承認済み、置き場と名前は要承認）、API の名前と形は D4、非目標の記述は D8 に写した（どれも要承認）。Phase 2 の名前は Phase 2 の着手の前に別に承認する（D9） |
| 改善する劣位 | Swift（AppKit・Foundation を直接）・Electron と Tauri（`clipboard`・`dialog`・`Notification`・設定の保存の API）・Rust（`arboard`・`rfd`・`directories`）比: クリップボード、ファイルを選ぶダイアログと保存のダイアログ、通知、設定の保存、バンドルの資源のパスを、型付きで失敗を値で返す API で書けない。今は外のコマンドを `Process.run` で呼ぶか、`objc_msgSend` を ABI の知識で呼ぶしかない |
| 手本にする既存実装 | [W27](W27-win32-package-from-winmd.md) の Phase 2（std の外の公式の git パッケージ、出どころと利用条件を書いた README、パッケージのリポジトリの CI）、[M07](M07-apple-sdk-bindgen-and-package.md) の Phase 2（同じリポジトリの `apple` パッケージ、`namespace = "Apple"`、核の一覧）、`std/Net.tz`（私的な直接の関数 `*_with` と公開の関数の分け方、`error_kind`）、`std/Process.tz` の `join_arguments`（引数の検査を `Result` で返す形）、`std/ZString.tz`（文字列を外の表現に変える関数の形）、言語リファレンスの std のモジュールの「引数の順序」の節（`_tsuzuri/language-reference/built-in-types-and-modules/anchor.md` の 27〜30 行） |
| 主な影響ファイル | このリポジトリ: `_tsuzuri/language-reference/compiler/native-interop.md`（632 行の非目標の段落の後に、公式パッケージへの案内を足す）、`_tsuzuri/language-reference/compiler/objective-c.md`（M01 が作るページ。あれば同じ案内）、`docs/language.md`（同じ案内の 1 文）。コンパイラ・std・テストのコードは変えない。パッケージのリポジトリ（M07 の Phase 2 の `apple`。このリポジトリの外なので、パスは repo-kb が展開しないよう引用符なしで書く）: Desktop.tz と Desktop/ の Clipboard.tz・FileDialog.tz・Preferences.tz・Resources.tz（新規）、api.txt（核の一覧に宣言を足す）、tests/desktop.mjs と tests/desktop/（新規）、README。Phase 2: Desktop/Notifications.tz |

## 実装者向け早見

- 範囲（Phase 1）: M07 の Phase 2 が作る公式の git パッケージ `apple` のリポジトリに、Tsuzuri で書いた 4 つのモジュール `Apple::Desktop::Clipboard`・`Apple::Desktop::FileDialog`・`Apple::Desktop::Preferences`・`Apple::Desktop::Resources` と、共通の型のモジュール `Apple::Desktop`（`Desktop.Error`）を足す。中身は、M07 が生成した `extern class` の宣言（M01・M02）を呼ぶ薄い包みである。コンパイラ・std・このリポジトリのコードは変えない。このリポジトリで変えるのは、公式パッケージへの案内の文書だけ。
- **人間が求めない限り Phase 1 だけを実装する。** Phase 2 は通知（UserNotifications。bundle と許可が要る）、Phase 3 は Windows・Linux の版を同じ API で足すかの判断である。
- 最初に読む: ① 現状 3 の再現（今の回避策で 4 つのサービスがすべて動く）と現状 4 の事実 ② 仕様の Phase 1 ③ M07 の仕様「Phase 2: 公式の `apple` パッケージ」と「他チケットへ提供するインターフェース」 ④ M01・M02 の所有の規則（`ref C` は +0、族の外の結果は +0 を retain）と M07 の D3（スレッドの性質の写し方）。
- 最初に書くテスト: パッケージのリポジトリの `Desktop/Clipboard.tz` の `test "a unique board keeps a string"`。`Clipboard.unique ()` の板に `"こんにちは 🍵"` を書いて読み戻す。利用者の general の pasteboard には触れない。
- 触ってはいけない: std（モジュールを足さず、`RESERVED_MODULES` の 78 個を変えない）、コンパイラ、M07 が生成したファイル（`MARKER` で始まるもの）、利用者の general の pasteboard と既定の defaults の領域（テストから書かない）。
- 完了の見分け方（Phase 1）: `apple` を git の依存にしたプロジェクトで、次の `Main.tz` が `check` を通り、`build` した実行ファイルが終了コード 0 で終わる。今のコンパイラでは `Apple::Desktop` がないので、`check` が `error[E1011]: using 'Apple::Desktop' names no namespace with modules` になる（計画の API。HEAD の開発版で確かめた）。

```tsuzuri
namespace App

using Apple::Desktop

def main :: unit -> i32 = \() ->
    let board = Clipboard.unique ()
    match Clipboard.write_string (ref "こんにちは 🍵") (ref board) with
    | Result.Error _ -> 1
    | Result.Ok _ ->
        match Clipboard.read_string (ref board) with
        | Result.Ok (Maybe.Some text) -> if text == "こんにちは 🍵" then 0 else 2
        | _ -> 3
```

```sh
target/debug/tsuzuri fetch app
target/debug/tsuzuri build app -o app/out/app
perl -e 'alarm 15; exec @ARGV' app/out/app; echo $?
```

## 目的

デスクトップのアプリは、ウィンドウとメニューのほかに、OS のいくつかのサービスを必ず使う。コピーと貼り付け、開くファイルと保存先の選択、設定の保存、アプリに同梱した資源の場所、通知である。Swift は AppKit と Foundation を直接呼び、Electron と Tauri はこれらを JS の API として揃えている。

Tsuzuri の std には、どれもない。M01〜M07 がそろえば、利用者は AppKit の宣言を型付きで呼べる。それでも、文字列の受け渡し（`NSString` と `string`）、nil の扱い、メインスレッドの規律、ダイアログの結果の読み方は、アプリごとに同じ手順を書き直すことになる。

このチケットは、その手順を 1 回だけ書いた薄い包みを、std の外の公式パッケージとして用意する。std に入れない理由は 3 つある。

1. GUI フレームワークを同梱しないという非目標（`_features/README.md:117`、native-interop.md:632）と、それを改めない D-67 の決定（「改めないもの」）。このパッケージはウィジェット・レイアウト・描画を持たないが、それでも OS の UI の部品（ダイアログ）を含むので、std の外に置く。
2. std はコンパイラに埋め込まれ、すべての OS に配られる。macOS だけのモジュールは、ほかの OS で使えない名前を std に増やす。
3. 包みが呼ぶ宣言は、M07 が SDK から生成する `apple` パッケージにある。std はパッケージに依存できない。

TLS を std に入れず公式パッケージにする E09 の D13 と同じ形である。

## 着手条件と停止条件

### 着手条件

- **人間が求めない限り Phase 1 だけを実装する**（GUIDE §0）。
- **承認が記録されるまで、該当する Phase に着手しない**（GUIDE §0 の 1）。D1（置き場と名前。分岐 5 は承認済み）が承認されるまでチケット全体に着手しない（D10 の分岐 1 は承認済み（2026-11-26））。D4（API の名前と形）が承認されるまで手順 2 に着手しない。D8（非目標との関係の記述）は手順 7 の文書の前に承認を受ける。Phase 2 は D9、Phase 3 は D3 の承認の後。
- Phase 1: M07 の Phase 2 が done（`apple` パッケージのリポジトリと macOS のランナーの CI がある）。そのために M01・M02 の Phase 1・M06 の Phase 1・M20 の Phase 2・W17 も done である。確認: `grep -nE "^\| (M01|M02|M06|M07|M20|W17) " _features/README.md` の状態の列。
- M09 は弱い依存。M09 の Phase 2 が done なら、M07 は `NSOpenPanel`・`NSSavePanel` を `extern class main` に写し、FileDialog の呼び出しの場所はコンパイル時に限られる。done でなければ、FileDialog は実行時に `pthread_main_np` を確かめる（仕様「Phase 1: スレッド」）。M09 の Phase 1 があれば、その検査は `MainThread.is_current ()` で書く。
- Phase 2: M04 の Phase 1 と M15 が done。M09 の Phase 1 が done（完了ハンドラーの結果を main へ戻す `MainThread.post`）。
- Phase 3: D3 の承認（W27 の `win32` パッケージの配布の形が決まった後）。
- 手元の macOS（arm64）と、Xcode か Command Line Tools の SDK。

### 停止条件

- **S1** Phase 1 が要る宣言（`NSPasteboardTypeString` のデータシンボル、`NSURL` の `path`、`UTType` の `typeWithFilenameExtension:` など）を、M07 の生成器の一覧に足しても生成できない。包みの中に `objc_msgSend` の手書きの宣言（現状 3 の形）を置かず、止めて報告する。
- **S2** コンパイラの変更（新しい構文・組み込み関数・診断）が要る。このチケットの範囲を出るので、止めて該当するチケットを報告する。
- **S3** `Desktop` のモジュールを足した後、`apple` を依存にした hello の `check` の時間が目に見えて伸びる（依存のモジュールはすべて型検査される。W27 の現状）。計測の値を書いて報告する（D1 の別案の別リポジトリに移すかを人間が決める。CI に速度の閾値は入れない）。
- **S4** パッケージのリポジトリの CI の macOS のランナーで、headless の E2E（現状 3 の 2・3 の形）が動かない（ウィンドウサーバーがない、パネルのサービスが起動しない）。検査を手元だけにして通ったことにせず、報告する。
- **S5** 分岐 5 が否決される（公式パッケージにしない）か、分岐 1 が Y′・Z′ になる。D10 のとおり止める。分岐 5 は「認める」、分岐 1 は X′ に決まった（2026-11-26）ので、今は当たらない。
- **S6** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `804b7970` で確認）

### 1. std にも台帳にもない

- `std/` の 70 個のファイル（`git ls-tree --name-only 804b7970 std/`）に、クリップボード・ダイアログ・通知・設定・バンドルを扱うモジュールはない。組み込みの予約名は 78 個（`src/stdlib.rs:82` の `RESERVED_MODULES`、903 行の検査）。
- 未完了と完了のチケットのどちらにも、macOS の OS サービスを扱うものはない。次の 2 つは何も出力しない。

```sh
git grep -n -i -E 'clipboard|NSPasteboard|NSUserDefaults|UNUserNotification|NSOpenPanel|クリップボード' 804b7970 -- _features
git grep -n -i -E 'clipboard|pasteboard|クリップボード|NSBundle|UserDefaults' 804b7970 -- std
```

- ファイルダイアログが出てくるのは、Windows の COM の `IFileOpenDialog` を扱う W25〜W28 だけである（`git grep -c -i -E 'file dialog|ファイルダイアログ|IFileOpenDialog' 804b7970 -- '_features/*.md'`）。

### 2. 方針

- `_features/README.md:117`（「採用しない改善」）: 「GUI・Web フレームワークの同梱 | ホストの責務。E13 の生成 glue で既存のフレームワークと接続する」。
- native-interop.md:632: 「GUI は言語の一部ではありません。デスクトップの画面は、ホストがオブジェクトを呼ぶ側に置きます。」
- [GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22) の「改めないもの」（この起票で足した。D-67 とともに 2026-11-26 に承認済み。HEAD `804b7970` にはまだない）: AppKit・Foundation の宣言は std に入れず公式パッケージにする（M07。分岐 5）。デスクトップの OS サービス（M13）と TLS・HTTP（M14）も std に入れない。
- 前例: E09 の D13（`_features/_completed/E09-network.md:165`）は、HTTP と TLS を std に入れず公式パッケージにすると決めた。

### 3. 今の回避策

1. **外のコマンド**: `Process.run "pbcopy" [] bytes`・`Process.run "pbpaste" [] []`、`Process.run "osascript" ["-e", "choose file"] []`、`Process.run "defaults" ["write", …] []`。権限 `process` が要り、呼ぶたびにプロセスを起動する。結果は標準出力のバイト列で、型も失敗の区別もない。`Process.run` は終わるまで待つ形だけである（process.md の公開 API の表、82 行）。
2. **ObjC の実行時の C の API（C を書かない）**: 次のソースは、クリップボード（使い捨ての pasteboard）、設定（使い捨ての suite）、バンドルの資源、開くパネルの構成を、今のコンパイラだけで動かす。`objc_msgSend` の宣言は、W14 の同一形の規則を避けるために、すべて整数 9 個と `f64` 4 個の上位形にそろえてある（D-67 の経緯に書いた回避策）。general の pasteboard と既定の defaults には触れない。

```tsuzuri
// M13 probe: the OS services of a desktop app through the Objective-C runtime C API, with no C shim.
// Every objc_msgSend declaration has one C shape (W14): x0..x7 and one stack slot as 64-bit
// integers or pointers, then d0..d3 as f64. It touches no shared state of the machine: a unique
// pasteboard (released at the end), a unique defaults suite (removed at the end), and a panel that
// is never shown.

@trusted
@count(1, none)
extern "objc_getClass" def objc_get_class :: ref utf8string -> i64

@trusted
@count(1, none)
extern "sel_registerName" def sel_register_name :: ref utf8string -> i64

@trusted
extern "objc_msgSend" def send :: i64 -> i64 -> i64 -> i64 -> i64 -> i64 -> i64 -> i64 -> i64 -> f64 -> f64 -> f64 -> f64 -> i64

@trusted
@count(3, none)
extern "objc_msgSend" def send_cstr :: i64 -> i64 -> ref utf8string -> i64 -> i64 -> i64 -> i64 -> i64 -> i64 -> f64 -> f64 -> f64 -> f64 -> i64

@trusted
@count(3, none)
extern "objc_msgSend" def send_buf :: i64 -> i64 -> ref mut [ubyte] -> i64 -> i64 -> i64 -> i64 -> i64 -> i64 -> f64 -> f64 -> f64 -> f64 -> i64

@trusted
extern "pthread_main_np" def pthread_main_np :: unit -> i32

@trusted
extern "getpid" def getpid :: unit -> i32

def cls :: ref utf8string -> i64 = \name -> objc_get_class name

def sel :: ref utf8string -> i64 = \name -> sel_register_name name

def send0 :: i64 -> i64 -> i64 = \obj s -> send obj s 0 0 0 0 0 0 0 0.0 0.0 0.0 0.0

def send1 :: i64 -> i64 -> i64 -> i64 = \obj s a -> send obj s a 0 0 0 0 0 0 0.0 0.0 0.0 0.0

def send2 :: i64 -> i64 -> i64 -> i64 -> i64 = \obj s a b -> send obj s a b 0 0 0 0 0 0.0 0.0 0.0 0.0

def ns_string :: ref utf8string -> i64 = \text ->
    send_cstr (cls (ref u8"NSString\0")) (sel (ref u8"stringWithUTF8String:\0")) text 0 0 0 0 0 0 0.0 0.0 0.0 0.0

// NSString -> utf8string: -lengthOfBytesUsingEncoding: and -getCString:maxLength:encoding: (UTF-8 = 4).
def from_ns_string :: i64 -> utf8string = \s ->
    if s == 0 then u8"<nil>" else
        let n = send1 s (sel (ref u8"lengthOfBytesUsingEncoding:\0")) 4
        let mut buf: [ubyte] = new [ubyte](n + 1, \_ -> 0ubyte)
        let ok = send_buf s (sel (ref u8"getCString:maxLength:encoding:\0")) (ref mut buf) (n + 1) 4 0 0 0 0 0.0 0.0 0.0 0.0
        let bytes: [ubyte] = new [ubyte](n, \i -> buf[i])
        if (ok % 256) == 0 then u8"<getCString failed>" else
            match Utf8String.from_bytes bytes with
            | Maybe.Some text -> text
            | Maybe.None -> u8"<invalid utf8>"

def main :: unit -> i32 = \() ->
    // Clipboard: a unique pasteboard, so the general pasteboard of the user is not touched.
    let pb = send0 (cls (ref u8"NSPasteboard\0")) (sel (ref u8"pasteboardWithUniqueName\0"))
    let _ = send0 pb (sel (ref u8"clearContents\0"))
    // The value of the data symbol NSPasteboardTypeString, written as a literal.
    let utf8_type = ns_string (ref u8"public.utf8-plain-text\0")
    let wrote = send2 pb (sel (ref u8"setString:forType:\0")) (ns_string (ref u8"こんにちは 🍵\0")) utf8_type
    let back = from_ns_string (send1 pb (sel (ref u8"stringForType:\0")) utf8_type)
    let _ = send0 pb (sel (ref u8"releaseGlobally\0"))
    do! IO.writeln $"clipboard: wrote={wrote % 256} back={back}"
    // Preferences: a unique suite, removed at the end.
    let suite_name = Utf8String.from_string $"dev.tsuzuri.m13-probe.{getpid ()}\u{0}"
    let suite = ns_string (ref suite_name)
    let defaults = send1 (send0 (cls (ref u8"NSUserDefaults\0")) (sel (ref u8"alloc\0"))) (sel (ref u8"initWithSuiteName:\0")) suite
    let key_s = ns_string (ref u8"greeting\0")
    let key_i = ns_string (ref u8"count\0")
    let key_b = ns_string (ref u8"enabled\0")
    let _ = send2 defaults (sel (ref u8"setObject:forKey:\0")) (ns_string (ref u8"hello\0")) key_s
    let _ = send2 defaults (sel (ref u8"setInteger:forKey:\0")) 42 key_i
    let _ = send2 defaults (sel (ref u8"setBool:forKey:\0")) 1 key_b
    let got_s = from_ns_string (send1 defaults (sel (ref u8"stringForKey:\0")) key_s)
    let got_i = send1 defaults (sel (ref u8"integerForKey:\0")) key_i
    let got_b = send1 defaults (sel (ref u8"boolForKey:\0")) key_b % 256
    let missing = from_ns_string (send1 defaults (sel (ref u8"stringForKey:\0")) (ns_string (ref u8"absent\0")))
    let _ = send1 defaults (sel (ref u8"removePersistentDomainForName:\0")) suite
    do! IO.writeln $"defaults: string={got_s} integer={got_i} bool={got_b} absent={missing}"
    // Resources: the main bundle of a plain executable is the directory of the executable.
    let bundle = send0 (cls (ref u8"NSBundle\0")) (sel (ref u8"mainBundle\0"))
    let ident = from_ns_string (send0 bundle (sel (ref u8"bundleIdentifier\0")))
    let found = from_ns_string (send2 bundle (sel (ref u8"pathForResource:ofType:\0")) (ns_string (ref u8"data\0")) (ns_string (ref u8"txt\0")))
    let absent = from_ns_string (send2 bundle (sel (ref u8"pathForResource:ofType:\0")) (ns_string (ref u8"nothing\0")) (ns_string (ref u8"txt\0")))
    do! IO.writeln $"bundle: identifier={ident} data.txt={found} nothing.txt={absent}"
    // FileDialog: create and configure an open panel without showing it.
    let app = send0 (cls (ref u8"NSApplication\0")) (sel (ref u8"sharedApplication\0"))
    // NSApplicationActivationPolicyAccessory = 1: no Dock icon, no menu bar.
    let policy = send1 app (sel (ref u8"setActivationPolicy:\0")) 1
    let panel = send0 (cls (ref u8"NSOpenPanel\0")) (sel (ref u8"openPanel\0"))
    let _ = send1 panel (sel (ref u8"setCanChooseFiles:\0")) 1
    let _ = send1 panel (sel (ref u8"setCanChooseDirectories:\0")) 0
    let _ = send1 panel (sel (ref u8"setAllowsMultipleSelection:\0")) 0
    let _ = send1 panel (sel (ref u8"setMessage:\0")) (ns_string (ref u8"Choose a file\0"))
    let files = send0 panel (sel (ref u8"canChooseFiles\0")) % 256
    let message = from_ns_string (send0 panel (sel (ref u8"message\0")))
    let visible = send0 panel (sel (ref u8"isVisible\0")) % 256
    let _ = send0 panel (sel (ref u8"close\0"))
    do! IO.writeln $"panel: main={pthread_main_np ()} policy={policy % 256} panel={panel != 0} files={files} message={message} visible={visible}"
    0
```

```sh
SDK=$(xcrun --show-sdk-path)
mkdir -p /tmp/tz-m13/out && cp Main.tz /tmp/tz-m13/ && cd /tmp/tz-m13
tsuzuri check Main.tz
tsuzuri build Main.tz -l objc --link $SDK/System/Library/Frameworks/AppKit.framework/AppKit.tbd \
    --link $SDK/System/Library/Frameworks/Foundation.framework/Foundation.tbd -o out/probe
echo hello > out/data.txt
perl -e 'alarm 15; exec @ARGV' ./out/probe; echo "exit=$?"
```

実際の出力（macOS 27.2.1 arm64、SDK 27.2、開発版 `target/debug/tsuzuri`。パスは作業ディレクトリによる）:

```text
clipboard: wrote=1 back=こんにちは 🍵
defaults: string=hello integer=42 bool=1 absent=<nil>
bundle: identifier=<nil> data.txt=/tmp/tz-m13/out/data.txt nothing.txt=<nil>
panel: main=1 policy=1 panel=true files=1 message=Choose a file visible=0
exit=0
```

`check` は 0 で、プロセスは残らなかった。`.tbd` を `--link` で渡すのは、`-framework` を渡す口がないためである（M20 の現状）。この形は、型の保護も所有の管理もなく、セレクターと引数の位置の誤りはどれも実行時まで分からない。

3. **ダイアログの経路も headless で試せる**: 上のソースの `main` を次に置き換えると、`runModal` を、モーダルの run loop の mode に置いた遅延の `abortModal` で終わらせられる。コールバックもクラスの定義も要らない。

```tsuzuri
def main :: unit -> i32 = \() ->
    let app = send0 (cls (ref u8"NSApplication\0")) (sel (ref u8"sharedApplication\0"))
    let _ = send1 app (sel (ref u8"setActivationPolicy:\0")) 1
    let panel = send0 (cls (ref u8"NSOpenPanel\0")) (sel (ref u8"openPanel\0"))
    // The value of the data symbol NSModalPanelRunLoopMode, written as a literal.
    let modes = send1 (cls (ref u8"NSArray\0")) (sel (ref u8"arrayWithObject:\0")) (ns_string (ref u8"NSModalPanelRunLoopMode\0"))
    let _ = send app (sel (ref u8"performSelector:withObject:afterDelay:inModes:\0")) (sel (ref u8"abortModal\0")) 0 modes 0 0 0 0 0.5 0.0 0.0 0.0
    let response = send0 panel (sel (ref u8"runModal\0"))
    let url = send0 panel (sel (ref u8"URL\0"))
    do! IO.writeln $"runModal: response={response} url={url != 0} main={pthread_main_np ()}"
    0
```

```text
runModal: response=-1001 url=false main=1
exit=0
```

-1001 は `NSModalResponseAbort`（`AppKit.framework/Headers/NSApplication.h:123`）。パネルは 0.5 秒だけ画面に出て閉じた。プロセスは残らなかった。

4. **通知は bundle のない実行ファイルでは使えない**: `main` を次に置き換え、`UserNotifications.framework/UserNotifications.tbd` を足してリンクすると、例外でプロセスが終わる（終了コード 134。出力のパスは作業ディレクトリによる）。ObjC の例外は Tsuzuri のフレームを越えると abort する（D-67 の事実 7）。

```tsuzuri
def main :: unit -> i32 = \() ->
    do! IO.writeln "notify: calling +[UNUserNotificationCenter currentNotificationCenter]"
    let center = send0 (cls (ref u8"UNUserNotificationCenter\0")) (sel (ref u8"currentNotificationCenter\0"))
    do! IO.writeln $"notify: center={center != 0}"
    0
```

```text
notify: calling +[UNUserNotificationCenter currentNotificationCenter]
*** Terminating app due to uncaught exception 'NSInternalInconsistencyException', reason: 'bundleProxyForCurrentProcess is nil: mainBundle.bundleURL file:///tmp/tz-m13/out/'
```

### 4. 確かめた事実（同じ環境）

- **設定の後始末**: `removePersistentDomainForName:` は領域を空にするが、`~/Library/Preferences/<suite>.plist` のファイルは残る（42 バイトの空の辞書）。書き込みは cfprefsd が非同期に行うので、プロセスの終了の直後は古い値（`"greeting" => "hello"`）が残っていることもあった。終了の直後にファイルを消すと、cfprefsd が空のファイルを書き戻した。`defaults delete <suite>` は空の領域に `Domain ... not found` を返し、ファイルを消さない。pid を名前に含めた suite は、実行のたびにファイルを 1 つ残す。
- **資源の場所**: 平らな実行ファイルの `[NSBundle mainBundle]` は実行ファイルのディレクトリで、`bundleIdentifier` は nil（上の出力）。`tsuzuri run` で走らせると、`bundlePath` は一時ディレクトリ（`$TMPDIR/.tsuzuri-<pid>-<n>`）になる。資源の検査は `tsuzuri build -o` の実行ファイルか、M15 の `.app` で行う。
- **NSString の符号化**: 孤立サロゲートを含む `NSString` は、UTF-8 への変換（`dataUsingEncoding:` の 4）が nil を返し、UTF-16LE への変換（`0x94000100`）は 6 バイトを返した。U+0000 を含む文字列は、どちらでも欠けずに変換された。上の `from_ns_string`（`getCString:maxLength:encoding:`）は孤立サロゲートで失敗し、U+0000 で切れる。std の `String.to_code_units`・`String.from_code_units`（string.md の 184〜185 行）は孤立サロゲートも残す。
- **活性化のポリシー**: `setActivationPolicy:` は Accessory（1）で YES を返した。Prohibited（2）では NO を返した（初回の試行）。
- **スレッドの注釈**（SDK 27.2）: `NSResponder` に `NS_SWIFT_UI_ACTOR`（`AppKit.framework/Headers/NSResponder.h:20-21`）。`NSSavePanel : NSPanel`（`NSSavePanel.h:20`）、`NSOpenPanel : NSSavePanel`（`NSOpenPanel.h:18`）なので、パネルは継承でメインスレッドのクラスである。`NSPasteboard`（`NSPasteboard.h:157`）と `NSUserDefaults`（`Foundation.framework/Headers/NSUserDefaults.h:144`）に注釈はない。`NSUserDefaults` は文書で thread-safe と書かれている（同 99〜100 行）。`NSBundle` は `NS_SWIFT_SENDABLE`（`NSBundle.h:100-101`）。M07 の D3 は、これをパネルは `main`（M09 の Phase 2。なければ既定の `local`）、`NSPasteboard`・`NSUserDefaults` は既定、`NSBundle` は `sync` に写す。
- **型の指定**: `allowedContentTypes` は `UTType` の配列で macOS 11 から（`NSSavePanel.h:62`）。拡張子の配列の `allowedFileTypes` は非推奨（`NSSavePanel.h` の 274 行からの `NSDeprecated` の分類）。拡張子から型を作るのは `+[UTType typeWithFilenameExtension:]`（`UniformTypeIdentifiers.framework/Headers/UTType.h:100`。nil を返しうる）。
- **データシンボル**: `NSPasteboardTypeString` は `APPKIT_EXTERN NSPasteboardType const`（`NSPasteboard.h:24`）で、値は `"public.utf8-plain-text"`。M06 の Phase 1 の `view def` と M07 の写像で読む。
- **suite の名前**: アプリ自身の bundle の識別子と `NSGlobalDomain` を `initWithSuiteName:` に渡すのは誤り（`NSUserDefaults.h:164` の注釈）。結果は nullable（166 行）。現状 3 の 2 の宣言で確かめると、`initWithSuiteName:` は `"NSGlobalDomain"` に nil を返し、標準エラーに `Using NSGlobalDomain as an NSUserDefaults suite name does not make sense and will not work.` を書いた。空の名前（`""`）には nil でない物体を返した。
- **テストの本体のスレッド**: `tsuzuri test` の各テストの本体はメインスレッドで走る。`@trusted extern "pthread_main_np" def pthread_main_np :: unit -> i32` を宣言した `Checks.tz` に `test "runs on the main thread" = assert (pthread_main_np () == 1)` と、`== 0` を表明するテストを置いて `tsuzuri test .` を走らせると、前者が `ok`、後者が `not ok`（`trapped or terminated by signal`）だった（既定のネイティブの `-O0`）。

### 5. パッケージと型の仕組み

- git のパッケージはリポジトリのルートの 1 つだけで（`_tsuzuri/language-reference/organizing-tsuzuri/packages.md:120`）、`[native]` を書けない（121 行）。リポジトリの下位ディレクトリのパッケージは未実装（477 行）。そのため、`apple` のリポジトリに置くモジュールは `apple` パッケージの一部になり、framework のリンクは M20 の Phase 2 の `@link(framework = "...")` でしか運べない（M07 の現状 12）。
- 依存のパッケージのモジュールは、使わないものも毎回型検査される（W27 の現状）。
- `IO` のアクションの中へ `Drop` の値を持ち出すと `E1005`（`_tsuzuri/language-reference/built-in-types-and-modules/io.md:329`）。M01 の ObjC の物体は drop で `objc_release` を呼ぶ所有の値で、既定の性質 `local` は捕捉できない。M09 の Phase 2 は、メインスレッドのハンドルを使う関数値を `E1013`（`main_thread_function_value`）にする（M09 の仕様「Phase 2: 拒否」）。
- 名前空間とモジュールは同じ道筋で共存できる（W27 の現状の再現。`Win32/UI/Shell.tz` と `Win32/UI/Shell/`）。M07 の Phase 2 のパッケージは `name = "apple"`・`namespace = "Apple"` で、framework ごとのモジュール（`Apple::AppKit`・`Apple::Foundation`）を持ち、M13 の Phase 1 が使う宣言は、M13 が同じリポジトリの核の一覧 `api.txt` に足して生成し直す（M07 の「他チケットへ提供するインターフェース」、M13 の実装手順 2）。

## 変えてはいけない意味

- このチケットは、どのプログラムのコンパイラの出力（IR・object・header・診断・既定の `.wasm`）も変えない。コンパイラと std に触れないからである。
- std の公開の名前と `RESERVED_MODULES` を変えない。`Desktop`・`Clipboard` などは std の予約名にしない。
- `apple` パッケージの生成した宣言（M07）の意味を変えない。包みは宣言を呼ぶだけで、宣言を手で書き換えない。
- テストは、利用者の general の pasteboard、既定の defaults の領域、ホームの外のファイルに書かない。

## 仕様

### Phase の範囲

| Phase | 内容 | 規模 | 前提 |
| --- | --- | --- | --- |
| 1 | `Apple::Desktop.Error`、`Clipboard`（文字列）、`FileDialog`（開くと保存の `runModal`、結果はパスの `string`）、`Preferences`（string・i64・bool）、`Resources`（main bundle の資源のパス）、headless の E2E、README | L | M07 の Phase 2。D1・D4 の承認（D10 は承認済み） |
| 2 | `Notifications`（UserNotifications。bundle の識別子と許可が要り、完了ハンドラーは M04 の Block） | M | M04 の Phase 1・M09 の Phase 1・M15。D9 の承認 |
| 3 | Windows・Linux の版を同じ API で足すかの判断（W27 の `win32` パッケージの上） | S（判断の記録） | D3 の承認 |

### Phase 1: 置き場と名前

- `apple` パッケージのリポジトリのルートに `Desktop.tz`（モジュール `Apple::Desktop`）と、ディレクトリ `Desktop/` の 4 つのモジュールを置く。名前空間を外側へ補う解決（`_tsuzuri/language-reference/organizing-tsuzuri/namespaces.md` の 103 行からの「名前解決の順序」）で、`Desktop/` の中のモジュールは共通の型を `Desktop.Error` と書ける。

| ファイル | モジュール | 中身 |
| --- | --- | --- |
| `Desktop.tz` | `Apple::Desktop` | `Error`・`message`、私的な文字列の変換とスレッドの検査 |
| `Desktop/Clipboard.tz` | `Apple::Desktop::Clipboard` | `Board`・`general`・`unique`・`read_string`・`write_string`・`clear`・`change_count` |
| `Desktop/FileDialog.tz` | `Apple::Desktop::FileDialog` | `Options`・`SaveOptions`・`options`・`save_options`・`open_file`・`open_files`・`save_file` |
| `Desktop/Preferences.tz` | `Apple::Desktop::Preferences` | `Store`・`standard`・`suite`・`get_string`・`set_string`・`get_i64`・`set_i64`・`get_bool`・`set_bool`・`remove` |
| `Desktop/Resources.tz` | `Apple::Desktop::Resources` | `path`・`directory`・`bundle_identifier` |

- 利用者は `using Apple::Desktop` の後に `Clipboard.read_string (ref board)` と書く。共通の型は `Apple::Desktop.Error` と書く。
- 契約の案 `AppleDesktop.*` は、git のパッケージがリポジトリのルートの 1 つで既定の名前空間 `Apple` を持つ（現状 5）ので、`Apple::Desktop` の形に読み替えた（決定事項 D1）。

### Phase 1: 共通の型 `Apple::Desktop.Error`

```tsuzuri
union Error = NotMainThread | InvalidInput | Rejected | PanelFailed | NoBundle | NotAuthorized deriving (Eq, Display)

def message :: ref Error -> string
```

| case | いつ | Phase |
| --- | --- | --- |
| `NotMainThread` | メインスレッドでないところで FileDialog を呼んだ | 1 |
| `InvalidInput` | 空の拡張子、`UTType` を作れない拡張子、空の suite の名前、nil を返した suite の名前 | 1 |
| `Rejected` | OS が失敗を返した（`setString:forType:` が NO） | 1 |
| `PanelFailed` | `runModal` が `NSModalResponseAbort`（-1001）を返した | 1 |
| `NoBundle` | bundle の識別子がない実行ファイルで通知を使った | 2 |
| `NotAuthorized` | 通知の許可がない | 2 |

- Phase 2 の 2 つの case も Phase 1 から持つ。union に case を足すと網羅的な `match` が壊れるからである（net.md の「エラーの読み方」と同じ理由。決定事項 D5）。
- 例外にしない（D-10）。ObjC の例外になる呼び方は、呼ぶ前に検査して値で返す（スレッドの検査、bundle の識別子の検査）。M05 には依らない。

### Phase 1: Clipboard

| 関数 | 型 | 下げ方 |
| --- | --- | --- |
| `general` | `unit -> Board` | `+[NSPasteboard generalPasteboard]` |
| `unique` | `unit -> Board` | `+[NSPasteboard pasteboardWithUniqueName]`。`Board` の drop で `releaseGlobally` |
| `read_string` | `ref Board -> Result<Maybe<string>, Desktop.Error>` | `stringForType: NSPasteboardTypeString`。nil は `Maybe.None` |
| `write_string` | `ref string -> ref Board -> Result<unit, Desktop.Error>` | `clearContents` の後に `setString:forType:`。NO は `Rejected` |
| `clear` | `ref Board -> unit` | `clearContents` |
| `change_count` | `ref Board -> i64` | `changeCount` |

- `Board` は `NSPasteboard` の所有の参照（M01）と、使い捨てかどうかの印を持つ record である。general の板の drop は `objc_release` だけで、`releaseGlobally` を呼ばない。
- 文字列だけを扱う（`public.utf8-plain-text`）。画像・RTF・ファイルの URL・複数の項目は対象外。

### Phase 1: FileDialog

```tsuzuri
record Options { title: Maybe<string>, message: Maybe<string>, directory: Maybe<string>, extensions: [string], choose_directories: bool, multiple: bool }
record SaveOptions { title: Maybe<string>, message: Maybe<string>, directory: Maybe<string>, name: Maybe<string>, extensions: [string] }
```

| 関数 | 型 | 意味 |
| --- | --- | --- |
| `options` | `unit -> Options` | 既定（題と文なし、ディレクトリは OS の既定、拡張子の制限なし、ファイルだけ、1 つだけ） |
| `save_options` | `unit -> SaveOptions` | 既定 |
| `open_file` | `ref Options -> Result<Maybe<string>, Desktop.Error>` | 開くパネルを `runModal` で出し、選んだ 1 つのパス。取り消しは `Maybe.None` |
| `open_files` | `ref Options -> Result<[string], Desktop.Error>` | 選んだすべてのパス（`URLs`）。取り消しは空の配列 |
| `save_file` | `ref SaveOptions -> Result<Maybe<string>, Desktop.Error>` | 保存のパネルの `runModal` と `URL`。取り消しは `Maybe.None` |

- 下げ方: `+[NSApplication sharedApplication]` を呼んでから（何度でもよい）パネルを作り、`setCanChooseFiles:`・`setCanChooseDirectories:`・`setAllowsMultipleSelection:`・`setTitle:`・`setMessage:`・`setDirectoryURL:`（`+[NSURL fileURLWithPath:isDirectory:]`）・`setNameFieldStringValue:`・`setAllowedContentTypes:`（拡張子ごとに `+[UTType typeWithFilenameExtension:]`）を設定し、`runModal` を呼ぶ。
- 結果: `NSModalResponseOK`（1）は `URL.path`・`URLs` の各 `path` を `string` に変える。`NSModalResponseCancel`（0）は取り消し。`NSModalResponseAbort`（-1001）は `PanelFailed`。ほかの値も `PanelFailed`。
- `extensions` の要素が空か、`typeWithFilenameExtension:` が nil を返したら、パネルを出さずに `InvalidInput`。空の配列は制限なし（`allowedContentTypes` に `@[]`）。
- `multiple` が false の `open_files` は、要素が 0 か 1 の配列を返す。
- シートの形（`beginSheetModalForWindow:completionHandler:`）と、完了ハンドラーの形（`beginWithCompletionHandler:`）は対象外（M04 の Block が要る。Phase 1 は `runModal` だけ）。

### Phase 1: Preferences

| 関数 | 型 | 下げ方 |
| --- | --- | --- |
| `standard` | `unit -> Store` | `+[NSUserDefaults standardUserDefaults]` |
| `suite` | `ref string -> Result<Store, Desktop.Error>` | `initWithSuiteName:`。空の名前は呼ぶ前に `InvalidInput`、nil の結果も `InvalidInput` |
| `get_string` | `ref string -> ref Store -> Maybe<string>` | `objectForKey:` が `NSString` のときだけ値 |
| `set_string` | `ref string -> ref string -> ref Store -> unit` | `setObject:forKey:` |
| `get_i64` | `ref string -> ref Store -> Maybe<i64>` | `objectForKey:` が `NSNumber` のときだけ `longLongValue` |
| `set_i64` | `ref string -> i64 -> ref Store -> unit` | `setInteger:forKey:` |
| `get_bool` | `ref string -> ref Store -> Maybe<bool>` | `objectForKey:` が `NSNumber` のときだけ `boolValue` |
| `set_bool` | `ref string -> bool -> ref Store -> unit` | `setBool:forKey:` |
| `remove` | `ref string -> ref Store -> unit` | `removeObjectForKey:` |

- 値の型を変換しない。`"42"` の文字列を `get_i64` で読むと `Maybe.None`（`integerForKey:` の暗黙の変換を使わない。決定事項 D7）。無い key も `Maybe.None`。ただし bool と整数はどちらも `NSNumber` なので、この 2 つは区別しない。`set_bool` で書いた値を `get_i64` で読むと `Maybe.Some 1` になる（現状 3 の 2 の宣言で、`setBool:forKey:` の後の `objectForKey:` が `isKindOfClass: NSNumber` で YES、`longLongValue` で 1 を返した）。
- 空の suite の名前は `initWithSuiteName:` を呼ばずに `InvalidInput` にする（空の名前には nil が返らず、検査が OS に任せられないため。現状 4）。
- key が空の文字列なら、何もせずに getter は `Maybe.None`、setter と `remove` は何もしない（`InvalidInput` を返す形にしない理由は D7）。
- 書き込みの永続化は cfprefsd に任せる（`synchronize` を呼ばない。`NSUserDefaults.h:368-371` は「unnecessary and shouldn't be used」としている）。

### Phase 1: Resources

| 関数 | 型 | 下げ方 |
| --- | --- | --- |
| `path` | `ref string -> ref string -> Maybe<string>` | `[[NSBundle mainBundle] pathForResource:name ofType:extension]` |
| `directory` | `unit -> Maybe<string>` | `resourcePath` |
| `bundle_identifier` | `unit -> Maybe<string>` | `bundleIdentifier` |

- 平らな実行ファイルでは、資源の場所は実行ファイルのディレクトリで、`bundle_identifier` は `Maybe.None`（現状 3・4）。M15 の `.app` では `Contents/Resources` と `[macos] bundle_id` になる。
- 引数の順序は `pathForResource:ofType:` と同じ（名前、拡張子）。どちらも対象ではない。

### Phase 1: 文字列の受け渡し

- `string` から `NSString`: `String.to_code_units` の UTF-16 を `initWithCharacters:length:` に渡す。孤立サロゲートも U+0000 も失わない。
- `NSString` から `string`: `dataUsingEncoding:` に UTF-16LE（`0x94000100`）を渡し、`NSData` の `length` と `getBytes:length:` で `[ubyte]` に写し、2 バイトずつ `i16u` にして `String.from_code_units` に渡す。現状 4 のとおり孤立サロゲートも U+0000 も失わない。nil（起きないはずの変換の失敗）は、`Result` を返す関数では `Rejected`、`Maybe` を返す関数（Preferences の getter、Resources）では `Maybe.None` にする。
- M02 の Phase 2（NSRange の値渡し）があれば、`getCharacters:range:` で直接 `ref mut [i16u]` に写してよい。結果のバイト列は同じでなければならない（テストで比べる）。
- 定数の `NSString`（型の名前、run loop の mode の名前）は M02 の `ObjC.literal "..."` か、M06 のデータシンボルで読む。

### Phase 1: スレッド

- FileDialog の 3 つの関数は、入口で `pthread_main_np () != 0` を確かめ、違えば何も作らずに `NotMainThread` を返す。M09 の Phase 1 があれば `MainThread.is_current ()` を使う。
- M09 の Phase 2 があれば、M07 は `NSOpenPanel`・`NSSavePanel` を `extern class main` に写すので、FileDialog の関数を tzroutine や関数値から呼ぶとコンパイル時に `E1013` になる。入口の検査はそのまま残す（M09 の Phase 2 がなければ、パネルは既定の `local` で、`Routine.with_locked_thread`（ワーカーのスレッドに固定する。M08）や GCD のスレッドのコールバックの中から呼べてしまうため）。
- Clipboard・Preferences・Resources はスレッドを確かめない。`NSPasteboard` と `NSUserDefaults` は M07 の既定の性質（M01 の D1 の推奨は W19 の `local`）になるので、`Board`・`Store` はほかの tzroutine へ送れず（`E1013`）、関数値に捕捉できない（`E1005`）。それを触る関数は効果 `pinned` を持ち、`tzroutine { ... }` などで始めると `E1013`（`pinned_code_spawned`）になる（native-interop.md の「スレッド拘束ハンドル」）。Clipboard と Preferences は `main`・コールバック・`Routine.with_locked_thread` の中で呼ぶ。`NSBundle` は `sync` なので、Resources にこの制限はない。
- ダイアログを出している間、`runModal` はメインスレッドの入れ子の run loop を回す。ほかの tzroutine はワーカーのスレッドで進む。

### Phase 1: 引数の順序

- D-59 の規則（対象を最後に）に従う。`tests/std_argument_order.rs` が検査するのは `std/` と組み込みだけで、パッケージは検査の外だが、同じ規則で書く。
- 対象は板（`Board`）と保存先（`Store`）とダイアログの設定（`Options`）。key と値はその前に置く（`Preferences.set_string key value store`、`Clipboard.write_string text board`）。資源のハンドルの例外は使わない。
- パッケージの README の各モジュールの節に「引数の順序」を書く（言語リファレンスの std のモジュールと同じ書き方）。

### Phase 2: Notifications

- モジュール `Apple::Desktop::Notifications`（`Desktop/Notifications.tz`）。名前は Phase 2 の着手の前に承認を受ける（D9）。案:

| 関数 | 型 | 意味 |
| --- | --- | --- |
| `request_authorization` | `Owned.Function<Result<bool, Desktop.Error>, unit> -> Result<Notifications.Pending, Desktop.Error>` | `requestAuthorizationWithOptions:completionHandler:`。結果はメインスレッドで関数に渡す |
| `post` | `ref Notifications.Content -> Result<unit, Desktop.Error>` | `UNNotificationRequest` を作り `addNotificationRequest:withCompletionHandler:`（完了ハンドラーは nil） |

- `record Content { identifier: string, title: string, body: string }`。
- 入口で `[[NSBundle mainBundle] bundleIdentifier]` を確かめ、nil なら `UNUserNotificationCenter` に触れずに `NoBundle` を返す（現状 3 の 4 の abort を避ける）。
- 完了ハンドラー: M04 の `ObjC.block on_authorized token` で Block を作る。`token` は `Anchor.create` に預けた利用者の関数の鍵で、Block は GCD のスレッドで呼ばれる（`Anchor` は引けない）。Block の本体は `MainThread.post`（M09 の Phase 1）で、鍵と結果（`i64` と `bool`）を捕捉した関数を main に積み、main がその鍵で `Anchor` から利用者の関数を取り出して呼ぶ（M04 の仕様 6 の例と同じ形）。
- `Pending` は `Anchor` の所有者を持つ record。利用者が完了の前に drop すると、鍵は `Stale` になり、結果は捨てられる。run loop（NSApp）が回っていなければ、結果は届かない（M09 の Phase 1 の規則）。
- 許可を求める画面は利用者の操作が要るので、自動の検査は「bundle がないときの `NoBundle`」と「`getNotificationSettingsWithCompletionHandler:` が画面を出さずに状態を返す」までにする。

### Phase 3: Windows・Linux

- 判断だけを行い、結果をこのチケットに記録する。Windows は W27 の `win32` パッケージの上に、同じ名前の API（Clipboard は `OpenClipboard`・`SetClipboardData`、FileDialog は W25 の `IFileOpenDialog`、Preferences はファイルか registry）を足せるかを調べる。Linux はデスクトップの環境ごとに API が違う（X11・Wayland・portal）ので、対象にするかを含めて決める。
- W27 の配布の形（W27 の D3・D9）が決まるまで判断しない（決定事項 D3）。

## 設計

### 置き場（パッケージのリポジトリ）

| ファイル | 内容 |
| --- | --- |
| `Desktop.tz` | `Error`・`message`。私的な関数 `ns_string_of`（`string` → `NSString`）、`string_of_ns`（`NSString` → `string`）、`on_main_thread` |
| `Desktop/Clipboard.tz` | 仕様の表。`Board` と `instance Drop<Board>`（使い捨ての板だけ `releaseGlobally`） |
| `Desktop/FileDialog.tz` | 仕様の表。私的な `configure`（パネルの共通の設定）と `content_types`（拡張子から `UTType` の配列） |
| `Desktop/Preferences.tz` | 仕様の表 |
| `Desktop/Resources.tz` | 仕様の表 |
| `api.txt` | M07 の核の一覧に、下の「宣言の出どころ」の宣言を足す |
| `tests/desktop.mjs`・`tests/desktop/` | headless の E2E（テスト計画） |
| `README.md` | 使い方、モジュールごとの「引数の順序」、スレッドの規律、テストの後始末 |

### 宣言の出どころ

- 包みが呼ぶ宣言は、すべて M07 の生成器が `api.txt` から生成する。足す項目の案（正確な綴りは M07 の D10 の書式）: `NSPasteboard`（`generalPasteboard`・`pasteboardWithUniqueName`・`releaseGlobally`・`clearContents`・`changeCount`・`setString:forType:`・`stringForType:`）、`NSPasteboardTypeString`、`NSSavePanel`・`NSOpenPanel`（仕様の設定と `runModal`・`URL`・`URLs`）、`NSURL`（`fileURLWithPath:isDirectory:`・`path`）、`NSArray`（`count`・`objectAtIndex:`・`array`・`arrayByAddingObject:`）、`UTType`（`typeWithFilenameExtension:`。framework `UniformTypeIdentifiers` を生成の対象に足す）、`NSUserDefaults`（仕様の表）、`NSNumber`（`longLongValue`・`boolValue`）、`NSString`（`initWithCharacters:length:`・`dataUsingEncoding:`）、`NSData`（`length`・`getBytes:length:`）、`NSBundle`（仕様の表）、`NSApplication`（`sharedApplication`）、`isKindOfClass:` の代わりの `ObjC.downcast`（M01）。
- 可変長のセレクター（`arrayWithObjects:`）は M07 が省く。`arrayWithObjects:count:` は物体のポインターの C の配列を取り、M02 の型で書けないので使わない。`UTType` の配列は `array` と `arrayByAddingObject:` で 1 つずつ足して作る。
- 生成した宣言に `@trusted` は付かない（M07 の D6）。包みのモジュールは `extern` を宣言しないので、`W1010` は生成した宣言の側に出る（M07 の Phase 3 の印で消す）。

### 所有

- 包みの中の ObjC の物体は、M01 の所有の値として関数の中で作って捨てる。`Board`・`Store` の field の物体だけが関数を越えて生きる。
- `runModal` の結果の `URLs` の要素は +0 の借用で、`path` を読んだらすぐ捨てる。`NSURL` を利用者に返さない（パスの `string` にする）。

### 採らなかった案

置き場（std・別のリポジトリ）は D1、`IO` を返す形は D4、UTF-8 の受け渡しは D6 に、採らない理由とともに書いた。ほかに次を採らなかった。

| 案 | 採らなかった理由 |
| --- | --- |
| `objc_msgSend` の手書きの宣言で包む（現状 3 の形） | 型の保護がなく、W14 の上位形の回避策を公式パッケージに固定する。宣言は M07 の生成に任せる（S1） |
| 完了ハンドラーのダイアログ（`beginWithCompletionHandler:`）を Phase 1 にする | M04 の Block と M09 の投入が要り、Phase 1 の依存が増える。`runModal` は入れ子の run loop で UI を止めない |
| 包みの中に宣言を複写する（`apple` の生成物に依らない） | 同じ宣言が 2 か所にでき、SDK を上げたときに食い違う。依存の型検査の量も増える（S3） |

## 前提とする他チケットのインターフェース

- [M07](M07-apple-sdk-bindgen-and-package.md)（Phase 2）: 公式の `apple` パッケージ（`namespace = "Apple"`、framework ごとのモジュール、核の一覧 `api.txt`、macOS のランナーの CI、`git diff --exit-code` の生成の検査）。生成器の `--api` の書式（D10）、名前の規則（D2）、スレッドの性質の写し方（D3）、availability の `@weak`（D5）、生成物の `@trusted`（D6）。
- [M01](M01-objc-class-types.md)・[M02](M02-objc-typed-message-send.md)（M07 を通して）: `extern class` の所有（+1 の値、drop で `objc_release`、`ref C` の +0、`Maybe<C>` の nil）、`ObjC.downcast`、メンバーの送信と buffer の引数（ポインターと長さの 2 つ。M02 の D10）、`ObjC.literal`。
- [M06](_completed/M06-extern-data-symbols-and-zutf8string.md)（M07 を通して）: Phase 1 の `view def` で `NSPasteboardTypeString` を読む。
- [M20](_completed/M20-macos-framework-link-inputs.md)（M07 を通して）: Phase 2 の `@link(framework = "...")`（git のパッケージから AppKit・Foundation・UniformTypeIdentifiers・UserNotifications をリンクする）。
- [M09](M09-main-thread-dispatch-and-handles.md)（弱い）: Phase 1 の `MainThread.is_current`（スレッドの検査）と `MainThread.post`（Phase 2 の完了の結果）。Phase 2 の `extern class main`（M07 がパネルに付ける）。
- [M04](M04-objc-blocks.md)（Phase 2）: `ObjC.Block<F>`・`ObjC.block`（`(bool -> Maybe<ref NSError> -> unit)` の完了ハンドラー）。M13 が Block を使うのは Phase 2 の通知の完了ハンドラーだけで、ファイルダイアログの `beginWithCompletionHandler:` は対象外（Phase 1 は `runModal` だけ）。M04 の「他チケットへ提供するインターフェース」も同じ形で書いてある。
- [M15](M15-macos-app-bundle.md)（Phase 2）: `tsuzuri build --emit app` と `[macos] bundle_id`（通知の検査を bundle の中で走らせる）。
- [W23](_completed/W23-anchor-state.md): `Anchor.create`・`token`・`read`（Phase 2 の利用者の関数の預け先）。

## 他チケットへ提供するインターフェース

- なし（後続はない）。
- M07 の核の一覧に足す宣言の一覧（設計の「宣言の出どころ」）は、M07 の README の核の節に記録し、M07 の CI が生成の差と型検査で守る。
- Phase 3 の判断の結果は、W27 の Phase 2 の公式パッケージ（`win32`）の README に、同じ API を足すかどうかとして渡す。

## 実装手順

### Phase 1

1. **変更**: なし（パッケージのリポジトリのブランチ。手元の作業は `target/scratch/m13/`）。**内容**: `node scripts/repo-kb.mjs brief M13`。着手条件の確認。`tsuzuri test` のテストの本体がメインスレッドで走ること（現状 4）を、着手時の HEAD と `-O0`・`-O3` で確かめ直す（走らなければ、FileDialog の `test` を E2E に移す）。現状 3 の 2〜4 を着手時の HEAD とパッケージの版で再現し、違えばこのチケットに追記する。`apple` を依存にした hello の `check` の時間を測って記録する（S3 の基準）。**確認**: 現状と同じ出力。基準の時間を記録した。
2. **変更**: `api.txt`（パッケージのリポジトリ）。**内容**: 設計の「宣言の出どころ」の宣言を核の一覧に足し、M07 の手順で生成し直す（D4 の承認の後）。**確認**: `git diff` が足した宣言だけで、M07 の CI（生成の差と headless のアプリ）が通る。生成できない宣言があれば S1。
3. **変更**: `Desktop.tz`。**内容**: `Error`・`message`、文字列の変換、スレッドの検査。**確認**: `Desktop.tz` の `test` で、`"こんにちは 🍵"`・孤立サロゲート（`String.from_code_units [0x41i16u, 0xD800i16u]`）・U+0000 を含む文字列・空の文字列が `NSString` を往復して同じ符号単位になる。M02 の Phase 2 があれば `getCharacters:range:` の結果とも比べる。
4. **変更**: `Desktop/Clipboard.tz`・`Desktop/Preferences.tz`・`Desktop/Resources.tz`。**内容**: 仕様の 3 つの表。**確認**: 各ファイルの `test`（テスト計画の表の Clipboard・Preferences）が `tsuzuri test` で通る（`running N tests` の N を確かめる）。Resources は手順 6 の E2E で確かめる（`tsuzuri test` の実行ファイルの場所が決まらないため）。
5. **変更**: `Desktop/FileDialog.tz`。**内容**: 仕様の FileDialog とスレッド。**確認**: パネルを出さない検査（設定の読み戻し、`InvalidInput`、tzroutine の中の `Routine.with_locked_thread` からの `NotMainThread`）が `tsuzuri test` で通る。
6. **変更**: `tests/desktop.mjs`・`tests/desktop/`。**内容**: テスト計画の E2E（`build` の実行ファイル、隣の資源、`runModal` と `abortModal`、後始末）。`-O0` と `-O3`。**確認**: 手元の macOS で通り、プロセスが残らず、`~/Library/Preferences/` にテストの suite のファイルが残らない。パッケージの CI の macOS のジョブで通る（S4）。
7. **変更**: パッケージの `README.md`、このリポジトリの文書（「ドキュメント」）。**内容**: D8 の承認の後に書く。**確認**: このリポジトリで `node scripts/check-docs.mjs <変更したページ>`。`target/scratch/m13/` を消す。

### Phase 2

1. D9 の承認の後、`Desktop/Notifications.tz` と、M15 の `--emit app` で作った bundle の中で走る E2E。
2. bundle のない実行ファイルの `NoBundle`（終了コード 0、abort しない）と、`getNotificationSettingsWithCompletionHandler:` の状態の読み取り（画面を出さない）を自動の検査にする。許可と配達は手で確かめ、結果をこのチケットに記録する。

### Phase 3

- D3 の承認の後、W27 の `win32` パッケージの上で Windows の版の見積もりをし、Linux を対象にするかを含めて判断を記録する。

## テスト計画

パッケージのリポジトリの検査である。このリポジトリには Rust のテストも node の suite も足さない。

| 検査 | 内容 | 期待 |
| --- | --- | --- |
| `Desktop.tz` の `test` | 文字列の往復（手順 3 の 4 種） | 同じ符号単位 |
| `Clipboard.tz` の `test "a unique board keeps a string"` | `unique` の板に書いて読む | 同じ文字列。`change_count` が増える |
| `Clipboard.tz` の `test "a cleared board has no string"` | `clear` の後に読む | `Result.Ok Maybe.None` |
| `Clipboard.tz` の `test "a board keeps surrogates and NUL"` | 孤立サロゲートと U+0000 を含む文字列 | 同じ符号単位 |
| `Preferences.tz` の `test` | テストごとに固定の名前の suite（`dev.tsuzuri.apple-desktop-test.<テストの名前>`。`tsuzuri test` は各テストを別のプロセスで並べて走らせるので、テストどうしで suite を分ける）で string・i64・bool を書いて読む。無い key、型の違う値、空の key、`remove` | 仕様の表のとおり。最初と最後に `removePersistentDomainForName:` を呼ぶ |
| `Preferences.tz` の `test "a suite named like the global domain is invalid"` | `suite (ref "NSGlobalDomain")` と `suite (ref "")` | どちらも `Result.Error InvalidInput`（前者は OS が nil を返す。後者は呼ぶ前の検査。現状 4） |
| `FileDialog.tz` の `test` | パネルを出さずに `configure` の結果を読み戻す。空の拡張子、`UTType` を作れない拡張子 | 設定が読める。`InvalidInput` |
| `FileDialog.tz` の `test "off the main thread"` | tzroutine の中の `Routine.with_locked_thread` で `open_file`（パネルは `local` で効果 `pinned` を持つので、tzroutine の本体から直接は呼べない。`with_locked_thread` はワーカーのスレッドに固定する。M08） | `Result.Error NotMainThread`（M09 の Phase 2 があれば `main_thread_code_spawned` の `E1013` になるので、`check` の検査に置き換える） |
| E2E: 資源 | `tsuzuri build -o out/app` の隣に `data.txt` を置いて `Resources.path`・`directory`・`bundle_identifier` | パスが一致、無い資源は `None`、識別子は `None` |
| E2E: ダイアログ | 現状 3 の 3 と同じ遅延の `abortModal` を置いて `open_file`・`save_file` | `Result.Error PanelFailed`、プロセスが残らない |
| E2E: Accessory | どの E2E も `setActivationPolicy:` を Accessory にし、`perl -e 'alarm 15; exec @ARGV'` で時間を切る | Dock に出ず、時間内に終わる |
| E2E: 後始末 | `tsuzuri test` と E2E の後に cfprefsd の書き込みを待ち（2 秒）、`~/Library/Preferences/dev.tsuzuri.apple-desktop-test.*.plist` を消す | 消した後に書き戻されない（5 秒後に確かめる）。suite の名前を固定するので、失敗しても残るのはテストの数まで |
| `-O0`・`-O3` | E2E を両方で | 同じ結果 |

- general の pasteboard への書き込みは自動の検査にしない（利用者のクリップボードを壊す）。README の例は `unique` を使う。
- 遅延の `abortModal` は、検査のプログラムの中だけで宣言する（パッケージの公開の API にしない）。
- 同梱の zig（lld）と Apple の ld の両方でリンクを確かめる（D-67 の共通の規則）。

## ドキュメント

- パッケージのリポジトリの `README.md`: 目的と非目標（ウィジェット・レイアウト・描画は持たない）、使い方（`apple = { git = "...", rev = "...", capabilities = ["ffi"] }`）、モジュールごとの関数の表と「引数の順序」、スレッドの規律（FileDialog はメインスレッド、`Board`・`Store` は送れない）、`tsuzuri run` では資源が見つからないこと、テストの後始末。
- このリポジトリの `_tsuzuri/language-reference/compiler/native-interop.md`: 632 行の「GUI は言語の一部ではありません」の段落の後に、公式パッケージ `apple` の `Apple::Desktop` が OS のサービスを包んでいることを、std の外のパッケージとして案内する（D8）。パッケージが公開されるまでは書かない。書くときも、コンパイラの機能ではないことを明記する。
- `_tsuzuri/language-reference/compiler/objective-c.md`（M01 が作るページ）があれば、同じ案内を関連項目に足す。`docs/language.md` の対応する節に 1 文。
- `node scripts/check-docs.mjs <変更したページ>`。std の公開 API は変わらないので、`node scripts/gen-docs.mjs` と `node scripts/check-cli-docs.mjs` の対象はない。

## 受け入れ条件

- [ ] D1・D4・D8・D10 の承認の記録がある。
- [ ] コンパイラ・std・このリポジトリのテストのコードを変えていない（`git diff --stat` が文書だけ）。
- [ ] Phase 1 の 5 つのモジュールが `apple` のリポジトリにあり、仕様の表のとおりに動く。`tsuzuri test` が通る（`running N tests` を確かめた）。
- [ ] 文字列の往復が孤立サロゲートと U+0000 を保つ。
- [ ] FileDialog がメインスレッドの外で `NotMainThread` を返し、`runModal` の abort を `PanelFailed` にする。
- [ ] E2E が `-O0`・`-O3`、Apple の ld と同梱の zig で通り、プロセスと設定のファイルが残らない。パッケージの CI の macOS のジョブで通る。
- [ ] 包みのモジュールに `objc_msgSend` の手書きの宣言がない（宣言はすべて M07 の生成物）。
- [ ] README とこのリポジトリの文書が、パッケージをコンパイラの機能として書いていない。`node scripts/check-docs.mjs` が通る。
- [ ] `target/scratch/m13/` を消した。

## 落とし穴

- **general の pasteboard に書かない**: 検査と例は `Clipboard.unique ()` を使う。general に書く検査は、開発者のクリップボードを黙って上書きする。
- **設定のファイルは残る**: `removePersistentDomainForName:` はファイルを消さず、cfprefsd が非同期に書く（現状 4）。終了の直後にファイルを消すと書き戻される。検査は suite の名前を固定し、待ってから消す。pid を名前に入れると、実行のたびにファイルが増える。
- **`tsuzuri run` の bundle**: `tsuzuri run` の実行ファイルは一時ディレクトリにあるので、`Resources.path` は資源を見つけない（現状 4）。検査と README の例は `tsuzuri build -o` の実行ファイルを使う。
- **UTF-8 で読まない**: `getCString:maxLength:encoding:` と `UTF8String` は孤立サロゲートで失敗し、U+0000 で切れる（現状 4）。UTF-16 で受け渡す。
- **`allowedFileTypes` は非推奨**: 拡張子は `UTType` に変えて `allowedContentTypes` に渡す。`typeWithFilenameExtension:` は nil を返しうるので、パネルを出す前に検査する。
- **パネルはメインスレッドのクラス**: `NSResponder` の `NS_SWIFT_UI_ACTOR` を継承する（現状 4）。tzroutine や `Routine.with_locked_thread` の中で作ると AppKit の例外になり、Tsuzuri のフレームを越えて abort する（M08）。入口の検査を省かない。
- **通知は bundle が要る**: bundle のない実行ファイルで `currentNotificationCenter` を呼ぶと abort する（現状 3 の 4）。M05 がなくても安全なように、識別子を先に確かめる。
- **ダイアログの検査はパネルを一瞬出す**: 遅延の `abortModal` の検査は、手元の画面にパネルを 0.5 秒ほど出す。CI の macOS のランナーで動かす。
- **`local` の物体は tzroutine で使えない**: `Board`・`Store` を触る関数は効果 `pinned` を持つ（M07 が `NSPasteboard`・`NSUserDefaults` を既定の `local` に写すため）。tzroutine の本体から呼ぶと `E1013` になる。README に、`main` かコールバックで呼ぶことを書く。M07 が `NSUserDefaults` を `sync` に写す根拠（Swift の Sendable の注釈）が SDK に入れば、この制限は Preferences から外れる。
- **Prohibited のポリシー**: `setActivationPolicy:` の Prohibited は NO を返した（現状 4）。検査は Accessory を使う。
- **依存の型検査の時間**: `Desktop` のモジュールは `apple` を使うすべての利用者で型検査される。宣言を包みに複写せず、生成した宣言を参照して大きさを抑える（S3）。

## 対象外

- ウィジェット・レイアウト・描画のツールキット（非目標。`_features/README.md:117`、native-interop.md:632）。
- std への同梱と、コンパイラの組み込みのモジュール。
- ドラッグ&ドロップ、印刷、Services のメニュー、URL スキームの受け取り、Dock のメニュー、ステータスバーの項目。
- クリップボードの文字列以外の型（画像・RTF・HTML・ファイルの URL・複数の項目）と、変更の監視（`changeCount` を読む関数だけを持つ）。
- シートのダイアログと完了ハンドラーのダイアログ（`beginSheetModalForWindow:`・`beginWithCompletionHandler:`）、アクセサリービュー、`NSAlert`。
- `NSUserDefaults` の string・i64・bool 以外の値（f64・日付・配列・辞書・データ）、KVO による変更の監視、App Group の共有。
- App Sandbox の security-scoped bookmark（サンドボックスのアプリがダイアログで得たパスを次の起動で使う仕組み）。M16 は App Sandbox を対象外にしているので、App Sandbox を扱う別のチケット（起票していない。M16 の対象外）で扱う。
- iOS・Mac Catalyst。

## 決定事項

- **D1（要承認。分岐 5 は承認済み（2026-11-26））置き場と名前（D-67 の分岐 5 を含む）**: 決定（推奨）: std の外の公式パッケージとし、M07 の Phase 2 の `apple` パッケージのリポジトリに、`Desktop.tz`（`Apple::Desktop`）と `Desktop/` の 4 つのモジュール（`Apple::Desktop::Clipboard` など）を置く。共通の型は `Apple::Desktop.Error`。採らない案: (a) std のモジュール（非目標、すべての OS に配られる、std はパッケージに依存できない）。(b) 別のリポジトリ `apple-desktop`（版の組が 2 つに増え、M07 の CI で一緒に検査できない。S3 のときの退き先）。(c) 契約の案の `AppleDesktop.*` の名前（git のパッケージはルートの 1 つで、既定の名前空間 `Apple` の下にしか置けない。現状 5）。理由: 包みが呼ぶ宣言と同じリポジトリにあれば、SDK の版を上げたときの生成し直しと包みの検査が同じ CI で済む。std の外の公式パッケージにすること（D-67 の分岐 5。M07 の D8 と同じ）は承認済み（2026-11-26）で、W27 の D3・D9 は W27 自身の決定として残る。置き場と名前は要承認のままで、承認前はチケット全体に着手しない。
- **D2（既定案）失敗の表し方**: 決定: 失敗は `Result<T, Apple::Desktop.Error>` で返し、取り消しと無い値は `Maybe` で返す。例外にしない。ObjC の例外になる呼び方（メインスレッドの外のパネル、bundle のない通知）は呼ぶ前に検査する。採らない案: M05 の `Result<R, ObjC.Exception>` で受ける（分岐 2 は 2026-11-26 に承認されたが、包みが M05 に依存する）、トラップする（利用者が回復できない）。理由: D-10。std の OS の関数と同じく、失敗を値で返す。
- **D3（既定案）Phase 3 の時期**: 決定: Windows・Linux の版を足すかの判断は、W27 の `win32` パッケージの配布の形（W27 の D3・D9）が決まってから行う。採らない案: 先に多 OS の抽象の API を決める（使う OS の API が見えないまま名前を固定する）。理由: Windows の版は W27 の公式パッケージの上にしか作れない。Linux はデスクトップの環境ごとに API が違い、対象にするかから決める必要がある。
- **D4（要承認）API の名前と形**: 決定（推奨）: 仕様の表の名前と型。すべて直接の関数（`IO` を返さない）で、対象（`Board`・`Store`・`Options`）を最後に置く。採らない案: (a) std の `File` と同じく `IO<Result<…>>` を返す（`Drop` の物体を `IO` に持ち出せない。M09 の Phase 2 がメインスレッドの関数値を拒む。現状 5）。(b) 板と保存先を持たず、general と standard だけの関数にする（検査が利用者の状態を書き換える。現状 3 の 2 は使い捨ての板と suite で検査できた）。(c) 資源のハンドルとして板を先頭に置く（D-59 の例外は要らず、対象を最後に置く規則に従える）。理由: 公開の API になり、後から変えると利用者のコードが壊れる。承認前は手順 2 に着手しない。
- **D5（既定案）Error の case**: 決定: Phase 2 の `NoBundle`・`NotAuthorized` も Phase 1 から union に入れる。採らない案: Phase 2 で足す（網羅的な `match` が壊れる）、`Other of i64` の逃げ道を置く（意味のない case が増える）。理由: net.md の「エラーの読み方」と同じく、case の追加を互換でない変更として避ける。
- **D6（既定案）文字列の受け渡し**: 決定: UTF-16 で受け渡す（`initWithCharacters:length:` と、UTF-16LE の `dataUsingEncoding:`・`getBytes:length:`・`String.from_code_units`）。M02 の Phase 2 があれば `getCharacters:range:` でもよいが、結果は同じでなければならない。採らない案: UTF-8（孤立サロゲートと U+0000 で欠ける。現状 4）、`getCharacters:range:` だけにする（M02 の Phase 2 と W36 の Phase 2 を Phase 1 の前提にする）。理由: `string` は UTF-16 なので、変換の失敗が起きない。M02 の Phase 1 だけで書ける。
- **D7（既定案）設定の値の型**: 決定: getter は保存された物体の型（`NSString`・`NSNumber`）を確かめ、違えば `Maybe.None`。`integerForKey:` などの暗黙の変換を使わない。空の key は getter が `None`、setter が何もしない。採らない案: `integerForKey:` の変換をそのまま使う（`"42"` を 42 と読み、無い key と 0 を区別できない）、空の key を `InvalidInput` にする（getter の型が `Result<Maybe<T>, …>` に重くなる）。理由: 値の有無と型の違い（文字列と数。bool と整数はどちらも `NSNumber` なので区別しない）を、利用者が `Maybe` だけで扱える。
- **D8（要承認）非目標との関係の記述**: 決定（推奨）: README の「採用しない改善」の「GUI・Web フレームワークの同梱」の行と native-interop.md:632 の文言は変えない。このパッケージは「OS のサービスの薄い包みで、ウィジェット・レイアウト・描画を持たず、std の外にある」と、パッケージの README とこのリポジトリの案内の文に書く。採らない案: 非目標の行に例外を書き足す（非目標が弱まって見える）、案内を書かない（利用者が見つけられない）。理由: D-67 の「改めないもの」。言語リファレンスはコンパイラの機能だけを書くので、案内は std の外のパッケージとして分けて書く。
- **D9（要承認）Phase 2 の名前と形**: 決定: 仕様の案（`Notifications.request_authorization`・`post`・`Content`・`Pending`）を Phase 2 の着手の前に承認を受けて決める。採らない案: Phase 1 と一緒に決める（M04・M09 の形が出荷される前に名前を固定する）。理由: 完了ハンドラーの受け方は M04 と M09 の出荷した形に依る。
- **D10（承認済み（2026-11-26。D-67 の分岐 1））D-67 の分岐 1 との関係**: 決定: X′ を前提にする（M07 の `extern class` の宣言を呼ぶ）。選ばれなかった案での扱いは、Y′ ならこのチケットを閉じ（AppKit の宣言を Tsuzuri で持たない）、Z′ なら M07 の計画し直しの結果に合わせて計画し直す、だった。
