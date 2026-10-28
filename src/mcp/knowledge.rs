//! The tools that read the compiler's references and the project's knowledge graph (G29 Phase 2,
//! D-60): the guide, `explain`, the queries of `kg`, type search, `impact` and the diagrams. The
//! graph is read from disk when it is fresh, and built in memory otherwise; nothing is written.

use std::collections::BTreeSet;
use std::sync::Arc;

use serde_json::{Map, Value, json};

use super::backend::{ProjectBackend, ToolError, ToolOutput, ToolRequest};
use super::path::{Wanted, resolve_path, scrub};
use crate::kg::manifest::Freshness;
use crate::kg::query::{self, BriefTarget, Options, QueryError};
use crate::kg::store::{KgData, KgView, StdSource};

/// The graph of the last call, for its inputs' hash.
pub struct Cached {
    hash: String,
    view: Arc<KgView>,
}

const INCOMPLETE: &str = "the project does not type-check, so there are no facts yet; call tz_check and fix the errors first";

fn argument<'a>(request: &'a ToolRequest<'_>, key: &str) -> Option<&'a Value> {
    request.arguments.get(key)
}

fn text_argument<'a>(request: &'a ToolRequest<'_>, key: &str) -> Option<&'a str> {
    argument(request, key).and_then(Value::as_str)
}

fn number_argument(request: &ToolRequest<'_>, key: &str, default: u64) -> usize {
    argument(request, key)
        .and_then(Value::as_u64)
        .unwrap_or(default) as usize
}

/// The graph of `data`'s project, with std's graph built in memory when a query needs it.
fn view_of(data: KgData) -> Arc<KgView> {
    Arc::new(KgView::new(Some(Arc::new(data)), StdSource::Memory))
}

/// The project's graph: the one on disk when it is fresh, else one built in memory. A graph on
/// disk is read whole and checked against its manifest at once (`KgData::read_all`: plain paths
/// below the graph's directory, every file with its SHA-256), and the manifest of what was read
/// must be fresh (`KgData::open_fresh`), so that what the server keeps does not depend on the
/// directory later; one that does not pass is built in memory instead.
pub(crate) fn view(backend: &ProjectBackend) -> Result<Arc<KgView>, ToolError> {
    opened(backend).map(|(view, _)| view)
}

/// The project's graph (as `view`) and the project as it was loaded for it.
fn opened(backend: &ProjectBackend) -> Result<(Arc<KgView>, crate::driver::Project), ToolError> {
    let root = &backend.root;
    let project = crate::driver::Project::load_for_tests(root)
        .map_err(|error| ToolError(scrub(&error.diagnostic.message, root)))?;
    let hash = crate::kg::input::hex(&crate::kg::input::source_hash(&project));
    if let Ok(cached) = backend.knowledge.lock()
        && let Some(cached) = cached.as_ref()
        && cached.hash == hash
    {
        return Ok((Arc::clone(&cached.view), project));
    }
    let stamp =
        crate::kg::build::current_stamp().map_err(|error| ToolError(error.message.clone()))?;
    let data = match crate::kg::manifest::inspect(&backend.kg_dir, &hash, &stamp) {
        Freshness::Incomplete(_) => return Err(ToolError(INCOMPLETE.into())),
        Freshness::Fresh => match KgData::open_fresh(&backend.kg_dir, &hash, &stamp) {
            Ok(data) if data.read_all().is_ok() => data,
            _ => built(backend, &project)?,
        },
        _ => built(backend, &project)?,
    };
    let view = view_of(data);
    if let Ok(mut cached) = backend.knowledge.lock() {
        *cached = Some(Cached {
            hash,
            view: Arc::clone(&view),
        });
    }
    Ok((view, project))
}

