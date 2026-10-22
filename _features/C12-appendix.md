# C12 付録: 検証済みの `std/Map.tz`・`std/Set.tz`・ベンチマーク・実測

[C12 本体](C12-ordered-map-set-chunked.md)の付録。ここのコードは、HEAD のリポジトリの複製に置いて `cargo build --release` したコンパイラで、
`map_set`（26 件）・`map_set_chunks`（29 件）・`json`（165 件）・`iteration_protocol`（26 件）の E2E suite（native と WASM の `-O0`／`-O3`）、
`tsuzuri test tests/fixtures/map_set_chunks`（`-O0`／`-O3` とも 5 件通過）、`tests/user_drop.mjs` を通した**検証済みの全文**である。
実装者は、**そのまま写し、設計と食い違うと思っても書き換えない**（食い違いは本体「停止条件」に従って報告する）。

- 型検査を通らない書き換えをしないこと。特に「借用を持つキー・値」を `ref` のローカル変数経由で読む書き方（本体「落とし穴」1）は、`E1014` になる。
- `std/Map.tz`・`std/Set.tz` のインデントは **TAB 1 文字**（HEAD の `std/Map.tz` と同じ）。空白に直さない。`tsuzuri fmt` を std に掛けない（HEAD の std も `fmt --check` は不一致）。
- 本文の英語のコメントは、そのまま残す（std のコメントは英語）。

## A. `std/Map.tz`（全文。379 行・16,193 バイト）

