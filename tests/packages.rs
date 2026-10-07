//! Git dependencies, `Tsuzuri.lock`, `tsuzuri fetch`, and offline builds (E10).
//!
//! Every repository is a local bare repository under the test's own scratch
//! directory, reached with a `file:///` url: no test uses the network. Fixture
//! commits are written with `hash-object`, `mktree`, and `commit-tree`, so trees
//! may hold paths that a working tree on this file system could not.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use tsuzuri::package::{
    DependencySource, GIT_FORM, GIT_REV_RULE, GIT_URL_RULE, LockEntry, content_sha256, parse_lock,
    parse_manifest, render_lock,
};

const REV: &str = "0123456789abcdef0123456789abcdef01234567";
const GEOMETRY: &str = "[package]\nname = \"geometry-core\"\nversion = \"0.1.0\"\n";
const POINT: &str = "def value :: i64\nfn value = 42\n";

/// A scratch directory below Cargo's target directory, removed when the test passes.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "packages-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("gitconfig"), "").unwrap();
        Self(root)
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.0.join(relative)
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.path(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn cache(&self) -> PathBuf {
        self.path("cache")
    }

    fn store(&self, sha256: &str) -> PathBuf {
        self.cache().join("packages/git").join(sha256)
    }

    /// Runs the compiler with this scratch directory's cache.
    fn tsuzuri(&self, arguments: &[&str]) -> Output {
        self.tsuzuri_with(arguments, &[])
    }

    fn tsuzuri_with(&self, arguments: &[&str], environment: &[(&str, &Path)]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tsuzuri"));
        command
            .args(arguments)
            .env("TSUZURI_CACHE_DIR", self.cache());
        for (key, value) in environment {
            command.env(key, value);
        }
        command.output().unwrap()
    }

    /// A bare repository `repos/<name>.git`.
    fn repository(&self, name: &str) -> Repository {
        let repository = Repository {
            directory: self.path(&format!("repos/{name}.git")),
            config: self.path("gitconfig"),
        };
        fs::create_dir_all(&repository.directory).unwrap();
        repository.git(&["init", "--bare", "--quiet"], b"");
        repository
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

/// A fixture repository. Its git commands never read the user's configuration and
/// always name the repository, so they cannot reach the repository of this checkout.
struct Repository {
    directory: PathBuf,
    config: PathBuf,
}

/// One tree entry: path, mode, object type, and object id.
type Entry = (String, &'static str, &'static str, String);

impl Repository {
    fn git(&self, arguments: &[&str], input: &[u8]) -> String {
        let mut command = Command::new("git");
        for (key, _) in std::env::vars_os() {
            if key
                .to_string_lossy()
                .to_ascii_uppercase()
                .starts_with("GIT_")
            {
                command.env_remove(key);
            }
        }
        let mut child = command
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", &self.config)
            .env("GIT_AUTHOR_NAME", "Tsuzuri Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.org")
            .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
            .env("GIT_COMMITTER_NAME", "Tsuzuri Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.org")
            .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
            .arg("-c")
            .arg("init.defaultBranch=main")
            .arg(format!("--git-dir={}", self.directory.display()))
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let input = input.to_vec();
        let writer = std::thread::spawn(move || std::io::Write::write_all(&mut stdin, &input));
        let output = child.wait_with_output().unwrap();
        writer.join().unwrap().unwrap();
        assert!(
            output.status.success(),
            "git {arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn url(&self) -> String {
        let path = self.directory.to_string_lossy().replace('\\', "/");
        if path.starts_with('/') {
            format!("file://{path}")
        } else {
            format!("file:///{path}")
        }
    }

    fn blob(&self, bytes: &[u8]) -> String {
        self.git(&["hash-object", "-w", "--stdin"], bytes)
    }

    /// Writes nested trees for `entries` with `mktree`, which checks no path rules.
    fn tree(&self, entries: &[Entry]) -> String {
        let mut here = Vec::new();
        let mut directories = BTreeMap::<String, Vec<Entry>>::new();
        for (path, mode, kind, id) in entries {
            match path.split_once('/') {
                Some((directory, rest)) => directories
                    .entry(directory.to_owned())
                    .or_default()
                    .push((rest.to_owned(), mode, kind, id.clone())),
                None => here.push((path.clone(), *mode, *kind, id.clone())),
            }
        }
        for (directory, entries) in directories {
            let id = self.tree(&entries);
            here.push((directory, "040000", "tree", id));
        }
        let input: Vec<u8> = here
            .iter()
            .flat_map(|(name, mode, kind, id)| {
                let mut line = format!("{mode} {kind} {id}\t").into_bytes();
                line.extend(name.as_bytes());
                line.push(0);
                line
            })
            .collect();
        self.git(&["mktree", "-z"], &input)
    }

    /// Commits the entries on top of `main` and returns the commit id.
    fn commit_entries(&self, entries: &[Entry]) -> String {
        let tree = self.tree(entries);
        let parent = Command::new("git")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", &self.config)
            .arg(format!("--git-dir={}", self.directory.display()))
            .args(["rev-parse", "--verify", "--quiet", "refs/heads/main"])
            .output()
            .unwrap();
        let parent = String::from_utf8(parent.stdout).unwrap().trim().to_owned();
        let mut arguments = vec!["commit-tree", tree.as_str(), "-m", "fixture"];
        if !parent.is_empty() {
            arguments.extend(["-p", parent.as_str()]);
        }
        let commit = self.git(&arguments, b"");
        self.git(&["update-ref", "refs/heads/main", &commit], b"");
        commit
    }

    /// Commits regular files.
    fn commit(&self, files: &[(&str, &[u8])]) -> String {
        let entries: Vec<Entry> = files
            .iter()
            .map(|(path, bytes)| ((*path).to_owned(), "100644", "blob", self.blob(bytes)))
            .collect();
        self.commit_entries(&entries)
    }
}

/// The content hash of files given as text, for expected lockfiles.
fn sha256_of(files: &[(&str, &str)]) -> String {
    content_sha256(
        &files
            .iter()
            .map(|(path, text)| ((*path).to_owned(), text.as_bytes()))
            .collect::<BTreeMap<_, _>>(),
    )
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[track_caller]
fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        stderr(output)
    );
}

#[track_caller]
fn assert_error(output: &Output, code: &str, message: &str) {
    let text = stderr(output);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(text.contains(&format!("error[{code}]")), "{code}: {text}");
    assert!(text.contains(message), "{message}: {text}");
}

fn app_manifest(dependencies: &str) -> String {
    format!("[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\n{dependencies}")
}

fn git_dependency(name: &str, url: &str, rev: &str) -> String {
    format!("{name} = {{ git = \"{url}\", rev = \"{rev}\" }}\n")
}

#[test]
fn manifest_accepts_git_dependencies() {
    let manifest = parse_manifest(
        &format!(
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\ngeometry-core = {{ git = \"https://example.org/geometry-core.git\", rev = \"{REV}\" }}\nlocal-util = {{ path = \"../local-util\" }}\ntight = {{git=\"file:///srv/git/tight.git\",rev=\"{REV}\"}}\n\ttabbed\t=\t{{\tgit\t=\t\"https://example.org/a/b/c.git\"\t,\trev\t=\t\"{}\"\t}}\t# pinned\n",
            "f".repeat(40)
        ),
        0,
    )
    .unwrap();
    let git = |url: &str, rev: &str| DependencySource::Git {
        url: url.to_owned(),
        rev: rev.to_owned(),
    };
    assert_eq!(
        manifest.dependencies["geometry-core"].source,
        git("https://example.org/geometry-core.git", REV)
    );
    assert_eq!(
        manifest.dependencies["local-util"].source,
        DependencySource::Path(PathBuf::from("../local-util"))
    );
    assert_eq!(
        manifest.dependencies["tight"].source,
        git("file:///srv/git/tight.git", REV)
    );
    assert_eq!(
        manifest.dependencies["tabbed"].source,
        git("https://example.org/a/b/c.git", &"f".repeat(40))
    );
    assert!(!manifest.dependencies["geometry-core"].native);
    // The longest accepted url has 2048 bytes.
    let long = format!("https://example.org/{}", "a".repeat(2048 - 20));
    assert_eq!(long.len(), 2048);
    let manifest = parse_manifest(&app_manifest(&git_dependency("long", &long, REV)), 0).unwrap();
    assert_eq!(manifest.dependencies["long"].source, git(&long, REV));
}

#[test]
fn manifest_rejects_invalid_git_dependencies() {
    let url = "https://example.org/r.git";
    let too_long = format!("https://example.org/{}", "a".repeat(2049 - 20));
    let upper = REV.to_ascii_uppercase();
    let wide = "a".repeat(64);
    let cases = [
        (format!("{{ git = \"{url}\" }}"), GIT_FORM),
        (format!("{{ rev = \"{REV}\", git = \"{url}\" }}"), GIT_FORM),
        (
            format!("{{ git = \"{url}\", branch = \"main\" }}"),
            GIT_FORM,
        ),
        (format!("{{ git = \"{url}\", tag = \"v1\" }}"), GIT_FORM),
        (format!("{{ git \"{url}\", rev = \"{REV}\" }}"), GIT_FORM),
        (
            format!("{{ git = \"{url}\", rev = \"{REV}\", native = true }}"),
            GIT_FORM,
        ),
        (format!("{{ git = \"{url}\", rev = \"{REV}\""), GIT_FORM),
        ("{ branch = \"main\" }".to_owned(), GIT_FORM),
        (
            format!("{{ git = \"{url}\", rev = \"0123456\" }}"),
            GIT_REV_RULE,
        ),
        (
            format!("{{ git = \"{url}\", rev = \"{upper}\" }}"),
            GIT_REV_RULE,
        ),
        (
            format!("{{ git = \"{url}\", rev = \"{wide}\" }}"),
            GIT_REV_RULE,
        ),
        (
            format!("{{ git = \"{url}\", rev = \"main\" }}"),
            GIT_REV_RULE,
        ),
        (
            format!("{{ git = \"http://example.org/r.git\", rev = \"{REV}\" }}"),
            GIT_URL_RULE,
        ),
        (
            format!("{{ git = \"ssh://example.org/r.git\", rev = \"{REV}\" }}"),
            GIT_URL_RULE,
        ),
        (
            format!("{{ git = \"git://example.org/r.git\", rev = \"{REV}\" }}"),
            GIT_URL_RULE,
        ),
        (
            format!("{{ git = \"git@example.org:r.git\", rev = \"{REV}\" }}"),
            GIT_URL_RULE,
        ),
        (
            format!("{{ git = \"https://user@example.org/r.git\", rev = \"{REV}\" }}"),
            GIT_URL_RULE,
        ),
        (
            format!("{{ git = \"https://user:token@example.org/r.git\", rev = \"{REV}\" }}"),
            GIT_URL_RULE,
        ),
        (
            format!("{{ git = \"https://example.org/r.git?ref=main\", rev = \"{REV}\" }}"),
            GIT_URL_RULE,
        ),
        (
            format!("{{ git = \"https://example.org/r.git#main\", rev = \"{REV}\" }}"),
            GIT_URL_RULE,
        ),
        (
            format!("{{ git = \"https://example.org/my repo.git\", rev = \"{REV}\" }}"),
            GIT_URL_RULE,
        ),
        (
            format!("{{ git = \"https://example.org\\\\r.git\", rev = \"{REV}\" }}"),
            GIT_URL_RULE,
        ),
        (
            format!("{{ git = \"https:///r.git\", rev = \"{REV}\" }}"),
            GIT_URL_RULE,
        ),
        (
            format!("{{ git = \"file://host/r.git\", rev = \"{REV}\" }}"),
            GIT_URL_RULE,
        ),
        (
            format!("{{ git = \"HTTPS://example.org/r.git\", rev = \"{REV}\" }}"),
            GIT_URL_RULE,
        ),
        (
            format!("{{ git = \"{too_long}\", rev = \"{REV}\" }}"),
            GIT_URL_RULE,
        ),
        (
            format!("{{ url = \"{url}\" }}"),
            "expected a path or git dependency",
        ),
        (
            "{ version = \"1.0.0\" }".to_owned(),
            "expected a path or git dependency",
        ),
    ];
    for (value, message) in cases {
        let source = app_manifest(&format!("dependency = {value}\n"));
        let error = parse_manifest(&source, 4).unwrap_err();
        assert_eq!(error.code, "E0002", "{value}: {}", error.message);
        assert_eq!(error.message, message, "{value}");
        assert_eq!(error.span.source, Some(4), "{value}");
        let line = source.find("dependency =").unwrap();
        assert_eq!(error.span.start, line, "{value}");
    }
}

#[test]
fn lock_renders_canonical_json_and_round_trips() {
    let entry = |git: &str, rev: char, sha: char| LockEntry {
        git: git.to_owned(),
        rev: rev.to_string().repeat(40),
        sha256: sha.to_string().repeat(64),
    };
    let entries = BTreeMap::from([
        (
            "other".to_owned(),
            entry("file:///srv/git/other.git", 'b', 'd'),
        ),
        (
            "geometry-core".to_owned(),
            entry("https://example.org/geometry-core.git", 'a', 'c'),
        ),
    ]);
    let canonical = format!(
        "{{\n  \"format\": 1,\n  \"packages\": [\n    {{\n      \"name\": \"geometry-core\",\n      \"git\": \"https://example.org/geometry-core.git\",\n      \"rev\": \"{}\",\n      \"sha256\": \"{}\"\n    }},\n    {{\n      \"name\": \"other\",\n      \"git\": \"file:///srv/git/other.git\",\n      \"rev\": \"{}\",\n      \"sha256\": \"{}\"\n    }}\n  ]\n}}\n",
        "a".repeat(40),
        "c".repeat(64),
        "b".repeat(40),
        "d".repeat(64)
    );
    assert_eq!(render_lock(&entries), canonical);
    assert_eq!(parse_lock(&canonical, 0).unwrap(), entries);
    // Key order, package order, and whitespace do not matter when reading.
    let shuffled = format!(
        "{{\"packages\":[{{\"sha256\":\"{}\",\"rev\":\"{}\",\"git\":\"file:///srv/git/other.git\",\"name\":\"other\"}},\r\n\t{{ \"rev\" : \"{}\" , \"name\" : \"geometry-core\" , \"sha256\" : \"{}\" , \"git\" : \"https://example.org/geometry-core.git\" }}],\"format\":1}}",
        "d".repeat(64),
        "b".repeat(40),
        "a".repeat(40),
        "c".repeat(64)
    );
    let parsed = parse_lock(&shuffled, 0).unwrap();
    assert_eq!(parsed, entries);
    assert_eq!(render_lock(&parsed), canonical);
    assert_eq!(
        render_lock(&parse_lock(&render_lock(&parsed), 0).unwrap()),
        canonical
    );
    let empty = "{\n  \"format\": 1,\n  \"packages\": []\n}\n";
    assert_eq!(render_lock(&BTreeMap::new()), empty);
    assert!(parse_lock(empty, 0).unwrap().is_empty());
    assert!(
        parse_lock("{\"format\":1,\"packages\":[]}", 0)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn lock_rejects_invalid_files() {
    let sha = "c".repeat(64);
    let package = |fields: &str| format!("{{\"format\": 1, \"packages\": [{fields}]}}");
    let valid = format!(
        "{{\"name\": \"geometry-core\", \"git\": \"https://example.org/g.git\", \"rev\": \"{REV}\", \"sha256\": \"{sha}\"}}"
    );
    let cases = [
        ("not json".to_owned(), "expected"),
        ("[]".to_owned(), "expected an object"),
        (
            format!("{{\"format\": 2, \"packages\": [{valid}]}}"),
            "format must be 1",
        ),
        (
            format!("{{\"format\": \"1\", \"packages\": [{valid}]}}"),
            "format must be 1",
        ),
        (
            format!("{{\"format\": 1.0, \"packages\": [{valid}]}}"),
            "format must be 1",
        ),
        (format!("{{\"packages\": [{valid}]}}"), "format must be 1"),
        ("{\"format\": 1}".to_owned(), "packages must be an array"),
        (
            format!("{{\"format\": 1, \"packages\": [{valid}], \"extra\": 0}}"),
            "unknown key 'extra'",
        ),
        (
            package(&valid.replace("\"sha256\"", "\"hash\"")),
            "unknown package key 'hash'",
        ),
        (
            package(&valid.replace(", \"sha256\": \"", ", \"sha\": \"")),
            "unknown package key 'sha'",
        ),
        (
            package(&format!("{valid}, {valid}")),
            "duplicate package 'geometry-core'",
        ),
        (
            package(&valid.replace(&sha, &sha[..63])),
            "invalid sha256 of 'geometry-core'",
        ),
        (
            package(&valid.replace(&sha, &sha.to_ascii_uppercase())),
            "invalid sha256 of 'geometry-core'",
        ),
        (
            package(&valid.replace(REV, &REV[..39])),
            "invalid rev of 'geometry-core'",
        ),
        (
            package(&valid.replace("https://", "http://")),
            "invalid git url of 'geometry-core'",
        ),
        (
            package(&valid.replace("geometry-core", "Geometry")),
            "invalid package name 'Geometry'",
        ),
        (
            package(&valid.replace(&format!("\"{sha}\""), "7")),
            "each package needs a string sha256",
        ),
        (package("7"), "each package must be an object"),
        (
            package(&valid.replace(
                "\"rev\":",
                &format!("\"sha256\": \"{}\", \"rev\":", "d".repeat(64)),
            )),
            "duplicate key 'sha256'",
        ),
        (
            format!("{{\"format\": 1, \"format\": 1, \"packages\": [{valid}]}}"),
            "duplicate key 'format'",
        ),
    ];
    for (text, detail) in cases {
        let error = parse_lock(&text, 2).unwrap_err();
        assert_eq!(error.code, "E2007", "{text}: {}", error.message);
        assert!(
            error
                .message
                .starts_with("Tsuzuri.lock is not a valid lockfile ("),
            "{}",
            error.message
        );
        assert!(
            error.message.contains(detail),
            "{detail}: {}",
            error.message
        );
        assert!(
            error
                .message
                .ends_with("; restore it from version control or delete it and run tsuzuri fetch"),
            "{}",
            error.message
        );
        assert_eq!(error.span.source, Some(2));
    }
    // A key that only looks like a duplicate inside a string value is fine.
    assert!(parse_lock(&package(&valid.replace("g.git", "g-sha256-rev.git")), 0).is_ok());
    let large = format!("{}{}", " ".repeat(1 << 20), package(&valid));
    let error = parse_lock(&large, 0).unwrap_err();
    assert_eq!(
        (error.code, error.message.as_str()),
        ("E1017", "Tsuzuri.lock exceeds 1 MiB")
    );
}

#[test]
fn content_sha256_matches_independent_reference() {
    // Computed with Python's hashlib from the definition: u64 little-endian
    // length-prefixed tags and values, paths in byte order.
    assert_eq!(
        sha256_of(&[("Tsuzuri.toml", GEOMETRY), ("Point.tz", POINT)]),
        "7948540d0d22602ebe527bbb228b364d248048395ffb2f5559224265d85ea6ab"
    );
    assert_eq!(
        sha256_of(&[]),
        "80e3f8d1648e11ec4df38758c1ec937c3a006bee1c4544137fe769758a0a14d3"
    );
    // Order of insertion does not matter, while every byte and path does.
    assert_eq!(
        sha256_of(&[("Point.tz", POINT), ("Tsuzuri.toml", GEOMETRY)]),
        sha256_of(&[("Tsuzuri.toml", GEOMETRY), ("Point.tz", POINT)])
    );
    assert_ne!(
        sha256_of(&[("Tsuzuri.toml", GEOMETRY), ("Points.tz", POINT)]),
        sha256_of(&[("Tsuzuri.toml", GEOMETRY), ("Point.tz", POINT)])
    );
}

/// An app that depends on a hand-made store entry of `geometry-core`.
fn offline_project(scratch: &Scratch, url: &str, files: &[(&str, &str)]) -> String {
    let sha256 = sha256_of(files);
    for (path, text) in files {
        scratch.write(&format!("cache/packages/git/{sha256}/{path}"), text);
    }
    scratch.write(
        "app/Tsuzuri.toml",
        &app_manifest(&git_dependency("geometry-core", url, REV)),
    );
    scratch.write(
        "app/Main.tz",
        "def main :: unit -> i32 = \\() -> GeometryCore::Point.value() as i32\n",
    );
    sha256
}

fn lock_text(entries: &[(&str, &str, &str, &str)]) -> String {
    render_lock(
        &entries
            .iter()
            .map(|(name, git, rev, sha256)| {
                (
                    (*name).to_owned(),
                    LockEntry {
                        git: (*git).to_owned(),
                        rev: (*rev).to_owned(),
                        sha256: (*sha256).to_owned(),
                    },
                )
            })
            .collect(),
    )
}

#[test]
fn offline_build_reads_store_and_detects_tampering() {
    let scratch = Scratch::new("offline");
    let url = "https://example.org/geometry-core.git";
    let sha256 = offline_project(
        &scratch,
        url,
        &[("Tsuzuri.toml", GEOMETRY), ("Point.tz", POINT)],
    );
    let app = scratch.path("app");
    let app = app.to_str().unwrap();
    let lock = scratch.path("app/Tsuzuri.lock");
    let missing = scratch.tsuzuri(&["check", app]);
    assert_error(
        &missing,
        "E2007",
        "Tsuzuri.lock is missing; run tsuzuri fetch to download git dependencies and record them",
    );
    assert!(
        stderr(&missing).contains("app/Tsuzuri.toml"),
        "{}",
        stderr(&missing)
    );
    fs::write(&lock, lock_text(&[("geometry-core", url, REV, &sha256)])).unwrap();
    assert_success(&scratch.tsuzuri(&["check", app]));
    // The lockfile is protected from outputs like the manifests.
    let before = fs::read(&lock).unwrap();
    let output = scratch.tsuzuri(&["build", app, "--emit", "llvm", "-o", lock.to_str().unwrap()]);
    assert!(stderr(&output).contains("E2003"), "{}", stderr(&output));
    assert_eq!(fs::read(&lock).unwrap(), before);
    // A changed source, an added source, and a changed manifest in the store.
    for (path, text) in [
        ("Point.tz", "def value :: i64\nfn value = 41\n"),
        ("Extra.tz", "def extra :: i64\nfn extra = 1\n"),
        ("Tsuzuri.toml", &format!("{GEOMETRY}# edited\n")),
    ] {
        let target = scratch.store(&sha256).join(path);
        let original = fs::read(&target).ok();
        fs::write(&target, text).unwrap();
        assert_error(
            &scratch.tsuzuri(&["check", app]),
            "E2007",
            "git dependency 'geometry-core' does not match its sha256 in Tsuzuri.lock; run tsuzuri fetch to restore it",
        );
        match original {
            Some(bytes) => fs::write(&target, bytes).unwrap(),
            None => fs::remove_file(&target).unwrap(),
        }
    }
    assert_success(&scratch.tsuzuri(&["check", app]));
    // Files that module discovery skips do not change the content.
    scratch.write(
        &format!("cache/packages/git/{sha256}/.hidden/Skip.tz"),
        "broken",
    );
    scratch.write(&format!("cache/packages/git/{sha256}/README.md"), "notes");
    assert_success(&scratch.tsuzuri(&["check", app]));
    let not_recorded = "Tsuzuri.lock does not record git dependency 'geometry-core' at this url and rev; run tsuzuri fetch";
    for entries in [
        vec![("other", url, REV, sha256.as_str())],
        vec![("geometry-core", url, &"1".repeat(40), sha256.as_str())],
        vec![(
            "geometry-core",
            "https://example.org/fork.git",
            REV,
            sha256.as_str(),
        )],
    ] {
        fs::write(&lock, lock_text(&entries)).unwrap();
        assert_error(&scratch.tsuzuri(&["check", app]), "E2007", not_recorded);
    }
    fs::write(
        &lock,
        lock_text(&[("geometry-core", url, REV, &"e".repeat(64))]),
    )
    .unwrap();
    assert_error(
        &scratch.tsuzuri(&["check", app]),
        "E2007",
        "git dependency 'geometry-core' is not downloaded; run tsuzuri fetch",
    );
    fs::write(&lock, lock_text(&[("geometry-core", url, REV, &sha256)])).unwrap();
    let empty = scratch.path("empty-cache");
    fs::create_dir(&empty).unwrap();
    assert_error(
        &scratch.tsuzuri_with(&["check", app], &[("TSUZURI_CACHE_DIR", &empty)]),
        "E2007",
        "git dependency 'geometry-core' is not downloaded; run tsuzuri fetch",
    );
    assert_error(
        &scratch.tsuzuri_with(&["check", app], &[("TSUZURI_CACHE_DIR", Path::new(""))]),
        "E2007",
        "no package store is available; set TSUZURI_CACHE_DIR and run tsuzuri fetch",
    );
    fs::write(&lock, "{\"format\": 1}").unwrap();
    let output = scratch.tsuzuri(&["check", app, "--json"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("\"code\":\"E2007\""),
        "{}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("Tsuzuri.lock"),
        "{}",
        stderr(&output)
    );
    // A lockfile is read even when no dependency needs it.
    scratch.write("app/Tsuzuri.toml", &app_manifest(""));
    scratch.write("app/Main.tz", "42\n");
    assert_error(
        &scratch.tsuzuri(&["check", app]),
        "E2007",
        "Tsuzuri.lock is not a valid lockfile",
    );
    fs::write(&lock, lock_text(&[])).unwrap();
    assert_success(&scratch.tsuzuri(&["check", app]));
    #[cfg(unix)]
    {
        fs::write(&lock, lock_text(&[("geometry-core", url, REV, &sha256)])).unwrap();
        scratch.write(
            "app/Tsuzuri.toml",
            &app_manifest(&git_dependency("geometry-core", url, REV)),
        );
        scratch.write(
            "app/Main.tz",
            "def main :: unit -> i32 = \\() -> GeometryCore::Point.value() as i32\n",
        );
        let real = scratch.path("moved");
        fs::rename(scratch.store(&sha256), &real).unwrap();
        std::os::unix::fs::symlink(&real, scratch.store(&sha256)).unwrap();
        assert_error(
            &scratch.tsuzuri(&["check", app]),
            "E2007",
            "git dependency 'geometry-core' is not downloaded; run tsuzuri fetch",
        );
        fs::remove_file(scratch.store(&sha256)).unwrap();
        fs::rename(&real, scratch.store(&sha256)).unwrap();
        assert_success(&scratch.tsuzuri(&["check", app]));
        let target = scratch.path("lock-target");
        fs::rename(&lock, &target).unwrap();
        std::os::unix::fs::symlink(&target, &lock).unwrap();
        assert_error(
            &scratch.tsuzuri(&["check", app]),
            "E2007",
            "Tsuzuri.lock must be a regular file",
        );
    }
}

#[test]
fn git_packages_reject_path_dependencies() {
    let scratch = Scratch::new("git-path");
    let url = "https://example.org/geometry-core.git";
    let manifest = format!("{GEOMETRY}[dependencies]\nhelper = {{ path = \"../helper\" }}\n");
    let sha256 = offline_project(
        &scratch,
        url,
        &[("Tsuzuri.toml", &manifest), ("Point.tz", POINT)],
    );
    scratch.write(
        "app/Tsuzuri.lock",
        &lock_text(&[("geometry-core", url, REV, &sha256)]),
    );
    let output = scratch.tsuzuri(&["check", scratch.path("app").to_str().unwrap()]);
    assert_error(
        &output,
        "E1011",
        "git packages cannot have path dependencies; use a git dependency",
    );
    assert!(
        stderr(&output).contains(&format!("{sha256}/Tsuzuri.toml")),
        "{}",
        stderr(&output)
    );
    // A git package cannot declare [native] link settings either.
    let manifest = format!("{GEOMETRY}[native]\nlibraries = [\"m\"]\n");
    let sha256 = offline_project(
        &scratch,
        url,
        &[("Tsuzuri.toml", &manifest), ("Point.tz", POINT)],
    );
    scratch.write(
        "app/Tsuzuri.lock",
        &lock_text(&[("geometry-core", url, REV, &sha256)]),
    );
    assert_error(
        &scratch.tsuzuri(&["check", scratch.path("app").to_str().unwrap()]),
        "E2000",
        "git packages cannot declare [native] link settings; move them to the application manifest",
    );
}

/// `geometry-core` and `other` repositories as in `tests/modules.rs`, and an app
/// that depends on both by git. Returns the repositories and their commits.
fn fetch_project(scratch: &Scratch) -> (Repository, String, Repository, String) {
    let geometry = scratch.repository("geometry-core");
    let geometry_rev = geometry.commit(&[
        ("Tsuzuri.toml", GEOMETRY.as_bytes()),
        ("Point.tz", POINT.as_bytes()),
        ("Main.tz", b"def main :: i64\nfn main = 99\n"),
        ("README.md", b"# geometry-core\n"),
    ]);
    let other = scratch.repository("other");
    let other_manifest = format!(
        "[package]\nname = \"other\"\nversion = \"0.1.0\"\n[dependencies]\n{}",
        git_dependency("geometry-core", &geometry.url(), &geometry_rev)
    );
    let other_rev = other.commit(&[
        ("Tsuzuri.toml", other_manifest.as_bytes()),
        (
            "Library.tz",
            b"def value :: i64\nfn value = GeometryCore::Main.main() - 99\n",
        ),
    ]);
    scratch.write(
        "app/Tsuzuri.toml",
        &app_manifest(&format!(
            "{}{}",
            git_dependency("geometry-core", &geometry.url(), &geometry_rev),
            git_dependency("other", &other.url(), &other_rev)
        )),
    );
    scratch.write(
        "app/Main.tz",
        "def main :: unit -> i32 = \\() -> (GeometryCore::Point.value() + Other::Library.value()) as i32\n",
    );
    (geometry, geometry_rev, other, other_rev)
}

fn expected_lock(
    geometry: &Repository,
    geometry_rev: &str,
    other: &Repository,
    other_rev: &str,
) -> String {
    let other_manifest = format!(
        "[package]\nname = \"other\"\nversion = \"0.1.0\"\n[dependencies]\n{}",
        git_dependency("geometry-core", &geometry.url(), geometry_rev)
    );
    lock_text(&[
        (
            "geometry-core",
            &geometry.url(),
            geometry_rev,
            &sha256_of(&[
                ("Tsuzuri.toml", GEOMETRY),
                ("Point.tz", POINT),
                ("Main.tz", "def main :: i64\nfn main = 99\n"),
            ]),
        ),
        (
            "other",
            &other.url(),
            other_rev,
            &sha256_of(&[
                ("Tsuzuri.toml", &other_manifest),
                (
                    "Library.tz",
                    "def value :: i64\nfn value = GeometryCore::Main.main() - 99\n",
                ),
            ]),
        ),
    ])
}

#[test]
fn fetch_downloads_file_repositories_and_writes_lock() {
    let scratch = Scratch::new("fetch");
    let (geometry, geometry_rev, other, other_rev) = fetch_project(&scratch);
    let app = scratch.path("app");
    let app = app.to_str().unwrap();
    let lock = scratch.path("app/Tsuzuri.lock");
    let output = scratch.tsuzuri(&["fetch", app]);
    assert_success(&output);
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
    let expected = expected_lock(&geometry, &geometry_rev, &other, &other_rev);
    assert_eq!(fs::read_to_string(&lock).unwrap(), expected);
    // The store holds only the package files, and the cache root stays a build cache.
    let sha256 = parse_lock(&expected, 0).unwrap()["geometry-core"]
        .sha256
        .clone();
    let mut stored: Vec<_> = fs::read_dir(scratch.store(&sha256))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    stored.sort();
    assert_eq!(stored, ["Main.tz", "Point.tz", "Tsuzuri.toml"]);
    assert!(scratch.cache().join(".tsuzuri-cache").is_file());
    let mut packages: Vec<_> = fs::read_dir(scratch.cache().join("packages"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    packages.sort();
    assert_eq!(packages, ["git"], "no staging directory is left behind");
    assert_success(&scratch.tsuzuri(&["check", app]));
    // A second fetch neither rewrites the lockfile nor needs git.
    let modified = fs::metadata(&lock).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    let output = scratch.tsuzuri_with(&["fetch", app], &[("PATH", Path::new(""))]);
    assert_success(&output);
    assert_eq!(fs::read_to_string(&lock).unwrap(), expected);
    assert_eq!(fs::metadata(&lock).unwrap().modified().unwrap(), modified);
    // Entries that the graph no longer needs leave the lockfile.
    scratch.write(
        "app/Tsuzuri.toml",
        &app_manifest(&git_dependency(
            "geometry-core",
            &geometry.url(),
            &geometry_rev,
        )),
    );
    scratch.write(
        "app/Main.tz",
        "def main :: unit -> i32 = \\() -> GeometryCore::Point.value() as i32\n",
    );
    assert_success(&scratch.tsuzuri(&["fetch", app, "--json"]));
    let remaining = parse_lock(&fs::read_to_string(&lock).unwrap(), 0).unwrap();
    assert_eq!(remaining.keys().collect::<Vec<_>>(), ["geometry-core"]);
    // Without git dependencies an existing lockfile becomes empty, and none is created.
    scratch.write("app/Tsuzuri.toml", &app_manifest(""));
    scratch.write("app/Main.tz", "42\n");
    assert_success(&scratch.tsuzuri(&["fetch", app]));
    assert_eq!(fs::read_to_string(&lock).unwrap(), lock_text(&[]));
    fs::remove_file(&lock).unwrap();
    assert_success(&scratch.tsuzuri(&["fetch", app]));
    assert!(!lock.exists());
}

#[test]
fn fetch_cli_rejects_bad_arguments() {
    let scratch = Scratch::new("fetch-cli");
    scratch.write("app/Tsuzuri.toml", &app_manifest(""));
    scratch.write("app/Main.tz", "42\n");
    scratch.write("plain/Main.tz", "42\n");
    let app = scratch.path("app");
    let app = app.to_str().unwrap();
    let main = scratch.path("app/Main.tz");
    let plain = scratch.path("plain");
    let usage = "fetch takes one project directory and optionally --json";
    let manifest = "fetch requires a Tsuzuri.toml in the project directory";
    for (arguments, message) in [
        (vec!["fetch"], usage),
        (vec!["fetch", app, app], usage),
        (vec!["fetch", app, "-O3"], usage),
        (vec!["fetch", "--json", app, "--json"], usage),
        (vec!["fetch", "--target", "wasm32"], usage),
        (vec!["fetch", main.to_str().unwrap()], manifest),
        (vec!["fetch", plain.to_str().unwrap()], manifest),
        (
            vec!["fetch", scratch.path("missing").to_str().unwrap()],
            manifest,
        ),
    ] {
        let output = scratch.tsuzuri(&arguments);
        let text = stderr(&output);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}: {text}");
        assert!(
            text.contains(&format!("error[E2000]: {message}"))
                || text.contains("\"code\":\"E2000\"") && text.contains(message),
            "{arguments:?}: {text}"
        );
    }
    let output = scratch.tsuzuri(&["fetch", "--json", plain.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("\"code\":\"E2000\""),
        "{}",
        stderr(&output)
    );
    assert_success(&scratch.tsuzuri(&["fetch", "--json", app]));
}

#[test]
fn fetch_rejects_unsafe_trees() {
    let scratch = Scratch::new("unsafe");
    let repository = scratch.repository("bad");
    let manifest = repository.blob(GEOMETRY.as_bytes());
    let point = repository.blob(POINT.as_bytes());
    let file = |path: &str, id: &str| (path.to_owned(), "100644", "blob", id.to_owned());
    let base = |extra: Vec<Entry>| {
        let mut entries = vec![file("Tsuzuri.toml", &manifest), file("Point.tz", &point)];
        entries.extend(extra);
        entries
    };
    let symlink = repository.blob(b"Point.tz");
    let large = repository.blob(&vec![b'1'; 1_048_577]);
    let latin1 = repository.blob(b"def caf\xe9 :: i64\n");
    let many: Vec<Entry> = (0..4097)
        .map(|index| file(&format!("M{index}.tz"), &point))
        .collect();
    let deep = format!("{}Deep.tz", "D/".repeat(16));
    let cases: Vec<(Vec<Entry>, &str, String)> = vec![
        (
            base(vec![("L.tz".to_owned(), "120000", "blob", symlink.clone())]),
            "E2007",
            "git dependency 'geometry-core' has a symbolic link or submodule at 'L.tz'; packages may contain only regular source files".to_owned(),
        ),
        (
            base(vec![("Sub.tz".to_owned(), "160000", "commit", REV.to_owned())]),
            "E2007",
            "has a symbolic link or submodule at 'Sub.tz'".to_owned(),
        ),
        (
            base(vec![file("a.tz", &point), file("A.tz", &point)]),
            "E2007",
            "git dependency 'geometry-core' has paths that differ only in case: ".to_owned(),
        ),
        (
            base(vec![file("shapes/X.tz", &point), file("Shapes/Y.tz", &point)]),
            "E2007",
            "has paths that differ only in case".to_owned(),
        ),
        (
            vec![file("sub/Tsuzuri.toml", &manifest), file("Point.tz", &point)],
            "E2007",
            "git dependency 'geometry-core' has no Tsuzuri.toml at the repository root of rev ".to_owned(),
        ),
        (
            base(vec![file("Latin.tz", &latin1)]),
            "E2007",
            "git dependency 'geometry-core' has a source that is not UTF-8: 'Latin.tz'".to_owned(),
        ),
        (
            base(vec![file("Large.tz", &large)]),
            "E0003",
            "source exceeds the 1048576-byte limit".to_owned(),
        ),
        (
            base(many),
            "E1017",
            "module discovery exceeds 4096 source files".to_owned(),
        ),
        (
            base(vec![file(&deep, &point)]),
            "E1017",
            "module discovery exceeds 1024 directories or 16 path segments".to_owned(),
        ),
        (
            base(vec![file("a:b.tz", &point)]),
            "E2007",
            "git dependency 'geometry-core' has an unsafe path 'a:b.tz'".to_owned(),
        ),
        (
            base(vec![file("a\\b.tz", &point)]),
            "E2007",
            "has an unsafe path 'a\\b.tz'".to_owned(),
        ),
        (
            base(vec![file("x\u{1}y.tz", &point)]),
            "E2007",
            "has an unsafe path".to_owned(),
        ),
        (
            base(vec![file(&format!("{}.tz", "N".repeat(253)), &point)]),
            "E2007",
            "has an unsafe path".to_owned(),
        ),
    ];
    let app = scratch.path("app");
    let app = app.to_str().unwrap();
    scratch.write("app/Main.tz", "42\n");
    for (entries, code, message) in cases {
        let rev = repository.commit_entries(&entries);
        scratch.write(
            "app/Tsuzuri.toml",
            &app_manifest(&git_dependency("geometry-core", &repository.url(), &rev)),
        );
        let output = scratch.tsuzuri(&["fetch", app]);
        assert_error(&output, code, &message);
        assert!(
            stderr(&output).contains("app/Tsuzuri.toml"),
            "{}",
            stderr(&output)
        );
        assert!(!scratch.path("app/Tsuzuri.lock").exists(), "{message}");
    }
    // A 252-byte stem plus `.tz` is the longest accepted name, so the check above
    // rejects 256 bytes. Files outside the package selection are ignored.
    let accepted = format!("{}.tz", "N".repeat(252));
    let rev = repository.commit_entries(&base(vec![
        file("README.md", &point),
        file(".hidden/X.tz", &latin1),
        file("sub/Tsuzuri.toml", &manifest),
        file("Tsuzuri.lock", &point),
        ("vendor".to_owned(), "160000", "commit", REV.to_owned()),
        ("docs/link.md".to_owned(), "120000", "blob", symlink),
        file("notes/a:b.txt", &point),
        file("Large.bin", &large),
        file(&accepted, &point),
        file("Nested/Deeper/Value.tz", &point),
    ]));
    scratch.write(
        "app/Tsuzuri.toml",
        &app_manifest(&git_dependency("geometry-core", &repository.url(), &rev)),
    );
    assert_success(&scratch.tsuzuri(&["fetch", app]));
    let sha256 = parse_lock(
        &fs::read_to_string(scratch.path("app/Tsuzuri.lock")).unwrap(),
        0,
    )
    .unwrap()["geometry-core"]
        .sha256
        .clone();
    assert_eq!(
        sha256,
        sha256_of(&[
            ("Tsuzuri.toml", GEOMETRY),
            ("Point.tz", POINT),
            (&accepted, POINT),
            ("Nested/Deeper/Value.tz", POINT),
        ])
    );
    let store = scratch.store(&sha256);
    for ignored in [
        "README.md",
        ".hidden",
        "sub",
        "Tsuzuri.lock",
        "vendor",
        "docs",
        "notes",
        "Large.bin",
    ] {
        assert!(!store.join(ignored).exists(), "{ignored}");
    }
    assert!(store.join("Nested/Deeper/Value.tz").is_file());
}

#[test]
fn url_allowed_rejects_file_urls_under_https_packages() {
    use tsuzuri::fetch::url_allowed;
    assert!(!url_allowed(Some("https://a/b.git"), "file:///x"));
    assert!(url_allowed(Some("https://a/b.git"), "https://c/d.git"));
    assert!(url_allowed(None, "file:///x"));
    assert!(url_allowed(None, "https://c/d.git"));
    assert!(url_allowed(Some("file:///y"), "file:///x"));
    assert!(url_allowed(Some("file:///y"), "https://c/d.git"));
}

#[test]
fn fetch_detects_lock_mismatch_and_repairs_store() {
    let scratch = Scratch::new("mismatch");
    let (geometry, geometry_rev, other, other_rev) = fetch_project(&scratch);
    let app = scratch.path("app");
    let app = app.to_str().unwrap();
    let lock = scratch.path("app/Tsuzuri.lock");
    assert_success(&scratch.tsuzuri(&["fetch", app]));
    let expected = expected_lock(&geometry, &geometry_rev, &other, &other_rev);
    let sha256 = parse_lock(&expected, 0).unwrap()["geometry-core"]
        .sha256
        .clone();
    // A lockfile that records other content for the same url and rev stops the fetch.
    let forged = expected.replace(&sha256, &"0".repeat(64));
    fs::write(&lock, &forged).unwrap();
    let output = scratch.tsuzuri(&["fetch", app]);
    assert_error(
        &output,
        "E2007",
        &format!(
            "git dependency 'geometry-core' at rev {geometry_rev} has sha256 {sha256} but Tsuzuri.lock records {}; remove its entry only if you trust the new content",
            "0".repeat(64)
        ),
    );
    assert!(stderr(&output).contains("Tsuzuri.lock"));
    assert_eq!(fs::read_to_string(&lock).unwrap(), forged);
    fs::write(&lock, &expected).unwrap();
    // A damaged store entry fails the build until fetch replaces it.
    let point = scratch.store(&sha256).join("Point.tz");
    fs::write(&point, "def value :: i64\nfn value = 0\n").unwrap();
    assert_error(
        &scratch.tsuzuri(&["check", app]),
        "E2007",
        "git dependency 'geometry-core' does not match its sha256 in Tsuzuri.lock; run tsuzuri fetch to restore it",
    );
    assert_success(&scratch.tsuzuri(&["fetch", app]));
    assert_eq!(fs::read_to_string(&point).unwrap(), POINT);
    assert_eq!(fs::read_to_string(&lock).unwrap(), expected);
    assert_success(&scratch.tsuzuri(&["check", app]));
    // A missing store entry is downloaded again.
    fs::remove_dir_all(scratch.store(&sha256)).unwrap();
    assert_success(&scratch.tsuzuri(&["fetch", app]));
    assert_success(&scratch.tsuzuri(&["check", app]));
    // One package name cannot name two revs in one graph.
    let second = geometry.commit(&[
        ("Tsuzuri.toml", GEOMETRY.as_bytes()),
        ("Point.tz", b"def value :: i64\nfn value = 43\n"),
    ]);
    scratch.write(
        "app/Tsuzuri.toml",
        &app_manifest(&format!(
            "{}{}",
            git_dependency("geometry-core", &geometry.url(), &second),
            git_dependency("other", &other.url(), &other_rev)
        )),
    );
    let conflict = "package 'geometry-core' is required from different sources";
    let output = scratch.tsuzuri(&["fetch", app]);
    assert_error(&output, "E1011", conflict);
    assert_eq!(fs::read_to_string(&lock).unwrap(), expected);
    // Offline, the root's new rev is not in the lockfile yet.
    assert_error(
        &scratch.tsuzuri(&["check", app]),
        "E2007",
        "Tsuzuri.lock does not record git dependency 'geometry-core' at this url and rev",
    );
    // A git package cannot share its name with a path package.
    scratch.write("local/Tsuzuri.toml", GEOMETRY);
    scratch.write("local/Point.tz", POINT);
    scratch.write(
        "app/Tsuzuri.toml",
        &app_manifest(&format!(
            "geometry-core = {{ path = \"../local\" }}\n{}",
            git_dependency("other", &other.url(), &other_rev)
        )),
    );
    assert_error(&scratch.tsuzuri(&["fetch", app]), "E1011", conflict);
    // A missing commit reports git's reason.
    scratch.write(
        "app/Tsuzuri.toml",
        &app_manifest(&git_dependency(
            "geometry-core",
            &geometry.url(),
            &"9".repeat(40),
        )),
    );
    let output = scratch.tsuzuri(&["fetch", app]);
    assert_error(
        &output,
        "E2007",
        &format!(
            "git fetch of {} at {} failed: ",
            geometry.url(),
            "9".repeat(40)
        ),
    );
    assert!(
        stderr(&output).contains("check the url, the commit id, and network access"),
        "{}",
        stderr(&output)
    );
    assert_eq!(fs::read_to_string(&lock).unwrap(), expected);
}

#[cfg(unix)]
#[test]
fn builds_never_run_git() {
    use std::os::unix::fs::PermissionsExt;
    let scratch = Scratch::new("no-git");
    fetch_project(&scratch);
    let app = scratch.path("app");
    let app = app.to_str().unwrap();
    assert_success(&scratch.tsuzuri(&["fetch", app]));
    let marker = scratch.path("git-was-run");
    let bin = scratch.path("bin");
    fs::create_dir(&bin).unwrap();
    let git = bin.join("git");
    fs::write(
        &git,
        format!("#!/bin/sh\necho \"$@\" > '{}'\nexit 1\n", marker.display()),
    )
    .unwrap();
    fs::set_permissions(&git, fs::Permissions::from_mode(0o755)).unwrap();
    let docs = scratch.path("docs");
    let ir = scratch.path("app.ll");
    for arguments in [
        vec!["check", app],
        vec!["doc", app, "-o", docs.to_str().unwrap()],
        vec!["build", app, "--emit", "llvm", "-o", ir.to_str().unwrap()],
        vec!["test", app, "--list"],
    ] {
        let output = scratch.tsuzuri_with(&arguments, &[("PATH", &bin)]);
        assert_success(&output);
        assert!(!marker.exists(), "{arguments:?} ran git");
    }
    // The fake git does run when fetch needs to download.
    fs::remove_dir_all(scratch.cache().join("packages/git")).unwrap();
    let output = scratch.tsuzuri_with(&["fetch", app], &[("PATH", &bin)]);
    assert_error(
        &output,
        "E2007",
        "git 2.32 or later is required to fetch git dependencies (found: none)",
    );
    assert!(marker.exists());
}