fn built(backend: &ProjectBackend, project: &crate::driver::Project) -> Result<KgData, ToolError> {
    backend.count_analysis();
    crate::kg::build::memory(project).map_err(|unbuilt| match unbuilt {
        crate::kg::build::Unbuilt::Errors(_) => ToolError(INCOMPLETE.into()),
        crate::kg::build::Unbuilt::Failure(error) => {
            ToolError(scrub(&error.message, &backend.root))
        }
    })
}

/// The message of an ID that does not resolve, in the words of the tools.
fn id_error(error: QueryError) -> ToolError {
    ToolError(match error {
        QueryError::Ambiguous {
            text,
            candidates,
            more,
        } => {
            let mut message = format!(
                "ambiguous id '{text}'; candidates: {}",
                candidates.join(", ")
            );
            if more > 0 {
                message.push_str(&format!(", \u{2026}{more} more"));
            }
            message
        }
        QueryError::Unknown { text, suggestions } if suggestions.is_empty() => {
            format!("unknown id '{text}'; use tz_search to find ids")
        }
        QueryError::Unknown { text, suggestions } => format!(
            "unknown id '{text}'; use tz_search to find ids; did you mean: {}",
            suggestions.join(", ")
        ),
        QueryError::Empty(message) => message.to_owned(),
    })
}

fn first_row(output: &query::Output) -> Value {
    output
        .rows
        .first()
        .and_then(|row| serde_json::from_str(row).ok())
        .unwrap_or(Value::Null)
}

fn rows(output: &query::Output) -> Vec<Value> {
    output
        .rows
        .iter()
        .filter_map(|row| serde_json::from_str(row).ok())
        .collect()
}

pub(crate) fn guide(request: &ToolRequest<'_>) -> Result<ToolOutput, ToolError> {
    use crate::guide::{self, Action};
    let edition = crate::syntax::Edition::LATEST;
    let topic = text_argument(request, "topic");
    let (name, text) = match topic {
        None => (
            "card",
            guide::render(&Action::Level(0), edition).map(|rendered| rendered.text),
        ),
        Some("list") => ("list", Ok(guide::list_text(edition))),
        Some("std") => (
            "std",
            guide::render(
                &Action::Std(text_argument(request, "module").map(str::to_owned)),
                edition,
            )
            .map(|rendered| rendered.text),
        ),
        Some(topic) => (
            topic,
            guide::render(&Action::Topic(topic.to_owned()), edition).map(|rendered| rendered.text),
        ),
    };
    let text = text.map_err(|error| {
        ToolError(match (error.kind, topic) {
            (Some("guide_unknown_topic"), Some(topic)) => format!(
                "unknown guide topic '{topic}'; call tz_guide with topic \"list\" to see the topics"
            ),
            _ => error.message.clone(),
        })
    })?;
    Ok(ToolOutput {
        structured: json!({"topic": name, "text": text}),
        text,
    })
}

pub(crate) fn explain(request: &ToolRequest<'_>) -> Result<ToolOutput, ToolError> {
    use crate::explain::{self, Target};
    let query = text_argument(request, "query").unwrap_or_default();
    let target = explain::resolve(query).map_err(|candidates| {
        let mut message = format!(
            "unknown diagnostic '{query}'; use a code such as E1012 or a kind such as use_after_move"
        );
        if !candidates.is_empty() {
            let shown: Vec<&str> = candidates.iter().take(5).map(String::as_str).collect();
            message.push_str(&format!("; did you mean: {}", shown.join(", ")));
        }
        ToolError(message)
    })?;
    let (text, json_line, kind) = match target {
        Target::Kind(kind) => (
            explain::render_kind(kind, false),
            explain::render_kind(kind, true),
            true,
        ),
        Target::Code(code) => (
            explain::render_code(code, false),
            explain::render_code(code, true),
            false,
        ),
    };
    let row: Value = serde_json::from_str(json_line.trim()).unwrap_or(Value::Null);
    let mut structured = Map::new();
    structured.insert("code".into(), row["code"].clone());
    structured.insert("title".into(), row["title"].clone());
    if kind {
        structured.insert("kind".into(), row["kind"].clone());
        if let Some(since) = row.get("since").filter(|since| since.is_string()) {
            structured.insert("since".into(), since.clone());
        }
    }
    structured.insert("text".into(), json!(text));
    Ok(ToolOutput {
        text,
        structured: Value::Object(structured),
    })
}

