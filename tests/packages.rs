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
            "expected a path, git, or version dependency",
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
        version: None,
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
            format!("{{\"format\": 3, \"packages\": [{valid}]}}"),
            "format must be 1 or 2",
        ),
        (
            package(&valid.replace("\"rev\":", "\"version\": \"1.0.0\", \"rev\":")),
            "unknown package key 'version'",
        ),
        (
            format!(
                "{{\"format\": 2, \"packages\": [{}]}}",
                valid.replace("\"rev\":", "\"version\": \"1.0\", \"rev\":")
            ),
            "invalid version of 'geometry-core'",
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
                        version: None,
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
        "git and registry packages cannot declare [native] link settings; move them to the application manifest",
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
    let large = repository.blob(&vec![b'1'; 4 * 1024 * 1024 + 1]);
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
            "source exceeds the 4194304-byte limit".to_owned(),
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

// Phase 2: registry dependencies, minimal version selection, and `tsuzuri publish`.

use tsuzuri::package::{VERSION_FORM, VERSION_RULE, Version, parse_index, render_index_entry};

fn version(text: &str) -> Version {
    Version::parse(text).unwrap()
}

#[test]
fn versions_parse_and_compare_compatibility_ranges() {
    assert_eq!(
        version("1.2.3"),
        Version {
            major: 1,
            minor: 2,
            patch: 3
        }
    );
    assert_eq!(version("0.0.0").to_string(), "0.0.0");
    assert_eq!(
        version("18446744073709551615.0.10").to_string(),
        "18446744073709551615.0.10"
    );
    for text in [
        "",
        "1",
        "1.2",
        "1.2.3.4",
        "01.2.3",
        "1.02.3",
        "1.2.03",
        "v1.2.3",
        "1.2.3-beta",
        "1.2.3+build",
        "^1.2.3",
        ">=1.2.3",
        "~1.2.3",
        "1.2.x",
        "1.2.*",
        " 1.2.3",
        "1.2.3 ",
        "1..3",
        "-1.2.3",
        "+1.2.3",
        "18446744073709551616.0.0",
        "１.2.3",
    ] {
        assert_eq!(Version::parse(text), None, "{text}");
    }
    assert!(version("1.0.0") < version("1.0.1") && version("1.9.9") < version("1.10.0"));
    // Same major from 1.0.0, same 0.minor before it.
    for (requirement, candidate, satisfied) in [
        ("1.2.3", "1.2.3", true),
        ("1.2.3", "1.2.4", true),
        ("1.2.3", "1.9.0", true),
        ("1.2.3", "1.2.2", false),
        ("1.2.3", "2.0.0", false),
        ("1.2.3", "0.9.0", false),
        ("0.2.3", "0.2.9", true),
        ("0.2.3", "0.3.0", false),
        ("0.2.3", "1.0.0", false),
        ("0.0.1", "0.0.2", true),
        ("0.0.1", "0.1.0", false),
    ] {
        assert_eq!(
            version(candidate).satisfies(version(requirement)),
            satisfied,
            "{candidate} for {requirement}"
        );
        assert_eq!(
            version(requirement).compatible(version(candidate)),
            satisfied
                || version(candidate) < version(requirement)
                    && version(candidate).major == version(requirement).major
                    && (version(requirement).major != 0
                        || version(candidate).minor == version(requirement).minor),
            "{candidate} and {requirement}"
        );
    }
}

#[test]
fn manifest_accepts_registry_dependencies_and_index() {
    let manifest = parse_manifest(
        &format!(
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[registry]\nindex = \"https://example.org/index.git\" # pinned below\nrev = \"{REV}\"\n\n[dependencies]\ngeometry-core = {{ version = \"1.2.3\" }}\ntight = {{version=\"0.1.0\"}}\nlocal = {{ path = \"../local\" }}\n"
        ),
        0,
    )
    .unwrap();
    assert_eq!(
        manifest.dependencies["geometry-core"].source,
        DependencySource::Registry(version("1.2.3"))
    );
    assert_eq!(
        manifest.dependencies["tight"].source,
        DependencySource::Registry(version("0.1.0"))
    );
    let registry = manifest.registry.unwrap();
    assert_eq!(registry.index, "https://example.org/index.git");
    assert_eq!(registry.rev.as_deref(), Some(REV));
    let manifest = parse_manifest(
        &format!("{GEOMETRY}[registry]\nindex = \"file:///srv/index.git\"\n"),
        0,
    )
    .unwrap();
    assert_eq!(manifest.registry.unwrap().rev, None);
    assert_eq!(parse_manifest(GEOMETRY, 0).unwrap().registry, None);
    for (text, message) in [
        ("[dependencies]\na = { version = \"1.2\" }", VERSION_RULE),
        ("[dependencies]\na = { version = \"^1.2.3\" }", VERSION_RULE),
        (
            "[dependencies]\na = { version = \"1.2.3-beta\" }",
            VERSION_RULE,
        ),
        ("[dependencies]\na = { version = \"01.2.3\" }", VERSION_RULE),
        ("[dependencies]\na = { version \"1.2.3\" }", VERSION_FORM),
        (
            "[dependencies]\na = { version = \"1.2.3\", git = \"https://e.org/a.git\" }",
            VERSION_FORM,
        ),
        (
            "[dependencies]\na = { version = \"1.2.3\", native = true }",
            VERSION_FORM,
        ),
        ("[dependencies]\na = { tag = \"v1\" }", GIT_FORM),
        (
            "[registry]\nrev = \"0123456789abcdef0123456789abcdef01234567\"",
            "[registry] requires index = \"https://...\"",
        ),
        ("[registry]", "[registry] requires index = \"https://...\""),
        (
            "[registry]\nindex = \"http://e.org/index.git\"",
            GIT_URL_RULE,
        ),
        (
            "[registry]\nindex = \"https://e.org/index.git\"\nrev = \"main\"",
            GIT_REV_RULE,
        ),
        (
            "[registry]\nindex = \"https://e.org/a.git\"\nindex = \"https://e.org/b.git\"",
            "registry keys must occur at most once",
        ),
        (
            "[registry]\nurl = \"https://e.org/a.git\"",
            "unknown registry key; expected index or rev",
        ),
        (
            "[registry]\nindex = [\"https://e.org/a.git\"]",
            "expected '\"'",
        ),
        (
            "[registry]\nindex = \"https://e.org/a.git\"\n[registry]",
            "expected [package] followed by optional [dependencies], [wasm], [native] and [registry], each once",
        ),
    ] {
        let error = parse_manifest(&format!("{GEOMETRY}{text}\n"), 3).unwrap_err();
        assert_eq!(error.code, "E0002", "{text}: {}", error.message);
        assert_eq!(error.message, message, "{text}");
        assert_eq!(error.span.source, Some(3), "{text}");
    }
}

#[test]
fn lock_format_2_records_registry_versions_and_reads_format_1() {
    let entry = |git: &str, rev: char, sha: char, version: Option<&str>| LockEntry {
        git: git.to_owned(),
        rev: rev.to_string().repeat(40),
        sha256: sha.to_string().repeat(64),
        version: version.map(self::version),
    };
    let entries = BTreeMap::from([
        (
            "gamma".to_owned(),
            entry("https://example.org/gamma.git", 'a', 'c', Some("1.3.0")),
        ),
        (
            "fork".to_owned(),
            entry("file:///srv/fork.git", 'b', 'd', None),
        ),
    ]);
    let canonical = format!(
        "{{\n  \"format\": 2,\n  \"packages\": [\n    {{\n      \"name\": \"fork\",\n      \"git\": \"file:///srv/fork.git\",\n      \"rev\": \"{}\",\n      \"sha256\": \"{}\"\n    }},\n    {{\n      \"name\": \"gamma\",\n      \"version\": \"1.3.0\",\n      \"git\": \"https://example.org/gamma.git\",\n      \"rev\": \"{}\",\n      \"sha256\": \"{}\"\n    }}\n  ]\n}}\n",
        "b".repeat(40),
        "d".repeat(64),
        "a".repeat(40),
        "c".repeat(64)
    );
    assert_eq!(render_lock(&entries), canonical);
    assert_eq!(parse_lock(&canonical, 0).unwrap(), entries);
    let shuffled = format!(
        "{{\"packages\": [{{\"sha256\": \"{}\", \"git\": \"https://example.org/gamma.git\", \"version\": \"1.3.0\", \"rev\": \"{}\", \"name\": \"gamma\"}}, {{\"name\": \"fork\", \"rev\": \"{}\", \"git\": \"file:///srv/fork.git\", \"sha256\": \"{}\"}}], \"format\": 2}}",
        "c".repeat(64),
        "a".repeat(40),
        "b".repeat(40),
        "d".repeat(64)
    );
    assert_eq!(render_lock(&parse_lock(&shuffled, 0).unwrap()), canonical);
    // Without registry packages the lockfile stays format 1, which format 2 readers accept.
    let git_only = BTreeMap::from([(
        "fork".to_owned(),
        entry("file:///srv/fork.git", 'b', 'd', None),
    )]);
    assert!(render_lock(&git_only).starts_with("{\n  \"format\": 1,\n"));
    assert_eq!(parse_lock(&render_lock(&git_only), 0).unwrap(), git_only);
}

#[test]
fn index_files_parse_strictly_and_entries_render_canonically() {
    let sha = "c".repeat(64);
    let version_entry = |version: &str, dependencies: &str| {
        format!(
            "{{\"version\": \"{version}\", \"git\": \"https://example.org/gamma.git\", \"rev\": \"{REV}\", \"sha256\": \"{sha}\", \"dependencies\": {{{dependencies}}}}}"
        )
    };
    let file = |versions: &[String]| {
        format!(
            "{{\"name\": \"gamma\", \"versions\": [{}]}}",
            versions.join(", ")
        )
    };
    let parsed = parse_index(
        "gamma",
        &file(&[
            version_entry("1.1.0", ""),
            version_entry("1.0.0", "\"delta\": \"0.2.0\", \"alpha\": \"1.0.0\""),
        ]),
    )
    .unwrap();
    assert_eq!(
        parsed.keys().map(Version::to_string).collect::<Vec<_>>(),
        ["1.0.0", "1.1.0"]
    );
    let first = &parsed[&version("1.0.0")];
    assert_eq!(first.git, "https://example.org/gamma.git");
    assert_eq!(
        first.dependencies,
        BTreeMap::from([
            ("alpha".to_owned(), version("1.0.0")),
            ("delta".to_owned(), version("0.2.0"))
        ])
    );
    assert_eq!(
        render_index_entry(version("1.0.0"), first),
        format!(
            "{{\n  \"version\": \"1.0.0\",\n  \"git\": \"https://example.org/gamma.git\",\n  \"rev\": \"{REV}\",\n  \"sha256\": \"{sha}\",\n  \"dependencies\": {{\n    \"alpha\": \"1.0.0\",\n    \"delta\": \"0.2.0\"\n  }}\n}}\n"
        )
    );
    assert_eq!(
        render_index_entry(version("1.1.0"), &parsed[&version("1.1.0")]),
        format!(
            "{{\n  \"version\": \"1.1.0\",\n  \"git\": \"https://example.org/gamma.git\",\n  \"rev\": \"{REV}\",\n  \"sha256\": \"{sha}\",\n  \"dependencies\": {{}}\n}}\n"
        )
    );
    assert!(
        parse_index("gamma", "{\"name\": \"gamma\", \"versions\": []}")
            .unwrap()
            .is_empty()
    );
    for (text, detail) in [
        ("[]".to_owned(), "expected an object"),
        (
            file(&[]).replace("\"gamma\"", "\"delta\""),
            "name must be \"gamma\"",
        ),
        (
            file(&[]).replace("versions", "releases"),
            "unknown key 'releases'",
        ),
        (
            file(&[version_entry("1.1.0", ""), version_entry("1.1.0", "")]),
            "duplicate version 1.1.0",
        ),
        (file(&[version_entry("1.1", "")]), "invalid version '1.1'"),
        (
            file(&[version_entry("1.1.0", "\"Delta\": \"1.0.0\"")]),
            "invalid dependency 'Delta' of version 1.1.0",
        ),
        (
            file(&[version_entry("1.1.0", "\"delta\": \">=1.0.0\"")]),
            "invalid dependency 'delta' of version 1.1.0",
        ),
        (
            file(&[version_entry("1.1.0", "").replace("https://", "http://")]),
            "invalid git url of version 1.1.0",
        ),
        (
            file(&[version_entry("1.1.0", "").replace(REV, "main")]),
            "invalid rev of version 1.1.0",
        ),
        (
            file(&[version_entry("1.1.0", "").replace(&sha, "c")]),
            "invalid sha256 of version 1.1.0",
        ),
        (
            file(&[version_entry("1.1.0", "").replace("\"dependencies\": {}", "\"yanked\": true")]),
            "unknown version key 'yanked'",
        ),
        (
            file(&[version_entry("1.1.0", "").replace(", \"dependencies\": {}", "")]),
            "version 1.1.0 needs a dependencies object",
        ),
        (
            file(&[
                version_entry("1.1.0", "").replace("\"git\"", "\"version\": \"1.2.0\", \"git\"")
            ]),
            "duplicate key 'version'",
        ),
    ] {
        let error = parse_index("gamma", &text).unwrap_err();
        assert!(error.contains(detail), "{detail}: {error}");
    }
}

/// A registry: one repository per package with a commit per version, and an index
/// repository whose `index/<name>.json` lists them. Expected hashes come from the
/// committed files.
struct RegistryFixture<'a> {
    scratch: &'a Scratch,
    repositories: BTreeMap<String, Repository>,
    versions: BTreeMap<String, Vec<String>>,
    index: Repository,
}

