# M12: ウィンドウへの GPU の表示（バンドルの中の GPU ランタイムの探索と CAMetalLayer への提示）

| 項目 | 内容 |
| --- | --- |
| ID | M12 |
| 優先度 | P3（GPU の計算を使う `.app` と、GPU でウィンドウに描くアプリだけが要る。今の回避策は、Phase 1 が環境変数 `TSUZURI_VULKAN_LIBRARY`・`TSUZURI_WEBGPU_LIBRARY`、Phase 2 が C・ObjC・Swift のシムである） |
| 規模 | L（Phase 1・2。人間が求めない限り Phase 1 だけを実装する） |
| 依存 | ([NC13](NC13-tensor-simd-parallel-and-gpu-kernels.md)), ([M01](_completed/M01-objc-class-types.md)), ([M02](_completed/M02-objc-typed-message-send.md)), ([M15](M15-macos-app-bundle.md))（M01 は Phase 2 の前提。M15 は同梱の手順の文書だけ） |
| 後続 | – |
| 状態 | todo |
| 起票 | 2026-11-26（D-67。利用者の依頼「「どのチケットにもない壁」と「調査中に見つけたコンパイラの不具合（自分で再現済み）」のそれぞれについて @_features/ に開発チケットまたは修正チケットを作成して」から起票。調査で報告した「壁 5（Gpu は計算専用で、ウィンドウへの表示がない。GPU のランタイムが Homebrew の絶対パスを dlopen するので、.app に同梱した形にならない）」に当たる。HEAD `804b7970` で確認） |
| 承認 | 要承認: Phase 1 の D1（macOS の既定の探索の順に、実行ファイルから求めた `../Frameworks` を足す。macOS で Gpu を使うプログラムのランタイムの object が変わるので、D-67 の「1 byte も変えない」の例外としても諮る。「変えてはいけない意味」）。Phase 2 の API の名前（D-55。D6）と、NC13 の D2（Metal）との関係（D7）。D2〜D5・D8〜D10 は既定案。Phase 1 は D-67 の分岐 1 に依らない。Phase 2 の layer の受け方は分岐 1 に従う。分岐 1 は X′ で承認済み（2026-11-26）なので、D6 の (a)（`ref ObjC.Object`）にする（[GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22)） |
| 改善する劣位 | Rust（wgpu と winit の surface、アプリに同梱した MoltenVK）・Swift（MetalKit の `MTKView`）・C++（SDL・GLFW の Metal の layer）比: Tsuzuri の `Gpu` は計算だけで、結果をウィンドウに出す経路がない。`.app` に同梱した GPU のランタイムも探さないので、Homebrew のない Mac に配った `.app` では `Gpu.request` が `Unavailable` になる |
| 手本にする既存実装 | `src/runtime/gpu-vulkan.c` の `tz_vk_find_library`・`tz_vk_open_default`・`TZ_VK_DEFAULT_LIBRARIES_LIST`（プラットフォームの既定の一覧）・`TZ_VK_LIBRARY_DEFAULTS`（テストが候補を差し替える口）、`src/runtime/gpu.c` の `tz_wgpu_load`・`tz_gpu_open_default`・`TZ_GPU_WGPU_DEFAULTS`（テストの口）、`gpu.c:61-65` の `TSUZURI_BLOCKING_HOOKS_DEFINED`（同じ翻訳単位に入るランタイムの部品のうち、先に来たほうが定義する形）、`src/runtime/backtrace.c` の `<mach-o/dyld.h>`（ランタイムが dyld の API を使う前例）、`tests/gpu_vulkan.mjs` の「loader: the search skips a library that is no Vulkan loader and finds the next candidate」と `tests/gpu_runtime.mjs` の作業ディレクトリの検査（印のファイルで読み込みを見る形）。Phase 2 は `src/llvm_gpu.rs` の `runtime_source`（IR のマーカーでランタイムの C を選ぶ形）と、[NC13](NC13-tensor-simd-parallel-and-gpu-kernels.md) の Phase の書き方 |
| 主な影響ファイル | Phase 1: `src/runtime/gpu-vulkan.c`, `src/runtime/gpu.c`, `tests/gpu_vulkan.mjs`, `tests/gpu_runtime.mjs`, `tests/gpu_vulkan_language.mjs`, `docs/architecture.md`, `docs/language.md`, `_tsuzuri/language-reference/built-in-types-and-modules/gpu.md`。Phase 2: `std/Gpu.tz`, `src/llvm_gpu.rs`, `src/check.rs`, `src/capabilities.rs`, `src/driver.rs`, `tests/std_argument_order.rs`, `tests/gpu_present.mjs`（新規）, `tests/gpu_present_host.m`（新規） |

## 実装者向け早見

- **人間が求めない限り Phase 1 だけを実装する。** Phase 2 は、人間が求め、D6・D7 の承認が記録され、NC13 の D2 が決まり、M01 が done になってから着手する。
- 範囲（Phase 1）: macOS の GPU のランタイムの既定の探索に、`.app` の中の `Contents/Frameworks` を足す。実行ファイルの実のパス（`_NSGetExecutablePath` と `realpath`）が `<名前>.app/Contents/MacOS/<ファイル>` のときだけ、`Contents/Frameworks/libvulkan.1.dylib`・`libMoltenVK.dylib`（Vulkan）と `libwgpu_native.dylib`（WebGPU）の絶対パスを、Homebrew の絶対パスより先に試す。環境変数の上書きは今のまま最優先。Linux と Windows は変えない。
- 範囲（Phase 2、要承認）: `Gpu.Buffer<i32u>`（1 画素 1 要素の BGRA8）を、CAMetalLayer の drawable に写して提示する API（案 `Gpu.Surface`・`Gpu.surface`・`Gpu.resize`・`Gpu.present`）。Vulkan は `VK_EXT_metal_surface` とスワップチェーン、WebGPU は Metal の layer の surface と `wgpuSurfaceConfigure`。使えなければ `Result.Error Gpu.Unavailable` で、暗黙の代替をしない。
- 最初に読む: ① 「現状」の再現 ② `src/runtime/gpu-vulkan.c:913-978` ③ `src/runtime/gpu.c:106-160`・`646-680` ④ `tests/gpu_vulkan.mjs:333-360`・`tests/gpu_runtime.mjs:420-450`。
- 最初に書くテスト: `tests/gpu_vulkan.mjs` の「loader: a library in the bundle's Frameworks comes before the defaults」（harness を一時の `.app` に置き、模擬のローダーを `Contents/Frameworks/libvulkan.1.dylib` に置く）。
- 触ってはいけない: 環境変数の意味（設定すればそのパスだけ、空なら無効）、作業ディレクトリを探さない規則、`name[0] == '/'` の不変条件、Linux と Windows の探索、wgpu-native の版の検査、Gpu を使わないプログラムと Gpu を使うプログラムの IR。
- 完了の見分け方（Phase 1）: 「現状」の `Demo.app` を環境変数なしで動かすと、`DYLD_PRINT_LIBRARIES=1` が `<dir>/Demo.app/Contents/Frameworks/libMoltenVK.dylib` を示し、終了コード 24 を返す（今は Homebrew の `libvulkan` と `libMoltenVK` を読む）。

## 目的

`Gpu.request Gpu.Vulkan` と `Gpu.request Gpu.WebGpu` は、GPU のランタイムを実行時に `dlopen` で読む。macOS で試す候補は Homebrew の絶対パス（`/opt/homebrew/lib`・`/usr/local/lib`）だけである。開発機では動くが、Homebrew のない利用者の Mac に配った `.app` では `Unavailable` になる。`.app` の `Contents/Frameworks` に MoltenVK や wgpu-native を同梱しても、ランタイムはそれを探さない。今の回避策は環境変数で、`.app` を Finder から開く利用者には設定させられない。

