//! `ownership` (G30, D6): one state diagram per variable of a function, from its ownership events
//! (moves, borrows, drops), and the ledger of every event with the role that a diagnostic gives
//! the same position.

use super::escape::{code_span, transition};
use super::input::{Event, Input};
use super::{DiagramError, MAX_EDGES};

const VARIABLE_LIMIT: usize = 12;
const LEDGER_LIMIT: usize = 200;

/// The states of one variable's diagram as they are written.
struct States {
    lines: Vec<String>,
    moved: Vec<String>,
    borrowed: Vec<String>,
    current: &'static str,
    last: String,
    step: usize,
    limit: usize,
}

impl States {
    fn add(&mut self, name: &'static str, label: &str) {
        self.step += 1;
        let state = format!("s{}", self.step);
        self.lines.push(format!("    state \"{name}\" as {state}"));
        self.lines.push(format!(
            "    {} --> {state} : {}",
            self.last,
            transition(label, self.limit)
        ));
        match name {
            "Moved" | "Dropped" => self.moved.push(state.clone()),
            "Borrowed" | "BorrowedMut" => self.borrowed.push(state.clone()),
            _ => {}
        }
        self.current = name;
        self.last = state;
    }
}

/// The state diagram of one variable, and how many of its events it leaves out to stay within
/// `MAX_EDGES` transitions; `events` are its events other than copies, in order. A borrow lasts
/// until the variable's next event.
fn variable_diagram(events: &[&Event], limit: usize) -> (String, usize) {
    let mut states = States {
        lines: vec![
            "stateDiagram-v2".to_owned(),
            "    direction LR".to_owned(),
            "    state \"Owned\" as s0".to_owned(),
            "    [*] --> s0".to_owned(),
        ],
        moved: Vec::new(),
        borrowed: Vec::new(),
        current: "Owned",
        last: "s0".to_owned(),
        step: 0,
        limit,
    };
    // `[*] --> s0` and the final `--> [*]`.
    let mut transitions = 2;
    let mut omitted = 0;
    for (index, event) in events.iter().enumerate() {
        let borrowing = matches!(states.current, "Borrowed" | "BorrowedMut");
        let kind = event.event.as_str();
        let release = borrowing && matches!(kind, "borrow" | "borrow_mut" | "move" | "drop");
        let needed = 1 + usize::from(release);
        if transitions + needed > MAX_EDGES {
            omitted = events.len() - index;
            break;
        }
        transitions += needed;
        let to = event
            .to
            .as_deref()
            .map(|to| format!(" \u{2192} {to}"))
            .unwrap_or_default();
        let line = event.line;
        if release {
            states.add("Owned", "release");
        }
        match kind {
            "borrow" => states.add("Borrowed", &format!("L{line} borrow{to}")),
            "borrow_mut" => states.add("BorrowedMut", &format!("L{line} borrow_mut{to}")),
            "reborrow" => {
                let state = if states.current == "BorrowedMut" {
                    "BorrowedMut"
                } else {
                    "Borrowed"
                };
                states.add(state, &format!("L{line} reborrow{to}"))
            }
            "move" => states.add("Moved", &format!("L{line} move{to}")),
            _ => states.add("Dropped", &format!("L{line} drop")),
        }
    }
    let mut lines = states.lines;
    lines.push(format!("    {} --> [*]", states.last));
    if !states.moved.is_empty() {
        lines.push("    classDef moved fill:#ffebe9,stroke:#cf222e".to_owned());
    }
    if !states.borrowed.is_empty() {
        lines.push("    classDef borrowed fill:#fff8c5,stroke:#9a6700".to_owned());
    }
    if !states.moved.is_empty() {
        lines.push(format!("    class {} moved", states.moved.join(",")));
    }
    if !states.borrowed.is_empty() {
        lines.push(format!("    class {} borrowed", states.borrowed.join(",")));
    }
    (lines.join("\n") + "\n", omitted)
}