impl<'a> RegistryFixture<'a> {
    fn new(scratch: &'a Scratch) -> Self {
        Self {
            scratch,
            repositories: BTreeMap::new(),
            versions: BTreeMap::new(),
            index: scratch.repository("index"),
        }
    }

    fn repository(&mut self, name: &str) -> &Repository {
        if !self.repositories.contains_key(name) {
            let repository = self.scratch.repository(name);
            self.repositories.insert(name.to_owned(), repository);
        }
        &self.repositories[name]
    }

    fn manifest(name: &str, version: &str, dependencies: &[(&str, &str)]) -> String {
        let mut manifest =
            format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\n[dependencies]\n");
        for (dependency, requirement) in dependencies {
            manifest += &format!("{dependency} = {{ version = \"{requirement}\" }}\n");
        }
        manifest
    }

    /// Commits `name` `version` with a `Value.tz` and adds its index entry with
    /// `entry_dependencies`; returns the commit and its content hash.
    fn publish_with(
        &mut self,
        name: &str,
        version: &str,
        dependencies: &[(&str, &str)],
        entry_dependencies: &[(&str, &str)],
        source: &str,
    ) -> (String, String) {
        let manifest = Self::manifest(name, version, dependencies);
        let repository = self.repository(name);
        let rev = repository.commit(&[
            ("Tsuzuri.toml", manifest.as_bytes()),
            ("Value.tz", source.as_bytes()),
        ]);
        let url = repository.url();
        let sha256 = sha256_of(&[("Tsuzuri.toml", &manifest), ("Value.tz", source)]);
        self.add_entry(name, version, &url, &rev, &sha256, entry_dependencies);
        (rev, sha256)
    }

