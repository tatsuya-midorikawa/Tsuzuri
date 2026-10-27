//! The brief cards of a knowledge graph (G27 §7, D-56, D-60): small, deterministic English
//! summaries of the project (L0, 2,400 characters), a module (L1, 1,600) and a node (L2, 700).
//! A card is blocks of (line, priority); a line of priority 0 always stays, and while the card is
//! too long the line with the largest (priority, block, position) gives way to `…N more`. The
//! rules are those of the reference implementation in G27's appendix F-1.

use std::collections::{BTreeMap, BTreeSet};

use super::store::{KgData, Node};

pub const ELLIPSIS: char = '\u{2026}';
pub const L0_LIMIT: usize = 2400;
pub const L1_LIMIT: usize = 1600;
pub const L2_LIMIT: usize = 700;
/// The most L2 cards that a graph keeps as files; the others are made on request.
pub const L2_FILES: usize = 256;

const TOP_KINDS: [&str; 9] = [
    "record", "union", "class", "alias", "handle", "const", "def", "extern", "instance",
];
const TYPE_KINDS: [&str; 5] = ["record", "union", "class", "alias", "handle"];
const FILE_KINDS: [&str; 9] = [
    "def", "const", "record", "union", "class", "method", "alias", "extern", "handle",
];
const DEPENDENCY_RELATIONS: [&str; 5] =
    ["calls", "uses_type", "implements", "instance_of", "imports"];
/// The last line of L0: where to read on.
pub const NEXT: &str = "next: brief/L1/<key>.md, brief/L2/<id>.md, nodes.jsonl, edges.jsonl, facts/<key>.jsonl ('::' is '.' in file names)";

fn member_kind(kind: &str) -> Option<&'static str> {
    match kind {
        "record" => Some("field"),
        "union" => Some("case"),
        "class" => Some("method"),
        _ => None,
    }
}

pub fn chars(text: &str) -> usize {
    text.chars().count()
}

/// Whitespace runs as one space, without leading or trailing whitespace.
pub fn norm(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `text`, or its first `limit - 1` characters and `…` when it is longer than `limit`.
pub fn clip(text: &str, limit: usize) -> String {
    if chars(text) <= limit {
        text.to_owned()
    } else {
        let mut clipped: String = text.chars().take(limit.saturating_sub(1)).collect();
        clipped.push(ELLIPSIS);
        clipped
    }
}

pub fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

fn after_colons(signature: &str) -> &str {
    match signature.find(" :: ") {
        Some(at) => &signature[at + 4..],
        None => signature,
    }
}

/// The first non-empty line of a description, normalized and clipped to 100 characters.
pub fn first_line(doc: Option<&str>) -> String {
    doc.unwrap_or_default()
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| clip(&norm(line), 100))
        .unwrap_or_default()
}

/// A block of a card: its header, and its lines with their priorities.
type Block = (Option<String>, Vec<(String, usize)>);

/// A card: blocks of lines with priorities, rendered within a limit of characters.
pub struct Card {
    limit: usize,
    gap: &'static str,
    blocks: Vec<Block>,
}

impl Card {
    pub fn new(limit: usize, gap: &'static str) -> Self {
        Self {
            limit,
            gap,
            blocks: Vec::new(),
        }
    }

    /// Adds a block. In a `spread` block, line `i` of base priority `p > 0` has priority
    /// `p + i / 5`, so a long list keeps its head.
    pub fn block(&mut self, header: Option<&str>, lines: Vec<(String, usize)>, spread: bool) {
        let lines = if spread {
            lines
                .into_iter()
                .enumerate()
                .map(|(index, (text, priority))| {
                    let priority = if priority == 0 {
                        0
                    } else {
                        priority + index / 5
                    };
                    (text, priority)
                })
                .collect()
        } else {
            lines
        };
        self.blocks.push((header.map(str::to_owned), lines));
    }

