//! `types` (G30): a class diagram of the records, unions, classes and instances, with their
//! members, the classes they implement, and the types their fields and cases hold.

use std::collections::{BTreeMap, BTreeSet};

use super::escape::{dot_esc, esc, member};
use super::input::{Input, Symbol};
use super::{Limits, MAX_EDGES};

const MEMBER_LIMIT: usize = 12;

/// A relation: source, target, arrow, label.
type Relation = (String, String, &'static str, Option<String>);

struct Model<'a> {
    nodes: BTreeMap<&'a str, &'a Symbol>,
    members: BTreeMap<String, Vec<&'a Symbol>>,
    builtin: BTreeSet<String>,
    relations: BTreeSet<Relation>,
}

fn model<'a>(input: &'a Input, focus: Option<&str>) -> Model<'a> {
    let nodes: BTreeMap<&str, &Symbol> = input
        .symbols
        .iter()
        .filter(|symbol| {
            matches!(
                symbol.kind.as_str(),
                "record" | "union" | "class" | "instance"
            ) && focus.is_none_or(|focus| symbol.module == focus)
        })
        .map(|symbol| (symbol.id.as_str(), symbol))
        .collect();
    let mut members: BTreeMap<String, Vec<&Symbol>> = BTreeMap::new();
    for symbol in &input.symbols {
        if matches!(symbol.kind.as_str(), "field" | "case" | "method")
            && !symbol.id.starts_with("instance:")
        {
            let owner = input.parent(&symbol.id);
            if nodes.contains_key(owner.as_str()) {
                members.entry(owner).or_default().push(symbol);
            }
        }
    }
    let mut builtin = BTreeSet::new();
    let mut relations: BTreeSet<Relation> = BTreeSet::new();
    let unknown = |name: &str| !nodes.contains_key(name) && input.symbol(name).is_none();
    for edge in &input.edges {
        let (a, b) = (edge.from.as_str(), edge.to.as_str());
        match edge.rel.as_str() {
            "instance_of" if nodes.contains_key(a) => {
                if unknown(b) {
                    builtin.insert(b.to_owned());
                }
                relations.insert((a.to_owned(), b.to_owned(), "..|>", None));
                let head = a
                    .strip_prefix(&format!("instance:{b}<"))
                    .and_then(|rest| rest.get(..rest.len().saturating_sub(1)));
                if let Some(head) = head.filter(|head| nodes.contains_key(head)) {
                    relations.insert((a.to_owned(), head.to_owned(), "..>", Some("for".into())));
                }
            }
            "implements"
                if nodes.contains_key(a)
                    && !nodes.contains_key(format!("instance:{b}<{a}>").as_str()) =>
            {
                if unknown(b) {
                    builtin.insert(b.to_owned());
                }
                relations.insert((a.to_owned(), b.to_owned(), "..|>", Some("derive".into())));
            }
            "uses_type" => {
                let owner = input.parent(a);
                if let Some(source) = input.symbol(a)
                    && matches!(source.kind.as_str(), "field" | "case")
                    && nodes.contains_key(owner.as_str())
                    && nodes.contains_key(b)
                    && owner != b
                {
                    relations.insert((owner, b.to_owned(), "-->", Some(source.name.clone())));
                }
            }
            _ => {}
        }
    }
    // A relation to a type outside the diagram (another module's) has no node to point at.
    relations.retain(|(a, b, _, _)| {
        (nodes.contains_key(a.as_str()) || builtin.contains(a))
            && (nodes.contains_key(b.as_str()) || builtin.contains(b))
    });
    for list in members.values_mut() {
        list.sort_by_key(|symbol| (symbol.line, symbol.column));
    }
    Model {
        nodes,
        members,
        builtin,
        relations,
    }
}

impl Model<'_> {
    fn keys(&self) -> Vec<String> {
        let mut keys: BTreeSet<String> = self.nodes.keys().map(|key| (*key).to_owned()).collect();
        keys.extend(self.builtin.iter().cloned());
        keys.into_iter().collect()
    }

    /// The name of each type node; the full ID for names that two nodes share.
    fn labels(&self) -> BTreeMap<String, String> {
        let names: BTreeMap<String, String> = self
            .keys()
            .into_iter()
            .map(|key| {
                let name = if self.builtin.contains(&key) {
                    key.clone()
                } else {
                    self.nodes[key.as_str()].name.clone()
                };
                (key, name)
            })
            .collect();
        let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
        for name in names.values() {
            *seen.entry(name).or_default() += 1;
        }
        names
            .iter()
            .map(|(key, name)| {
                let label = if seen[name.as_str()] > 1 { key } else { name };
                (key.clone(), label.clone())
            })
            .collect()
    }
}

