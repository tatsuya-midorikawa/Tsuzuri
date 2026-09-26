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

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_array_copy(count: i64, seed: u64) -> u64 {
    check_count(count);
    unsafe {
        let values = malloc((count as usize * 8).max(1));
        let copied = malloc((count as usize * 8).max(1));
        if values.is_null() || copied.is_null() {
            abort();
        }
        for index in 0..count as usize {
            values.add(index).write(
                (index as u64 ^ seed)
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407),
            );
        }
        core::ptr::copy_nonoverlapping(values, copied, count as usize);
        let mut total = 0u64;
        for value in core::slice::from_raw_parts(values, count as usize) {
            total = total.wrapping_add(*value);
        }
        for value in core::slice::from_raw_parts(copied, count as usize) {
            total = total.wrapping_add(*value);
        }
        free(copied);
        free(values);
        total
    }
}

#[repr(C)]
struct Node {
    value: u64,
    next: *mut Node,
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_list_sum(count: i64, seed: u64) -> u64 {
    check_count(count);
    unsafe {
        let mut head = core::ptr::null_mut::<Node>();
        let mut tail = &mut head as *mut *mut Node;
        for index in 0..count as u64 {
            let node = malloc(core::mem::size_of::<Node>()).cast::<Node>();
            if node.is_null() {
                abort();
            }
            node.write(Node {
                value: (index ^ seed)
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407),
                next: core::ptr::null_mut(),
            });
            tail.write(node);
            tail = &raw mut (*node).next;
        }
        let mut total = 0u64;
        let mut node = head;
        while !node.is_null() {
            total = total.wrapping_add((*node).value);
            node = (*node).next;
        }
        while !head.is_null() {
            let next = (*head).next;
            free(head.cast());
            head = next;
        }
        total
    }
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_closure_capture(count: i64, seed: u64) -> u64 {
    check_count(count);
    let first = |value| step(value, seed);
    let second = |value| step(value, seed ^ 71);
    let transform: &dyn Fn(u64) -> u64 = if seed & 1 == 0 { &first } else { &second };
    let mut state = seed;
    for _ in 0..count {
        state = transform(state);
    }
    state
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_closure_churn(count: i64, seed: u64) -> u64 {
    check_count(count);
    let mut state = seed;
    for _ in 0..count {
        let captured = state;
        let first = |value| step(value, captured);
        let second = |value| step(value, captured ^ 71);
        let transform: &dyn Fn(u64) -> u64 = if captured & 1 == 0 { &first } else { &second };
        let copy = transform;
        state = copy(state);
        state = transform(state);
    }
    state
}

struct State<Value> {
    value: Value,
    remaining: i64,
}

fn advance(state: &State<u64>) -> State<u64> {
    State {
        value: step(state.value, state.remaining as u64),
        remaining: state.remaining - 1,
    }
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_record_pipeline(count: i64, seed: u64) -> u64 {
    check_count(count);
    let mut state = State {
        value: seed,
        remaining: count,
    };
    while state.remaining > 0 {
        state = advance(&state);
    }
    state.value
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_integer128_mix(count: i64, seed: u64) -> u64 {
    check_count(count);
    let mut state = ((seed as u128) << 64) | 1_442_695_040_888_963_407;
    for remaining in (1..=count).rev() {
        state = (state ^ (state >> 43))
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(remaining as u128);
    }
    (state ^ (state >> 64)) as u64
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_float32_mix(count: i64, seed: u64) -> u64 {
    check_count(count);
    let mut state = (seed & 65535) as f32 / 16.0 + 1.0;
    for index in 0..count {
        state = state * 1.000001 + (index & 7) as f32 / 16.0;
    }
    state as u64
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_float64_mix(count: i64, seed: u64) -> u64 {
    check_count(count);
    let mut state = (seed & 65535) as f64 / 16.0 + 1.0;
    for index in 0..count {
        state = state * 1.0000001 + (index & 7) as f64 / 16.0;
    }
    state as u64
}
