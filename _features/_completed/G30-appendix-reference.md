# G30 付録 2/2（図表の生成: 参照実装と検証の手順）

[G30-diagrams.md](G30-diagrams.md) の付録。付録 E〜F を置く。付録 A〜D（制約・fixture・期待出力）は [G30-appendix.md](G30-appendix.md)。

## 付録 E: 参照実装（Python 3）

**期待出力の正本**。`fixtures.py` が付録 C の fixture の KB の行を作り、`gen.py` が付録 D の全期待出力を作り、`validate_ref.py` が構造検証器の参照実装。Rust の実装は、`diagram_ref.py` の関数と 1 対 1 に対応させる（G30 の「アルゴリズム」の表）。**リポジトリには入れない**（読むための資料。期待出力と fixture は `tests/golden/diagram/`・`tests/fixtures/diagram/` に入れる）。**参照実装が実装していないもの**: 辺 400 本・テキスト 45,000 文字の上限の処理（fixture が小さい）、`Input` の必須キーの検査、ID の解決、`transitions` の構文木の抽出（`gen.py` に手計算の 7 つの遷移を渡している）。これらは本文の規則が正本。作り直すときの手順:

```text
mkdir -p g30/ref && cd g30/ref        # 下の 4 つのファイルをここに置く（neg/ は付録 D.8 の 7 ファイルを g30/neg/ に置く）
python3 fixtures.py                   # ../kb/<名前>/*.jsonl を書く
python3 gen.py                        # ../out/*.mmd・*.dot・*.md を書く（各ファイルの文字数を表示する）
python3 validate_ref.py ../out/*.mmd ../neg/*.mmd   # 期待出力は全部 OK、負の例は全部 FAIL
```

**E.1 `diagram_ref.py`**（図の生成。共通の書き手・7 種の図・views・DOT）:

````python
"""Reference implementation of the G30 diagram generators.

Reads a KB directory (symbols.jsonl, graph.jsonl, facts.jsonl = all modules, layout.jsonl).
Only the keys that the ticket lists are read; unknown keys are ignored.
"""
import json
import re
from collections import defaultdict
from pathlib import Path

IO_EFFECTS = {"io", "os", "net", "task", "atomic", "rc", "extern", "debug", "user_drop", "gpu"}
REGION_CALLS = {"Task.parallel", "Task.scope"}
LABEL_LIMIT = 60
EDGE_LABEL_LIMIT = 40
MEMBER_LIMIT = 12
VARIABLE_LIMIT = 12
LEDGER_LIMIT = 200

ESCAPES = {"#": "#35;", '"': "#quot;", "<": "#lt;", ">": "#gt;", "&": "#amp;", "`": "#96;"}


def cut(text, limit):
    text = re.sub(r"[\t\r\n]", " ", text)
    return text if len(text) <= limit else text[: limit - 1] + "\u2026"


def esc(text, limit=LABEL_LIMIT):
    return "".join(ESCAPES.get(ch, ch) for ch in cut(text, limit))


def member(text):
    """The text of a classDiagram member: a bare parenthesis would make Mermaid read a field as a method."""
    return esc(text).replace("(", "#40;").replace(")", "#41;")


def dot_esc(text, limit=LABEL_LIMIT):
    return cut(text, limit).replace("\\", "\\\\").replace('"', '\\"')


def read_jsonl(path):
    path = Path(path)
    if not path.exists():
        return []
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


class Kb:
    def __init__(self, directory):
        directory = Path(directory)
        self.symbols = read_jsonl(directory / "symbols.jsonl")
        self.graph = read_jsonl(directory / "graph.jsonl")
        self.facts = {row["id"]: row for row in read_jsonl(directory / "facts.jsonl")}
        self.layouts = read_jsonl(directory / "layout.jsonl")
        self.by_id = {row["id"]: row for row in self.symbols}
        self.modules = sorted(row["id"] for row in self.symbols if row["kind"] == "module")

    def module_of(self, identifier):
        row = self.by_id.get(identifier)
        return row["module"] if row else None


def position(row):
    return (row["span"]["line"], row["span"].get("column", 0))


# ------------------------------------------------------------------ graph model and writers

class Graph:
    def __init__(self, direction):
        self.direction = direction
        self.nodes = {}  # key -> (label, shape, cls, bold)
        self.edges = []  # (src, dst, label, style, red)

    def node(self, key, label, shape="box", cls=None, bold=False):
        self.nodes[key] = (label, shape, cls, bold)

    def edge(self, src, dst, label=None, style="solid", red=False, weight=1):
        self.edges.append((src, dst, label, style, red, weight))

    def numbered(self):
        keys = sorted(self.nodes)
        ids = {key: index for index, key in enumerate(keys, 1)}
        edges = sorted(self.edges, key=lambda e: (ids[e[0]], ids[e[1]], e[2] or "", e[3]))
        return keys, ids, edges


SHAPES = {"box": ('["', '"]'), "round": ('("', '")'), "stadium": ('(["', '"])'), "subroutine": ('[["', '"]]'), "hex": ('{{"', '"}}')}
ARROWS = {"solid": "-->", "dashed": "-.->"}
CLASS_ORDER = ["pure", "io", "alloc", "trap", "ext", "cycle", "dep", "region", "cell", "note"]
CLASS_DEFS = {
    "pure": "fill:#dafbe1,stroke:#1a7f37",
    "io": "fill:#ddf4ff,stroke:#0969da",
    "alloc": "fill:#fff8c5,stroke:#9a6700",
    "trap": "fill:#ffebe9,stroke:#cf222e",
    "ext": "fill:#f6f8fa,stroke:#8c959f,stroke-dasharray:4 2",
    "cycle": "fill:#ffebe9,stroke:#cf222e",
    "dep": "fill:#f6f8fa,stroke:#8c959f",
    "region": "fill:#fff1e5,stroke:#bc4c00",
    "cell": "fill:#fbefff,stroke:#8250df",
    "note": "fill:#fff8c5,stroke:#d4a72c,stroke-dasharray:4 2",
}
DOT_FILL = {"pure": "#dafbe1", "io": "#ddf4ff", "alloc": "#fff8c5", "trap": "#ffebe9", "ext": "#f6f8fa", "cycle": "#ffebe9", "dep": "#f6f8fa", "region": "#fff1e5", "cell": "#fbefff", "note": "#fff8c5"}
DOT_SHAPE = {"box": "box", "round": "box", "stadium": "ellipse", "subroutine": "box", "hex": "hexagon"}


def flowchart(graph):
    keys, ids, edges = graph.numbered()
    lines = [f"flowchart {graph.direction}"]
    for key in keys:
        label, shape, _, _ = graph.nodes[key]
        opening, closing = SHAPES[shape]
        lines.append(f"    n{ids[key]}{opening}{esc(label)}{closing}")
    for src, dst, label, style, _, _ in edges:
        middle = f"{ARROWS[style]}|\"{esc(label, EDGE_LABEL_LIMIT)}\"|" if label else ARROWS[style]
        lines.append(f"    n{ids[src]} {middle} n{ids[dst]}")
    used = [name for name in CLASS_ORDER if any(graph.nodes[key][2] == name for key in keys)]
    for name in used:
        lines.append(f"    classDef {name} {CLASS_DEFS[name]}")
    for name in used:
        members = ",".join(f"n{ids[key]}" for key in keys if graph.nodes[key][2] == name)
        lines.append(f"    class {members} {name}")
    for key in keys:
        if graph.nodes[key][3]:
            lines.append(f"    style n{ids[key]} stroke-width:3px")
    for index, edge in enumerate(edges):
        if edge[4]:
            lines.append(f"    linkStyle {index} stroke:#cf222e,stroke-width:2px")
    return "\n".join(lines) + "\n"