    fn publish(
        &mut self,
        name: &str,
        version: &str,
        dependencies: &[(&str, &str)],
        source: &str,
    ) -> (String, String) {
        self.publish_with(name, version, dependencies, dependencies, source)
    }

    fn add_entry(
        &mut self,
        name: &str,
        version: &str,
        url: &str,
        rev: &str,
        sha256: &str,
        dependencies: &[(&str, &str)],
    ) {
        let dependencies = dependencies
            .iter()
            .map(|(dependency, requirement)| format!("\"{dependency}\": \"{requirement}\""))
            .collect::<Vec<_>>()
            .join(", ");
        self.versions.entry(name.to_owned()).or_default().push(format!(
            "{{\"version\": \"{version}\", \"git\": \"{url}\", \"rev\": \"{rev}\", \"sha256\": \"{sha256}\", \"dependencies\": {{{dependencies}}}}}"
        ));
    }

    /// Commits the index files and returns the commit.
    fn commit_index(&self) -> String {
        let files: Vec<(String, String)> = self
            .versions
            .iter()
            .map(|(name, versions)| {
                (
                    format!("index/{name}.json"),
                    format!(
                        "{{\n  \"name\": \"{name}\",\n  \"versions\": [\n    {}\n  ]\n}}\n",
                        versions.join(",\n    ")
                    ),
                )
            })
            .collect();
        let files: Vec<(&str, &[u8])> = files
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_bytes()))
            .collect();
        self.index.commit(&files)
    }
}

