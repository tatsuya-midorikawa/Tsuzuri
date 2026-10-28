//! The rows that the diagrams read (G30 appendix C): from a knowledge graph (`from_kg`) or from
//! JSON Lines texts (`from_texts`, the fixtures of the tests). Only the listed keys are read, and
//! the differences between the two sources are absorbed here and nowhere else.

use std::collections::BTreeMap;

use serde_json::Value;

use super::DiagramError;
use crate::kg::store::KgData;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub ty: String,
}

/// A row of `nodes.jsonl`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    pub id: String,
    pub kind: String,
    pub module: String,
    pub name: String,
    pub origin: String,
    pub params: Vec<Param>,
    pub result: Option<String>,
    pub pure: bool,
    pub line: usize,
    pub column: usize,
}

/// A row of `edges.jsonl`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relation {
    pub rel: String,
    pub from: String,
    pub to: String,
    pub args: Vec<String>,
    pub line: usize,
    pub column: usize,
}

/// An ownership event of a facts row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub var: String,
    pub event: String,
    pub to: Option<String>,
    pub line: usize,
    pub column: usize,
}

/// A row of `facts/*.jsonl`. `ownership` is `None` when the row has no such key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FactRow {
    pub effects: Vec<String>,
    pub effects_complete: bool,
    pub ownership: Option<Vec<Event>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutField {
    pub name: String,
    pub ty: String,
    pub slot: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutCase {
    pub name: String,
    pub payload: Option<String>,
}

/// A row of `layout/*.jsonl`: the native lower and upper bounds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutRow {
    pub id: String,
    pub kind: String,
    pub size: (u64, u64),
    pub align: (u64, u64),
    pub reordered: bool,
    pub layout: Option<String>,
    pub tag_bytes: Option<u64>,
    pub niche: Option<String>,
    pub drop_flag: bool,
    pub fields: Vec<LayoutField>,
    pub cases: Vec<LayoutCase>,
}

/// A module of `manifest.json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleRow {
    pub name: String,
    pub key: String,
    pub origin: String,
    pub path: String,
    pub source: String,
}

/// Everything a diagram reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Input {
    pub symbols: Vec<Symbol>,
    pub edges: Vec<Relation>,
    pub facts: BTreeMap<String, FactRow>,
    pub layouts: Vec<LayoutRow>,
    pub modules: Vec<ModuleRow>,
    pub edition: String,
    by_id: BTreeMap<String, usize>,
    /// The parent of each node by `contains`.
    parents: BTreeMap<String, String>,
}

fn broken(file: &str, line: usize, key: &str) -> DiagramError {
    DiagramError {
        kind: "kg_broken",
        message: format!("bad row in '{file}' line {line}: missing '{key}'"),
    }
}

/// The JSON rows of a JSON Lines text, with their line numbers.
fn rows(text: &str, file: &str) -> Result<Vec<(usize, Value)>, DiagramError> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| {
            serde_json::from_str(line)
                .map(|row| (index + 1, row))
                .map_err(|_| DiagramError {
                    kind: "kg_broken",
                    message: format!("bad row in '{file}' line {}: not JSON", index + 1),
                })
        })
        .collect()
}

/// The reader of one row: the keys it must have, and the optional ones.
struct Row<'a> {
    value: &'a Value,
    file: &'a str,
    line: usize,
}

impl Row<'_> {
    fn get(&self, key: &str) -> Result<&Value, DiagramError> {
        self.value
            .get(key)
            .ok_or_else(|| broken(self.file, self.line, key))
    }

    fn string(&self, key: &str) -> Result<String, DiagramError> {
        self.get(key)?
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| broken(self.file, self.line, key))
    }

    /// A key that must be there, as a string or `null`.
    fn nullable(&self, key: &str) -> Result<Option<String>, DiagramError> {
        Ok(self.get(key)?.as_str().map(str::to_owned))
    }

    fn flag(&self, key: &str) -> bool {
        self.value
            .get(key)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    fn array(&self, key: &str) -> Result<&Vec<Value>, DiagramError> {
        self.get(key)?
            .as_array()
            .ok_or_else(|| broken(self.file, self.line, key))
    }

    /// `span.line` and `span.column` (0 when the column is missing).
    fn position(&self) -> Result<(usize, usize), DiagramError> {
        let span = self.get("span")?;
        let line = span
            .get("line")
            .and_then(Value::as_u64)
            .ok_or_else(|| broken(self.file, self.line, "span.line"))?;
        let column = span.get("column").and_then(Value::as_u64).unwrap_or(0);
        Ok((line as usize, column as usize))
    }

    fn bounds(&self, key: &str) -> Result<(u64, u64), DiagramError> {
        let pair = self.array(key)?;
        match (
            pair.first().and_then(Value::as_u64),
            pair.get(1).and_then(Value::as_u64),
        ) {
            (Some(low), Some(high)) => Ok((low, high)),
            _ => Err(broken(self.file, self.line, key)),
        }
    }
}

