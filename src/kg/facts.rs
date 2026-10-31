//! The facts of the function-like nodes (G26 phases 3 to 6): effects, sites, ownership events,
//! recursion, loops and specializations. The instance view (the final, specialized module) gives
//! what needs concrete types; each final function belongs to the declaration it came from (its
//! root, appendix C2), and a declaration collects the facts of all its instances.

use std::collections::BTreeMap;

use super::effects::{self, Evidence};
use super::model::{
    AllocSite, CopySiteRow, EffectSite, FactsRow, LoopRow, OwnershipRow, ParallelRow, SpanTable,
    TrapSite,
};
use super::sites;
use crate::check::kg_capture::{Capture, FUNCTION_NODES, VIEW_NODES};
use crate::check::{CheckedModule, ModuleOrigin};
use crate::copies::{CopyCost, CopyKind};
use crate::diagnostic::Span;

/// Every array of a facts row keeps at most this many elements (G26 rule 6).
pub(crate) const ARRAY_LIMIT: usize = 100;

/// The node of each final function (appendix C2): a lifted lambda belongs to the function it was
/// lifted from, a specialization to its generic declaration; std functions and generated helpers
/// belong to none.
pub(crate) fn roots(module: &CheckedModule, capture: &Capture) -> Vec<Option<String>> {
    let mut roots = vec![None; module.functions.len()];
    for (index, root) in roots.iter_mut().enumerate() {
        let mut at = index;
        // A lambda's parent comes before it, but a chain of lifts is short; follow it.
        for _ in 0..module.functions.len() {
            let function = &module.functions[at];
            if matches!(function.module.as_str(), "$lambda" | "$task") {
                match function.origin.parent {
                    Some(parent) if parent != at => {
                        at = parent;
                        continue;
                    }
                    _ => break,
                }
            }
            if function.module.starts_with('$') || function.origin.module == ModuleOrigin::Std {
                break;
            }
            let base = function
                .name
                .split(".$mono.")
                .next()
                .unwrap_or(&function.name);
            *root = capture
                .function_ids
                .get(&(function.module.clone(), base.to_owned()))
                .cloned();
            break;
        }
    }
    roots
}

/// The name of each final function in the chains of `effect_sites`: a node's ID, or an std
/// function's `Module.name`; lifted lambdas and generated helpers have none.
fn chain_names(module: &CheckedModule, capture: &Capture) -> Vec<Option<String>> {
    module
        .functions
        .iter()
        .map(|function| {
            if function.module.starts_with('$') {
                return None;
            }
            let base = function
                .name
                .split(".$mono.")
                .next()
                .unwrap_or(&function.name);
            if function.origin.module == ModuleOrigin::Std {
                return (!base.starts_with('$')).then(|| format!("{}.{base}", function.module));
            }
            capture
                .function_ids
                .get(&(function.module.clone(), base.to_owned()))
                .cloned()
        })
        .collect()
}

fn copy_kind(kind: CopyKind) -> &'static str {
    match kind {
        CopyKind::Local => "local",
        CopyKind::Field => "field",
        CopyKind::Element => "element",
        CopyKind::Tail => "tail",
        CopyKind::Payload => "payload",
        CopyKind::Dereference => "dereference",
        CopyKind::Temporary => "temporary",
    }
}

fn copy_cost(cost: CopyCost) -> &'static str {
    match cost {
        CopyCost::Length => "length",
        CopyCost::Environment => "environment",
    }
}

/// What the instances of one node collect before they become a row.
#[derive(Default)]
struct Collected {
    effects: BTreeMap<&'static str, Evidence>,
    traps: Vec<sites::Trap>,
    allocations: Vec<(Span, &'static str)>,
    /// Position, type, kind, cost, and the copied variable (`$value` for another expression).
    copies: Vec<(Span, String, &'static str, &'static str, String)>,
    specializations: usize,
    parallel: Vec<ParallelRow>,
    truncated: bool,
}

/// The sources of `check --report parallel` as the graph sees them: paths relative to the root.
struct ReportSources<'t, 'a>(&'t SpanTable<'a>);

impl crate::parallel_report::Sources for ReportSources<'_, '_> {
    fn text(&self, span: Span) -> &str {
        self.0.source(span).map_or("", |(_, text)| text)
    }

