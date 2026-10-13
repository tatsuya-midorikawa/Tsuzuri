fn check_size(size: i64, maximum: i64) -> usize {
    assert!(size >= 0 && size <= maximum);
    size as usize
}

fn hash(key: u64) -> u64 {
    let value = key
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    value ^ (value >> 29)
}

fn fold(total: u64, value: u64) -> u64 {
    total.wrapping_mul(31).wrapping_add(value)
}

fn split_owned(text: &str, separator: char) -> Vec<String> {
    text.split(separator).map(str::to_owned).collect()
}

#[no_mangle]
pub extern "C" fn rust_tokenize(size: i64, seed: u64) -> u64 {
    let n = check_size(size, 10000000);
    if n == 0 {
        return 0;
    }
    let words = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta"];
    let parts: Vec<String> = (0..n)
        .map(|index| words[(hash(seed.wrapping_add(index as u64)) >> 61) as usize].to_owned())
        .collect();
    let text = parts.join(" ");
    let tokens = split_owned(&text, ' ');
    tokens.iter().fold(tokens.len() as u64, |total, token| fold(total, token.len() as u64))
}

#[no_mangle]
pub extern "C" fn rust_format_join(size: i64, seed: u64) -> u64 {
    let n = check_size(size, 10000000);
    if n == 0 {
        return 0;
    }
    let parts: Vec<String> = (0..n)
        .map(|index| hash(seed.wrapping_add(index as u64)).to_string())
        .collect();
    let text = parts.join(",");
    let fields = split_owned(&text, ',');
    let sum = fields.iter().fold(0u64, |total, field| {
        total.wrapping_add(field.parse::<u64>().expect("generated decimal"))
    });
    fold(text.len() as u64, sum)
}

#[no_mangle]
pub extern "C" fn rust_csv_aggregate(size: i64, seed: u64) -> u64 {
    let n = check_size(size, 10000000);
    if n == 0 {
        return 0;
    }
    let parts: Vec<String> = (0..n)
        .map(|index| {
            let value = hash(seed.wrapping_add(index as u64));
            let category = (value >> 60).to_string();
            let amount = ((value >> 20) & 65535).to_string();
            let prefix = category + ",";
            prefix + &amount
        })
        .collect();
    let text = parts.join("\n");
    let lines = split_owned(&text, '\n');
    let mut sums = vec![0u64; 16];
    let mut counts = vec![0u64; 16];
    for line in &lines {
        let fields = split_owned(line, ',');
        assert_eq!(fields.len(), 2);
        let category = fields[0].parse::<u64>().expect("generated category") as usize;
        let amount = fields[1].parse::<u64>().expect("generated amount");
        sums[category] = sums[category].wrapping_add(amount);
        counts[category] = counts[category].wrapping_add(1);
    }
    (0..16).fold(fold(0, lines.len() as u64), |total, category| {
        fold(fold(total, sums[category]), counts[category])
    })
}

#[no_mangle]
pub extern "C" fn rust_map_count(size: i64, seed: u64) -> u64 {
    let n = check_size(size, 10000000);
    let mut map: Vec<(u64, u64)> = Vec::new();
    let mut set: Vec<u64> = Vec::new();
    let mut state = seed;
    for _ in 0..n {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let key = (state >> 33) & 1023;
        let position = map.partition_point(|entry| entry.0 < key);
        if position < map.len() && map[position].0 == key {
            map[position].1 = map[position].1.wrapping_add(1);
        } else {
            map.push((key, 1));
            for swap in (position + 1..map.len()).rev() {
                map.swap(swap, swap - 1);
            }
        }
        let key = key & 255;
        let position = set.partition_point(|entry| *entry < key);
        if position == set.len() || set[position] != key {
            set.push(key);
            for swap in (position + 1..set.len()).rev() {
                set.swap(swap, swap - 1);
            }
        }
    }
    let initial = (map.len() as u64)
        .wrapping_mul(1000003)
        .wrapping_add(set.len() as u64);
    (0..1024).fold(initial, |total, key| {
        let position = map.partition_point(|entry| entry.0 < key);
        let count = if position < map.len() && map[position].0 == key { map[position].1 } else { 0 };
        fold(total, count)
    })
}

