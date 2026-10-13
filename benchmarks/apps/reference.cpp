#include <algorithm>
#include <charconv>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <functional>
#include <memory>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

namespace {
using U64 = std::uint64_t;

std::size_t check_size(std::int64_t size, std::int64_t maximum) {
    if (size < 0 || size > maximum) std::abort();
    return static_cast<std::size_t>(size);
}

U64 hash(U64 key) {
    U64 value = key * U64{6364136223846793005} + U64{1442695040888963407};
    return value ^ (value >> 29);
}

U64 fold(U64 total, U64 value) { return total * 31 + value; }

std::string decimal(U64 value) {
    char buffer[21];
    auto result = std::to_chars(buffer, buffer + sizeof(buffer), value);
    if (result.ec != std::errc{}) std::abort();
    return std::string(buffer, result.ptr);
}

U64 parse_u64(const std::string &text) {
    U64 value = 0;
    auto result = std::from_chars(text.data(), text.data() + text.size(), value);
    if (result.ec != std::errc{} || result.ptr != text.data() + text.size()) std::abort();
    return value;
}

std::string join(const std::vector<std::string> &parts, char separator) {
    std::size_t size = parts.empty() ? 0 : parts.size() - 1;
    for (const auto &part : parts) size += part.size();
    std::string text;
    text.reserve(size);
    for (std::size_t index = 0; index < parts.size(); ++index) {
        if (index) text += separator;
        text += parts[index];
    }
    return text;
}

std::vector<std::string> split_owned(const std::string &text, char separator) {
    std::vector<std::string> result;
    result.reserve(static_cast<std::size_t>(std::count(text.begin(), text.end(), separator)) + 1);
    std::size_t start = 0;
    for (std::size_t index = 0; index <= text.size(); ++index) {
        if (index == text.size() || text[index] == separator) {
            result.emplace_back(text, start, index - start);
            start = index + 1;
        }
    }
    return result;
}

struct Item { U64 key, id; };
enum class ExprKind { lit, add, mul, neg };
struct Expr {
    ExprKind kind;
    U64 value;
    std::unique_ptr<Expr> left, right;
};

std::unique_ptr<Expr> make_expr(ExprKind kind, U64 value = 0,
        std::unique_ptr<Expr> left = {}, std::unique_ptr<Expr> right = {}) {
    return std::make_unique<Expr>(Expr{kind, value, std::move(left), std::move(right)});
}

std::unique_ptr<Expr> build_expr(unsigned depth, U64 key) {
    U64 value = hash(key);
    if (!depth) return make_expr(ExprKind::lit, value >> 40);
    auto left = build_expr(depth - 1, key * 2 + 1);
    if ((value & 3) == 0) return make_expr(ExprKind::neg, 0, std::move(left));
    auto right = build_expr(depth - 1, key * 2 + 2);
    return make_expr((value & 3) == 1 ? ExprKind::mul : ExprKind::add, 0, std::move(left), std::move(right));
}

std::unique_ptr<Expr> simplify(std::unique_ptr<Expr> expression) {
    if (expression->kind == ExprKind::lit) return make_expr(ExprKind::lit, expression->value);
    if (expression->kind == ExprKind::neg) {
        auto child = simplify(std::move(expression->left));
        if (child->kind == ExprKind::neg) return std::move(child->left);
        if (child->kind == ExprKind::lit) return make_expr(ExprKind::lit, U64{0} - child->value);
        return make_expr(ExprKind::neg, 0, std::move(child));
    }
    auto left = simplify(std::move(expression->left));
    auto right = simplify(std::move(expression->right));
    if (expression->kind == ExprKind::add && left->kind == ExprKind::lit && right->kind == ExprKind::lit)
        return make_expr(ExprKind::lit, left->value + right->value);
    return make_expr(expression->kind, 0, std::move(left), std::move(right));
}

U64 eval(const Expr &expression) {
    switch (expression.kind) {
        case ExprKind::lit: return expression.value;
        case ExprKind::neg: return U64{0} - eval(*expression.left);
        case ExprKind::add: return eval(*expression.left) + eval(*expression.right);
        case ExprKind::mul: return eval(*expression.left) * eval(*expression.right);
    }
    std::abort();
}

struct Tree { std::unique_ptr<Tree> left, right; };

std::unique_ptr<Tree> build_tree(unsigned depth) {
    std::unique_ptr<Tree> left, right;
    if (depth) {
        left = build_tree(depth - 1);
        right = build_tree(depth - 1);
    }
    return std::make_unique<Tree>(Tree{std::move(left), std::move(right)});
}

U64 count_tree(const Tree &tree) {
    if (!tree.left) return 1;
    return 1 + count_tree(*tree.left) + count_tree(*tree.right);
}

struct Edge { U64 source, destination; };

double matrix_value(std::size_t row, std::size_t column) {
    U64 sum = row + column;
    return 1.0 / static_cast<double>(sum * (sum + 1) / 2 + row + 1);
}

std::vector<double> multiply(const std::vector<double> &values, bool transpose) {
    std::vector<double> result(values.size());
    for (std::size_t row = 0; row < values.size(); ++row) {
        double total = 0.0;
        for (std::size_t column = 0; column < values.size(); ++column) {
            double coefficient = transpose ? matrix_value(column, row) : matrix_value(row, column);
            total = total + coefficient * values[column];
        }
        result[row] = total;
    }
    return result;
}

std::vector<double> multiply_ata(const std::vector<double> &values) {
    auto intermediate = multiply(values, false);
    return multiply(intermediate, true);
}
}

