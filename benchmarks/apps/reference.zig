const std = @import("std");
const allocator = std.heap.c_allocator;

fn checkSize(size: i64, maximum: i64) usize {
    std.debug.assert(size >= 0 and size <= maximum);
    return @intCast(size);
}

fn hash(key: u64) u64 {
    const value = key *% 6364136223846793005 +% 1442695040888963407;
    return value ^ (value >> 29);
}

fn fold(total: u64, value: u64) u64 {
    return total *% 31 +% value;
}

fn decimal(value: u64) []const u8 {
    return std.fmt.allocPrint(allocator, "{d}", .{value}) catch @panic("allocation failed");
}

fn parseU64(text: []const u8) u64 {
    return std.fmt.parseInt(u64, text, 10) catch @panic("generated decimal");
}

fn freeStrings(parts: []const []const u8) void {
    for (parts) |part| allocator.free(part);
    allocator.free(parts);
}

fn splitOwned(text: []const u8, separator: u8) [][]const u8 {
    var count: usize = 1;
    for (text) |value| {
        if (value == separator) count += 1;
    }
    const parts = allocator.alloc([]const u8, count) catch @panic("allocation failed");
    var iterator = std.mem.splitScalar(u8, text, separator);
    var index: usize = 0;
    while (iterator.next()) |part| : (index += 1) {
        parts[index] = allocator.dupe(u8, part) catch @panic("allocation failed");
    }
    return parts;
}

export fn zig_tokenize(size: i64, seed: u64) u64 {
    const n = checkSize(size, 10000000);
    if (n == 0) return 0;
    const words = [_][]const u8{ "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta" };
    const parts = allocator.alloc([]const u8, n) catch @panic("allocation failed");
    defer freeStrings(parts);
    for (parts, 0..) |*part, index| {
        part.* = allocator.dupe(u8, words[hash(seed +% @as(u64, @intCast(index))) >> 61]) catch @panic("allocation failed");
    }
    const text = std.mem.join(allocator, " ", parts) catch @panic("allocation failed");
    defer allocator.free(text);
    const tokens = splitOwned(text, ' ');
    defer freeStrings(tokens);
    var total: u64 = @intCast(tokens.len);
    for (tokens) |token| total = fold(total, @intCast(token.len));
    return total;
}

export fn zig_format_join(size: i64, seed: u64) u64 {
    const n = checkSize(size, 10000000);
    if (n == 0) return 0;
    const parts = allocator.alloc([]const u8, n) catch @panic("allocation failed");
    defer freeStrings(parts);
    for (parts, 0..) |*part, index| part.* = decimal(hash(seed +% @as(u64, @intCast(index))));
    const text = std.mem.join(allocator, ",", parts) catch @panic("allocation failed");
    defer allocator.free(text);
    const fields = splitOwned(text, ',');
    defer freeStrings(fields);
    var sum: u64 = 0;
    for (fields) |field| sum +%= parseU64(field);
    return fold(@intCast(text.len), sum);
}

export fn zig_csv_aggregate(size: i64, seed: u64) u64 {
    const n = checkSize(size, 10000000);
    if (n == 0) return 0;
    const parts = allocator.alloc([]const u8, n) catch @panic("allocation failed");
    defer freeStrings(parts);
    for (parts, 0..) |*part, index| {
        const value = hash(seed +% @as(u64, @intCast(index)));
        const category = decimal(value >> 60);
        defer allocator.free(category);
        const amount = decimal((value >> 20) & 65535);
        defer allocator.free(amount);
        const prefix = std.mem.concat(allocator, u8, &.{ category, "," }) catch @panic("allocation failed");
        defer allocator.free(prefix);
        part.* = std.mem.concat(allocator, u8, &.{ prefix, amount }) catch @panic("allocation failed");
    }
    const text = std.mem.join(allocator, "\n", parts) catch @panic("allocation failed");
    defer allocator.free(text);
    const lines = splitOwned(text, '\n');
    defer freeStrings(lines);
    const sums = allocator.alloc(u64, 16) catch @panic("allocation failed");
    defer allocator.free(sums);
    const counts = allocator.alloc(u64, 16) catch @panic("allocation failed");
    defer allocator.free(counts);
    @memset(sums, 0);
    @memset(counts, 0);
    for (lines) |line| {
        const fields = splitOwned(line, ',');
        defer freeStrings(fields);
        std.debug.assert(fields.len == 2);
        const category = parseU64(fields[0]);
        sums[category] +%= parseU64(fields[1]);
        counts[category] +%= 1;
    }
    var total = fold(0, @intCast(lines.len));
    for (0..16) |category| total = fold(fold(total, sums[category]), counts[category]);
    return total;
}