<!-- file: std/Map.tz -->
```text
private record Entry<'key, 'value> { key: 'key, value: 'value }

/// The entries in ascending key order, kept in chunks of 1 to 512 entries so that an update moves at most one chunk.
/// `head` is the first chunk and `rest` holds the others in order; `length` is the number of entries. A map without entries has an empty
/// `head` and an empty `rest`, and any two neighbouring chunks hold more than 256 entries together.
record Map<'key, 'value> { head: Vec<Entry<'key, 'value>>, rest: Vec<Vec<Entry<'key, 'value>>>, length: i64 }

/// The most entries of a chunk; a full chunk splits in two when an entry goes into it.
private const MaxChunk: i64 = 512

/// Two neighbouring chunks that hold this many entries or fewer together are merged into one.
private const MergeLimit: i64 = 256

/// Takes the item at `index` out of `items` in constant time, which moves the last item to `index`; `put` with the same `index` undoes that.
private def take :: Vec<'a> -> i64 -> (Vec<'a> * 'a)
fn take items index =
	let last = Vec.length (ref items) - 1
	match Vec.pop (Vec.swap index last items) with
	| (rest, Maybe.Some item) -> (rest, item)
	| (_, Maybe.None) -> unreachable ()

private def put :: Vec<'a> -> i64 -> 'a -> Vec<'a>
fn put items index item =
	let pushed = Vec.push item items
	let last = Vec.length (ref pushed) - 1
	Vec.swap index last pushed

private def insert_at :: Vec<'a> -> i64 -> 'a -> Vec<'a>
fn insert_at items index item =
	let mut result = Vec.push item items
	let mut position = Vec.length (ref result) - 1
	while position > index do
		result = Vec.swap position (position - 1) result
		position = position - 1
	result

private def remove_at :: Vec<'a> -> i64 -> (Vec<'a> * 'a)
fn remove_at items index =
	let mut result = items
	let last = Vec.length (ref result) - 1
	let mut position = index
	while position < last do
		result = Vec.swap position (position + 1) result
		position = position + 1
	match Vec.pop result with
	| (rest, Maybe.Some item) -> (rest, item)
	| (_, Maybe.None) -> unreachable ()

private def reverse :: Vec<'a> -> Vec<'a>
fn reverse items =
	let mut result = items
	let count = Vec.length (ref result)
	let mut index = 0
	while index < count / 2 do
		result = Vec.swap index (count - 1 - index) result
		index = index + 1
	result

/// Moves the items from `at` on into a new `Vec`, in order.
private def split_off :: Vec<'a> -> i64 -> (Vec<'a> * Vec<'a>)
fn split_off items at =
	let mut head = items
	let mut tail = Vec.with_capacity (Vec.length (ref head) - at)
	while Vec.length (ref head) > at do
		match Vec.pop head with
		| (rest, Maybe.Some item) ->
			head = rest
			tail = Vec.push item tail
		| (rest, Maybe.None) -> head = rest
	(head, reverse tail)

/// Appends the items of `tail` to `head`, in order.
private def join :: Vec<'a> -> Vec<'a> -> Vec<'a>
fn join head tail =
	let mut result = head
	let mut rest = reverse tail
	while Vec.length (ref rest) > 0 do
		match Vec.pop rest with
		| (remaining, Maybe.Some item) ->
			rest = remaining
			result = Vec.push item result
		| (remaining, Maybe.None) -> rest = remaining
	result

def empty :: Map<'key, 'value>
fn empty = Map { head: Vec.empty(), rest: Vec.empty(), length: 0 }

def singleton :: 'key -> 'value -> Map<'key, 'value>
fn singleton key value = Map { head: Vec.push (Entry { key: key, value: value }) (Vec.empty()), rest: Vec.empty(), length: 1 }

def length :: ref Map<'key, 'value> -> i64
fn length map = map.length

def is_empty :: ref Map<'key, 'value> -> bool
fn is_empty map = map.length == 0

private def chunk_total :: ref Map<'key, 'value> -> i64
fn chunk_total map = if map.length == 0 then 0 else 1 + Vec.length (ref map.rest)

/// The number of chunks that hold the entries (0 for an empty map). A diagnostic for tests, like `HashMap.longest_probe`.
def chunk_count :: ref Map<'key, 'value> -> i64
fn chunk_count map = chunk_total map

/// Chunk `index` in key order: `head` for 0, `rest[index - 1]` after it.
private def chunk_of :: ref Map<'key, 'value> -> i64 -> ref Vec<Entry<'key, 'value>>
fn chunk_of map index = if index == 0 then ref map.head else ref map.rest[index - 1]

/// Where `key` is: `(chunk, offset)` when an entry has it, and `(chunk, -1 - offset)` when none has, where `offset` is the place in `chunk`
/// where an entry for it would go. The chunk is the first one whose last key is not less than `key`, or the last chunk. An empty map gives `(0, -1)`.
private def locate :: Ord<'key> => ref Map<'key, 'value> -> ref 'key -> (i64 * i64)
fn locate map key =
	assert (Eq.eq key key)
	let total = chunk_total map
	if total == 0 then (0, -1)
	else
		let mut low = 0
		let mut high = total - 1
		while low < high do
			let middle = low + (high - low) / 2
			let chunk = chunk_of map middle
			let last = Vec.length chunk - 1
			assert (Eq.eq (ref chunk[last].key) (ref chunk[last].key))
			if Ord.lt (ref chunk[last].key) key then low = middle + 1 else high = middle
		let entries = chunk_of map low
		let size = Vec.length entries
		let mut start = 0
		let mut stop = size
		while start < stop do
			let middle = start + (stop - start) / 2
			assert (Eq.eq (ref entries[middle].key) (ref entries[middle].key))
			if Ord.lt (ref entries[middle].key) key then start = middle + 1 else stop = middle
		if start < size && !(Ord.lt key (ref entries[start].key)) then (low, start) else (low, -1 - start)

def contains_key :: Ord<'key> => ref Map<'key, 'value> -> 'key -> bool
fn contains_key map key =
	match locate map (ref key) with
	| (_chunk, offset) -> offset >= 0

def get :: (Ord<'key>, Copy<'value>) => ref Map<'key, 'value> -> 'key -> Maybe<'value>
fn get map key =
	match locate map (ref key) with
	| (chunk, offset) ->
		if offset >= 0 then
			let entries = chunk_of map chunk
			Maybe.Some entries[offset].value
		else Maybe.None

def at :: Ord<'key> => ref Map<'key, 'value> -> 'key -> ref 'value
fn at map key =
	match locate map (ref key) with
	| (chunk, offset) ->
		assert (offset >= 0)
		let entries = chunk_of map chunk
		ref entries[offset].value

/// Takes chunk `index` out of the map. The map that comes back has a hole at `index`; `put_chunk` fills it.
private def take_chunk :: Map<'key, 'value> -> i64 -> (Map<'key, 'value> * Vec<Entry<'key, 'value>>)
fn take_chunk map index =
	if index == 0 then (Map { head: Vec.empty(), rest: map.rest, length: map.length }, map.head)
	else
		match take map.rest (index - 1) with
		| (others, chunk) -> (Map { head: map.head, rest: others, length: map.length }, chunk)

private def put_chunk :: Map<'key, 'value> -> i64 -> Vec<Entry<'key, 'value>> -> i64 -> Map<'key, 'value>
fn put_chunk map index chunk count =
	if index == 0 then Map { head: chunk, rest: map.rest, length: count }
	else Map { head: map.head, rest: put map.rest (index - 1) chunk, length: count }

/// The hot path of a counter update: it keeps the old key and takes the chunk out of the map only when the entry is not in `head`.
private def replace_value :: Map<'key, 'value> -> i64 -> i64 -> 'value -> Map<'key, 'value>
fn replace_value map chunk offset value =
	let count = map.length
	if chunk == 0 then
		match take map.head offset with
		| (remaining, old) -> Map { head: put remaining offset (Entry { key: old.key, value: value }), rest: map.rest, length: count }
	else
		match take map.rest (chunk - 1) with
		| (others, entries) ->
			match take entries offset with
			| (remaining, old) -> Map { head: map.head, rest: put others (chunk - 1) (put remaining offset (Entry { key: old.key, value: value })), length: count }

/// Puts `entry` at `offset` of chunk `chunk`. A full chunk splits in two halves first, except at the end of the last chunk, which starts a new chunk.
private def add_entry :: Map<'key, 'value> -> i64 -> i64 -> Entry<'key, 'value> -> Map<'key, 'value>
fn add_entry map chunk offset entry =
	let count = map.length
	if count == 0 then Map { head: Vec.push entry (Vec.empty()), rest: Vec.empty(), length: 1 }
	else
		let total = chunk_total (ref map)
		let size = Vec.length (chunk_of (ref map) chunk)
		if size < MaxChunk then
			match take_chunk map chunk with
			| (hole, entries) -> put_chunk hole chunk (insert_at entries offset entry) (count + 1)
		elif chunk == total - 1 && offset == size then Map { head: map.head, rest: Vec.push (Vec.push entry (Vec.empty())) map.rest, length: count + 1 }
		else
			match take_chunk map chunk with
			| (hole, entries) ->
				let middle = size / 2
				match split_off entries middle with
				| (left, right) ->
					if offset <= middle then
						let restored = put_chunk hole chunk (insert_at left offset entry) (count + 1)
						Map { head: restored.head, rest: insert_at restored.rest chunk right, length: count + 1 }
					else
						let restored = put_chunk hole chunk left (count + 1)
						Map { head: restored.head, rest: insert_at restored.rest chunk (insert_at right (offset - middle) entry), length: count + 1 }

/// Whether `key` is greater than every key of the map, which makes an insertion an append.
private def beyond_last :: Ord<'key> => ref Map<'key, 'value> -> ref 'key -> bool
fn beyond_last map key =
	assert (Eq.eq key key)
	if map.length == 0 then false
	else
		let entries = chunk_of map (chunk_total map - 1)
		let last = Vec.length entries - 1
		assert (Eq.eq (ref entries[last].key) (ref entries[last].key))
		Ord.lt (ref entries[last].key) key

def insert :: Ord<'key> => Map<'key, 'value> -> 'key -> 'value -> Map<'key, 'value>
fn insert map key value =
	if beyond_last (ref map) (ref key) then
		let chunk = chunk_total (ref map) - 1
		let size = Vec.length (chunk_of (ref map) chunk)
		add_entry map chunk size (Entry { key: key, value: value })
	else
		match locate (ref map) (ref key) with
		| (chunk, offset) ->
			if offset >= 0 then replace_value map chunk offset value
			else add_entry map chunk (-1 - offset) (Entry { key: key, value: value })

/// Merges chunk `index + 1` into chunk `index`.
private def merge_next :: Map<'key, 'value> -> i64 -> Map<'key, 'value>
fn merge_next map index =
	let count = map.length
	match remove_at map.rest index with
	| (others, tail) ->
		match take_chunk (Map { head: map.head, rest: others, length: count }) index with
		| (hole, entries) -> put_chunk hole index (join entries tail) count

/// Restores the rule for neighbouring chunks after chunk `index` lost an entry: it merges with the chunk before it or, failing that, after it.
private def shrink :: Map<'key, 'value> -> i64 -> Map<'key, 'value>
fn shrink map index =
	let size = Vec.length (chunk_of (ref map) index)
	if index > 0 && Vec.length (chunk_of (ref map) (index - 1)) + size <= MergeLimit then merge_next map (index - 1)
	elif index + 1 < chunk_total (ref map) && size + Vec.length (chunk_of (ref map) (index + 1)) <= MergeLimit then merge_next map index
	else map

/// Removes chunk `index`, which holds one entry; the next chunk becomes `head` when that is chunk 0.
private def delete_chunk :: Map<'key, 'value> -> i64 -> Map<'key, 'value>
fn delete_chunk map index =
	let count = map.length - 1
	if index == 0 then
		if Vec.length (ref map.rest) == 0 then Map { head: Vec.empty(), rest: Vec.empty(), length: 0 }
		else
			match remove_at map.rest 0 with
			| (others, first) -> Map { head: first, rest: others, length: count }
	else
		match remove_at map.rest (index - 1) with
		| (others, _gone) -> Map { head: map.head, rest: others, length: count }

def remove :: Ord<'key> => Map<'key, 'value> -> 'key -> Map<'key, 'value>
fn remove map key =
	match locate (ref map) (ref key) with
	| (chunk, offset) ->
		if offset >= 0 then
			let count = map.length
			if Vec.length (chunk_of (ref map) chunk) == 1 then delete_chunk map chunk
			else
				match take_chunk map chunk with
				| (hole, entries) ->
					match remove_at entries offset with
					| (remaining, _removed) -> shrink (put_chunk hole chunk remaining (count - 1)) chunk
		else map

/// The `append_*` functions read the entries through their own parameter, not through a local reference: a key or a value that holds a borrow
/// would otherwise keep the loan of that reference alive in the `Vec` that is built, and moving that `Vec` would be rejected (`E1014`).
private def append_pairs :: (Copy<'key>, Copy<'value>) => Vec<('key * 'value)> -> ref Vec<Entry<'key, 'value>> -> Vec<('key * 'value)>
fn append_pairs output entries =
	let mut result = output
	let mut offset = 0
	while offset < Vec.length entries do
		result = Vec.push (entries[offset].key, entries[offset].value) result
		offset = offset + 1
	result

private def append_keys :: Copy<'key> => Vec<'key> -> ref Vec<Entry<'key, 'value>> -> Vec<'key>
fn append_keys output entries =
	let mut result = output
	let mut offset = 0
	while offset < Vec.length entries do
		result = Vec.push entries[offset].key result
		offset = offset + 1
	result

private def append_values :: Copy<'value> => Vec<'value> -> ref Vec<Entry<'key, 'value>> -> Vec<'value>
fn append_values output entries =
	let mut result = output
	let mut offset = 0
	while offset < Vec.length entries do
		result = Vec.push entries[offset].value result
		offset = offset + 1
	result

def to_array :: (Copy<'key>, Copy<'value>) => ref Map<'key, 'value> -> [('key * 'value)]
fn to_array map =
	let mut output = append_pairs (Vec.with_capacity map.length) (ref map.head)
	let mut chunk = 0
	while chunk < Vec.length (ref map.rest) do
		output = append_pairs output (ref map.rest[chunk])
		chunk = chunk + 1
	Vec.to_array output

def keys :: Copy<'key> => ref Map<'key, 'value> -> ['key]
fn keys map =
	let mut output = append_keys (Vec.with_capacity map.length) (ref map.head)
	let mut chunk = 0
	while chunk < Vec.length (ref map.rest) do
		output = append_keys output (ref map.rest[chunk])
		chunk = chunk + 1
	Vec.to_array output

def values :: Copy<'value> => ref Map<'key, 'value> -> ['value]
fn values map =
	let mut output = append_values (Vec.with_capacity map.length) (ref map.head)
	let mut chunk = 0
	while chunk < Vec.length (ref map.rest) do
		output = append_values output (ref map.rest[chunk])
		chunk = chunk + 1
	Vec.to_array output

def fold :: ('state -> ref 'key -> ref 'value -> 'state) -> 'state -> ref Map<'key, 'value> -> 'state
fn fold folder initial map =
	let mut state = initial
	let total = chunk_total map
	let mut chunk = 0
	while chunk < total do
		let entries = chunk_of map chunk
		let count = Vec.length entries
		let mut offset = 0
		while offset < count do
			state = folder state (ref entries[offset].key) (ref entries[offset].value)
			offset = offset + 1
		chunk = chunk + 1
	state

private def rec iter_from :: ref Map<'key, 'value> -> i64 -> i64 -> Seq<(ref 'key * ref 'value)>
fn rec iter_from map chunk offset = Seq.defer (\() ->
	if chunk < chunk_total map then
		let entries = chunk_of map chunk
		let next = if offset + 1 < Vec.length entries then iter_from map chunk (offset + 1) else iter_from map (chunk + 1) 0
		(next, Maybe.Some (ref entries[offset].key, ref entries[offset].value))
	else (Seq.empty(), Maybe.None))

def iter :: ref Map<'key, 'value> -> Seq<(ref 'key * ref 'value)>
fn iter map = iter_from map 0 0

/// Whether the chunks obey their rules: 1 to 512 entries each, more than 256 entries in any two neighbours, strictly ascending keys,
/// and `length` equal to the number of entries. A diagnostic for tests, like `HashMap.longest_probe`.
def invariants_hold :: Ord<'key> => ref Map<'key, 'value> -> bool
fn invariants_hold map =
	let total = chunk_total map
	let mut valid = map.length > 0 || (Vec.length (ref map.head) == 0 && Vec.length (ref map.rest) == 0)
	let mut sum = 0
	let mut chunk = 0
	while chunk < total do
		let entries = chunk_of map chunk
		let size = Vec.length entries
		if size < 1 || size > MaxChunk then valid = false
		if chunk > 0 && Vec.length (chunk_of map (chunk - 1)) + size <= MergeLimit then valid = false
		if size > 0 then
			if chunk > 0 then
				let before = chunk_of map (chunk - 1)
				if !(Ord.lt (ref before[Vec.length before - 1].key) (ref entries[0].key)) then valid = false
			let mut offset = 1
			while offset < size do
				if !(Ord.lt (ref entries[offset - 1].key) (ref entries[offset].key)) then valid = false
				offset = offset + 1
		sum = sum + size
		chunk = chunk + 1
	valid && sum == map.length
```