def dot(graph):
    keys, ids, edges = graph.numbered()
    lines = ["digraph tsuzuri {", f"    rankdir={'TB' if graph.direction == 'TD' else 'LR'};", '    node [shape=box, fontname="monospace"];', '    edge [fontname="monospace"];']
    for key in keys:
        label, shape, cls, bold = graph.nodes[key]
        styles = []
        attributes = [f'label="{dot_esc(label)}"', f"shape={DOT_SHAPE[shape]}"]
        if shape == "round":
            styles.append("rounded")
        if shape == "subroutine":
            attributes.append("peripheries=2")
        if cls:
            styles.append("filled")
            attributes.append(f'fillcolor="{DOT_FILL[cls]}"')
            if cls in ("ext", "note"):
                styles.append("dashed")
        if styles:
            attributes.append(f'style="{",".join(styles)}"')
        if bold:
            attributes.append("penwidth=3")
        lines.append(f"    n{ids[key]} [{', '.join(attributes)}];")
    for src, dst, label, style, red, _ in edges:
        attributes = []
        if label:
            attributes.append(f'label="{dot_esc(label, EDGE_LABEL_LIMIT)}"')
        if style == "dashed":
            attributes.append("style=dashed")
        if red:
            attributes.append('color="#cf222e"')
        suffix = f" [{', '.join(attributes)}]" if attributes else ""
        lines.append(f"    n{ids[src]} -> n{ids[dst]}{suffix};")
    lines.append("}")
    return "\n".join(lines) + "\n"


def strongly_connected(nodes, adjacency):
    index, low, stack, on_stack, counter, result = {}, {}, [], set(), [0], []

    def visit(v):
        index[v] = low[v] = counter[0]
        counter[0] += 1
        stack.append(v)
        on_stack.add(v)
        for w in sorted(adjacency.get(v, ())):
            if w not in index:
                visit(w)
                low[v] = min(low[v], low[w])
            elif w in on_stack:
                low[v] = min(low[v], index[w])
        if low[v] == index[v]:
            component = []
            while True:
                w = stack.pop()
                on_stack.discard(w)
                component.append(w)
                if w == v:
                    break
            result.append(sorted(component))

    for v in sorted(nodes):
        if v not in index:
            visit(v)
    return result


def fold(graph, group_of, keep, max_nodes):
    """Folds every node outside `keep` into one node per group; then keeps the best-connected groups."""
    groups = defaultdict(list)
    for key in graph.nodes:
        if key not in keep:
            groups[group_of(key)].append(key)
    degree = defaultdict(int)
    for src, dst, *_ in graph.edges:
        degree[group_of(src)] += 1
        degree[group_of(dst)] += 1
    room = max_nodes - len(keep)
    ranked = sorted(groups, key=lambda g: (-degree[g], g))
    shown = ranked if len(ranked) <= room else ranked[: room - 1]
    rest = [g for g in ranked if g not in shown]
    out = Graph(graph.direction)
    for key in keep:
        out.nodes[key] = graph.nodes[key]
    for g in shown:
        out.node("group:" + g, f"{g} ({len(groups[g])})")
    if rest:
        out.node("group:\u2026", f"\u2026{len(rest)} more groups ({sum(len(groups[g]) for g in rest)})")
    mapping = {key: key for key in keep}
    for g, members in groups.items():
        for key in members:
            mapping[key] = "group:" + (g if g in shown else "\u2026")
    counts, dashed, single = defaultdict(int), defaultdict(lambda: True), defaultdict(list)
    for src, dst, label, style, _, weight in graph.edges:
        a, b = mapping[src], mapping[dst]
        if a != b or a in keep:
            counts[(a, b)] += weight
            single[(a, b)].append(label)
            dashed[(a, b)] = dashed[(a, b)] and style == "dashed"
    for (a, b), n in counts.items():
        label = single[(a, b)][0] if len(single[(a, b)]) == 1 and n == 1 else (f"\u00d7{n}" if n >= 2 else None)
        out.edge(a, b, label, "dashed" if dashed[(a, b)] else "solid", False, n)
    return out


# ------------------------------------------------------------------ module

def module_graph(kb, focus=None, max_nodes=60):
    counts = defaultdict(int)
    for row in kb.graph:
        if row["rel"] in ("calls", "uses_type", "implements", "instance_of"):
            a, b = kb.module_of(row["from"]), kb.module_of(row["to"])
            if a and b and a != b:
                counts[(a, b)] += 1
    using = set()
    for row in kb.graph:
        if row["rel"] == "imports":
            prefix = row["to"] + "::"
            for m in kb.modules:
                if m != row["from"] and m.startswith(prefix) and "::" not in m[len(prefix):] and (row["from"], m) not in counts:
                    using.add((row["from"], m))
    adjacency = defaultdict(set)
    for a, b in counts:
        adjacency[a].add(b)
    component = {}
    for members in strongly_connected(kb.modules, adjacency):
        for m in members:
            component[m] = (len(members), members[0])
    pairs = set(counts) | using
    nodes = set(kb.modules)
    if focus:
        nodes = {focus} | {b for a, b in pairs if a == focus} | {a for a, b in pairs if b == focus}
    degree = defaultdict(int)
    for a, b in pairs:
        degree[a] += 1
        degree[b] += 1
    folded = len(nodes) > max_nodes
    keep = set(sorted(nodes, key=lambda m: (-degree[m], m))[: max_nodes - 1]) if folded else nodes
    more = "\u2026more"
    graph = Graph("TD")
    for m in sorted(keep):
        cycle = component[m][0] >= 2
        graph.node(m, m, "box", "cycle" if cycle else ("dep" if kb.by_id[m]["origin"] != "project" else None))
    if folded:
        graph.node(more, f"\u2026{len(nodes) - len(keep)} more modules", "box", "dep")
    merged, style, red = defaultdict(int), {}, {}
    for (a, b) in sorted(pairs):
        if a in nodes and b in nodes:
            ka, kb_ = (a if a in keep else more), (b if b in keep else more)
            if ka != kb_:
                merged[(ka, kb_)] += counts.get((a, b), 0)
                style[(ka, kb_)] = "solid" if (a, b) in counts or style.get((ka, kb_)) == "solid" else "dashed"
                red[(ka, kb_)] = ka == a and kb_ == b and (a, b) in counts and component[a][0] >= 2 and component[a] == component[b]
    for (a, b), n in merged.items():
        label = f"\u00d7{n}" if n >= 2 else ("using" if style[(a, b)] == "dashed" else None)
        graph.edge(a, b, label, style[(a, b)], red[(a, b)], max(n, 1))
    return graph, folded


# ------------------------------------------------------------------ calls

def effect_class(kb, identifier):
    row = kb.by_id.get(identifier)
    if row is None:
        return "ext"
    facts = kb.facts.get(identifier)
    if facts is None:
        return None
    effects = set(facts["effects"])
    if effects & IO_EFFECTS:
        return "io"
    if effects & {"alloc", "copy"}:
        return "alloc"
    if effects & {"trap", "diverge"}:
        return "trap"
    return "pure" if (facts["effects_complete"] or row.get("pure")) else None


def node_shape(kb, identifier):
    row = kb.by_id.get(identifier)
    if row is None:
        return "stadium"
    if row["name"] == "$entry":
        return "subroutine"
    return {"method": "round", "test": "hex", "bench": "hex"}.get(row["kind"], "box")


def calls_graph(kb, root, depth=2, max_nodes=60):
    callees = defaultdict(list)
    for row in kb.graph:
        if row["rel"] == "calls":
            callees[row["from"]].append(row)
    for rows in callees.values():
        rows.sort(key=lambda r: (position(r), r["to"]))
    levels, frontier = {root: 0}, [root]
    for level in range(1, depth + 1):
        following = []
        for identifier in sorted(frontier):
            for row in callees[identifier]:
                if row["to"] not in levels:
                    levels[row["to"]] = level
                    following.append(row["to"])
        frontier = following
    graph = Graph("TD")
    grouped = defaultdict(list)
    for identifier in levels:
        for row in callees[identifier]:
            if row["to"] in levels:
                grouped[(identifier, row["to"])].append(row)
    for identifier, level in levels.items():
        truncated = level == depth and any(row["to"] not in levels for row in callees[identifier])
        graph.node(identifier, identifier + (" \u2026" if truncated else ""), node_shape(kb, identifier), effect_class(kb, identifier), identifier == root)
    for (a, b), rows in grouped.items():
        distinct = []
        for row in rows:
            text = ",".join(row.get("args", []))
            if text and text not in distinct:
                distinct.append(text)
        label = " / ".join(distinct)
        if len(rows) >= 2:
            label = (label + " " if label else "") + f"\u00d7{len(rows)}"
        graph.edge(a, b, label or None, "solid", False, len(rows))
    if len(levels) > max_nodes:
        def group_of(identifier):
            row = kb.by_id.get(identifier)
            if row:
                return row["module"]
            return "std:" + identifier.split(".")[0] if "." in identifier else "std:builtin"
        return fold(graph, group_of, {root}, max_nodes), True
    return graph, False