fn symbol(row: &Row<'_>) -> Result<Symbol, DiagramError> {
    let params = row
        .array("params")?
        .iter()
        .map(|param| Param {
            name: param
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            ty: param
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        })
        .collect();
    let (line, column) = row.position()?;
    Ok(Symbol {
        id: row.string("id")?,
        kind: row.string("kind")?,
        module: row.string("module")?,
        name: row.string("name")?,
        origin: row.string("origin")?,
        params,
        result: row.nullable("result")?,
        pure: row.flag("pure"),
        line,
        column,
    })
}

fn relation(row: &Row<'_>) -> Result<Relation, DiagramError> {
    let (line, column) = row.position().unwrap_or((0, 0));
    Ok(Relation {
        rel: row.string("rel")?,
        from: row.string("from")?,
        to: row.string("to")?,
        args: strings(row.value.get("args")),
        line,
        column,
    })
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn events(value: Option<&Value>) -> Option<Vec<Event>> {
    let items = value?.as_array()?;
    Some(
        items
            .iter()
            .map(|item| {
                let span = item.get("span");
                let number = |key: &str| {
                    span.and_then(|span| span.get(key))
                        .and_then(Value::as_u64)
                        .unwrap_or(0) as usize
                };
                Event {
                    var: item
                        .get("var")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    event: item
                        .get("event")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    to: item.get("to").and_then(Value::as_str).map(str::to_owned),
                    line: number("line"),
                    column: number("column"),
                }
            })
            .collect(),
    )
}

fn fact(row: &Row<'_>) -> Result<(String, FactRow), DiagramError> {
    Ok((
        row.string("id")?,
        FactRow {
            effects: row
                .array("effects")?
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect(),
            effects_complete: row.flag("effects_complete"),
            ownership: events(row.value.get("ownership")),
        },
    ))
}

fn layout(row: &Row<'_>) -> Result<LayoutRow, DiagramError> {
    let kind = row.string("kind")?;
    let record = kind == "record";
    let fields = if record {
        row.array("fields")?
            .iter()
            .map(|field| LayoutField {
                name: field
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                ty: field
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                slot: field.get("slot").and_then(Value::as_u64).unwrap_or(0),
            })
            .collect()
    } else {
        Vec::new()
    };
    let cases = if record {
        Vec::new()
    } else {
        row.array("cases")?
            .iter()
            .map(|case| LayoutCase {
                name: case
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                payload: case
                    .get("payload")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            })
            .collect()
    };
    Ok(LayoutRow {
        id: row.string("id")?,
        size: row.bounds("size")?,
        align: row.bounds("align")?,
        reordered: row.flag("reordered"),
        layout: if record {
            None
        } else {
            Some(row.string("layout")?)
        },
        tag_bytes: row.value.get("tag_bytes").and_then(Value::as_u64),
        niche: row
            .value
            .get("niche")
            .and_then(Value::as_str)
            .map(str::to_owned),
        drop_flag: row.flag("drop_flag"),
        fields,
        cases,
        kind,
    })
}

impl Input {
    fn new(
        symbols: Vec<Symbol>,
        edges: Vec<Relation>,
        facts: BTreeMap<String, FactRow>,
        layouts: Vec<LayoutRow>,
        modules: Vec<ModuleRow>,
        edition: String,
    ) -> Self {
        let by_id = symbols
            .iter()
            .enumerate()
            .map(|(index, symbol)| (symbol.id.clone(), index))
            .collect();
        let parents = edges
            .iter()
            .filter(|edge| edge.rel == "contains")
            .map(|edge| (edge.to.clone(), edge.from.clone()))
            .collect();
        Self {
            symbols,
            edges,
            facts,
            layouts,
            modules,
            edition,
            by_id,
            parents,
        }
    }

    /// The rows of a knowledge graph. Its `facts/` and `layout/` files are all read.
    pub fn from_kg(data: &KgData) -> Result<Input, DiagramError> {
        let symbols = data
            .nodes
            .iter()
            .map(|node| Symbol {
                id: node.id.clone(),
                kind: node.kind.clone(),
                module: node.module.clone(),
                name: node.name.clone(),
                origin: node.origin.clone(),
                params: node
                    .params
                    .iter()
                    .map(|param| Param {
                        name: param.name.clone(),
                        ty: param.ty.clone(),
                    })
                    .collect(),
                result: node.result.clone(),
                pure: node.pure,
                line: node.span.line,
                column: node.span.column,
            })
            .collect();
        let edges = data
            .edges()
            .iter()
            .map(|edge| Relation {
                rel: edge.rel.clone(),
                from: edge.from.clone(),
                to: edge.to.clone(),
                args: edge.args.clone().unwrap_or_default(),
                line: edge.span.line,
                column: edge.span.column,
            })
            .collect();
        let facts = data
            .facts
            .iter()
            .flat_map(|tables| tables.all())
            .map(|fact| {
                (
                    fact.id.clone(),
                    FactRow {
                        effects: fact.effects.clone(),
                        effects_complete: fact.effects_complete,
                        ownership: events(fact.raw.get("ownership")),
                    },
                )
            })
            .collect();
        let mut layouts = Vec::new();
        if let Some(tables) = &data.layouts {
            for (number, value) in tables.all().enumerate() {
                layouts.push(layout(&Row {
                    value,
                    file: "layout",
                    line: number + 1,
                })?);
            }
        }
        let modules = data
            .meta
            .modules
            .iter()
            .map(|module| ModuleRow {
                name: module.name.clone(),
                key: module.key.clone(),
                origin: module.origin.clone(),
                path: module.path.clone(),
                source: module.source.clone(),
            })
            .collect();
        Ok(Self::new(
            symbols,
            edges,
            facts,
            layouts,
            modules,
            data.meta.edition.clone(),
        ))
    }

    /// The same rows from JSON Lines texts: `symbols` (`nodes.jsonl`), `graph` (`edges.jsonl`),
    /// the facts and layout files, and `manifest` (`manifest.json`; empty for none).
    pub fn from_texts(
        symbols: &str,
        graph: &str,
        facts: &[&str],
        layouts: &[&str],
        manifest: &str,
    ) -> Result<Input, DiagramError> {
        let read = |text: &str, file: &str| -> Result<Vec<(usize, Value)>, DiagramError> {
            rows(text, file)
        };
        let mut symbol_rows = Vec::new();
        for (line, value) in read(symbols, "symbols")? {
            symbol_rows.push(symbol(&Row {
                value: &value,
                file: "symbols",
                line,
            })?);
        }
        let mut edges = Vec::new();
        for (line, value) in read(graph, "graph")? {
            edges.push(relation(&Row {
                value: &value,
                file: "graph",
                line,
            })?);
        }
        let mut fact_rows = BTreeMap::new();
        for text in facts {
            for (line, value) in read(text, "facts")? {
                let (id, row) = fact(&Row {
                    value: &value,
                    file: "facts",
                    line,
                })?;
                fact_rows.insert(id, row);
            }
        }
        let mut layout_rows = Vec::new();
        for text in layouts {
            for (line, value) in read(text, "layouts")? {
                layout_rows.push(layout(&Row {
                    value: &value,
                    file: "layouts",
                    line,
                })?);
            }
        }
        let (modules, edition) = if manifest.trim().is_empty() {
            (Vec::new(), "2026".to_owned())
        } else {
            let value: Value = serde_json::from_str(manifest).map_err(|_| DiagramError {
                kind: "kg_broken",
                message: "bad row in 'manifest' line 1: not JSON".to_owned(),
            })?;
            let modules = value
                .get("modules")
                .and_then(Value::as_array)
                .map(|modules| {
                    modules
                        .iter()
                        .map(|module| {
                            let text = |key: &str| {
                                module
                                    .get(key)
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_owned()
                            };
                            ModuleRow {
                                name: text("name"),
                                key: text("key"),
                                origin: text("origin"),
                                path: text("path"),
                                source: text("source"),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            let edition = value
                .get("edition")
                .and_then(Value::as_str)
                .unwrap_or("2026")
                .to_owned();
            (modules, edition)
        };
        Ok(Self::new(
            symbol_rows,
            edges,
            fact_rows,
            layout_rows,
            modules,
            edition,
        ))
    }

    pub fn symbol(&self, id: &str) -> Option<&Symbol> {
        self.by_id.get(id).map(|index| &self.symbols[*index])
    }

    pub fn module_of(&self, id: &str) -> Option<&str> {
        self.symbol(id).map(|symbol| symbol.module.as_str())
    }

    /// The owner of a member: its parent by `contains`, or the ID before its last `.` in rows
    /// without the structural edges.
    pub fn parent(&self, id: &str) -> String {
        match self.parents.get(id) {
            Some(parent) => parent.clone(),
            None => id
                .rsplit_once('.')
                .map_or(id, |(owner, _)| owner)
                .to_owned(),
        }
    }

    /// The IDs of the modules, in byte order.
    pub fn module_ids(&self) -> Vec<&str> {
        let mut ids: Vec<&str> = self
            .symbols
            .iter()
            .filter(|symbol| symbol.kind == "module")
            .map(|symbol| symbol.id.as_str())
            .collect();
        ids.sort_unstable();
        ids
    }
}
