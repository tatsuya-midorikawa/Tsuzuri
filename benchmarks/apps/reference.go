package main

/*
#include <stdint.h>
*/
import "C"

import (
	"math"
	"runtime"
	"sort"
	"strconv"
	"strings"
)

func checkSize(size int64, maximum int64) int {
	if size < 0 || size > maximum {
		panic("app workload size out of range")
	}
	return int(size)
}

func hash(key uint64) uint64 {
	value := key*6364136223846793005 + 1442695040888963407
	return value ^ (value >> 29)
}

func fold(total, value uint64) uint64 { return total*31 + value }

func splitOwned(text, separator string) []string {
	result := strings.Split(text, separator)
	for index := range result {
		result[index] = strings.Clone(result[index])
	}
	return result
}

func parseU64(text string) uint64 {
	value, err := strconv.ParseUint(text, 10, 64)
	if err != nil {
		panic(err)
	}
	return value
}

func tokenize(size int64, seed uint64) uint64 {
	n := checkSize(size, 10000000)
	if n == 0 {
		return 0
	}
	words := [...]string{"alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta"}
	parts := make([]string, n)
	for index := range parts {
		parts[index] = strings.Clone(words[hash(seed+uint64(index))>>61])
	}
	text := strings.Join(parts, " ")
	tokens := splitOwned(text, " ")
	total := uint64(len(tokens))
	for _, token := range tokens {
		total = fold(total, uint64(len(token)))
	}
	return total
}

func formatJoin(size int64, seed uint64) uint64 {
	n := checkSize(size, 10000000)
	if n == 0 {
		return 0
	}
	parts := make([]string, n)
	for index := range parts {
		parts[index] = strconv.FormatUint(hash(seed+uint64(index)), 10)
	}
	text := strings.Join(parts, ",")
	fields := splitOwned(text, ",")
	var sum uint64
	for _, field := range fields {
		sum += parseU64(field)
	}
	return fold(uint64(len(text)), sum)
}

func csvAggregate(size int64, seed uint64) uint64 {
	n := checkSize(size, 10000000)
	if n == 0 {
		return 0
	}
	parts := make([]string, n)
	for index := range parts {
		value := hash(seed + uint64(index))
		category := strconv.FormatUint(value>>60, 10)
		amount := strconv.FormatUint((value>>20)&65535, 10)
		prefix := category + ","
		parts[index] = prefix + amount
	}
	text := strings.Join(parts, "\n")
	lines := splitOwned(text, "\n")
	sums, counts := make([]uint64, 16), make([]uint64, 16)
	for _, line := range lines {
		fields := splitOwned(line, ",")
		if len(fields) != 2 {
			panic("generated CSV field count")
		}
		category := parseU64(fields[0])
		sums[category] += parseU64(fields[1])
		counts[category]++
	}
	total := fold(0, uint64(len(lines)))
	for category := range sums {
		total = fold(fold(total, sums[category]), counts[category])
	}
	return total
}

type entry struct{ key, count uint64 }

func mapCount(size int64, seed uint64) uint64 {
	n := checkSize(size, 10000000)
	var counts []entry
	var keys []uint64
	state := seed
	for index := 0; index < n; index++ {
		state = state*6364136223846793005 + 1442695040888963407
		key := (state >> 33) & 1023
		position := sort.Search(len(counts), func(index int) bool { return counts[index].key >= key })
		if position < len(counts) && counts[position].key == key {
			counts[position].count++
		} else {
			counts = append(counts, entry{key, 1})
			for swap := len(counts) - 1; swap > position; swap-- {
				counts[swap], counts[swap-1] = counts[swap-1], counts[swap]
			}
		}
		key &= 255
		position = sort.Search(len(keys), func(index int) bool { return keys[index] >= key })
		if position == len(keys) || keys[position] != key {
			keys = append(keys, key)
			for swap := len(keys) - 1; swap > position; swap-- {
				keys[swap], keys[swap-1] = keys[swap-1], keys[swap]
			}
		}
	}
	total := uint64(len(counts))*1000003 + uint64(len(keys))
	for key := uint64(0); key < 1024; key++ {
		position := sort.Search(len(counts), func(index int) bool { return counts[index].key >= key })
		var count uint64
		if position < len(counts) && counts[position].key == key {
			count = counts[position].count
		}
		total = fold(total, count)
	}
	return total
}

type item struct{ key, id uint64 }