/// The class diagram, its number of classes, and whether it was folded.
pub fn mermaid(
    input: &Input,
    focus: Option<&str>,
    max_nodes: usize,
    limits: &Limits,
) -> (String, usize, bool) {
    let model = model(input, focus);
    let keys = model.keys();
    let mut lines = vec!["classDiagram".to_owned(), "    direction LR".to_owned()];
    if keys.len() > max_nodes {
        let group_of = |key: &str| -> String {
            if model.builtin.contains(key) {
                "builtin".to_owned()
            } else {
                model.nodes[key].module.clone()
            }
        };
        let mut sizes: BTreeMap<String, usize> = BTreeMap::new();
        for key in &keys {
            *sizes.entry(group_of(key)).or_default() += 1;
        }
        let ids: BTreeMap<&str, usize> = sizes
            .keys()
            .enumerate()
            .map(|(index, group)| (group.as_str(), index + 1))
            .collect();
        let mut counts: BTreeMap<(usize, usize), usize> = BTreeMap::new();
        for (a, b, _, _) in &model.relations {
            let (ga, gb) = (group_of(a), group_of(b));
            if ga != gb {
                *counts
                    .entry((ids[ga.as_str()], ids[gb.as_str()]))
                    .or_default() += 1;
            }
        }
        for (group, size) in &sizes {
            let annotation = if group == "builtin" {
                "builtin"
            } else {
                "module"
            };
            lines.push(format!(
                "    class n{}[\"{} ({size})\"] {{",
                ids[group.as_str()],
                esc(group, limits.label)
            ));
            lines.push(format!("        <<{annotation}>>"));
            lines.push("    }".to_owned());
        }
        push_relations(
            &mut lines,
            counts.into_iter().map(|((a, b), n)| {
                if n >= 2 {
                    format!("    n{a} --> n{b} : \u{d7}{n}")
                } else {
                    format!("    n{a} --> n{b}")
                }
            }),
        );
        return (lines.join("\n") + "\n", sizes.len(), true);
    }
    let ids: BTreeMap<&str, usize> = keys
        .iter()
        .enumerate()
        .map(|(index, key)| (key.as_str(), index + 1))
        .collect();
    let labels = model.labels();
    for key in &keys {
        lines.push(format!(
            "    class n{}[\"{}\"] {{",
            ids[key.as_str()],
            esc(&labels[key], limits.label)
        ));
        let Some(symbol) = model.nodes.get(key.as_str()) else {
            lines.push("        <<builtin>>".to_owned());
            lines.push("    }".to_owned());
            continue;
        };
        lines.push(format!("        <<{}>>", symbol.kind));
        let ordered = model.members.get(key).map_or(&[][..], Vec::as_slice);
        for item in ordered.iter().take(MEMBER_LIMIT) {
            let name = member(&item.name, limits.label);
            lines.push(match item.kind.as_str() {
                "field" => format!(
                    "        +{name} {}",
                    member(item.result.as_deref().unwrap_or(""), limits.label)
                ),
                "case" => match &item.result {
                    Some(payload) => format!("        +{name} {}", member(payload, limits.label)),
                    None => format!("        +{name}"),
                },
                _ => {
                    let params: Vec<&str> =
                        item.params.iter().map(|param| param.ty.as_str()).collect();
                    format!(
                        "        +{name}({}) {}",
                        member(&params.join(", "), limits.label),
                        member(item.result.as_deref().unwrap_or("unit"), limits.label)
                    )
                }
            });
        }
        if ordered.len() > MEMBER_LIMIT {
            lines.push(format!(
                "        +\u{2026} {} more",
                ordered.len() - MEMBER_LIMIT
            ));
        }
        lines.push("    }".to_owned());
    }
    let mut relations: Vec<&Relation> = model.relations.iter().collect();
    relations.sort_by_key(|(a, b, arrow, label)| {
        (
            ids[a.as_str()],
            ids[b.as_str()],
            *arrow,
            label.as_deref().unwrap_or(""),
        )
    });
    push_relations(
        &mut lines,
        relations.into_iter().map(|(a, b, arrow, label)| {
            let tail = match label {
                Some(label) => format!(" : {}", esc(label, limits.relation)),
                None => String::new(),
            };
            format!(
                "    n{} {arrow} n{}{tail}",
                ids[a.as_str()],
                ids[b.as_str()]
            )
        }),
    );
    (lines.join("\n") + "\n", keys.len(), false)
}

