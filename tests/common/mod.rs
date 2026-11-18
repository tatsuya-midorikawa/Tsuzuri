//! Helpers shared by the integration tests that include `mod common`.
#![allow(dead_code)]

pub mod ir_scan;

/// The items of `items` that slice `slice` (from 0) of `slices` checks: those whose index is `slice` modulo
/// `slices`. The slices of a list together take every item once.
pub fn slice<T>(items: &[T], slice: usize, slices: usize) -> impl Iterator<Item = &T> {
    assert!(slice < slices, "slice {slice} of {slices}");
    items.iter().skip(slice).step_by(slices)
}

/// Declares a slow test as `slices` tests that each check a part of the work, so that `cargo nextest` runs them in
/// processes of their own and CI can spread them over shards. `$body(slice, slices)` checks the slice `slice`
/// (from 0) of `slices`, usually the items that [`slice`] gives it. Each test is named after the body with
/// `_<k>_of_<n>`, so that `cargo test <body>` still runs them all:
///
/// ```ignore
/// common::slices!(check_every_project, 2:
///     1 => check_every_project_1_of_2,
///     2 => check_every_project_2_of_2,
/// );
/// ```
///
/// `cargo test` runs the slices of a file as threads of one process: a body that writes files gives each slice
/// a path of its own.
// Not every file that includes `mod common` has slices.
#[allow(unused_macros)]
macro_rules! slices {
    ($body:ident, $slices:literal: $($slice:literal => $name:ident),+ $(,)?) => {
        // One test for each slice from 1 to `$slices`: the names, which the compiler keeps apart, carry the number.
        const _: () = {
            assert!([$($slice),+].len() == $slices, "one test per slice");
            $(assert!($slice >= 1 && $slice <= $slices, "a slice from 1 to the count");)+
        };
        $(
            #[test]
            fn $name() {
                assert_eq!(
                    stringify!($name),
                    format!("{}_{}_of_{}", stringify!($body), $slice, $slices),
                    "the name of a slice names its body and its number"
                );
                $body($slice - 1, $slices);
            }
        )+
    };
}
#[allow(unused_imports)]
pub(crate) use slices;