extern "C" U64 cpp_tokenize(std::int64_t size, U64 seed) {
    std::size_t n = check_size(size, 10000000);
    if (!n) return 0;
    constexpr std::string_view words[] = {"alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta"};
    std::vector<std::string> parts;
    parts.reserve(n);
    for (std::size_t index = 0; index < n; ++index) parts.emplace_back(words[hash(seed + index) >> 61]);
    auto text = join(parts, ' ');
    auto tokens = split_owned(text, ' ');
    U64 total = tokens.size();
    for (const auto &token : tokens) total = fold(total, token.size());
    return total;
}

extern "C" U64 cpp_format_join(std::int64_t size, U64 seed) {
    std::size_t n = check_size(size, 10000000);
    if (!n) return 0;
    std::vector<std::string> parts;
    parts.reserve(n);
    for (std::size_t index = 0; index < n; ++index) parts.push_back(decimal(hash(seed + index)));
    auto text = join(parts, ',');
    auto fields = split_owned(text, ',');
    U64 sum = 0;
    for (const auto &field : fields) sum += parse_u64(field);
    return fold(text.size(), sum);
}

extern "C" U64 cpp_csv_aggregate(std::int64_t size, U64 seed) {
    std::size_t n = check_size(size, 10000000);
    if (!n) return 0;
    std::vector<std::string> parts;
    parts.reserve(n);
    for (std::size_t index = 0; index < n; ++index) {
        U64 value = hash(seed + index);
        auto category = decimal(value >> 60);
        auto amount = decimal((value >> 20) & 65535);
        auto prefix = category + ",";
        parts.push_back(prefix + amount);
    }
    auto text = join(parts, '\n');
    auto lines = split_owned(text, '\n');
    std::vector<U64> sums(16, 0), counts(16, 0);
    for (const auto &line : lines) {
        auto fields = split_owned(line, ',');
        if (fields.size() != 2) std::abort();
        U64 category = parse_u64(fields[0]);
        if (category >= 16) std::abort();
        sums[category] += parse_u64(fields[1]);
        counts[category] += 1;
    }
    U64 total = fold(0, lines.size());
    for (std::size_t category = 0; category < 16; ++category) total = fold(fold(total, sums[category]), counts[category]);
    return total;
}

extern "C" U64 cpp_map_count(std::int64_t size, U64 seed) {
    std::size_t n = check_size(size, 10000000);
    std::vector<std::pair<U64, U64>> map;
    std::vector<U64> set;
    U64 state = seed;
    for (std::size_t index = 0; index < n; ++index) {
        state = state * U64{6364136223846793005} + U64{1442695040888963407};
        U64 key = (state >> 33) & 1023;
        auto found = std::lower_bound(map.begin(), map.end(), key, [](const auto &entry, U64 needle) { return entry.first < needle; });
        std::size_t position = static_cast<std::size_t>(found - map.begin());
        if (found != map.end() && found->first == key) found->second += 1;
        else {
            map.emplace_back(key, 1);
            for (std::size_t swap = map.size() - 1; swap > position; --swap) std::swap(map[swap], map[swap - 1]);
        }
        key &= 255;
        auto member = std::lower_bound(set.begin(), set.end(), key);
        position = static_cast<std::size_t>(member - set.begin());
        if (member == set.end() || *member != key) {
            set.push_back(key);
            for (std::size_t swap = set.size() - 1; swap > position; --swap) std::swap(set[swap], set[swap - 1]);
        }
    }
    U64 total = map.size() * U64{1000003} + set.size();
    for (U64 key = 0; key < 1024; ++key) {
        auto found = std::lower_bound(map.begin(), map.end(), key, [](const auto &entry, U64 needle) { return entry.first < needle; });
        total = fold(total, found != map.end() && found->first == key ? found->second : 0);
    }
    return total;
}