fn value(number: i64) -> String {
    format!("def value :: i64\nfn value = {number}\n")
}

fn registry_app(scratch: &Scratch, index: &str, rev: Option<&str>, dependencies: &[(&str, &str)]) {
    let mut manifest = app_manifest("");
    for (name, requirement) in dependencies {
        manifest += &format!("{name} = {{ version = \"{requirement}\" }}\n");
    }
    manifest += &format!("\n[registry]\nindex = \"{index}\"\n");
    if let Some(rev) = rev {
        manifest += &format!("rev = \"{rev}\"\n");
    }
    scratch.write("app/Tsuzuri.toml", &manifest);
    scratch.write("app/Main.tz", "42\n");
}

fn registry_lock(entries: &[(&str, &str, &str, &str, &str)]) -> String {
    render_lock(
        &entries
            .iter()
            .map(|(name, version, git, rev, sha256)| {
                (
                    (*name).to_owned(),
                    LockEntry {
                        git: (*git).to_owned(),
                        rev: (*rev).to_owned(),
                        sha256: (*sha256).to_owned(),
                        version: Some(self::version(version)),
                    },
                )
            })
            .collect(),
    )
}

#[test]
fn registry_resolution_selects_minimal_versions() {
    let scratch = Scratch::new("registry");
    let mut registry = RegistryFixture::new(&scratch);
    let mut gamma = BTreeMap::new();
    for (version, number) in [
        ("1.1.0", 11),
        ("1.2.0", 12),
        ("1.3.0", 13),
        ("1.4.0", 14),
        ("2.0.0", 20),
    ] {
        gamma.insert(
            version,
            registry.publish("gamma", version, &[], &value(number)),
        );
    }
    registry.publish("delta", "1.0.0", &[], &value(1));
    registry.publish(
        "alpha",
        "1.0.0",
        &[("gamma", "1.3.0"), ("delta", "1.0.0")],
        &value(100),
    );
    let alpha_source = "def value :: i64\nfn value = 110 + Gamma::Value.value()\n";
    let alpha = registry.publish("alpha", "1.1.0", &[("gamma", "1.2.0")], alpha_source);
    registry.publish("alpha", "1.2.0", &[], &value(120));
    let beta_source = "def value :: i64\nfn value = Alpha::Value.value() + Gamma::Value.value()\n";
    let beta = registry.publish(
        "beta",
        "1.0.0",
        &[("alpha", "1.0.0"), ("gamma", "1.1.0")],
        beta_source,
    );
    let index_rev = registry.commit_index();
    let index = registry.index.url();
    // app needs alpha 1.1.0 and beta 1.0.0; beta needs alpha 1.0.0 (a diamond) and gamma 1.1.0;
    // alpha 1.1.0 needs gamma 1.2.0, and the unselected alpha 1.0.0 still raises gamma to 1.3.0.
    // delta is needed only by alpha 1.0.0, so the build has no delta.
    registry_app(
        &scratch,
        &index,
        Some(&index_rev),
        &[("alpha", "1.1.0"), ("beta", "1.0.0")],
    );
    scratch.write(
        "app/Main.tz",
        "def main :: unit -> i32 = \\() -> (Alpha::Value.value() + Beta::Value.value()) as i32\n",
    );
    let app = scratch.path("app");
    let app = app.to_str().unwrap();
    let output = scratch.tsuzuri(&["fetch", app]);
    assert_success(&output);
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
    let alpha_manifest = RegistryFixture::manifest("alpha", "1.1.0", &[("gamma", "1.2.0")]);
    let beta_manifest =
        RegistryFixture::manifest("beta", "1.0.0", &[("alpha", "1.0.0"), ("gamma", "1.1.0")]);
    let gamma_manifest = RegistryFixture::manifest("gamma", "1.3.0", &[]);
    let url = |name: &str| registry.repositories[name].url();
    let expected = registry_lock(&[
        (
            "alpha",
            "1.1.0",
            &url("alpha"),
            &alpha.0,
            &sha256_of(&[
                ("Tsuzuri.toml", &alpha_manifest),
                ("Value.tz", alpha_source),
            ]),
        ),
        (
            "beta",
            "1.0.0",
            &url("beta"),
            &beta.0,
            &sha256_of(&[("Tsuzuri.toml", &beta_manifest), ("Value.tz", beta_source)]),
        ),
        (
            "gamma",
            "1.3.0",
            &url("gamma"),
            &gamma["1.3.0"].0,
            &sha256_of(&[("Tsuzuri.toml", &gamma_manifest), ("Value.tz", &value(13))]),
        ),
    ]);
    let lock = scratch.path("app/Tsuzuri.lock");
    assert_eq!(fs::read_to_string(&lock).unwrap(), expected);
    assert!(expected.starts_with("{\n  \"format\": 2,\n"));
    assert_success(&scratch.tsuzuri(&["check", app]));
    // The selection does not depend on the lockfile, the pinned index commit, or a rerun.
    let modified = fs::metadata(&lock).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    assert_success(&scratch.tsuzuri(&["fetch", app]));
    assert_eq!(fs::metadata(&lock).unwrap().modified().unwrap(), modified);
    fs::remove_file(&lock).unwrap();
    registry_app(
        &scratch,
        &index,
        None,
        &[("alpha", "1.1.0"), ("beta", "1.0.0")],
    );
    scratch.write(
        "app/Main.tz",
        "def main :: unit -> i32 = \\() -> (Alpha::Value.value() + Beta::Value.value()) as i32\n",
    );
    assert_success(&scratch.tsuzuri(&["fetch", app]));
    assert_eq!(fs::read_to_string(&lock).unwrap(), expected);
    // Builds stay offline: they need neither the index nor its [registry] section.
    let manifest = fs::read_to_string(scratch.path("app/Tsuzuri.toml")).unwrap();
    let without_registry = &manifest[..manifest.find("\n[registry]").unwrap() + 1];
    fs::write(scratch.path("app/Tsuzuri.toml"), without_registry).unwrap();
    assert_success(&scratch.tsuzuri(&["check", app]));
    assert_error(
        &scratch.tsuzuri(&["fetch", app]),
        "E2007",
        "registry dependency 'alpha' needs a [registry] index in the root package's Tsuzuri.toml",
    );
    assert_eq!(fs::read_to_string(&lock).unwrap(), expected);
    // A requirement that the locked version satisfies needs no fetch; a newer one does.
    fs::write(
        scratch.path("app/Tsuzuri.toml"),
        without_registry.replace(
            "alpha = { version = \"1.1.0\" }",
            "alpha = { version = \"1.0.5\" }",
        ),
    )
    .unwrap();
    assert_success(&scratch.tsuzuri(&["check", app]));
    for requirement in ["1.2.0", "2.0.0", "0.1.0"] {
        fs::write(
            scratch.path("app/Tsuzuri.toml"),
            without_registry.replace(
                "alpha = { version = \"1.1.0\" }",
                &format!("alpha = {{ version = \"{requirement}\" }}"),
            ),
        )
        .unwrap();
        assert_error(
            &scratch.tsuzuri(&["check", app]),
            "E2007",
            &format!(
                "Tsuzuri.lock does not record a version of 'alpha' that satisfies {requirement}; run tsuzuri fetch"
            ),
        );
    }
    // A lockfile whose version was edited no longer matches the package.
    fs::write(scratch.path("app/Tsuzuri.toml"), without_registry).unwrap();
    fs::write(
        &lock,
        expected.replace("\"version\": \"1.3.0\"", "\"version\": \"1.3.1\""),
    )
    .unwrap();
    assert_error(
        &scratch.tsuzuri(&["check", app]),
        "E2007",
        "registry package 'gamma' declares version 1.3.0 but Tsuzuri.lock records 1.3.1; run tsuzuri fetch",
    );
    fs::write(&lock, &expected).unwrap();
    let gamma_root = scratch.store(&parse_lock(&expected, 0).unwrap()["gamma"].sha256);
    fs::write(gamma_root.join("Value.tz"), value(99)).unwrap();
    assert_error(
        &scratch.tsuzuri(&["check", app]),
        "E2007",
        "registry dependency 'gamma' does not match its sha256 in Tsuzuri.lock; run tsuzuri fetch to restore it",
    );
}

