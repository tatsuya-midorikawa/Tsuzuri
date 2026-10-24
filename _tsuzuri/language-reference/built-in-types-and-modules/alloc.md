# Alloc

`Alloc` は、計数 allocator が記録するヒープの確保・再確保・解放の回数と要求バイト数を読みます。
時間ではなく構造的な性質（「借用の読み取りは確保しない」「Vec の容量は倍増する」など）をテストできます。
名前を書いたプログラムだけが読み込む標準モジュールです。`Alloc` という利用者のモジュール名は予約され、衝突は `E1011` です。

## API

```text
record Stats { allocs: i64, reallocs: i64, frees: i64, bytes: i64 }
Alloc.snapshot :: unit -> Alloc.Stats
Alloc.since    :: Alloc.Stats -> Alloc.Stats
Alloc.measure  :: (unit -> 'a) -> Alloc.Stats * 'a
```

- `snapshot ()` はプログラム開始からの累計です。
- `since start` は以前の snapshot からの差を、フィールドごとの引き算で返します。
- `measure f` は `f ()` を一度呼び、`(差分, 結果)` を返します。戻り値の解放は窓の後です。
- これらの読み取り自体は確保しません。窓は入れ子にでき、外の窓は内の窓の出来事も含みます。

### 引数の順序

`since` が読む対象は最後の唯一の引数 `Stats` です。`start |> Alloc.since` とも書けます。
`snapshot` と `measure` は `Stats` の入力を持ちません。
[標準ライブラリの引数の順序](argument-order.md)に従い、資源ハンドルの例外ではありません。

```tsuzuri
test "a mapped array takes one block" =
    let xs = new [i64](10, \i -> i)
    let start = Alloc.snapshot ()
    let ys = Array.map (\x -> x + 1) (ref xs)
    let used = start |> Alloc.since
    assert (used.allocs == 1 && used.reallocs == 0 && used.frees == 0 && used.bytes == 80)
    assert (ys[9] == 10)
```

## 数える出来事

`@tz.alloc`・`@tz.realloc`・`@tz.free` の呼び出し（ホストの `tsuzuri_alloc` も含みます）を数えます。

| 出来事 | `allocs` | `reallocs` | `frees` | `bytes` |
| --- | --- | --- | --- | --- |
| 確保 `alloc(size)` | +1 | 0 | 0 | +size |
| null から正のサイズへの再確保 | +1 | 0 | 0 | +新サイズ |
| 生きたブロックの正のサイズへの再確保 | 0 | +1 | 0 | +新サイズ |
| 生きたブロックの解放、またはサイズ 0 への再確保 | 0 | 0 | +1 | 0 |
| null の解放 | 0 | 0 | 0 | 0 |

`bytes` は要求の累計であり、再確保では差ではなく新サイズ全体を加えます。
計数 heap の 16 バイトのヘッダーは含めず、解放で減りません。現在量やピーク、リークを表す値ではありません。
各フィールドは `i64` です。内部の符号なし 64-bit 累計をそのまま読むので、2<sup>63</sup> 以上では負になります。

単一スレッドの同じ処理では決定的です。カウンターはプロセス全体で共有され、
`Task`・`Parallel` の worker など、同じ heap を使う他のスレッドの確保も含みます。
C ライブラリ自身の `malloc` など、この heap を通らない確保は数えません。
並行処理の独立した窓や、4 フィールドを同時に読む原子的 snapshot は提供しません。
内部の `Alloc.__counter` は std 専用で、利用者から呼ぶと `E1022` です。
カウンターの観測は確保効果ではなくデバッグ効果です。`pure def` から `snapshot`・`since`・`measure` を
呼ぶと、`E1029`（`reads allocation counters`）で拒否します。読み取り自体が確保しないこととは別の制限です。

## 使える出力