#[derive(Clone, Copy, Default)]
struct Item {
    key: u64,
    id: u64,
}

#[no_mangle]
pub extern "C" fn rust_record_sort(size: i64, seed: u64) -> u64 {
    let n = check_size(size, 10000000);
    let items: Vec<Item> = (0..n)
        .map(|index| Item { key: hash(seed.wrapping_add(index as u64)) >> 44, id: index as u64 })
        .collect();
    let mut sorted = items.clone();
    let mut scratch = vec![Item::default(); n];
    let mut width = 1;
    while width < n {
        for start in (0..n).step_by(2 * width) {
            let middle = (start + width).min(n);
            let end = (start + 2 * width).min(n);
            let (mut left, mut right) = (start, middle);
            for output in start..end {
                if left < middle && (right == end || sorted[left].key <= sorted[right].key) {
                    scratch[output] = sorted[left];
                    left += 1;
                } else {
                    scratch[output] = sorted[right];
                    right += 1;
                }
            }
        }
        std::mem::swap(&mut sorted, &mut scratch);
        width *= 2;
    }
    sorted.iter().fold(0, |total, item| fold(total, item.id))
}

enum Expr {
    Lit(u64),
    Add(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
}

fn build_expr(depth: u32, key: u64) -> Box<Expr> {
    let value = hash(key);
    if depth == 0 {
        return Box::new(Expr::Lit(value >> 40));
    }
    let left = build_expr(depth - 1, key.wrapping_mul(2).wrapping_add(1));
    if value & 3 == 0 {
        return Box::new(Expr::Neg(left));
    }
    let right = build_expr(depth - 1, key.wrapping_mul(2).wrapping_add(2));
    Box::new(if value & 3 == 1 { Expr::Mul(left, right) } else { Expr::Add(left, right) })
}

fn simplify(expression: Box<Expr>) -> Box<Expr> {
    match *expression {
        Expr::Lit(value) => Box::new(Expr::Lit(value)),
        Expr::Neg(child) => {
            let child = simplify(child);
            match child.as_ref() {
                Expr::Neg(_) => {
                    if let Expr::Neg(nested) = *child { nested } else { unreachable!() }
                }
                Expr::Lit(value) => Box::new(Expr::Lit(0u64.wrapping_sub(*value))),
                _ => Box::new(Expr::Neg(child)),
            }
        }
        Expr::Add(left, right) => {
            let left = simplify(left);
            let right = simplify(right);
            match (left.as_ref(), right.as_ref()) {
                (Expr::Lit(a), Expr::Lit(b)) => Box::new(Expr::Lit(a.wrapping_add(*b))),
                _ => Box::new(Expr::Add(left, right)),
            }
        }
        Expr::Mul(left, right) => {
            let left = simplify(left);
            let right = simplify(right);
            Box::new(Expr::Mul(left, right))
        }
    }
}

fn eval(expression: &Expr) -> u64 {
    match expression {
        Expr::Lit(value) => *value,
        Expr::Neg(child) => 0u64.wrapping_sub(eval(child)),
        Expr::Add(left, right) => eval(left).wrapping_add(eval(right)),
        Expr::Mul(left, right) => eval(left).wrapping_mul(eval(right)),
    }
}

#[no_mangle]
pub extern "C" fn rust_tree_transform(size: i64, seed: u64) -> u64 {
    let n = check_size(size, 100000);
    let mut total = 0;
    for index in 0..n {
        let tree = simplify(build_expr(12, seed.wrapping_add(index as u64)));
        total = fold(total, eval(&tree));
    }
    total
}

enum Tree {
    Leaf,
    Node(Box<Tree>, Box<Tree>),
}

fn build_tree(depth: u32) -> Box<Tree> {
    if depth == 0 {
        return Box::new(Tree::Leaf);
    }
    let left = build_tree(depth - 1);
    let right = build_tree(depth - 1);
    Box::new(Tree::Node(left, right))
}

fn count_tree(tree: &Tree) -> u64 {
    match tree {
        Tree::Leaf => 1,
        Tree::Node(left, right) => 1u64.wrapping_add(count_tree(left)).wrapping_add(count_tree(right)),
    }
}

#[no_mangle]
pub extern "C" fn rust_binary_trees(size: i64, seed: u64) -> u64 {
    let n = check_size(size, 1000000);
    let mut total = 0;
    for index in 0..n {
        let tree = build_tree(4 + (seed.wrapping_add(index as u64) & 7) as u32);
        total = fold(total, count_tree(&tree));
    }
    total
}

#[no_mangle]
pub extern "C" fn rust_closure_pipeline(size: i64, seed: u64) -> u64 {
    let n = check_size(size, 10000000);
    let functions: Vec<Box<dyn Fn(u64) -> u64>> = (0..4u64)
        .map(|key| {
            Box::new(move |value: u64| value.wrapping_mul(6364136223846793005).wrapping_add(seed ^ key))
                as Box<dyn Fn(u64) -> u64>
        })
        .collect();
    let values: Vec<u64> = (0..n).map(|index| seed.wrapping_add(index as u64)).collect();
    let mapped: Vec<u64> = values.iter().map(|value| functions[(value & 3) as usize](*value)).collect();
    mapped.iter().fold(0, |total, value| {
        if value & 1 == 0 { fold(total, *value) } else { total }
    })
}

struct Edge {
    source: u64,
    destination: u64,
}

#[no_mangle]
pub extern "C" fn rust_graph_bfs(size: i64, seed: u64) -> u64 {
    let n = check_size(size, 1000000);
    if n == 0 {
        return 0;
    }
    let edges: Vec<Edge> = (0..4 * n)
        .map(|index| {
            let value = hash(seed.wrapping_add(index as u64));
            Edge { source: (value >> 32) % n as u64, destination: (value & 0xffffffff) % n as u64 }
        })
        .collect();
    let mut degrees = vec![0u64; n];
    for edge in &edges {
        degrees[edge.source as usize] = degrees[edge.source as usize].wrapping_add(1);
    }
    let mut offsets = vec![0u64; n + 1];
    for index in 0..n {
        offsets[index + 1] = offsets[index].wrapping_add(degrees[index]);
    }
    let mut cursor = offsets[..n].to_vec();
    let mut adjacent = vec![0u64; 4 * n];
    for edge in &edges {
        let source = edge.source as usize;
        adjacent[cursor[source] as usize] = edge.destination;
        cursor[source] = cursor[source].wrapping_add(1);
    }
    let mut distances = vec![-1i64; n];
    distances[0] = 0;
    let mut queue = Vec::with_capacity(n);
    queue.push(0usize);
    let mut head = 0;
    while head < queue.len() {
        let source = queue[head];
        head += 1;
        for position in offsets[source]..offsets[source + 1] {
            let destination = adjacent[position as usize] as usize;
            if distances[destination] == -1 {
                distances[destination] = distances[source].wrapping_add(1);
                queue.push(destination);
            }
        }
    }
    distances.iter().fold(0, |total, distance| fold(total, *distance as u64))
}

fn matrix_value(row: usize, column: usize) -> f64 {
    let sum = (row + column) as u64;
    1.0 / (sum * (sum + 1) / 2 + row as u64 + 1) as f64
}

fn multiply(values: &[f64], transpose: bool) -> Vec<f64> {
    (0..values.len())
        .map(|row| {
            let mut total = 0.0;
            for column in 0..values.len() {
                let coefficient = if transpose { matrix_value(column, row) } else { matrix_value(row, column) };
                total += coefficient * values[column];
            }
            total
        })
        .collect()
}

fn multiply_ata(values: &[f64]) -> Vec<f64> {
    let intermediate = multiply(values, false);
    multiply(&intermediate, true)
}

#[no_mangle]
pub extern "C" fn rust_spectral_norm(size: i64, seed: u64) -> u64 {
    let n = check_size(size, 5000);
    if n == 0 {
        return 0;
    }
    let mut u = vec![1.0; n];
    let mut v = vec![0.0; n];
    for _ in 0..10 {
        v = multiply_ata(&u);
        u = multiply_ata(&v);
    }
    let (mut uv, mut vv) = (0.0, 0.0);
    for index in 0..n {
        uv += u[index] * v[index];
        vv += v[index] * v[index];
    }
    let scaled = (uv / vv).sqrt() * 1e15;
    assert!(scaled.is_finite() && scaled >= 0.0 && scaled < 18446744073709551616.0);
    scaled as u64 ^ seed
}
