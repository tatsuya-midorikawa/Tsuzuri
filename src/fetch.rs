//! `tsuzuri fetch` and `tsuzuri publish`, the only commands that run `git` or use
//! the network.
//!
//! A git dependency names one commit; a registry dependency names a version that
//! the registry index maps to a commit. The fetcher downloads each commit into a
//! temporary bare repository, lists it with `ls-tree`, reads the package files with
//! `cat-file --batch`, and writes them itself into the package store as
//! `<cache root>/packages/git/<content sha256>/`, so git never checks out a
//! working tree (no hooks, filters, symbolic links, or submodules). The other
//! commands read only `Tsuzuri.lock` and the store.
//!
//! Registry versions are chosen by minimal version selection: every requirement
//! reachable from the graph, including those of versions that end up unselected,
//! takes part, and each package gets the newest of its required minimum versions.
//! Requirements of one package must share a compatibility range (one name, one version).

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::diagnostic::{Diagnostic, Span};
use crate::driver::{PackageRequest, Resolution, SourceError, TemporaryDirectory};
use crate::package::{DependencySource, IndexEntry, LockEntry, Version};
use crate::syntax::MAX_SOURCE_BYTES;

/// The most bytes that `git ls-tree` may print for one package or index.
const MAX_TREE_LISTING: u64 = 64 << 20;
/// The most packages and package versions that registry resolution visits.
const MAX_REGISTRY_PACKAGES: usize = 1024;
const MAX_REGISTRY_VERSIONS: usize = 16384;

/// Whether a package fetched from `parent_url` (`None` for the root package and
/// path dependencies) may depend on `url`: a package fetched over https cannot
/// make the build read the local repositories that a `file:///` url names.
pub fn url_allowed(parent_url: Option<&str>, url: &str) -> bool {
    !(parent_url.is_some_and(|parent| parent.starts_with("https://"))
        && url.starts_with("file:///"))
}

/// Downloads the git and registry dependencies of the package graph in
/// `directory` into the package store and writes `Tsuzuri.lock` once the whole
/// graph resolves.
pub fn fetch(directory: &Path) -> Result<(), SourceError> {
    let lock = crate::driver::read_lockfile(directory)?;
    let mut fetcher = Fetcher::new(
        lock.as_ref()
            .map(|lock| lock.entries.clone())
            .unwrap_or_default(),
        directory.join("Tsuzuri.lock"),
    );
    // The first walk downloads git packages and collects version requirements; once
    // the versions are selected and downloaded, the second walk checks the whole graph.
    let packages =
        crate::driver::load_packages(directory, &mut |request| fetcher.resolve(request))?;
    if !fetcher.requirements.is_empty() {
        let root = fs::canonicalize(directory).unwrap_or_default();
        let root = packages
            .iter()
            .find(|package| package.id.root == root)
            .expect("the walk loads the root package first");
        fetcher.select(&root.id.root.join("Tsuzuri.toml"), &root.manifest)?;
        crate::driver::load_packages(directory, &mut |request| fetcher.resolve(request))?;
    }
    write_lock(
        &fetcher.lock_path,
        &fetcher.new,
        lock.as_ref().map(|lock| lock.text.as_str()),
    )
}

/// Validates the package in `directory` for publication and returns the entry of
/// `index/<name>.json` that names its commit `rev` of `url`. The commit's package
/// files must equal the directory's. It changes no registry.
pub fn publish(directory: &Path, url: &str, rev: &str) -> Result<String, SourceError> {
    let manifest_path = directory.join("Tsuzuri.toml");
    let text = crate::driver::read_source_text(&manifest_path)
        .map_err(|error| SourceError::new(&manifest_path, error))?;
    let manifest = crate::package::parse_manifest(&text, 0)
        .map_err(|error| SourceError::new(&manifest_path, error))?;
    let location = Location {
        manifest: manifest_path.clone(),
        span: Span::default(),
    };
    let version = Version::parse(&manifest.version).ok_or_else(|| {
        location.error(
            "E1011",
            "a published package needs [package] version in MAJOR.MINOR.PATCH form without leading zeros, such as \"1.2.3\"".to_owned(),
        )
    })?;
    let mut dependencies = BTreeMap::new();
    for (name, dependency) in &manifest.dependencies {
        let DependencySource::Registry(requirement) = dependency.source else {
            return Err(SourceError::new(
                &manifest_path,
                Diagnostic::new(
                    "E1011",
                    format!(
                        "a published package can depend only on registry packages; '{name}' is a path or git dependency"
                    ),
                    dependency.span,
                ),
            ));
        };
        dependencies.insert(name.clone(), requirement);
    }
    if let Some((_, span)) = &manifest.native {
        return Err(SourceError::new(
            &manifest_path,
            Diagnostic::new(
                "E2000",
                "a published package cannot declare [native] link settings",
                *span,
            ),
        ));
    }
    let local = package_sha256(directory).ok_or_else(|| {
        location.error(
            "E2007",
            format!("cannot read the package files of {}", directory.display()),
        )
    })?;
    let mut fetcher = Fetcher::new(BTreeMap::new(), directory.join("Tsuzuri.lock"));
    let store = fetcher.store(&location)?;
    fetcher.check_git(&location)?;
    let download = download(&store, &manifest.name, url, rev, &location)?;
    let sha256 = download.sha256.clone();
    download.commit(&store)?;
    if sha256 != local {
        return Err(location.error(
            "E2007",
            format!(
                "the package in {} does not match {url} at {rev} (sha256 {local} here, {sha256} in the commit); commit and push the package, then publish that commit",
                directory.display()
            ),
        ));
    }
    Ok(crate::package::render_index_entry(
        version,
        &IndexEntry {
            git: url.to_owned(),
            rev: rev.to_owned(),
            sha256,
            dependencies,
        },
    ))
}

