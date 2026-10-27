use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use tsuzuri::guide;
use tsuzuri::syntax::Edition;

const CHAPTERS: [&str; 4] = ["language", "ownership", "pitfalls", "workflow"];

fn repo(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// A fresh directory with the given files.
fn root(label: &str, files: &[(&str, &str)]) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "tsuzuri-guide-{label}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    for (path, text) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    root
}

/// `(status, stdout, stderr)` of the compiler in `cwd` (a directory without a manifest when `None`).
fn tsuzuri_in(arguments: &[&str], cwd: Option<&Path>) -> (i32, String, String) {
    let empty;
    let cwd = match cwd {
        Some(cwd) => cwd,
        None => {
            empty = root("cwd", &[]);
            &empty
        }
    };
    let output = Command::new(env!("CARGO_BIN_EXE_tsuzuri"))
        .args(arguments)
        .current_dir(cwd)
        .output()
        .unwrap();
    (
        output.status.code().unwrap(),
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

fn tsuzuri(arguments: &[&str]) -> (i32, String, String) {
    tsuzuri_in(arguments, None)
}

fn editions() -> Vec<String> {
    Edition::SUPPORTED.iter().map(ToString::to_string).collect()
}

/// `text` as `edition` shows it: the blocks of other editions and the marker lines removed. A
/// small reimplementation, so that the expected text does not come from the compiler.
fn shown(text: &str, edition: &str) -> String {
    let mut keep = true;
    let mut output = String::new();
    for line in text.replace("\r\n", "\n").split_inclusive('\n') {
        let bare = line.trim_end();
        if let Some(year) = bare
            .strip_prefix("<!--if edition=")
            .and_then(|rest| rest.strip_suffix("-->"))
        {
            keep = year == edition;
        } else if bare == "<!--endif-->" {
            keep = true;
        } else if keep {
            output.push_str(line);
        }
    }
    output
}

/// The pitfalls chapter: its generated section is the hint table, one line per hint.
fn expected_chapter(name: &str, edition: &str) -> String {
    let text = shown(&read(&repo(&format!("guide/{name}.md"))), edition);
    let Some(start) = text.find("<!-- generated:pitfalls -->\n") else {
        return text;
    };
    let end = text.find("<!-- /generated:pitfalls -->\n").unwrap();
    let mut hints = String::new();
    for hint in tsuzuri::hints::TABLE {
        let code = tsuzuri::diagnostic_kinds::DiagnosticKind::from_id(hint.kind)
            .unwrap()
            .code();
        hints.push_str(&format!("- {code}: {}\n", hint.help));
    }
    format!(
        "{}{hints}{}",
        &text[..start],
        &text[end + "<!-- /generated:pitfalls -->\n".len()..]
    )
}

fn latest() -> String {
    Edition::LATEST.to_string()
}

#[test]
fn the_guide_prints_level_zero_by_default() {
    let (status, stdout, stderr) = tsuzuri(&["guide"]);
    assert_eq!((status, stderr.as_str()), (0, ""));
    assert_eq!(stdout, shown(&read(&repo("guide/card.md")), &latest()));
}

#[test]
fn level_zero_and_the_card_topic_are_the_same() {
    let (_, default, _) = tsuzuri(&["guide"]);
    assert_eq!(tsuzuri(&["guide", "--level", "0"]).1, default);
    assert_eq!(tsuzuri(&["guide", "--topic", "card"]).1, default);
}

#[test]
fn level_one_joins_the_chapters() {
    let expected: Vec<String> = CHAPTERS
        .iter()
        .map(|name| expected_chapter(name, &latest()))
        .collect();
    let (status, stdout, _) = tsuzuri(&["guide", "--level", "1"]);
    assert_eq!(status, 0);
    assert_eq!(stdout, expected.join("\n"));
}

#[test]
fn every_topic_prints_its_file() {
    for edition in editions() {
        for name in CHAPTERS {
            let (status, stdout, _) = tsuzuri(&["guide", "--topic", name, "--edition", &edition]);
            assert_eq!(status, 0, "{name}");
            assert_eq!(stdout, expected_chapter(name, &edition), "{name} {edition}");
        }
    }
}

#[test]
fn list_names_every_topic() {
    let (status, stdout, _) = tsuzuri(&["guide", "--list"]);
    assert_eq!(status, 0);
    let names: Vec<&str> = stdout
        .lines()
        .map(|line| line.split_whitespace().next().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "card",
            "language",
            "ownership",
            "pitfalls",
            "workflow",
            "std",
            "std-all"
        ]
    );
    for line in stdout.lines().take(5) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let name = fields[0];
        let text = if name == "card" {
            shown(&read(&repo("guide/card.md")), &latest())
        } else {
            expected_chapter(name, &latest())
        };
        assert_eq!(
            fields[1],
            if name == "card" { "L0" } else { "L1" },
            "{line}"
        );
        assert_eq!(fields[2], text.chars().count().to_string(), "{line}");
    }
    let (_, json, _) = tsuzuri(&["guide", "--list", "--json"]);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(json.starts_with(&format!(
        "{{\"schema\":\"tz-guide/1\",\"compiler\":\"{}\",\"topics\":[{{\"name\":\"card\",\"level\":0,\"chars\":",
        env!("CARGO_PKG_VERSION")
    )));
    assert_eq!(value["topics"].as_array().unwrap().len(), 7);
}

