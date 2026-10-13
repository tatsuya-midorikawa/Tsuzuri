#include <errno.h>
#include <inttypes.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static size_t check_size(int64_t size, int64_t maximum) {
    if (size < 0 || size > maximum) abort();
    return (size_t)size;
}

static void *allocate(size_t count, size_t width) {
    if (width && count > SIZE_MAX / width) abort();
    void *result = malloc(count ? count * width : 1);
    if (!result) abort();
    return result;
}

static void *resize(void *old, size_t count, size_t width) {
    if (width && count > SIZE_MAX / width) abort();
    void *result = realloc(old, count ? count * width : 1);
    if (!result) abort();
    return result;
}

static uint64_t hash(uint64_t key) {
    uint64_t value = key * UINT64_C(6364136223846793005) + UINT64_C(1442695040888963407);
    return value ^ (value >> 29);
}

static uint64_t fold(uint64_t total, uint64_t value) {
    return total * 31 + value;
}

static char *copy_text(const char *text, size_t length) {
    char *result = allocate(length + 1, 1);
    memcpy(result, text, length);
    result[length] = '\0';
    return result;
}

struct Strings { char **items; size_t count; };

static void free_strings(struct Strings strings) {
    for (size_t index = 0; index < strings.count; ++index) free(strings.items[index]);
    free(strings.items);
}

static char *join_strings(struct Strings strings, char separator) {
    size_t length = strings.count ? strings.count - 1 : 0;
    for (size_t index = 0; index < strings.count; ++index) length += strlen(strings.items[index]);
    char *text = allocate(length + 1, 1);
    size_t offset = 0;
    for (size_t index = 0; index < strings.count; ++index) {
        if (index) text[offset++] = separator;
        size_t width = strlen(strings.items[index]);
        memcpy(text + offset, strings.items[index], width);
        offset += width;
    }
    text[offset] = '\0';
    return text;
}

static struct Strings split_owned(const char *text, char separator) {
    size_t count = 1;
    for (const char *cursor = text; *cursor; ++cursor) if (*cursor == separator) ++count;
    struct Strings result = {allocate(count, sizeof(char *)), count};
    const char *start = text;
    size_t index = 0;
    for (const char *cursor = text;; ++cursor) {
        if (*cursor == separator || !*cursor) {
            result.items[index++] = copy_text(start, (size_t)(cursor - start));
            start = cursor + 1;
            if (!*cursor) break;
        }
    }
    return result;
}

static char *decimal(uint64_t value) {
    char buffer[21];
    int length = snprintf(buffer, sizeof(buffer), "%" PRIu64, value);
    if (length < 1 || (size_t)length >= sizeof(buffer)) abort();
    return copy_text(buffer, (size_t)length);
}

static uint64_t parse_u64(const char *text) {
    char *end;
    errno = 0;
    unsigned long long value = strtoull(text, &end, 10);
    if (errno || end == text || *end) abort();
    return (uint64_t)value;
}

uint64_t c_tokenize(int64_t size, uint64_t seed) {
    size_t n = check_size(size, 10000000);
    if (!n) return 0;
    const char *words[] = {"alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta"};
    struct Strings parts = {allocate(n, sizeof(char *)), n};
    for (size_t index = 0; index < n; ++index) {
        const char *word = words[hash(seed + index) >> 61];
        parts.items[index] = copy_text(word, strlen(word));
    }
    char *text = join_strings(parts, ' ');
    struct Strings tokens = split_owned(text, ' ');
    uint64_t total = tokens.count;
    for (size_t index = 0; index < tokens.count; ++index) total = fold(total, strlen(tokens.items[index]));
    free_strings(tokens);
    free(text);
    free_strings(parts);
    return total;
}

uint64_t c_format_join(int64_t size, uint64_t seed) {
    size_t n = check_size(size, 10000000);
    if (!n) return 0;
    struct Strings parts = {allocate(n, sizeof(char *)), n};
    for (size_t index = 0; index < n; ++index) parts.items[index] = decimal(hash(seed + index));
    char *text = join_strings(parts, ',');
    struct Strings fields = split_owned(text, ',');
    uint64_t sum = 0;
    for (size_t index = 0; index < fields.count; ++index) sum += parse_u64(fields.items[index]);
    uint64_t total = fold(strlen(text), sum);
    free_strings(fields);
    free(text);
    free_strings(parts);
    return total;
}