    fn layout(&self, dropped: &BTreeSet<(usize, usize)>) -> String {
        let mut parts = Vec::new();
        for (block, (header, lines)) in self.blocks.iter().enumerate() {
            if lines.is_empty() {
                continue;
            }
            let mut part: Vec<String> = header.iter().cloned().collect();
            part.extend(
                lines
                    .iter()
                    .enumerate()
                    .filter(|(line, _)| !dropped.contains(&(block, *line)))
                    .map(|(_, (text, _))| text.clone()),
            );
            let count = (0..lines.len())
                .filter(|line| dropped.contains(&(block, *line)))
                .count();
            if count > 0 {
                part.push(format!("{ELLIPSIS}{count} more"));
            }
            parts.push(part.join("\n"));
        }
        parts.join(self.gap) + "\n"
    }

    pub fn render(&self) -> String {
        // The length of the layout, kept up to date as lines are dropped: per block, the kept
        // characters, the kept lines and the dropped lines.
        let mut kept: Vec<(usize, usize, usize)> = self
            .blocks
            .iter()
            .map(|(_, lines)| {
                (
                    lines.iter().map(|(text, _)| chars(text)).sum(),
                    lines.len(),
                    0,
                )
            })
            .collect();
        let length = |kept: &[(usize, usize, usize)]| {
            let mut total = 0;
            let mut parts = 0usize;
            for ((header, lines), (characters, count, dropped)) in self.blocks.iter().zip(kept) {
                if lines.is_empty() {
                    continue;
                }
                parts += 1;
                let mut items = *count;
                total += characters;
                if let Some(header) = header {
                    total += chars(header);
                    items += 1;
                }
                if *dropped > 0 {
                    total += chars(&format!("{ELLIPSIS}{dropped} more"));
                    items += 1;
                }
                total += items.saturating_sub(1);
            }
            total + chars(self.gap) * parts.saturating_sub(1) + 1
        };
        let mut candidates: BTreeSet<(usize, usize, usize)> = self
            .blocks
            .iter()
            .enumerate()
            .flat_map(|(block, (_, lines))| {
                lines
                    .iter()
                    .enumerate()
                    .filter(|(_, (_, priority))| *priority > 0)
                    .map(move |(line, (_, priority))| (*priority, block, line))
            })
            .collect();
        let mut dropped = BTreeSet::new();
        while length(&kept) > self.limit {
            let Some((_, block, line)) = candidates.pop_last() else {
                let text = self.layout(&dropped);
                return clip(&text, self.limit);
            };
            dropped.insert((block, line));
            let entry = &mut kept[block];
            entry.0 -= chars(&self.blocks[block].1[line].0);
            entry.1 -= 1;
            entry.2 += 1;
        }
        self.layout(&dropped)
    }
}

/// The view of a graph that the cards read, with what several cards of one graph share: each
/// module's rows and the module dependencies.
struct Kg<'a> {
    data: &'a KgData,
    rows: std::cell::OnceCell<BTreeMap<&'a str, Vec<&'a Node>>>,
    dependencies: std::cell::OnceCell<BTreeMap<&'a str, BTreeSet<&'a str>>>,
}

impl<'a> Kg<'a> {
    fn new(data: &'a KgData) -> Self {
        Self {
            data,
            rows: std::cell::OnceCell::new(),
            dependencies: std::cell::OnceCell::new(),
        }
    }

