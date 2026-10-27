//! `tsuzuri guide` (G21): a short English guide for AI agents that ships with the compiler, so
//! that it matches the compiler's behavior. The chapters are files of `guide/` embedded with
//! `include_str!`; their examples and the error codes that they name are checked by
//! `scripts/check-guide.mjs` and `tests/guide.rs`. The guide teaches the forms that every
//! supported edition accepts; what differs between editions is in edition blocks.

use std::fmt::Write;
use std::path::Path;

use crate::diagnostic::{Diagnostic, DiagnosticKind, Span, json_string};
use crate::syntax::Edition;

/// One chapter of the guide; `level` 0 is the card that `tsuzuri guide` prints.
pub struct Chapter {
    pub name: &'static str,
    pub level: u8,
    pub title: &'static str,
    pub text: &'static str,
}

pub const CHAPTERS: &[Chapter] = &[
    Chapter {
        name: "card",
        level: 0,
        title: "Language card: syntax, numbers, ownership, pitfalls, workflow",
        text: include_str!("../guide/card.md"),
    },
    Chapter {
        name: "language",
        level: 1,
        title: "Language: modules, declarations, patterns, type classes, builders, tasks, std map",
        text: include_str!("../guide/language.md"),
    },
    Chapter {
        name: "ownership",
        level: 1,
        title: "Ownership: moves, borrows, lifetimes, sharing, performance",
        text: include_str!("../guide/ownership.md"),
    },
    Chapter {
        name: "pitfalls",
        level: 1,
        title: "Pitfalls: wrong form, error code, right form",
        text: include_str!("../guide/pitfalls.md"),
    },
    Chapter {
        name: "workflow",
        level: 1,
        title: "Workflow: commands, diagnostics, tests, debugging",
        text: include_str!("../guide/workflow.md"),
    },
];

pub const AGENTS_TEMPLATE: &str = include_str!("../guide/AGENTS.md.tmpl");
pub const LLMS_TEMPLATE: &str = include_str!("../guide/llms.txt.tmpl");

/// The limits of D-56, in Unicode code points of the output.
pub const LIMIT_L0: usize = 6_000;
pub const LIMIT_L1: usize = 20_000;
pub const LIMIT_AGENTS: usize = 3_500;
pub const LIMIT_LLMS: usize = 1_500;
pub const LIMIT_STD_MODULE: usize = 6_000;
pub const LIMIT_STD_ALL: usize = 45_000;
pub const CHAPTER_LIMITS: &[(&str, usize)] = &[
    ("language", 8_500),
    ("ownership", 4_500),
    ("pitfalls", 4_000),
    ("workflow", 2_990),
];