Phase 1 は、`.app` に同梱したランタイムを探すようにする。作業ディレクトリを探さない今の安全の規則は保ち、候補は実行ファイルの実のパスから求めた絶対パスだけにする。

Phase 2 は、Gpu で計算した画素をウィンドウに出す最小の経路を足す。今の `Gpu` は計算専用で（gpu.md:4-6）、描画のパイプライン・サーフェス・スワップチェーンを扱うチケットは NC13 を含めてない。Phase 2 は描画の一般の API を足さず、画素の buffer を CAMetalLayer に提示することだけを扱う。

## 着手条件と停止条件

### 着手条件

- `node scripts/repo-kb.mjs brief M12` を読む。
- **人間が求めない限り Phase 1 だけを実装する。** 承認が記録されるまで、該当する Phase に着手しない。Phase 1 は D1 の承認が要る。Phase 2 は、人間が求めたこと、D6・D7 の承認、D-55 の登録（`Gpu.Surface`・`Gpu.surface`・`Gpu.resize`・`Gpu.present`）、NC13 の D2 の決定、M01 が done であること（D6 の (a)）が要る。
- P3 のチケットなので、着手前にその時点のコードと台帳でこのチケットをレビューし、指摘を反映してから実装する（GUIDE §0 の 2）。
- Phase 2 の layer の受け方（D6）は (a)（`ref ObjC.Object`）にする。D-67 の分岐 1 は X′ で承認済み（2026-11-26）なので、Y′ のときの案だった (b)（組み込みの `Gpu.MetalLayer`）は採らない。M01 が todo のうちは Phase 2 に着手しない（README の状態で確かめる）。
- macOS の arm64 の実機と、Homebrew の `molten-vk`・`vulkan-loader`・`wgpu-native` があること（実デバイスの検査に使う。なければその検査を飛ばす）。起票時は MoltenVK 1.4.2、vulkan-loader 1.4.363、wgpu-native 29.0.1.1（版 0 を返すビルド）だった。
- 「現状」の再現を着手時の `main` で実行し直す。食い違えば、このチケットを直してから始める。
- 基準の保存: `tests/fixtures/*/Main.tz` と `examples/*/Main.tz` のうち Gpu を使うものの `--emit llvm`（native、`aarch64-apple-darwin`、`x86_64-unknown-linux-gnu`）を `target/scratch/m12/base/` に保存する。Linux の機械では、同じ fixture の `--emit object`（`-O0`・`-O3`）も保存する（Linux の object を変えないことの基準）。

### 停止条件

- **S1** Linux か Windows の探索の挙動か object、Gpu を使わないプログラムの IR か object、または Gpu を使うプログラムの IR が変わる。macOS の Gpu を使うプログラムのランタイムの object が変わるのは「変えてはいけない意味」の例外で、これに当たらない。
- **S2** bundle の候補を求めるのに、CoreFoundation・Foundation のリンクか、リンクの行の変更が要る。
- **S3** テストで、`.app` の外の `../Frameworks`、作業ディレクトリ、相対パスのライブラリが読み込まれる。
- **S4**（Phase 2）表示に、利用者に見える描画のパイプラインの一般の API（シェーダー・テクスチャー・頂点の指定）が要るとわかった。ランタイムの中に閉じた固定の写しのパス（D9）は、これに当たらない。
- **S5**（Phase 2）ウィンドウのない CAMetalLayer への提示を、CI で決定的に確かめられない（`nextDrawable` が nil を返す、提示が完了しない）。
- **S6**（Phase 2）Vulkan と WebGPU で同じ意味（画素の形式、向き、失敗の扱い）にできない。
- **S7** GUIDE-LITE §9 の共通の条件。

## 現状（HEAD `804b7970` で確認）

### 文書と決定

- gpu.md:4-6: `Gpu` は実験的で、計算（`init`・`map`）だけを扱う。`Cuda`・`Metal` の要求は失敗する（`Unavailable`）。gpu.md:517-533 の「まだないもの」にも描画とサーフェスはない。
- gpu.md:142（WebGPU）と gpu.md:267（Vulkan）: 環境変数がなければ決まった絶対パスだけを探し、作業ディレクトリは探さない。macOS の `dlopen` はディレクトリのない名前を作業ディレクトリから探すので、置かれたライブラリの初期化処理が動いてしまうから、という理由が書いてある。
- `docs/architecture.md:477`（WebGPU）・`484`（Vulkan）、`docs/language.md:3335`（WebGPU）・`3352`（Vulkan）も同じ探索を書く。
- NC13 は `Metal` を「可能なら検討」にとどめる（NC13:27）。`Cuda`・`Metal` は `Error Unavailable`（NC13:55）。D2（要承認）は、Metal のネイティブのバックエンドを「端から端までの実測優位性と保守コストを確認したうえで着手を判断する」（NC13:151）。
- `std/Gpu.tz:1-4`: `union Backend = CpuReference | WebGpu | Vulkan | Cuda | Metal | Auto`、`union Error = Unavailable`、`record Device { backend: Backend }`、`record Buffer<'a> { values: ['a] }`。`tests/std_argument_order.rs:100` は `("Gpu", subject(&["Buffer"]))`。

### コード

- `src/runtime/gpu-vulkan.c:913-951`: 既定の候補の規則。macOS（935 行の `#if defined(__APPLE__)`）では `tz_vk_open_default` が `name[0] == '/'` のときだけ開き（936 行）、候補は `/opt/homebrew/lib` と `/usr/local/lib` の `libvulkan.1.dylib`・`libvulkan.dylib`・`libMoltenVK.dylib`（937〜939 行の `TZ_VK_DEFAULT_LIBRARIES_LIST`）。946〜951 行の `TZ_VK_LIBRARY_DEFAULTS` はテストが候補の一覧を差し替える口である。
- `src/runtime/gpu-vulkan.c:953-978` の `tz_vk_find_library`: `TSUZURI_VULKAN_LIBRARY` があればそれだけを開く（空なら無効）。なければ候補を順に開き、`vkGetInstanceProcAddr` を持たないものは `dlclose` して次へ進む。読み込めたライブラリが後でインスタンスを作れなくても、次の候補には戻らない。
- `src/runtime/gpu.c:138`: `tz_gpu_open_default` も `path[0] == '/'` のときだけ開く。macOS の候補は `/opt/homebrew/lib/libwgpu_native.dylib` と `/usr/local/lib/libwgpu_native.dylib`（150 行）。155〜160 行の `TZ_GPU_WGPU_DEFAULTS` がテストの口。`tz_wgpu_load`（646 行）は `TSUZURI_WEBGPU_LIBRARY` を優先し、版 0 を返すライブラリは環境変数で名指ししたときだけ受け入れる（669〜679 行）。既定の候補の loop は最初に `dlopen` できたもので止まり、その後の版の検査で拒んでも次の候補には戻らない（656 行）。
- `src/llvm_gpu.rs:323` の `runtime_source`: IR にマーカー `; tsuzuri-gpu: vulkan`（18 行の `VULKAN_MARKER`）があれば、`#define TZ_GPU_VULKAN 1` と `gpu-vulkan.c` を `gpu.c` の前に置いて 1 つの翻訳単位にする。なければ `gpu.c` だけ。
- `src/runtime/backtrace.c:20-22` は Apple で `<mach-o/dyld.h>` を include する。`_NSGetExecutablePath`・`realpath` は libSystem にあり、新しいリンクは要らない。`grep -rn _NSGetExecutablePath src/runtime` は 0 件。
- サーフェスの扱いはない。`src/runtime/gpu.c` の `surface` は `WGPUSurface` の typedef（190 行）とアダプタの要求の `compatibleSurface`（279 行）だけで、Vulkan の側には 0 件。

### 再現 1: `.app` に同梱した MoltenVK を読まない

`target/scratch/m12/vk/Main.tz`。コマンドは `target/scratch/m12` で実行する（`<dir>` はその絶対パス）:

```tsuzuri
def main :: unit -> i32 = \() ->
    match Gpu.request Gpu.Vulkan with
    | Result.Ok device ->
        let values = Gpu.to_array (Gpu.init device 4 (\index -> index * 2 + 3))
        values[0] + values[1] + values[2] + values[3]
    | Result.Error Gpu.Unavailable -> 99
```

```sh
$ tsuzuri build vk -o vkapp && ./vkapp; echo $?
24
$ mkdir -p Demo.app/Contents/MacOS Demo.app/Contents/Frameworks
$ cp vkapp Demo.app/Contents/MacOS/demo
$ cp /opt/homebrew/opt/molten-vk/lib/libMoltenVK.dylib Demo.app/Contents/Frameworks/
$ DYLD_PRINT_LIBRARIES=1 Demo.app/Contents/MacOS/demo 2>&1 | grep -iE 'vulkan|molten'
dyld[...]: <AEAFA852-...> /opt/homebrew/Cellar/vulkan-loader/1.4.363.0/lib/libvulkan.1.4.363.dylib
dyld[...]: <95027DCA-...> /opt/homebrew/Cellar/molten-vk/1.4.2/lib/libMoltenVK.dylib
$ TSUZURI_VULKAN_LIBRARY=$PWD/Demo.app/Contents/Frameworks/libMoltenVK.dylib DYLD_PRINT_LIBRARIES=1 \
    Demo.app/Contents/MacOS/demo 2>&1 | grep -iE 'vulkan|molten'
dyld[...]: <95027DCA-...> <dir>/Demo.app/Contents/Frameworks/libMoltenVK.dylib
```

どちらも終了コードは 24。環境変数がないと Homebrew のローダーと MoltenVK を読み、同梱した MoltenVK は読まない。Homebrew のない Mac では、候補のどれも開けず `Unavailable`（99）になる（コードから。Homebrew のない Mac では動かしていない）。`DYLD_*` の変数は、SIP で守られたプログラム（`/usr/bin/perl`・`/usr/bin/env` など）を間に挟むと消える。シェルから直接動かす。

### 再現 2: WebGPU も同じで、Homebrew の wgpu-native は版 0 を返す

同じプログラムの `Gpu.Vulkan` を `Gpu.WebGpu` にした `wg/Main.tz` を `tsuzuri build wg -o wdemo` でビルドし、同じ `.app` に置く。Homebrew の `libwgpu_native.dylib`（`/opt/homebrew/opt/wgpu-native/lib/libwgpu_native.dylib`）を `Contents/Frameworks` に写す:

```sh
$ TSUZURI_GPU_DEBUG=1 Demo.app/Contents/MacOS/wdemo; echo $?
tsuzuri gpu: webgpu: the library reports no version; set TSUZURI_WEBGPU_LIBRARY to use a build of wgpu-native 29
99
$ TSUZURI_WEBGPU_LIBRARY=$PWD/Demo.app/Contents/Frameworks/libwgpu_native.dylib Demo.app/Contents/MacOS/wdemo; echo $?
24
```

`DYLD_PRINT_LIBRARIES=1` では、1 つ目は `/opt/homebrew/Cellar/wgpu-native/29.0.1.1/lib/libwgpu_native.dylib` を、2 つ目は同梱したものを読む。

### 再現 3: hardened runtime と ad-hoc 署名

```sh
$ cp vkapp hr && codesign -s - --options runtime -f hr
hr: replacing existing signature
$ TSUZURI_GPU_DEBUG=1 ./hr; echo $?
tsuzuri: Vulkan: no Vulkan loader library was found
tsuzuri: Vulkan: the Vulkan library cannot be loaded
99
```

Homebrew のライブラリ（`codesign -dv` で `Signature=adhoc`、`TeamIdentifier=not set`）も、ライブラリの検証で拒まれる。拒まれた理由はランタイムの報告に出ない。同じ `dlopen` を C の小さなプログラム（`codesign -s - --options runtime` で署名）で呼ぶと、`dlerror()` は `... not valid for use in process: mapping process and mapped file (non-platform) have different Team IDs` を返す。配る `.app` では、同梱したライブラリを同じ Team ID で署名する必要がある（M16）。

### 同梱するライブラリの性質（`otool`・`nm` で確かめた）

- `libMoltenVK.dylib`（1.4.2）は `vkGetInstanceProcAddr`・`vkCreateMetalSurfaceEXT`・`vk_icdGetInstanceProcAddr` を export し、依存はシステムの framework（IOSurface・CoreGraphics・AppKit・IOKit・Metal・Foundation・QuartzCore・CoreFoundation）と `libc++`・`libSystem`・`libobjc` だけ（`otool -L`）。単独で同梱でき、ICD の manifest は要らない。
- `libvulkan.1.dylib`（vulkan-loader 1.4.363）は `CFBundleGetMainBundle`・`CFBundleCopyResourcesDirectoryURL` を import する。bundle の `Resources` の `vulkan/icd.d` の manifest を探すとみられるが、起票時に動かしていない。ローダーだけを同梱して manifest がなければ、ドライバが見つからず `Unavailable` になり、上のとおり次の候補（MoltenVK）には戻らない。
- `libwgpu_native.dylib`（29.0.1.1）の依存は `libSystem`・QuartzCore・Metal・Foundation・CoreFoundation・`libobjc`・`libiconv` で、`wgpuInstanceCreateSurface`・`wgpuSurfaceConfigure`・`wgpuSurfaceGetCurrentTexture`・`wgpuSurfacePresent`・`wgpuSurfaceGetCapabilities`・`wgpuQueueWriteTexture` を export する。

### Phase 2 の前提を試した結果（ObjC の試作。Tsuzuri のコードではない）

- MoltenVK: ウィンドウに入れない `[CAMetalLayer layer]` から `vkCreateMetalSurfaceEXT`（インスタンスの拡張 `VK_KHR_surface`・`VK_EXT_metal_surface`・`VK_KHR_portability_enumeration`）で surface を作れた。`vkGetPhysicalDeviceSurfaceCapabilitiesKHR` の `supportedUsageFlags` は `0x1f`（`TRANSFER_DST` を含む）、画像は 2〜3 枚。最初の形式は 44（`VK_FORMAT_B8G8R8A8_UNORM`、色空間 `SRGB_NONLINEAR`）。キューの族 0 が提示できる。スワップチェーンへの複写と提示は動かしていない。
- wgpu-native: `WGPUSurfaceSourceMetalLayer` で surface を作れた。`wgpuSurfaceGetCapabilities` の `usages` は `0x11`（`CopySrc`・`RenderAttachment`。`CopyDst` がない）、形式は `BGRA8UnormSrgb`・`BGRA8Unorm`・`RGBA16Float`・`RGB10A2Unorm`、提示の方式は 2 つ。`CopyDst` を付けて構成し（能力の外）、`wgpuQueueWriteTexture` と `wgpuSurfacePresent` を 3 回呼ぶと、`wgpuSurfacePresent` は 3 回とも成功を返し、`nextDrawable` を数える CAMetalLayer の派生クラスは 3 回を数えた。画素が届いたかは確かめていない。能力に `CopyDst` がないので、仕様に沿う実装は `RenderAttachment` で描く（D9）。
- Metal の計算は `objc_msgSend` の宣言だけで動いた（`MTLCreateSystemDefaultDevice`・`newLibraryWithSource`・`dispatchThreads` で `out=3 5 7 9`。調査の検証の結果で、起票時に再実行していない）。Metal を直接使う提示は NC13 の D2 に属する（D7）。

## 変えてはいけない意味