## B. `std/Set.tz`（全文。446 行・18,255 バイト）

`Set` は `Map` と同じ構造・同じ規則で、`Entry` を持たず `Vec<'key>` を直接持つ。`union`・`intersect`・`difference` が `Set` だけの関数である。

<!-- file: std/Set.tz -->
```text
/// The keys in ascending key order, kept in chunks of 1 to 512 keys (see `Map`): `head` is the first chunk and `rest` holds the others in order;
/// `length` is the number of keys. A set without keys has an empty `head` and an empty `rest`, and any two neighbouring chunks hold more than
/// 256 keys together.
record Set<'key> { head: Vec<'key>, rest: Vec<Vec<'key>>, length: i64 }

/// The most keys of a chunk; a full chunk splits in two when a key goes into it.
private const MaxChunk: i64 = 512

/// Two neighbouring chunks that hold this many keys or fewer together are merged into one.
private const MergeLimit: i64 = 256

/// The number of keys of the chunks that `union`, `intersect` and `difference` build, except for the last one.
private const BulkFill: i64 = 384

/// Takes the item at `index` out of `items` in constant time, which moves the last item to `index`; `put` with the same `index` undoes that.
private def take :: Vec<'a> -> i64 -> (Vec<'a> * 'a)
fn take items index =
	let last = Vec.length (ref items) - 1
	match Vec.pop (Vec.swap index last items) with
	| (rest, Maybe.Some item) -> (rest, item)
	| (_, Maybe.None) -> unreachable ()

private def put :: Vec<'a> -> i64 -> 'a -> Vec<'a>
fn put items index item =
	let pushed = Vec.push item items
	let last = Vec.length (ref pushed) - 1
	Vec.swap index last pushed

private def insert_at :: Vec<'a> -> i64 -> 'a -> Vec<'a>
fn insert_at items index item =
	let mut result = Vec.push item items
	let mut position = Vec.length (ref result) - 1
	while position > index do
		result = Vec.swap position (position - 1) result
		position = position - 1
	result

private def remove_at :: Vec<'a> -> i64 -> (Vec<'a> * 'a)
fn remove_at items index =
	let mut result = items
	let last = Vec.length (ref result) - 1
	let mut position = index
	while position < last do
		result = Vec.swap position (position + 1) result
		position = position + 1
	match Vec.pop result with
	| (rest, Maybe.Some item) -> (rest, item)
	| (_, Maybe.None) -> unreachable ()

private def reverse :: Vec<'a> -> Vec<'a>
fn reverse items =
	let mut result = items
	let count = Vec.length (ref result)
	let mut index = 0
	while index < count / 2 do
		result = Vec.swap index (count - 1 - index) result
		index = index + 1
	result

/// Moves the items from `at` on into a new `Vec`, in order.
private def split_off :: Vec<'a> -> i64 -> (Vec<'a> * Vec<'a>)
fn split_off items at =
	let mut head = items
	let mut tail = Vec.with_capacity (Vec.length (ref head) - at)
	while Vec.length (ref head) > at do
		match Vec.pop head with
		| (rest, Maybe.Some item) ->
			head = rest
			tail = Vec.push item tail
		| (rest, Maybe.None) -> head = rest
	(head, reverse tail)

/// Appends the items of `tail` to `head`, in order.
private def join :: Vec<'a> -> Vec<'a> -> Vec<'a>
fn join head tail =
	let mut result = head
	let mut rest = reverse tail
	while Vec.length (ref rest) > 0 do
		match Vec.pop rest with
		| (remaining, Maybe.Some item) ->
			rest = remaining
			result = Vec.push item result
		| (remaining, Maybe.None) -> rest = remaining
	result

def empty :: Set<'key>
fn empty = Set { head: Vec.empty(), rest: Vec.empty(), length: 0 }

def singleton :: 'key -> Set<'key>
fn singleton key = Set { head: Vec.push key (Vec.empty()), rest: Vec.empty(), length: 1 }

def length :: ref Set<'key> -> i64
fn length set = set.length

def is_empty :: ref Set<'key> -> bool
fn is_empty set = set.length == 0

private def chunk_total :: ref Set<'key> -> i64
fn chunk_total set = if set.length == 0 then 0 else 1 + Vec.length (ref set.rest)

/// The number of chunks that hold the keys (0 for an empty set). A diagnostic for tests, like `HashSet.longest_probe`.
def chunk_count :: ref Set<'key> -> i64
fn chunk_count set = chunk_total set

/// Chunk `index` in key order: `head` for 0, `rest[index - 1]` after it.
private def chunk_of :: ref Set<'key> -> i64 -> ref Vec<'key>
fn chunk_of set index = if index == 0 then ref set.head else ref set.rest[index - 1]

/// Where `key` is: `(chunk, offset)` when the set has it, and `(chunk, -1 - offset)` when it has not, where `offset` is the place in `chunk`
/// where it would go. The chunk is the first one whose last key is not less than `key`, or the last chunk. An empty set gives `(0, -1)`.
private def locate :: Ord<'key> => ref Set<'key> -> ref 'key -> (i64 * i64)
fn locate set key =
	assert (Eq.eq key key)
	let total = chunk_total set
	if total == 0 then (0, -1)
	else
		let mut low = 0
		let mut high = total - 1
		while low < high do
			let middle = low + (high - low) / 2
			let chunk = chunk_of set middle
			let last = Vec.length chunk - 1
			assert (Eq.eq (ref chunk[last]) (ref chunk[last]))
			if Ord.lt (ref chunk[last]) key then low = middle + 1 else high = middle
		let keys = chunk_of set low
		let size = Vec.length keys
		let mut start = 0
		let mut stop = size
		while start < stop do
			let middle = start + (stop - start) / 2
			assert (Eq.eq (ref keys[middle]) (ref keys[middle]))
			if Ord.lt (ref keys[middle]) key then start = middle + 1 else stop = middle
		if start < size && !(Ord.lt key (ref keys[start])) then (low, start) else (low, -1 - start)

def contains :: Ord<'key> => ref Set<'key> -> 'key -> bool
fn contains set key =
	match locate set (ref key) with
	| (_chunk, offset) -> offset >= 0

/// Takes chunk `index` out of the set. The set that comes back has a hole at `index`; `put_chunk` fills it.
private def take_chunk :: Set<'key> -> i64 -> (Set<'key> * Vec<'key>)
fn take_chunk set index =
	if index == 0 then (Set { head: Vec.empty(), rest: set.rest, length: set.length }, set.head)
	else
		match take set.rest (index - 1) with
		| (others, chunk) -> (Set { head: set.head, rest: others, length: set.length }, chunk)

private def put_chunk :: Set<'key> -> i64 -> Vec<'key> -> i64 -> Set<'key>
fn put_chunk set index chunk count =
	if index == 0 then Set { head: chunk, rest: set.rest, length: count }
	else Set { head: set.head, rest: put set.rest (index - 1) chunk, length: count }

/// Puts `key` at `offset` of chunk `chunk`. A full chunk splits in two halves first, except at the end of the last chunk, which starts a new chunk.
private def add_key :: Set<'key> -> i64 -> i64 -> 'key -> Set<'key>
fn add_key set chunk offset key =
	let count = set.length
	if count == 0 then Set { head: Vec.push key (Vec.empty()), rest: Vec.empty(), length: 1 }
	else
		let total = chunk_total (ref set)
		let size = Vec.length (chunk_of (ref set) chunk)
		if size < MaxChunk then
			match take_chunk set chunk with
			| (hole, keys) -> put_chunk hole chunk (insert_at keys offset key) (count + 1)
		elif chunk == total - 1 && offset == size then Set { head: set.head, rest: Vec.push (Vec.push key (Vec.empty())) set.rest, length: count + 1 }
		else
			match take_chunk set chunk with
			| (hole, keys) ->
				let middle = size / 2
				match split_off keys middle with
				| (left, right) ->
					if offset <= middle then
						let restored = put_chunk hole chunk (insert_at left offset key) (count + 1)
						Set { head: restored.head, rest: insert_at restored.rest chunk right, length: count + 1 }
					else
						let restored = put_chunk hole chunk left (count + 1)
						Set { head: restored.head, rest: insert_at restored.rest chunk (insert_at right (offset - middle) key), length: count + 1 }

/// Whether `key` is greater than every key of the set, which makes an insertion an append.
private def beyond_last :: Ord<'key> => ref Set<'key> -> ref 'key -> bool
fn beyond_last set key =
	assert (Eq.eq key key)
	if set.length == 0 then false
	else
		let keys = chunk_of set (chunk_total set - 1)
		let last = Vec.length keys - 1
		assert (Eq.eq (ref keys[last]) (ref keys[last]))
		Ord.lt (ref keys[last]) key

def insert :: Ord<'key> => Set<'key> -> 'key -> Set<'key>
fn insert set key =
	if beyond_last (ref set) (ref key) then
		let chunk = chunk_total (ref set) - 1
		let size = Vec.length (chunk_of (ref set) chunk)
		add_key set chunk size key
	else
		match locate (ref set) (ref key) with
		| (chunk, offset) -> if offset >= 0 then set else add_key set chunk (-1 - offset) key

/// Merges chunk `index + 1` into chunk `index`.
private def merge_next :: Set<'key> -> i64 -> Set<'key>
fn merge_next set index =
	let count = set.length
	match remove_at set.rest index with
	| (others, tail) ->
		match take_chunk (Set { head: set.head, rest: others, length: count }) index with
		| (hole, keys) -> put_chunk hole index (join keys tail) count

/// Restores the rule for neighbouring chunks after chunk `index` lost a key: it merges with the chunk before it or, failing that, after it.
private def shrink :: Set<'key> -> i64 -> Set<'key>
fn shrink set index =
	let size = Vec.length (chunk_of (ref set) index)
	if index > 0 && Vec.length (chunk_of (ref set) (index - 1)) + size <= MergeLimit then merge_next set (index - 1)
	elif index + 1 < chunk_total (ref set) && size + Vec.length (chunk_of (ref set) (index + 1)) <= MergeLimit then merge_next set index
	else set

/// Removes chunk `index`, which holds one key; the next chunk becomes `head` when that is chunk 0.
private def delete_chunk :: Set<'key> -> i64 -> Set<'key>
fn delete_chunk set index =
	let count = set.length - 1
	if index == 0 then
		if Vec.length (ref set.rest) == 0 then Set { head: Vec.empty(), rest: Vec.empty(), length: 0 }
		else
			match remove_at set.rest 0 with
			| (others, first) -> Set { head: first, rest: others, length: count }
	else
		match remove_at set.rest (index - 1) with
		| (others, _gone) -> Set { head: set.head, rest: others, length: count }

def remove :: Ord<'key> => Set<'key> -> 'key -> Set<'key>
fn remove set key =
	match locate (ref set) (ref key) with
	| (chunk, offset) ->
		if offset >= 0 then
			let count = set.length
			if Vec.length (chunk_of (ref set) chunk) == 1 then delete_chunk set chunk
			else
				match take_chunk set chunk with
				| (hole, keys) ->
					match remove_at keys offset with
					| (remaining, _removed) -> shrink (put_chunk hole chunk remaining (count - 1)) chunk
		else set

/// Reads the keys through its own parameter, not through a local reference: a key that holds a borrow would otherwise keep the loan of that
/// reference alive in the `Vec` that is built, and moving that `Vec` would be rejected (`E1014`).
private def append_keys :: Copy<'key> => Vec<'key> -> ref Vec<'key> -> Vec<'key>
fn append_keys output keys =
	let mut result = output
	let mut offset = 0
	while offset < Vec.length keys do
		result = Vec.push keys[offset] result
		offset = offset + 1
	result

def to_array :: Copy<'key> => ref Set<'key> -> ['key]
fn to_array set =
	let mut output = append_keys (Vec.with_capacity set.length) (ref set.head)
	let mut chunk = 0
	while chunk < Vec.length (ref set.rest) do
		output = append_keys output (ref set.rest[chunk])
		chunk = chunk + 1
	Vec.to_array output

def fold :: ('state -> ref 'key -> 'state) -> 'state -> ref Set<'key> -> 'state
fn fold folder initial set =
	let mut state = initial
	let total = chunk_total set
	let mut chunk = 0
	while chunk < total do
		let keys = chunk_of set chunk
		let count = Vec.length keys
		let mut offset = 0
		while offset < count do
			state = folder state (ref keys[offset])
			offset = offset + 1
		chunk = chunk + 1
	state

private def compare_keys :: Ord<'key> => ref 'key -> ref 'key -> i64
fn compare_keys left right =
	assert (Eq.eq left left && Eq.eq right right)
	if Ord.lt left right then -1 else if Ord.lt right left then 1 else 0

/// All chunks of `set`, in key order, in one `Vec`.
private def chunk_list :: Set<'key> -> Vec<Vec<'key>>
fn chunk_list set = if set.length == 0 then Vec.empty() else insert_at set.rest 0 set.head

/// Gives `keys` the last chunk of `chunks` when `keys` has run out.
private def refill :: Vec<Vec<'a>> -> Vec<'a> -> (Vec<Vec<'a>> * Vec<'a>)
fn refill chunks keys =
	if Vec.length (ref keys) > 0 then (chunks, keys)
	else
		match Vec.pop chunks with
		| (rest, Maybe.Some chunk) -> (rest, chunk)
		| (rest, Maybe.None) -> (rest, keys)

/// The set whose chunks are `chunks`, in key order, with `total` keys.
private def from_chunks :: Vec<Vec<'key>> -> i64 -> Set<'key>
fn from_chunks chunks total =
	if total == 0 then Set { head: Vec.empty(), rest: Vec.empty(), length: 0 }
	else
		match remove_at chunks 0 with
		| (others, first) -> Set { head: first, rest: others, length: total }

def union :: Ord<'key> => Set<'key> -> Set<'key> -> Set<'key>
fn union left right =
	let left_count = left.length
	let right_count = right.length
	assert (left_count <= 9223372036854775807 - right_count)
	let mut left_chunks = chunk_list left
	let mut right_chunks = chunk_list right
	let mut left_keys: Vec<'key> = Vec.empty()
	let mut right_keys: Vec<'key> = Vec.empty()
	let mut finished: Vec<Vec<'key>> = Vec.empty()
	let mut current: Vec<'key> = Vec.with_capacity (if left_count + right_count < BulkFill then left_count + right_count else BulkFill)
	let mut total = 0
	let mut more = true
	while more do
		match refill left_chunks left_keys with
		| (rest, keys) ->
			left_chunks = rest
			left_keys = keys
		match refill right_chunks right_keys with
		| (rest, keys) ->
			right_chunks = rest
			right_keys = keys
		let left_length = Vec.length (ref left_keys)
		let right_length = Vec.length (ref right_keys)
		if left_length == 0 && right_length == 0 then more = false
		else
			let order = if left_length == 0 then -1 else if right_length == 0 then 1 else compare_keys (ref left_keys[left_length - 1]) (ref right_keys[right_length - 1])
			if order >= 0 then
				match Vec.pop left_keys with
				| (remaining, value) ->
					left_keys = remaining
					let key = Maybe.get value
					assert (Eq.eq (ref key) (ref key))
					current = Vec.push key current
			else
				match Vec.pop right_keys with
				| (remaining, value) ->
					right_keys = remaining
					let key = Maybe.get value
					assert (Eq.eq (ref key) (ref key))
					current = Vec.push key current
			total = total + 1
			if order == 0 then
				match Vec.pop right_keys with
				| (remaining, _duplicate) -> right_keys = remaining
			if Vec.length (ref current) == BulkFill then
				finished = Vec.push (reverse current) finished
				current = Vec.with_capacity BulkFill
	if Vec.length (ref current) > 0 then finished = Vec.push (reverse current) finished
	from_chunks (reverse finished) total

/// The key at `offset` of chunk `chunk`, copied. Like `append_keys`, it reads through its own parameter.
private def key_at :: Copy<'key> => ref Set<'key> -> i64 -> i64 -> 'key
fn key_at set chunk offset = if chunk == 0 then set.head[offset] else set.rest[chunk - 1][offset]

private def chunk_size :: ref Set<'key> -> i64 -> i64
fn chunk_size set chunk = if chunk == 0 then Vec.length (ref set.head) else Vec.length (ref set.rest[chunk - 1])

private def select :: (Ord<'key>, Copy<'key>) => ref Set<'key> -> ref Set<'key> -> bool -> Set<'key>
fn select left right include_equal =
	let left_count = left.length
	let right_count = right.length
	let capacity = if include_equal && right_count < left_count then right_count else left_count
	let mut finished: Vec<Vec<'key>> = Vec.empty()
	let mut current: Vec<'key> = Vec.with_capacity (if capacity < BulkFill then capacity else BulkFill)
	let mut total = 0
	let mut left_index = 0
	let mut left_chunk = 0
	let mut left_offset = 0
	let mut right_index = 0
	let mut right_chunk = 0
	let mut right_offset = 0
	while left_index < left_count do
		let key = key_at left left_chunk left_offset
		assert (Eq.eq (ref key) (ref key))
		let order =
			if right_index >= right_count then -1
			else
				let other = key_at right right_chunk right_offset
				compare_keys (ref key) (ref other)
		if order <= 0 then
			if (order == 0) == include_equal then
				current = Vec.push key current
				total = total + 1
				if Vec.length (ref current) == BulkFill then
					finished = Vec.push current finished
					current = Vec.with_capacity BulkFill
			left_index = left_index + 1
			if left_offset + 1 < chunk_size left left_chunk then left_offset = left_offset + 1
			else
				left_chunk = left_chunk + 1
				left_offset = 0
		if order >= 0 then
			right_index = right_index + 1
			if right_index < right_count then
				if right_offset + 1 < chunk_size right right_chunk then right_offset = right_offset + 1
				else
					right_chunk = right_chunk + 1
					right_offset = 0
	if Vec.length (ref current) > 0 then finished = Vec.push current finished
	from_chunks finished total

def intersect :: (Ord<'key>, Copy<'key>) => ref Set<'key> -> ref Set<'key> -> Set<'key>
fn intersect left right = select left right true

def difference :: (Ord<'key>, Copy<'key>) => ref Set<'key> -> ref Set<'key> -> Set<'key>
fn difference left right = select left right false

private def rec iter_from :: ref Set<'key> -> i64 -> i64 -> Seq<ref 'key>
fn rec iter_from set chunk offset = Seq.defer (\() ->
	if chunk < chunk_total set then
		let keys = chunk_of set chunk
		let next = if offset + 1 < Vec.length keys then iter_from set chunk (offset + 1) else iter_from set (chunk + 1) 0
		(next, Maybe.Some (ref keys[offset]))
	else (Seq.empty(), Maybe.None))

def iter :: ref Set<'key> -> Seq<ref 'key>
fn iter set = iter_from set 0 0

/// Whether the chunks obey their rules: 1 to 512 keys each, more than 256 keys in any two neighbours, strictly ascending keys, and `length`
/// equal to the number of keys. A diagnostic for tests, like `HashSet.longest_probe`.
def invariants_hold :: Ord<'key> => ref Set<'key> -> bool
fn invariants_hold set =
	let total = chunk_total set
	let mut valid = set.length > 0 || (Vec.length (ref set.head) == 0 && Vec.length (ref set.rest) == 0)
	let mut sum = 0
	let mut chunk = 0
	while chunk < total do
		let keys = chunk_of set chunk
		let size = Vec.length keys
		if size < 1 || size > MaxChunk then valid = false
		if chunk > 0 && Vec.length (chunk_of set (chunk - 1)) + size <= MergeLimit then valid = false
		if size > 0 then
			if chunk > 0 then
				let before = chunk_of set (chunk - 1)
				if !(Ord.lt (ref before[Vec.length before - 1]) (ref keys[0])) then valid = false
			let mut offset = 1
			while offset < size do
				if !(Ord.lt (ref keys[offset - 1]) (ref keys[offset])) then valid = false
				offset = offset + 1
		sum = sum + size
		chunk = chunk + 1
	valid && sum == set.length
```