| 出力・コマンド | 条件 |
| --- | --- |
| `tsuzuri test`（native・WASM、`--coverage`、`-g` を含む） | 選んだテストが `Alloc` に届くときだけ counting を自動で連結します |
| `build`・`run` のコード生成 | 明示的な `--allocator counting` が必要です |
| `build` の `--allocator host`・`small` | `Alloc` に届くと `E2000`。counting へ変更します |
| `bench` の時間用実行器 | `E1018`。確保の計測には [bench --allocs](bench.md#確保数を測る) を使います |
| `bench --allocs` の別の計数用実行器 | counting を使い、`Alloc` の読み取りも可能です。時間用実行器の制限は解除しません |
| `script`・`repl` | `E1018`。`--allocator` は受けません |
| `check`・`doc`・`lsp` | 型検査だけで、到達の制限は IR の生成時に検査します |

公開の補助関数が `Alloc` を読むだけでは、通常のビルドへ入りません。
入口・export・選んだテスト・`Drop` などから本当に到達するときに検査します。
既定の allocator は `system` のままで、通常のビルドが黙って計数 heap に切り替わることはありません。
`test --allocator counting` は、`Alloc` を使わないテストにも明示的に counting を連結します。
`run` と `test` が受ける `--allocator` の値は `counting` だけです。

```text
error[E1018]: Alloc reads the counters of the counting allocator, which only tsuzuri test and builds with --allocator counting link, but export `Main.total_allocs` reads it; build with --allocator counting, or call it only from tests and from functions that only tests use
```

計数 heap は原子演算とヘッダーを加えるため、速度の測定には使いません。
`bench --allocs` は時間用と計数用の実行器を分けます。
既存のホスト ABI `tsuzuri_alloc_stats` は従来の 32 バイト・4 フィールド
（`allocations, frees, live_bytes, peak_bytes`）のままです。

## 所有値を測る

`measure` の閉包が捕捉した所有値は、閉包の中では一意ではありません。
`Alloc.measure (\() -> Array.update_at 0 5 xs)` は、捕捉した配列を複製するので確保が 1 回入ります。
所有値を消費する操作は `snapshot` と `since` で測ります。
閉包の環境は `measure` の呼び出し前に作られるので、その確保は窓の外です。

```tsuzuri
test "an owned array is updated in place" =
    let xs = new [i64](4, \i -> i)
    let start = Alloc.snapshot ()
    let ys = Array.update_at 0 5 xs
    let used = Alloc.since start
    assert (used.allocs == 0 && used.reallocs == 0 && used.bytes == 0)
    assert (ys[0] == 5)

test "measure returns the callback result" =
    match Alloc.measure (\() -> new [i64](10, \i -> i)) with
    | (used, xs) ->
        assert (used.allocs == 1 && used.bytes == 80)
        assert (Array.length (ref xs) == 10)
```

窓の中に `Debug.print` や文字列補間を足すと、その確保も数えます。
同じ関数のどこかで局所配列を借用すると、[最後の使用の move](../ownership-and-memory/ownership.md) の対象外になり、
更新ループで毎回複製することがあります。測定のために対象へ借用を足さないでください。
捕捉つき閉包をライブラリ関数に渡す場合、環境の確保はポインター幅などで target に依存します。
`Array.sort` の確保回数は契約に含めません（`Array.sort_in_place` の無確保とは区別します）。

## 標準ライブラリの確保の契約

`tests/fixtures/alloc_contracts/Main.tz` の 42 テストは edition 2026・2027 それぞれで、native と wasm32 の `-O0`・`-O3` を検証します。
値の順は **allocs, reallocs, frees, bytes** です。ポインター依存の配置は出来事だけを検査します。
UTF-16 の既定の文字列表現を対象とし、opt-in の compact 表現のバイト数を約束する表ではありません。
期待値は規則から導き、時間や実行結果を写した値を合否に使いません。
Vec の消費する更新には、両版で同じ契約の `Vec.push_owned` を使います。

| # | 操作・規則 | 値 |
| --- | --- | --- |
| 1 | `new [i64](10, f)`：要素領域 1 つ | 1, 0, 0, 80 |
| 2 | 長さ 0 の動的配列：1 byte のブロック | 1, 0, 0, 1 |
| 3 | `new` のない配列リテラル：スタック | 0, 0, 0, 0 |
| 4 | `new [1i64, 2i64, 3i64]` | 1, 0, 0, 24 |
| 5 | 空配列：`[]` は無確保、`new []` は 1 byte | 0, 0, 0, 0／1, 0, 0, 1 |
| 6 | `Array.map`：10 要素の結果領域 | 1, 0, 0, 80 |
| 7 | `Array.copy`：10 要素のコピー | 1, 0, 0, 80 |
| 8 | `Array.append`：10 + 5 要素 | 1, 0, 0, 120 |
| 9 | `Array.rev`：50 要素の結果領域 | 1, 0, 0, 400 |
| 10 | 配列の sum・fold・添字・length・for | 0, 0, 0, 0 |
| 11 | スライスの借用 | 0, 0, 0, 0 |
| 12 | 配列の生成とスコープ終了 | 1, 0, 1, 80 |
| 13 | 一意な配列の `Array.update_at` | 0, 0, 0, 0 |
| 14 | 最後の使用で配列を再束縛する更新ループ | 0, 0, 0, 0 |
| 15 | 関数内で借用する 100 要素の配列を 10 回更新 | 10, 0, 10, 8000 |
| 16 | 二つの束縛を使う配列の暗黙コピー | 1, 0, 0, 80 |
| 17 | `Array.copy` の直後の `update_at`：コピーだけ | 1, 0, 0, 80 |
| 18 | `Array.filter`：25 要素を Vec へ集め、容量 4→8→16→32 | 1, 3, 0, 480 |
| 19 | `Array.sort_in_place` | 0, 0, 0, 0 |
| 20 | `Vec.empty`：バッファなし | 0, 0, 0, 0 |
| 21 | Vec の最初の 4 push：容量 4 | 1, 0, 0, 32 |
| 22 | 5 push：容量 4→8 | 1, 1, 0, 96 |
| 23 | 100 push：容量 4→8→16→32→64→128 | 1, 5, 0, 2016 |
| 24 | `Vec.with_capacity 100` と 100 push | 1, 0, 0, 800 |
| 25 | `Vec.to_array`：バッファの移動 | 0, 0, 0, 0 |
| 26 | `Vec.length`・`capacity`：メタデータの読み取り | 0, 0, 0, 0 |
| 27 | 配列 3 つを持つ Vec | 確保 4、再確保 0、解放 0 |
| 28 | 文字列リテラル・長さ・等価比較・検索 | 0, 0, 0, 0 |
| 29 | `to_string 12345`：UTF-16 5 桁 | 1, 0, 0, 10 |
| 30 | `String.repeat`：400 code units | 1, 0, 0, 800 |
| 31 | 文字列補間：400 + 1 + 400 code units | 1, 0, 0, 1602 |
| 32 | 所有文字列へ 3 回追記：401・402・403 code units | 0, 3, 0, 2412 |
| 33 | `String.trim`：556 code units | 1, 0, 0, 1112 |
| 34 | record・tuple・Maybe・非再帰 union | 0, 0, 0, 0 |
| 35 | 再帰 union の 3 ノード | 構築は確保 3、構築と解放は確保 3・解放 3 |
| 36 | 配列を持つ Maybe：配列だけ | 1, 0, 0, 32 |
| 37 | 配列の `Rc.new` と `Rc.share` | new は確保 2、share は 0, 0, 0, 0 |
| 38 | 外へ逃げない局所閉包 | 0, 0, 0, 0 |
| 39 | snapshot 直後の since | 0, 0, 0, 0 |
| 40 | measure 内の 10 要素の map | 1, 0, 0, 80 |
| 41 | 無確保の measure | 0, 0, 0, 0 |
| 42 | 内側と外側の窓 | 内側 1, 0, 0, 32／外側 2, 0, 0, 64 |

規則の出典は [配列](array.md)・[Vec](vec.md)・[文字列](string.md)・[Rc](rc.md)・
[スタックとヒープ](../ownership-and-memory/stack-and-heap.md)・[所有権](../ownership-and-memory/ownership.md) です。
Map・Set・HashMap・Task・Parallel の確保数、リーク検査、確保場所の報告はこの契約の対象外です。