const LEVEL_ONE_TITLE: &str = "Guide level 1: language, ownership, pitfalls, workflow";
const STD_TITLE: &str = "Std modules with function counts";
const STD_ALL_TITLE: &str = "Std index: every function, one line each";
const SCHEMA: &str = "tz-guide/1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Level(u8),
    Topic(String),
    List,
    Agents(String),
    LlmsTxt(String),
    Std(Option<String>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invocation {
    pub action: Action,
    pub json: bool,
    /// `--edition`: the edition whose blocks the guide shows.
    pub edition: Option<Edition>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rendered {
    pub level: u8,
    pub topic: Option<String>,
    pub title: String,
    pub text: String,
}

/// The values of a project that `AGENTS.md` and `llms.txt` name.
pub struct Values<'a> {
    pub name: &'a str,
    pub namespace: &'a str,
    pub edition: Edition,
    pub version: &'a str,
}

/// The size of a text in Unicode code points.
pub fn chars(text: &str) -> usize {
    text.chars().count()
}

fn bad(kind: DiagnosticKind, message: impl Into<String>) -> Diagnostic {
    Diagnostic::of(kind, message, Span::default())
}

fn bad_arguments(message: impl Into<String>) -> Diagnostic {
    bad(DiagnosticKind::GuideBadArguments, message)
}

/// Parses the arguments after `guide`; every error is an `E2000`.
pub fn parse_arguments(arguments: &[String]) -> Result<Invocation, Diagnostic> {
    let mut flags: Vec<&str> = Vec::new();
    let (mut level, mut topic, mut edition, mut directory) = (None, None, None, None);
    let mut std = None;
    let mut words = arguments.iter().peekable();
    while let Some(word) = words.next() {
        if !word.starts_with('-') {
            // `std [MODULE]` is the only positional form, before or after the options.
            if std.is_none() && word == "std" {
                let module = words.next_if(|next| !next.starts_with('-')).cloned();
                std = Some(module);
                continue;
            }
            return Err(bad_arguments(format!(
                "unexpected argument '{word}'; usage: tsuzuri guide [--level 0|1] [--topic NAME] [--list] [--json] [--edition YEAR]"
            )));
        }
        let flag = match word.as_str() {
            flag @ ("--json" | "--list" | "--level" | "--topic" | "--edition" | "--agents"
            | "--llms-txt") => flag,
            _ => {
                return Err(bad_arguments(format!(
                    "unknown option '{word}' for guide; supported: --level, --topic, --list, --json, --edition, --agents, --llms-txt"
                )));
            }
        };
        if flags.contains(&flag) {
            return Err(bad_arguments(format!("{flag} specified more than once")));
        }
        flags.push(flag);
        match flag {
            "--level" => {
                level = Some(match words.next().map(String::as_str) {
                    Some("0") => 0,
                    Some("1") => 1,
                    _ => return Err(bad_arguments("--level needs 0 or 1")),
                });
            }
            "--topic" => {
                topic = Some(words.next().cloned().ok_or_else(|| {
                    bad_arguments("--topic needs a name; run 'tsuzuri guide --list' for the names")
                })?);
            }
            "--edition" => {
                let value = words
                    .next()
                    .ok_or_else(|| bad_arguments("--edition needs a year such as 2027"))?;
                edition = Some(Edition::parse(value).ok_or_else(|| {
                    bad(
                        DiagnosticKind::GuideEditionUnsupported,
                        format!(
                            "unsupported edition '{value}'; supported editions: {}",
                            Edition::supported_names()
                        ),
                    )
                })?);
            }
            "--agents" | "--llms-txt" => {
                directory = Some(
                    words
                        .next_if(|next| !next.starts_with('-'))
                        .cloned()
                        .unwrap_or_else(|| ".".to_owned()),
                );
            }
            _ => {}
        }
    }
    let conflict = |flag: &str, other: &str| {
        Err(bad_arguments(format!(
            "{flag} cannot be combined with {other}"
        )))
    };
    if level.is_some() && topic.is_some() {
        return Err(bad_arguments("--level and --topic cannot be combined"));
    }
    if flags.contains(&"--list")
        && let Some(other) = flags
            .iter()
            .find(|flag| !matches!(**flag, "--list" | "--json"))
    {
        return conflict("--list", other);
    }
    for template in ["--agents", "--llms-txt"] {
        if flags.contains(&template)
            && let Some(other) = flags.iter().find(|flag| **flag != template)
        {
            return conflict(template, other);
        }
    }
    if std.is_some()
        && let Some(other) = flags.iter().find(|flag| {
            matches!(
                **flag,
                "--level" | "--topic" | "--list" | "--agents" | "--llms-txt"
            )
        })
    {
        return conflict("std", other);
    }
    let action = if let Some(module) = std {
        Action::Std(module)
    } else if flags.contains(&"--list") {
        Action::List
    } else if flags.contains(&"--agents") {
        Action::Agents(directory.unwrap_or_default())
    } else if flags.contains(&"--llms-txt") {
        Action::LlmsTxt(directory.unwrap_or_default())
    } else if let Some(topic) = topic {
        Action::Topic(topic)
    } else {
        Action::Level(level.unwrap_or(0))
    };
    Ok(Invocation {
        action,
        json: flags.contains(&"--json"),
        edition,
    })
}

/// The text of a guide file as the given edition shows it: CRLF as LF, the edition blocks of
/// other editions removed, and the generated sections filled in.
pub fn prepare(text: &str, edition: Edition) -> Result<String, String> {
    let text = text.replace("\r\n", "\n");
    let text = filter_edition(&text, edition)?;
    expand_generated(&text, &generator)
}

/// Keeps the lines between `<!--if edition=YYYY-->` and `<!--endif-->` when `YYYY` is `edition`,
/// and removes the other blocks; the marker lines are removed. Blocks do not nest.
pub fn filter_edition(text: &str, edition: Edition) -> Result<String, String> {
    let mut output = String::with_capacity(text.len());
    let mut block: Option<bool> = None;
    for (number, line) in text.split_inclusive('\n').enumerate() {
        let trimmed = line.trim_end_matches('\n');
        if let Some(year) = trimmed
            .strip_prefix("<!--if edition=")
            .and_then(|rest| rest.strip_suffix("-->"))
        {
            if block.is_some() {
                return Err(format!("line {}: nested edition block", number + 1));
            }
            let year = Edition::parse(year)
                .ok_or_else(|| format!("line {}: unsupported edition '{year}'", number + 1))?;
            block = Some(year == edition);
            continue;
        }
        if trimmed == "<!--endif-->" {
            if block.take().is_none() {
                return Err(format!("line {}: endif without a block", number + 1));
            }
            continue;
        }
        if block != Some(false) {
            output.push_str(line);
        }
    }
    if block.is_some() {
        return Err("an edition block is not closed".to_owned());
    }
    Ok(output)
}

/// Replaces the body between `<!-- generated:NAME -->` and `<!-- /generated:NAME -->` with
/// `generate(NAME)`; the marker lines are removed.
pub fn expand_generated(
    text: &str,
    generate: &dyn Fn(&str) -> Option<String>,
) -> Result<String, String> {
    let mut output = String::with_capacity(text.len());
    let mut open: Option<&str> = None;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_end_matches('\n');
        if let Some(name) = trimmed
            .strip_prefix("<!-- generated:")
            .and_then(|rest| rest.strip_suffix(" -->"))
        {
            if open.is_some() {
                return Err(format!("nested generated section '{name}'"));
            }
            output.push_str(
                &generate(name).ok_or_else(|| format!("unknown generated section '{name}'"))?,
            );
            open = Some(name);
            continue;
        }
        if let Some(name) = trimmed
            .strip_prefix("<!-- /generated:")
            .and_then(|rest| rest.strip_suffix(" -->"))
        {
            if open != Some(name) {
                return Err(format!("unmatched end of generated section '{name}'"));
            }
            open = None;
            continue;
        }
        if open.is_none() {
            output.push_str(line);
        }
    }
    match open {
        Some(name) => Err(format!("generated section '{name}' is not closed")),
        None => Ok(output),
    }
}