## C. ベンチマークの program（`benchmarks/ordered/Main.tz`。Phase 2）

使い方は `exe <workload> <size>` で、`<workload> <size> <checksum> <nanoseconds>` を 1 行出す。**計時するのは操作だけ**で、操作の対象のコンテナを作る時間は含めない
（`insert_random`・`insert_ascending`・`update_counter` は、挿入そのものが操作なので構築を含む）。checksum は旧実装と新実装で一致しなければならない（一致しない
ときは、速さの比較をせず、まず実装を疑う）。インデントは空白 4 つ（`tsuzuri fmt --check` が通る形）。

<!-- file: benchmarks/ordered/Main.tz -->
```text
// Measures the ordered containers: `exe <workload> <size>` prints "<workload> <size> <checksum> <nanoseconds>".
// Only the operation is timed; building the container that it works on is not.

def next_state :: i64u -> i64u
fn next_state state = state * 6364136223846793005 + 1442695040888963407

def fold :: i64u -> i64u -> i64u
fn fold total value = total * 31 + value

def random_keys :: i64 -> [i64u]
fn random_keys size =
    let mut keys = Vec.with_capacity size
    let mut state = 88172645463325252
    for _index in 0i64 .. size - 1 do
        state = next_state state
        keys = Vec.push (state >>> 20) keys
    Vec.to_array keys

def filled :: ref [i64u] -> Map<i64u, i64u>
fn filled keys =
    let mut map: Map<i64u, i64u> = Map.empty()
    for index in 0i64 .. keys.length - 1 do map = Map.insert map keys[index] (keys[index] & 1023)
    map

def ascending_set :: i64 -> i64 -> Set<i64>
fn ascending_set size step =
    let mut set: Set<i64> = Set.empty()
    for index in 0i64 .. size - 1 do set = Set.insert set (index * step)
    set

def insert_random :: i64 -> (i64u * i64)
fn insert_random size =
    let keys = random_keys size
    let! first = Time.monotonic_ns ()
    let map = filled (ref keys)
    let! second = Time.monotonic_ns ()
    (Map.length (ref map) as i64u, Result.get second - Result.get first)

def lookup_random :: i64 -> (i64u * i64)
fn lookup_random size =
    let keys = random_keys size
    let map = filled (ref keys)
    let! first = Time.monotonic_ns ()
    let mut total = 0i64u
    for index in 0i64 .. size - 1 do total = fold total (Maybe.default_value 0 (Map.get (ref map) keys[index]))
    let! second = Time.monotonic_ns ()
    (total, Result.get second - Result.get first)

def remove_random :: i64 -> (i64u * i64)
fn remove_random size =
    let keys = random_keys size
    let mut map = filled (ref keys)
    let! first = Time.monotonic_ns ()
    for index in 0i64 .. size - 1 do map = Map.remove map keys[index]
    let! second = Time.monotonic_ns ()
    (Map.length (ref map) as i64u, Result.get second - Result.get first)

def insert_ascending :: i64 -> (i64u * i64)
fn insert_ascending size =
    let! first = Time.monotonic_ns ()
    let mut map: Map<i64, i64> = Map.empty()
    for index in 0i64 .. size - 1 do map = Map.insert map index 1
    let! second = Time.monotonic_ns ()
    (Map.length (ref map) as i64u, Result.get second - Result.get first)

// The map_count kernel of benchmarks/apps: `size` updates of the counters of 1,024 keys.
def update_counter :: i64 -> (i64u * i64)
fn update_counter size =
    let mut counts: Map<i64u, i64u> = Map.empty()
    for key in 0i64u .. 1023i64u do counts = Map.insert counts key 0
    let! first = Time.monotonic_ns ()
    let mut state = 1i64u
    for _index in 0i64 .. size - 1 do
        state = next_state state
        let key = (state >>> 33) & 1023
        let count = Maybe.default_value 0 (Map.get (ref counts) key)
        counts = Map.insert counts key (count + 1)
    let! second = Time.monotonic_ns ()
    let mut total = 0i64u
    for key in 0i64u .. 1023i64u do total = fold total (Maybe.default_value 0 (Map.get (ref counts) key))
    (total, Result.get second - Result.get first)

def set_algebra :: i64 -> (i64u * i64)
fn set_algebra size =
    let left = ascending_set size 2
    let right = ascending_set size 3
    let! first = Time.monotonic_ns ()
    let common = Set.intersect (ref left) (ref right)
    let different = Set.difference (ref left) (ref right)
    let both = Set.union left right
    let! second = Time.monotonic_ns ()
    ((Set.length (ref common) + Set.length (ref different) + Set.length (ref both)) as i64u, Result.get second - Result.get first)

def main :: Array<string> -> i32 = \args ->
    let size: i64 = Maybe.get (Parse.parse (ref args[1]))
    let name = clone_string (ref args[0])
    let outcome =
        if name == "insert_random" then insert_random size
        elif name == "lookup_random" then lookup_random size
        elif name == "remove_random" then remove_random size
        elif name == "insert_ascending" then insert_ascending size
        elif name == "update_counter" then update_counter size
        else set_algebra size
    match outcome with
    | (checksum, elapsed) -> do! IO.writeln $"{name} {size} {checksum} {elapsed}"
    0
```

