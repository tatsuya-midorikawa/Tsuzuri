#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_mix(iterations: i64, seed: i64) -> i64 {
    if iterations < 0 {
        std::process::abort();
    }
    let mut state = seed as u64;
    for _ in 0..iterations {
        state = (state ^ (state >> 13))
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
    }
    state as i64
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_mandelbrot(size: i64, seed: i64) -> i64 {
    if !(0..=4096).contains(&size) {
        std::process::abort();
    }
    if size == 0 {
        return 0;
    }
    let dx = 3.0 / size as f64;
    let dy = 2.0 / size as f64;
    let offset = seed as f64 * 0.000001;
    let mut total = 0;
    for index in 0..size * size {
        let cr = (index % size) as f64 * dx - 2.0 + offset;
        let ci = (index / size) as f64 * dy - 1.0;
        let mut real = 0.0;
        let mut imaginary = 0.0;
        let mut count = 0;
        while count < 256 && real * real + imaginary * imaginary <= 4.0 {
            let next_real = real * real - imaginary * imaginary + cr;
            imaginary = 2.0 * real * imaginary + ci;
            real = next_real;
            count += 1;
        }
        total += count;
    }
    total
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_array_sum(count: i64, seed: i64) -> i64 {
    if !(0..=100000000).contains(&count) {
        std::process::abort();
    }
    let values: Vec<u64> = (0..count as u64)
        .map(|index| {
            (index ^ seed as u64)
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407)
        })
        .collect();
    values.iter().copied().fold(0, u64::wrapping_add) as i64
}

fn make_text(count: i64, seed: i64) -> Vec<u16> {
    if !(0..=100000000).contains(&count) {
        std::process::abort();
    }
    if count == 0 {
        return Vec::new();
    }
    let mut text = if seed & 1 == 0 {
        vec![65, 122, 48, 57, 45, 95, 32, 10]
    } else {
        vec![65, 0, 937, 0xD83D, 0xDE00, 0x4E2D, 122, 10]
    };
    while text.len() < count as usize {
        let copy = text.clone();
        text = [copy, text].concat();
    }
    text
}

fn text_sum(text: &[u16]) -> i64 {
    text.iter().map(|&unit| i64::from(unit)).sum()
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_utf16_scan(count: i64, seed: i64) -> i64 {
    text_sum(&make_text(count, seed))
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_utf16_compare(count: i64, seed: i64) -> i64 {
    let text = make_text(count, seed);
    let mut left = text.clone();
    let mut right = text;
    left.push(if seed & 2 == 0 { 97 } else { 98 });
    right.push(if seed & 4 == 0 { 97 } else { 98 });
    if left == right {
        1
    } else if left < right {
        2
    } else {
        4
    }
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_utf16_validate(count: i64, seed: i64) -> i64 {
    let mut text = make_text(count, seed);
    if seed & 8 != 0 {
        text.push(0xD800);
    }
    let mut repaired = Vec::with_capacity(text.len());
    for decoded in char::decode_utf16(text.iter().copied()) {
        let mut units = [0; 2];
        repaired.extend_from_slice(
            decoded
                .unwrap_or(char::REPLACEMENT_CHARACTER)
                .encode_utf16(&mut units),
        );
    }
    text_sum(&repaired)
        + i64::from(char::decode_utf16(text.iter().copied()).all(|value| value.is_ok()))
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_utf8_roundtrip(count: i64, seed: i64) -> i64 {
    let text = make_text(count, seed);
    let bytes = String::from_utf16(&text).unwrap().into_bytes();
    let restored: Vec<u16> = std::str::from_utf8(&bytes)
        .unwrap()
        .encode_utf16()
        .collect();
    assert_eq!(text, restored);
    text_sum(&restored) + bytes.len() as i64
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_format_parse(count: i64, seed: i64) -> i64 {
    if !(0..=100000000).contains(&count) {
        std::process::abort();
    }
    let mut state = seed as u64;
    for index in 0..count as u64 {
        state = (state ^ (state >> 13))
            .wrapping_mul(6364136223846793005)
            .wrapping_add(index)
            .wrapping_add(1442695040888963407);
        let text: Vec<u16> = state.to_string().encode_utf16().collect();
        state = String::from_utf16(&text).unwrap().parse::<u64>().unwrap() ^ text.len() as u64;
    }
    state as i64
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_math_intrinsics(count: i64, seed: i64) -> i64 {
    if !(0..=100000000).contains(&count) {
        std::process::abort();
    }
    let mut state = ((seed & 65535) + 1) as f64 / 16.0;
    for index in 0..count {
        state = (state.abs() + (index & 255) as f64).sqrt();
        state = (state * 16.0).floor() / 16.0 + state.ceil() / 1024.0;
    }
    (state * 1048576.0) as i64
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_task_sequence(count: i64, seed: i64) -> i64 {
    if !(0..=100000000).contains(&count) {
        std::process::abort();
    }
    let mut state = seed as u64;
    for _ in 0..count {
        let task = || state;
        let value = task();
        state = (value ^ (value >> 13))
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
    }
    state as i64
}

fn task_partition(count: i64, seed: i64, first: usize, stride: usize) -> u64 {
    (first..16).step_by(stride).fold(0u64, |total, index| {
        total.wrapping_add(rust_mix(count, seed ^ index as i64) as u64)
    })
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_task_sequential(count: i64, seed: i64) -> i64 {
    if !(0..=100000000).contains(&count) {
        std::process::abort();
    }
    task_partition(count, seed, 0, 1) as i64
}

#[no_mangle]
#[inline(never)]
pub extern "C" fn rust_task_parallel(count: i64, seed: i64) -> i64 {
    if !(0..=100000000).contains(&count) {
        std::process::abort();
    }
    static PARALLELISM: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    let parallelism = *PARALLELISM.get_or_init(|| {
        std::thread::available_parallelism()
            .map_or(1, usize::from)
            .min(16)
    });
    std::thread::scope(|scope| {
        let workers: Vec<_> = (1..parallelism)
            .map(|first| scope.spawn(move || task_partition(count, seed, first, parallelism)))
            .collect();
        let mut total = task_partition(count, seed, 0, parallelism);
        for worker in workers {
            total = total.wrapping_add(worker.join().unwrap());
        }
        total as i64
    })
}