# ------------------------------------------------------------------ concurrency

CELL_EVIDENCE = [
    ("Channel.Sender<", "send", "Channel"),
    ("Channel.Receiver<", "recv", "Channel"),
    ("Mutex<", "lock", "Mutex"),
    ("Atomic<", "atomic", "Atomic"),
]
CREATORS = {"Channel.bounded": "Channel", "Mutex.create": "Mutex", "Atomic.create": "Atomic"}


def inner_type(text, opening):
    start = text.index(opening) + len(opening)
    depth = 1
    for index in range(start, len(text)):
        if text[index] == "<":
            depth += 1
        elif text[index] == ">":
            depth -= 1
            if depth == 0:
                return text[start:index]
    return text[start:]


def has_opening(text, opening):
    return re.search(r"(?<![A-Za-z0-9_])" + re.escape(opening), text) is not None


def concurrency_graph(kb, focus=None, max_nodes=60):
    evidence, region, known = defaultdict(set), defaultdict(set), defaultdict(set)
    for row in kb.symbols:
        if row["kind"] not in ("def", "method"):
            continue
        places = [(p["type"], "param") for p in row["params"]] + ([(row["result"], "result")] if row["result"] else [])
        for text, where in places:
            for opening, relation, kind in CELL_EVIDENCE:
                if has_opening(text, opening):
                    t = inner_type(text, opening)
                    known[kind].add(t)
                    evidence[row["id"]].add((relation if where == "param" else "create", kind, t))
    for row in kb.graph:
        if row["rel"] != "calls" or row["from"] not in kb.by_id:
            continue
        if row["to"] in REGION_CALLS or row["to"].startswith("Parallel."):
            region[row["from"]].add(row["to"])
        if row["to"] in CREATORS:
            kind = CREATORS[row["to"]]
            t = next(iter(known[kind])) if len(known[kind]) == 1 else "?"
            evidence[row["from"]].add(("create", kind, t))
    involved = set(evidence) | set(region)
    if focus:
        involved = {f for f in involved if kb.module_of(f) == focus}
    graph = Graph("LR")
    for f in involved:
        suffix = " \u00b7 " + ", ".join(sorted(region[f])) if f in region else ""
        graph.node(f, f + suffix, "subroutine" if f in region else "box", "region" if f in region else None)
    for f in involved:
        for relation, kind, t in evidence[f]:
            cell = f"cell:{kind}<{t}>"
            graph.node(cell, f"{kind}<{t}>", "round", "cell")
            if relation == "recv":
                graph.edge(cell, f, "recv")
            elif relation == "create":
                graph.edge(f, cell, "create", "dashed")
            else:
                graph.edge(f, cell, relation)
    for row in kb.graph:
        if row["rel"] == "calls" and row["from"] in region and row["to"] in involved and row["to"] != row["from"]:
            graph.edge(row["from"], row["to"], "calls", "dashed")
    graph.edges = sorted(set(graph.edges))
    folded = len(graph.nodes) + 1 > max_nodes
    if folded:
        graph = fold(graph, lambda key: key if key.startswith("cell:") else kb.module_of(key), {k for k in graph.nodes if k.startswith("cell:") or k in region}, max_nodes - 1)
    graph.node("note", "approximate: from signatures and call sites", "box", "note")
    return graph, folded


# ------------------------------------------------------------------ types (classDiagram)

def node_labels(nodes, builtin):
    """The name of each type node; the full id for names that two nodes share."""
    names = {k: (k if k in builtin else nodes[k]["name"]) for k in set(nodes) | builtin}
    return {k: (k if list(names.values()).count(name) > 1 else name) for k, name in names.items()}


def types_diagram(kb, focus=None, max_nodes=60):
    nodes = {r["id"]: r for r in kb.symbols if r["kind"] in ("record", "union", "class", "instance") and (focus is None or r["module"] == focus)}
    members = defaultdict(list)
    for r in kb.symbols:
        if r["kind"] in ("field", "case", "method") and not r["id"].startswith("instance:"):
            owner = r["id"].rsplit(".", 1)[0]
            if owner in nodes:
                members[owner].append(r)
    builtin, relations = set(), set()
    for row in kb.graph:
        a, b, rel = row["from"], row["to"], row["rel"]
        if rel == "instance_of" and a in nodes:
            if b not in nodes and b not in kb.by_id:
                builtin.add(b)
            relations.add((a, b, "..|>", None))
            head = a[len("instance:" + b) + 1:-1] if a.startswith("instance:" + b + "<") else None
            if head in nodes:
                relations.add((a, head, "..>", "for"))
        elif rel == "implements" and a in nodes and "instance:" + b + "<" + a + ">" not in nodes:
            if b not in nodes and b not in kb.by_id:
                builtin.add(b)
            relations.add((a, b, "..|>", "derive"))
        elif rel == "uses_type":
            source = kb.by_id.get(a)
            owner = a.rsplit(".", 1)[0]
            if source and source["kind"] in ("field", "case") and owner in nodes and b in nodes and owner != b:
                relations.add((owner, b, "-->", source["name"]))
    keys = sorted(set(nodes) | builtin)
    folded = len(keys) > max_nodes
    if folded:
        group_of = {k: ("builtin" if k in builtin else nodes[k]["module"]) for k in keys}
        counts = defaultdict(lambda: 0)
        sizes = defaultdict(int)
        for k in keys:
            sizes[group_of[k]] += 1
        for a, b, _, _ in relations:
            if group_of[a] != group_of[b]:
                counts[(group_of[a], group_of[b])] += 1
        groups = sorted(sizes)
        ids = {g: f"n{i}" for i, g in enumerate(groups, 1)}
        lines = ["classDiagram", "    direction LR"]
        for g in groups:
            lines += [f"    class {ids[g]}[\"{esc(g)} ({sizes[g]})\"] {{", f"        <<{'builtin' if g == 'builtin' else 'module'}>>", "    }"]
        for (a, b), n in sorted(counts.items(), key=lambda kv: (ids[kv[0][0]], ids[kv[0][1]])):
            lines.append(f"    {ids[a]} --> {ids[b]} : \u00d7{n}" if n >= 2 else f"    {ids[a]} --> {ids[b]}")
        return "\n".join(lines) + "\n", len(groups), True
    ids = {k: f"n{i}" for i, k in enumerate(keys, 1)}
    labels = node_labels(nodes, builtin)
    lines = ["classDiagram", "    direction LR"]
    for k in keys:
        if k in builtin:
            lines += [f"    class {ids[k]}[\"{esc(labels[k])}\"] {{", "        <<builtin>>", "    }"]
            continue
        r = nodes[k]
        lines += [f"    class {ids[k]}[\"{esc(labels[k])}\"] {{", f"        <<{r['kind']}>>"]
        ordered = sorted(members.get(k, []), key=position)
        for m in ordered[:MEMBER_LIMIT]:
            if m["kind"] == "field":
                lines.append(f"        +{member(m['name'])} {member(m['result'])}")
            elif m["kind"] == "case":
                lines.append(f"        +{member(m['name'])}" + (f" {member(m['result'])}" if m["result"] else ""))
            else:
                params = ", ".join(p["type"] for p in m["params"])
                lines.append(f"        +{member(m['name'])}({member(params)}) {member(m['result'] or 'unit')}")
        if len(ordered) > MEMBER_LIMIT:
            lines.append(f"        +\u2026 {len(ordered) - MEMBER_LIMIT} more")
        lines.append("    }")
    for a, b, arrow, label in sorted(relations, key=lambda r: (ids[r[0]], ids[r[1]], r[2], r[3] or "")):
        tail = f" : {esc(label, 30)}" if label else ""
        lines.append(f"    {ids[a]} {arrow} {ids[b]}{tail}")
    return "\n".join(lines) + "\n", len(keys), False


# ------------------------------------------------------------------ ownership (Markdown with one stateDiagram-v2 per variable)