## D. 実測（参考値。速度の合否の閾値には使わない）

測定環境: Apple M1 Max（10 コア）・64 GB・macOS 27.0.1・Apple clang 21.0.0・rustc 1.98.1。**測定中のロードアベレージは 11〜36**（ほかの担当者のビルドが動いていた）で、時間の揺れは大きい。
`tsuzuri build <dir> -O3 -o <exe>` で作った実行ファイルを、5 回ずつ（交互に）走らせた中央値（範囲）。before = HEAD の `std`、after = 付録 A・B の `std`。
checksum は、すべての workload と大きさで before と after が一致した（10 通りの workload・大きさで確認）。

| workload | size | before | after | 比 |
| --- | --- | --- | --- | --- |
| `insert_random` | 10^4 | 約 16 ms（13.5〜24） | 約 1.7 ms | 約 9 倍速い |
| `insert_random` | 10^5 | 約 1.84 s（1.57〜2.85）。最後の再測定 1.30 s | 約 25 ms（最後の再測定 23.5 ms） | 約 55〜75 倍速い |
| `insert_random` | 10^6 | 200.5 s（1 回） | 0.4〜0.74 s | 約 300〜500 倍速い |
| `remove_random` | 10^4 | 約 7.1 ms | 約 1.35 ms | 約 5 倍速い |
| `remove_random` | 10^5 | 約 1.11 s（最後の再測定 0.85 s） | 約 22.7 ms（最後の再測定 22.2 ms） | 約 40〜50 倍速い |
| `remove_random` | 10^6 | 105.1 s（1 回） | 0.55〜0.85 s | 約 120〜190 倍速い |
| `lookup_random` | 10^4 | 約 0.41 ms | 約 0.65 ms | 約 1.6 倍**遅い** |
| `lookup_random` | 10^5 | 約 10.8 ms（最後の再測定 6.3 ms） | 約 9.8 ms（最後の再測定 9.7 ms） | 同程度〜1.5 倍遅い |
| `lookup_random` | 10^6 | 0.368 s | 0.27〜0.67 s | 同程度（揺れが大きい） |
| `insert_ascending` | 10^6 | 約 52 ms（最後の再測定 42 ms） | 約 22 ms（最後の再測定 20 ms） | 約 2〜2.5 倍速い |
| `update_counter`（1,024 キー） | 5×10^6 | 約 235 ms | 約 283 ms | 約 1.2 倍遅い |
| `set_algebra` | 10^6 | 約 14.4 ms | 約 18 ms（最後の再測定 16.4 ms） | 約 1.1〜1.25 倍遅い |