pub(crate) fn symbol(
    backend: &ProjectBackend,
    request: &ToolRequest<'_>,
) -> Result<ToolOutput, ToolError> {
    let view = view(backend)?;
    let id = text_argument(request, "id").unwrap_or_default();
    let output = query::show(&view, id, &Options::default()).map_err(id_error)?;
    let row = first_row(&output);
    let ids = |key: &str, limit: usize| -> (Vec<Value>, bool) {
        let mut seen = BTreeSet::new();
        let all: Vec<Value> = row[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|edge| edge["to"].as_str())
            .filter(|to| seen.insert(*to))
            .map(|to| json!(to))
            .collect();
        let cut = all.len() > limit;
        (all.into_iter().take(limit).collect(), cut)
    };
    let (callees, callees_cut) = ids("callees", 50);
    let (tests, tests_cut) = ids("tests", 20);
    let resolved = query::resolve_one(&view, id, false).map_err(id_error)?;
    let symbol = match &row["symbol"] {
        Value::Null => json!({
            "id": resolved,
            "kind": "external",
            "name": resolved.rsplit('.').next().unwrap_or(&resolved),
        }),
        symbol => symbol.clone(),
    };
    let callers = row["callers"].as_array().map_or(0, Vec::len);
    Ok(ToolOutput {
        text: output.text,
        structured: json!({
            "symbol": symbol,
            "facts": row["facts"],
            "callees": callees,
            "tests": tests,
            "callers": callers,
            "truncated": callees_cut || tests_cut,
        }),
    })
}

pub(crate) fn brief(
    backend: &ProjectBackend,
    request: &ToolRequest<'_>,
) -> Result<ToolOutput, ToolError> {
    let view = view(backend)?;
    let target = match text_argument(request, "id") {
        Some(id) => BriefTarget::Id(id.to_owned()),
        None => BriefTarget::Project,
    };
    let options = Options {
        budget: argument(request, "budget")
            .and_then(Value::as_u64)
            .map(|budget| budget as usize),
        depth: 1,
    };
    let output = query::brief(&view, &target, &options).map_err(id_error)?;
    let row = first_row(&output);
    Ok(ToolOutput {
        text: output.text,
        structured: json!({"id": row["id"], "level": row["level"], "text": row["text"]}),
    })
}

pub(crate) fn search(
    backend: &ProjectBackend,
    request: &ToolRequest<'_>,
) -> Result<ToolOutput, ToolError> {
    let view = view(backend)?;
    let text = text_argument(request, "query").unwrap_or_default();
    let kind = text_argument(request, "kind");
    let limit = number_argument(request, "limit", 20);
    let output = query::search(&view, text, false, &Options::default()).map_err(id_error)?;
    let all: Vec<Value> = rows(&output)
        .into_iter()
        .filter(|row| row["type"] == "match")
        .filter(|row| kind.is_none_or(|kind| row["kind"] == kind))
        .collect();
    let truncated = all.len() > limit;
    let matches: Vec<Value> = all
        .iter()
        .take(limit)
        .map(|row| json!({"id": row["id"], "kind": row["kind"], "signature": row["signature"]}))
        .collect();
    // The text of `kg search` when nothing is left out, else the same lines for what is kept.
    let text = if kind.is_none() && !truncated {
        output.text
    } else {
        let mut lines = vec![format!("search '{text}' (project)")];
        for (index, row) in matches.iter().enumerate() {
            lines.push(format!(
                "{} {}  {}  {}",
                index + 1,
                row["id"].as_str().unwrap_or_default(),
                row["kind"].as_str().unwrap_or_default(),
                row["signature"].as_str().unwrap_or_default()
            ));
        }
        lines.push(format!(
            "summary: {} {}",
            matches.len(),
            if matches.len() == 1 {
                "match"
            } else {
                "matches"
            }
        ));
        lines.join("\n") + "\n"
    };
    Ok(ToolOutput {
        text,
        structured: json!({"query": text_argument(request, "query"), "matches": matches, "truncated": truncated}),
    })
}