    fn modules(&self) -> Vec<&'a Node> {
        self.data
            .nodes
            .iter()
            .filter(|node| node.kind == "module")
            .collect()
    }

    fn rows_of(&self, module: &str) -> Vec<&'a Node> {
        let data = self.data;
        self.rows
            .get_or_init(|| {
                let mut rows: BTreeMap<&str, Vec<&Node>> = BTreeMap::new();
                for node in &data.nodes {
                    if node.kind != "module" && !node.name.starts_with('$') {
                        rows.entry(node.module.as_str()).or_default().push(node);
                    }
                }
                rows
            })
            .get(module)
            .cloned()
            .unwrap_or_default()
    }

    fn public_top(&self, module: &str) -> Vec<&'a Node> {
        self.rows_of(module)
            .into_iter()
            .filter(|node| node.public && TOP_KINDS.contains(&node.kind.as_str()))
            .collect()
    }

    fn has_graph(&self) -> bool {
        self.data.edges.is_some()
    }

    /// Module → the modules that its declarations depend on through the dependency relations.
    fn dependencies(&self) -> BTreeMap<&'a str, BTreeSet<&'a str>> {
        self.dependencies
            .get_or_init(|| self.compute_dependencies())
            .clone()
    }

    fn compute_dependencies(&self) -> BTreeMap<&'a str, BTreeSet<&'a str>> {
        let mut result: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for edge in self.data.edges() {
            if !DEPENDENCY_RELATIONS.contains(&edge.rel.as_str()) {
                continue;
            }
            let (Some(from), Some(to)) = (self.data.node(&edge.from), self.data.node(&edge.to))
            else {
                continue;
            };
            if from.module != to.module {
                result
                    .entry(from.module.as_str())
                    .or_default()
                    .insert(to.module.as_str());
            }
        }
        result
    }

    /// The other ends of the edges of `rel` out of (`outgoing`) or into `id`, unique and sorted.
    fn ends(&self, rel: &str, id: &str, outgoing: bool) -> Vec<&'a str> {
        let edges = if outgoing {
            self.data.edges_from(rel, id)
        } else {
            self.data.edges_to(rel, id)
        };
        let ends: BTreeSet<&str> = edges
            .into_iter()
            .map(|edge| {
                if outgoing {
                    edge.to.as_str()
                } else {
                    edge.from.as_str()
                }
            })
            .collect();
        ends.into_iter().collect()
    }
}

fn declaration_order(nodes: &mut [&Node]) {
    nodes.sort_by(|a, b| (a.span.start, &a.id).cmp(&(b.span.start, &b.id)));
}

fn module_summary(kg: &Kg<'_>, module: &str) -> String {
    let mut rows = kg.public_top(module);
    declaration_order(&mut rows);
    let groups: Vec<String> = TOP_KINDS
        .iter()
        .filter_map(|kind| {
            let names: Vec<String> = rows
                .iter()
                .filter(|node| node.kind == *kind)
                .map(|node| clip(&node.name, 32))
                .collect();
            (!names.is_empty()).then(|| format!("{kind} {}", names.join(", ")))
        })
        .collect();
    if groups.is_empty() {
        "(no public items)".into()
    } else {
        clip(&groups.join("; "), 100)
    }
}

/// L0: the project.
pub fn l0(data: &KgData, limit: usize) -> String {
    l0_of(&Kg::new(data), limit)
}

