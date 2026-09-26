#![no_std]

extern "C" {
    fn malloc(size: usize) -> *mut u64;
    fn free(value: *mut u64);
    fn abort() -> !;
}

fn mix(value: u64, salt: u64) -> u64 {
    (value ^ (value >> 13))
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(salt)
}

fn run_loop<const KIND: u8>(count: i64, seed: i64) -> i64 {
    if count < 0 {
        unsafe { abort() }
    }
    let mut state = seed as u64;
    for remaining in (1..=count).rev() {
        let salt = 1_442_695_040_888_963_407u64.wrapping_add(remaining as u64);
        state = match KIND {
            0 => mix(state, salt),
            1 => {
                let first = state ^ salt;
                let result = if state & 7 == 0 {
                    Err(first)
                } else {
                    Ok(first)
                };
                result
                    .and_then(|value| {
                        let second = mix(value, salt);
                        if value & 3 == 0 {
                            Err(second)
                        } else {
                            Ok(second ^ salt)
                        }
                    })
                    .unwrap_or_else(|error| error)
            }
            2 => mix(state, salt)
                .wrapping_add(mix(state ^ 71, salt))
                .wrapping_add(mix(state ^ 113, salt)),
            3 => {
                let value = if state & 7 == 0 {
                    None
                } else {
                    Some(state ^ salt)
                };
                value.map(|value| mix(value, salt)).unwrap_or(state ^ salt)
            }
            _ => mix(state, salt) ^ if state & 7 == 0 { 0 } else { 12 },
        };
    }
    state as i64
}

macro_rules! loop_kernel {
    ($name:ident, $kind:expr) => {
        #[no_mangle]
        #[inline(never)]
        pub extern "C" fn $name(count: i64, seed: i64) -> i64 {
            run_loop::<$kind>(count, seed)
        }
    };
}

loop_kernel!(rust_bind, 0);
loop_kernel!(rust_checked, 1);
loop_kernel!(rust_delayed, 2);
loop_kernel!(rust_std_option, 3);
loop_kernel!(rust_std_result, 1);
loop_kernel!(rust_std_option_owned, 4);

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_array_for(count: i64, seed: i64) -> i64 {
    if !(0..=100_000_000).contains(&count) {
        unsafe { abort() }
    }
    unsafe {
        let values = malloc((count as usize * 8).max(1));
        if values.is_null() {
            abort();
        }
        for index in 0..count as usize {
            values.add(index).write(mix(index as u64, seed as u64));
        }
        let mut total = 0u64;
        let scale = seed as u64 | 1;
        for &value in core::slice::from_raw_parts(values, count as usize) {
            total = total.wrapping_add((value ^ (value >> 17)).wrapping_mul(scale));
        }
        free(values);
        total as i64
    }
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_array_bind(count: i64, seed: i64) -> i64 {
    rust_array_for(count, seed)
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_owned_capture(count: i64, seed: i64) -> i64 {
    if count < 0 {
        unsafe { abort() }
    }
    let mut values = [0u64; 256];
    for (index, value) in values.iter_mut().enumerate() {
        *value = mix(index as u64, seed as u64);
    }
    let mut state = seed as u64;
    for _ in 0..count {
        state = mix(values[(state & 255) as usize], state);
    }
    state as i64
}