小さいマップ（512 エントリ以下で、チャンクが `head` 1 つだけの間）の 1 操作あたりの比（after ÷ before）: `get` 約 1.04、既存キーの置換 約 1.2、`Set` の `insert`／`contains` 約 1.1、500 キーのマップの `remove` + `insert` 約 1.17。
1,024 キーでは、既存キーの置換 約 2.1（チャンクが 2 つになり、`rest` 側を経由する）、`remove` + `insert` は約 2 倍**速い**。
チャンクの最大長 B の振り: B = 256 は、ランダム挿入で n = 3×10^3〜3×10^5 のとき B = 512 より 12〜19% 速かったが、257〜512 エントリのマップが 1 チャンクの経路で済む利点と
種（seed）の値を優先して **B = 512** を採った（決定事項 D3）。B = 128・1024・2048 は、ランダム挿入で B = 512 より遅いか、小さいマップの置換が遅かった。
LLVM IR の大きさ（`-O3`、`benchmarks` の program）: 33,544 → 33,640 バイト。

**`Ord.lt` の呼び出し回数**（`comparison_bounds` の実測。ランダムな 32,768 キー、`Probe` キー）:

| 局面 | 実測の合計 | 上界 `n × (⌊log2 n⌋ + 6)` = 688,128 |
| --- | --- | --- |
| 挿入（32,768 回） | 512,688 | 下回る |
| 検索（32,768 回。全部ある） | 529,816 | 下回る |
| 削除（32,768 回） | 482,816 | 下回る |