#[test]
fn registry_rejects_incompatible_and_missing_versions() {
    let scratch = Scratch::new("registry-errors");
    let mut registry = RegistryFixture::new(&scratch);
    let (gamma_rev, _) = registry.publish("gamma", "1.1.0", &[], &value(11));
    registry.publish("gamma", "2.0.0", &[], &value(20));
    registry.publish("zeta", "0.1.0", &[], &value(1));
    registry.publish("zeta", "0.2.0", &[], &value(2));
    registry.publish("beta", "1.0.0", &[("gamma", "2.0.0")], &value(3));
    registry.publish("eta", "1.0.0", &[("zeta", "0.2.0")], &value(4));
    let index_rev = registry.commit_index();
    let index = registry.index.url();
    let app = scratch.path("app");
    let app = app.to_str().unwrap();
    let lock = scratch.path("app/Tsuzuri.lock");
    for (dependencies, code, message) in [
        (
            vec![("gamma", "1.1.0"), ("beta", "1.0.0")],
            "E1011",
            "package 'gamma' is required at incompatible versions 1.1.0 (by app) and 2.0.0 (by beta 1.0.0); one package name has one version",
        ),
        (
            vec![("zeta", "0.1.0"), ("eta", "1.0.0")],
            "E1011",
            "package 'zeta' is required at incompatible versions 0.1.0 (by app) and 0.2.0 (by eta 1.0.0); one package name has one version",
        ),
        (
            vec![("gamma", "1.5.0")],
            "E2007",
            "the registry index has no version 1.5.0 of 'gamma' (required by app)",
        ),
        (
            vec![("omega", "1.0.0")],
            "E2007",
            "the registry index has no package 'omega'",
        ),
    ] {
        registry_app(&scratch, &index, Some(&index_rev), &dependencies);
        let output = scratch.tsuzuri(&["fetch", app]);
        assert_error(&output, code, message);
        assert!(
            stderr(&output).contains("app/Tsuzuri.toml"),
            "{}",
            stderr(&output)
        );
        assert!(!lock.exists(), "{message}");
    }
    // A missing index commit is a git failure.
    registry_app(
        &scratch,
        &index,
        Some(&"7".repeat(40)),
        &[("gamma", "1.1.0")],
    );
    assert_error(
        &scratch.tsuzuri(&["fetch", app]),
        "E2007",
        &format!(
            "git fetch of the registry index {index} at {} failed: ",
            "7".repeat(40)
        ),
    );
    // A registry package name cannot also name a git or path package.
    registry_app(&scratch, &index, Some(&index_rev), &[("gamma", "1.1.0")]);
    let manifest = fs::read_to_string(scratch.path("app/Tsuzuri.toml")).unwrap();
    scratch.write(
        "local/Tsuzuri.toml",
        &format!(
            "[package]\nname = \"local\"\nversion = \"0.1.0\"\n[dependencies]\n{}",
            git_dependency("gamma", &registry.repositories["gamma"].url(), &gamma_rev)
        ),
    );
    scratch.write("local/Value.tz", &value(5));
    fs::write(
        scratch.path("app/Tsuzuri.toml"),
        manifest.replace(
            "[dependencies]\n",
            "[dependencies]\nlocal = { path = \"../local\" }\n",
        ),
    )
    .unwrap();
    assert_error(
        &scratch.tsuzuri(&["fetch", app]),
        "E1011",
        "package 'gamma' is required from different sources",
    );
    assert!(!lock.exists());
    // Registry packages depend only on registry packages.
    let (rev, sha256) = registry.publish("theta", "1.0.0", &[], &value(6));
    let manifest = format!(
        "{}local = {{ path = \"../local\" }}\n",
        RegistryFixture::manifest("theta", "1.0.0", &[])
    );
    let root = scratch.store(&sha256_of(&[
        ("Tsuzuri.toml", &manifest),
        ("Value.tz", &value(6)),
    ]));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("Tsuzuri.toml"), &manifest).unwrap();
    fs::write(root.join("Value.tz"), value(6)).unwrap();
    scratch.write(
        "app/Tsuzuri.toml",
        &app_manifest("theta = { version = \"1.0.0\" }\n"),
    );
    let sha256_with_path = sha256_of(&[("Tsuzuri.toml", &manifest), ("Value.tz", &value(6))]);
    assert_ne!(sha256_with_path, sha256);
    fs::write(
        &lock,
        registry_lock(&[(
            "theta",
            "1.0.0",
            &registry.repositories["theta"].url(),
            &rev,
            &sha256_with_path,
        )]),
    )
    .unwrap();
    assert_error(
        &scratch.tsuzuri(&["check", app]),
        "E1011",
        "registry packages can depend only on registry packages; use a version requirement",
    );
}