const Entry = struct { key: u64, count: u64 };

fn mapLowerBound(entries: []const Entry, key: u64) usize {
    var lower: usize = 0;
    var upper = entries.len;
    while (lower < upper) {
        const middle = lower + (upper - lower) / 2;
        if (entries[middle].key < key) lower = middle + 1 else upper = middle;
    }
    return lower;
}

fn setLowerBound(entries: []const u64, key: u64) usize {
    var lower: usize = 0;
    var upper = entries.len;
    while (lower < upper) {
        const middle = lower + (upper - lower) / 2;
        if (entries[middle] < key) lower = middle + 1 else upper = middle;
    }
    return lower;
}

export fn zig_map_count(size: i64, seed: u64) u64 {
    const n = checkSize(size, 10000000);
    var map: std.ArrayList(Entry) = .empty;
    defer map.deinit(allocator);
    var set: std.ArrayList(u64) = .empty;
    defer set.deinit(allocator);
    var state = seed;
    for (0..n) |_| {
        state = state *% 6364136223846793005 +% 1442695040888963407;
        const key = (state >> 33) & 1023;
        const position = mapLowerBound(map.items, key);
        if (position < map.items.len and map.items[position].key == key) {
            map.items[position].count +%= 1;
        } else {
            map.append(allocator, .{ .key = key, .count = 1 }) catch @panic("allocation failed");
            var swap = map.items.len - 1;
            while (swap > position) : (swap -= 1) std.mem.swap(Entry, &map.items[swap], &map.items[swap - 1]);
        }
        const member = key & 255;
        const slot = setLowerBound(set.items, member);
        if (slot == set.items.len or set.items[slot] != member) {
            set.append(allocator, member) catch @panic("allocation failed");
            var swap = set.items.len - 1;
            while (swap > slot) : (swap -= 1) std.mem.swap(u64, &set.items[swap], &set.items[swap - 1]);
        }
    }
    var total: u64 = @as(u64, @intCast(map.items.len)) *% 1000003 +% @as(u64, @intCast(set.items.len));
    for (0..1024) |key| {
        const position = mapLowerBound(map.items, @intCast(key));
        const count = if (position < map.items.len and map.items[position].key == key) map.items[position].count else 0;
        total = fold(total, count);
    }
    return total;
}

const Item = struct { key: u64, id: u64 };

export fn zig_record_sort(size: i64, seed: u64) u64 {
    const n = checkSize(size, 10000000);
    const items = allocator.alloc(Item, n) catch @panic("allocation failed");
    defer allocator.free(items);
    for (items, 0..) |*item, index| item.* = .{ .key = hash(seed +% @as(u64, @intCast(index))) >> 44, .id = @intCast(index) };
    var sorted = allocator.dupe(Item, items) catch @panic("allocation failed");
    defer allocator.free(sorted);
    var scratch = allocator.alloc(Item, n) catch @panic("allocation failed");
    defer allocator.free(scratch);
    var width: usize = 1;
    while (width < n) : (width *= 2) {
        var start: usize = 0;
        while (start < n) : (start += 2 * width) {
            const middle = @min(start + width, n);
            const end = @min(start + 2 * width, n);
            var left = start;
            var right = middle;
            for (start..end) |output| {
                if (left < middle and (right == end or sorted[left].key <= sorted[right].key)) {
                    scratch[output] = sorted[left];
                    left += 1;
                } else {
                    scratch[output] = sorted[right];
                    right += 1;
                }
            }
        }
        std.mem.swap([]Item, &sorted, &scratch);
    }
    var total: u64 = 0;
    for (sorted) |item| total = fold(total, item.id);
    return total;
}

const Pair = struct { left: *Expr, right: *Expr };
const Expr = union(enum) { lit: u64, add: Pair, mul: Pair, neg: *Expr };

fn makeExpr(value: Expr) *Expr {
    const result = allocator.create(Expr) catch @panic("allocation failed");
    result.* = value;
    return result;
}

fn buildExpr(depth: u32, key: u64) *Expr {
    const value = hash(key);
    if (depth == 0) return makeExpr(.{ .lit = value >> 40 });
    const left = buildExpr(depth - 1, key *% 2 +% 1);
    if (value & 3 == 0) return makeExpr(.{ .neg = left });
    const right = buildExpr(depth - 1, key *% 2 +% 2);
    const pair: Pair = .{ .left = left, .right = right };
    return if (value & 3 == 1) makeExpr(.{ .mul = pair }) else makeExpr(.{ .add = pair });
}