挿入中のチャンク数の最大は 97（上界 2n/257 + 1 = 256.0 の約 1/2.6）。n = 10^6 のランダム挿入では、`Ord.lt` は挿入 20.6M・検索 21.1M・削除 19.7M 回、チャンクは 2,726 個
（n/チャンク数 = 367 エントリ。1 チャンクの平均が 512 の 72%）だった。

**変異による検出力の確認**（筆者が `std` に 1 つずつ入れて、`map_set_chunks` の境界の case と性質テストを走らせた。実装者は再実行しない）:

| 変異 | `std` の変更 | 検出したテスト |
| --- | --- | --- |
| M3 | `delete_chunk` が `index == 0` で、次のチャンクを `head` に昇格しない | 境界の `head_deleted` だけ（`199256` ≠ `201256`）。性質テストは見逃した |
| M5 | チャンク内の二分探索の比較の向きを逆にする | 性質テスト |
| M7 | `put` が最後の `swap` をしない | 性質テスト |
| M8 | 合計が 256 を超えても併合する | 境界の `merge_after_removals`（`21001256` ≠ `22001256`） |

## E. 再現用のプログラム（本体「現状」の `E1014` の実出力の元）

`r2` は `E1014`（`error[E1014]: cannot move an exclusive reference while it is reborrowed`、19 行 33 桁）、`r3` は通る（出力なし）。違いは、`Probe` を読む箇所が、ローカルの `ref`（`let keys = chunk_of source chunk`）か、
引数 `ref Bag` を受け取る関数 `key_at` か、だけである。`tsuzuri check <dir>/Main.tz` で確かめる。

