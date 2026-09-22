#![no_std]

extern "C" {
    fn malloc(size: usize) -> *mut u64;
    fn free(value: *mut u64);
    fn abort() -> !;
}

fn check_count(count: i64) {
    if !(0..=100_000_000).contains(&count) {
        unsafe { abort() }
    }
}

fn step(state: u64, salt: u64) -> u64 {
    (state ^ (state >> 13))
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(salt)
        .wrapping_add(1_442_695_040_888_963_407)
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_while_mix(mut count: i64, mut seed: u64) -> u64 {
    check_count(count);
    while count > 0 {
        seed = step(seed, count as u64);
        count -= 1;
    }
    seed
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_for_mix(count: i64, mut seed: u64) -> u64 {
    check_count(count);
    for remaining in (1..=count as i32).rev() {
        seed = step(seed, remaining as u64);
    }
    seed
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_tail_mix(mut count: i64, mut seed: u64) -> u64 {
    check_count(count);
    loop {
        match count {
            0 => return seed,
            remaining => {
                seed = step(seed, remaining as u64);
                count = remaining - 1;
            }
        }
    }
}

fn classify(n: u64) -> u64 {
    match n {
        0 => 17,
        1 => 3,
        2 => 29,
        3 => 7,
        4 => 61,
        5 => 11,
        6 => 83,
        7 => 5,
        8 => 47,
        9 => 19,
        10 => 101,
        11 => 31,
        12 => 53,
        13 => 23,
        14 => 97,
        _ => 13,
    }
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_match_dispatch(count: i64, seed: u64) -> u64 {
    check_count(count);
    let mut total = seed;
    for index in 0..count {
        total = total.wrapping_add(
            classify(seed.wrapping_add(index as u64) & 15).wrapping_mul(index as u64 + 1),
        );
    }
    total
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_array_sum(count: i64, seed: u64) -> u64 {
    check_count(count);
    unsafe {
        // All languages include the same uninitialized system allocation and full initialization.
        let values = malloc((count as usize * 8).max(1));
        if values.is_null() {
            abort();
        }
        for index in 0..count as usize {
            values.add(index).write(
                (index as u64 ^ seed)
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407),
            );
        }
        let mut total = 0u64;
        for value in core::slice::from_raw_parts(values, count as usize) {
            total = total.wrapping_add(*value);
        }
        free(values);
        total
    }
}