fn simplify(expression: *Expr) *Expr {
    defer allocator.destroy(expression);
    switch (expression.*) {
        .lit => |value| return makeExpr(.{ .lit = value }),
        .neg => |original| {
            const child = simplify(original);
            switch (child.*) {
                .neg => |nested| {
                    allocator.destroy(child);
                    return nested;
                },
                .lit => |value| {
                    const result = makeExpr(.{ .lit = @as(u64, 0) -% value });
                    allocator.destroy(child);
                    return result;
                },
                else => return makeExpr(.{ .neg = child }),
            }
        },
        .add => |pair| {
            const left = simplify(pair.left);
            const right = simplify(pair.right);
            if (std.meta.activeTag(left.*) == .lit and std.meta.activeTag(right.*) == .lit) {
                const result = makeExpr(.{ .lit = left.lit +% right.lit });
                allocator.destroy(left);
                allocator.destroy(right);
                return result;
            }
            return makeExpr(.{ .add = .{ .left = left, .right = right } });
        },
        .mul => |pair| {
            const left = simplify(pair.left);
            const right = simplify(pair.right);
            return makeExpr(.{ .mul = .{ .left = left, .right = right } });
        },
    }
}

fn eval(expression: *const Expr) u64 {
    return switch (expression.*) {
        .lit => |value| value,
        .neg => |child| @as(u64, 0) -% eval(child),
        .add => |pair| eval(pair.left) +% eval(pair.right),
        .mul => |pair| eval(pair.left) *% eval(pair.right),
    };
}

fn freeExpr(expression: *Expr) void {
    switch (expression.*) {
        .lit => {},
        .neg => |child| freeExpr(child),
        .add, .mul => |pair| {
            freeExpr(pair.left);
            freeExpr(pair.right);
        },
    }
    allocator.destroy(expression);
}

export fn zig_tree_transform(size: i64, seed: u64) u64 {
    const n = checkSize(size, 100000);
    var total: u64 = 0;
    for (0..n) |index| {
        const tree = simplify(buildExpr(12, seed +% @as(u64, @intCast(index))));
        total = fold(total, eval(tree));
        freeExpr(tree);
    }
    return total;
}

const Tree = struct { left: ?*Tree, right: ?*Tree };

fn buildTree(depth: u32) *Tree {
    var left: ?*Tree = null;
    var right: ?*Tree = null;
    if (depth != 0) {
        left = buildTree(depth - 1);
        right = buildTree(depth - 1);
    }
    const result = allocator.create(Tree) catch @panic("allocation failed");
    result.* = .{ .left = left, .right = right };
    return result;
}

fn countTree(tree: *const Tree) u64 {
    if (tree.left == null) return 1;
    return 1 +% countTree(tree.left.?) +% countTree(tree.right.?);
}

fn freeTree(tree: *Tree) void {
    if (tree.left) |left| {
        freeTree(left);
        freeTree(tree.right.?);
    }
    allocator.destroy(tree);
}

export fn zig_binary_trees(size: i64, seed: u64) u64 {
    const n = checkSize(size, 1000000);
    var total: u64 = 0;
    for (0..n) |index| {
        const tree = buildTree(4 + @as(u32, @intCast((seed +% @as(u64, @intCast(index))) & 7)));
        total = fold(total, countTree(tree));
        freeTree(tree);
    }
    return total;
}

const Capture = struct { seed: u64, key: u64 };
const Closure = struct { capture: *Capture, call: *const fn (*const Capture, u64) u64 };

fn applyClosure(capture: *const Capture, value: u64) u64 {
    return value *% 6364136223846793005 +% (capture.seed ^ capture.key);
}

export fn zig_closure_pipeline(size: i64, seed: u64) u64 {
    const n = checkSize(size, 10000000);
    const functions = allocator.alloc(Closure, 4) catch @panic("allocation failed");
    defer allocator.free(functions);
    for (functions, 0..) |*function, key| {
        const capture = allocator.create(Capture) catch @panic("allocation failed");
        capture.* = .{ .seed = seed, .key = @intCast(key) };
        function.* = .{ .capture = capture, .call = applyClosure };
    }
    defer for (functions) |function| allocator.destroy(function.capture);
    const values = allocator.alloc(u64, n) catch @panic("allocation failed");
    defer allocator.free(values);
    const mapped = allocator.alloc(u64, n) catch @panic("allocation failed");
    defer allocator.free(mapped);
    for (values, 0..) |*value, index| value.* = seed +% @as(u64, @intCast(index));
    for (values, 0..) |value, index| {
        const function = functions[value & 3];
        mapped[index] = function.call(function.capture, value);
    }
    var total: u64 = 0;
    for (mapped) |value| {
        if (value & 1 == 0) total = fold(total, value);
    }
    return total;
}