/// Where a diagnostic about a fetched package points: a manifest and a line in it.
#[derive(Clone)]
struct Location {
    manifest: PathBuf,
    span: Span,
}

impl Location {
    fn of(request: &PackageRequest<'_>) -> Self {
        Self {
            manifest: request.manifest.to_owned(),
            span: request.span,
        }
    }

    fn error(&self, code: &'static str, message: String) -> SourceError {
        SourceError::new(&self.manifest, Diagnostic::new(code, message, self.span))
    }
}

/// A version requirement of the registry graph and the package that states it.
#[derive(Clone)]
struct Requirement {
    name: String,
    version: Version,
    /// `app` for a manifest in the graph, or `name version` for an index entry.
    by: String,
    /// The manifest line that starts this chain of requirements.
    location: Location,
}

struct Fetcher {
    /// The entries of the existing `Tsuzuri.lock`.
    old: BTreeMap<String, LockEntry>,
    /// The git and registry packages of the graph, recorded as they resolve.
    new: BTreeMap<String, LockEntry>,
    lock_path: PathBuf,
    store: Option<PathBuf>,
    git_checked: bool,
    /// The version requirements of the graph's manifests, from the first walk.
    requirements: Vec<Requirement>,
    /// The selected registry versions, once [`Fetcher::select`] ran.
    selected: Option<BTreeMap<String, Version>>,
}

impl Fetcher {
    fn new(old: BTreeMap<String, LockEntry>, lock_path: PathBuf) -> Self {
        Self {
            old,
            new: BTreeMap::new(),
            lock_path,
            store: None,
            git_checked: false,
            requirements: Vec::new(),
            selected: None,
        }
    }

    fn resolve(&mut self, request: &PackageRequest<'_>) -> Result<Option<Resolution>, SourceError> {
        match request.source {
            DependencySource::Git { url, rev } => self.resolve_git(request, url, rev).map(Some),
            DependencySource::Registry(requirement) => {
                let Some(selected) = &self.selected else {
                    self.requirements.push(Requirement {
                        name: request.name.to_owned(),
                        version: *requirement,
                        by: request.parent.to_owned(),
                        location: Location::of(request),
                    });
                    return Ok(None);
                };
                let entry = selected
                    .get(request.name)
                    .and_then(|_| self.new.get(request.name))
                    .filter(|entry| {
                        entry
                            .version
                            .is_some_and(|version| version.satisfies(*requirement))
                    })
                    .ok_or_else(|| {
                        Location::of(request).error(
                            "E2007",
                            format!(
                                "registry dependency '{}' was not selected; run tsuzuri fetch again",
                                request.name
                            ),
                        )
                    })?;
                Ok(Some(Resolution {
                    root: self.store_path(&entry.sha256),
                    sha256: entry.sha256.clone(),
                    version: entry.version,
                }))
            }
            DependencySource::Path(_) => unreachable!("the walk resolves path dependencies itself"),
        }
    }

    fn store_path(&self, sha256: &str) -> PathBuf {
        self.store
            .as_ref()
            .expect("the store opens before any package resolves")
            .join("git")
            .join(sha256)
    }

    fn resolve_git(
        &mut self,
        request: &PackageRequest<'_>,
        url: &str,
        rev: &str,
    ) -> Result<Resolution, SourceError> {
        let location = Location::of(request);
        if !url_allowed(request.parent_url, url) {
            return Err(location.error(
                "E2007",
                format!(
                    "package '{}' was fetched over https and cannot use the file:// url of '{}'",
                    request.parent, request.name
                ),
            ));
        }
        let store = self.store(&location)?;
        // The graph walk guarantees one url and rev per name.
        if let Some(entry) = self.new.get(request.name) {
            return Ok(Resolution {
                root: store.join("git").join(&entry.sha256),
                sha256: entry.sha256.clone(),
                version: None,
            });
        }
        let old = self
            .old
            .get(request.name)
            .filter(|entry| entry.git == url && entry.rev == rev && entry.version.is_none())
            .cloned();
        if let Some(old) = &old {
            let root = store.join("git").join(&old.sha256);
            if store_sha256(&root).as_deref() == Some(old.sha256.as_str()) {
                self.new.insert(request.name.to_owned(), old.clone());
                return Ok(Resolution {
                    root,
                    sha256: old.sha256.clone(),
                    version: None,
                });
            }
        }
        self.check_git(&location)?;
        let download = download(&store, request.name, url, rev, &location)?;
        if let Some(old) = &old
            && old.sha256 != download.sha256
        {
            return Err(SourceError::new(
                &self.lock_path,
                Diagnostic::new(
                    "E2007",
                    format!(
                        "git dependency '{}' at rev {rev} has sha256 {} but Tsuzuri.lock records {}; remove its entry only if you trust the new content",
                        request.name, download.sha256, old.sha256
                    ),
                    Span::default(),
                ),
            ));
        }
        let sha256 = download.sha256.clone();
        let root = download.commit(&store)?;
        self.new.insert(
            request.name.to_owned(),
            LockEntry {
                git: url.to_owned(),
                rev: rev.to_owned(),
                sha256: sha256.clone(),
                version: None,
            },
        );
        Ok(Resolution {
            root,
            sha256,
            version: None,
        })
    }