#[test]
fn registry_ignores_requirements_of_unselected_versions() {
    let scratch = Scratch::new("registry-unselected");
    let mut registry = RegistryFixture::new(&scratch);
    registry.publish("aaa", "1.0.0", &[], &value(1));
    registry.publish("aaa", "2.0.0", &[], &value(2));
    registry.publish("aaa", "3.0.0", &[], &value(3));
    // beta 1.0.0 is visited (delta requires it) but not selected: app needs beta 1.1.0.
    registry.publish("beta", "1.0.0", &[("aaa", "1.0.0")], &value(10));
    registry.publish("beta", "1.1.0", &[("aaa", "2.0.0")], &value(11));
    registry.publish("delta", "1.0.0", &[("beta", "1.0.0")], &value(20));
    // The same with an unselected version that needs a newer incompatible series.
    registry.publish("gamma", "1.0.0", &[("aaa", "3.0.0")], &value(30));
    registry.publish("gamma", "1.1.0", &[("aaa", "2.0.0")], &value(31));
    registry.publish("epsilon", "1.0.0", &[("gamma", "1.0.0")], &value(40));
    let index_rev = registry.commit_index();
    let index = registry.index.url();
    let app = scratch.path("app");
    let app = app.to_str().unwrap();
    let lock = scratch.path("app/Tsuzuri.lock");
    for (dependencies, selected) in [
        (
            vec![("beta", "1.1.0"), ("delta", "1.0.0")],
            vec![("aaa", "2.0.0"), ("beta", "1.1.0"), ("delta", "1.0.0")],
        ),
        (
            vec![("gamma", "1.1.0"), ("epsilon", "1.0.0")],
            vec![("aaa", "2.0.0"), ("epsilon", "1.0.0"), ("gamma", "1.1.0")],
        ),
    ] {
        let _ = fs::remove_file(&lock);
        registry_app(&scratch, &index, Some(&index_rev), &dependencies);
        assert_success(&scratch.tsuzuri(&["fetch", app]));
        let entries = parse_lock(&fs::read_to_string(&lock).unwrap(), 0).unwrap();
        let versions: Vec<(&str, String)> = entries
            .iter()
            .map(|(name, entry)| (name.as_str(), entry.version.unwrap().to_string()))
            .collect();
        let expected: Vec<(&str, String)> = selected
            .iter()
            .map(|(name, version)| (*name, (*version).to_owned()))
            .collect();
        assert_eq!(versions, expected);
        assert_success(&scratch.tsuzuri(&["check", app]));
    }
}