/// The Markdown of `function`, the number of variables drawn, and whether a diagram or the ledger
/// left events out.
pub fn markdown(
    input: &Input,
    function: &str,
    limit: usize,
) -> Result<(String, usize, bool), DiagramError> {
    let unavailable = || DiagramError {
        kind: "diagram_unavailable",
        message: format!(
            "the KG has no ownership facts for '{function}'; build the KG with a compiler that records them"
        ),
    };
    let all = input
        .facts
        .get(function)
        .and_then(|facts| facts.ownership.as_ref())
        .ok_or_else(unavailable)?;
    let mut events: Vec<&Event> = all
        .iter()
        .filter(|event| !event.var.starts_with('$'))
        .collect();
    events.sort_by(|a, b| (a.line, a.column, &a.var).cmp(&(b.line, b.column, &b.var)));
    let mut variables: Vec<&str> = Vec::new();
    for event in &events {
        if !variables.contains(&event.var.as_str()) {
            variables.push(&event.var);
        }
    }
    fn of<'a, 'b>(
        events: &'b [&'a Event],
        variable: &'b str,
    ) -> impl Iterator<Item = &'b &'a Event> + 'b {
        events.iter().filter(move |event| event.var == variable)
    }
    let mut drawn: Vec<&str> = variables
        .iter()
        .copied()
        .filter(|variable| of(&events, variable).any(|event| event.event != "copy"))
        .collect();
    if drawn.len() > VARIABLE_LIMIT {
        drawn.retain(|variable| {
            of(&events, variable)
                .any(|event| matches!(event.event.as_str(), "move" | "borrow" | "borrow_mut"))
        });
    }
    drawn.truncate(VARIABLE_LIMIT);
    let omitted = variables.len() - drawn.len();
    let parameters: Vec<(&str, &str)> = input
        .symbol(function)
        .map(|symbol| {
            symbol
                .params
                .iter()
                .map(|param| (param.name.as_str(), param.ty.as_str()))
                .collect()
        })
        .unwrap_or_default();
    // A test's ID holds its name, which may be any text: the heading shows it as code.
    let mut parts = vec![format!("# Ownership of {}\n", code_span(function))];
    let mut cut = false;
    for variable in &drawn {
        let own: Vec<&Event> = of(&events, variable)
            .filter(|event| event.event != "copy")
            .copied()
            .collect();
        let (diagram, omitted) = variable_diagram(&own, limit);
        let heading = match parameters.iter().find(|(name, _)| name == variable) {
            // The type as inline code: `<i64>` of `Vec<i64>` is not HTML.
            Some((_, ty)) => format!("## {variable} (parameter: {})", code_span(ty)),
            None => format!("## {variable}"),
        };
        parts.push(format!("{heading}\n\n```mermaid\n{diagram}```\n"));
        if omitted > 0 {
            cut = true;
            parts.push(format!(
                "{omitted} later events of {variable} are not drawn.\n"
            ));
        }
    }
    if omitted > 0 {
        parts.push(format!(
            "{omitted} variables are not drawn; the table lists every event.\n"
        ));
    }
    if events.is_empty() {
        parts.push("(no ownership events)\n".to_owned());
    }
    let mut table = vec![
        "| # | line | var | event | to | diagnostic role |".to_owned(),
        "| --- | --- | --- | --- | --- | --- |".to_owned(),
    ];
    for (index, event) in events.iter().take(LEDGER_LIMIT).enumerate() {
        let role = match event.event.as_str() {
            "move" => "moved_here",
            "borrow" | "borrow_mut" => "borrowed_here",
            _ => "-",
        };
        table.push(format!(
            "| {} | {} | {} | {} | {} | {role} |",
            index + 1,
            event.line,
            event.var,
            event.event,
            event.to.as_deref().unwrap_or("-")
        ));
    }
    parts.push(table.join("\n") + "\n");
    if events.len() > LEDGER_LIMIT {
        cut = true;
        parts.push(format!(
            "{} more events are not listed.\n",
            events.len() - LEDGER_LIMIT
        ));
    }
    Ok((parts.join("\n"), drawn.len(), cut))
}

/// The Mermaid blocks of a Markdown text, for the validator.
pub fn mermaid_blocks(markdown: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current: Option<String> = None;
    for line in markdown.lines() {
        match &mut current {
            None if line == "```mermaid" => current = Some(String::new()),
            Some(block) if line == "```" => {
                blocks.push(std::mem::take(block));
                current = None;
            }
            Some(block) => {
                block.push_str(line);
                block.push('\n');
            }
            None => {}
        }
    }
    blocks
}
