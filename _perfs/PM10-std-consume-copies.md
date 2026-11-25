# PM10: std の所有を移す関数の要素の複製の除去と、借用の根になる局所の最後の使用での移動（A17 の X1 の精密化）

| 項目 | 内容 |
| --- | --- |
| ID | PM10 |
| 分類 | メモリ |
| 優先度 | P1 |
| 規模 | M（Phase 1 は std の本体の書き換え、Phase 2 は `src/last_use.rs` の X1 の精密化） |
| 依存 | [A17](../_features/_completed/A17-last-use-move.md), [PM07](_completed/PM07-copy-elision.md)（どちらも完了） |
| 関連 | [NC20](../_features/NC20-compound-assignment-and-mutable-index-views.md)（起票元の D13 (a)。`+=`・`IndexMut`・`IndexBorrowMut` の対象の借用の根）、[A39](../_features/A39-field-exclusive-borrow.md)（レコードのフィールドの排他借用。edition 2027 以降）、[NC06](../_features/NC06-tensor-ufuncs-and-elementwise.md)（`*_into` の出力先を `Tensor.update` で用意する計画）、[A15](../_features/_completed/A15-copy-cost-visibility.md)（`W1006`） |
| 状態 | todo |
| 起票 | 2026-11-25（NC20 の D13 (a) の決定。[GUIDE D-66](../_features/GUIDE.md#d-66-自然な添字範囲演算子d-65-の改訂a37nc16nc20)。HEAD `858a949c` の dev バイナリで再測定した） |
| 承認 | 決定（D-66 の委任で決定、2026-11-25）: 起票、範囲、Phase の分け方、型付き IR だけから借用の期間を求めること、文書を先に直すこと（D1・D3〜D7）。言語の意味・構文・診断コードは変えない |
| 改善する劣位 | 文書が「複製しない」「その場で」と書く std の関数が、要素が Copy 型のとき中身を複製する。`Matrix.set` は 1 要素の置き換えなのに要素数に比例し、ループで使うと要素数と回数の積になる。`ref mut out[0..]` で書いてから `out` を返す関数も、配列を 1 回余分に複製する |
| 手本にする既存実装 | 所有する scrutinee の `match` の束縛の移動: A17 の Phase 2（`src/llvm_control.rs` の `match_expression`、規則 M1〜M4）。複製か移動かの分岐: `src/llvm.rs:5919` の `FunctionEmitter::clones_on_take`。最後の使用と除外: `src/last_use.rs` の `LastUses::with_budget`（X1 は 55〜59 行）。結果が loan を持たない呼び出しの判定: `src/constants.rs:308` の `temporary_borrows` と、そこから呼ぶ `Checker::loan_free`（`Type::carries_loans`、`src/check.rs:907`）。差分テスト: `tests/last_use_fuzz.mjs` |
| 主な影響ファイル | Phase 1: `std/Tensor.tz`（`to_array`・`reshape`・`reshape_infer`・`try_reshape`・`to_matrix`・`update` の本体）、`std/Matrix.tz`（`to_array`・`set` の本体）、`tests/fixtures/alloc_contracts/Main.tz`・`tests/alloc_contracts.rs`。Phase 2: `src/last_use.rs`、`tests/last_use.rs`、`tests/last_use_fuzz.mjs`、`tests/fixtures/last_use/Main.tz`・`tests/features.mjs`。両方: `_tsuzuri/language-reference/built-in-types-and-modules/tensor.md`・`matrix.md`・`alloc.md`、`docs/language.md`、`docs/architecture.md`、`std/Tensor.tz`・`std/Matrix.tz` の docstring |
| 計測対象 | `Alloc.since` で数える 1 回の呼び出しの確保の回数とバイト数（`[1000]` と `[2000]` の `i64`）。IR の `call ptr @tz.alloc` の数。時間は主張しない |

## 実装者向け早見

- 範囲: 要素が Copy 型のとき中身を複製する std の 9 つの関数（`Tensor.to_array`・`reshape`・`reshape_infer`・`try_reshape`・`of_matrix`・`to_matrix`・`update`、`Matrix.to_array`・`set`）の複製をなくす。決定の記録は 7 つを挙げた。`reshape_infer`・`try_reshape` は再測定で見つけた同じ形の関数である。
- 原因は 3 つある（「現状と計測」）。(a) 非 Copy の根（`Tensor.Tensor`・`Matrix.Matrix`）のフィールドの射影は、単一使用でも移動しない（PM07 の R2 の条件）。(b) `match` の束縛を 2 回読むと、A17 の X3 で最後の使用にならない。(c) 借用の根になる局所は、関数全体で移動しない（A17 の X1）。
- Phase 1 は std の本体の書き換えだけで (a)・(b) を直す。コンパイラは変えない。Phase 2 は `src/last_use.rs` で X1 を精密にして (c) を直す。
- 最初に読む: ① `src/llvm.rs:5919-5931`（`clones_on_take`。5928 行が R2 の Copy の根の条件） ② `src/last_use.rs:55-92`（X1〜X3 と捕捉の除外） ③ A17 の「移動してはいけない形」（`_features/_completed/A17-last-use-move.md:124-135`）と D1・D3（631〜647 行） ④ 下の「再現」。
- 最初に書くテスト: 9 つの関数の確保の回数とバイト数が `[1000]` と `[2000]` で同じことを確かめる `tests/fixtures/alloc_contracts/Main.tz` のテスト（今は失敗する）。
- 触ってはいけない: `Type::is_copy`・`is_noncopy_record`、所有権の規則と診断、PM07 の R2 の条件（D2）、A17 の X2〜X4、`single_use_locals`、std の関数の型と引数の順。

## 目的

`Tensor` と `Matrix` は、バッファを所有する非 Copy の std のレコードである（`src/check.rs:738-741` の `is_noncopy_record`）。
所有権を移す関数（`own Tensor<'a>`・`own Matrix<'a>` を取る関数）は、バッファを移すだけで済むはずである。
文書もそう約束していた（2026-11-25 まで、docstring と `tensor.md`・`matrix.md` の「without copying」「複製しない」「その場で」「O(1)」）。
実際には、要素が Copy 型（`i64`・`f64` など）のとき、9 つの関数が中身を複製する。費用は要素数に比例する。

このチケットは、意味を変えずに、この複製をなくす。Phase 1 は std の本体を書き換える。Phase 2 は、借用の期間を型付き IR から求め、A17 が意図して粗くした X1 を精密にする。
Phase 2 は、利用者の関数（`ref mut out[0..]` で書いてから `out` を返す形）と、A39・NC20 が作る借用の根のうち型が Copy のもの（配列をフィールドに持つ Copy のレコード `b` の `ref mut b.xs[0..]` の後に `b` を返す形、Copy の利用者のレコードの `AddAssign` で `acc += d` の後に `acc` を返す形）にも効く。`Tensor` とそれをフィールドに持つレコードは Copy でないので、`t += u` の後の `t` や `ref mut model.w` の後の `model` を返すときは今も move で、複製しない（HEAD で確認: `ref mut t` を取った後に `Tensor.zeros [1000]` の `t` を返す関数は、返すだけの関数と同じ 3 回・8,016 バイト。`record Acc { xs: [f64] }` を同じように返す関数は、1 回・8,000 バイトが 2 回・16,000 バイトになる）。

文書は、このチケットを待たずに今の振る舞いへ直した（D6）。このチケットが終わったら、docstring と既知の制限の注記を戻す。

## 着手条件と停止条件

### 着手条件

- A17 と PM07 が done であること。確認: `ls _features/_completed/A17-last-use-move.md _perfs/_completed/PM07-copy-elision.md src/last_use.rs tests/last_use_fuzz.mjs`。
- GUIDE §2.3 の基準コマンドが成功する状態から始める。
- 手順 1 のとおり、変更前のコンパイラと測定値を `target/perf/PM10/` に残していること（GUIDE §14）。
- Phase 2 は A39・NC20 を待たない。どちらかが先に入ったら、その借用の根の差分テストを手順 9 で足す。

### 停止条件

次の場合は即興で回避せず、作業を止めて状況と候補案を報告する（GUIDE §13）。

- Phase 1 の書き換えで、std の型・引数の順・トラップの条件と順・`run=` の例の結果のどれかを変える必要が出た。
- 手順 2 で、std の中の所有する scrutinee の `match` が束縛を移動しない（利用者のモジュールの同じ形は移動する。「現状と計測」）。原因を推測で直さない。
- Phase 2 の解析が、所有権検査の結果（`live`・loan・`State`）を必要とすると分かった（A17 の D1、このチケットの D3）。
- `tests/features.mjs last_use`、`tests/last_use_fuzz.mjs`、A17 の「移動してはいけない形」の 3 つ、または下の追加の負のテストの結果が 1 件でも変わった。上限を上げたり、テストを外したりして通さない。
- native と WASM、`-O0` と `-O3` で結果が違う。`live != 0`、二重解放、`tracked_free` の magic の `assert`、segmentation fault が出た。
- debug build の `check_copy_inventory`（A15 D5）が失敗した、または `W1006` が「移動する位置」に出た。
- 借用の根を持たない関数の IR が、手順 1 で保存した IR と 1 byte でも違う。IR が非決定的になった。
- 手順 8 で、`Tensor.update` の `writer (ref mut view)` の後に窓の借用が残らないことを、型付き IR だけから示せない（「設計」の B3）。`update` の 2 回目の複製を残し、その結論を報告する。
- 「既存テストへの影響」に挙げた以外の既存の期待値（IR、確保回数、診断）を変える必要が出た。
- `unsafe`、新しい crate、既定の WASM import、新しいランタイム記号が必要になった。
- 同じ失敗を直す試みが 2 回続けて失敗した。

## 現状と計測（HEAD `858a949c`、2026-11-25 の再測定）

### 複製の経路

- `src/llvm.rs:5919-5931` の `FunctionEmitter::clones_on_take` は、値の型が Copy で「最後の使用」でないとき複製する。
  最後の使用は、根の局所が `single_use`（R1）か `LastUses`（A17）にあり、`borrowed_locals` にないことである。
  さらに 5928 行は、式が `Local` そのものでないとき（フィールドの射影）、根の型が Copy であることを求める（PM07 の R2。`_perfs/_completed/PM07-copy-elision.md:235`・`:242`・D1 の 596〜600 行）。
- `Tensor.Tensor` と `Matrix.Matrix` は、`src/check.rs:740` の一覧で非 Copy のレコードになる。だから `tensor.data` の射影は、`tensor` が単一使用の `own` 引数でも複製する。これが原因 (a) である。
- `src/last_use.rs:74-87` は、`Match` の局所と、パターンの束縛を最後の使用の対象から外す（A17 の X3）。束縛を 1 回だけ読むなら `single_use` が移動する。2 回読むと移動しない。これが原因 (b) である。
- `src/last_use.rs:55-59` は、`Borrow`・`BorrowOperand`・`Slice` の被演算子の根の局所を、関数全体で最後の使用の対象から外す（A17 の X1。`_features/_completed/A17-last-use-move.md:189`）。
  A17 は、借用が呼び出しの後に生き残らない場合の緩和を Phase 3 に回した（同 194 行）。これが原因 (c) である。
- フィールドの読み `xs.length` と添字の読み `xs[0]` は、根を X1 に入れない（下の表の「`xs.length` の後に返す」）。`Array.length (ref xs)` の明示の借用は入れる。

### std の関数ごとの原因（HEAD の行）

| 関数 | 本体（HEAD） | 原因 | 直す Phase |
| --- | --- | --- | --- |
| `Matrix.to_array`（`std/Matrix.tz:98`） | `matrix.data` | (a) | 1 |
| `Matrix.set`（`std/Matrix.tz:107`） | `matrix.rows`・`matrix.cols` を読み、`Array.__set index value matrix.data` | (a) | 1 |
| `Tensor.of_matrix`（`std/Tensor.tz:155`） | `Matrix.to_array matrix` を呼ぶ（`matrix` は非 Copy なので引数としては移動する） | `Matrix.to_array` の (a) | 1 |
| `Tensor.to_matrix`（`std/Tensor.tz:166`） | `tensor.shape` を 3 回読み、`tensor.data` | (a) | 1 |
| `Tensor.to_array`（`std/Tensor.tz:185`） | `tensor.data` | (a) | 1 |
| `Tensor.reshape`（`std/Tensor.tz:195`） | `tensor.data.length` と `tensor.data` | (a)。`match` に書き換えると、束縛を 2 回読むので (b) | 1 |
| `Tensor.reshape_infer`（`std/Tensor.tz:1752`） | `tensor.data.length` と `tensor.data`（`None` の腕は `tensor`） | (a)。`match` に書き換えると、束縛を 2 回読むので (b) | 1 |
| `Tensor.try_reshape`（`std/Tensor.tz:1772`） | `tensor.data.length` と `tensor.data` | (a)。`match` に書き換えると、束縛を 2 回読むので (b) | 1 |
| `Tensor.update`（`std/Tensor.tz:644`） | `let mut data = tensor.data` の後、`borrow_mut … (ref mut data)` の窓を `writer` に渡し、`data` を返す | 1 回目は (a)、2 回目は (c) | 1 と 2 |

行は HEAD のものである。このチケットを起票した PR は docstring を直したので、同じ関数が数行下にずれている。関数の名前で探す。

### 計測済みの事実（2026-11-25、dev バイナリ、`--allocator counting`）

測り方: 関数ごとに、引数を作った後に `Alloc.snapshot ()` を取り、関数を 1 回呼び、`Alloc.since` を読む（「再現」のプログラム）。
native は `tsuzuri run --allocator counting` の `-O0`・`-O3`、wasm32 は同じ値を `assert` するテストを `tsuzuri test --target wasm32` の `-O0`・`-O3` で動かした。
4 つの組み合わせで、回数もバイト数も同じだった。値は「確保の回数 / バイト数」で、`[1000]` と `[2000]` は `i64` の要素数である。

| 関数（要素は `i64`） | `[1000]` | `[2000]` | 要素の複製 |
| --- | --- | --- | --- |
| `Tensor.update`（何かを 1 つ書く `writer`） | 8 / 16,048 | 8 / 32,048 | 2 回 |
| `Tensor.to_array` | 1 / 8,000 | 1 / 16,000 | 1 回 |
| `Tensor.reshape`（形は窓の外で `new` した配列） | 2 / 8,016 | 2 / 16,016 | 1 回 |
| `Tensor.reshape_infer`（形は `new [-1, 2]`） | 3 / 8,032 | 3 / 16,032 | 1 回 |
| `Tensor.try_reshape`（同じ） | 3 / 8,032 | 3 / 16,032 | 1 回 |
| `Tensor.of_matrix` | 2 / 8,016 | 2 / 16,016 | 1 回 |
| `Tensor.to_matrix` | 1 / 8,000 | 1 / 16,000 | 1 回 |
| `Matrix.to_array` | 1 / 8,000 | 1 / 16,000 | 1 回 |
| `Matrix.set` | 1 / 8,000 | 1 / 16,000 | 1 回 |
| 対照: `Tensor.of_array`（形は `new` した配列。形をリテラルで書くと 2 / 16） | 1 / 8 | 1 / 8 | なし |
| 対照: `Matrix.of_array` | 0 / 0 | 0 / 0 | なし |

要素が Copy でない型（`Tensor.init [n] (\i -> to_string i)` など）では、どの関数も要素を複製しない。
`to_array`・`to_matrix`・`Matrix.to_array`・`Matrix.set` は 0 / 0、`reshape` と `of_matrix` は 1 / 16（形の配列）、`update` は何もしない `writer` で 5 / 40（Copy の行と同じ 1 つ書く `writer` では 7 / 50）で、`[1000]` と `[2000]` で同じだった。
`let mut` の配列から作る窓は複製しない。`Tensor.borrow_mut [n] (ref mut data)` と `Tensor.write` は 4 / 32、`MatrixView.of_array_mut` と `MatrixView.write` は 0 / 0 だった。

### 利用者のモジュールでの再現（原因の切り分け）

`Tensor`・`Matrix` と同じ形の非 Copy のレコードを、文字列のフィールドを足して作った（`record NT { name: string, shape: [i64], data: [i64] }`、`record NM { name: string, rows: i64, cols: i64, data: [i64] }`）。
値は `[1000]` のもので、`[2000]` では要素の複製の分だけ倍になった。native・wasm32 × `-O0`・`-O3` で同じだった。

| 形 | 確保 | 要素の複製 |
| --- | --- | --- |
| Copy のレコード（`record Box { tag: i64, data: [i64] }`）の `own` 引数から `b.data` | 0 / 0 | なし（R2 が移動する） |
| 非 Copy の `NT` の `own` 引数から `t.data`（std の `to_array` と同じ形） | 1 / 8,000 | 1 回（原因 (a)） |
| `NM` の射影の `set`（std の `Matrix.set` と同じ形） | 1 / 8,000 | 1 回 |
| `match t with \| NT { name: _, shape: _, data: data } -> data` | 0 / 0 | なし |
| `NM` の `match` の `set`、`NM` の `match` の `to_array`、`NT` の `match` の `to_matrix` | 0 / 0 | なし |
| `NM` の `match` の `to_array` を呼ぶ `of_matrix` | 1 / 16 | なし（形の配列だけ） |
| `match` の束縛 `data` を `data.length` と `data` で 2 回読む `reshape` | 2 / 8,016 | 1 回（原因 (b)） |
| 同じ `reshape` で、`let d = data` に束縛し直してから `d.length` と `d` を読む | 1 / 16 | なし（`vol (ref shape)` の後の形の配列の複製だけ。原因 (c)） |
| 同じ書き方の `reshape_infer`（2 つの腕で `d` を返す） | 1 / 16 | なし |
| `match` で取り出した `data` を `let mut d` に移し、`Tensor.borrow_mut … (ref mut d)` の窓を `writer` に渡してから `d` を返す `update` | 6 / 8,080 | 1 回（原因 (c)） |
| `own [i64]` を受け取り、`xs.length` を読んでから `xs` を返す | 0 / 0 | なし |
| 同じ関数で、`Array.length (ref xs)` を読んでから `xs` を返す | 1 / 8,000 | 1 回（原因 (c)） |
| `let mut out = new [i64](n, …)`、`Array.write 0 7 (ref mut out[0..])`、`out` を返す（`new` を含めて数えた） | 2 / 16,000 | 1 回（原因 (c)） |

決定の記録（NC20 の D13 (a)）との差:

- 記録は、原因 (a) を「std の本体でだけ起きる。同じ形の利用者のモジュールの関数は要素を複製しない。原因は未特定」と書いた。再測定では、利用者のモジュールでも、根が非 Copy のレコードなら同じく複製した。比べた利用者のレコードが Copy だったことが差の理由と考える。原因は PM07 の R2 の条件である。
- 記録は、`reshape`・`to_matrix`・`of_matrix` を Phase 2 で直すと書いた。再測定では、Phase 1 の書き換えだけで直った（利用者のモジュールの同じ形）。Phase 2 が要るのは `update` の 2 回目の複製と、利用者の関数である。
- 記録の `update` の 7 / 16,040 と `reshape` の 3 / 8,032 は、`writer` と形の渡し方が違う測定である。要素の複製の回数（2 回と 1 回）は同じだった。

### 再現（2026-11-25 に確認）

`target/scratch/pm10/Main.tz` に次を置く。

```tsuzuri
record Owner { name: string, data: [i64] }

def report :: string -> i64 -> Alloc.Stats -> unit
fn report label n used =
    Debug.print $"{label} {n}: {used.allocs} {used.bytes}"

def take_projection :: own Owner -> [i64]
fn take_projection owner = owner.data

def take_match :: own Owner -> [i64]
fn take_match owner =
    match owner with
    | Owner { name: _, data: data } -> data

def write_then_return :: i64 -> [i64]
fn write_then_return n =
    let mut out = new [i64](n, \i -> i)
    Array.write 0 7 (ref mut out[0..])
    out

def probe :: i64 -> unit
fn probe n =
    let t = Tensor.init [n] (\i -> i)
    let start = Alloc.snapshot ()
    let a = Tensor.to_array t
    report "Tensor.to_array" n (Alloc.since start)
    let m = Matrix.init (n / 2) 2 (\r c -> r + c)
    let start = Alloc.snapshot ()
    let m2 = Matrix.set 0 1 9 m
    report "Matrix.set" n (Alloc.since start)
    let u = Tensor.init [n] (\i -> i)
    let start = Alloc.snapshot ()
    let u2 = Tensor.update (\w -> Tensor.write [1] 9 w) u
    report "Tensor.update" n (Alloc.since start)
    let o = Owner { name: "", data: new [i64](n, \i -> i) }
    let start = Alloc.snapshot ()
    let b = take_projection o
    report "owner.data" n (Alloc.since start)
    let o2 = Owner { name: "", data: new [i64](n, \i -> i) }
    let start = Alloc.snapshot ()
    let c = take_match o2
    report "match owner" n (Alloc.since start)
    let start = Alloc.snapshot ()
    let d = write_then_return n
    report "write_then_return" n (Alloc.since start)
    assert (a.length + Matrix.rows (ref m2) + (Tensor.to_array u2).length + b.length + c.length + d.length > 0)

def main :: unit -> i32
fn main _unit =
    probe 1000
    probe 2000
    0i32
```

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
for o in -O0 -O3; do target/debug/tsuzuri run target/scratch/pm10/Main.tz $o --allocator counting; done
```

期待（HEAD、`-O0`・`-O3` で同じ）: `Tensor.to_array 1000: 1 8000`、`Matrix.set 1000: 1 8000`、`Tensor.update 1000: 8 16048`、`owner.data 1000: 1 8000`、`match owner 1000: 0 0`、`write_then_return 1000: 2 16000`。
`[2000]` では、`match owner` 以外のバイト数が要素の分だけ増える（`16000`・`16000`・`32048`・`16000`・`32000`）。
Phase 1 の後は、`Tensor.to_array` と `Matrix.set` が `0 0` になり、`Tensor.update` は要素の複製が 1 回になる。Phase 2 の後は、`Tensor.update` の要素の複製がなくなり、`write_then_return` が `1 8000`（`[2000]` では `1 16000`）になる。

## 目標と指標

目標は CI の合否条件にしない（GUIDE §14）。意味の検査（結果の一致、確保の固定値、`live == 0`）だけを合否にする。

- G1（Phase 1）: 9 つの関数のうち `update` 以外の 8 つで、要素の複製をなくす。`update` は 1 回目の複製をなくす。
- G2（Phase 2）: `update` の 2 回目の複製と、借用が呼び出しで終わった後に根を返す利用者の関数の複製をなくす。
- G3: 後で読まれる値、借用が生きている間の値、借用が格納されうる値は、今と同じく複製する。結果は 1 つも変わらない。
- G4: 借用の根を持たない関数の IR は変わらない。

| 指標 | 期待（規則と再現から計算。計測で確かめる） |
| --- | --- |
| 9 つの関数の確保（`Alloc.since`、`[1000]` と `[2000]`） | Phase 1 の後: `update` 以外の 8 つは、回数とバイト数が `[1000]` と `[2000]` で同じ。`update` は要素の複製が 1 回。Phase 2 の後: 9 つとも `[1000]` と `[2000]` で同じ |
| 利用者の関数（「再現」の `write_then_return`） | 2 / 16,000 → 1 / 8,000（Phase 2） |
| `Array.length (ref xs)` の後に `xs` を返す関数 | 1 / 8,000 → 0 / 0（Phase 2） |
| Copy の対象の `acc += d` の後に `acc` を返す関数（NC20 の後。`record Acc { xs: [f64] }` の利用者の `AddAssign`） | 要素の複製なし（Phase 2。NC20 が入ってから測る）。`Tensor` の `t += u` の後に `t` を返す関数は、`Tensor` が Copy でないので今も複製しない |
| `tests/features.mjs last_use`、`tests/last_use_fuzz.mjs` | どの段でも結果は同じ（native/WASM × `-O0`/`-O3`、`live == 0`） |
| `copy_inventory` の合計（`cargo test --locked --test copy_inventory -- --ignored --nocapture`） | 増えない。std の行が減る |

## 変えてはいけない意味

- Copy の独立性: 値を複製する代わりに移動してよいのは、元の値を後で読む式も、元の値を指す借用も、もう存在しないときだけである。観測できる違いがあってはならない。
- 評価順序（左から右）、短絡評価、トラップの時点・条件。Phase 1 の書き換えは、`assert` の条件と順を変えない。`match` の分解は、引数の評価の後、`assert` の前に行う。
- drop の時点と回数: `Tensor`・`Matrix` は利用者 `Drop` を持たない。A17 の D10 のとおり、射影が `has_user_drop` の値を経由する束縛は移動しない。
- std の関数の型、引数の順（`tests/std_argument_order.rs`）、トラップの文言、`run=` の例の結果は変えない。
- 所有権・借用の規則、`E1012`・`E1013`・`E1014` などの診断、`Type::is_copy`・`Type::needs_drop`・`is_noncopy_record` の意味は変えない。所有権検査の結果は使わない（D3）。
- IR の決定性: 集合は `BTreeSet`・`BTreeMap`。native と WASM、`-O0` と `-O3` の結果は同じ。
- 診断: 新しいコード・メッセージはない。`W1006` が出る位置が減ることがある。警告の位置は保証の対象外である。

## 設計

### Phase 1: std の本体の書き換え

所有する引数を `match` で分解する。A17 の Phase 2（M1〜M4）が、所有する scrutinee の Copy の束縛を移動する。
束縛を 2 回以上読む本体では、束縛を `let` の局所に束縛し直してから読む。`let` の局所は X3 に入らないので、A17 の Phase 1 の最後の使用が移動する。

| 関数 | 書き換え（形だけ。名前は実装で決める） | Phase 1 の後の要素の複製 |
| --- | --- | --- |
| `Matrix.to_array` | `match matrix with \| Matrix { rows: _, cols: _, data: data } -> data` | なし |
| `Matrix.set` | `match matrix with \| Matrix { rows: rows, cols: cols, data: data } ->` の中で、今と同じ `assert` と `index` を `rows`・`cols` から求め、`Matrix { rows: rows, cols: cols, data: Array.__set index value data }` | なし |
| `Tensor.of_matrix` | 変えない（`Matrix.to_array` が直る） | なし |
| `Tensor.to_array` | `match tensor with \| Tensor { shape: _, data: data } -> data` | なし |
| `Tensor.to_matrix` | `match` で `shape`・`data` を取り出し、今と同じ `assert (shape.length == 2)` の後、`Matrix.of_array shape[0] shape[1] data` | なし |
| `Tensor.reshape` | `match` で `data` を取り出し、`let elements = data` に束縛し直す。今と同じ `assert (volume (ref shape) == elements.length)` の後、`Tensor { shape: shape, data: elements }` | なし（`shape` の複製は残る。原因 (c)。Phase 2 で消える） |
| `Tensor.reshape_infer` | 同じく束縛し直し、`resolve_infer_shape (ref spec) elements.length` の結果で分ける。`None` の腕は今と同じく `assert false` の後、元の形と `elements` から作り直した値を返す | なし |
| `Tensor.try_reshape` | 同じく束縛し直す。`None` の腕は今と同じ `Result.Error (Error.InvalidShape spec)` | なし |
| `Tensor.update` | `match` で `shape`・`data` を取り出し、`let mut data = data` に移してから、今と同じく窓を作る | 1 回（原因 (c)。Phase 2 で消える） |

- `Matrix.Matrix`・`Tensor.Tensor` のパターンは std の中でだけ書ける（利用者は `E1022`）。std は `Edition::LATEST` で読む。
- `Matrix.set` の本体が呼ぶ `Array.__set`（`Builtin::ArraySet`、`src/check.rs:2736`。AGENTS.md の引数の順の節が、コンパイラが名前で知る関数として挙げる）は、呼び方と引数の順を変えない。
- 書き換えは IR の形を変えるが、値は変えない。`tests/fixtures/tensor/Main.tz`・`tests/fixtures/matrix/Main.tz` の suite と、docstring の `run=` の例で確かめる。

採らない案: PM07 の R2 を、利用者 `Drop` を持たない非 Copy の根に広げる（`src/llvm.rs:5928` の条件を変える）。利用者のプログラムの生成も変わるので、独自の差分テストが要る（D2）。

### Phase 2: X1 の精密化（`src/last_use.rs`）

A17 の D1 のとおり、`CheckedFunction.body` の型付き IR だけから借用の期間を求める。所有権検査の結果は使わない。
今の X1（借用の根の局所を関数全体で除外する）を、次の規則で置き換える。どの規則にも当てはまらない借用は、今と同じく関数全体で除外する（迷ったら移動しない。D4）。

| 規則 | 内容 |
| --- | --- |
| B1 | 借用（`Borrow`・`BorrowOperand`・`Slice`。根は今の `root` と同じ）が、呼び出し（名前付きの関数、組み込み関数、クラスのメソッド、関数値。`AddAssign` 系を含む）の引数に直接書かれている。呼び出しの結果の型が loan を持たない（具体型に `Type::carries_loans` が偽）。同じ呼び出しの引数と呼び出される値に、借用を格納しうる場所がない。格納しうる場所は、型（特殊化した具体型）が、所有する値・レコードのフィールド・タプルの成分・コレクションの要素・参照の先・共有のポインタ（`Rc`・`Arc`）を再帰的にたどって、次のどれかに届く引数と呼び出される値である。呼び出される値は関数値（局所・フィールド・呼び出しの結果など）のときだけ数え、名前付きの関数・組み込み関数・クラスのメソッドを直接呼ぶとき（型付き IR の `Function`・`GenericFunction`・`Method`）は、型が関数でも環境がないので数えない。届く先は次の 3 つである: (i) loan を持てる型への `ref mut`。loan を持てる型は A31 の `hidden_type` の `holds` が真の型である。`hidden_type` は `without_views` で排他スライスの `[T..]` を `[T]` に直してから、`Type::carries_loans`、関数値を隠すか、セルに届くかを見る（`[T..]` は `Type::ArrayView` で、`Type::carries_loans` も `TypeContext::reaches` もその中を見ないので、`ref mut [View { r }..]` の先に直接当てると偽になる）。引数そのものが `ref mut` でなくてもよい。所有するレコードのフィールドの `ref mut` を通して格納できる（`record Slot { q r } { dst: ref mut { q } View { r } }` を値で渡す `stash (ref xs) slot` は、`deref slot.dst = View { data: src }` で `ref xs` を呼び出し元の `v` に格納し、結果は `unit`。HEAD で受理）。(ii) 関数値・`dyn` の値・`Owned.Function`（環境やメソッドが格納しうる）。(iii) 内容が loan を持ちうる可変のセル（`Type::is_mutable_cell`。`Mutex` など）。判定は所有権の検査と同じ型の述語（A31 の `hidden_type` は `src/ownership.rs` 8232 行付近、`reaches_cell` は 8156 行付近。どちらも `TypeContext::reaches` で型をたどる）を使い、所有権の検査の結果は使わない（A17 の D1）。この 2 つは所有権の検査器の非公開のメソッド（キャッシュを持つ `&mut self`）なので、型だけを読む自由関数に切り出して `src/last_use.rs` からも使う（`hides_environment`・`may_reach_store` は 6019・6029 行付近で、すでに自由関数である）。型変数や未知の型が残るなら格納しうるとみなす。`extern` の関数の呼び出しは B1 に当てない。解析している借用そのものの引数（B3 では `L` の借用 `ref mut L`）は、格納しうる場所に数えない（そこに格納されたものは `L` とともに終わり、B3 が `L` の生存区間で追う）。このとき、借用はその呼び出しで終わる |
| B2 | B1 の借用は、呼び出しを実行する位置での根の使用として生存解析に入れる。同じ呼び出しの、借用より後の引数で根を読んでも、移動しない（「負のテスト」の `borrow_then_consume`）。借用より前の引数の読みは、借用が後で根を読むので最後の使用にならない |
| B3 | 借用が、結果が loan を持つ呼び出し（`Tensor.borrow_mut … (ref mut data)` など）の引数で、その結果を `let` で局所 `L` に束縛する。その呼び出しのほかの引数と呼び出される値に、B1 の意味で格納しうる場所がない（借用は `L` にだけ残る）。`L` の使い方がすべて B1 の形（`L` の借用が、結果が loan を持たず格納しうる場所のない呼び出しで終わる）なら、根は `L` が生きている間だけ生きているとする。束縛する呼び出しに格納しうる場所がある（`lend (ref xs) slot` は `slot` のフィールドの `ref mut` を通して `ref xs` を `v` に格納し、同じ借用を持つ `View` も返す）か、`L` が返される・捕捉される・フィールドや要素に格納される・ほかの局所に移る・B1 でない呼び出しに渡るなら、今と同じく関数全体で除外する |
| B4 | 借用が関数の結果になりうる、閉包に捕捉される、レコード・タプル・配列の構築に入る、`match` の scrutinee になる、パターンで束縛される、`ForEach` の source になる場合は、今と同じく関数全体で除外する |
| B5 | 解析の予算（A17 の D7）を超えたら、その関数の結果を空にする |

- `Tensor.update` の窓は B3 の `L`（`view`）である。`view` の唯一の使い方は `writer (ref mut view)` である。B1 の格納しうる場所に当たるのは、呼び出される値 `writer`（関数値）だけである（`ref mut view` 自身は数えない）。B1 のままでは 2 回目の複製は残る。
  `writer` の型（`ref mut Mut<'a> -> unit`）の仮引数の region が、呼び出し元のどの region とも結びつかないことから「窓を残せない」と型だけで言えるかは未確認である。手順 8 で確かめ、言えなければ停止条件のとおり報告する。
- A39 のフィールドの排他借用（`ref mut model.w`。edition 2027 以降）と、NC20 の `+=`・`IndexMut`・`IndexBorrowMut` の対象の排他借用（`Borrow(p, true)`）も、根は今の `root` がたどる局所である。同じ規則を当てる。`AddAssign` 系のメソッドの結果は `unit` なので、`t += u` の借用は B1 で終わる。
- `W1006` の一覧（`copies::sites`）は、生成と同じ `LastUses` を読む（A17 の L6）。新しい規則は `LastUses` の中だけで足し、生成と一覧が別の判定をしない。

## 実装手順

各手順の後で tree は compile でき、それまでのテストは成功する。`cargo test <name>` は 0 件でも成功するので、`running N tests` の N を見る。重いコマンドは `node scripts/slot.mjs --` で包む（AGENTS.md）。

### 手順 1: ベースライン

- 変更: なし。
- 内容: release のコンパイラを `target/perf/PM10/tsuzuri-before` に写す。「再現」のプログラムを `target/scratch/pm10/` に置き、`-O0`・`-O3` の出力を `target/perf/PM10/before.txt` に保存する。
  `tests/fixtures/last_use`・`tests/fixtures/alloc_contracts`・`tests/fixtures/tensor`・`tests/fixtures/matrix` の `--emit llvm`（`-O0`）を `target/perf/PM10/ir-before/` に保存する。
- 確認: `before.txt` が「再現」の期待と一致する。

### 手順 2: std の中で `match` の移動が効くことを確かめる

- 変更: `std/Matrix.tz` の `to_array` だけを書き換える。
- 確認: 「再現」に `Matrix.to_array` と `Tensor.of_matrix` を足し、`Matrix.to_array` が `[1000]`・`[2000]` で 0 / 0、`Tensor.of_matrix` が 1 / 16 になる。ならなければ停止条件。

### 手順 3: 残りの std の書き換え

- 変更: 「設計」の Phase 1 の表のとおり、`std/Matrix.tz` の `set` と、`std/Tensor.tz` の 6 つの関数を書き換える。
- 確認: 9 つの関数の確保が、表の「Phase 1 の後」のとおりになる。`cargo nextest run --locked --test tensor --test matrix --test std_argument_order`、
  `node tests/tensor-cases.mjs target/debug/tsuzuri`・`node tests/matrix-cases.mjs target/debug/tsuzuri`、
  `TSUZURI_BIN=target/debug/tsuzuri node scripts/check-docs.mjs _tsuzuri/language-reference/built-in-types-and-modules/tensor.md _tsuzuri/language-reference/built-in-types-and-modules/matrix.md` が通る。

### 手順 4: Phase 1 の確保の契約

- 変更: `tests/fixtures/alloc_contracts/Main.tz` に、9 つの関数を `[1000]` と `[2000]` の `i64` で呼ぶテストを足す（「テスト計画」）。`alloc.md` の契約の表と件数を同じ PR で直す。
- 確認: `cargo nextest run --locked --test alloc_contracts`（edition 2026・2027 × native・wasm32 × `-O0`・`-O3`）。`update` の行は、Phase 1 の後の値（要素の複製 1 回）を書き、Phase 2 で直す。

### 手順 5: Phase 1 の文書

- 変更: 「ドキュメント」の Phase 1 の行。`update` 以外の 8 つの関数の docstring を「複製しない」に戻し、既知の制限の注記を `update` だけにする。
- 確認: `node scripts/check-docs.mjs` を変えたページに、`node scripts/gen-docs.mjs --check`、`node scripts/check-cli-docs.mjs`。

### 手順 6: Phase 2 の骨格（何も移動を増やさない）

- 変更: `src/last_use.rs` に、借用ごとに B1〜B4 のどれに当たるかを分類する処理を足す。結果はまだ使わない（X1 の集合は今のまま）。
- 確認: 手順 1 の IR と `--emit llvm` の出力が byte で一致する。`cargo nextest run --locked --test last_use`。

### 手順 7: B1・B2 を有効にする

- 変更: B1 に当たる借用の根を、X1 の集合から外す。B2 の「呼び出しの位置での使用」を生存解析に足す。
- 確認: 「再現」の `write_then_return` が 1 / 8,000、`Array.length (ref xs)` の後に返す関数が 0 / 0、`Tensor.reshape` が要素数に依らず 0 回の要素の複製で形の複製も消える。
  `node tests/features.mjs target/debug/tsuzuri last_use`（native/WASM × `-O0`/`-O3`）、`node tests/last_use_fuzz.mjs target/debug/tsuzuri 300 1000 4`（手順 9 で足す形を含む）、「負のテスト」が HEAD と同じ結果。

### 手順 8: B3 を有効にする

- 変更: 結果が loan を持つ呼び出しを束縛した局所 `L` の生存区間を、根の生存に足す。
- 確認: `Tensor.update` の確保が `[1000]` と `[2000]` で同じになる。ならなければ、`writer (ref mut view)` が B1 の「関数値」に当たることが理由かを確かめ、停止条件のとおり報告する（「設計」の注記）。
  手順 7 の確認をすべて繰り返す。

### 手順 9: A39・NC20 の借用の根

- 変更: A39・NC20 のうち、HEAD に入っているものについて、差分テストと E2E を足す。Copy の持ち主の `ref mut b.xs[0..]`（`record B { xs: [f64] }`。edition 2027）を `Array.write` などに渡した後に `b` を返す形、Copy の対象の `acc += d` の後に `acc` を返す形。`Tensor` の形（`ref mut model.w` の後の `model`、`t += u` の後の `t`）は今も move なので、複製がないことを回帰として確かめるだけにする。
- 確認: 要素の複製がない。入っていないものは、そのチケットの受け入れ条件に「PM10 のテストを足す」があることを確かめ、完了報告に書く。

### 手順 10: 全体の確認と計測

- 変更: なし。
- 確認: `cargo nextest run --locked`、debug build で `check_copy_inventory` が通る、`copy_inventory` の合計が増えない、手順 1 の IR と比べて借用の根を持たない関数の IR が同じ。「計測手順」の表を完了報告に貼る。

### 手順 11: Phase 2 の文書

- 変更: 「ドキュメント」の Phase 2 の行。`update` の docstring を戻し、既知の制限の注記と PM10 へのリンクを消す。`_perfs/README.md` の状態を done にする。
- 確認: 手順 5 と同じ。

## 計測手順

- 確保: 「再現」と同じく、関数の呼び出しを `Alloc.snapshot ()` と `Alloc.since` で挟む。`tsuzuri run --allocator counting` の native `-O0`・`-O3` で読み、
  同じ値を `assert` するテストを `tsuzuri test --target wasm32` の `-O0`・`-O3` で動かす。`[1000]` と `[2000]` の両方を測る。期待値は規則から数え、コンパイラの出力を写さない。
- `Alloc.measure` の閉包の中で所有値を消費すると、捕捉した値を複製するので測らない（`alloc.md` の「所有値を測る」）。
- 時間は主張しない。測るなら GUIDE §14 のとおり、同じ機械の before と after だけで比べる（`Matrix.set` を `n` 回呼ぶループは、要素数と回数の積から要素数に比例する形へ変わる見込みである）。
- 速さの閾値を CI に入れない。

## 生成コードの確認

```sh
cd /Users/tmidorikawa/Documents/git/Tsuzuri
target/debug/tsuzuri build target/scratch/pm10 --emit llvm -O0 --allocator counting -o target/scratch/pm10/pm10.ll
fn_body() { awk -v name="@tz.fn.$1" '/^define / { p = index($0, name "(") || index($0, name ".$mono.") } p; /^}/ { p = 0 }' target/scratch/pm10/pm10.ll; }
for f in Tensor.to_array Matrix.set Tensor.update Main.write_then_return; do
  printf '%s ' "$f"; fn_body "$f" | grep -c 'call .*ptr @tz.alloc'; done
```

- 「再現」のプログラムは `Alloc` を呼ぶので、`--allocator counting` がないと `E1018` で止まる。std の関数は特殊化した名前（`@tz.fn.Tensor.to_array.$mono.10` など。番号はプログラムで変わる）になるので、`fn_body` は `.$mono.` の付いた名前も拾う。プログラムが呼ばない関数は IR になく、数は 0 になる。手順 2 で `Matrix.to_array` を足したら、それも数える。
- HEAD（2026-11-25 に確認）: `Tensor.to_array 1`、`Matrix.set 1`、`Tensor.update 4`、`Main.write_then_return 2`。`Tensor.update` の 4 には形の配列の確保も含まれる。
- 期待（Phase 1 の後）: `Tensor.to_array 0`、`Matrix.set 0`、`Matrix.to_array 0`（手順 2 で足した後）。PR01 が `nonnull` を付けるので、`call .*ptr` で数える（A17 の D10）。
- 期待（Phase 2 の後）: `Main.write_then_return 1`（`new` の 1 回だけ）。
- 移動した位置では、フィールドか局所の `load` の直後に同じ pointer への `store … zeroinitializer` があり、`clone_value` の要素の複写がない。

## テスト計画

### Rust テスト

1. `tests/last_use.rs`（Phase 2）: B1〜B4 の分類の単体テスト。B1 で終わる借用、B2 の同じ呼び出しの後の引数、B3 の束縛した局所の生存区間、B4 の各形（返す、捕捉、構築、scrutinee、`ForEach`）を、`LastUses` の結果で確かめる。
2. `tests/alloc_contracts.rs` と `tests/fixtures/alloc_contracts/Main.tz`（Phase 1）: 9 つの関数の確保。期待は規則から数える（要素の複製なし、形の配列の確保は形の長さ × 8 バイト）。
3. 既存の `tests/tensor.rs`・`tests/matrix.rs`・`tests/std_argument_order.rs` が通る。

### E2E

- `tests/fixtures/last_use/Main.tz` と `tests/features.mjs` の `last_use`（Phase 2）: B1・B3 で移動する形と、移動しない形を足す。JavaScript の参照値は独立に書く。native/WASM × `-O0`/`-O3`、`live == 0`。
- 負のテスト（どの段でも結果が変わらないこと。HEAD の dev バイナリで確認した値）:

| 形 | 結果 |
| --- | --- |
| A17 の「移動してはいけない形」の 3 つ（`_features/_completed/A17-last-use-move.md:128-133`）。`Array.set xs 0 9` は今の `Array.update_at 0 9 xs`、2 つ目のループは `for i in 0 ..= 2 do`（3 回）と書く。edition 2027 の `for i in 0 .. 2` は `E0004` | 10、9、10 |
| `borrow_then_consume`: `let xs = new [i64](4, \i -> i + 1)` の後、`pair_sum (ref xs) (Array.update_at 0 9 xs)`（`pair_sum r ys = r[0] + ys[0]`） | 10 |
| `bound_borrow`: `let r = ref xs` の後、`let ys = Array.update_at 0 9 xs`、`r[0] + ys[0]` | 10 |
| `stash_through_owned_record`: `record View { r } { data: ref { r } [i64] }`、`record Slot { q r } { dst: ref mut { q } View { r } }`、`stash src slot = deref slot.dst = View { data: src }`（`def stash { q r } :: ref { r } [i64] -> own Slot { q r } -> unit`）。`let fallback = [0, 0, 0, 0]`、`let xs = new [i64](4, \i -> i + 1)`、`let mut v = View { data: ref fallback }`、`let slot = Slot { dst: ref mut v }` の後、`stash (ref xs) slot`、`let ys = Array.update_at 0 9 xs`、`v.data[0] + ys[0]`（B1 が `ref mut` の引数だけを格納しうる場所に数えると、`ref xs` が `stash` で終わったとみなして `xs` を移動し、18 になる） | 10（HEAD の dev バイナリで `-O0`・`-O3` とも） |
| `lend_through_owned_record`: `stash_through_owned_record` の `View`・`Slot`・`fallback`・`xs`・`v`・`slot` で、`def lend { q r } :: ref { r } [i64] -> own Slot { q r } -> View { r }`、`fn lend src slot =` の本体は `deref slot.dst = View { data: src }` の後に `View { data: src }`。`let l = lend (ref xs) slot`、`let n = Array.length (ref l.data)`、`let ys = Array.update_at 0 9 xs`、`v.data[0] + ys[0] + n`（B3 が束縛する呼び出しのほかの引数を見ないと、`l` の最後の使用の後に `xs` を移動し、22 になる） | 14（HEAD の dev バイナリで `-O0`・`-O3` とも） |
| `stash_through_slice`: `stash_through_owned_record` の `View` と `fallback`・`xs` で、`def put { r } :: ref { r } [i64] -> ref mut [View { r }..] -> unit`、`fn put src views = Array.write 0 (View { data: src }) views`。`let mut views = [View { data: ref fallback }]` の後、`put (ref xs) (ref mut views[0..])`、`let ys = Array.update_at 0 9 xs`、`views[0].data[0] + ys[0]`（`ref mut` の先の `[T..]` を `without_views` で直さずに判定すると 18 になる） | 10（HEAD の dev バイナリで `-O0`・`-O3` とも） |

### 差分テスト（`tests/last_use_fuzz.mjs`）

- 今の文の種類（`borrow` を含む 13 種）に、次を足す。AST 評価器は値を複製するだけで、最後の使用を考えない（今のまま）。
  - `borrow_call`: `Array.length (ref xs)` を足し込む、`Array.write i v (ref mut xs[0..])` で書く（評価器は `xs` の `i` 番目を書き換える）。
  - `borrow_bound`: `let r = ref xs` の後に `xs` を更新し、`r[i]` を読む。
  - `borrow_then_return`: 借用の後に配列を返す小さな関数を呼ぶ。
- 実行: `node tests/last_use_fuzz.mjs target/debug/tsuzuri 300 1000 4`。README の検証コマンドの行の説明を同じ PR で直す。

### 既存テストへの影響

- `tests/copy_cost.rs`: B1 で移動する位置が `W1006` の期待にあれば、その期待を規則に合わせて変え、完了報告に書く。
- `alloc.md` の契約の表と件数（Phase 1 で行を足す）。
- それ以外の既存の期待値は変えない。

### 性能

「計測手順」のとおり。共有 CI に時間の合否条件を足さない。

## ドキュメント

| ファイル | 節 | Phase | 内容 |
| --- | --- | --- | --- |
| `std/Tensor.tz`・`std/Matrix.tz` | 9 つの関数の docstring | 1・2 | 直った関数から「The current compiler copies the elements …」を消し、複製しないと書く |
| `_tsuzuri/language-reference/built-in-types-and-modules/tensor.md` | 冒頭、「所有者の操作と Matrix との変換」の表・例の説明・既知の制限の注記、「計算量」、「できないこと」、「まとめ」 | 1・2 | 直った関数の「要素が Copy 型なら要素を複製する」を消す。Phase 2 の後に注記と PM10 へのリンクを消す |
| `_tsuzuri/language-reference/built-in-types-and-modules/matrix.md` | 「この記事のポイント」、「読む」の表、「要素を置き換える」と既知の制限の注記、「計算量」 | 1 | `to_array`・`set` の複製の記述と注記を消す |
| `_tsuzuri/language-reference/built-in-types-and-modules/matrix-view.md` | 「書き込める窓」の、行列の中身を書き換える手順の後の注記 | 1 | `Matrix.to_array` の複製の注記を消す |
| `_tsuzuri/language-reference/built-in-types-and-modules/alloc.md` | 「標準ライブラリの確保の契約」 | 1 | 足したテストの行と件数 |
| `_tsuzuri/language-reference/ownership-and-memory/ownership.md` | 最後の使用の move の節 | 2 | 呼び出しで終わる借用の後の最後の使用は移動すること（意味は変わらない） |
| `docs/language.md` | Matrix の節（`to_array`・`set`・計算量）と Tensor の節 | 1 | `matrix.md`・`tensor.md` と同じ内容 |
| `docs/architecture.md` | 性能設計の原則の、最後の使用の move の段落 | 2 | B1〜B5 と、所有権検査の結果を使わないこと |
| `_perfs/README.md` | `### PM. メモリと成果物サイズ` | 2 | 状態を done にし、チケットを `_completed/` へ移してリンクを直す |

## 受け入れ条件

- [ ] 9 つの関数の確保の回数とバイト数が、`[1000]` と `[2000]` の `i64` で同じ（native/WASM × `-O0`/`-O3`）。
- [ ] 「再現」の `write_then_return` が 1 回・8,000 バイト（`[1000]`）、`Array.length (ref xs)` の後に返す関数が 0 回。
- [ ] NC20 が入っていれば、Copy の対象の `acc += d` の後に `acc` を返す関数が要素を複製しない（A39 が入っていれば、Copy の持ち主の `ref mut b.xs[0..]` の後に `b` を返す関数も同じ）。入っていなければ、NC20・A39 の受け入れ条件にそのテストの項目がある。
- [ ] A17 の「移動してはいけない形」の 3 つと、追加の負のテストが HEAD と同じ結果。
- [ ] `tests/features.mjs last_use` と `tests/last_use_fuzz.mjs`（追加した形を含む）が通り、`live == 0`。
- [ ] 借用の根を持たない関数の IR が、手順 1 の IR と同じ。IR が決定的。
- [ ] debug build の `check_copy_inventory` が通り、`copy_inventory` の合計が増えない。
- [ ] 「ドキュメント」の表を更新し、docstring と既知の制限の注記を戻した。PM10 を指すリンクが残っていない。

## 落とし穴

- 利用者のモジュールで再現するときは、レコードを非 Copy にする（文字列のフィールドを足す）。Copy のレコードは R2 が移動するので、std と違う結果になる。
- 再現の関数の仮引数には `own` を書く。edition 2027 の読み取りの仮引数は借用なので、射影も `match` も複製する。
- `match` の束縛を 2 回読むと X3 で移動しない。`let` に束縛し直す。束縛し直した局所を `ref` で借りると、Phase 1 では X1 で移動しない（`reshape` の `shape`）。
- `xs.length` は借用ではない。`Array.length (ref xs)` は借用である。std の本体で `Array.length (ref data)` に書き換えると、Phase 1 の効果が消える。
- B1 の格納しうる場所を、トップレベルの `ref mut` の引数と呼び出される関数値だけで判定しない。値で渡す引数の中の `ref mut`（所有するレコードのフィールド）、関数値・`dyn` の値を持つフィールド、可変のセルも格納する。負のテスト `stash_through_owned_record` が 18 を返したら、この見落としである。B3 で `L` を束縛する呼び出しのほかの引数も同じく判定する（`lend_through_owned_record` が 22 を返したら、この見落としである）。`ref mut` の先の `[T..]` は `without_views` で `[T]` に直してから判定する（`stash_through_slice` が 18 を返したら、この見落としである）。
- B1 の「結果が loan を持たない」は、特殊化した具体型で判定する。型変数が残る型で判定しない（`Checker::loan_free` は型変数を開いた型として扱う）。
- B2 を忘れると、`pair_sum (ref xs) (Array.update_at 0 9 xs)` が 18 を返す。差分テストに必ず入れる。
- `Alloc.measure` の閉包で所有値を消費すると、捕捉の複製が入る。`snapshot` と `since` で測る。
- 期待値は規則から数える。実装後のコンパイラの出力を写して期待にしない。

## 対象外

- 新しいゼロコピーの API（`Tensor.as_mut` など）と、`update` の廃止。同じ仕事に別の性能の道ができる（AGENTS.md）。
- PM07 の R2 を非 Copy の根に広げること（D2）。
- NC20 の `+=`・`IndexBorrowMut` そのもの（NC20）と、A39 のフィールドの排他借用そのもの（A39）。
- `Index`・`ListTail`・`UnionPayload` の place の移動、閉包の環境の移動（A17 の Phase 3 の残り）。
- Copy の意味の変更、`--warn implicit-copy` の既定の変更。

## 決定事項

### D1: 起票と範囲

- 決定: NC20 の D13 (a) を、このチケットにする。対象は、文書が「複製しない」「その場で」と書くのに、要素が Copy 型で中身を複製する std の関数である。
  決定の記録は 7 つ（`Tensor.update`・`to_array`・`reshape`・`of_matrix`・`to_matrix`、`Matrix.to_array`・`set`）を挙げた。再測定で、同じ形の `Tensor.reshape_infer`・`try_reshape` を足した（`own Tensor<'a>`・`own Matrix<'a>` を取る std の関数は、この 9 つですべてである）。
  NC20 には入れない。
- 理由: `update` だけを直すと、隣の関数の誤りと `Matrix.set` の要素数に比例する費用が残る。X1 の精密化は横断の変更なので、独自の差分テストを持つ別の PR にする。
- 採らなかった案: `update` だけを直す案、文書だけを直す案、NC20 に入れる案、新しいゼロコピーの API を足す案。
- 状態: 決定（D-66 の委任で決定、2026-11-25）

### D2: Phase 1 は std の書き換えで直し、R2 は広げない

- 決定: 原因 (a)・(b) は、所有する scrutinee の `match` と、束縛し直しで直す。`src/llvm.rs:5928` の条件（PM07 の D1）は変えない。
- 理由: std の書き換えだけで、利用者のモジュールの同じ形が要素を複製しなくなることを測った。R2 を広げると利用者のプログラムの生成も変わり、PM07 の D1 の理由（利用者 `Drop` と、空にしたフィールドの drop）を非 Copy の根で示し直す必要がある。
- 状態: 既定案（実装者はこの案に従う）

### D3: Phase 2 は型付き IR だけから借用の期間を求める

- 決定: `src/last_use.rs` で、`CheckedFunction.body` から借用の期間を求める。所有権検査の結果（`live`・loan・`State`）は使わない。
- 理由: A17 の D1 の理由（所有権の結果は生成へ渡らず、span で式を対応させると builder の展開で衝突しうる）は今も成り立つ。`src/llvm_facts.rs` の PR01 の属性も、所有権検査の結果ではなく型から作る。
- 状態: 決定（D-66 の委任で決定、2026-11-25）

### D4: 迷ったら移動しない

- 決定: B1〜B3 に当てはまらない借用は、今と同じく関数全体で除外する。返却・捕捉・フィールドへの格納の可能性があれば除外する。
- 理由: 借用の見落としは誤コンパイルになる（A17 の試作で結果が黙って 18 になった。A17 の D3）。機会の損失は許す。
- 状態: 決定（D-66 の委任で決定、2026-11-25）

### D5: A39・NC20 の借用の根にも同じ規則を当てる

- 決定: A39 のフィールドの排他借用と、NC20 の `+=`・`IndexMut`・`IndexBorrowMut` の対象の排他借用が作る根も、B1〜B4 で扱う。それらが未実装なら、入った後に差分テストを足す。PM10 は A39 に依存しない。
- 理由: どちらも根の局所を X1 に入れるので、根の型が Copy なら（配列をフィールドに持つレコード `b` の `ref mut b.xs[0..]` の後の `b`、Copy の対象の `acc += d` の後の `acc`）、借用の後に根を返すときに複製する。`Tensor` とそれをフィールドに持つレコードは Copy でないので、`t += u` の後に `t` を返しても move で、複製しない。`update` は `match` で `data` を局所に移せば、フィールドの排他借用なしに窓を作れる。
- 状態: 決定（D-66 の委任で決定、2026-11-25）

### D6: 文書を先に直す

- 決定: 起票の PR で、9 つの関数の docstring と `tensor.md`・`matrix.md` の「複製しない」「O(1)」「その場で」を今の振る舞いに直し、既知の制限の注記と PM10 へのリンクを足した。コメントと文書だけで、コードは変えていない。このチケットが直した関数から、記述を戻す。
- 理由: 文書は今の振る舞いだけを書く（AGENTS.md）。
- 状態: 決定（D-66 の委任で決定、2026-11-25）

### D7: 測り方

- 決定: 確保は `Alloc.since` と IR で数える。期待は「要素数に依らない」と規則から数えた固定値にする。速さの閾値は置かない。
- 理由: 構造的な性質は確保の数で決定的に示せる。時間は共有 CI の合否にしない（AGENTS.md）。
- 状態: 決定（D-66 の委任で決定、2026-11-25）