- `TSUZURI_VULKAN_LIBRARY`・`TSUZURI_WEBGPU_LIBRARY` の意味（設定すればそのパスを書いたとおりに試し、空なら無効）と、それが最優先であること。
- 作業ディレクトリと、ディレクトリのない名前を探さないこと（既定の候補は絶対パスだけ）。
- `vkGetInstanceProcAddr` を持たないライブラリを読み飛ばすこと。版 0 の wgpu-native を、環境変数で名指ししたときだけ受け入れること。
- Linux・Windows の探索、Gpu を使わないプログラムの IR と object、Gpu を使うプログラムの IR と Linux・Windows の object。
- 失敗した明示的な要求を CPU に切り替えないこと（Phase 2 の提示も同じ）。
- 例外（Phase 1）: macOS で Gpu に到達するプログラムのランタイムの object は変わる（`gpu-vulkan.c`・`gpu.c` の Apple の節に足す探索のコード）。利用者が新しい機能を書かなくても起きる変化なので、[GUIDE D-67](GUIDE.md#d-67-tsuzuri-だけで-macos-のデスクトップアプリを書くm01m22) の「新しい機能を使わないプログラムの IR・object・header・診断・既定の `.wasm` は 1 byte も変えない」の例外にあたる。D-67 の例外の一覧には ③ として挙げてあるが、例外として認めるかは D1 の承認のときに諮る（同じ一覧の ① 分岐 6 と ② M19（分岐 7）の例外は、2026-11-26 に分岐とともに承認された。③ はまだ承認されていない）。到達したときだけの define（M15 の D13 の形）にはできない。`.app` に入れるかどうかはコンパイルの後に決まるからである（「現状」の再現 1 は `--emit exe` の実行ファイルを手で bundle に写した）。`--emit app` のときだけ define を付けると、M15 の D5（中の実行ファイルは `--emit exe` と同じキャッシュの項目とバイト列）が崩れ、手で組んだ bundle では探さない。IR と、bundle の外で動かしたときの挙動は変えない。

## 仕様

### Phase の範囲

| Phase | 内容 | 着手の条件 |
| --- | --- | --- |
| Phase 1 | macOS の既定の探索に `.app` の `Contents/Frameworks` を足す。文書に同梱の手順を書く | D1 の承認 |
| Phase 2 | CAMetalLayer への提示（`Gpu.Surface` など） | 人間が求めること、D6・D7 の承認、D-55 の登録、NC13 の D2 の決定、M01 が done |

### Phase 1: `.app` の中の探索

- **順序**（macOS）: ① 環境変数（今のまま）。② bundle の候補（新規）。③ 今の既定の候補（`TZ_VK_DEFAULT_LIBRARIES_LIST`・`TZ_GPU_DEFAULT_LIBRARIES`、テストでは差し替えた一覧）。
- **bundle の候補を試す条件**: `_NSGetExecutablePath` で得たパスを `realpath` で実のパスにし、その親のディレクトリの名前が `MacOS`、その親が `Contents`、その親の名前が `.app` で終わるときだけ（D3）。どちらかの関数が失敗したとき（パスが `PATH_MAX` を超えるなど）は、② を飛ばして ③ へ進む。
- **bundle の候補**（D2）: `<…>.app/Contents/Frameworks/` に次の名前を付けた絶対パス。`..` を含まない文字列にする。
  - Vulkan: `libvulkan.1.dylib`、`libMoltenVK.dylib` の順。
  - WebGPU: `libwgpu_native.dylib`。
- **扱い**: bundle の候補は、③ の既定の候補と同じ規則で扱う。`vkGetInstanceProcAddr` がなければ読み飛ばして次へ進む。読み込めた候補で探索を終える。WebGPU は `dlopen` できた候補で終わり、その後の版の検査で拒んでも Homebrew の候補には進まない（今の 656 行の loop と同じ）。版 0 の wgpu-native は受け入れない（D4）。
- **診断**: `TSUZURI_GPU_DEBUG` が空でないとき、bundle の候補のファイルがあるのに `dlopen` が失敗したら、その理由を 1 行出す（D5）。Vulkan は今の `tz_vk_debug` の形で `tsuzuri: Vulkan: <path> cannot be loaded: <dlerror()>`、WebGPU は `tz_gpu_report` の形で `tsuzuri gpu: webgpu: <path> cannot be loaded: <dlerror()>`。ファイルがない候補は、今の既定の候補と同じく何も出さない。`TSUZURI_GPU_DEBUG` がないときの出力は変えない。
- **macOS 以外**: Linux・Windows は変えない（コードは `#if defined(__APPLE__)` の中だけで、Linux・Windows の前処理の後のトークンの並びも変えない。`gpu.c`・`gpu-vulkan.c` は `assert`・`__LINE__` を使わないので、行がずれても object は変わらない）。
- **署名**: hardened runtime の下では、同梱したライブラリは実行ファイルと同じ Team ID で署名されていなければ `dlopen` が失敗する（再現 3）。ランタイムはライブラリの検証を回避しない（`com.apple.security.cs.disable-library-validation` を勧めない）。

### Phase 2: CAMetalLayer への提示（要承認）

名前と形は D6 の要承認の案である。

```tsuzuri
// std/Gpu.tz に足す（案）
record Surface      // 不透明。Copy でない。drop でランタイムの surface を解放し、layer の参照を返す
def surface :: ref Device -> own i64 -> own i64 -> ref ObjC.Object -> Result<Surface, Error>   // D6 (a)
def resize :: own i64 -> own i64 -> ref mut Surface -> Result<unit, Error>
def present :: ref Buffer<i32u> -> ref mut Surface -> Result<unit, Error>
```

- `Gpu.surface device width height layer`: `device` が `Vulkan` か `WebGpu` のデバイスで、`layer` が CAMetalLayer なら、幅 `width`・高さ `height` 画素の surface を作る。`CpuReference`・`Auto` のデバイスは `Result.Error Unavailable`（D8）。`layer` が CAMetalLayer でなければトラップする（`objc_opt_isKindOfClass` で確かめる。契約違反で、`Gpu.Error` に場合を足さない）。`width`・`height` が 1 未満ならトラップ、デバイスか surface の上限を超えれば `Result.Error Unavailable`。ランタイムは surface が生きている間 layer の参照を 1 つ持つ。M01 は基底の型への暗黙の変換をしないので、利用者が `ObjC.upcast (ref layer)` で `ref ObjC.Object` にして渡す（COMMON の M01 の綴り）。4 つ目の引数を `Gpu.MetalLayer` にする (b) は、D-67 の分岐 1 で Y′ が選ばれたときの案で、X′ に決まったので採らない（D6）。
- `Gpu.resize width height surface`: スワップチェーン（WebGPU は surface の構成）を作り直す。ウィンドウの大きさが変わったときに呼ぶ。
- `Gpu.present pixels surface`: `pixels` の要素は 1 画素で、値は `0xAARRGGBB`（little endian のメモリの並びは B・G・R・A。`BGRA8Unorm`）。並びは左上から行ごと。要素の数が `width * height` でなければトラップ。drawable を 1 つ得て画素を写し、提示する。提示の方式は FIFO で、drawable が空くまで（最大でおよそ 1 フレーム）待つ。surface が今は提示できないとき（古くなった、失われた、時間切れ）は `Result.Error Unavailable` で、プログラムは `Gpu.resize` してからやり直す。デバイスを失ったときは、今のランタイムの実行時のエラーと同じく理由を出してトラップする。
- **色**: 両方のバックエンドで `BGRA8Unorm` と sRGB の非線形の色空間で提示し、値を変換しない（ガンマの変換をしない）。
- **意味の一致**: Vulkan と WebGPU で、画素の形式・向き・失敗の扱いを同じにする（S6）。
- **target**: wasm32・wasm64 の build で `Gpu.surface`・`Gpu.resize`・`Gpu.present` に到達したら `E2000`。Apple でない native の target では `E2002`（`platform_unsupported_api`）。`check` は受理する。(b) を採らないので、`Gpu.MetalLayer` を `extern` の結果に書く形と、その `E1008`（wasm は `abi_native_only`、Apple でない native は `abi_apple_only`）は足さない。
- **権限**: 3 つとも権限 `gpu`（`src/capabilities.rs` の `explicit` に足す）。
- **スレッド**: `Surface` は所有の値で、tzroutine へ移せる（Send）。共有はできない。提示はランタイムの実行のロックで 1 回ずつ順に動く。layer を作ってビューに付けるのは利用者の AppKit のコードで、メインスレッドで行う（M09）。

## 設計

### Phase 1: 探索（`src/runtime/gpu-vulkan.c`・`src/runtime/gpu.c`）

- Apple だけの補助を 1 つ足す: `static int tz_gpu_bundle_library(const char *name, char *out, size_t size)`。`_NSGetExecutablePath`（`PATH_MAX` の領域）→ `realpath` → 末尾の 3 つの要素（`<ファイル>`・`MacOS`・`Contents`）と `.app` の接尾辞を確かめる → `<Contents>/Frameworks/<name>` を `out` に書いて 1 を返す。条件に合わなければ 0。`#include <mach-o/dyld.h>`・`<limits.h>`・`<unistd.h>`（`access`）も `#if defined(__APPLE__)` の中に置く。[M15](M15-macos-app-bundle.md) の `Env.executable_path`（`os.c` の `#if defined(TZ_OS_EXECUTABLE_PATH)` の中の入口 `tsuzuri_os_executable_path`。到達したプログラムでだけコンパイルされる。M15 の D13）も macOS で同じ `_NSGetExecutablePath` と `realpath` を使う。ランタイムの部品は使うときだけ入るので関数は共有しないが、`realpath` を通してから使う規則は両方でそろえる（後に入るチケットが先の実装に合わせる）。失敗したときの扱いは用途で違う。M12 は ② を飛ばし、M15 は `Os.Error` を返す。領域も違い、M12 は `PATH_MAX` の固定の領域、M15 は返された長さで取り直す。
- 置き場所: 同じ定義を `gpu-vulkan.c` と `gpu.c` の両方の Apple の節に置き、`#ifndef TZ_GPU_BUNDLE_LIBRARY_DEFINED` / `#define TZ_GPU_BUNDLE_LIBRARY_DEFINED` で守る。翻訳単位で先に来たほうが定義する（`gpu.c:61-65` の `TSUZURI_BLOCKING_HOOKS_DEFINED` と同じ形）。ランタイムの C は 3 つの形で翻訳される。プログラムの実行ファイルでは `runtime_source` が `gpu-vulkan.c` を `gpu.c` の前に置く（WebGPU だけのプログラムは `gpu.c` だけ）。`tests/gpu_vulkan_runtime.c:41` は `gpu-vulkan.c` だけを include する。`tests/gpu_runtime.mjs` の C のホストは `gpu.c` だけをビルドする（365 行）。どの形でも定義はちょうど 1 つになる。採らない形: `gpu-vulkan.c` に `static` の前方宣言だけを置き、定義を `gpu.c` に 1 つ置く（`tests/gpu_vulkan_runtime.c` のハーネスでは定義がなくなり、Phase 1 のテストがビルドできない）。新しいランタイムのファイルは足さない（足すなら `scripts/check-runtime-includes.sh` と `include_str!` を合わせる）。
- `tz_vk_find_library`: 環境変数の処理の後、既定の候補の loop の前に、Apple なら bundle の 2 つの候補を、loop の本体と同じ規則（`vkGetInstanceProcAddr` の検査と読み飛ばし）で試す。足すのは `#if defined(__APPLE__)` の中の呼び出し 1 つと、Apple の節の中の関数だけにする。今の loop は括り出さない。括り出すと Linux・Windows の前処理の後のトークンの並びが変わり、Gpu を使うプログラムの Linux・Windows の object が変わる（`-O0` では関数の呼び出しが残る）。検査の数行は Apple の節の中で重ねて書く。
- `tz_wgpu_load`: 同じく、既定の候補の前に bundle の 1 つの候補を試す（`#if defined(__APPLE__)` の中だけ）。版 0 の扱いは、既定の候補と同じ（`requested == NULL` の枝）。
- 診断（D5）: bundle の候補で `access(path, F_OK) == 0` かつ `dlopen` が NULL のときだけ、`dlerror()` の文を出す。
- 決定性: 候補の順は固定。環境に依存するのは実行ファイルのパスだけ。

### Phase 2: 提示

- **std と検査**: `std/Gpu.tz` に `Surface`（不透明な record。ランタイムの surface の番号を持つ）と `surface`・`resize`・`present`、非公開の組み込み `Gpu.__surface`・`Gpu.__resize`・`Gpu.__present`・`Gpu.__surface_release`（`Drop`）。`tests/std_argument_order.rs:100` を `subject(&["Buffer", "Surface"])` にする（`present` は 2 つの対象のうち書き込む surface を最後に置く。AGENTS.md の Standard library API conventions）。
- **生成**: 組み込みはランタイムの C の関数 `tsuzuri_gpu_surface_create(backend, layer, width, height, &id)`・`tsuzuri_gpu_surface_resize(id, width, height)`・`tsuzuri_gpu_surface_present(id, pixels, count)`・`tsuzuri_gpu_surface_release(id)` を呼ぶ。surface を使うプログラムの IR にマーカー `; tsuzuri-gpu: surface` を置き、`runtime_source` は `#define TZ_GPU_SURFACE 1` を足す。マーカーのないプログラムのランタイムの C は今のまま。
- **Vulkan**: `TZ_GPU_SURFACE` のときだけ、インスタンスに `VK_KHR_surface`・`VK_EXT_metal_surface` を足し（インスタンスは最初の `Gpu.request` で作るので、コンパイル時のマーカーで決める）、論理デバイスに `VK_KHR_swapchain` を足す。どの拡張も、`vkEnumerateInstanceExtensionProperties`・`vkEnumerateDeviceExtensionProperties` の列挙にあるときだけ足す。ないときは足さずに作り、`Gpu.surface` が `Unavailable` を返す（計算の `Gpu.request` を巻き込んで失敗させない）。surface は `vkCreateMetalSurfaceEXT`。選んだ計算のキューの族が `vkGetPhysicalDeviceSurfaceSupportKHR` で提示できなければ `Unavailable`。スワップチェーンは `B8G8R8A8_UNORM`・`SRGB_NONLINEAR`・`FIFO`・`TRANSFER_DST`。提示ごとに、staging の buffer に画素を写し、`vkAcquireNextImageKHR`、`UNDEFINED`→`TRANSFER_DST_OPTIMAL` の遷移、`vkCmdCopyBufferToImage`、`PRESENT_SRC_KHR` への遷移、送信、`vkQueuePresentKHR`。`VK_ERROR_OUT_OF_DATE_KHR`・`VK_SUBOPTIMAL_KHR` は `Unavailable`（D8）。
- **WebGPU**: `TZ_GPU_SURFACE` のときだけ、`webgpu.h` の surface と描画のパスの部分の宣言を足す（今と同じく、必要な部分を wgpu-native 29 の header から写す）。surface は `wgpuInstanceCreateSurface` と `WGPUSurfaceSourceMetalLayer`。能力に `BGRA8Unorm` と `RenderAttachment` がなければ `Unavailable`。構成は `BGRA8Unorm`・`RenderAttachment`・`Fifo`。中間のテクスチャー（`TextureBinding`・`CopyDst`、`BGRA8Unorm`、`width`×`height`）を 1 つ持ち、提示ごとに `wgpuQueueWriteTexture` で画素を書き、ランタイムに埋め込んだ固定の WGSL（画面全体の三角形 1 つで、中間のテクスチャーを `textureLoad` で読む）の描画のパスで surface のテクスチャーに写し、`wgpuSurfacePresent`。surface の能力に `CopyDst` がない（「現状」）ので、写しは描画のパスで行う（D9）。
- **費用**: 1 回の提示で `width * height * 4` byte をホストから GPU へ写す。`Gpu.Buffer` はホストのメモリにある（gpu.md「GPU メモリに常駐するバッファ: 未実装」）ので、Gpu で計算した画素は、計算の読み戻しと提示の書き込みで 2 回境界を越える。性能は計測の後にだけ主張する。常駐するバッファは対象外。

## 前提とする他チケットのインターフェース

- Phase 1: なし。
- Phase 2:
  - [M01](_completed/M01-objc-class-types.md): `ObjC.Object`（`ref ObjC.Object` の引数）と `objc_opt_isKindOfClass` の下げ方（D6 の (a)）。D-67 の分岐 1 が X′ に決まったので D6 の (b) は採らず、M01 が done になってから Phase 2 に着手する。
  - [M02](_completed/M02-objc-typed-message-send.md): 利用者が CAMetalLayer を作り（`CAMetalLayer.layer ()`）、ビューの `setLayer:`・`setWantsLayer:` を呼ぶ型付きの送信。M02 がなければ、利用者は今の `objc_msgSend` の手書きの宣言で layer を作る。
  - [NC13](NC13-tensor-simd-parallel-and-gpu-kernels.md) の D2: Metal のバックエンドの判断（D7）。
- 文書: [M15](M15-macos-app-bundle.md) の `[macos] embed`（`Contents/Frameworks` に写すファイル）と、[M16](M16-macos-signing-notarization-hardening.md) の Developer ID 署名。出荷されていなければ、手で bundle を作る手順と `codesign` の手順を書く。

## 他チケットへ提供するインターフェース

- Phase 1: bundle の `Contents/Frameworks` の GPU のランタイムの名前（`libvulkan.1.dylib`・`libMoltenVK.dylib`・`libwgpu_native.dylib`）。M15 の `embed` の文書の例と、M16 の内側から署名する手順の例に使う。
- Phase 2: `Gpu.Surface`・`Gpu.surface`・`Gpu.resize`・`Gpu.present` と、ランタイムの `tsuzuri_gpu_surface_*`。NC13 が Metal のバックエンドを足すときは、同じ API を Metal で実装する。Windows（HWND）と Linux（Wayland・X11）のサーフェスは、別のチケットが同じ API に layer の代わりの入力を足す。

## 実装手順

### Phase 1

#### 手順 1: 基準と再現

- 変更: なし（`target/scratch/m12/` の下書きだけ）。
- 内容: 着手条件の基準を保存し、「現状」の再現 1〜3 を実行し直す。
- 確認: 再現の出力を記録する。

#### 手順 2: Vulkan の探索

- 変更: `src/runtime/gpu-vulkan.c`、`src/runtime/gpu.c`（補助の定義）。
- 内容: 設計の「Phase 1: 探索」の Vulkan の部分。
- 確認: `node tests/gpu_vulkan.mjs` の新しい検査（「テスト計画」）と既存の検査。`sh scripts/check-runtime-includes.sh`。

#### 手順 3: WebGPU の探索

- 変更: `src/runtime/gpu.c`。
- 内容: 設計の「Phase 1: 探索」の WebGPU の部分。
- 確認: `node tests/gpu_runtime.mjs target/debug/tsuzuri` の新しい検査と既存の検査。

#### 手順 4: 実デバイスと文書

- 変更: `tests/gpu_vulkan_language.mjs`、「ドキュメント」の節のページ。
- 内容: 「テスト計画」の実デバイスの E2E。文書の更新。
- 確認: T2（`node tests/gpu.mjs target/debug/tsuzuri`、`node tests/gpu_vulkan_language.mjs target/debug/tsuzuri` を含む）。手順 1 の基準の IR が `cmp` で一致する。PR に `ci:full` を付ける（macOS の挙動を変える）。

### Phase 2（人間が求めたとき）

#### 手順 5: std と検査

- 変更: `std/Gpu.tz`、`src/check.rs`、`src/capabilities.rs`、`src/driver.rs`、`tests/std_argument_order.rs`。
- 内容: 仕様の型と関数、target の誤り、権限。
- 確認: T0・T1。`tests/std_argument_order.rs` が通る。Gpu を使うが surface を使わないプログラムの IR が手順 1 の基準と一致する。

#### 手順 6: ランタイム

- 変更: `src/llvm_gpu.rs`、`src/runtime/gpu-vulkan.c`、`src/runtime/gpu.c`。
- 内容: 設計の「Phase 2: 提示」の Vulkan と WebGPU。
- 確認: `tests/gpu_present.mjs` の C・ObjC のホストの検査。`TSUZURI_SANITIZE=1` でも通る。

#### 手順 7: 言語の E2E と文書

- 変更: `tests/gpu_present.mjs`、文書。
- 内容: 「テスト計画」の Phase 2 の言語の E2E と文書。
- 確認: T2。PR に `ci:full`。

## テスト計画

### Phase 1

- `tests/gpu_vulkan.mjs`（macOS だけ。ほかの OS では検査を飛ばしたと出す）:
  - 「loader: a library in the bundle's Frameworks comes before the defaults」: `tests/gpu_vulkan_runtime.c` を `-DTZ_VK_LIBRARY_DEFAULTS` に印付きの模擬のローダー（`TZ_VK_MOCK_MARKER`）の絶対パスを渡してビルドし、`<tmp>/T.app/Contents/MacOS/harness` に置く。印のない模擬を `T.app/Contents/Frameworks/libvulkan.1.dylib` に置く。環境変数なしで `probe` すると `Mock Vulkan device` が出て、既定の候補の印のファイルはできない。
  - 「loader: the bundle's MoltenVK name is tried after the loader name」: `Contents/Frameworks/libvulkan.1.dylib` に `vkGetInstanceProcAddr` のない stub、`libMoltenVK.dylib` に模擬を置くと、stub を読み飛ばして模擬を使い、`TSUZURI_GPU_DEBUG=1` で `libvulkan.1.dylib has no vkGetInstanceProcAddr and is skipped` が出る。
  - 「loader: an executable outside a bundle does not look in ../Frameworks」: harness を `<tmp>/plain/bin/harness`、印付きの模擬を `<tmp>/plain/Frameworks/libvulkan.1.dylib` に置くと、印のファイルはできない。`<tmp>/X.notapp/Contents/MacOS/harness` も同じ。
  - 「loader: a symbolic link to the bundle's executable finds the bundle」: `<tmp>/link/harness` を `T.app/Contents/MacOS/harness` への symlink にして動かすと、bundle の模擬を使う（`realpath`）。
  - 「loader: TSUZURI_VULKAN_LIBRARY still comes first」: 環境変数に別の模擬のパスを書くと、bundle の模擬（印付き）は読まれない。空の値は今どおり無効。
  - `codesign` があれば: harness を `codesign -s - --options runtime` で署名し直し、`TSUZURI_GPU_DEBUG=1` で `cannot be loaded:` と `Team IDs` を含む行が出る。
- `tests/gpu_runtime.mjs`（macOS だけ）: `tests/gpu_runtime_host.c` と `gpu.c` を、`-DTZ_GPU_WGPU_DEFAULTS` に存在しない絶対パスを 1 つ渡して（Homebrew の候補を外して）ビルドし、`<tmp>/W.app/Contents/MacOS/` に置く。`-DMOCK_VERSION=0x1D000101u -DMOCK_MARKER` の模擬を `Contents/Frameworks/libwgpu_native.dylib` に置き、`MOCK_LOAD_MARKER` に印のパスを渡して `open` すると、印のファイルができ、今の名指しの検査と同じく状態 2 と `the library lacks wgpu` になる。版 0 の模擬（今の `unversioned`）を置くと、状態 2 と `the library reports no version; set TSUZURI_WEBGPU_LIBRARY to use a build of wgpu-native 29`（D4）。bundle の外の `../Frameworks` は読まない。
- `tests/gpu_vulkan_language.mjs`（macOS で Homebrew の MoltenVK があるとき）: 「現状」の `vk` のプログラムをビルドして一時の `.app` に置き、`libMoltenVK.dylib` を `Contents/Frameworks` に写す。環境変数なしで動かすと 24 を返し、`DYLD_PRINT_LIBRARIES=1` の出力に bundle のパスがあり、Homebrew の `libvulkan` がない。
- 変えないこと: Linux の CI で既存の `gpu_vulkan.mjs`・`gpu_runtime.mjs` の検査がそのまま通る。手順 1 の IR の基準と一致する。Linux の Gpu を使う fixture の `--emit object` が手順 1 の基準と `cmp` で一致する。

### Phase 2

- `tests/gpu_present_host.m`（新規、ObjC）と `tests/gpu_present.mjs`（新規、macOS だけ。MoltenVK・wgpu-native がなければそのバックエンドを飛ばす）:
  - ウィンドウに入れない CAMetalLayer の派生クラス（`nextDrawable` を数え、最後の drawable を保持する）を作り、ランタイムの `tsuzuri_gpu_surface_create`・`present` を 3 回呼ぶ。両方のバックエンドで、3 回とも状態 0、`nextDrawable` が 3 回以上。
  - Vulkan では drawable のテクスチャーを読み戻し（`TRANSFER_DST` の構成で `framebufferOnly` は NO になるはず。着手時に確かめる）、4×2 の画素が `0xAARRGGBB` の並びと向きのとおりか比べる。WebGPU で読み戻せないときは、数だけを確かめ、その理由を記録する（S5 に当たるかを判断する）。
  - 失敗の経路: `CpuReference` のデバイスで `Unavailable`、CAMetalLayer でない物体でトラップ、要素の数の違いでトラップ、大きさ 0 でトラップ。`resize` の後の提示が成功する。
- 言語の E2E（M02 が done のとき。M01 は Phase 2 の前提なので done である）: Tsuzuri のプログラムが CAMetalLayer を作って `Gpu.surface`・`Gpu.present` を呼ぶ。wasm32 の build は `E2000`、`--target x86_64-unknown-linux-gnu` の build は `E2002`。surface を使わない Gpu のプログラムの IR は基準と一致する。

## ドキュメント

- Phase 1:
  - `_tsuzuri/language-reference/built-in-types-and-modules/gpu.md` の「native: wgpu-native を実行時に読み込む」（133 行。探索の段落は 142 行）と「native: Vulkan のローダーを実行時に読み込む」（258 行。探索の段落は 267 行）: 環境変数の次に `.app` の `Contents/Frameworks` を探すこと、その条件（実行ファイルが `Contents/MacOS` にあるとき）、名前と順序。
  - 同じページに節「`.app` に同梱する」: `libMoltenVK.dylib` だけを同梱するのを勧める理由（単独で動き、ICD の manifest が要らない）、ローダーを同梱するなら manifest が要ること（起票時に動かしていないと書く）、Homebrew の wgpu-native は版 0 なので公式のリリース（版 29）を同梱すること、hardened runtime では同じ Team ID で署名すること（ad-hoc では Team ID の違いで拒まれる）、`TSUZURI_GPU_DEBUG` で拒まれた理由が出ること。M15・M16 が出荷されていれば `[macos] embed` と署名の手順へのリンク、されていなければ手で bundle を作る手順。
  - `docs/language.md:3335`・`3352`、`docs/architecture.md:477`（WebGPU の読み込み）・`478`（その検証）・`484`（Vulkan の探索）・`485`（その検証）。
- Phase 2: gpu.md に節「ウィンドウに表示する（macOS）」（API、画素の形式と向き、FIFO の待ち、`Unavailable` の扱い、費用、まだないもの）。「まだないもの」の表を直す。`docs/language.md` の Gpu の節。M02 が出荷されていれば、AppKit のビューに layer を付ける例（M01 は Phase 2 の前提）。
- 変えたページに `node scripts/check-docs.mjs` を通す（GPU の実行が要る例は `run=` を付けない）。

## 受け入れ条件

- [ ] Phase 1: 環境変数がないとき、`.app` の `Contents/Frameworks` の Vulkan と WebGPU のランタイムが Homebrew より先に読まれる（模擬と実デバイスの両方）。
- [ ] Phase 1: `.app` の外の `../Frameworks`、作業ディレクトリ、相対パスは読まれない。環境変数の意味は変わらない。
- [ ] Phase 1: 版 0 の wgpu-native を bundle から受け入れない。`TSUZURI_GPU_DEBUG` で、ファイルがあるのに読めない bundle の候補の `dlerror()` が出る。
- [ ] Phase 1: Linux・Windows の探索と、Gpu を使うプログラムの IR と Linux の object が変わらない（比べた件数を記録した）。macOS のランタイムの object の変化を、D-67 の例外として PR に書いた。
- [ ] Phase 1: 文書を直し、T2 が成功し、PR に `ci:full` を付けた。
- [ ] Phase 2（求められたとき）: 両方のバックエンドで提示が動き、画素の並び・向き・失敗の扱いが同じ。surface を使わないプログラムの IR が変わらない。

## 落とし穴

- `_NSGetExecutablePath` が返すパスは、起動に使ったパスそのもので、相対パスや symlink のことがある。必ず `realpath` を通してから `Contents/MacOS` を確かめる。
- 候補の文字列に `..` を残さない（`<exe のディレクトリ>/../Frameworks` ではなく `<Contents>/Frameworks`）。`TSUZURI_GPU_DEBUG` の行と `DYLD_PRINT_LIBRARIES` の行が実のパスで一致し、テストで比べやすい。
- `@executable_path`・`@rpath` を `dlopen` に渡さない。`name[0] == '/'` の不変条件を崩し、既存のテスト（ディレクトリのない候補を開かないこと）の意味が変わる（D1）。
- ローダー（`libvulkan.1.dylib`）を同梱して manifest を置かないと、ローダーは読めるのにドライバが見つからず `Unavailable` になり、同梱した MoltenVK に戻らない。探索をインスタンスの作成の失敗で次へ進めるように変えない（今の意味を変える）。文書で MoltenVK だけの同梱を勧める。
- 開発機には Homebrew があるので、bundle の候補が壊れていても実デバイスの検査は Homebrew で通ってしまう。実デバイスの検査は `DYLD_PRINT_LIBRARIES` で読んだパスを確かめる。模擬の検査は `TZ_VK_LIBRARY_DEFAULTS`・`TZ_GPU_WGPU_DEFAULTS` で既定の候補を外して行う。
- `DYLD_PRINT_LIBRARIES` は、SIP で守られたプログラム（`/usr/bin/env`・`/usr/bin/perl` など）を間に挟むと消える。テストは node の `spawnSync` の `env` で直接渡す。
- （Phase 2）インスタンスとデバイスは最初の `Gpu.request` で作る。surface を使うかは、そのときには実行時にわからないので、IR のマーカーでコンパイル時に決める。マーカーのないプログラムのインスタンスの拡張を変えない。
- （Phase 2）WebGPU の surface の能力に `CopyDst` がないので、`wgpuQueueWriteTexture` で surface のテクスチャーに直接書かない（試作では成功を返したが、能力の外で、画素が届いたかも確かめていない）。

## 対象外

- 描画のパイプライン・シェーダー・テクスチャーの一般の API（Phase 2 の写しのパスはランタイムの中に閉じる）。GUI フレームワークに近く、D-67 の「改めないもの」に含める。要るアプリは Metal か Vulkan を `extern` で直接呼ぶ。
- 文字の描画。
- Metal のバックエンド（NC13 の D2。`Gpu.request Gpu.Metal` は `Unavailable` のまま）。
- Windows（HWND）と Linux（Wayland・X11）のサーフェス（別のチケット）。
- GPU のメモリに常駐するバッファ（計算の結果を読み戻さずに提示する経路）。
- ライブラリの検証を回避する entitlement を勧めること、同梱したライブラリの署名（M16）、bundle の生成（M15）。

## 決定事項

| ID | 決定 | 理由（採らない案を含む） | 状態 |
| --- | --- | --- | --- |
| D1 | macOS の既定の探索に、実行ファイルの実のパス（`_NSGetExecutablePath` と `realpath`）から求めた `Contents/Frameworks` の絶対パスの候補を足し、環境変数の次、Homebrew の候補の前に試す。macOS で Gpu を使うプログラムのランタイムの object はこのために変わる（D-67 の例外。「変えてはいけない意味」） | 配る `.app` で動くのは、開発者が同梱して署名したライブラリだけである。Homebrew の候補を先にすると、開発機では版の違うライブラリ（版 0 の wgpu-native など）を読み、利用者の Mac とは違う構成で試すことになる。候補を絶対パスに限るのは、今の安全の規則（作業ディレクトリを探さない、`name[0] == '/'`）をそのまま保つため。採らない案: (a) `@executable_path/../Frameworks/...` を `dlopen` に渡す（不変条件を崩し、`TSUZURI_GPU_DEBUG` が実のパスを出せない）。(b) `@rpath` の名前を渡し、実行ファイルの rpath に任せる（rpath はリンクの設定で決まり、Tsuzuri の実行ファイルは今 rpath を持たない。M20 の `--rpath` に結び付く）。(c) CFBundle の API で探す（CoreFoundation のリンクが要る。S2） | 要承認（承認前は Phase 1 の手順 2 以降に着手しない） |
| D2 | bundle の候補の名前と順は、Vulkan が `libvulkan.1.dylib`・`libMoltenVK.dylib`、WebGPU が `libwgpu_native.dylib` | 今の既定の候補と同じ相対の順（ローダー、次に MoltenVK）にする。名前のない版（`libvulkan.dylib`）は Homebrew の symlink の名前で、同梱には要らない。文書では MoltenVK だけの同梱を勧める（単独で動き、manifest が要らない） | 既定案（実装者はこの案に従う） |
| D3 | bundle の候補は、実行ファイルの実のパスが `<…>.app/Contents/MacOS/<ファイル>` のときだけ試す | 任意の場所の実行ファイルの `../Frameworks`（`/usr/local/bin` の実行ファイルなら `/usr/local/Frameworks`）は、アプリの作者が管理する場所とは限らない。bundle の形に限れば、候補は署名の対象の中にある | 既定案（実装者はこの案に従う） |
| D4 | bundle から読んだ wgpu-native が版 0 を返せば、既定の候補と同じく受け入れない | 版の検査は ABI の食い違いを防ぐ。版 0 のビルド（Homebrew のもの）は、ランタイムが ABI を確かめられないものそのもので、同梱しても事情は変わらない。公式のリリースは版 29 を返す。利用者が保証するなら環境変数で名指しできる（今のまま） | 既定案（実装者はこの案に従う） |
| D5 | `TSUZURI_GPU_DEBUG` のとき、ファイルがあるのに読めない bundle の候補について `dlerror()` の文を 1 行出す | hardened runtime と ad-hoc 署名の組み合わせでは、Team ID の違いで拒まれるのに、今の報告は「見つからない」だけになる（再現 3）。ファイルのない候補まで出すと、Homebrew の候補の今の出力と合わず、行が増える | 既定案（実装者はこの案に従う） |
| D6 | Phase 2 の API は `Gpu.Surface`・`Gpu.surface :: ref Device -> own i64 -> own i64 -> ref ObjC.Object -> Result<Surface, Error>`・`Gpu.resize :: own i64 -> own i64 -> ref mut Surface -> Result<unit, Error>`・`Gpu.present :: ref Buffer<i32u> -> ref mut Surface -> Result<unit, Error>`（D-55 の登録）。D-59 に従い、書き込む対象の surface を最後に置く（`Gpu` の `MODULES` の行を `subject(&["Buffer", "Surface"])` にする）。layer は (a) M01 の `ref ObjC.Object` で受け、実行時に CAMetalLayer かを確かめる。D-67 の分岐 1 は X′ で承認済み（2026-11-26）なので、M01 のないときの (b)（組み込みの不透明な Copy の型 `Gpu.MetalLayer` を足し、利用者の `@trusted extern` がそれを返す形。分岐 1 が Y′ を含むときの案）は採らない。Phase 2 は M01 が done になってから着手する | 安全でない部分は、利用者の `@trusted` の宣言（D-63・D-67 の信頼の置き場）に残す。契約の案の「利用者の `extern type copy` で受ける」形は、そのままでは書けない。利用者が宣言した `extern type copy` は利用者のモジュールの型で、std の `Gpu.surface` の型に名前を書けないからである。std の側に `extern type` を置く形も採らない。std は今 `extern` を 1 つも宣言しておらず（`grep -l '^extern' std/*.tz` は 0 件）、その前例をこのチケットで作らない。そこで (b) は組み込みの型として足す案だった（分岐 1 が X′ に決まったので採らない）。採らない案: (c) layer のアドレスを `i64` で受ける（std の関数が任意のアドレスを ObjC の物体として扱い、`@trusted` と `ffi` の監査の外で未定義の動作を起こせる）。`Gpu.Error` に場合を足す案は採らない（利用者の網羅した `match` を壊す） | 要承認（D-55 の登録。分岐 1 は承認済み（2026-11-26）で、layer は (a) に決まった。承認前は Phase 2 に着手しない） |
| D7 | Phase 2 は Vulkan（MoltenVK）と WebGPU（wgpu-native）だけで提示し、Metal を直接使わない。Phase 2 は NC13 の D2 が決まってから着手する。NC13 が Metal のバックエンドを採れば、同じ API の Metal の実装は NC13 の側で足し、Phase 2 はその計画に合わせて見直す | Metal への経路を 2 つ（M12 と NC13）作らない。MoltenVK と wgpu-native は内部で Metal を使うので、macOS の表示はこの 2 つで足りる。Metal の計算が `objc_msgSend` で動くことは、NC13 の判断の材料として渡す | 要承認（承認前は Phase 2 に着手しない） |
| D8 | Phase 2 は暗黙の代替をしない。`CpuReference`・`Auto` のデバイスと、提示できない surface（古い、失われた、時間切れ）は `Result.Error Unavailable`。デバイスの喪失は今の実行時のエラーと同じくトラップ。wasm は `E2000`、Apple でない native は `E2002`（`platform_unsupported_api`） | gpu.md の「明示的な GPU 要求を暗黙に CPU 実行へ切り替えない」と同じ。`Auto` は呼び出しごとに場所を選ぶ仕組みで、提示の先の surface は 1 つのデバイスに結び付くので、`Auto` では作れない。target の誤りは、ほかの OS 専用の API（M09 の組み込みのモジュール `MainThread`）と同じ分類 | 既定案（実装者はこの案に従う） |
| D9 | 画素は `i32u` 1 つで 1 画素の `0xAARRGGBB`（`BGRA8Unorm`、左上から行ごと）。両方のバックエンドで `BGRA8Unorm` と sRGB の非線形の色空間で、値を変換せずに提示する。Vulkan はスワップチェーンの画像へ `vkCmdCopyBufferToImage` で写し、WebGPU は中間のテクスチャーからランタイムの中の固定の描画のパスで写す | MoltenVK の surface の最初の形式は `B8G8R8A8_UNORM` で `TRANSFER_DST` を許し、wgpu-native の surface は `BGRA8Unorm` を持つが `CopyDst` を持たない（「現状」）。どちらも並べ替えなしに同じ byte の並びを使える。描画のパスは利用者に見せないので、S4 には当たらない | 既定案（実装者はこの案に従う） |
| D10 | Phase 1 は macOS だけを変え、Linux・Windows の探索は変えない | Linux の動的リンカーは作業ディレクトリを探さず、soname で探す今の規則で足りる。Windows の同梱（アプリのディレクトリの DLL）は `LoadLibraryExA` の探索の範囲の決定が要り、このチケットの調査の外 | 既定案（実装者はこの案に従う） |