def variable_diagram(events):
    """The stateDiagram-v2 of one variable; `events` are its non-copy events in source order."""
    lines = ["stateDiagram-v2", "    direction LR", '    state "Owned" as s0', "    [*] --> s0"]
    classes = {"moved": [], "borrowed": []}
    current, last, step = "Owned", "s0", 0

    def add(name, label):
        nonlocal current, last, step
        step += 1
        lines.extend([f'    state "{name}" as s{step}', f"    {last} --> s{step} : {esc(label)}"])
        if name in ("Moved", "Dropped"):
            classes["moved"].append(f"s{step}")
        elif name in ("Borrowed", "BorrowedMut"):
            classes["borrowed"].append(f"s{step}")
        current, last = name, f"s{step}"

    for e in events:
        kind, line = e["event"], e["span"]["line"]
        to = f" \u2192 {e['to']}" if e.get("to") else ""
        if current in ("Borrowed", "BorrowedMut") and kind in ("borrow", "borrow_mut", "move", "drop"):
            add("Owned", "release")
        if kind == "borrow":
            add("Borrowed", f"L{line} borrow{to}")
        elif kind == "borrow_mut":
            add("BorrowedMut", f"L{line} borrow_mut{to}")
        elif kind == "reborrow":
            add(current if current in ("Borrowed", "BorrowedMut") else "Borrowed", f"L{line} reborrow{to}")
        elif kind == "move":
            add("Moved", f"L{line} move{to}")
        else:
            add("Dropped", f"L{line} drop")
    lines.append(f"    {last} --> [*]")
    if classes["moved"]:
        lines.append("    classDef moved fill:#ffebe9,stroke:#cf222e")
    if classes["borrowed"]:
        lines.append("    classDef borrowed fill:#fff8c5,stroke:#9a6700")
    lines += [f"    class {','.join(classes[name])} {name}" for name in ("moved", "borrowed") if classes[name]]
    return "\n".join(lines) + "\n"


def ownership_markdown(kb, function):
    row = kb.by_id[function]
    events = sorted((e for e in kb.facts[function]["ownership"] if not e["var"].startswith("$")), key=lambda e: (position(e), e["var"]))
    variables = []
    for e in events:
        if e["var"] not in variables:
            variables.append(e["var"])
    drawn = [v for v in variables if any(e["event"] != "copy" for e in events if e["var"] == v)]
    if len(drawn) > VARIABLE_LIMIT:
        drawn = [v for v in drawn if any(e["event"] in ("move", "borrow", "borrow_mut") for e in events if e["var"] == v)]
    omitted = len(variables) - len(drawn[:VARIABLE_LIMIT])
    drawn = drawn[:VARIABLE_LIMIT]
    param_types = {p["name"]: p["type"] for p in row["params"]}
    parts, diagrams = [f"# Ownership of {function}\n"], []
    for v in drawn:
        diagram = variable_diagram([e for e in events if e["var"] == v and e["event"] != "copy"])
        diagrams.append(diagram)
        heading = f"## {v} (parameter: {param_types[v]})" if v in param_types else f"## {v}"
        parts.append(f"{heading}\n\n```mermaid\n{diagram}```\n")
    if omitted:
        parts.append(f"{omitted} variables are not drawn; the table lists every event.\n")
    if not events:
        parts.append("(no ownership events)\n")
    role = {"move": "moved_here", "borrow": "borrowed_here", "borrow_mut": "borrowed_here"}
    table = ["| # | line | var | event | to | diagnostic role |", "| --- | --- | --- | --- | --- | --- |"]
    for number, e in enumerate(events[:LEDGER_LIMIT], 1):
        table.append(f"| {number} | {e['span']['line']} | {e['var']} | {e['event']} | {e.get('to') or '-'} | {role.get(e['event'], '-')} |")
    parts.append("\n".join(table) + "\n")
    return "\n".join(parts), diagrams, len(drawn)


# ------------------------------------------------------------------ layout (Markdown)

PRIMITIVES = {"i8": (1, 1), "u8": (1, 1), "bool": (1, 1), "i16": (2, 2), "u16": (2, 2), "i32": (4, 4), "u32": (4, 4), "f32": (4, 4), "i64": (8, 8), "u64": (8, 8), "f64": (8, 8)}
WASM_NOTE = "Native only. On wasm32 a pointer is 4 bytes and the `null_pointer` niche does not exist, so these numbers are not valid there for any type that holds a pointer."


def bound(pair):
    return str(pair[0]) if pair[0] == pair[1] else f"{pair[0]}..{pair[1]}"