    /// Selects a version of each registry package with the root's `[registry]`,
    /// downloads it, and checks it against its index entry.
    fn select(
        &mut self,
        manifest: &Path,
        root: &crate::package::Manifest,
    ) -> Result<(), SourceError> {
        let first = self.requirements[0].clone();
        let registry = root.registry.as_ref().ok_or_else(|| {
            first.location.error(
                "E2007",
                format!(
                    "registry dependency '{}' needs a [registry] index in the root package's Tsuzuri.toml",
                    first.name
                ),
            )
        })?;
        let at_registry = Location {
            manifest: manifest.to_owned(),
            span: registry.span,
        };
        let store = self.store(&at_registry)?;
        self.check_git(&at_registry)?;
        let mut index = Index::open(&store, registry, at_registry)?;
        let mut required = BTreeMap::<String, Vec<Requirement>>::new();
        let mut visited = BTreeSet::new();
        let mut queue = self.requirements.clone();
        while !queue.is_empty() {
            index.load(queue.iter().map(|requirement| requirement.name.as_str()))?;
            for requirement in std::mem::take(&mut queue) {
                let name = requirement.name.clone();
                let version = requirement.version;
                let Some(versions) = index.entries.get(&name) else {
                    return Err(requirement.location.error(
                        "E2007",
                        format!("the registry index has no package '{name}'"),
                    ));
                };
                let Some(entry) = versions.get(&version) else {
                    return Err(requirement.location.error(
                        "E2007",
                        format!(
                            "the registry index has no version {version} of '{name}' (required by {})",
                            requirement.by
                        ),
                    ));
                };
                if visited.insert((name.clone(), version)) {
                    queue.extend(entry.dependencies.iter().map(|(dependency, minimum)| {
                        Requirement {
                            name: dependency.clone(),
                            version: *minimum,
                            by: format!("{name} {version}"),
                            location: requirement.location.clone(),
                        }
                    }));
                }
                required.entry(name).or_default().push(requirement);
                if required.len() > MAX_REGISTRY_PACKAGES || visited.len() > MAX_REGISTRY_VERSIONS {
                    return Err(self.requirements[0].location.error(
                        "E1017",
                        format!(
                            "registry resolution exceeds {MAX_REGISTRY_PACKAGES} packages or {MAX_REGISTRY_VERSIONS} package versions"
                        ),
                    ));
                }
            }
        }
        // Each package series (a name and a compatibility range) gets the newest of its
        // required minimum versions, counting the requirements of every visited version
        // as minimal version selection does. Only the edges of the selected versions
        // decide which series the build needs, so a requirement that only an unselected
        // version states never conflicts.
        let mut newest = BTreeMap::<(&str, (u64, u64)), Version>::new();
        for (name, requirements) in &required {
            for requirement in requirements {
                let slot = newest
                    .entry((name.as_str(), requirement.version.series()))
                    .or_insert(requirement.version);
                if requirement.version > *slot {
                    *slot = requirement.version;
                }
            }
        }
        let mut reached = BTreeMap::<String, Requirement>::new();
        let mut pending: Vec<Requirement> = self.requirements.iter().rev().cloned().collect();
        while let Some(requirement) = pending.pop() {
            let series = requirement.version.series();
            let version = newest[&(requirement.name.as_str(), series)];
            if let Some(first) = reached.get(&requirement.name) {
                if first.version.series() != series {
                    let (lower, higher) = if first.version < requirement.version {
                        (first, &requirement)
                    } else {
                        (&requirement, first)
                    };
                    return Err(requirement.location.error(
                        "E1011",
                        format!(
                            "package '{}' is required at incompatible versions {} (by {}) and {} (by {}); one package name has one version",
                            requirement.name, lower.version, lower.by, higher.version, higher.by
                        ),
                    ));
                }
                continue;
            }
            let entry = &index.entries[&requirement.name][&version];
            pending.extend(
                entry
                    .dependencies
                    .iter()
                    .rev()
                    .map(|(dependency, minimum)| Requirement {
                        name: dependency.clone(),
                        version: *minimum,
                        by: format!("{} {version}", requirement.name),
                        location: requirement.location.clone(),
                    }),
            );
            reached.insert(requirement.name.clone(), requirement);
        }
        let mut selected = BTreeMap::new();
        for (name, first) in &reached {
            let version = newest[&(name.as_str(), first.version.series())];
            let entry = &index.entries[name][&version];
            self.fetch_registry_package(&store, &index.url, name, version, entry, &first.location)?;
            selected.insert(name.clone(), version);
        }
        self.selected = Some(selected);
        Ok(())
    }

