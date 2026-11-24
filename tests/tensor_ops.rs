use std::process::Command;
use tsuzuri::{analyze, llvm};

fn run_native(source: &str, opt: &str) -> String {
    let dir = std::env::temp_dir().join(format!(
        "tsuzuri-nc05-{}-{}",
        std::process::id(),
        opt.trim_start_matches('-')
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Main.tz"), source).unwrap();
    let bin = env!("CARGO_BIN_EXE_tsuzuri");
    let out = Command::new(bin)
        .args(["run", opt, dir.to_str().unwrap()])
        .output()
        .expect("run tsuzuri");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "tsuzuri run {opt} failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn nc05_shape_concat_split_mask_and_fancy_indexing_match_numpy_semantics() {
    let source = r#"
def main :: unit -> i32
fn main _ =
    // 1. reshape_infer & try_reshape
    let t = Tensor.init [2, 3, 4] (\i -> i)
    let r = Tensor.reshape_infer [-1, 6] (Tensor.clone (ref t))
    let rv = Tensor.view (ref r)
    assert (Array.copy (Tensor.shape (ref rv)) == [4, 6])
    let bad_r = match Tensor.try_reshape [-1, -1] (Tensor.clone (ref t)) with
        | Result.Error (Tensor.Error.InvalidShape _) -> true
        | _ -> false
    assert bad_r
    let bad_div = match Tensor.try_reshape [5, -1] (Tensor.clone (ref t)) with
        | Result.Error (Tensor.Error.InvalidShape _) -> true
        | _ -> false
    assert bad_div

    // 2. Zero-copy shape views: transpose, swapaxes, moveaxis, squeeze, expand_dims, atleast_*d, flip*, rot90
    let v = Tensor.view (ref t)
    let tr = Tensor.transpose (ref v)
    assert (Array.copy (Tensor.shape (ref tr)) == [4, 3, 2])
    assert (tr[[3, 2, 1]] == 23)
    let sw = Tensor.swapaxes 0 (-1) (ref v)
    assert (Array.copy (Tensor.shape (ref sw)) == [4, 3, 2])
    let mv = Tensor.moveaxis 0 (-1) (ref v)
    assert (Array.copy (Tensor.shape (ref mv)) == [3, 4, 2])
    let s1 = Tensor.init [1, 2, 1, 3] (\i -> i)
    let s1v = Tensor.view (ref s1)
    let sq = Tensor.squeeze (ref s1v)
    let sqa = Tensor.squeeze_axis (-2) (ref s1v)
    let ed = Tensor.expand_dims (-1) (ref s1v)
    assert (Array.copy (Tensor.shape (ref sq)) == [2, 3])
    assert (Array.copy (Tensor.shape (ref sqa)) == [1, 2, 3])
    assert (Array.copy (Tensor.shape (ref ed)) == [1, 2, 1, 3, 1])
    let sc = Tensor.scalar 9
    let scv = Tensor.view (ref sc)
    let a1d = Tensor.atleast_1d (ref scv)
    let a2d = Tensor.atleast_2d (ref scv)
    let a3d = Tensor.atleast_3d (ref scv)
    assert (Array.copy (Tensor.shape (ref a1d)) == [1])
    assert (Array.copy (Tensor.shape (ref a2d)) == [1, 1])
    assert (Array.copy (Tensor.shape (ref a3d)) == [1, 1, 1])

    let g = Tensor.of_array [2, 2] [1, 2, 3, 4]
    let gv = Tensor.view (ref g)
    let g_flip = Tensor.flip (ref gv)
    let g_flr = Tensor.fliplr (ref gv)
    let g_fud = Tensor.flipud (ref gv)
    let g_r1 = Tensor.rot90 1 (ref gv)
    let g_r2 = Tensor.rot90 2 (ref gv)
    let g_r3 = Tensor.rot90 3 (ref gv)
    let g_rav = Tensor.ravel_view (ref gv)
    let g_tr = Tensor.transpose (ref gv)
    assert (Tensor.to_array (Tensor.to_tensor (ref g_flip)) == [4, 3, 2, 1])
    assert (Tensor.to_array (Tensor.to_tensor (ref g_flr)) == [2, 1, 4, 3])
    assert (Tensor.to_array (Tensor.to_tensor (ref g_fud)) == [3, 4, 1, 2])
    assert (Tensor.to_array (Tensor.to_tensor (ref g_r1)) == [2, 4, 1, 3])
    assert (Tensor.to_array (Tensor.to_tensor (ref g_r2)) == [4, 3, 2, 1])
    assert (Tensor.to_array (Tensor.to_tensor (ref g_r3)) == [3, 1, 4, 2])
    assert (Tensor.to_array (Tensor.to_tensor (ref g_rav)) == [1, 2, 3, 4])
    assert (Tensor.to_array (Tensor.flatten (ref g_tr)) == [1, 3, 2, 4])

    // 3. roll, tile, repeat, pad (all 5 modes)
    let a1 = Tensor.of_array [4] [1, 2, 3, 4]
    let a1v = Tensor.view (ref a1)
    assert (Tensor.to_array (Tensor.roll 0 1 (ref a1v)) == [4, 1, 2, 3])
    assert (Tensor.to_array (Tensor.roll 0 (-1) (ref a1v)) == [2, 3, 4, 1])
    let t2 = Tensor.of_array [2] [1, 2]
    let t2v = Tensor.view (ref t2)
    assert (Tensor.to_array (Tensor.tile [2, 2] (ref t2v)) == [1, 2, 1, 2, 1, 2, 1, 2])
    assert (Tensor.to_array (Tensor.repeat 1 2 (ref gv)) == [1, 1, 2, 2, 3, 3, 4, 4])
    let p1 = Tensor.of_array [3] [1, 2, 3]
    let p1v = Tensor.view (ref p1)
    assert (Tensor.to_array (Tensor.pad [(2, 1)] (Tensor.PadMode.Constant 9) (ref p1v)) == [9, 9, 1, 2, 3, 9])
    assert (Tensor.to_array (Tensor.pad [(2, 2)] Tensor.PadMode.Edge (ref p1v)) == [1, 1, 1, 2, 3, 3, 3])
    assert (Tensor.to_array (Tensor.pad [(2, 2)] Tensor.PadMode.Reflect (ref p1v)) == [3, 2, 1, 2, 3, 2, 1])
    assert (Tensor.to_array (Tensor.pad [(2, 2)] Tensor.PadMode.Symmetric (ref p1v)) == [2, 1, 1, 2, 3, 3, 2])
    assert (Tensor.to_array (Tensor.pad [(2, 2)] Tensor.PadMode.Wrap (ref p1v)) == [2, 3, 1, 2, 3, 1, 2])

    // 4. concatenate, stack, vstack, hstack, dstack, column_stack, split, split_at, array_split, hsplit, vsplit, dsplit
    let x = Tensor.of_array [2] [1, 2]
    let y = Tensor.of_array [2] [3, 4]
    let pair = [Tensor.view (ref x), Tensor.view (ref y)]
    let vs = match Tensor.vstack (ref pair) with | Result.Ok t -> t | Result.Error _ -> Tensor.zeros [0]
    assert (Tensor.to_array vs == [1, 2, 3, 4])
    let hs = match Tensor.hstack (ref pair) with | Result.Ok t -> t | Result.Error _ -> Tensor.zeros [0]
    assert (Tensor.to_array hs == [1, 2, 3, 4])
    let cs = match Tensor.column_stack (ref pair) with | Result.Ok t -> t | Result.Error _ -> Tensor.zeros [0]
    assert (Tensor.to_array cs == [1, 3, 2, 4])
    let st = match Tensor.stack 1 (ref pair) with | Result.Ok t -> t | Result.Error _ -> Tensor.zeros [0]
    assert (Tensor.to_array st == [1, 3, 2, 4])
    let five = Tensor.of_array [5] [1, 2, 3, 4, 5]
    let fivev = Tensor.view (ref five)
    let parts = Tensor.array_split 0 3 (ref fivev)
    assert (parts.length == 3 && Tensor.count (ref parts[0]) == 2 && Tensor.count (ref parts[2]) == 1)
    let cut = Tensor.split_at 0 [1, 3] (ref fivev)
    assert (cut.length == 3 && Tensor.to_array (Tensor.to_tensor (ref cut[1])) == [2, 3])
    let hparts = Tensor.hsplit 2 (ref gv)
    assert (Tensor.to_array (Tensor.to_tensor (ref hparts[0])) == [1, 3])
    let vparts = Tensor.vsplit 2 (ref gv)
    assert (Tensor.to_array (Tensor.to_tensor (ref vparts[1])) == [3, 4])

    // 5. Boolean mask (function + Index + IndexMut), compress, nonzero, argwhere, flatnonzero, where_select, where_scalar, clip, take, take_along_axis, put_along_axis
    let bmask = Tensor.of_array [2, 2] [false, true, true, false]
    assert (Tensor.to_array g[bmask] == [2, 3])
    assert (Tensor.to_array (Tensor.compress 1 [true, false] (ref gv)) == [1, 3])
    let sp = Tensor.of_array [2, 2] [0, 5, 0, 9]
    let spv = Tensor.view (ref sp)
    assert (Tensor.flatnonzero (ref spv) == [1, 3])
    let nz = Tensor.nonzero (ref spv)
    assert (nz.length == 2 && nz[0] == [0, 1] && nz[1] == [1, 1])
    assert (Tensor.to_array (Tensor.argwhere (ref spv)) == [0, 1, 1, 1])
    let masked_mut = Tensor.update (\m -> m[bmask] = 99) (Tensor.clone (ref g))
    assert (Tensor.to_array masked_mut == [1, 99, 99, 4])
    let bcond = Tensor.of_array [1, 2] [true, false]
    let bcondv = Tensor.view (ref bcond)
    let zeros21 = Tensor.full 0 [2, 1]
    let zeros21v = Tensor.view (ref zeros21)
    let ws = match Tensor.where_select (ref bcondv) (ref gv) (ref zeros21v) with
        | Result.Ok out -> out
        | Result.Error _ -> Tensor.zeros [0]
    assert (Tensor.to_array ws == [1, 0, 3, 0])
    assert (Tensor.to_array (Tensor.where_scalar 1 (-1) (ref bcondv)) == [1, -1])
    assert (Tensor.to_array (Tensor.clip 2 3 (ref gv)) == [2, 2, 3, 3])
    let idx1 = Tensor.of_array [2] [-1, 0]
    let idx1v = Tensor.view (ref idx1)
    assert (Tensor.to_array (Tensor.take 0 (ref idx1v) (ref gv)) == [3, 4, 1, 2])
    let idx2 = Tensor.of_array [2, 1] [1, 0]
    let idx2v = Tensor.view (ref idx2)
    assert (Tensor.to_array (Tensor.take_along_axis 1 (ref idx2v) (ref gv)) == [2, 3])
    let scatter_vals = Tensor.of_array [2, 1] [88, 77]
    let scattered = Tensor.update (\m ->
        let iv = Tensor.view (ref idx2)
        let vv = Tensor.view (ref scatter_vals)
        Tensor.put_along_axis 1 (ref iv) (ref vv) m) (Tensor.clone (ref g))
    assert (Tensor.to_array scattered == [1, 88, 77, 4])
    0i32
"#;
    // Also verify both native and WASM IR emit cleanly and deterministically.
    let module = analyze(source).unwrap_or_else(|e| panic!("{}: {}", e.code, e.message));
    for wasm in [false, true] {
        let ir = llvm::emit_target(&module, llvm::Entry::Library, wasm).unwrap();
        assert!(llvm::duplicate_declarations(&ir).is_empty());
    }
    run_native(source, "-O0");
    run_native(source, "-O3");
}