def layout_markdown(kb, identifier):
    row = next(r for r in kb.layouts if r["id"] == identifier)
    lines = [f"# Layout of {identifier} (native, 64-bit)", "", "| item | value |", "| --- | --- |", f"| kind | {row['kind']} |", f"| size (bytes) | {bound(row['size'])} |", f"| align (bytes) | {bound(row['align'])} |"]
    if row["kind"] == "record":
        lines.append(f"| reordered | {'yes' if row['reordered'] else 'no'} |")
    else:
        lines += [f"| layout | {row['layout']} |", f"| tag bytes | {'-' if row['tag_bytes'] is None else row['tag_bytes']} |", f"| niche | {row['niche'] or 'none'} |"]
    lines += [f"| drop flag | {'yes' if row['drop_flag'] else 'no'} |", ""]
    if row["kind"] == "record":
        lines += ["| slot | field | type | size | align | offset |", "| --- | --- | --- | --- | --- | --- |"]
        offset, known = 0, True
        for f in sorted(row["fields"], key=lambda f: f["slot"]):
            primitive = PRIMITIVES.get(f["type"])
            if primitive and known:
                start = -(-offset // primitive[1]) * primitive[1]
                lines.append(f"| {f['slot']} | {f['name']} | {f['type']} | {primitive[0]} | {primitive[1]} | {start} |")
                offset = start + primitive[0]
            else:
                known = False
                lines.append(f"| {f['slot']} | {f['name']} | {f['type']} | ? | ? | ? |")
        if known and row["size"][0] == row["size"][1]:
            lines.append(f"| - | (tail padding) | - | {row['size'][0] - offset} | - | {offset} |")
    else:
        lines += ["| case | payload |", "| --- | --- |"] + [f"| {c['name']} | {c['payload'] or '-'} |" for c in row["cases"]]
    return "\n".join(lines + ["", WASM_NOTE, ""])


# ------------------------------------------------------------------ transitions (stateDiagram-v2)

def transitions_diagram(kb, union, edges):
    cases = sorted((r for r in kb.symbols if r["kind"] == "case" and r["id"].rsplit(".", 1)[0] == union), key=position)
    order = [c["name"] for c in cases]
    ids = {name: f"s{i}" for i, name in enumerate(order, 1)}
    lines = ["stateDiagram-v2", "    direction LR"]
    for c in cases:
        lines.append(f"    state \"{esc(c['name'] + (' of ' + c['result'] if c['result'] else ''))}\" as {ids[c['name']]}")
    merged = defaultdict(set)
    for source, target, function in edges:
        merged[(source, target)].add(function)
    for (source, target) in sorted(merged, key=lambda k: (order.index(k[0]), order.index(k[1]))):
        lines.append(f"    {ids[source]} --> {ids[target]} : {esc(', '.join(sorted(merged[(source, target)])), EDGE_LABEL_LIMIT)}")
    return "\n".join(lines) + "\n", len(cases)


# ------------------------------------------------------------------ views

def views(kb):
    files = {}
    if len(kb.modules) >= 2:
        files["views/modules.mmd"] = flowchart(module_graph(kb)[0])
    text, count, _ = types_diagram(kb)
    if count:
        files["views/types.mmd"] = text
    graph, _ = concurrency_graph(kb)
    if len(graph.nodes) > 1:
        files["views/concurrency.mmd"] = flowchart(graph)
    return files


# ------------------------------------------------------------------ DOT for types and transitions

DOT_ARROWS = {"..|>": "arrowhead=empty, style=dashed", "..>": "arrowhead=open, style=dashed", "-->": "arrowhead=vee"}


def types_dot(kb, focus=None):
    """The same nodes and relations as types_diagram (unfolded), written as DOT."""
    nodes = {r["id"]: r for r in kb.symbols if r["kind"] in ("record", "union", "class", "instance") and (focus is None or r["module"] == focus)}
    members = defaultdict(list)
    for r in kb.symbols:
        if r["kind"] in ("field", "case", "method") and not r["id"].startswith("instance:"):
            owner = r["id"].rsplit(".", 1)[0]
            if owner in nodes:
                members[owner].append(r)
    builtin, relations = set(), set()
    for row in kb.graph:
        a, b, rel = row["from"], row["to"], row["rel"]
        if rel == "instance_of" and a in nodes:
            if b not in nodes and b not in kb.by_id:
                builtin.add(b)
            relations.add((a, b, "..|>", None))
            head = a[len("instance:" + b) + 1:-1] if a.startswith("instance:" + b + "<") else None
            if head in nodes:
                relations.add((a, head, "..>", "for"))
        elif rel == "implements" and a in nodes and "instance:" + b + "<" + a + ">" not in nodes:
            if b not in nodes and b not in kb.by_id:
                builtin.add(b)
            relations.add((a, b, "..|>", "derive"))
        elif rel == "uses_type":
            source = kb.by_id.get(a)
            owner = a.rsplit(".", 1)[0]
            if source and source["kind"] in ("field", "case") and owner in nodes and b in nodes and owner != b:
                relations.add((owner, b, "-->", source["name"]))
    keys = sorted(set(nodes) | builtin)
    ids = {k: f"n{i}" for i, k in enumerate(keys, 1)}
    labels = node_labels(nodes, builtin)
    lines = ["digraph tsuzuri {", "    rankdir=LR;", '    node [shape=box, fontname="monospace"];', '    edge [fontname="monospace"];']
    for k in keys:
        if k in builtin:
            lines.append(f'    {ids[k]} [label="<<builtin>>\\n{dot_esc(labels[k])}"];')
            continue
        r = nodes[k]
        body = f"<<{r['kind']}>>\\n{dot_esc(labels[k])}\\n"
        ordered = sorted(members.get(k, []), key=position)
        for m in ordered[:MEMBER_LIMIT]:
            if m["kind"] == "field":
                body += f"+{dot_esc(m['name'])} {dot_esc(m['result'])}\\l"
            elif m["kind"] == "case":
                body += f"+{dot_esc(m['name'])}" + (f" {dot_esc(m['result'])}" if m["result"] else "") + "\\l"
            else:
                params = ", ".join(p["type"] for p in m["params"])
                body += f"+{dot_esc(m['name'])}({dot_esc(params)}) {dot_esc(m['result'] or 'unit')}\\l"
        if len(ordered) > MEMBER_LIMIT:
            body += f"+\u2026 {len(ordered) - MEMBER_LIMIT} more\\l"
        lines.append(f'    {ids[k]} [label="{body}"];')
    for a, b, arrow, label in sorted(relations, key=lambda r: (ids[r[0]], ids[r[1]], r[2], r[3] or "")):
        attributes = DOT_ARROWS[arrow] + (f', label="{dot_esc(label, 30)}"' if label else "")
        lines.append(f"    {ids[a]} -> {ids[b]} [{attributes}];")
    lines.append("}")
    return "\n".join(lines) + "\n"


def transitions_dot(kb, union, edges):
    cases = sorted((r for r in kb.symbols if r["kind"] == "case" and r["id"].rsplit(".", 1)[0] == union), key=position)
    order = [c["name"] for c in cases]
    ids = {name: f"s{i}" for i, name in enumerate(order, 1)}
    lines = ["digraph tsuzuri {", "    rankdir=LR;", '    node [shape=ellipse, fontname="monospace"];', '    edge [fontname="monospace"];']
    for c in cases:
        lines.append(f'    {ids[c["name"]]} [label="{dot_esc(c["name"] + (" of " + c["result"] if c["result"] else ""))}"];')
    merged = defaultdict(set)
    for source, target, function in edges:
        merged[(source, target)].add(function)
    for (source, target) in sorted(merged, key=lambda k: (order.index(k[0]), order.index(k[1]))):
        lines.append(f'    {ids[source]} -> {ids[target]} [label="{dot_esc(", ".join(sorted(merged[(source, target)])), EDGE_LABEL_LIMIT)}"];')
    lines.append("}")
    return "\n".join(lines) + "\n"
````

**E.2 `validate_ref.py`**（構造検証器）:

```python
"""Reference structural validator for the Mermaid text that G30 generates (not a general Mermaid parser)."""
import re
import sys
from pathlib import Path

ID = r"(?:n\d+|s\d+|v\d+(?:_\d+)?)"
LABEL = r'"(?P<label>[^"]*)"'
CLASS_NAMES = "pure|io|alloc|trap|ext|cycle|dep|region|cell|note|moved|borrowed"
PROPS = r"[a-z-]+:[#0-9a-zA-Z .]+(?:,[a-z-]+:[#0-9a-zA-Z .]+)*"
MAX_CHARS = 45000
MAX_EDGES = 400
MAX_NODES = 500
RESERVED = {"end", "graph", "flowchart", "subgraph", "style", "class", "classdef", "click", "call", "linkstyle", "default", "direction", "state", "note", "classdiagram", "statediagram"}


def bad_label(label):
    if re.search(r'["<>&`\\]|[\x00-\x1f]', label):
        return "raw special character in label"
    for match in re.finditer(r"#", label):
        if not re.match(r"#(?:\d+|[a-z]+);", label[match.start():]):
            return "'#' that is not an entity code"
    return None


def validate(text):
    errors = []
    if len(text) > MAX_CHARS:
        errors.append("text longer than 45,000 characters")
    if not text.endswith("\n") or "\r" in text or "\t" in text or "\n\n" in text:
        errors.append("must end with one newline and have no CR, tab or blank line")
    lines = text.rstrip("\n").split("\n")
    header = lines[0]
    body = lines[1:]
    declared, edges, uses, styles = set(), 0, [], []

    def check_label(number, label):
        message = bad_label(label)
        if message:
            errors.append(f"line {number}: {message}")

    def check_id(number, identifier):
        if identifier.lower() in RESERVED or not re.fullmatch(ID, identifier):
            errors.append(f"line {number}: bad id {identifier}")

    if header in ("flowchart TD", "flowchart LR"):
        phase = 0  # 0 nodes, 1 edges, 2 classDef, 3 class, 4 style, 5 linkStyle
        node_re = re.compile(r'^    (n\d+)(?:\["(.*)"\]|\("(.*)"\)|\(\["(.*)"\]\)|\[\["(.*)"\]\]|\{\{"(.*)"\}\})$')
        edge_re = re.compile(r'^    (n\d+) (-->|-\.->)(?:\|"([^"]*)"\|)? (n\d+)$')
        class_defs, class_uses = set(), set()
        for number, line in enumerate(body, 2):
            m = node_re.match(line)
            if m:
                if phase != 0:
                    errors.append(f"line {number}: node after edge")
                check_id(number, m.group(1))
                if m.group(1) in declared:
                    errors.append(f"line {number}: duplicate node")
                declared.add(m.group(1))
                check_label(number, next(g for g in m.groups()[1:] if g is not None))
                continue
            m = edge_re.match(line)
            if m:
                phase = max(phase, 1)
                edges += 1
                uses += [(number, m.group(1)), (number, m.group(4))]
                if m.group(3) is not None:
                    check_label(number, m.group(3))
                continue
            m = re.match(rf"^    classDef ({CLASS_NAMES}) ({PROPS})$", line)
            if m:
                phase = max(phase, 2)
                class_defs.add(m.group(1))
                continue
            m = re.match(rf"^    class ({ID}(?:,{ID})*) ({CLASS_NAMES})$", line)
            if m:
                phase = max(phase, 3)
                uses += [(number, i) for i in m.group(1).split(",")]
                class_uses.add(m.group(2))
                continue
            m = re.match(r"^    style (n\d+) stroke-width:3px$", line)
            if m:
                uses.append((number, m.group(1)))
                continue
            m = re.match(r"^    linkStyle (\d+) stroke:#cf222e,stroke-width:2px$", line)
            if m:
                styles.append((number, int(m.group(1))))
                continue
            if re.match(r"^    %% omitted: \d+ edges$", line):
                continue
            errors.append(f"line {number}: unknown form: {line!r}")
        if class_defs != class_uses:
            errors.append("classDef and class do not match")
        if len(declared) > MAX_NODES or edges > MAX_EDGES:
            errors.append("too many nodes or edges")
    elif header == "classDiagram":
        if body[:1] != ["    direction LR"]:
            errors.append("the second line must be '    direction LR'")
        open_block = False
        for number, line in enumerate(body[1:], 3):
            m = re.match(r'^    class (n\d+)\["([^"]*)"\] \{$', line)
            if m:
                if open_block:
                    errors.append(f"line {number}: nested block")
                open_block = True
                check_id(number, m.group(1))
                declared.add(m.group(1))
                check_label(number, m.group(2))
                continue
            if open_block and re.match(r"^        <<(record|union|class|instance|builtin|module)>>$", line):
                continue
            if open_block and line.startswith("        +"):
                check_label(number, line[len("        +"):].replace("(", "").replace(")", ""))
                continue
            if line == "    }" and open_block:
                open_block = False
                continue
            m = re.match(r"^    (n\d+) (-->|\.\.\|>|\.\.>) (n\d+)(?: : ([^\"]*))?$", line)
            if m:
                uses += [(number, m.group(1)), (number, m.group(3))]
                if m.group(4):
                    check_label(number, m.group(4))
                continue
            errors.append(f"line {number}: unknown form: {line!r}")
        if open_block:
            errors.append("unclosed class block")
    elif header == "stateDiagram-v2":
        if body[:1] != ["    direction LR"]:
            errors.append("the second line must be '    direction LR'")
        depth = 0
        for number, line in enumerate(body[1:], 3):
            pad = "    " * (depth + 1)
            m = re.match(rf'^{pad}state "([^"]*)" as ({ID})( \{{)?$', line)
            if m:
                check_label(number, m.group(1))
                declared.add(m.group(2))
                if m.group(3):
                    depth += 1
                continue
            if depth and line == pad + "direction LR":
                continue
            m = re.match(rf"^{pad}(\[\*\]|{ID}) --> (\[\*\]|{ID})(?: : (.*))?$", line)
            if m:
                uses += [(number, i) for i in (m.group(1), m.group(2)) if i != "[*]"]
                if m.group(3):
                    check_label(number, m.group(3))
                continue
            if depth and line == "    " * depth + "}":
                depth -= 1
                continue
            m = re.match(rf"^    classDef (moved|borrowed) ({PROPS})$", line)
            if m:
                continue
            m = re.match(rf"^    class ({ID}(?:,{ID})*) (moved|borrowed)$", line)
            if m:
                uses += [(number, i) for i in m.group(1).split(",")]
                continue
            errors.append(f"line {number}: unknown form: {line!r}")
        if depth:
            errors.append("unclosed state block")
    else:
        errors.append(f"unsupported header {header!r}")
    for number, identifier in uses:
        if identifier not in declared:
            errors.append(f"line {number}: {identifier} is not declared")
    if header.startswith("flowchart"):
        for number, index in styles:
            if index >= edges:
                errors.append(f"line {number}: linkStyle index out of range")
    return errors