/// The generated sections of the guide: `pitfalls` is the help of each hint of the hint
/// database (G23), with the code of its diagnostic.
pub fn generator(name: &str) -> Option<String> {
    match name {
        "pitfalls" => {
            let mut text = String::new();
            for hint in crate::hints::TABLE {
                let code = crate::diagnostic_kinds::DiagnosticKind::from_id(hint.kind)
                    .map_or("", |kind| kind.code());
                let _ = writeln!(text, "- {code}: {}", hint.help);
            }
            Some(text)
        }
        _ => None,
    }
}

fn chapter(name: &str) -> Option<&'static Chapter> {
    CHAPTERS.iter().find(|chapter| chapter.name == name)
}

/// The text of a chapter for `edition`. The files are checked by the tests, so a malformed
/// edition block or generated section only shows up during development.
fn chapter_text(chapter: &Chapter, edition: Edition) -> String {
    prepare(chapter.text, edition)
        .unwrap_or_else(|error| format!("guide/{}.md: {error}\n", chapter.name))
}

fn unknown_topic(name: &str) -> Diagnostic {
    bad(
        DiagnosticKind::GuideUnknownTopic,
        format!("unknown guide topic '{name}'; run 'tsuzuri guide --list' for the names"),
    )
}

/// The text of `--level`, `--topic`, and `std`.
pub fn render(action: &Action, edition: Edition) -> Result<Rendered, Diagnostic> {
    let rendered = |level, topic: Option<&str>, title: &str, text: String| Rendered {
        level,
        topic: topic.map(str::to_owned),
        title: title.to_owned(),
        text,
    };
    match action {
        Action::Level(0) => render(&Action::Topic("card".to_owned()), edition),
        Action::Level(_) => {
            let texts: Vec<String> = CHAPTERS
                .iter()
                .filter(|chapter| chapter.level == 1)
                .map(|chapter| chapter_text(chapter, edition))
                .collect();
            if texts.is_empty() {
                return Err(bad(
                    DiagnosticKind::GuideUnknownTopic,
                    "this build has no level 1 guide",
                ));
            }
            Ok(rendered(1, None, LEVEL_ONE_TITLE, texts.join("\n")))
        }
        Action::Topic(name) => match name.as_str() {
            "std" => render(&Action::Std(None), edition),
            "std-all" => Ok(rendered(
                1,
                Some("std-all"),
                STD_ALL_TITLE,
                crate::guide_std::render_all(edition),
            )),
            _ => {
                let chapter = chapter(name).ok_or_else(|| unknown_topic(name))?;
                Ok(rendered(
                    chapter.level,
                    Some(chapter.name),
                    chapter.title,
                    chapter_text(chapter, edition),
                ))
            }
        },
        Action::Std(None) => Ok(rendered(
            1,
            Some("std"),
            STD_TITLE,
            crate::guide_std::render_modules(edition),
        )),
        Action::Std(Some(module)) => {
            let text = crate::guide_std::render_module(module, edition).ok_or_else(|| {
                bad(
                    DiagnosticKind::GuideUnknownStdModule,
                    format!("unknown std module '{module}'; run 'tsuzuri guide std' for the list"),
                )
            })?;
            Ok(Rendered {
                level: 1,
                topic: Some(format!("std:{module}")),
                title: format!("Std module {module}"),
                text,
            })
        }
        Action::List | Action::Agents(_) | Action::LlmsTxt(_) => {
            Err(bad_arguments("this action is not a guide text"))
        }
    }
}