#[test]
fn json_output_has_the_documented_keys() {
    let (status, stdout, _) = tsuzuri(&["guide", "--json"]);
    assert_eq!(status, 0);
    assert_eq!(stdout.lines().count(), 1);
    assert!(
        stdout.starts_with(&format!(
            "{{\"schema\":\"tz-guide/1\",\"compiler\":\"{}\",\"edition\":\"{}\",\"level\":0,\"topic\":\"card\",\"title\":\"",
            env!("CARGO_PKG_VERSION"),
            latest()
        )),
        "{stdout}"
    );
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let text = value["text"].as_str().unwrap();
    assert_eq!(text, shown(&read(&repo("guide/card.md")), &latest()));
    assert_eq!(value["chars"], text.chars().count());
    let (_, level_one, _) = tsuzuri(&["guide", "--level", "1", "--json"]);
    assert!(
        level_one.contains("\"level\":1,\"topic\":null,"),
        "{level_one}"
    );
}

#[test]
fn bad_arguments_are_rejected_with_e2000() {
    let cases: &[(&[&str], &str)] = &[
        (&["guide", "--level", "2"], "--level needs 0 or 1"),
        (&["guide", "--level"], "--level needs 0 or 1"),
        (&["guide", "--topic"], "--topic needs a name"),
        (&["guide", "--topic", "nope"], "unknown guide topic 'nope'"),
        (
            &["guide", "--level", "0", "--topic", "card"],
            "--level and --topic cannot be combined",
        ),
        (
            &["guide", "--list", "--topic", "card"],
            "--list cannot be combined with --topic",
        ),
        (&["guide", "--bogus"], "unknown option '--bogus' for guide"),
        (
            &["guide", "--json", "--json"],
            "--json specified more than once",
        ),
        (
            &["guide", "--level", "1", "--level", "1"],
            "--level specified more than once",
        ),
        (
            &["guide", "--edition", "2030"],
            "unsupported edition '2030'",
        ),
        (&["guide", "std", "Nope"], "unknown std module 'Nope'"),
    ];
    for (arguments, message) in cases {
        let (status, stdout, stderr) = tsuzuri(arguments);
        assert_eq!(status, 2, "{arguments:?}");
        assert!(stdout.is_empty());
        // With --json among the arguments, the error is a JSON line too.
        let marker = if arguments.contains(&"--json") {
            "\"code\":\"E2000\""
        } else {
            "error[E2000]"
        };
        assert!(
            stderr.contains(marker) && stderr.contains(message),
            "{arguments:?}: {stderr}"
        );
        let mut json = arguments.to_vec();
        if !json.contains(&"--json") {
            json.push("--json");
        }
        let (_, _, stderr) = tsuzuri(&json);
        assert!(stderr.contains("\"code\":\"E2000\""), "{json:?}: {stderr}");
    }
}

#[test]
fn guide_sizes_stay_within_the_limits() {
    for edition in editions() {
        let card = shown(&read(&repo("guide/card.md")), &edition);
        assert!(card.chars().count() <= guide::LIMIT_L0, "card in {edition}");
        for (name, limit) in guide::CHAPTER_LIMITS {
            let text = expected_chapter(name, &edition);
            assert!(text.chars().count() <= *limit, "{name} in {edition}");
        }
        let level_one = guide::render(&guide::Action::Level(1), Edition::parse(&edition).unwrap())
            .unwrap()
            .text;
        assert!(level_one.chars().count() <= guide::LIMIT_L1, "{edition}");
    }
    for entry in std::fs::read_dir(repo("guide")).unwrap() {
        let path = entry.unwrap().path();
        let text = read(&path);
        assert!(
            text.is_ascii() && !text.contains('\r'),
            "{}",
            path.display()
        );
    }
}