extern "C" U64 cpp_record_sort(std::int64_t size, U64 seed) {
    std::size_t n = check_size(size, 10000000);
    std::vector<Item> items(n);
    for (std::size_t index = 0; index < n; ++index) items[index] = {hash(seed + index) >> 44, index};
    auto sorted = items;
    std::vector<Item> scratch(n);
    for (std::size_t width = 1; width < n; width *= 2) {
        for (std::size_t start = 0; start < n; start += 2 * width) {
            std::size_t middle = std::min(start + width, n), end = std::min(start + 2 * width, n);
            std::size_t left = start, right = middle;
            for (std::size_t output = start; output < end; ++output) {
                if (left < middle && (right == end || sorted[left].key <= sorted[right].key)) scratch[output] = sorted[left++];
                else scratch[output] = sorted[right++];
            }
        }
        sorted.swap(scratch);
    }
    U64 total = 0;
    for (const auto &item : sorted) total = fold(total, item.id);
    return total;
}

extern "C" U64 cpp_tree_transform(std::int64_t size, U64 seed) {
    std::size_t n = check_size(size, 100000);
    U64 total = 0;
    for (std::size_t index = 0; index < n; ++index) {
        auto tree = simplify(build_expr(12, seed + index));
        total = fold(total, eval(*tree));
    }
    return total;
}

extern "C" U64 cpp_binary_trees(std::int64_t size, U64 seed) {
    std::size_t n = check_size(size, 1000000);
    U64 total = 0;
    for (std::size_t index = 0; index < n; ++index) {
        auto tree = build_tree(4 + static_cast<unsigned>((seed + index) & 7));
        total = fold(total, count_tree(*tree));
    }
    return total;
}

extern "C" U64 cpp_closure_pipeline(std::int64_t size, U64 seed) {
    std::size_t n = check_size(size, 10000000);
    std::vector<std::function<U64(U64)>> functions;
    functions.reserve(4);
    for (U64 key = 0; key < 4; ++key) functions.emplace_back([seed, key](U64 value) {
        return value * U64{6364136223846793005} + (seed ^ key);
    });
    std::vector<U64> values(n), mapped(n);
    for (std::size_t index = 0; index < n; ++index) values[index] = seed + index;
    for (std::size_t index = 0; index < n; ++index) mapped[index] = functions[values[index] & 3](values[index]);
    U64 total = 0;
    for (U64 value : mapped) if (!(value & 1)) total = fold(total, value);
    return total;
}

extern "C" U64 cpp_graph_bfs(std::int64_t size, U64 seed) {
    std::size_t n = check_size(size, 1000000);
    if (!n) return 0;
    std::vector<Edge> edges(4 * n);
    for (std::size_t index = 0; index < edges.size(); ++index) {
        U64 value = hash(seed + index);
        edges[index] = {(value >> 32) % n, (value & U64{0xffffffff}) % n};
    }
    std::vector<U64> degrees(n, 0), offsets(n + 1, 0);
    for (const auto &edge : edges) degrees[edge.source] += 1;
    for (std::size_t index = 0; index < n; ++index) offsets[index + 1] = offsets[index] + degrees[index];
    std::vector<U64> cursor(offsets.begin(), offsets.begin() + static_cast<std::ptrdiff_t>(n)), adjacent(4 * n, 0);
    for (const auto &edge : edges) adjacent[cursor[edge.source]++] = edge.destination;
    std::vector<std::int64_t> distances(n, -1);
    distances[0] = 0;
    std::vector<U64> queue;
    queue.reserve(n);
    queue.push_back(0);
    std::size_t head = 0;
    while (head < queue.size()) {
        U64 source = queue[head++];
        for (U64 position = offsets[source]; position < offsets[source + 1]; ++position) {
            U64 destination = adjacent[position];
            if (distances[destination] == -1) {
                distances[destination] = distances[source] + 1;
                queue.push_back(destination);
            }
        }
    }
    U64 total = 0;
    for (auto distance : distances) total = fold(total, static_cast<U64>(distance));
    return total;
}

extern "C" U64 cpp_spectral_norm(std::int64_t size, U64 seed) {
    std::size_t n = check_size(size, 5000);
    if (!n) return 0;
    std::vector<double> u(n, 1.0), v(n, 0.0);
    for (unsigned iteration = 0; iteration < 10; ++iteration) {
        v = multiply_ata(u);
        u = multiply_ata(v);
    }
    double uv = 0.0, vv = 0.0;
    for (std::size_t index = 0; index < n; ++index) {
        uv = uv + u[index] * v[index];
        vv = vv + v[index] * v[index];
    }
    double scaled = std::sqrt(uv / vv) * 1e15;
    if (!std::isfinite(scaled) || scaled < 0.0 || scaled >= 0x1p64) std::abort();
    return static_cast<U64>(scaled) ^ seed;
}