if __name__ == "__main__":
    failed = 0
    for name in sys.argv[1:]:
        problems = validate(Path(name).read_text(encoding="utf-8"))
        print(("OK   " if not problems else "FAIL ") + name, *problems[:3])
        failed += bool(problems)
    sys.exit(1 if failed else 0)
```

**E.3 `fixtures.py`**（fixture の KB の行）:

```python
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / "kb"


def sym(id, kind, module, name, signature, params=(), result=None, path="Main.tz", line=1, column=1, origin="project", pure=False, public=True, export=False):
    return {"id": id, "kind": kind, "module": module, "name": name, "signature": signature, "params": [{"name": n, "type": t, "conv": c} for n, t, c in params], "result": result, "public": public, "export": export, "pure": pure, "origin": origin, "span": {"path": path, "line": line, "column": column}}


def edge(rel, src, dst, path, line, column, args=None):
    row = {"rel": rel, "from": src, "to": dst}
    if args is not None:
        row["args"] = args
    row["span"] = {"path": path, "line": line, "column": column}
    return row


def fact(id, effects, complete=False, ownership=()):
    return {"id": id, "effects": list(effects), "effects_complete": complete, "ownership": list(ownership)}


def event(var, kind, line, column, to=None, path="Main.tz"):
    row = {"var": var, "event": kind}
    if to:
        row["to"] = to
    row["span"] = {"path": path, "line": line, "column": column}
    return row


def write(name, symbols, graph=(), facts=(), layouts=()):
    directory = ROOT / name
    directory.mkdir(parents=True, exist_ok=True)
    for file, rows in (("symbols", symbols), ("graph", graph), ("facts", facts), ("layout", layouts)):
        text = "".join(json.dumps(row, ensure_ascii=False, separators=(",", ":")) + "\n" for row in rows)
        (directory / f"{file}.jsonl").write_text(text, encoding="utf-8")


def module(id, path):
    return sym(id, "module", id, id.split("::")[-1], f"module {id}", path=path)


# point (examples/point; the rows of G26 appendix H1, reduced to the keys that G30 reads)
write(
    "point",
    [
        module("Main", "Main.tz"),
        sym("Main.$entry", "def", "Main", "$entry", "entry :: f64", result="f64", public=False),
        module("Point", "Point.tz"),
        sym("Point.Point", "record", "Point", "Point", "record Point", path="Point.tz", line=1, column=8),
        sym("Point.Point.x", "field", "Point", "x", "x: f64", result="f64", path="Point.tz", line=1, column=16),
        sym("Point.Point.y", "field", "Point", "y", "y: f64", result="f64", path="Point.tz", line=1, column=24),
        sym("Point.distance", "def", "Point", "distance", "def distance :: Point -> f64", [("point", "Point", "copy")], "f64", "Point.tz", 3, 5),
        sym("Point.hypotenuse", "def", "Point", "hypotenuse", "export def hypotenuse :: f64 -> f64 -> f64", [("x", "f64", "copy"), ("y", "f64", "copy")], "f64", "Point.tz", 6, 12, export=True),
    ],
    [
        edge("calls", "Main.$entry", "Point.distance", "Main.tz", 2, 9, ["copy"]),
        edge("calls", "Point.distance", "sqrt", "Point.tz", 4, 5, ["copy"]),
        edge("calls", "Point.hypotenuse", "Point.distance", "Point.tz", 7, 5, ["copy"]),
        edge("uses_type", "Main.$entry", "Point.Point", "Main.tz", 1, 9),
        edge("uses_type", "Point.distance", "Point.Point", "Point.tz", 3, 17),
        edge("uses_type", "Point.hypotenuse", "Point.Point", "Point.tz", 7, 15),
    ],
    [fact("Main.$entry", []), fact("Point.distance", []), fact("Point.hypotenuse", [])],
)

# cycle (A.tz, B.tz, Main.tz)
write(
    "cycle",
    [
        module("A", "A.tz"),
        sym("A.A", "record", "A", "A", "record A", path="A.tz", line=1, column=8),
        sym("A.A.n", "field", "A", "n", "n: i32", result="i32", path="A.tz", line=1, column=14),
        sym("A.a_of", "def", "A", "a_of", "def a_of :: B -> i32", [("b", "B", "copy")], "i32", "A.tz", 3, 5),
        module("B", "B.tz"),
        sym("B.B", "record", "B", "B", "record B", path="B.tz", line=1, column=8),
        sym("B.B.m", "field", "B", "m", "m: i32", result="i32", path="B.tz", line=1, column=14),
        sym("B.b_of", "def", "B", "b_of", "def b_of :: A -> i32", [("a", "A", "copy")], "i32", "B.tz", 3, 5),
        module("Main", "Main.tz"),
        sym("Main.$entry", "def", "Main", "$entry", "entry :: i32", result="i32", public=False),
    ],
    [
        edge("uses_type", "A.a_of", "B.B", "A.tz", 3, 13),
        edge("uses_type", "B.b_of", "A.A", "B.tz", 3, 13),
        edge("calls", "Main.$entry", "A.a_of", "Main.tz", 3, 2, ["copy"]),
        edge("calls", "Main.$entry", "B.b_of", "Main.tz", 3, 15, ["copy"]),
        edge("uses_type", "Main.$entry", "A.A", "Main.tz", 1, 9),
        edge("uses_type", "Main.$entry", "B.B", "Main.tz", 2, 9),
    ],
    [fact("A.a_of", []), fact("B.b_of", []), fact("Main.$entry", [])],
)