func recordSort(size int64, seed uint64) uint64 {
	n := checkSize(size, 10000000)
	items := make([]item, n)
	for index := range items {
		items[index] = item{hash(seed+uint64(index)) >> 44, uint64(index)}
	}
	sorted, scratch := make([]item, n), make([]item, n)
	copy(sorted, items)
	for width := 1; width < n; width *= 2 {
		for start := 0; start < n; start += 2 * width {
			middle, end := min(start+width, n), min(start+2*width, n)
			left, right := start, middle
			for output := start; output < end; output++ {
				if left < middle && (right == end || sorted[left].key <= sorted[right].key) {
					scratch[output] = sorted[left]
					left++
				} else {
					scratch[output] = sorted[right]
					right++
				}
			}
		}
		sorted, scratch = scratch, sorted
	}
	var total uint64
	for _, record := range sorted {
		total = fold(total, record.id)
	}
	return total
}

const (
	lit = iota
	add
	mul
	neg
)

type expr struct {
	kind        int
	value       uint64
	left, right *expr
}

func buildExpr(depth int, key uint64) *expr {
	value := hash(key)
	if depth == 0 {
		return &expr{kind: lit, value: value >> 40}
	}
	left := buildExpr(depth-1, key*2+1)
	if value&3 == 0 {
		return &expr{kind: neg, left: left}
	}
	right := buildExpr(depth-1, key*2+2)
	kind := add
	if value&3 == 1 {
		kind = mul
	}
	return &expr{kind: kind, left: left, right: right}
}

func simplify(expression *expr) *expr {
	switch expression.kind {
	case lit:
		return &expr{kind: lit, value: expression.value}
	case neg:
		child := simplify(expression.left)
		switch child.kind {
		case neg:
			return child.left
		case lit:
			return &expr{kind: lit, value: uint64(0) - child.value}
		default:
			return &expr{kind: neg, left: child}
		}
	case add:
		left := simplify(expression.left)
		right := simplify(expression.right)
		if left.kind == lit && right.kind == lit {
			return &expr{kind: lit, value: left.value + right.value}
		}
		return &expr{kind: add, left: left, right: right}
	case mul:
		left := simplify(expression.left)
		right := simplify(expression.right)
		return &expr{kind: mul, left: left, right: right}
	}
	panic("invalid expression")
}

func eval(expression *expr) uint64 {
	switch expression.kind {
	case lit:
		return expression.value
	case neg:
		return uint64(0) - eval(expression.left)
	case add:
		return eval(expression.left) + eval(expression.right)
	case mul:
		return eval(expression.left) * eval(expression.right)
	}
	panic("invalid expression")
}

func treeTransform(size int64, seed uint64) uint64 {
	n := checkSize(size, 100000)
	var total uint64
	for index := 0; index < n; index++ {
		tree := simplify(buildExpr(12, seed+uint64(index)))
		total = fold(total, eval(tree))
	}
	return total
}

type tree struct{ left, right *tree }

func buildTree(depth int) *tree {
	if depth == 0 {
		return &tree{}
	}
	left := buildTree(depth - 1)
	right := buildTree(depth - 1)
	return &tree{left, right}
}

func countTree(node *tree) uint64 {
	if node.left == nil {
		return 1
	}
	return 1 + countTree(node.left) + countTree(node.right)
}

func binaryTrees(size int64, seed uint64) uint64 {
	n := checkSize(size, 1000000)
	var total uint64
	for index := 0; index < n; index++ {
		node := buildTree(4 + int((seed+uint64(index))&7))
		total = fold(total, countTree(node))
	}
	return total
}

func closurePipeline(size int64, seed uint64) uint64 {
	n := checkSize(size, 10000000)
	functions := make([]func(uint64) uint64, 4)
	for key := uint64(0); key < 4; key++ {
		key := key
		functions[key] = func(value uint64) uint64 { return value*6364136223846793005 + (seed ^ key) }
	}
	values, mapped := make([]uint64, n), make([]uint64, n)
	for index := range values {
		values[index] = seed + uint64(index)
	}
	for index, value := range values {
		mapped[index] = functions[value&3](value)
	}
	var total uint64
	for _, value := range mapped {
		if value&1 == 0 {
			total = fold(total, value)
		}
	}
	return total
}

type edge struct{ source, destination uint64 }