    fn path(&self, span: Span) -> String {
        self.0
            .source(span)
            .map_or_else(String::new, |(path, _)| path.to_owned())
    }
}

/// Sorts, removes duplicates and keeps at most `ARRAY_LIMIT` elements, counting the dropped ones
/// under `key` in `truncated`.
fn limit<T: Ord>(mut values: Vec<T>, key: &str, truncated: &mut BTreeMap<String, usize>) -> Vec<T> {
    values.sort();
    values.dedup();
    if values.len() > ARRAY_LIMIT {
        truncated.insert(key.to_owned(), values.len() - ARRAY_LIMIT);
        values.truncate(ARRAY_LIMIT);
    }
    values
}

/// The facts rows of the function-like nodes, by node ID.
pub(crate) fn build(
    module: &CheckedModule,
    capture: &Capture,
    table: &SpanTable,
) -> BTreeMap<String, FactsRow> {
    let types = module.types();
    let roots = roots(module, capture);
    let names = chain_names(module, capture);
    let mut budget = VIEW_NODES;
    let mut drops = BTreeMap::new();
    let scans: Vec<_> = (0..module.functions.len())
        .map(|function| effects::scan(module, function, FUNCTION_NODES, &mut budget, &mut drops))
        .collect();
    let truncated_scans: Vec<bool> = scans.iter().map(|scan| scan.truncated).collect();
    let summary = effects::summarize(&scans, &names);
    let mut collected: BTreeMap<&str, Collected> = BTreeMap::new();
    let mut site_budget = VIEW_NODES;
    for (function, root) in roots.iter().enumerate() {
        let Some(root) = root else {
            continue;
        };
        let entry = collected.entry(root.as_str()).or_default();
        for (effect, evidence) in &summary[function] {
            let better = match entry.effects.get(effect) {
                Some(best) => {
                    (effects::order(evidence.span), evidence.length)
                        < (effects::order(best.span), best.length)
                }
                None => true,
            };
            if better {
                entry.effects.insert(effect, *evidence);
            }
        }
        let (traps, allocations, stopped) =
            sites::sites(module, function, FUNCTION_NODES, &mut site_budget);
        entry.traps.extend(traps);
        entry.allocations.extend(allocations);
        entry.truncated |= stopped || truncated_scans[function];
        if module.functions[function].name.contains(".$mono.") {
            entry.specializations += 1;
        }
    }
    for site in crate::copies::sites(module) {
        if let Some(root) = &roots[site.function] {
            let var = table
                .text(site.span)
                .filter(|text| {
                    !text.is_empty()
                        && text
                            .chars()
                            .all(|character| character.is_alphanumeric() || character == '_')
                })
                .unwrap_or("$value")
                .to_owned();
            collected.entry(root.as_str()).or_default().copies.push((
                site.span,
                site.ty.display(&types),
                copy_kind(site.kind),
                copy_cost(site.cost),
                var,
            ));
        }
    }
    // F15: the parallel report's entries, by the node of the function that holds the call.
    for entry in crate::parallel_report::entries(module, &ReportSources(table)) {
        let (Some(root), Some(span)) = (&roots[entry.function], table.locate(entry.span)) else {
            continue;
        };
        collected
            .entry(root.as_str())
            .or_default()
            .parallel
            .push(ParallelRow {
                span,
                kind: match entry.kind {
                    crate::parallel_report::EntryKind::Suggestion => "suggestion",
                    crate::parallel_report::EntryKind::Site => "site",
                },
                call: entry.call,
                parallel_safe: entry.parallel_safe,
                reason: entry.reason,
            });
    }
    let mut rows = BTreeMap::new();
    for node in &capture.nodes {
        if node.function.is_none()
            || !matches!(node.kind, "def" | "extern" | "test" | "bench" | "method")
        {
            continue;
        }
        let empty = Collected::default();
        let found = collected.get(node.id.as_str()).unwrap_or(&empty);
        let mut truncated = BTreeMap::new();
        if found.truncated || capture.truncated.contains(&node.id) {
            truncated.insert("nodes".to_owned(), 1);
        }
        let effect_names: Vec<String> = found.effects.keys().map(|e| (*e).to_owned()).collect();
        let effect_sites: Vec<EffectSite> = found
            .effects
            .iter()
            .filter_map(|(effect, evidence)| {
                let span = table.locate(evidence.span)?;
                let (via, left_out) = effects::via(&summary, &names, effect, evidence, ARRAY_LIMIT);
                if left_out > 0 {
                    *truncated.entry("via".to_owned()).or_default() += left_out;
                }
                Some(EffectSite {
                    effect: (*effect).to_owned(),
                    span,
                    via,
                })
            })
            .collect();
        let mut copied: Vec<(CopySiteRow, String)> = found
            .copies
            .iter()
            .filter_map(|(span, ty, kind, cost, var)| {
                Some((
                    CopySiteRow {
                        span: table.locate(*span)?,
                        ty: ty.clone(),
                        kind,
                        cost,
                    },
                    var.clone(),
                ))
            })
            .collect();
        // One site per position: the copies of the instances of a generic function coincide.
        copied.sort();
        copied.dedup_by(|a, b| a.0.span == b.0.span);
        let copy_events: Vec<OwnershipRow> = copied
            .iter()
            .map(|(site, var)| OwnershipRow {
                span: site.span.clone(),
                event: "copy",
                var: var.clone(),
                to: None,
            })
            .collect();
        let copy_sites = limit(
            copied.into_iter().map(|(site, _)| site).collect(),
            "copy_sites",
            &mut truncated,
        );
        let alloc_sites = found
            .allocations
            .iter()
            .filter_map(|(span, kind)| {
                Some(AllocSite {
                    span: table.locate(*span)?,
                    kind,
                })
            })
            .collect();
        let trap_sites: Vec<TrapSite> = found
            .traps
            .iter()
            .filter_map(|trap| {
                Some(TrapSite {
                    span: table.locate(trap.span)?,
                    kind: trap.kind,
                    proven: trap.proven,
                })
            })
            .collect();
        // A site that one instance proves and another does not is not proven.
        let mut traps: BTreeMap<(super::model::Location, &str), bool> = BTreeMap::new();
        for site in trap_sites {
            let proven = traps.entry((site.span, site.kind)).or_insert(true);
            *proven &= site.proven;
        }
        let trap_sites = traps
            .into_iter()
            .map(|((span, kind), proven)| TrapSite { span, kind, proven })
            .collect();
        let mut ownership: Vec<OwnershipRow> = capture
            .events
            .get(&node.id)
            .into_iter()
            .flatten()
            .filter_map(|event| {
                Some(OwnershipRow {
                    span: table.locate(event.span)?,
                    event: event.event,
                    var: event.var.clone(),
                    to: event.to.clone(),
                })
            })
            .collect();
        ownership.extend(copy_events);
        let loops = capture
            .loops
            .get(&node.id)
            .into_iter()
            .flatten()
            .filter_map(|(kind, span)| {
                Some(LoopRow {
                    span: table.locate(*span)?,
                    kind,
                })
            })
            .collect();
        let row = FactsRow {
            id: node.id.clone(),
            effects: effect_names,
            effect_sites,
            effects_complete: false,
            copy_sites,
            alloc_sites: limit(alloc_sites, "alloc_sites", &mut truncated),
            alloc_complete: false,
            trap_sites: limit(trap_sites, "trap_sites", &mut truncated),
            ownership: limit(ownership, "ownership", &mut truncated),
            recursion: capture.recursion.get(&node.id).copied().unwrap_or("none"),
            loops: limit(loops, "loops", &mut truncated),
            specializations: found.specializations,
            parallel: limit(found.parallel.clone(), "parallel", &mut truncated),
            truncated,
        };
        rows.insert(node.id.clone(), row);
    }
    rows
}