    fn fetch_registry_package(
        &mut self,
        store: &Path,
        index: &str,
        name: &str,
        version: Version,
        entry: &IndexEntry,
        location: &Location,
    ) -> Result<(), SourceError> {
        if !url_allowed(Some(index), &entry.git) {
            return Err(location.error(
                "E2007",
                format!(
                    "the registry index was fetched over https and cannot use the file:// url of '{name}'"
                ),
            ));
        }
        if let Some(old) = self
            .old
            .get(name)
            .filter(|old| old.version == Some(version))
            && old.sha256 != entry.sha256
        {
            return Err(SourceError::new(
                &self.lock_path,
                Diagnostic::new(
                    "E2007",
                    format!(
                        "registry package '{name}' {version} has sha256 {} in the registry index but Tsuzuri.lock records {}; remove its entry only if you trust the new content",
                        entry.sha256, old.sha256
                    ),
                    Span::default(),
                ),
            ));
        }
        let mut root = store.join("git").join(&entry.sha256);
        if store_sha256(&root).as_deref() != Some(entry.sha256.as_str()) {
            self.check_git(location)?;
            let download = download(store, name, &entry.git, &entry.rev, location)?;
            if download.sha256 != entry.sha256 {
                return Err(location.error(
                    "E2007",
                    format!(
                        "registry package '{name}' {version} at {} has sha256 {} but the registry index records {}",
                        entry.rev, download.sha256, entry.sha256
                    ),
                ));
            }
            root = download.commit(store)?;
        }
        // The index lists the package's own requirements; the commit must agree.
        let mismatch = |detail: String| {
            location.error(
                "E2007",
                format!(
                    "registry package '{name}' {version} does not match its registry index entry ({detail})"
                ),
            )
        };
        let manifest = crate::driver::read_source_text(&root.join("Tsuzuri.toml"))
            .ok()
            .and_then(|text| crate::package::parse_manifest(&text, 0).ok())
            .ok_or_else(|| mismatch("its Tsuzuri.toml is not valid".to_owned()))?;
        if manifest.name != name {
            return Err(mismatch(format!("its package name is '{}'", manifest.name)));
        }
        if manifest.version != version.to_string() {
            return Err(mismatch(format!("its version is \"{}\"", manifest.version)));
        }
        let dependencies: BTreeMap<String, Version> = manifest
            .dependencies
            .iter()
            .map(|(dependency, declared)| match declared.source {
                DependencySource::Registry(requirement) => Ok((dependency.clone(), requirement)),
                _ => Err(mismatch(format!(
                    "'{dependency}' is a path or git dependency"
                ))),
            })
            .collect::<Result<_, _>>()?;
        if dependencies != entry.dependencies {
            return Err(mismatch("its dependencies differ".to_owned()));
        }
        self.new.insert(
            name.to_owned(),
            LockEntry {
                git: entry.git.clone(),
                rev: entry.rev.clone(),
                sha256: entry.sha256.clone(),
                version: Some(version),
            },
        );
        Ok(())
    }

    /// The package store, created on first use after the build cache marks its root.
    fn store(&mut self, location: &Location) -> Result<PathBuf, SourceError> {
        if let Some(store) = &self.store {
            return Ok(store.clone());
        }
        let root = crate::cache::default_root().ok_or_else(|| {
            location.error(
                "E2007",
                "no package store is available; set TSUZURI_CACHE_DIR and run tsuzuri fetch"
                    .to_owned(),
            )
        })?;
        let unusable = |detail: String| {
            SourceError::new(
                Path::new("<command line>"),
                Diagnostic::new(
                    "E2007",
                    format!(
                        "cannot use the cache directory {} for packages ({detail}); set TSUZURI_CACHE_DIR to an empty or Tsuzuri cache directory",
                        root.display()
                    ),
                    Span::default(),
                ),
            )
        };
        // Opening the build cache first writes its marker, so builds keep using the root.
        crate::cache::BuildCache::open(&root).map_err(|error| unusable(error.to_string()))?;
        let store = root.join("packages");
        for directory in [store.clone(), store.join("git")] {
            match fs::create_dir(&directory) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(unusable(error.to_string())),
            }
            if !fs::symlink_metadata(&directory)
                .is_ok_and(|metadata| metadata.is_dir() && !metadata.is_symlink())
            {
                return Err(unusable(format!(
                    "{} is not a directory",
                    directory.display()
                )));
            }
        }
        let store = fs::canonicalize(&store).map_err(|error| unusable(error.to_string()))?;
        self.store = Some(store.clone());
        Ok(store)
    }

    fn check_git(&mut self, location: &Location) -> Result<(), SourceError> {
        if self.git_checked {
            return Ok(());
        }
        let found = git_command()
            .arg("--version")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| {
                String::from_utf8_lossy(&output.stdout)
                    .split_whitespace()
                    .nth(2)
                    .map(str::to_owned)
            });
        let supported = found.as_deref().is_some_and(|version| {
            let mut parts = version.split('.').map(str::parse::<u64>);
            matches!((parts.next(), parts.next()), (Some(Ok(major)), Some(Ok(minor))) if (major, minor) >= (2, 32))
        });
        if !supported {
            return Err(location.error(
                "E2007",
                format!(
                    "git 2.32 or later is required to fetch git dependencies (found: {})",
                    found.as_deref().unwrap_or("none")
                ),
            ));
        }
        self.git_checked = true;
        Ok(())
    }
}

/// A registry index: one commit of a git repository whose `index/<name>.json`
/// files list the published versions of each package.
struct Index {
    url: String,
    repository: Repository,
    location: Location,
    /// The index files by package name.
    files: BTreeMap<String, TreeEntry>,
    entries: BTreeMap<String, BTreeMap<Version, IndexEntry>>,
    _temporary: TemporaryDirectory,
}