fn l0_of(kg: &Kg<'_>, limit: usize) -> String {
    let data = kg.data;
    let mut card = Card::new(limit, "\n\n");
    let modules = kg.modules();
    let tests: Vec<&Node> = data
        .nodes
        .iter()
        .filter(|node| node.kind == "test")
        .collect();
    let benches = data
        .nodes
        .iter()
        .filter(|node| node.kind == "bench")
        .count();
    let mut title = format!("# {}", clip(&data.meta.project, 64));
    if let Some(version) = &data.meta.version {
        title.push_str(&format!(" {}", clip(version, 32)));
    }
    let mut stats = format!(
        "tz-kg/1 | edition {} | modules {} | nodes {}",
        data.meta.edition,
        modules.len(),
        data.nodes.len()
    );
    if let Some(edges) = &data.edges {
        stats.push_str(&format!(" | edges {}", edges.len()));
    }
    stats.push_str(&format!(" | tests {}", tests.len()));
    if benches > 0 {
        stats.push_str(&format!(" | benches {benches}"));
    }
    let mut status = "status: ok".to_owned();
    if data.meta.warnings > 0 {
        status.push_str(&format!(
            ", {}",
            plural(data.meta.warnings, "warning", "warnings")
        ));
    }
    let main = data.nodes.iter().find(|node| {
        node.kind == "def"
            && node.name == "main"
            && data
                .node(&node.module)
                .is_some_and(|module| module.name == "Main")
    });
    let top_level = data.nodes.iter().find(|node| node.name == "$entry");
    let entry = match (main, top_level) {
        (Some(main), _) => format!("entry: {}", main.id),
        (None, Some(top_level)) => format!("entry: top-level code of {}", top_level.module),
        (None, None) => "entry: none (library)".into(),
    };
    card.block(
        None,
        vec![(title, 0), (stats, 0), (status, 0), (entry, 0)],
        false,
    );
    let mut ordered: Vec<(usize, &Node)> = modules
        .iter()
        .map(|module| (kg.public_top(&module.id).len(), *module))
        .collect();
    ordered
        .sort_by(|a, b| (std::cmp::Reverse(a.0), &a.1.id).cmp(&(std::cmp::Reverse(b.0), &b.1.id)));
    let ordered: Vec<&Node> = ordered.into_iter().map(|(_, module)| module).collect();
    card.block(
        Some("## Modules"),
        ordered
            .iter()
            .map(|module| {
                (
                    format!(
                        "- {} ({}): {}",
                        module.id,
                        clip(&module.span.path, 80),
                        module_summary(kg, &module.id)
                    ),
                    1,
                )
            })
            .collect(),
        true,
    );
    card.block(
        Some("## Exports"),
        data.nodes
            .iter()
            .filter(|node| node.export && matches!(node.kind.as_str(), "def" | "const"))
            .map(|node| {
                (
                    format!(
                        "- {} :: {}",
                        node.id,
                        clip(after_colons(&norm(&node.signature)), 120)
                    ),
                    2,
                )
            })
            .collect(),
        true,
    );
    if kg.has_graph() {
        card.block(
            Some("## Dependencies"),
            kg.dependencies()
                .into_iter()
                .map(|(module, uses)| {
                    (
                        clip(
                            &format!(
                                "- {module} -> {}",
                                uses.into_iter().collect::<Vec<_>>().join(", ")
                            ),
                            200,
                        ),
                        3,
                    )
                })
                .collect(),
            true,
        );
    }
    let mut per_module: BTreeMap<&str, usize> = BTreeMap::new();
    for test in &tests {
        *per_module.entry(test.module.as_str()).or_default() += 1;
    }
    let mut per_module: Vec<(&str, usize)> = per_module.into_iter().collect();
    per_module.sort_by(|a, b| (std::cmp::Reverse(a.1), a.0).cmp(&(std::cmp::Reverse(b.1), b.0)));
    card.block(
        Some("## Tests"),
        per_module
            .into_iter()
            .map(|(module, count)| (format!("- {module}: {count}"), 4))
            .collect(),
        true,
    );
    if let Some(facts) = &data.facts {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for node in &data.nodes {
            if matches!(node.kind.as_str(), "def" | "method")
                && let Some(fact) = facts.get(&node.id)
            {
                for effect in &fact.effects {
                    *counts.entry(effect.as_str()).or_default() += 1;
                }
            }
        }
        card.block(
            Some("## Effects (functions)"),
            counts
                .into_iter()
                .map(|(effect, count)| (format!("- {effect}: {count}"), 5))
                .collect(),
            true,
        );
    }
    card.block(None, vec![(NEXT.to_owned(), 0)], false);
    card.render()
}

/// L1: one module.
pub fn l1(data: &KgData, module: &str, limit: usize) -> Option<String> {
    l1_of(&Kg::new(data), module, limit)
}