/// The files of one example project, by (edition, project).
type Projects = std::collections::BTreeMap<(Option<String>, String), Vec<(String, String)>>;

/// One entry of the pitfall table: (id, snippet, codes, edition, source).
type Pitfall = (String, String, Vec<String>, Option<String>, String);

/// The ```tsuzuri fences of a guide file as (project, file, text), with the edition each is in.
fn examples(text: &str) -> Vec<(Option<String>, String, String, String)> {
    let mut found = Vec::new();
    let mut edition = None;
    let mut open: Option<(String, String, String)> = None;
    let mut block = 0;
    for line in text.lines() {
        if open.is_none() {
            if let Some(year) = line
                .strip_prefix("<!--if edition=")
                .and_then(|rest| rest.strip_suffix("-->"))
            {
                edition = Some(year.to_owned());
            } else if line == "<!--endif-->" {
                edition = None;
            } else if let Some(options) = line.strip_prefix("```tsuzuri") {
                block += 1;
                let (mut project, mut file) = (format!("block-{block}"), "Main.tz".to_owned());
                for option in options.split_whitespace() {
                    let (key, value) = option.split_once('=').unwrap();
                    match key {
                        "project" => project = value.to_owned(),
                        "file" => file = value.to_owned(),
                        "run" => {}
                        _ => panic!("unknown example option {option}"),
                    }
                }
                open = Some((project, file, String::new()));
            }
        } else if line == "```" {
            let (project, file, body) = open.take().unwrap();
            found.push((edition.clone(), project, file, body));
        } else if let Some((_, _, body)) = &mut open {
            body.push_str(line);
            body.push('\n');
        }
    }
    found
}

fn manifest(edition: &str) -> String {
    format!("[package]\nname = \"g\"\nversion = \"0.1.0\"\nedition = \"{edition}\"\n")
}

#[test]
fn the_guide_examples_type_check() {
    let mut card_examples = 0;
    for entry in std::fs::read_dir(repo("guide")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let mut projects = Projects::new();
        for (edition, project, file, body) in examples(&read(&path)) {
            projects
                .entry((edition, project))
                .or_default()
                .push((file, body));
        }
        if name == "card.md" {
            card_examples = projects.len();
        }
        for ((edition, project), files) in projects {
            for edition in edition.map_or_else(editions, |edition| vec![edition]) {
                let mut all: Vec<(&str, &str)> = files
                    .iter()
                    .map(|(file, body)| (file.as_str(), body.as_str()))
                    .collect();
                let manifest = manifest(&edition);
                all.push(("Tsuzuri.toml", &manifest));
                let directory = root("example", &all);
                let (status, _, stderr) = tsuzuri_in(&["check", directory.to_str().unwrap()], None);
                assert_eq!(status, 0, "{name} {project} {edition}: {stderr}");
            }
        }
    }
    assert!(card_examples >= 2);
}