impl Index {
    fn open(
        store: &Path,
        registry: &crate::package::Registry,
        location: Location,
    ) -> Result<Self, SourceError> {
        let rev = registry.rev.as_deref().unwrap_or("HEAD");
        let failed = |command: &str, detail: String| {
            location.error(
                "E2007",
                format!(
                    "git {command} of the registry index {} at {rev} failed: {detail}; check the url, the commit id, and network access",
                    registry.index
                ),
            )
        };
        let temporary =
            TemporaryDirectory::new(store).map_err(|error| SourceError::new(store, error))?;
        let repository = Repository::create(&temporary, &registry.index)?;
        repository.init().map_err(|detail| failed("init", detail))?;
        repository
            .run(&[
                "fetch",
                "--quiet",
                "--depth=1",
                "--no-tags",
                "--no-recurse-submodules",
                &registry.index,
                rev,
            ])
            .map_err(|detail| failed("fetch", detail))?;
        let listing = repository
            .output(
                &["ls-tree", "-z", "-l", "--full-tree", "FETCH_HEAD", "index/"],
                MAX_TREE_LISTING,
            )
            .map_err(|failure| match failure {
                Failure::Git(detail) => failed("ls-tree", detail),
                Failure::TooLarge => location.error(
                    "E1017",
                    "the registry index has a listing larger than 64 MiB".to_owned(),
                ),
            })?;
        let mut files = BTreeMap::new();
        for record in listing
            .split(|byte| *byte == 0)
            .filter(|record| !record.is_empty())
        {
            let text = String::from_utf8_lossy(record);
            let Some((fields, path)) = text.split_once('\t') else {
                continue;
            };
            let fields: Vec<&str> = fields
                .split(' ')
                .filter(|field| !field.is_empty())
                .collect();
            let name = path
                .strip_prefix("index/")
                .and_then(|file| file.strip_suffix(".json"))
                .filter(|name| crate::package::namespace(name, Span::default()).is_ok());
            let (Some(name), ["100644" | "100755", "blob", oid, size]) = (name, &fields[..]) else {
                continue;
            };
            let Ok(size) = size.parse() else {
                continue;
            };
            files.insert(
                name.to_owned(),
                TreeEntry {
                    path: path.to_owned(),
                    oid: (*oid).to_owned(),
                    size,
                },
            );
        }
        Ok(Self {
            url: registry.index.clone(),
            repository,
            location,
            files,
            entries: BTreeMap::new(),
            _temporary: temporary,
        })
    }

    /// Reads and checks the index files of `names` that are not read yet.
    fn load<'a>(&mut self, names: impl Iterator<Item = &'a str>) -> Result<(), SourceError> {
        let names: BTreeSet<&str> = names
            .filter(|name| !self.entries.contains_key(*name))
            .filter(|name| self.files.contains_key(*name))
            .collect();
        let mut wanted = Vec::new();
        for name in &names {
            let file = &self.files[*name];
            if file.size > MAX_SOURCE_BYTES as u64 {
                return Err(self.location.error(
                    "E1017",
                    format!("the registry index file for '{name}' exceeds 1 MiB"),
                ));
            }
            wanted.push(TreeEntry {
                path: file.path.clone(),
                oid: file.oid.clone(),
                size: file.size,
            });
        }
        let blobs = self.repository.read_blobs(&wanted).map_err(|detail| {
            self.location.error(
                "E2007",
                format!(
                    "git cat-file of the registry index {} failed: {detail}",
                    self.url
                ),
            )
        })?;
        for (name, bytes) in names.into_iter().zip(blobs) {
            let versions = String::from_utf8(bytes)
                .map_err(|_| "not UTF-8".to_owned())
                .and_then(|text| crate::package::parse_index(name, &text))
                .map_err(|detail| {
                    self.location.error(
                        "E2007",
                        format!(
                            "the registry index file for '{name}' is invalid ({detail}); fix index/{name}.json in the registry index"
                        ),
                    )
                })?;
            self.entries.insert(name.to_owned(), versions);
        }
        Ok(())
    }
}

fn io_error(action: &str, path: &Path, error: io::Error) -> SourceError {
    SourceError::new(
        path,
        Diagnostic::new(
            "E2001",
            format!("cannot {action} '{}': {error}", path.display()),
            Span::default(),
        ),
    )
}