`r2/Main.tz`:

```text
record Probe { value: i64, hits: ref Atomic<i64> }
record Bag<'key> { head: Vec<'key>, rest: Vec<Vec<'key>>, length: i64 }

private def chunk_of :: ref Bag<'key> -> i64 -> ref Vec<'key>
fn chunk_of bag index = if index == 0 then ref bag.head else ref bag.rest[index - 1]

def collect :: Copy<'key> => ref Bag<'key> -> Vec<Vec<'key>>
fn collect source =
	let mut finished: Vec<Vec<'key>> = Vec.empty()
	let mut current: Vec<'key> = Vec.empty()
	let mut index = 0
	let mut chunk = 0
	let mut offset = 0
	while index < source.length do
		let keys = chunk_of source chunk
		let key = keys[offset]
		current = Vec.push key current
		if Vec.length (ref current) == 4 then
			finished = Vec.push current finished
			current = Vec.empty()
		index = index + 1
		if offset + 1 < Vec.length keys then offset = offset + 1
		else
			chunk = chunk + 1
			offset = 0
	finished

def run :: i64 -> i64
fn run size =
	let hits = Atomic.create 0i64
	let mut items: Vec<Probe> = Vec.empty()
	for index in 0i64 .. size - 1 do items = Vec.push (Probe { value: index, hits: ref hits }) items
	let bag = Bag { head: items, rest: Vec.empty(), length: size }
	let chunks = collect (ref bag)
	Vec.length (ref chunks)

run 10
```

`r3/Main.tz`（`record` と `run` は `r2` と同じ。`chunk_of` と `collect` を次に変える）:

```text
private def key_at :: Copy<'key> => ref Bag<'key> -> i64 -> i64 -> 'key
fn key_at bag chunk offset = if chunk == 0 then bag.head[offset] else bag.rest[chunk - 1][offset]

def collect :: Copy<'key> => ref Bag<'key> -> Vec<Vec<'key>>
fn collect source =
	let mut finished: Vec<Vec<'key>> = Vec.empty()
	let mut current: Vec<'key> = Vec.empty()
	let mut index = 0
	while index < source.length do
		current = Vec.push (key_at source 0 index) current
		if Vec.length (ref current) == 4 then
			finished = Vec.push current finished
			current = Vec.empty()
		index = index + 1
	finished
```

**IR の連番の確認**: 次の 3 行の `Main.tz` を、HEAD のコンパイラと付録 A・B のコンパイラで `tsuzuri build <dir> --emit llvm -O0 -o <out>.ll` し、`diff` した。

```text
def main :: Array<string> -> i32 = \args ->
    do! IO.writeln "hello"
    0
```

差は 5 行で、すべて `$intrinsic.Display.display.426` と `.475` の違い（`-O3` も同じ）。`W1001`（未使用の `args`）の警告は出る。