#[test]
fn the_guide_names_only_existing_subcommands() {
    let (_, _, usage) = tsuzuri(&[]);
    let mut commands = std::collections::BTreeSet::new();
    for line in usage.lines() {
        if let Some(rest) = line.trim_start().strip_prefix("tsuzuri ") {
            let word = rest.split_whitespace().next().unwrap_or("");
            commands.insert(if word == "[build]" { "build" } else { word }.to_owned());
        }
    }
    assert!(commands.contains("guide") && commands.contains("explain") && commands.contains("fix"));
    for entry in std::fs::read_dir(repo("guide")).unwrap() {
        let path = entry.unwrap().path();
        let text: String = read(&path)
            .lines()
            .filter(|line| !line.starts_with("```"))
            .collect::<Vec<_>>()
            .join("\n");
        for (index, _) in text.match_indices("`tsuzuri ") {
            let word: String = text[index + "`tsuzuri ".len()..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect();
            assert!(
                commands.contains(&word),
                "{}: tsuzuri {word} is not a subcommand",
                path.display()
            );
        }
    }
}

/// The entries of the pitfall table: (id, snippet, codes, edition, source).
fn pitfalls() -> Vec<Pitfall> {
    let text = read(&repo("tests/fixtures/guide/pitfalls.txt"));
    let mut entries = Vec::new();
    for part in text.split("\n## ").skip(1) {
        let (header, source) = part.split_once('\n').unwrap();
        let (left, right) = header.split_once(" => ").unwrap();
        let (id, snippet) = left.split_once(' ').unwrap();
        let (codes, edition) = match right.split_once(" @") {
            Some((codes, edition)) => (codes, Some(edition.to_owned())),
            None => (right, None),
        };
        entries.push((
            id.to_owned(),
            snippet.to_owned(),
            codes.split(',').map(str::to_owned).collect(),
            edition,
            source.to_owned(),
        ));
    }
    entries
}

#[test]
fn pitfall_claims_hold_and_appear_in_the_card() {
    let entries = pitfalls();
    assert!(entries.len() >= 25);
    for (id, snippet, codes, edition, source) in entries {
        for edition in edition.map_or_else(editions, |edition| vec![edition]) {
            let sources: Vec<String> = if source.contains("{word}") {
                snippet
                    .split(' ')
                    .map(|word| source.replace("{word}", word))
                    .collect()
            } else {
                vec![source.clone()]
            };
            for source in sources {
                let manifest = manifest(&edition);
                let directory = root(
                    "pitfall",
                    &[("Main.tz", &source), ("Tsuzuri.toml", &manifest)],
                );
                let (_, _, stderr) =
                    tsuzuri_in(&["check", directory.to_str().unwrap(), "--json"], None);
                let first = stderr
                    .lines()
                    .find_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                    .unwrap_or_else(|| panic!("{id} {edition}: no diagnostic"));
                assert!(
                    codes.iter().any(|code| first["code"] == code.as_str()),
                    "{id} in {edition}: {first}"
                );
            }
            let texts = [
                shown(&read(&repo("guide/card.md")), &edition),
                shown(&read(&repo("guide/pitfalls.md")), &edition),
            ];
            assert!(
                texts.iter().flat_map(|text| text.lines()).any(|line| {
                    line.contains(&snippet) && codes.iter().all(|code| line.contains(code.as_str()))
                }),
                "{id}: no line of the guide in {edition} holds '{snippet}' and {codes:?}"
            );
        }
    }
}

#[test]
fn every_pitfall_line_has_a_verification_entry() {
    let snippets: Vec<String> = pitfalls().into_iter().map(|entry| entry.1).collect();
    let text = read(&repo("guide/pitfalls.md"));
    // The generated section is the hint table, which tests/hints.rs verifies.
    let manual = &text[..text.find("<!-- generated:").unwrap_or(text.len())];
    for line in manual.lines().filter(|line| line.starts_with("- ")) {
        // A runtime claim ("compiles, then traps") is checked by scripts/check-guide.mjs.
        assert!(
            line.contains("then traps")
                || snippets
                    .iter()
                    .any(|snippet| line.contains(snippet.as_str())),
            "no verification entry for: {line}"
        );
    }
}

#[test]
fn the_checker_rejects_a_broken_example() {
    let Ok(output) = Command::new("node")
        .arg(repo("scripts/check-guide.mjs"))
        .env(
            "TSUZURI_GUIDE_EXAMPLES",
            repo("tests/fixtures/guide_broken"),
        )
        .env("TSUZURI_BIN", env!("CARGO_BIN_EXE_tsuzuri"))
        .output()
    else {
        eprintln!("skipped: node is not available");
        return;
    };
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("card.md"), "{stderr}");
}

const MAIN_TZ: &str = "namespace Acme::Try\n\ndef main :: unit -> i32 = \\() ->\n    do! IO.writeln \"Hello, Tsuzuri!\"\n    0\n\ntest \"adds numbers\" = assert (1 + 2 == 3)\n";

#[test]
fn new_writes_the_agent_files() {
    let base = root("new", &[]);
    let directory = base.join("try");
    let (status, stdout, _) = tsuzuri_in(&["new", "try", "--namespace", "Acme::Try"], Some(&base));
    assert_eq!(status, 0);
    let created: Vec<&str> = stdout.lines().collect();
    let names = [
        "Tsuzuri.toml",
        "Main.tz",
        ".gitignore",
        "AGENTS.md",
        "llms.txt",
    ];
    assert_eq!(created.len(), 5, "{stdout}");
    for (line, name) in created.iter().zip(names) {
        assert!(
            line.starts_with("Created ") && line.ends_with(name),
            "{line}"
        );
    }
    let agents = read(&directory.join("AGENTS.md"));
    let values = guide::Values {
        name: "try",
        namespace: "Acme::Try",
        edition: Edition::LATEST,
        version: env!("CARGO_PKG_VERSION"),
    };
    assert_eq!(agents, guide::agents_md(&values));
    assert_eq!(read(&directory.join("llms.txt")), guide::llms_txt(&values));
    assert!(agents.contains("Acme::Try") && agents.contains(&latest()) && !agents.contains("{{"));
}