pub(crate) fn callers(
    backend: &ProjectBackend,
    request: &ToolRequest<'_>,
) -> Result<ToolOutput, ToolError> {
    let view = view(backend)?;
    let text = text_argument(request, "id").unwrap_or_default();
    let relation = text_argument(request, "relation").unwrap_or("calls");
    let depth = number_argument(request, "depth", 1);
    let limit = number_argument(request, "limit", 50);
    let id = query::resolve_one(&view, text, false).map_err(id_error)?;
    let options = Options {
        budget: None,
        depth,
    };
    let (text, all) = if relation == "calls" {
        let output = query::calls(&view, text, &options, false).map_err(id_error)?;
        let rows: Vec<Value> = rows(&output)
            .into_iter()
            .filter(|row| row["type"] == "edge")
            .map(|row| {
                let to = match &row["via"] {
                    Value::String(via) => via.clone(),
                    _ => id.clone(),
                };
                json!({"rel": "calls", "from": row["id"], "to": to, "span": row["span"], "depth": row["depth"]})
            })
            .collect();
        (output.text, rows)
    } else {
        uses(&view, text, &id, depth, &options)?
    };
    let truncated = all.len() > limit;
    let text = if truncated {
        limited_text(&text, &all, limit)
    } else {
        text
    };
    Ok(ToolOutput {
        text,
        structured: json!({
            "id": id,
            "relation": relation,
            "depth": depth,
            "callers": all.into_iter().take(limit).collect::<Vec<_>>(),
            "truncated": truncated,
        }),
    })
}

/// The text of the first `limit` of `rows` (edges with `from`, `to`, `span` and `depth`), between
/// the header and the summary of the full answer `text`, with the count of the rest: what the
/// model reads is what `structuredContent` holds.
fn limited_text(text: &str, rows: &[Value], limit: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = vec![lines.first().copied().unwrap_or_default().to_owned()];
    for row in rows.iter().take(limit) {
        let span = &row["span"];
        let mut line = format!(
            "{} {}  {}:{}:{}",
            row["depth"],
            row["from"].as_str().unwrap_or_default(),
            span["path"].as_str().unwrap_or_default(),
            span["line"],
            span["column"]
        );
        if row["depth"].as_u64().is_some_and(|depth| depth > 1) {
            line.push_str(&format!("  via {}", row["to"].as_str().unwrap_or_default()));
        }
        out.push(line);
    }
    out.push(format!("\u{2026}{} more; raise limit", rows.len() - limit));
    if let Some(summary) = lines.last().filter(|line| line.starts_with("summary:")) {
        out.push((*summary).to_owned());
    }
    out.join("\n") + "\n"
}