# shapes (Traits.tt, Shapes.tz, Main.tz)
write(
    "shapes",
    [
        module("Main", "Main.tz"),
        sym("Main.$entry", "def", "Main", "$entry", "entry :: i32", result="i32", public=False),
        module("Shapes", "Shapes.tz"),
        sym("Shapes.Point", "record", "Shapes", "Point", "record Point", path="Shapes.tz", line=1, column=8),
        sym("Shapes.Point.x", "field", "Shapes", "x", "x: f64", result="f64", path="Shapes.tz", line=1, column=16),
        sym("Shapes.Point.y", "field", "Shapes", "y", "y: f64", result="f64", path="Shapes.tz", line=1, column=24),
        sym("Shapes.Line", "record", "Shapes", "Line", "record Line", path="Shapes.tz", line=3, column=8),
        sym("Shapes.Line.a", "field", "Shapes", "a", "a: Point", result="Point", path="Shapes.tz", line=3, column=15),
        sym("Shapes.Line.b", "field", "Shapes", "b", "b: Point", result="Point", path="Shapes.tz", line=3, column=25),
        sym("Shapes.Shape", "union", "Shapes", "Shape", "union Shape", path="Shapes.tz", line=5, column=7),
        sym("Shapes.Shape.Dot", "case", "Shapes", "Dot", "Dot of Point", result="Point", path="Shapes.tz", line=6, column=7),
        sym("Shapes.Shape.Segment", "case", "Shapes", "Segment", "Segment of Line", result="Line", path="Shapes.tz", line=7, column=7),
        sym("Shapes.Shape.Empty", "case", "Shapes", "Empty", "Empty", path="Shapes.tz", line=8, column=7),
        sym("instance:Traits.Sized<Shapes.Point>", "instance", "Shapes", "Traits.Sized<Point>", "instance Traits.Sized<Point>", path="Shapes.tz", line=10, column=10),
        module("Traits", "Traits.tt"),
        sym("Traits.Sized", "class", "Traits", "Sized", "class Sized<'a>", path="Traits.tt", line=1, column=7),
        sym("Traits.Sized.size", "method", "Traits", "size", "def size :: ref 'a -> i64", [("arg1", "ref 'a", "ref")], "i64", "Traits.tt", 2, 9),
    ],
    [
        edge("uses_type", "Shapes.Line.a", "Shapes.Point", "Shapes.tz", 3, 18),
        edge("uses_type", "Shapes.Line.b", "Shapes.Point", "Shapes.tz", 3, 28),
        edge("uses_type", "Shapes.Shape.Dot", "Shapes.Point", "Shapes.tz", 6, 14),
        edge("uses_type", "Shapes.Shape.Segment", "Shapes.Line", "Shapes.tz", 7, 18),
        edge("uses_type", "instance:Traits.Sized<Shapes.Point>", "Shapes.Point", "Shapes.tz", 10, 23),
        edge("uses_type", "Main.$entry", "Shapes.Line", "Main.tz", 1, 12),
        edge("uses_type", "Main.$entry", "Shapes.Point", "Main.tz", 1, 29),
        edge("implements", "Shapes.Point", "Traits.Sized", "Shapes.tz", 10, 10),
        edge("implements", "Shapes.Point", "Eq", "Shapes.tz", 1, 43),
        edge("instance_of", "instance:Traits.Sized<Shapes.Point>", "Traits.Sized", "Shapes.tz", 10, 10),
    ],
)

# flow (Worker.tz, Main.tz)
write(
    "flow",
    [
        module("Main", "Main.tz"),
        sym("Main.$entry", "def", "Main", "$entry", "entry :: i64", result="i64", public=False),
        module("Worker", "Worker.tz"),
        sym("Worker.feed", "def", "Worker", "feed", "def feed :: ref Channel.Sender<i64> -> i64 -> i64", [("sender", "ref Channel.Sender<i64>", "ref"), ("n", "i64", "copy")], "i64", "Worker.tz", 1, 5),
        sym("Worker.drain", "def", "Worker", "drain", "def drain :: ref Channel.Receiver<i64> -> i64", [("receiver", "ref Channel.Receiver<i64>", "ref")], "i64", "Worker.tz", 7, 5),
        sym("Worker.bump", "def", "Worker", "bump", "def bump :: ref Mutex<i64> -> i64 -> i64", [("counter", "ref Mutex<i64>", "ref"), ("index", "i64", "copy")], "i64", "Worker.tz", 17, 5),
    ],
    [
        edge("calls", "Main.$entry", "Mutex.create", "Main.tz", 1, 15, ["copy"]),
        edge("calls", "Main.$entry", "Channel.bounded", "Main.tz", 2, 7, ["copy"]),
        edge("calls", "Main.$entry", "Task.scope", "Main.tz", 4, 17, ["ref", "copy", "copy"]),
        edge("calls", "Main.$entry", "Worker.feed", "Main.tz", 4, 63, ["ref", "copy"]),
        edge("calls", "Main.$entry", "Owned.drop", "Main.tz", 5, 5, ["own"]),
        edge("calls", "Main.$entry", "Task.scope", "Main.tz", 6, 17, ["ref", "copy", "copy"]),
        edge("calls", "Main.$entry", "Worker.bump", "Main.tz", 6, 64, ["ref", "copy"]),
        edge("calls", "Main.$entry", "Worker.drain", "Main.tz", 7, 5, ["ref"]),
        edge("calls", "Worker.feed", "Channel.send", "Worker.tz", 3, 11, ["ref", "copy"]),
        edge("calls", "Worker.drain", "Channel.recv", "Worker.tz", 12, 15, ["ref"]),
        edge("calls", "Worker.bump", "Mutex.with_lock", "Worker.tz", 19, 5, ["ref", "copy"]),
    ],
    [
        fact("Main.$entry", ["atomic", "task"]),
        fact("Worker.feed", ["atomic"]),
        fact("Worker.drain", ["atomic"]),
        fact("Worker.bump", ["atomic"]),
    ],
)

# own (Main.tz)
write(
    "own",
    [
        module("Main", "Main.tz"),
        sym("Main.$entry", "def", "Main", "$entry", "entry :: i64", result="i64", public=False, line=16),
        sym("Main.peek", "def", "Main", "peek", "def peek :: ref string -> i64", [("text", "ref string", "ref")], "i64", public=False, line=1, column=13),
        sym("Main.consume", "def", "Main", "consume", "def consume :: string -> i64", [("text", "string", "own")], "i64", public=False, line=4, column=13),
        sym("Main.demo", "def", "Main", "demo", "export def demo :: i64 -> i64", [("n", "i64", "copy")], "i64", line=7, column=12, export=True),
    ],
    [],
    [
        fact("Main.demo", [], False, [
            event("text", "borrow", 10, 22, "Main.peek"),
            event("text", "move", 11, 25, "Main.consume"),
            event("label", "borrow", 13, 23, "Main.peek"),
            event("label", "drop", 14, 29),
        ]),
    ],
)

# light (Light.tz, Main.tz)
write(
    "light",
    [
        module("Light", "Light.tz"),
        sym("Light.Light", "union", "Light", "Light", "union Light", path="Light.tz", line=1, column=7),
        sym("Light.Light.Red", "case", "Light", "Red", "Red", path="Light.tz", line=2, column=7),
        sym("Light.Light.Green", "case", "Light", "Green", "Green", path="Light.tz", line=3, column=7),
        sym("Light.Light.Yellow", "case", "Light", "Yellow", "Yellow of i64", result="i64", path="Light.tz", line=4, column=7),
        sym("Light.next", "def", "Light", "next", "def next :: Light -> Light", [("light", "Light", "copy")], "Light", "Light.tz", 6, 5),
        sym("Light.reset", "def", "Light", "reset", "def reset :: Light -> Light", [("light", "Light", "copy")], "Light", "Light.tz", 13, 5),
    ],
)