#[test]
fn new_without_agent_files_keeps_the_old_output() {
    let base = root("new-plain", &[]);
    let (status, stdout, _) = tsuzuri_in(
        &["new", "try", "--namespace", "Acme::Try", "--no-agent-files"],
        Some(&base),
    );
    assert_eq!(status, 0);
    assert_eq!(stdout.lines().count(), 3, "{stdout}");
    let directory = base.join("try");
    assert_eq!(read(&directory.join("Main.tz")), MAIN_TZ);
    assert_eq!(read(&directory.join(".gitignore")), ".tsuzuri/\n");
    assert_eq!(
        read(&directory.join("Tsuzuri.toml")),
        format!(
            "[package]\nname = \"try\"\nversion = \"0.1.0\"\nedition = \"{}\"\nnamespace = \"Acme::Try\"\n",
            latest()
        )
    );
    assert!(!directory.join("AGENTS.md").exists() && !directory.join("llms.txt").exists());
    let (status, _, stderr) = tsuzuri_in(
        &["new", "again", "--no-agent-files", "--no-agent-files"],
        Some(&base),
    );
    assert_eq!(status, 2);
    assert!(stderr.contains("E2000"));
}

#[test]
fn rendered_templates_stay_within_the_limits() {
    for edition in Edition::SUPPORTED {
        let values = guide::Values {
            name: "hello-world",
            namespace: "HelloWorld",
            edition: *edition,
            version: "0.1.0",
        };
        let agents = guide::agents_md(&values);
        let llms = guide::llms_txt(&values);
        assert!(agents.chars().count() <= guide::LIMIT_AGENTS, "{edition}");
        assert!(llms.chars().count() <= guide::LIMIT_LLMS, "{edition}");
        for text in [&agents, &llms] {
            assert!(!text.contains("{{") && text.is_ascii() && !text.contains("<!--"));
        }
    }
    assert!(guide::AGENTS_TEMPLATE.chars().count() <= guide::LIMIT_AGENTS);
    assert!(guide::LLMS_TEMPLATE.chars().count() <= guide::LIMIT_LLMS);
}

#[test]
fn guide_agents_prints_the_template_for_a_project() {
    let base = root("agents", &[]);
    assert_eq!(tsuzuri_in(&["new", "proj"], Some(&base)).0, 0);
    let directory = base.join("proj");
    let (status, stdout, _) = tsuzuri_in(&["guide", "--agents", "proj"], Some(&base));
    assert_eq!(status, 0);
    assert_eq!(stdout, read(&directory.join("AGENTS.md")));
    let (_, stdout, _) = tsuzuri_in(&["guide", "--llms-txt", "proj"], Some(&base));
    assert_eq!(stdout, read(&directory.join("llms.txt")));
    let (status, stdout, _) = tsuzuri_in(&["guide", "--agents"], Some(&directory));
    assert_eq!(status, 0);
    assert_eq!(stdout, read(&directory.join("AGENTS.md")));
}

#[test]
fn guide_agents_requires_a_manifest_and_rejects_other_flags() {
    let (status, _, stderr) = tsuzuri(&["guide", "--agents"]);
    assert_eq!(status, 1);
    assert!(stderr.contains("E2000") && stderr.contains("no readable Tsuzuri.toml"));
    for arguments in [
        &["guide", "--agents", "--json"][..],
        &["guide", "--agents", "--level", "0"],
    ] {
        assert_eq!(tsuzuri(arguments).0, 2, "{arguments:?}");
    }
}

#[test]
fn guide_std_lists_every_module() {
    let (status, stdout, _) = tsuzuri(&["guide", "std"]);
    assert_eq!(status, 0);
    let lines: Vec<&str> = stdout
        .lines()
        .filter(|line| line.contains(" functions"))
        .collect();
    let names: Vec<&str> = lines
        .iter()
        .map(|line| line.split_whitespace().next().unwrap())
        .collect();
    for (path, _) in tsuzuri::stdlib::SOURCES {
        let module = Path::new(path).file_stem().unwrap().to_str().unwrap();
        assert!(names.contains(&module), "{module} is missing");
    }
    for module in ["Int", "Rc", "Arc", "prelude"] {
        assert!(names.contains(&module), "{module} is missing");
    }
    assert_eq!(names.last(), Some(&"prelude"));
    let line = |name: &str| {
        *lines
            .iter()
            .find(|line| line.starts_with(&format!("{name} ")))
            .unwrap()
    };
    assert!(line("Json").ends_with("(opt-in)"));
    assert!(!line("Array").ends_with("(opt-in)"));
}