fn l1_of(kg: &Kg<'_>, module: &str, limit: usize) -> Option<String> {
    let data = kg.data;
    let node = data.node(module)?;
    let mut card = Card::new(limit, "\n\n");
    let rows = kg.rows_of(module);
    let top: Vec<&Node> = rows
        .iter()
        .copied()
        .filter(|node| TOP_KINDS.contains(&node.kind.as_str()))
        .collect();
    let mut tests: Vec<&Node> = rows
        .iter()
        .copied()
        .filter(|node| matches!(node.kind.as_str(), "test" | "bench"))
        .collect();
    card.block(
        None,
        vec![
            (format!("# {module} ({})", clip(&node.span.path, 80)), 0),
            (
                format!(
                    "{}, {} public | {}",
                    plural(top.len(), "item", "items"),
                    top.iter().filter(|node| node.public).count(),
                    plural(
                        tests.iter().filter(|node| node.kind == "test").count(),
                        "test",
                        "tests"
                    )
                ),
                0,
            ),
        ],
        false,
    );
    let ordered = |filter: &dyn Fn(&Node) -> bool| {
        let mut nodes: Vec<&Node> = top.iter().copied().filter(|node| filter(node)).collect();
        nodes.sort_by(|a, b| {
            (!a.public, a.span.start, &a.id).cmp(&(!b.public, b.span.start, &b.id))
        });
        nodes
    };
    let priority = |node: &Node| if node.public { 1 } else { 4 };
    let text = |node: &Node| {
        let flag = if node.public { "" } else { " [private]" };
        if let Some(member) = member_kind(&node.kind) {
            let names: Vec<&str> = data
                .children(&node.id, member)
                .into_iter()
                .map(|child| child.name.as_str())
                .collect();
            return format!("{} {} {{{}}}{flag}", node.kind, node.name, names.join(", "));
        }
        let mut line = clip(&norm(&node.signature), 140) + flag;
        if kg.has_graph() {
            let callers = kg.ends("calls", &node.id, false).len();
            if callers > 0 {
                line.push_str(&format!(" | callers {callers}"));
            }
        }
        if let Some(fact) = data.fact(&node.id)
            && !fact.effects.is_empty()
        {
            line.push_str(&format!(" | effects {}", fact.effects.join(",")));
        }
        line
    };
    card.block(
        Some("## Types"),
        ordered(&|node| TYPE_KINDS.contains(&node.kind.as_str()))
            .into_iter()
            .map(|node| (format!("- {}", text(node)), priority(node)))
            .collect(),
        true,
    );
    card.block(
        Some("## Functions"),
        ordered(&|node| matches!(node.kind.as_str(), "const" | "def" | "extern"))
            .into_iter()
            .map(|node| (format!("- {}", text(node)), priority(node)))
            .collect(),
        true,
    );
    card.block(
        Some("## Instances"),
        ordered(&|node| node.kind == "instance")
            .into_iter()
            .map(|node| (format!("- instance {}", node.name), 2))
            .collect(),
        true,
    );
    declaration_order(&mut tests);
    card.block(
        Some("## Tests"),
        tests
            .iter()
            .map(|test| {
                (
                    format!(
                        "- {}{}",
                        if test.kind == "bench" { "bench " } else { "" },
                        test.name
                    ),
                    3,
                )
            })
            .collect(),
        true,
    );
    if kg.has_graph() {
        let dependencies = kg.dependencies();
        let uses: Vec<&str> = dependencies
            .get(module)
            .map(|uses| uses.iter().copied().collect())
            .unwrap_or_default();
        let used_by: Vec<&str> = dependencies
            .iter()
            .filter(|(_, uses)| uses.contains(module))
            .map(|(other, _)| *other)
            .collect();
        let mut lines = Vec::new();
        if !uses.is_empty() {
            lines.push((clip(&format!("uses: {}", uses.join(", ")), 200), 3));
        }
        if !used_by.is_empty() {
            lines.push((clip(&format!("used by: {}", used_by.join(", ")), 200), 3));
        }
        card.block(None, lines, false);
    }
    Some(card.render())
}