# layout (the rows of G26 appendix H6, reduced to the keys that G30 reads)
write(
    "layout",
    [],
    [],
    [],
    [
        {"id": "Acme::Kb::Shapes.Packed", "kind": "record", "type": "Shapes.Packed", "size": [16, 16], "align": [8, 8], "fields": [{"name": "a", "type": "i8", "slot": 1}, {"name": "n", "type": "i64", "slot": 0}, {"name": "b", "type": "i8", "slot": 2}], "reordered": True, "drop_flag": False},
        {"id": "Acme::Kb::Shapes.Point", "kind": "record", "type": "Shapes.Point", "size": [16, 16], "align": [8, 8], "fields": [{"name": "x", "type": "f64", "slot": 0}, {"name": "y", "type": "f64", "slot": 1}], "reordered": False, "drop_flag": False},
        {"id": "Acme::Kb::Shapes.Shape", "kind": "union", "type": "Shapes.Shape", "size": [16, 16], "align": [8, 8], "layout": "common", "tag_bytes": 1, "niche": None, "cases": [{"name": "Circle", "payload": "f64"}, {"name": "Empty", "payload": None}], "drop_flag": False},
    ],
)
print("fixtures written")
```

**E.4 `gen.py`**（全期待出力を作る）:

```python
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import diagram_ref as d  # noqa: E402

HERE = Path(__file__).resolve().parent
KB = HERE.parent / "kb"
OUT = HERE.parent / "out"
OUT.mkdir(exist_ok=True)


def save(name, text):
    (OUT / name).write_text(text, encoding="utf-8")
    print(f"{name}: {len(text)} chars")


point, cycle, shapes, flow, own, light, layout = (d.Kb(KB / n) for n in ("point", "cycle", "shapes", "flow", "own", "light", "layout"))

graph, _ = d.module_graph(point)
save("point.module.mmd", d.flowchart(graph))
save("point.module.dot", d.dot(graph))
graph, _ = d.module_graph(cycle)
save("cycle.module.mmd", d.flowchart(graph))
save("cycle.module.dot", d.dot(graph))

graph, _ = d.calls_graph(point, "Point.hypotenuse", 2)
save("point.calls.mmd", d.flowchart(graph))
save("point.calls.dot", d.dot(graph))
graph, folded = d.calls_graph(flow, "Main.$entry", 2)
save("flow.calls.mmd", d.flowchart(graph))
graph, folded = d.calls_graph(flow, "Main.$entry", 2, max_nodes=5)
print("calls folded:", folded)
save("flow.calls.folded.mmd", d.flowchart(graph))

text, count, folded = d.types_diagram(shapes)
print("types nodes", count, folded)
save("shapes.types.mmd", text)
save("shapes.types.dot", d.types_dot(shapes))
text, count, folded = d.types_diagram(shapes, max_nodes=5)
print("types folded nodes", count, folded)
save("shapes.types.folded.mmd", text)

graph, folded = d.concurrency_graph(flow)
save("flow.concurrency.mmd", d.flowchart(graph))
save("flow.concurrency.dot", d.dot(graph))
graph, folded = d.concurrency_graph(flow, max_nodes=6)
print("concurrency folded:", folded)
save("flow.concurrency.folded.mmd", d.flowchart(graph))

text, diagrams, drawn = d.ownership_markdown(own, "Main.demo")
save("own.ownership.md", text)
for number, diagram in enumerate(diagrams, 1):
    save(f"own.ownership.{number}.mmd", diagram)

save("layout.packed.md", d.layout_markdown(layout, "Acme::Kb::Shapes.Packed"))
save("layout.shape.md", d.layout_markdown(layout, "Acme::Kb::Shapes.Shape"))

edges = [("Red", "Green", "next"), ("Green", "Yellow", "next"), ("Yellow", "Yellow", "next"), ("Yellow", "Red", "next"), ("Red", "Red", "reset"), ("Green", "Red", "reset"), ("Yellow", "Red", "reset")]
text, count = d.transitions_diagram(light, "Light.Light", edges)
save("light.transitions.mmd", text)
save("light.transitions.dot", d.transitions_dot(light, "Light.Light", edges))

for name, kb in (("shapes", shapes), ("flow", flow)):
    for path, text in d.views(kb).items():
        save(f"{name}.{path.replace('/', '.')}", text)
```

## 付録 F: 検証の手順と測定

**F.1 環境**（2026-10-18。macOS）: Node 24（`npx --yes --package=node@24 node …`。Node 20.19.6 では `npm install` が `EBADENGINE` の警告を出したため、24 で実行した）、`npm install --no-audit --no-fund mermaid@12.1.0 jsdom @viz-js/viz`（**リポジトリの外の作業ディレクトリで**。リポジトリの `package.json` には足さない）、Graphviz 14.1.2（`dot -V`）。

**F.2 harness `check.mjs`**（`mermaid.parse` で構文を検査する。`--md` は Markdown の ```` ```mermaid ```` ブロックを全部検査）:

````javascript
import { JSDOM } from 'jsdom';
import fs from 'node:fs';

const dom = new JSDOM('<!DOCTYPE html><html><body></body></html>');
globalThis.window = dom.window;
globalThis.document = dom.window.document;
Object.defineProperty(globalThis, 'navigator', { value: dom.window.navigator, configurable: true });

const mermaid = (await import('mermaid')).default;
mermaid.initialize({ startOnLoad: false, securityLevel: 'strict' });

// Usage: node check.mjs file.mmd ...   or   node check.mjs --md file.md (checks every ```mermaid block)
const args = process.argv.slice(2);
let failures = 0;
async function check(label, text) {
  try {
    const result = await mermaid.parse(text);
    console.log(`OK   ${label} (${result.diagramType})`);
  } catch (error) {
    failures += 1;
    console.log(`FAIL ${label}: ${String(error.message ?? error).split('\n').slice(0, 4).join(' | ')}`);
  }
}
if (args[0] === '--md') {
  for (const file of args.slice(1)) {
    const source = fs.readFileSync(file, 'utf8');
    const blocks = [...source.matchAll(/```mermaid[^\n]*\n([\s\S]*?)```/g)];
    let index = 0;
    for (const block of blocks) {
      index += 1;
      await check(`${file}#${index}`, block[1]);
    }
  }
} else {
  for (const file of args) {
    await check(file, fs.readFileSync(file, 'utf8'));
  }
}
console.log(`mermaid ${mermaid.version?.() ?? ''} failures=${failures}`);
process.exit(failures === 0 ? 0 : 1);
````

**F.3 実行と結果**（本チケットの作業で実行したもの）:

```text
node check.mjs g30/out/*.mmd            → OK 15 件（flowchart-v2 11・classDiagram 3・stateDiagram 3 のうち … 全部 OK）、failures=0
node check.mjs --md g30/out/*.md        → own.ownership.md#1・#2 が OK（stateDiagram）、failures=0
node check.mjs g30/neg/*.mmd            → 失敗 4（end-id・raw-quote・unquoted-parens・bad-arrow）、通る 3（class-undeclared・raw-angle・undeclared-node）
node check.mjs --md <文書の 73 ファイル> → 76 ブロック中 1 件の失敗（gpu.md#2。付録 A.5）
for f in g30/out/*.dot; do dot -Tplain "$f" > /dev/null; done   → 6 件すべて成功
python3 validate_ref.py ../out/*.mmd    → 全部 OK
python3 validate_ref.py ../neg/*.mmd    → 7 件すべて FAIL（付録 A.4 の表のとおり）
```

**F.4 出力の大きさ**（`gen.py` の表示。文字数）: `point.module.mmd` 64、`cycle.module.mmd` 285、`point.calls.mmd` 239、`flow.calls.mmd` 734、`flow.calls.folded.mmd` 329 ほか、`shapes.types.mmd` 626、`shapes.types.folded.mmd` 221、`flow.concurrency.mmd` 約 650、`own.ownership.md` 約 1,150、`layout.packed.md` 557、`light.transitions.mmd` 246。いちばん大きい出力でも 1.2 KB で、上限 45,000 文字に遠く及ばない（上限の確認は合成入力のテストで行う）。

**F.5 Rust の実装を検証するとき**: 手順 20 で、`kb diagram` を各 fixture に実行した出力を、`tests/golden/diagram/` のファイルと `diff` し、全 `.mmd` を F.2 の harness に通す。`mmdc` を使う環境では `mmdc -i <file>.mmd -o <出力>.svg` も通る（`mmdc` は puppeteer を使うので、この作業では実行していない）。