#[test]
fn guide_std_page_has_one_line_per_function() {
    let (status, stdout, _) = tsuzuri(&["guide", "std", "Array"]);
    assert_eq!(status, 0);
    let functions: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("Array."))
        .collect();
    assert!(functions.iter().all(|line| line.contains(" :: ")));
    let mut sorted = functions.clone();
    sorted.sort_unstable();
    assert_eq!(functions, sorted);
    // A page longer than 6,000 characters ends with a line that points at std-all, which then
    // lists the module in full.
    let (_, all, _) = tsuzuri(&["guide", "--topic", "std-all"]);
    let reachable: Vec<&str> = functions
        .iter()
        .copied()
        .chain(all.lines().filter(|line| line.starts_with("Array.")))
        .collect();
    if stdout
        .lines()
        .last()
        .is_some_and(|line| line.starts_with("... "))
    {
        assert!(
            stdout.ends_with("more: tsuzuri guide --topic std-all\n"),
            "{stdout}"
        );
    }
    let source = read(&repo("std/Array.tz"));
    for line in source.lines() {
        if let Some(rest) = line.strip_prefix("def ") {
            let name = rest.split_whitespace().next().unwrap();
            if name.starts_with(|c: char| c.is_ascii_lowercase()) && !name.contains("__") {
                assert!(
                    reachable
                        .iter()
                        .any(|line| line.starts_with(&format!("Array.{name} :: "))),
                    "Array.{name} is in neither its page nor std-all"
                );
            }
        }
    }
}

#[test]
fn guide_std_pages_stay_within_their_limits() {
    let (_, list, _) = tsuzuri(&["guide", "std"]);
    for line in list.lines().filter(|line| line.contains(" functions")) {
        let module = line.split_whitespace().next().unwrap();
        let (status, page, _) = tsuzuri(&["guide", "std", module]);
        assert_eq!(status, 0, "{module}");
        assert!(page.chars().count() <= guide::LIMIT_STD_MODULE, "{module}");
        assert!(page.is_ascii(), "{module}");
    }
    let (_, all, _) = tsuzuri(&["guide", "--topic", "std-all"]);
    assert!(all.chars().count() <= guide::LIMIT_STD_ALL);
    let (_, json, _) = tsuzuri(&["guide", "--list", "--json"]);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let listed = value["topics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|topic| topic["name"] == "std-all")
        .unwrap()["chars"]
        .as_u64()
        .unwrap();
    assert_eq!(listed as usize, all.chars().count());
}

#[test]
fn guide_std_is_deterministic_and_unknown_modules_fail() {
    assert_eq!(
        tsuzuri(&["guide", "--topic", "std-all"]),
        tsuzuri(&["guide", "--topic", "std-all"])
    );
    let (_, json, _) = tsuzuri(&["guide", "std", "Array", "--json"]);
    assert!(json.contains("\"topic\":\"std:Array\""), "{json}");
}

#[test]
fn guide_edition_flag_selects_the_edition() {
    assert_eq!(
        tsuzuri(&["guide", "--edition", &latest()]).1,
        tsuzuri(&["guide"]).1
    );
    for edition in editions() {
        let (status, stdout, _) = tsuzuri(&["guide", "--edition", &edition]);
        assert_eq!(status, 0);
        assert_eq!(stdout, shown(&read(&repo("guide/card.md")), &edition));
    }
    // The edition of the project in the working directory.
    let project = root("edition", &[("Tsuzuri.toml", &manifest("2026"))]);
    assert_eq!(
        tsuzuri_in(&["guide"], Some(&project)).1,
        shown(&read(&repo("guide/card.md")), "2026")
    );
    let (status, _, stderr) = tsuzuri(&["guide", "--edition", "2030"]);
    assert_eq!(status, 2);
    assert!(stderr.contains(&format!(
        "supported editions: {}",
        Edition::supported_names()
    )));
}

#[test]
fn every_guide_file_filters_for_every_supported_edition() {
    for entry in std::fs::read_dir(repo("guide")).unwrap() {
        let path = entry.unwrap().path();
        let text = read(&path);
        for edition in Edition::SUPPORTED {
            let prepared = guide::prepare(&text, *edition)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            assert!(!prepared.contains("<!--"), "{}", path.display());
        }
    }
}