const Edge = struct { source: u64, destination: u64 };

export fn zig_graph_bfs(size: i64, seed: u64) u64 {
    const n = checkSize(size, 1000000);
    if (n == 0) return 0;
    const edges = allocator.alloc(Edge, 4 * n) catch @panic("allocation failed");
    defer allocator.free(edges);
    for (edges, 0..) |*edge, index| {
        const value = hash(seed +% @as(u64, @intCast(index)));
        edge.* = .{ .source = (value >> 32) % n, .destination = (value & 0xffffffff) % n };
    }
    const degrees = allocator.alloc(u64, n) catch @panic("allocation failed");
    defer allocator.free(degrees);
    @memset(degrees, 0);
    for (edges) |edge| degrees[edge.source] +%= 1;
    const offsets = allocator.alloc(u64, n + 1) catch @panic("allocation failed");
    defer allocator.free(offsets);
    offsets[0] = 0;
    for (degrees, 0..) |degree, index| offsets[index + 1] = offsets[index] +% degree;
    const cursor = allocator.dupe(u64, offsets[0..n]) catch @panic("allocation failed");
    defer allocator.free(cursor);
    const adjacent = allocator.alloc(u64, 4 * n) catch @panic("allocation failed");
    defer allocator.free(adjacent);
    @memset(adjacent, 0);
    for (edges) |edge| {
        adjacent[cursor[edge.source]] = edge.destination;
        cursor[edge.source] +%= 1;
    }
    const distances = allocator.alloc(i64, n) catch @panic("allocation failed");
    defer allocator.free(distances);
    @memset(distances, -1);
    distances[0] = 0;
    const queue = allocator.alloc(u64, n) catch @panic("allocation failed");
    defer allocator.free(queue);
    queue[0] = 0;
    var head: usize = 0;
    var tail: usize = 1;
    while (head < tail) {
        const source = queue[head];
        head += 1;
        var position = offsets[source];
        while (position < offsets[source + 1]) : (position += 1) {
            const destination = adjacent[position];
            if (distances[destination] == -1) {
                distances[destination] = distances[source] +% 1;
                queue[tail] = destination;
                tail += 1;
            }
        }
    }
    var total: u64 = 0;
    for (distances) |distance| total = fold(total, @bitCast(distance));
    return total;
}

fn matrixValue(row: usize, column: usize) f64 {
    @setFloatMode(.strict);
    const sum: u64 = @intCast(row + column);
    return 1.0 / @as(f64, @floatFromInt(sum * (sum + 1) / 2 + row + 1));
}

fn multiply(values: []const f64, transpose: bool) []f64 {
    @setFloatMode(.strict);
    const result = allocator.alloc(f64, values.len) catch @panic("allocation failed");
    for (result, 0..) |*output, row| {
        var total: f64 = 0.0;
        for (values, 0..) |value, column| {
            const coefficient = if (transpose) matrixValue(column, row) else matrixValue(row, column);
            total = total + coefficient * value;
        }
        output.* = total;
    }
    return result;
}

fn multiplyAtA(values: []const f64) []f64 {
    const intermediate = multiply(values, false);
    defer allocator.free(intermediate);
    return multiply(intermediate, true);
}

export fn zig_spectral_norm(size: i64, seed: u64) u64 {
    @setFloatMode(.strict);
    const n = checkSize(size, 5000);
    if (n == 0) return 0;
    var u = allocator.alloc(f64, n) catch @panic("allocation failed");
    defer allocator.free(u);
    @memset(u, 1.0);
    var v = allocator.alloc(f64, n) catch @panic("allocation failed");
    defer allocator.free(v);
    @memset(v, 0.0);
    for (0..10) |_| {
        const next_v = multiplyAtA(u);
        allocator.free(v);
        v = next_v;
        const next_u = multiplyAtA(v);
        allocator.free(u);
        u = next_u;
    }
    var uv: f64 = 0.0;
    var vv: f64 = 0.0;
    for (u, v) |first, second| {
        uv = uv + first * second;
        vv = vv + second * second;
    }
    const scaled = @sqrt(uv / vv) * 1e15;
    std.debug.assert(std.math.isFinite(scaled) and scaled >= 0.0 and scaled < 18446744073709551616.0);
    return @as(u64, @intFromFloat(scaled)) ^ seed;
}