/// The relation lines, at most `MAX_EDGES` of them.
fn push_relations(lines: &mut Vec<String>, relations: impl Iterator<Item = String>) {
    let all: Vec<String> = relations.collect();
    let omitted = all.len().saturating_sub(MAX_EDGES);
    lines.extend(all.into_iter().take(MAX_EDGES));
    if omitted > 0 {
        lines.push(format!("    %% omitted: {omitted} edges"));
    }
}

/// The same nodes and relations as DOT (never folded), and the number of nodes.
pub fn dot(input: &Input, focus: Option<&str>, limits: &Limits) -> (String, usize) {
    let model = model(input, focus);
    let keys = model.keys();
    let ids: BTreeMap<&str, usize> = keys
        .iter()
        .enumerate()
        .map(|(index, key)| (key.as_str(), index + 1))
        .collect();
    let labels = model.labels();
    let mut lines = vec![
        "digraph tsuzuri {".to_owned(),
        "    rankdir=LR;".to_owned(),
        "    node [shape=box, fontname=\"monospace\"];".to_owned(),
        "    edge [fontname=\"monospace\"];".to_owned(),
    ];
    for key in &keys {
        let id = ids[key.as_str()];
        let Some(symbol) = model.nodes.get(key.as_str()) else {
            lines.push(format!(
                "    n{id} [label=\"<<builtin>>\\n{}\"];",
                dot_esc(&labels[key], limits.label)
            ));
            continue;
        };
        let mut body = format!(
            "<<{}>>\\n{}\\n",
            symbol.kind,
            dot_esc(&labels[key], limits.label)
        );
        let ordered = model.members.get(key).map_or(&[][..], Vec::as_slice);
        for item in ordered.iter().take(MEMBER_LIMIT) {
            let name = dot_esc(&item.name, limits.label);
            match item.kind.as_str() {
                "field" => body.push_str(&format!(
                    "+{name} {}\\l",
                    dot_esc(item.result.as_deref().unwrap_or(""), limits.label)
                )),
                "case" => {
                    body.push_str(&format!("+{name}"));
                    if let Some(payload) = &item.result {
                        body.push_str(&format!(" {}", dot_esc(payload, limits.label)));
                    }
                    body.push_str("\\l");
                }
                _ => {
                    let params: Vec<&str> =
                        item.params.iter().map(|param| param.ty.as_str()).collect();
                    body.push_str(&format!(
                        "+{name}({}) {}\\l",
                        dot_esc(&params.join(", "), limits.label),
                        dot_esc(item.result.as_deref().unwrap_or("unit"), limits.label)
                    ));
                }
            }
        }
        if ordered.len() > MEMBER_LIMIT {
            body.push_str(&format!(
                "+\u{2026} {} more\\l",
                ordered.len() - MEMBER_LIMIT
            ));
        }
        lines.push(format!("    n{id} [label=\"{body}\"];"));
    }
    let mut relations: Vec<&Relation> = model.relations.iter().collect();
    relations.sort_by_key(|(a, b, arrow, label)| {
        (
            ids[a.as_str()],
            ids[b.as_str()],
            *arrow,
            label.as_deref().unwrap_or(""),
        )
    });
    for (a, b, arrow, label) in relations.into_iter().take(MAX_EDGES) {
        let mut attributes = match *arrow {
            "..|>" => "arrowhead=empty, style=dashed",
            "..>" => "arrowhead=open, style=dashed",
            _ => "arrowhead=vee",
        }
        .to_owned();
        if let Some(label) = label {
            attributes.push_str(&format!(", label=\"{}\"", dot_esc(label, limits.relation)));
        }
        lines.push(format!(
            "    n{} -> n{} [{attributes}];",
            ids[a.as_str()],
            ids[b.as_str()]
        ));
    }
    lines.push("}".to_owned());
    (lines.join("\n") + "\n", keys.len())
}
