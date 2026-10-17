//! Function values (PM02): the shape of their IR and the size model behind the layout limit.
use tsuzuri::analyze;

/// A record with `count` function-valued fields.
fn handlers(count: usize) -> String {
    let fields = (0..count)
        .map(|index| format!("f{index}: i64 -> i64"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("record Handlers {{ {fields} }}\nexport def probe :: i64 -> i64\nfn probe n = n\n")
}

/// The limit on an inline value (E1010) counts a function value as 32 bytes, an upper bound of the
/// size LLVM gives it. Shrinking the value does not lower the estimate: that would change which
/// programs are accepted (PM01 D4, PM02 P2-c), so 2,048 function values still fill the limit.
#[test]
fn the_layout_limit_counts_a_function_value_as_32_bytes() {
    analyze(&handlers(2048)).expect("2048 function values fit in 64 KiB");
    let error = analyze(&handlers(2049)).expect_err("one more does not");
    assert_eq!(error.code, "E1010", "{}", error.message);
}

#[test]
fn nested_fixed_arrays_of_function_values_count_the_same_estimate() {
    let table = |rows: usize| {
        format!(
            "record Tables {{ rows: [[i64 -> i64; 1024]; {rows}] }}\nexport def probe :: i64 -> i64\nfn probe n = n\n"
        )
    };
    analyze(&table(2)).expect("2 * 1024 * 32 bytes is exactly the limit");
    let error = analyze(&table(3)).expect_err("3 * 1024 * 32 bytes is over it");
    assert_eq!(error.code, "E1010", "{}", error.message);
}