uint64_t c_csv_aggregate(int64_t size, uint64_t seed) {
    size_t n = check_size(size, 10000000);
    if (!n) return 0;
    struct Strings parts = {allocate(n, sizeof(char *)), n};
    for (size_t index = 0; index < n; ++index) {
        uint64_t value = hash(seed + index);
        char *category = decimal(value >> 60);
        char *amount = decimal((value >> 20) & 65535);
        char *prefix = allocate(strlen(category) + 2, 1);
        strcpy(prefix, category);
        strcat(prefix, ",");
        parts.items[index] = allocate(strlen(prefix) + strlen(amount) + 1, 1);
        strcpy(parts.items[index], prefix);
        strcat(parts.items[index], amount);
        free(prefix);
        free(amount);
        free(category);
    }
    char *text = join_strings(parts, '\n');
    struct Strings lines = split_owned(text, '\n');
    uint64_t *sums = allocate(16, sizeof(uint64_t)), *counts = allocate(16, sizeof(uint64_t));
    memset(sums, 0, 16 * sizeof(uint64_t));
    memset(counts, 0, 16 * sizeof(uint64_t));
    for (size_t index = 0; index < lines.count; ++index) {
        struct Strings fields = split_owned(lines.items[index], ',');
        if (fields.count != 2) abort();
        uint64_t category = parse_u64(fields.items[0]);
        if (category >= 16) abort();
        sums[category] += parse_u64(fields.items[1]);
        counts[category] += 1;
        free_strings(fields);
    }
    uint64_t total = fold(0, lines.count);
    for (size_t category = 0; category < 16; ++category) total = fold(fold(total, sums[category]), counts[category]);
    free(counts);
    free(sums);
    free_strings(lines);
    free(text);
    free_strings(parts);
    return total;
}

struct Entry { uint64_t key, count; };

static size_t map_lower_bound(const struct Entry *entries, size_t count, uint64_t key) {
    size_t lower = 0, upper = count;
    while (lower < upper) {
        size_t middle = lower + (upper - lower) / 2;
        if (entries[middle].key < key) lower = middle + 1;
        else upper = middle;
    }
    return lower;
}

static size_t set_lower_bound(const uint64_t *entries, size_t count, uint64_t key) {
    size_t lower = 0, upper = count;
    while (lower < upper) {
        size_t middle = lower + (upper - lower) / 2;
        if (entries[middle] < key) lower = middle + 1;
        else upper = middle;
    }
    return lower;
}

uint64_t c_map_count(int64_t size, uint64_t seed) {
    size_t n = check_size(size, 10000000);
    struct Entry *map = NULL;
    uint64_t *set = NULL;
    size_t map_count = 0, map_capacity = 0, set_count = 0, set_capacity = 0;
    uint64_t state = seed;
    for (size_t index = 0; index < n; ++index) {
        state = state * UINT64_C(6364136223846793005) + UINT64_C(1442695040888963407);
        uint64_t key = (state >> 33) & 1023;
        size_t position = map_lower_bound(map, map_count, key);
        if (position < map_count && map[position].key == key) map[position].count += 1;
        else {
            if (map_count == map_capacity) {
                map_capacity = map_capacity ? map_capacity * 2 : 4;
                map = resize(map, map_capacity, sizeof(struct Entry));
            }
            map[map_count++] = (struct Entry){key, 1};
            for (size_t swap = map_count - 1; swap > position; --swap) {
                struct Entry value = map[swap];
                map[swap] = map[swap - 1];
                map[swap - 1] = value;
            }
        }
        key &= 255;
        position = set_lower_bound(set, set_count, key);
        if (position == set_count || set[position] != key) {
            if (set_count == set_capacity) {
                set_capacity = set_capacity ? set_capacity * 2 : 4;
                set = resize(set, set_capacity, sizeof(uint64_t));
            }
            set[set_count++] = key;
            for (size_t swap = set_count - 1; swap > position; --swap) {
                uint64_t value = set[swap];
                set[swap] = set[swap - 1];
                set[swap - 1] = value;
            }
        }
    }
    uint64_t total = map_count * UINT64_C(1000003) + set_count;
    for (uint64_t key = 0; key < 1024; ++key) {
        size_t position = map_lower_bound(map, map_count, key);
        total = fold(total, position < map_count && map[position].key == key ? map[position].count : 0);
    }
    free(set);
    free(map);
    return total;
}

struct Item { uint64_t key, id; };

uint64_t c_record_sort(int64_t size, uint64_t seed) {
    size_t n = check_size(size, 10000000);
    if (!n) return 0;
    struct Item *items = allocate(n, sizeof(struct Item));
    for (size_t index = 0; index < n; ++index) items[index] = (struct Item){hash(seed + index) >> 44, index};
    struct Item *sorted = allocate(n, sizeof(struct Item)), *scratch = allocate(n, sizeof(struct Item));
    memcpy(sorted, items, n * sizeof(struct Item));
    for (size_t width = 1; width < n; width *= 2) {
        for (size_t start = 0; start < n; start += 2 * width) {
            size_t middle = start + width < n ? start + width : n;
            size_t end = start + 2 * width < n ? start + 2 * width : n;
            size_t left = start, right = middle;
            for (size_t output = start; output < end; ++output) {
                if (left < middle && (right == end || sorted[left].key <= sorted[right].key)) scratch[output] = sorted[left++];
                else scratch[output] = sorted[right++];
            }
        }
        struct Item *swap = sorted;
        sorted = scratch;
        scratch = swap;
    }
    uint64_t total = 0;
    for (size_t index = 0; index < n; ++index) total = fold(total, sorted[index].id);
    free(scratch);
    free(sorted);
    free(items);
    return total;
}