/// The declarations that use the type or class `id` (`uses_type`, and for a class `implements`
/// and `instance_of`, as `kg uses` lists them), and those that use them, up to `depth` levels.
fn uses(
    view: &KgView,
    text: &str,
    id: &str,
    depth: usize,
    options: &Options,
) -> Result<(String, Vec<Value>), ToolError> {
    let Some(data) = view.project() else {
        return Ok((String::new(), Vec::new()));
    };
    let mut seen: BTreeSet<String> = BTreeSet::from([id.to_owned()]);
    let mut frontier = vec![id.to_owned()];
    let mut rows = Vec::new();
    for level in 1..=depth {
        let mut next = Vec::new();
        frontier.sort();
        for target in &frontier {
            let mut edges: Vec<_> = query::USE_RELATIONS
                .iter()
                .flat_map(|rel| data.edges_to(rel, target))
                .collect();
            edges.sort_by(|a, b| {
                (&a.rel, &a.from, a.span.start).cmp(&(&b.rel, &b.from, b.span.start))
            });
            for edge in edges {
                rows.push(json!({
                    "rel": edge.rel,
                    "from": edge.from,
                    "to": edge.to,
                    "span": edge.raw["span"],
                    "depth": level,
                }));
                if seen.insert(edge.from.clone()) {
                    next.push(edge.from.clone());
                }
            }
        }
        frontier = next;
    }
    let text = if depth == 1 {
        query::uses(view, text, options).map_err(id_error)?.text
    } else {
        let mut lines = vec![format!("uses of {id} (depth {depth})")];
        for row in &rows {
            let span = &row["span"];
            lines.push(format!(
                "{} {}  {}:{}:{}",
                row["depth"],
                row["from"].as_str().unwrap_or_default(),
                span["path"].as_str().unwrap_or_default(),
                span["line"],
                span["column"]
            ));
        }
        lines.push(format!(
            "summary: {} {}, depth {depth}",
            rows.len(),
            if rows.len() == 1 { "use" } else { "uses" }
        ));
        lines.join("\n") + "\n"
    };
    Ok((text, rows))
}

pub(crate) fn path(
    backend: &ProjectBackend,
    request: &ToolRequest<'_>,
) -> Result<ToolOutput, ToolError> {
    let view = view(backend)?;
    let from = text_argument(request, "from").unwrap_or_default();
    let to = text_argument(request, "to").unwrap_or_default();
    let relations: Vec<&str> = match argument(request, "relations").and_then(Value::as_array) {
        Some(relations) => relations.iter().filter_map(Value::as_str).collect(),
        None => vec!["calls"],
    };
    let depth = number_argument(request, "depth", query::PATH_DEPTH as u64);
    let output =
        query::path(&view, from, to, &relations, depth, &Options::default()).map_err(id_error)?;
    let all = rows(&output);
    let steps: Vec<Value> = all
        .iter()
        .filter(|row| row["type"] == "step")
        .map(|row| json!({"index": row["index"], "from": row["from"], "rel": row["rel"], "to": row["to"], "span": row["span"]}))
        .collect();
    let found = all
        .iter()
        .find(|row| row["type"] == "summary")
        .and_then(|row| row["found"].as_bool())
        .unwrap_or(false);
    Ok(ToolOutput {
        text: output.text,
        structured: json!({
            "from": query::resolve_one(&view, from, false).map_err(id_error)?,
            "to": query::resolve_one(&view, to, false).map_err(id_error)?,
            "relations": relations,
            "depth": depth,
            "found": found,
            "steps": steps,
        }),
    })
}

pub(crate) fn find(
    backend: &ProjectBackend,
    request: &ToolRequest<'_>,
) -> Result<ToolOutput, ToolError> {
    use crate::kg::find;
    let view = view(backend).map_err(|error| {
        if error.0 == INCOMPLETE {
            ToolError("the project does not type-check; call tz_check first".into())
        } else {
            error
        }
    })?;
    let text = text_argument(request, "query").unwrap_or_default();
    let limit = number_argument(request, "limit", 10);
    let query = find::Query {
        text: Some(text.to_owned()),
        limit,
        ..find::Query::default()
    };
    let (found, searched) = find::search(&view, &query)
        .map_err(|error| ToolError(format!("cannot parse the type query: {}", error.0)))?;
    let (rendered, _) = find::render(text, &query, &found, searched, None);
    // The tool's argument is `limit`, at most 10.
    let rendered = rendered.replace(
        "; raise --limit (at most 100)\n",
        "; raise limit (at most 10)\n",
    );
    let candidates: Vec<Value> = found
        .iter()
        .take(limit)
        .map(|item| {
            json!({
                "name": item.name,
                "signature": format!("{} :: {}", item.name, item.signature),
                "how": if item.parameters > 0 { "call" } else { "value" },
            })
        })
        .collect();
    Ok(ToolOutput {
        text: rendered,
        structured: json!({"query": text, "candidates": candidates, "truncated": found.len() > limit}),
    })
}