/// A `git` command that ignores the user's and the system's configuration and
/// every `GIT_*` variable, and never prompts for credentials.
fn git_command() -> Command {
    let mut command = Command::new("git");
    for (key, _) in std::env::vars_os() {
        let name = key.to_string_lossy().to_ascii_uppercase();
        if name.starts_with("GIT_") || name == "SSH_ASKPASS" {
            command.env_remove(&key);
        }
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .stdin(Stdio::null());
    command
}

/// The first `fatal:` or `error:` line of git's standard error, or its last line.
fn failure_line(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    lines
        .iter()
        .find(|line| line.starts_with("fatal:") || line.starts_with("error:"))
        .or(lines.last())
        .map_or_else(
            || "git exited without a message".to_owned(),
            |line| (*line).to_owned(),
        )
}

/// A temporary bare repository, with an empty hooks directory and global config.
struct Repository {
    hooks: PathBuf,
    config: PathBuf,
    git_dir: PathBuf,
    file_protocol: bool,
}

enum Failure {
    Git(String),
    TooLarge,
}

impl Repository {
    /// The paths of a repository for `url` in a temporary directory, with its empty
    /// hooks directory and global configuration.
    fn create(temporary: &TemporaryDirectory, url: &str) -> Result<Self, SourceError> {
        let repository = Self {
            hooks: temporary.path.join("hooks"),
            config: temporary.path.join("gitconfig"),
            git_dir: temporary.path.join("repo.git"),
            file_protocol: url.starts_with("file:///"),
        };
        fs::create_dir(&repository.hooks)
            .map_err(|error| io_error("create hooks directory", &repository.hooks, error))?;
        fs::File::create_new(&repository.config)
            .map_err(|error| io_error("create git config", &repository.config, error))?;
        Ok(repository)
    }

    fn init(&self) -> Result<(), String> {
        let mut template = OsString::from("--template=");
        template.push(&self.hooks);
        let output = git_command()
            .env("GIT_CONFIG_GLOBAL", &self.config)
            .args(["init", "--bare", "--quiet"])
            .arg(template)
            .arg(&self.git_dir)
            .output()
            .map_err(|error| error.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            Err(failure_line(&output.stderr))
        }
    }

    fn command(&self) -> Command {
        let mut command = git_command();
        command.env("GIT_CONFIG_GLOBAL", &self.config);
        for setting in [
            "protocol.allow=never",
            "protocol.https.allow=always",
            "transfer.fsckObjects=true",
            "http.followRedirects=false",
            "credential.helper=",
            "fetch.recurseSubmodules=false",
        ] {
            command.arg("-c").arg(setting);
        }
        if self.file_protocol {
            command.args(["-c", "protocol.file.allow=always"]);
        }
        let mut hooks = OsString::from("core.hooksPath=");
        hooks.push(&self.hooks);
        let mut git_dir = OsString::from("--git-dir=");
        git_dir.push(&self.git_dir);
        command.arg("-c").arg(hooks).arg(git_dir);
        command
    }

    fn run(&self, arguments: &[&str]) -> Result<(), String> {
        let output = self
            .command()
            .args(arguments)
            .output()
            .map_err(|error| error.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            Err(failure_line(&output.stderr))
        }
    }

    /// The standard output of a git command, which must not exceed `limit` bytes.
    fn output(&self, arguments: &[&str], limit: u64) -> Result<Vec<u8>, Failure> {
        let mut child = self
            .command()
            .args(arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| Failure::Git(error.to_string()))?;
        let mut stdout = child.stdout.take().expect("piped standard output");
        let mut stderr = child.stderr.take().expect("piped standard error");
        let (read, errors) = std::thread::scope(|scope| {
            let errors = scope.spawn(move || {
                let mut text = Vec::new();
                let _ = stderr.read_to_end(&mut text);
                text
            });
            let mut bytes = Vec::new();
            let read = (&mut stdout)
                .take(limit + 1)
                .read_to_end(&mut bytes)
                .map(|_| bytes);
            if !read.as_ref().is_ok_and(|bytes| bytes.len() as u64 <= limit) {
                let _ = child.kill();
            }
            drop(stdout);
            (read, errors.join().unwrap_or_default())
        });
        let status = child
            .wait()
            .map_err(|error| Failure::Git(error.to_string()))?;
        let bytes = read.map_err(|error| Failure::Git(error.to_string()))?;
        if bytes.len() as u64 > limit {
            return Err(Failure::TooLarge);
        }
        if !status.success() {
            return Err(Failure::Git(failure_line(&errors)));
        }
        Ok(bytes)
    }

    /// The bytes of each entry's blob, read through one `git cat-file --batch`.
    fn read_blobs(&self, entries: &[TreeEntry]) -> Result<Vec<Vec<u8>>, String> {
        let mut child = self
            .command()
            .args(["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| error.to_string())?;
        let mut stdin = child.stdin.take().expect("piped standard input");
        let stdout = child.stdout.take().expect("piped standard output");
        let mut stderr = child.stderr.take().expect("piped standard error");
        let (blobs, errors) = std::thread::scope(|scope| {
            // Writing requests and reading answers on one thread deadlocks once both pipes fill.
            scope.spawn(move || {
                for entry in entries {
                    if writeln!(stdin, "{}", entry.oid).is_err() {
                        break;
                    }
                }
            });
            let errors = scope.spawn(move || {
                let mut text = Vec::new();
                let _ = stderr.read_to_end(&mut text);
                text
            });
            let blobs = read_batch(BufReader::new(stdout), entries);
            if blobs.is_err() {
                let _ = child.kill();
            }
            (blobs, errors.join().unwrap_or_default())
        });
        let status = child.wait().map_err(|error| error.to_string())?;
        match blobs {
            Ok(blobs) if status.success() => Ok(blobs),
            Ok(_) => Err(failure_line(&errors)),
            Err(detail) if errors.is_empty() => Err(detail),
            Err(detail) => Err(format!("{detail} ({})", failure_line(&errors))),
        }
    }
}

fn read_batch(mut reader: impl BufRead, entries: &[TreeEntry]) -> Result<Vec<Vec<u8>>, String> {
    let mut blobs = Vec::with_capacity(entries.len());
    for entry in entries {
        let mut header = Vec::new();
        reader
            .by_ref()
            .take(256)
            .read_until(b'\n', &mut header)
            .map_err(|error| error.to_string())?;
        let header = String::from_utf8_lossy(&header);
        let header = header.trim_end();
        match header.split(' ').collect::<Vec<_>>()[..] {
            [oid, "blob", size] if oid == entry.oid && size.parse() == Ok(entry.size) => {}
            [oid, "missing"] => return Err(format!("object {oid} is missing")),
            _ => return Err(format!("unexpected cat-file output '{header}'")),
        }
        let mut bytes = vec![0; entry.size as usize];
        let mut newline = [0];
        reader
            .read_exact(&mut bytes)
            .and_then(|()| reader.read_exact(&mut newline))
            .map_err(|error| error.to_string())?;
        if newline != *b"\n" {
            return Err("unexpected cat-file output".to_owned());
        }
        blobs.push(bytes);
    }
    Ok(blobs)
}

/// A file of the package: the root `Tsuzuri.toml` or a source.
struct TreeEntry {
    path: String,
    oid: String,
    size: u64,
}

/// Whether a tree path belongs to the package, as `collect_sources` would choose it:
/// no segment starts with `.`, and it is the root manifest or a source file.
fn selected(path: &[u8]) -> bool {
    !path
        .split(|byte| *byte == b'/')
        .any(|segment| segment.first() == Some(&b'.'))
        && (path == b"Tsuzuri.toml"
            || crate::driver::source_kind(Path::new(&*String::from_utf8_lossy(path))).is_some())
}

/// The package files of a `git ls-tree -r -z -l --full-tree` listing, checked
/// against the store's path rules and the limits of module discovery.
fn parse_tree(
    name: &str,
    rev: &str,
    listing: &[u8],
) -> Result<Vec<TreeEntry>, (&'static str, String)> {
    let unreadable = || {
        (
            "E2007",
            format!("git dependency '{name}' has an unreadable tree listing"),
        )
    };
    let limits = || {
        (
            "E1017",
            "module discovery exceeds 1024 directories or 16 path segments".to_owned(),
        )
    };
    let mut entries = Vec::new();
    let mut manifest = false;
    let mut directories = BTreeSet::new();
    // Every file and directory by its lowercase form: case-insensitive file systems merge them.
    let mut folded = BTreeMap::<String, String>::new();
    for record in listing
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let tab = record
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or_else(unreadable)?;
        let (fields, path) = (&record[..tab], &record[tab + 1..]);
        if !selected(path) {
            continue;
        }
        let path = std::str::from_utf8(path).map_err(|_| {
            (
                "E2007",
                format!(
                    "git dependency '{name}' has an unsafe path '{}'",
                    String::from_utf8_lossy(path)
                ),
            )
        })?;
        let fields = std::str::from_utf8(fields).map_err(|_| unreadable())?;
        let [mode, kind, oid, size] = fields
            .split(' ')
            .filter(|field| !field.is_empty())
            .collect::<Vec<_>>()[..]
        else {
            return Err(unreadable());
        };
        let segments: Vec<&str> = path.split('/').collect();
        if segments.iter().any(|segment| {
            segment.is_empty()
                || matches!(*segment, "." | "..")
                || segment.len() > 255
                || segment.eq_ignore_ascii_case(".git")
                || segment
                    .chars()
                    .any(|character| character.is_control() || matches!(character, '\\' | ':'))
        }) {
            return Err((
                "E2007",
                format!("git dependency '{name}' has an unsafe path '{path}'"),
            ));
        }
        if !matches!((mode, kind), ("100644" | "100755", "blob")) {
            return Err((
                "E2007",
                format!(
                    "git dependency '{name}' has a symbolic link or submodule at '{path}'; packages may contain only regular source files"
                ),
            ));
        }
        let size: u64 = size.parse().map_err(|_| unreadable())?;
        if size > MAX_SOURCE_BYTES as u64 {
            return Err((
                "E0003",
                format!("source exceeds the {MAX_SOURCE_BYTES}-byte limit"),
            ));
        }
        if path == "Tsuzuri.toml" {
            manifest = true;
        } else if entries.len() - usize::from(manifest) >= 4096 {
            return Err((
                "E1017",
                "module discovery exceeds 4096 source files".to_owned(),
            ));
        }
        if segments.len() > 16 {
            return Err(limits());
        }
        for depth in 1..segments.len() {
            directories.insert(segments[..depth].join("/"));
        }
        if directories.len() >= 1024 {
            return Err(limits());
        }
        for depth in 1..=segments.len() {
            let prefix = segments[..depth].join("/");
            match folded.get(&prefix.to_lowercase()) {
                Some(other) if *other != prefix => {
                    return Err((
                        "E2007",
                        format!(
                            "git dependency '{name}' has paths that differ only in case: '{other}' and '{prefix}'"
                        ),
                    ));
                }
                Some(_) => {}
                None => {
                    folded.insert(prefix.to_lowercase(), prefix);
                }
            }
        }
        entries.push(TreeEntry {
            path: path.to_owned(),
            oid: oid.to_owned(),
            size,
        });
    }
    if !manifest {
        return Err((
            "E2007",
            format!(
                "git dependency '{name}' has no Tsuzuri.toml at the repository root of rev {rev}"
            ),
        ));
    }
    Ok(entries)
}

/// A downloaded package, staged in a temporary directory of the store.
struct Download {
    temporary: TemporaryDirectory,
    package: PathBuf,
    sha256: String,
}

fn download(
    store: &Path,
    name: &str,
    url: &str,
    rev: &str,
    location: &Location,
) -> Result<Download, SourceError> {
    let failed = |command: &str, detail: String| {
        location.error(
            "E2007",
            format!(
                "git {command} of {url} at {rev} failed: {detail}; check the url, the commit id, and network access"
            ),
        )
    };
    let temporary =
        TemporaryDirectory::new(store).map_err(|error| SourceError::new(store, error))?;
    let repository = Repository::create(&temporary, url)?;
    repository.init().map_err(|detail| failed("init", detail))?;
    repository
        .run(&[
            "fetch",
            "--quiet",
            "--depth=1",
            "--no-tags",
            "--no-recurse-submodules",
            url,
            rev,
        ])
        .map_err(|detail| failed("fetch", detail))?;
    let listing = repository
        .output(
            &["ls-tree", "-r", "-z", "-l", "--full-tree", rev],
            MAX_TREE_LISTING,
        )
        .map_err(|failure| match failure {
            Failure::Git(detail) => failed("ls-tree", detail),
            Failure::TooLarge => location.error(
                "E1017",
                format!("git dependency '{name}' has a tree listing larger than 64 MiB"),
            ),
        })?;
    let entries =
        parse_tree(name, rev, &listing).map_err(|(code, message)| location.error(code, message))?;
    let blobs = repository
        .read_blobs(&entries)
        .map_err(|detail| failed("cat-file", detail))?;
    let package = temporary.path.join("package");
    let mut files = BTreeMap::new();
    for (entry, bytes) in entries.iter().zip(blobs) {
        let text = String::from_utf8(bytes).map_err(|_| {
            location.error(
                "E2007",
                format!(
                    "git dependency '{name}' has a source that is not UTF-8: '{}'",
                    entry.path
                ),
            )
        })?;
        let path = entry
            .path
            .split('/')
            .fold(package.clone(), |path, segment| path.join(segment));
        let parent = path.parent().expect("package files are below the package");
        fs::create_dir_all(parent)
            .map_err(|error| io_error("create package directory", parent, error))?;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .and_then(|mut file| file.write_all(text.as_bytes()))
            .map_err(|error| io_error("write package file", &path, error))?;
        files.insert(entry.path.clone(), text);
    }
    Ok(Download {
        sha256: crate::package::content_sha256(&files),
        temporary,
        package,
    })
}

impl Download {
    /// Moves the package to `<store>/git/<sha256>`, replacing a damaged entry.
    fn commit(self, store: &Path) -> Result<PathBuf, SourceError> {
        let target = store.join("git").join(&self.sha256);
        for attempt in 0..3 {
            match fs::symlink_metadata(&target) {
                Ok(metadata) => {
                    if metadata.is_dir()
                        && !metadata.is_symlink()
                        && store_sha256(&target).as_deref() == Some(self.sha256.as_str())
                    {
                        return Ok(target);
                    }
                    // The damaged entry goes away with the temporary directory.
                    let stale = self.temporary.path.join(format!("stale-{attempt}"));
                    match fs::rename(&target, &stale) {
                        Ok(()) => {}
                        Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                        Err(error) => {
                            return Err(io_error("replace damaged package", &target, error));
                        }
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(io_error("inspect package", &target, error)),
            }
            match fs::rename(&self.package, &target) {
                Ok(()) => return Ok(target),
                // Another fetch stored the package first; check it again.
                Err(_) if fs::symlink_metadata(&target).is_ok() => {}
                Err(error) => return Err(io_error("store package", &target, error)),
            }
        }
        Err(io_error(
            "store package",
            &target,
            io::Error::other("the entry kept changing"),
        ))
    }
}

/// The content hash of a stored package, or `None` when it is missing or damaged.
fn store_sha256(root: &Path) -> Option<String> {
    if !fs::symlink_metadata(root).is_ok_and(|metadata| metadata.is_dir() && !metadata.is_symlink())
    {
        return None;
    }
    package_sha256(root)
}

/// The content hash of the package files below `root`, as module discovery finds them.
fn package_sha256(root: &Path) -> Option<String> {
    let manifest = root.join("Tsuzuri.toml");
    if !fs::symlink_metadata(&manifest).is_ok_and(|metadata| metadata.is_file()) {
        return None;
    }
    let mut files = BTreeMap::new();
    files.insert(
        "Tsuzuri.toml".to_owned(),
        crate::driver::read_source_text(&manifest).ok()?,
    );
    for path in crate::driver::collect_sources(root, &[]).ok()? {
        let relative = path.strip_prefix(root).ok()?;
        let key = relative
            .components()
            .map(|part| part.as_os_str().to_str())
            .collect::<Option<Vec<_>>>()?
            .join("/");
        files.insert(key, crate::driver::read_source_text(&path).ok()?);
    }
    Some(crate::package::content_sha256(&files))
}

/// Writes the canonical lockfile through a temporary file and a rename. It writes
/// nothing when the text is unchanged, or when there is neither a git package nor
/// an existing lockfile.
fn write_lock(
    path: &Path,
    entries: &BTreeMap<String, LockEntry>,
    existing: Option<&str>,
) -> Result<(), SourceError> {
    if entries.is_empty() && existing.is_none() {
        return Ok(());
    }
    let text = crate::package::render_lock(entries);
    if existing == Some(text.as_str()) {
        return Ok(());
    }
    let temporary = path.with_file_name(format!(".Tsuzuri.lock.{}.tmp", std::process::id()));
    let _ = fs::remove_file(&temporary);
    let written = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .and_then(|mut file| {
            file.write_all(text.as_bytes())?;
            file.sync_all()
        })
        .and_then(|()| fs::rename(&temporary, path));
    if let Err(error) = written {
        let _ = fs::remove_file(&temporary);
        return Err(io_error("write lockfile", path, error));
    }
    Ok(())
}