enum ExprKind { LIT, ADD, MUL, NEG };
struct Expr { enum ExprKind kind; uint64_t value; struct Expr *left, *right; };

static struct Expr *make_expr(enum ExprKind kind, uint64_t value, struct Expr *left, struct Expr *right) {
    struct Expr *result = allocate(1, sizeof(struct Expr));
    *result = (struct Expr){kind, value, left, right};
    return result;
}

static struct Expr *build_expr(unsigned depth, uint64_t key) {
    uint64_t value = hash(key);
    if (!depth) return make_expr(LIT, value >> 40, NULL, NULL);
    struct Expr *left = build_expr(depth - 1, key * 2 + 1);
    if ((value & 3) == 0) return make_expr(NEG, 0, left, NULL);
    struct Expr *right = build_expr(depth - 1, key * 2 + 2);
    return make_expr((value & 3) == 1 ? MUL : ADD, 0, left, right);
}

static struct Expr *simplify(struct Expr *expression) {
    struct Expr *result;
    if (expression->kind == LIT) result = make_expr(LIT, expression->value, NULL, NULL);
    else if (expression->kind == NEG) {
        struct Expr *child = simplify(expression->left);
        if (child->kind == NEG) {
            result = child->left;
            free(child);
        } else if (child->kind == LIT) {
            result = make_expr(LIT, UINT64_C(0) - child->value, NULL, NULL);
            free(child);
        } else result = make_expr(NEG, 0, child, NULL);
    } else {
        struct Expr *left = simplify(expression->left);
        struct Expr *right = simplify(expression->right);
        if (expression->kind == ADD && left->kind == LIT && right->kind == LIT) {
            result = make_expr(LIT, left->value + right->value, NULL, NULL);
            free(left);
            free(right);
        } else result = make_expr(expression->kind, 0, left, right);
    }
    free(expression);
    return result;
}

static uint64_t eval(const struct Expr *expression) {
    switch (expression->kind) {
        case LIT: return expression->value;
        case NEG: return UINT64_C(0) - eval(expression->left);
        case ADD: return eval(expression->left) + eval(expression->right);
        case MUL: return eval(expression->left) * eval(expression->right);
    }
    abort();
}

static void free_expr(struct Expr *expression) {
    if (expression->left) free_expr(expression->left);
    if (expression->right) free_expr(expression->right);
    free(expression);
}

uint64_t c_tree_transform(int64_t size, uint64_t seed) {
    size_t n = check_size(size, 100000);
    uint64_t total = 0;
    for (size_t index = 0; index < n; ++index) {
        struct Expr *tree = simplify(build_expr(12, seed + index));
        total = fold(total, eval(tree));
        free_expr(tree);
    }
    return total;
}

struct Tree { struct Tree *left, *right; };

static struct Tree *build_tree(unsigned depth) {
    struct Tree *left = NULL, *right = NULL;
    if (depth) {
        left = build_tree(depth - 1);
        right = build_tree(depth - 1);
    }
    struct Tree *result = allocate(1, sizeof(struct Tree));
    *result = (struct Tree){left, right};
    return result;
}

static uint64_t count_tree(const struct Tree *tree) {
    if (!tree->left) return 1;
    return 1 + count_tree(tree->left) + count_tree(tree->right);
}

static void free_tree(struct Tree *tree) {
    if (tree->left) {
        free_tree(tree->left);
        free_tree(tree->right);
    }
    free(tree);
}

uint64_t c_binary_trees(int64_t size, uint64_t seed) {
    size_t n = check_size(size, 1000000);
    uint64_t total = 0;
    for (size_t index = 0; index < n; ++index) {
        struct Tree *tree = build_tree(4 + (unsigned)((seed + index) & 7));
        total = fold(total, count_tree(tree));
        free_tree(tree);
    }
    return total;
}

struct Closure { uint64_t seed, key; uint64_t (*call)(const struct Closure *, uint64_t); };

static uint64_t apply_closure(const struct Closure *closure, uint64_t value) {
    return value * UINT64_C(6364136223846793005) + (closure->seed ^ closure->key);
}