#[test]
fn registry_detects_tampered_index_entries() {
    let scratch = Scratch::new("registry-tamper");
    let app = scratch.path("app");
    let app = app.to_str().unwrap();
    let lock = scratch.path("app/Tsuzuri.lock");
    type Setup = Box<dyn Fn(&mut RegistryFixture)>;
    let cases: Vec<(&str, Setup, String)> = vec![
        (
            "a wrong sha256",
            Box::new(|registry: &mut RegistryFixture| {
                let manifest = RegistryFixture::manifest("gamma", "1.1.0", &[]);
                let repository = registry.repository("gamma");
                let rev = repository.commit(&[("Tsuzuri.toml", manifest.as_bytes()), ("Value.tz", value(11).as_bytes())]);
                let url = repository.url();
                registry.add_entry("gamma", "1.1.0", &url, &rev, &"0".repeat(64), &[]);
            }),
            format!("but the registry index records {}", "0".repeat(64)),
        ),
        (
            "other dependencies",
            Box::new(|registry: &mut RegistryFixture| {
                registry.publish("delta", "1.0.0", &[], &value(1));
                registry.publish_with("gamma", "1.1.0", &[], &[("delta", "1.0.0")], &value(11));
            }),
            "registry package 'gamma' 1.1.0 does not match its registry index entry (its dependencies differ)".to_owned(),
        ),
        (
            "another version",
            Box::new(|registry: &mut RegistryFixture| {
                let manifest = RegistryFixture::manifest("gamma", "1.0.0", &[]);
                let repository = registry.repository("gamma");
                let rev = repository.commit(&[("Tsuzuri.toml", manifest.as_bytes()), ("Value.tz", value(11).as_bytes())]);
                let url = repository.url();
                let sha256 = sha256_of(&[("Tsuzuri.toml", &manifest), ("Value.tz", &value(11))]);
                registry.add_entry("gamma", "1.1.0", &url, &rev, &sha256, &[]);
            }),
            "registry package 'gamma' 1.1.0 does not match its registry index entry (its version is \"1.0.0\")".to_owned(),
        ),
        (
            "another package",
            Box::new(|registry: &mut RegistryFixture| {
                let manifest = RegistryFixture::manifest("delta", "1.1.0", &[]);
                let repository = registry.repository("delta");
                let rev = repository.commit(&[("Tsuzuri.toml", manifest.as_bytes()), ("Value.tz", value(11).as_bytes())]);
                let url = repository.url();
                let sha256 = sha256_of(&[("Tsuzuri.toml", &manifest), ("Value.tz", &value(11))]);
                registry.add_entry("gamma", "1.1.0", &url, &rev, &sha256, &[]);
            }),
            "registry package 'gamma' 1.1.0 does not match its registry index entry (its package name is 'delta')".to_owned(),
        ),
        (
            "a duplicate version",
            Box::new(|registry: &mut RegistryFixture| {
                registry.publish("gamma", "1.1.0", &[], &value(11));
                registry.publish("gamma", "1.1.0", &[], &value(12));
            }),
            "the registry index file for 'gamma' is invalid (duplicate version 1.1.0); fix index/gamma.json in the registry index".to_owned(),
        ),
    ];
    for (index, (case, setup, message)) in cases.into_iter().enumerate() {
        let scratch = Scratch::new(&format!("registry-tamper-{index}"));
        let mut registry = RegistryFixture::new(&scratch);
        setup(&mut registry);
        let index_rev = registry.commit_index();
        registry_app(
            &scratch,
            &registry.index.url(),
            Some(&index_rev),
            &[("gamma", "1.1.0")],
        );
        let app = scratch.path("app");
        let output = scratch.tsuzuri(&["fetch", app.to_str().unwrap()]);
        assert_error(&output, "E2007", &message);
        assert!(!scratch.path("app/Tsuzuri.lock").exists(), "{case}");
    }
    // An index entry that changes after Tsuzuri.lock recorded it stops the fetch.
    let mut registry = RegistryFixture::new(&scratch);
    let (_, sha256) = registry.publish("gamma", "1.1.0", &[], &value(11));
    let index_rev = registry.commit_index();
    registry_app(
        &scratch,
        &registry.index.url(),
        Some(&index_rev),
        &[("gamma", "1.1.0")],
    );
    assert_success(&scratch.tsuzuri(&["fetch", app]));
    let recorded = fs::read_to_string(&lock).unwrap();
    registry.versions.clear();
    let (_, changed) = registry.publish("gamma", "1.1.0", &[], &value(12));
    let index_rev = registry.commit_index();
    registry_app(
        &scratch,
        &registry.index.url(),
        Some(&index_rev),
        &[("gamma", "1.1.0")],
    );
    assert_error(
        &scratch.tsuzuri(&["fetch", app]),
        "E2007",
        &format!(
            "registry package 'gamma' 1.1.0 has sha256 {changed} in the registry index but Tsuzuri.lock records {sha256}; remove its entry only if you trust the new content"
        ),
    );
    assert_eq!(fs::read_to_string(&lock).unwrap(), recorded);
}