func graphBFS(size int64, seed uint64) uint64 {
	n := checkSize(size, 1000000)
	if n == 0 {
		return 0
	}
	edges := make([]edge, 4*n)
	for index := range edges {
		value := hash(seed + uint64(index))
		edges[index] = edge{(value >> 32) % uint64(n), (value & 0xffffffff) % uint64(n)}
	}
	degrees, offsets := make([]uint64, n), make([]uint64, n+1)
	for _, link := range edges {
		degrees[link.source]++
	}
	for index, degree := range degrees {
		offsets[index+1] = offsets[index] + degree
	}
	cursor, adjacent := make([]uint64, n), make([]uint64, 4*n)
	copy(cursor, offsets)
	for _, link := range edges {
		adjacent[cursor[link.source]] = link.destination
		cursor[link.source]++
	}
	distances := make([]int64, n)
	for index := range distances {
		distances[index] = -1
	}
	distances[0] = 0
	queue := make([]uint64, 1, n)
	var head int
	for head < len(queue) {
		source := queue[head]
		head++
		for position := offsets[source]; position < offsets[source+1]; position++ {
			destination := adjacent[position]
			if distances[destination] == -1 {
				distances[destination] = distances[source] + 1
				queue = append(queue, destination)
			}
		}
	}
	var total uint64
	for _, distance := range distances {
		total = fold(total, uint64(distance))
	}
	return total
}

func matrixValue(row, column int) float64 {
	sum := uint64(row + column)
	return 1.0 / float64(sum*(sum+1)/2+uint64(row)+1)
}

func multiply(values []float64, transpose bool) []float64 {
	result := make([]float64, len(values))
	for row := range result {
		var total float64
		for column, value := range values {
			var coefficient float64
			if transpose {
				coefficient = matrixValue(column, row)
			} else {
				coefficient = matrixValue(row, column)
			}
			// The explicit conversion requires rounding before addition, forbidding fusion.
			product := float64(coefficient * value)
			total = total + product
		}
		result[row] = total
	}
	return result
}

func multiplyAtA(values []float64) []float64 {
	intermediate := multiply(values, false)
	return multiply(intermediate, true)
}

func spectralNorm(size int64, seed uint64) uint64 {
	n := checkSize(size, 5000)
	if n == 0 {
		return 0
	}
	u, v := make([]float64, n), make([]float64, n)
	for index := range u {
		u[index] = 1.0
	}
	for iteration := 0; iteration < 10; iteration++ {
		v = multiplyAtA(u)
		u = multiplyAtA(v)
	}
	var uv, vv float64
	for index := range u {
		productUV := float64(u[index] * v[index])
		productVV := float64(v[index] * v[index])
		uv = uv + productUV
		vv = vv + productVV
	}
	scaled := math.Sqrt(uv/vv) * 1e15
	if math.IsNaN(scaled) || math.IsInf(scaled, 0) || scaled < 0 || scaled >= 18446744073709551616.0 {
		panic("spectral result out of range")
	}
	return uint64(scaled) ^ seed
}

//export go_tokenize
func go_tokenize(size C.int64_t, seed C.uint64_t) C.uint64_t {
	return C.uint64_t(tokenize(int64(size), uint64(seed)))
}

//export go_format_join
func go_format_join(size C.int64_t, seed C.uint64_t) C.uint64_t {
	return C.uint64_t(formatJoin(int64(size), uint64(seed)))
}

//export go_csv_aggregate
func go_csv_aggregate(size C.int64_t, seed C.uint64_t) C.uint64_t {
	return C.uint64_t(csvAggregate(int64(size), uint64(seed)))
}

//export go_map_count
func go_map_count(size C.int64_t, seed C.uint64_t) C.uint64_t {
	return C.uint64_t(mapCount(int64(size), uint64(seed)))
}

//export go_record_sort
func go_record_sort(size C.int64_t, seed C.uint64_t) C.uint64_t {
	return C.uint64_t(recordSort(int64(size), uint64(seed)))
}

//export go_tree_transform
func go_tree_transform(size C.int64_t, seed C.uint64_t) C.uint64_t {
	return C.uint64_t(treeTransform(int64(size), uint64(seed)))
}

//export go_binary_trees
func go_binary_trees(size C.int64_t, seed C.uint64_t) C.uint64_t {
	return C.uint64_t(binaryTrees(int64(size), uint64(seed)))
}

//export go_closure_pipeline
func go_closure_pipeline(size C.int64_t, seed C.uint64_t) C.uint64_t {
	return C.uint64_t(closurePipeline(int64(size), uint64(seed)))
}

//export go_graph_bfs
func go_graph_bfs(size C.int64_t, seed C.uint64_t) C.uint64_t {
	return C.uint64_t(graphBFS(int64(size), uint64(seed)))
}

//export go_spectral_norm
func go_spectral_norm(size C.int64_t, seed C.uint64_t) C.uint64_t {
	return C.uint64_t(spectralNorm(int64(size), uint64(seed)))
}

//export go_runtime_gomaxprocs
func go_runtime_gomaxprocs() C.int { return C.int(runtime.GOMAXPROCS(0)) }

//export go_runtime_numcpu
func go_runtime_numcpu() C.int { return C.int(runtime.NumCPU()) }

func main() {}