/// Every topic as (name, level, chars, title) for `edition`.
fn topics(edition: Edition) -> Vec<(&'static str, u8, usize, &'static str)> {
    let mut topics: Vec<_> = CHAPTERS
        .iter()
        .map(|chapter| {
            (
                chapter.name,
                chapter.level,
                chars(&chapter_text(chapter, edition)),
                chapter.title,
            )
        })
        .collect();
    topics.push((
        "std",
        1,
        chars(&crate::guide_std::render_modules(edition)),
        STD_TITLE,
    ));
    topics.push((
        "std-all",
        1,
        chars(&crate::guide_std::render_all(edition)),
        STD_ALL_TITLE,
    ));
    topics
}

/// `tsuzuri guide --list`.
pub fn list_text(edition: Edition) -> String {
    let mut text = String::new();
    for (name, level, chars, title) in topics(edition) {
        let _ = writeln!(text, "{name:<10} L{level} {chars:>6}  {title}");
    }
    text
}

/// `tsuzuri guide --list --json`.
pub fn list_json(edition: Edition) -> String {
    let topics: Vec<String> = topics(edition)
        .into_iter()
        .map(|(name, level, chars, title)| {
            format!(
                "{{\"name\":{},\"level\":{level},\"chars\":{chars},\"title\":{}}}",
                json_string(name),
                json_string(title)
            )
        })
        .collect();
    format!(
        "{{\"schema\":{},\"compiler\":{},\"topics\":[{}]}}\n",
        json_string(SCHEMA),
        json_string(env!("CARGO_PKG_VERSION")),
        topics.join(",")
    )
}

/// A rendered text as the one line of `--json`.
pub fn json_text(rendered: &Rendered, edition: Edition) -> String {
    format!(
        "{{\"schema\":{},\"compiler\":{},\"edition\":{},\"level\":{},\"topic\":{},\"title\":{},\"chars\":{},\"text\":{}}}\n",
        json_string(SCHEMA),
        json_string(env!("CARGO_PKG_VERSION")),
        json_string(&edition.to_string()),
        rendered.level,
        rendered
            .topic
            .as_deref()
            .map_or_else(|| "null".to_owned(), json_string),
        json_string(&rendered.title),
        chars(&rendered.text),
        json_string(&rendered.text)
    )
}

/// The template as the project's edition shows it, with its four words replaced, in this order,
/// once each.
pub fn render_template(template: &str, values: &Values<'_>) -> String {
    prepare(template, values.edition)
        .unwrap_or_else(|error| format!("template: {error}\n"))
        .replace("{{name}}", values.name)
        .replace("{{namespace}}", values.namespace)
        .replace("{{edition}}", &values.edition.to_string())
        .replace("{{version}}", values.version)
}

pub fn agents_md(values: &Values<'_>) -> String {
    render_template(AGENTS_TEMPLATE, values)
}

pub fn llms_txt(values: &Values<'_>) -> String {
    render_template(LLMS_TEMPLATE, values)
}