/// The graph of a directory of `impact`: a graph as it is, or a project's, built in memory.
fn graph_at(backend: &ProjectBackend, arg: &str) -> Result<KgData, ToolError> {
    let directory = resolve_path(&backend.root, arg, Wanted::Directory)?;
    if crate::kg::write::has_marker(&directory) {
        // Read whole and checked now: a graph whose files leave its directory or do not match its
        // manifest is refused.
        return crate::kg::open::read(&directory)
            .and_then(|data| data.read_all().map(|()| data))
            .map_err(|error| ToolError(scrub(&error.message, &backend.root)));
    }
    let neither = || {
        ToolError(format!(
            "'{arg}' is neither a knowledge graph nor a project directory"
        ))
    };
    let project = crate::driver::Project::load_for_tests(&directory).map_err(|_| neither())?;
    if !project
        .sources
        .iter()
        .any(|source| source.origin == crate::check::ModuleOrigin::User)
    {
        return Err(neither());
    }
    built(backend, &project)
}

pub(crate) fn impact(
    backend: &ProjectBackend,
    request: &ToolRequest<'_>,
) -> Result<ToolOutput, ToolError> {
    let old = text_argument(request, "old").unwrap_or_default();
    let new = text_argument(request, "new").filter(|new| !new.is_empty());
    let base = graph_at(backend, old)?;
    let impact = match new {
        Some(new) => {
            let head = graph_at(backend, new)?;
            crate::kg::impact::diff(&base, &head, old, new)
        }
        None => {
            let view = view(backend)?;
            let head = view.project().ok_or_else(|| ToolError(INCOMPLETE.into()))?;
            crate::kg::impact::diff(&base, head, old, "working tree")
        }
    };
    let records: Vec<Value> = impact
        .render_json()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    Ok(ToolOutput {
        text: impact.render_text(),
        structured: json!({"old": old, "new": new.unwrap_or("working tree"), "records": records}),
    })
}

pub(crate) fn diagram(
    backend: &ProjectBackend,
    request: &ToolRequest<'_>,
) -> Result<ToolOutput, ToolError> {
    use crate::diagram::{self, DiagramRequest, Format, Kind};
    let kind_text = text_argument(request, "kind").unwrap_or_default();
    let kind = Kind::parse(kind_text)
        .ok_or_else(|| ToolError(format!("unknown diagram kind '{kind_text}'")))?;
    let (view, project) = opened(backend)?;
    let data = view.project().ok_or_else(|| ToolError(INCOMPLETE.into()))?;
    let input = diagram::Input::from_kg(data).map_err(|error| ToolError(error.message))?;
    // Only the sources that the project loaded, by the paths that the graph's modules name: a
    // path that a graph on disk names otherwise (a link out of the root) is not read.
    let sources = diagram::ProjectSources::of(&project);
    let request = DiagramRequest {
        kind,
        id: text_argument(request, "id").map(str::to_owned),
        depth: number_argument(request, "depth", diagram::DEFAULT_DEPTH as u64),
        max_nodes: number_argument(request, "max_nodes", diagram::DEFAULT_MAX_NODES as u64),
        format: match text_argument(request, "format") {
            Some("dot") => Format::Dot,
            _ => Format::Mermaid,
        },
    };
    let output =
        diagram::render(&input, &sources, &request).map_err(|error| ToolError(error.message))?;
    Ok(ToolOutput {
        structured: json!({
            "kind": kind.name(),
            "format": output.format.name(),
            "text": output.text,
            "nodes": output.nodes,
            "truncated": output.folded,
        }),
        text: output.text,
    })
}