fn listing(label: &str, items: &[&str]) -> String {
    let mut shown = items.iter().take(5).copied().collect::<Vec<_>>().join(", ");
    if items.len() > 5 {
        shown.push_str(&format!(", {ELLIPSIS}{} more", items.len() - 5));
    }
    format!("{label} ({}): {shown}", items.len())
}

/// L2: one node.
pub fn l2(data: &KgData, id: &str, limit: usize) -> Option<String> {
    l2_of(&Kg::new(data), id, limit)
}

fn l2_of(kg: &Kg<'_>, id: &str, limit: usize) -> Option<String> {
    let data = kg.data;
    let node = data.node(id)?;
    let mut card = Card::new(limit, "\n\n");
    let mut flags = vec![if node.public { "public" } else { "private" }];
    if node.export {
        flags.push("export");
    }
    if node.pure {
        flags.push("pure");
    }
    let mut lines = vec![
        (node.id.clone(), 0),
        (
            format!(
                "{} | {} | {}:{}",
                node.kind,
                flags.join(" "),
                clip(&node.span.path, 80),
                node.span.line
            ),
            0,
        ),
        (format!("sig: {}", clip(&norm(&node.signature), 120)), 0),
    ];
    if matches!(node.kind.as_str(), "def" | "extern" | "method" | "const")
        && !node.params.is_empty()
    {
        lines.push((
            format!(
                "params: {}",
                node.params
                    .iter()
                    .map(|param| if param.name.is_empty() {
                        param.conv.clone()
                    } else {
                        format!("{}:{}", param.name, param.conv)
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            1,
        ));
    }
    if let Some(member) = member_kind(&node.kind) {
        let children = data.children(&node.id, member);
        if !children.is_empty() {
            let label = match node.kind.as_str() {
                "record" => "fields",
                "union" => "cases",
                _ => "methods",
            };
            let items: Vec<String> = children
                .iter()
                .map(|child| {
                    if node.kind == "class" {
                        child.name.clone()
                    } else {
                        norm(&child.signature)
                    }
                })
                .collect();
            lines.push((format!("{label}: {}", items.join(", ")), 1));
        }
    }
    let doc = first_line(node.doc.as_deref());
    if !doc.is_empty() {
        lines.push((format!("doc: {doc}"), 2));
    }
    if let Some(deprecated) = node.deprecated.as_deref().filter(|note| !note.is_empty()) {
        lines.push((format!("deprecated: {}", clip(&norm(deprecated), 100)), 1));
    }
    if let Some(fact) = data.fact(id) {
        if !fact.effects.is_empty() {
            lines.push((format!("effects: {}", fact.effects.join(", ")), 1));
        }
        let sites: Vec<String> = [
            ("trap", &fact.trap_sites),
            ("alloc", &fact.alloc_sites),
            ("copy", &fact.copy_sites),
        ]
        .into_iter()
        .filter_map(|(name, sites)| {
            let count = sites.iter().filter(|site| !site.proven).count();
            (count > 0).then(|| format!("{name} {count}"))
        })
        .collect();
        if !sites.is_empty() {
            lines.push((format!("sites: {}", sites.join(", ")), 2));
        }
        if fact.recursion != "none" {
            lines.push((format!("recursion: {}", fact.recursion), 2));
        }
    }
    if kg.has_graph() {
        let mut pairs: Vec<(&str, Vec<&str>)> = Vec::new();
        if TYPE_KINDS.contains(&node.kind.as_str()) {
            let used: BTreeSet<&str> = ["uses_type", "implements", "instance_of"]
                .iter()
                .flat_map(|rel| data.edges_to(rel, id))
                .map(|edge| edge.from.as_str())
                .collect();
            pairs.push(("used by", used.into_iter().collect()));
        } else {
            pairs.push(("callees", kg.ends("calls", id, true)));
            pairs.push(("callers", kg.ends("calls", id, false)));
            pairs.push(("tests", kg.ends("tested_by", id, true)));
        }
        for (label, items) in pairs {
            if !items.is_empty() {
                lines.push((listing(label, &items), 3));
            }
        }
    }
    card.block(None, lines, false);
    Some(card.render())
}

/// The IDs of the L2 cards that a graph keeps as files: exported, then public, then private
/// declarations of the file kinds, in byte order, at most `L2_FILES`.
pub fn l2_file_ids(data: &KgData) -> (Vec<String>, usize) {
    let mut rows: Vec<&Node> = data
        .nodes
        .iter()
        .filter(|node| FILE_KINDS.contains(&node.kind.as_str()) && !node.name.starts_with('$'))
        .collect();
    let rank = |node: &Node| {
        if node.export {
            0
        } else if node.public {
            1
        } else {
            2
        }
    };
    rows.sort_by(|a, b| (rank(a), &a.id).cmp(&(rank(b), &b.id)));
    let total = rows.len();
    let ids: Vec<String> = rows
        .into_iter()
        .take(L2_FILES)
        .map(|node| node.id.clone())
        .collect();
    let omitted = total - ids.len();
    (ids, omitted)
}

/// Adds the cards of the graph in `files` (whose manifest says `manifest`) to `files`: L0, an L1
/// for every module, and the L2 files. Returns (L2 cards written, L2 cards omitted).
pub fn add_cards(
    data: &KgData,
    modules: &[(String, String)],
    files: &mut super::write::Files,
) -> (u64, u64) {
    let kg = Kg::new(data);
    files.put("brief/L0.md", l0_of(&kg, L0_LIMIT).into_bytes());
    let keys: Vec<String> = modules.iter().map(|(_, key)| key.clone()).collect();
    let names = super::write::unique_names(&keys);
    for (id, key) in modules {
        if let Some(card) = l1_of(&kg, id, L1_LIMIT) {
            files.put(format!("brief/L1/{}.md", names[key]), card.into_bytes());
        }
    }
    let (ids, omitted) = l2_file_ids(data);
    let names = super::write::unique_names(&ids);
    let mut written = 0;
    for id in &ids {
        if let Some(card) = l2_of(&kg, id, L2_LIMIT) {
            files.put(format!("brief/L2/{}.md", names[id]), card.into_bytes());
            written += 1;
        }
    }
    (written, omitted as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_fit_drops_the_lowest_priority_first() {
        let mut card = Card::new(40, "\n\n");
        card.block(None, vec![("# title".into(), 0)], false);
        card.block(
            Some("## List"),
            (0..12)
                .map(|index| (format!("- item {index}"), 1))
                .collect(),
            true,
        );
        let text = card.render();
        assert!(chars(&text) <= 40, "{text}");
        assert!(text.starts_with("# title\n\n## List\n- item 0\n"), "{text}");
        assert!(text.contains(&format!("{ELLIPSIS}")), "{text}");
        // The rendering without drops is the layout.
        let mut small = Card::new(1000, "\n");
        small.block(Some("h"), vec![("a".into(), 1), ("b".into(), 1)], false);
        assert_eq!(small.render(), "h\na\nb\n");
    }

    #[test]
    fn a_card_without_droppable_lines_is_clipped() {
        let mut card = Card::new(10, "\n");
        card.block(None, vec![("0123456789abc".into(), 0)], false);
        let text = card.render();
        assert_eq!(chars(&text), 10);
        assert!(text.ends_with(ELLIPSIS));
    }

    #[test]
    fn helpers_follow_the_reference() {
        assert_eq!(norm("  a \n\t b  "), "a b");
        assert_eq!(clip("abcdef", 4), format!("abc{ELLIPSIS}"));
        assert_eq!(clip("abc", 3), "abc");
        assert_eq!(plural(1, "test", "tests"), "1 test");
        assert_eq!(plural(2, "test", "tests"), "2 tests");
        assert_eq!(after_colons("def f :: i64 -> i64"), "i64 -> i64");
        assert_eq!(first_line(Some("\n  first  line \nsecond")), "first line");
    }
}