/// The name, namespace, and edition of the manifest `Tsuzuri.toml` of `directory`.
pub fn project_values(directory: &Path) -> Result<(String, String, Edition), Diagnostic> {
    let missing = || {
        bad(
            DiagnosticKind::GuideManifestMissing,
            format!(
                "no readable Tsuzuri.toml in '{}'; run it in a project directory",
                directory.display()
            ),
        )
    };
    // The manifest reader takes only a regular file, with a bounded read.
    let (_, manifest) =
        crate::driver::read_manifest(&directory.join("Tsuzuri.toml")).map_err(|_| missing())?;
    // The compiler keeps namespaces dotted; a manifest and a source write them with `::`.
    Ok((
        manifest.name,
        manifest.namespace.replace('.', "::"),
        manifest.edition,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(words: &[&str]) -> Result<Invocation, Diagnostic> {
        parse_arguments(
            &words
                .iter()
                .map(|word| (*word).to_owned())
                .collect::<Vec<_>>(),
        )
    }

    fn kind(words: &[&str]) -> &'static str {
        parse(words).unwrap_err().kind.unwrap()
    }

    #[test]
    fn parses_the_actions() {
        let action = |words: &[&str]| parse(words).unwrap().action;
        assert_eq!(action(&[]), Action::Level(0));
        assert_eq!(action(&["--level", "1"]), Action::Level(1));
        assert_eq!(
            action(&["--topic", "ownership"]),
            Action::Topic("ownership".into())
        );
        assert_eq!(action(&["--list", "--json"]), Action::List);
        assert_eq!(action(&["--agents"]), Action::Agents(".".into()));
        assert_eq!(
            action(&["--llms-txt", "app"]),
            Action::LlmsTxt("app".into())
        );
        assert_eq!(action(&["std"]), Action::Std(None));
        assert_eq!(action(&["std", "Array"]), Action::Std(Some("Array".into())));
        for words in [
            &["--json", "std", "Array"][..],
            &["std", "Array", "--json"],
            &["--edition", "2026", "std", "Vec"],
            &["std", "Vec", "--edition", "2026"],
        ] {
            let invocation = parse(words).unwrap();
            assert!(
                matches!(invocation.action, Action::Std(Some(_))),
                "{words:?}"
            );
            assert_eq!(
                (invocation.json, invocation.edition.is_some()),
                (words.contains(&"--json"), words.contains(&"--edition")),
                "{words:?}"
            );
        }
        assert!(parse(&["--json"]).unwrap().json);
        assert_eq!(
            parse(&["--edition", "2026"]).unwrap().edition,
            Edition::parse("2026")
        );
    }

    #[test]
    fn rejects_bad_arguments() {
        for words in [
            &["--level", "2"][..],
            &["--level"],
            &["--topic"],
            &["--json", "--json"],
            &["--nope"],
            &["--level", "0", "--topic", "card"],
            &["--list", "--level", "1"],
            &["--agents", "--json"],
            &["std", "--list"],
            &["std", "--agents", "app"],
            &["std", "--llms-txt", "app"],
            &["std", "Array", "--agents"],
            &["std", "Array", "std"],
            &["--json", "std", "Array", "Vec"],
            &["stray"],
        ] {
            assert_eq!(kind(words), "guide_bad_arguments", "{words:?}");
        }
        assert_eq!(kind(&["--edition", "2030"]), "guide_edition_unsupported");
        assert_eq!(
            render(&Action::Topic("nope".into()), Edition::LATEST)
                .unwrap_err()
                .kind,
            Some("guide_unknown_topic")
        );
    }

    #[test]
    fn edition_blocks_keep_only_the_chosen_edition() {
        let text = "a\n<!--if edition=2026-->\nb\n<!--endif-->\n<!--if edition=2027-->\nc\n<!--endif-->\nd\n";
        assert_eq!(
            filter_edition(text, Edition::parse("2026").unwrap()).unwrap(),
            "a\nb\nd\n"
        );
        assert_eq!(
            filter_edition(text, Edition::parse("2027").unwrap()).unwrap(),
            "a\nc\nd\n"
        );
        let latest = Edition::LATEST;
        assert!(
            filter_edition("<!--if edition=2026-->\n<!--if edition=2027-->\n", latest).is_err()
        );
        assert!(filter_edition("<!--endif-->\n", latest).is_err());
        assert!(filter_edition("<!--if edition=2026-->\nx\n", latest).is_err());
        assert!(filter_edition("<!--if edition=1999-->\n<!--endif-->\n", latest).is_err());
    }

    #[test]
    fn generated_sections_replace_their_bodies() {
        let generate = |name: &str| (name == "n").then(|| "new\n".to_owned());
        assert_eq!(
            expand_generated(
                "a\n<!-- generated:n -->\nold\n<!-- /generated:n -->\nb\n",
                &generate
            )
            .unwrap(),
            "a\nnew\nb\n"
        );
        assert!(
            expand_generated("<!-- generated:x -->\n<!-- /generated:x -->\n", &generate).is_err()
        );
        assert!(expand_generated("<!-- generated:n -->\n", &generate).is_err());
        assert!(expand_generated("<!-- /generated:n -->\n", &generate).is_err());
    }

    #[test]
    fn templates_replace_their_four_words() {
        let values = Values {
            name: "demo",
            namespace: "Acme::Demo",
            edition: Edition::LATEST,
            version: "9.9.9",
        };
        for text in [agents_md(&values), llms_txt(&values)] {
            assert!(!text.contains("{{"), "{text}");
            assert!(text.contains("demo"));
        }
        assert!(agents_md(&values).contains("compiler 9.9.9"));
        assert!(agents_md(&values).contains(&format!("edition {}", Edition::LATEST)));
    }
}