#[test]
fn publish_prints_index_entries() {
    let scratch = Scratch::new("publish");
    let mut registry = RegistryFixture::new(&scratch);
    registry.publish("delta", "1.0.0", &[], &value(1));
    let index_rev = registry.commit_index();
    let index = registry.index.url();
    // The package's working copy and the commit it publishes.
    let manifest = format!(
        "[package]\nname = \"gamma\"\nversion = \"1.2.0\"\n\n[dependencies]\ndelta = {{ version = \"1.0.0\" }}\n\n[registry]\nindex = \"{index}\"\nrev = \"{index_rev}\"\n"
    );
    let source = "def value :: i64\nfn value = Delta::Value.value() + 1\n";
    let deep = "def deep :: i64\nfn deep = 2\n";
    scratch.write("gamma/Tsuzuri.toml", &manifest);
    scratch.write("gamma/Value.tz", source);
    scratch.write("gamma/Sub/Deep.tz", deep);
    scratch.write("gamma/README.md", "not part of the package\n");
    let repository = scratch.repository("gamma");
    let rev = repository.commit(&[
        ("Tsuzuri.toml", manifest.as_bytes()),
        ("Value.tz", source.as_bytes()),
        ("Sub/Deep.tz", deep.as_bytes()),
    ]);
    let url = repository.url();
    let package = scratch.path("gamma");
    let package = package.to_str().unwrap();
    assert_success(&scratch.tsuzuri(&["fetch", package]));
    let output = scratch.tsuzuri(&["publish", package, "--git", &url, "--rev", &rev]);
    assert_success(&output);
    let sha256 = sha256_of(&[
        ("Tsuzuri.toml", &manifest),
        ("Value.tz", source),
        ("Sub/Deep.tz", deep),
    ]);
    let entry = format!(
        "{{\n  \"version\": \"1.2.0\",\n  \"git\": \"{url}\",\n  \"rev\": \"{rev}\",\n  \"sha256\": \"{sha256}\",\n  \"dependencies\": {{\n    \"delta\": \"1.0.0\"\n  }}\n}}\n"
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), entry);
    assert!(output.stderr.is_empty());
    // The printed entry is what the index lists, so an app can resolve it.
    registry
        .versions
        .insert("gamma".to_owned(), vec![entry.trim_end().to_owned()]);
    let index_rev = registry.commit_index();
    registry_app(&scratch, &index, Some(&index_rev), &[("gamma", "1.2.0")]);
    scratch.write(
        "app/Main.tz",
        "def main :: unit -> i32 = \\() -> (Gamma::Value.value() + Gamma::Sub::Deep.deep()) as i32\n",
    );
    let app = scratch.path("app");
    assert_success(&scratch.tsuzuri(&["fetch", app.to_str().unwrap()]));
    assert_success(&scratch.tsuzuri(&["check", app.to_str().unwrap()]));
    // A working copy that differs from the commit is not published.
    scratch.write(
        "gamma/Value.tz",
        "def value :: i64\nfn value = Delta::Value.value() + 2\n",
    );
    assert_error(
        &scratch.tsuzuri(&["publish", package, "--git", &url, "--rev", &rev]),
        "E2007",
        &format!("does not match {url} at {rev}"),
    );
    scratch.write("gamma/Value.tz", source);
    // A package that does not check is not published.
    scratch.write("gamma/Broken.tz", "def broken :: i64\nfn broken = false\n");
    let output = scratch.tsuzuri(&["publish", package, "--git", &url, "--rev", &rev]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("error[E1003]"),
        "{}",
        stderr(&output)
    );
    fs::remove_file(scratch.path("gamma/Broken.tz")).unwrap();
    for (replacement, code, message) in [
        (
            manifest.replace("version = \"1.2.0\"", "version = \"1.2\""),
            "E1011",
            "a published package needs [package] version in MAJOR.MINOR.PATCH form",
        ),
        (
            manifest.replace(
                "[dependencies]\n",
                "[dependencies]\nlocal = { path = \"../local\" }\n",
            ),
            "E1011",
            "a published package can depend only on registry packages; 'local' is a path or git dependency",
        ),
        (
            format!("{manifest}\n[native]\nlibraries = [\"m\"]\n"),
            "E2000",
            "a published package cannot declare [native] link settings",
        ),
    ] {
        scratch.write(
            "local/Tsuzuri.toml",
            "[package]\nname = \"local\"\nversion = \"0.1.0\"\n",
        );
        scratch.write("local/Value.tz", &value(3));
        scratch.write("gamma/Tsuzuri.toml", &replacement);
        assert_error(
            &scratch.tsuzuri(&["publish", package, "--git", &url, "--rev", &rev]),
            code,
            message,
        );
    }
    scratch.write("gamma/Tsuzuri.toml", &manifest);
    let usage =
        "publish takes one package directory, --git URL, --rev COMMIT, and optionally --json";
    let plain = scratch.path("plain");
    fs::create_dir_all(&plain).unwrap();
    for (arguments, message) in [
        (vec!["publish", package, "--git", &url], usage),
        (vec!["publish", package, "--rev", &rev], usage),
        (vec!["publish", "--git", &url, "--rev", &rev], usage),
        (
            vec!["publish", package, package, "--git", &url, "--rev", &rev],
            usage,
        ),
        (
            vec!["publish", package, "--git", &url, "--rev", &rev, "-O3"],
            usage,
        ),
        (
            vec![
                "publish", package, "--git", &url, "--git", &url, "--rev", &rev,
            ],
            usage,
        ),
        (
            vec![
                "publish",
                package,
                "--git",
                "http://example.org/g.git",
                "--rev",
                &rev,
            ],
            GIT_URL_RULE,
        ),
        (
            vec!["publish", package, "--git", &url, "--rev", "main"],
            GIT_REV_RULE,
        ),
        (
            vec![
                "publish",
                plain.to_str().unwrap(),
                "--git",
                &url,
                "--rev",
                &rev,
            ],
            "publish requires a Tsuzuri.toml in the package directory",
        ),
    ] {
        let output = scratch.tsuzuri(&arguments);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{arguments:?}: {}",
            stderr(&output)
        );
        assert!(
            stderr(&output).contains(&format!("error[E2000]: {message}")),
            "{arguments:?}: {}",
            stderr(&output)
        );
    }
}