uint64_t c_closure_pipeline(int64_t size, uint64_t seed) {
    size_t n = check_size(size, 10000000);
    struct Closure *functions = allocate(4, sizeof(struct Closure));
    for (uint64_t key = 0; key < 4; ++key) functions[key] = (struct Closure){seed, key, apply_closure};
    uint64_t *values = allocate(n, sizeof(uint64_t)), *mapped = allocate(n, sizeof(uint64_t));
    for (size_t index = 0; index < n; ++index) values[index] = seed + index;
    for (size_t index = 0; index < n; ++index) {
        const struct Closure *closure = &functions[values[index] & 3];
        mapped[index] = closure->call(closure, values[index]);
    }
    uint64_t total = 0;
    for (size_t index = 0; index < n; ++index) if (!(mapped[index] & 1)) total = fold(total, mapped[index]);
    free(mapped);
    free(values);
    free(functions);
    return total;
}

struct Edge { uint64_t source, destination; };

uint64_t c_graph_bfs(int64_t size, uint64_t seed) {
    size_t n = check_size(size, 1000000);
    if (!n) return 0;
    struct Edge *edges = allocate(4 * n, sizeof(struct Edge));
    for (size_t index = 0; index < 4 * n; ++index) {
        uint64_t value = hash(seed + index);
        edges[index] = (struct Edge){(value >> 32) % n, (value & UINT64_C(0xffffffff)) % n};
    }
    uint64_t *degrees = allocate(n, sizeof(uint64_t)), *offsets = allocate(n + 1, sizeof(uint64_t));
    memset(degrees, 0, n * sizeof(uint64_t));
    for (size_t index = 0; index < 4 * n; ++index) degrees[edges[index].source] += 1;
    offsets[0] = 0;
    for (size_t index = 0; index < n; ++index) offsets[index + 1] = offsets[index] + degrees[index];
    uint64_t *cursor = allocate(n, sizeof(uint64_t)), *adjacent = allocate(4 * n, sizeof(uint64_t));
    memcpy(cursor, offsets, n * sizeof(uint64_t));
    for (size_t index = 0; index < 4 * n; ++index) adjacent[cursor[edges[index].source]++] = edges[index].destination;
    int64_t *distances = allocate(n, sizeof(int64_t));
    for (size_t index = 0; index < n; ++index) distances[index] = -1;
    distances[0] = 0;
    uint64_t *queue = allocate(n, sizeof(uint64_t));
    queue[0] = 0;
    size_t head = 0, tail = 1;
    while (head < tail) {
        uint64_t source = queue[head++];
        for (uint64_t position = offsets[source]; position < offsets[source + 1]; ++position) {
            uint64_t destination = adjacent[position];
            if (distances[destination] == -1) {
                distances[destination] = distances[source] + 1;
                queue[tail++] = destination;
            }
        }
    }
    uint64_t total = 0;
    for (size_t index = 0; index < n; ++index) total = fold(total, (uint64_t)distances[index]);
    free(queue);
    free(distances);
    free(adjacent);
    free(cursor);
    free(offsets);
    free(degrees);
    free(edges);
    return total;
}

static double matrix_value(size_t row, size_t column) {
    uint64_t sum = row + column;
    return 1.0 / (double)(sum * (sum + 1) / 2 + row + 1);
}

static double *multiply(const double *values, size_t n, int transpose) {
    double *result = allocate(n, sizeof(double));
    for (size_t row = 0; row < n; ++row) {
        double total = 0.0;
        for (size_t column = 0; column < n; ++column) {
            double coefficient = transpose ? matrix_value(column, row) : matrix_value(row, column);
            total = total + coefficient * values[column];
        }
        result[row] = total;
    }
    return result;
}

static double *multiply_ata(const double *values, size_t n) {
    double *intermediate = multiply(values, n, 0);
    double *result = multiply(intermediate, n, 1);
    free(intermediate);
    return result;
}

uint64_t c_spectral_norm(int64_t size, uint64_t seed) {
    size_t n = check_size(size, 5000);
    if (!n) return 0;
    double *u = allocate(n, sizeof(double)), *v = allocate(n, sizeof(double));
    for (size_t index = 0; index < n; ++index) {
        u[index] = 1.0;
        v[index] = 0.0;
    }
    for (unsigned iteration = 0; iteration < 10; ++iteration) {
        double *next = multiply_ata(u, n);
        free(v);
        v = next;
        next = multiply_ata(v, n);
        free(u);
        u = next;
    }
    double uv = 0.0, vv = 0.0;
    for (size_t index = 0; index < n; ++index) {
        uv = uv + u[index] * v[index];
        vv = vv + v[index] * v[index];
    }
    double scaled = sqrt(uv / vv) * 1e15;
    if (!isfinite(scaled) || scaled < 0.0 || scaled >= 0x1p64) abort();
    uint64_t total = (uint64_t)scaled ^ seed;
    free(v);
    free(u);
    return total;
}
