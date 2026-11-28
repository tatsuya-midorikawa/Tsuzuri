//! M15: the parts of `--emit app` that run on every host: the property-list writer and reader, the
//! `Info.plist` of a bundle, the Mach-O minimum version, the icns header, and the option checks.
//!
//! The golden files in `tests/fixtures/macos_bundle/` are not this compiler's output: each `X.plist` is
//! `plutil -convert xml1 -o X.plist X.source.plist` of a hand-written `X.source.plist`, made on macOS.

use std::path::PathBuf;

use tsuzuri::app_bundle::{
    dsym_info_plist, info_plist, mach_o_magic, mach_o_min_version, minimum_system_version,
    valid_icns,
};
use tsuzuri::diagnostic_kinds::DiagnosticKind;
use tsuzuri::driver::{BuildOptions, Emit, Target, Triple, app_executable_name, app_output_error};
use tsuzuri::package::MacosSettings;
use tsuzuri::plist::{self, Value};

const INFO: &[u8] = include_bytes!("fixtures/macos_bundle/Info.plist");
const EXTRA: &[u8] = include_bytes!("fixtures/macos_bundle/Extra.plist");
const EXTRA_SOURCE: &[u8] = include_bytes!("fixtures/macos_bundle/Extra.source.plist");
const DSYM: &[u8] = include_bytes!("fixtures/macos_bundle/Dsym.plist");

/// The `[macos]` of the Probe example of the specification, with a category.
fn probe() -> MacosSettings {
    MacosSettings {
        bundle_id: Some("dev.example.probe".into()),
        name: Some("Probe".into()),
        version: Some("0.1.0".into()),
        build: Some("1".into()),
        icon: Some(PathBuf::from("assets/Probe.icns")),
        category: Some("public.app-category.developer-tools".into()),
        ui_element: Some(true),
        ..MacosSettings::default()
    }
}

fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap()
}

#[test]
fn info_plist_matches_plutil_canonical_form() {
    let extra = plist::read_dictionary(EXTRA_SOURCE).unwrap();
    let build = || {
        let entries = info_plist("Probe", &probe(), "12.0", Some(("Extra.plist", &extra))).unwrap();
        plist::write(&Value::Dict(entries))
    };
    let written = build();
    // The keys order by UTF-16 code units (`1` `B` `Z` `_x` `a` `b`, then U+00E9, the surrogates of
    // U+1F600, U+FF61), and only `&`, `<` and `>` are escaped.
    assert_eq!(text(&written), text(INFO));
    assert_eq!(written, build(), "the output is deterministic");
}

#[test]
fn info_plist_writes_only_the_keys_that_are_set() {
    let settings = MacosSettings {
        bundle_id: Some("dev.example.min".into()),
        ..MacosSettings::default()
    };
    let entries = info_plist("Min", &settings, "27.2", None).unwrap();
    let keys: Vec<&str> = entries.keys().map(plist::Key::as_str).collect();
    assert_eq!(
        keys,
        [
            "CFBundleExecutable",
            "CFBundleIdentifier",
            "CFBundleInfoDictionaryVersion",
            "CFBundlePackageType",
            "LSMinimumSystemVersion",
            "NSHighResolutionCapable",
        ]
    );
    let written = String::from_utf8(plist::write(&Value::Dict(entries))).unwrap();
    assert!(written.contains("\t<key>LSMinimumSystemVersion</key>\n\t<string>27.2</string>\n"));
    // `[package]` never fills a key in: there is no name and no version here.
    assert!(!written.contains("CFBundleName") && !written.contains("CFBundleShortVersionString"));
    let no_identifier = info_plist("Min", &MacosSettings::default(), "12.0", None).unwrap_err();
    assert_eq!(
        no_identifier.kind,
        Some(DiagnosticKind::MacosBundleIdMissing.id())
    );
    assert_eq!(no_identifier.code, "E2000");
}

#[test]
fn dsym_info_plist_has_the_keys_of_dsymutil() {
    let written = plist::write(&Value::Dict(dsym_info_plist(&probe()).unwrap()));
    assert_eq!(text(&written), text(DSYM));
    // Without the versions of the bundle, dsymutil writes 1.0 and 1.
    let plain = MacosSettings {
        bundle_id: Some("dev.example.probe".into()),
        ..MacosSettings::default()
    };
    let written =
        String::from_utf8(plist::write(&Value::Dict(dsym_info_plist(&plain).unwrap()))).unwrap();
    assert!(written.contains("<key>CFBundleShortVersionString</key>\n\t<string>1.0</string>"));
    assert!(written.contains("<key>CFBundleVersion</key>\n\t<string>1</string>"));
}

#[test]
fn plist_read_round_trips() {
    // The hand-written file (comments, entities, both forms of an empty element, base64 on one line)
    // and plutil's form of it are the same value, and writing it gives plutil's bytes.
    let source = plist::read(EXTRA_SOURCE).unwrap();
    let canonical = plist::read(EXTRA).unwrap();
    assert_eq!(source, canonical);
    assert_eq!(text(&plist::write(&source)), text(EXTRA));
    let Value::Dict(entries) = source else {
        panic!("the top-level value is a dictionary");
    };
    assert_eq!(
        entries.get(&plist::Key::new("b")),
        Some(&Value::string("a & b < c > d \"q\" 's' AB\ttab"))
    );
    assert_eq!(
        entries.get(&plist::Key::new("ws")),
        Some(&Value::string("  two leading spaces\n and a second line  "))
    );
}

fn document(body: &str) -> Vec<u8> {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\">\n{body}\n</plist>\n"
    )
    .into_bytes()
}

#[test]
fn plist_read_rejects() {
    let rejected = |bytes: &[u8], reason: &str| {
        let error = plist::read_dictionary(bytes).expect_err(reason);
        assert!(error.contains(reason), "{error}");
    };
    rejected(b"bplist00\xd1\x01\x02", "plutil -convert xml1");
    rejected(
        &document("<dict><key>a</key><string><![CDATA[x]]></string></dict>"),
        "CDATA",
    );
    rejected(&document("<![CDATA[x]]>"), "CDATA");
    rejected(&document("<dict><key>a</key><uid/></dict>"), "<uid>");
    rejected(&document("<array/>"), "must be a <dict>");
    rejected(&document("<string>x</string>"), "must be a <dict>");
    rejected(
        &document("<dict><key>a</key><true/><key>a</key><false/></dict>"),
        "occurs twice",
    );
    rejected(&document("<dict><key>a</key></dict>"), "has no value");
    rejected(&document("<dict><true/></dict>"), "expected a <key>");
    rejected(
        &document("<dict><key>a</key><string>&nbsp;</string></dict>"),
        "&nbsp;",
    );
    rejected(
        &document("<dict><key>a</key><string>&#xD800;</string></dict>"),
        "&#xD800;",
    );
    rejected(
        &document("<dict><key>a</key><string>&#0;</string></dict>"),
        "&#0;",
    );
    rejected(
        &document("<dict><key>a</key><string>a<!-- c -->b</string></dict>"),
        "text only",
    );
    rejected(
        &document("<dict><key>a</key><string lang=\"en\">x</string></dict>"),
        "no attributes",
    );
    rejected(
        &document("<dict><key>a</key><integer>0x10</integer></dict>"),
        "<integer>",
    );
    rejected(
        &document("<dict><key>a</key><real>nan</real></dict>"),
        "<real>",
    );
    rejected(
        &document("<dict><key>a</key><date>2026-02-30T00:00:00Z</date></dict>"),
        "<date>",
    );
    rejected(
        &document("<dict><key>a</key><data>A</data></dict>"),
        "base64",
    );
    rejected(&document("<dict/><dict/>"), "expected </plist>");
    rejected(&document("<dict>"), "expected </dict>");
    rejected(b"<plist version=\"2.0\"><dict/></plist>", "\"1.0\"");
    rejected(
        b"<?xml version=\"1.0\"?><?pi x?><plist><dict/></plist>",
        "processing instructions",
    );
    rejected(
        b"<!DOCTYPE plist SYSTEM \"file:///etc/passwd\"><plist><dict/></plist>",
        "DOCTYPE",
    );
    rejected(
        b"<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"x\" [<!ENTITY a \"b\">]><plist><dict/></plist>",
        "DOCTYPE",
    );
    rejected(
        b"<?xml version=\"1.0\" encoding=\"UTF-16\"?><plist><dict/></plist>",
        "UTF-8",
    );
    rejected(b"<plist><dict/></plist>trailing", "after </plist>");
    rejected(&[0xff, 0xfe, 0x00], "UTF-8");

    // 128 levels are read; 129 are not.
    let nested = |depth: usize| {
        document(&format!(
            "<dict><key>k</key>{}{}</dict>",
            "<array>".repeat(depth - 1),
            "</array>".repeat(depth - 1)
        ))
    };
    assert!(plist::read_dictionary(&nested(plist::MAX_DEPTH)).is_ok());
    rejected(&nested(plist::MAX_DEPTH + 1), "deeper than 128");

    // 1 MiB is read; one byte more is not.
    let sized = |length: usize| {
        let frame = document("<dict><key>k</key><string></string></dict>").len();
        let bytes = document(&format!(
            "<dict><key>k</key><string>{}</string></dict>",
            "x".repeat(length - frame)
        ));
        assert_eq!(bytes.len(), length);
        bytes
    };
    assert!(plist::read_dictionary(&sized(plist::MAX_BYTES)).is_ok());
    rejected(&sized(plist::MAX_BYTES + 1), "1 MiB");
}

#[test]
fn plist_read_accepts_the_forms_of_real_files() {
    // The DOCTYPE that dsymutil and older tools write, a byte-order mark, and single quotes.
    let bytes = "\u{feff}<?xml version='1.0' encoding='utf-8'?>\n<!DOCTYPE plist PUBLIC \"-//Apple Computer//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version='1.0'>\n<dict>\n\t<key>k</key>\n\t<real>2.0</real>\n</dict >\n</plist >\n";
    let entries = plist::read_dictionary(bytes.as_bytes()).unwrap();
    // The text of a number is kept: plutil would write 2.
    assert_eq!(
        entries.get(&plist::Key::new("k")),
        Some(&Value::Real("2.0".into()))
    );
}

#[test]
fn info_plist_conflicts() {
    let extra = |key: &str| {
        plist::read_dictionary(&document(&format!(
            "<dict><key>{key}</key><string>x</string></dict>"
        )))
        .unwrap()
    };
    let conflict = |settings: &MacosSettings, key: &str| {
        let error = info_plist(
            "Probe",
            settings,
            "12.0",
            Some(("Extra.plist", &extra(key))),
        )
        .unwrap_err();
        assert_eq!(
            error.kind,
            Some(DiagnosticKind::MacosBundleConflict.id()),
            "{key}"
        );
        assert_eq!(error.code, "E2000");
        error.message.clone()
    };
    assert_eq!(
        conflict(&probe(), "CFBundleIdentifier"),
        "Info.plist key 'CFBundleIdentifier' comes from both [macos] bundle_id and 'Extra.plist'; remove one"
    );
    // A generated key conflicts whether or not `[macos]` sets it.
    let plain = MacosSettings {
        bundle_id: Some("dev.example.probe".into()),
        ..MacosSettings::default()
    };
    assert!(conflict(&plain, "CFBundleName").contains("belongs to [macos] name"));
    for key in [
        "CFBundleExecutable",
        "CFBundleInfoDictionaryVersion",
        "CFBundlePackageType",
        "LSMinimumSystemVersion",
        "NSHighResolutionCapable",
    ] {
        assert!(conflict(&plain, key).contains("is written by '--emit app'"));
    }
    for key in [
        "CFBundleShortVersionString",
        "CFBundleVersion",
        "CFBundleIconFile",
        "LSApplicationCategoryType",
        "LSUIElement",
    ] {
        conflict(&probe(), key);
        conflict(&plain, key);
    }
    // Any other key is added as it is.
    let entries = info_plist(
        "Probe",
        &plain,
        "12.0",
        Some(("Extra.plist", &extra("NSPrincipalClass"))),
    )
    .unwrap();
    assert_eq!(
        entries.get(&plist::Key::new("NSPrincipalClass")),
        Some(&Value::string("x"))
    );
}

/// A thin 64-bit Mach-O header with one segment command and one `LC_BUILD_VERSION` for `minos`.
fn mach_o(minos: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut words = |values: &[u32]| {
        for value in values {
            bytes.extend(value.to_le_bytes());
        }
    };
    // magic, cputype (arm64), cpusubtype, filetype (MH_EXECUTE), ncmds, sizeofcmds, flags, reserved
    words(&[0xfeed_facf, 0x0100_000c, 0, 2, 2, 72 + 24, 0, 0]);
    // LC_SEGMENT_64 (0x19) of 72 bytes: the command, its size, and 64 bytes of a segment
    words(&[0x19, 72]);
    words(&[0; 16]);
    // LC_BUILD_VERSION (0x32): platform 1 (macOS), minos, sdk, ntools
    words(&[0x32, 24, 1, minos, 27 << 16, 0]);
    bytes
}

#[test]
fn mach_o_min_version_reads_build_version() {
    for (minos, version, shown) in [
        (12 << 16, (12, 0, 0), "12.0"),
        ((13 << 16) | (1 << 8) | 2, (13, 1, 2), "13.1.2"),
        ((27 << 16) | (2 << 8), (27, 2, 0), "27.2"),
    ] {
        assert_eq!(mach_o_min_version(&mach_o(minos)), Some(version));
        assert_eq!(minimum_system_version(version), shown);
    }
    let thin = mach_o(12 << 16);
    // A fat file, a 32-bit file, a big-endian file, and a file that ends early have no answer.
    for magic in [
        [0xca, 0xfe, 0xba, 0xbe],
        [0xca, 0xfe, 0xba, 0xbf],
        [0xce, 0xfa, 0xed, 0xfe],
        [0xfe, 0xed, 0xfa, 0xcf],
    ] {
        let mut other = thin.clone();
        other[..4].copy_from_slice(&magic);
        assert_eq!(mach_o_min_version(&other), None);
    }
    for length in [0, 3, 31, 40, thin.len() - 1] {
        assert_eq!(mach_o_min_version(&thin[..length]), None, "{length}");
    }
    // Without the command there is no version.
    let mut none = thin.clone();
    none[16] = 1;
    assert_eq!(mach_o_min_version(&none), None);
    // The magics of the code that `embed` takes.
    assert!(mach_o_magic(&thin));
    assert!(mach_o_magic(&[0xca, 0xfe, 0xba, 0xbe, 0, 0]));
    assert!(mach_o_magic(&[0xca, 0xfe, 0xba, 0xbf]));
    assert!(!mach_o_magic(b"#!/bin/sh"));
    assert!(!mach_o_magic(&[0xcf, 0xfa, 0xed]));
}

#[test]
fn icns_header_is_checked() {
    let mut icon = b"icns".to_vec();
    icon.extend(16u32.to_be_bytes());
    icon.extend([0u8; 8]);
    assert!(valid_icns(&icon));
    assert!(!valid_icns(&icon[..7]), "a short file");
    assert!(
        !valid_icns(&icon[..12]),
        "the length names more bytes than there are"
    );
    let mut longer = icon.clone();
    longer.push(0);
    assert!(
        !valid_icns(&longer),
        "the length names fewer bytes than there are"
    );
    let mut png = icon.clone();
    png[..4].copy_from_slice(b"\x89PNG");
    assert!(!valid_icns(&png), "another magic");
    // The length is big endian.
    let mut little = b"icns".to_vec();
    little.extend(16u32.to_le_bytes());
    little.extend([0u8; 8]);
    assert!(!valid_icns(&little));
}

#[test]
fn emit_app_validation() {
    let app = BuildOptions {
        emit: Emit::App,
        ..BuildOptions::default()
    };
    let message = |options: BuildOptions, macos_host: bool| {
        let error = options.validate_app(macos_host).unwrap_err();
        assert_eq!(error.code, "E2000");
        assert_eq!(
            error.kind,
            Some(DiagnosticKind::OptionUnsupportedPlatform.id())
        );
        error.message.clone()
    };
    // The host is checked first: no other host has codesign.
    assert_eq!(
        message(app, false),
        "'--emit app' needs a macOS host: the bundle is signed with codesign"
    );
    const TARGET: &str =
        "'--emit app' builds a macOS application; build for aarch64-apple-darwin or remove it";
    for other in [
        BuildOptions {
            target: Target::Wasm32,
            ..app
        },
        BuildOptions {
            triple: Some(Triple::X86_64LinuxGnu),
            ..app
        },
        BuildOptions {
            triple: Some(Triple::X86_64WindowsMsvc),
            ..app
        },
    ] {
        assert_eq!(message(other, true), TARGET);
    }
    let named = BuildOptions {
        triple: Some(Triple::Aarch64AppleDarwin),
        ..app
    };
    assert!(named.validate_app(true).is_ok());
    // `validate` is the rule of this machine.
    if cfg!(target_os = "macos") {
        assert!(app.validate().is_ok());
        // Everything else is the rule of the executable inside: `--strip` is an option of executables,
        // and `--trap-mode return` is not.
        assert!(BuildOptions { strip: true, ..app }.validate().is_ok());
        let trapping = BuildOptions {
            trap_return: true,
            trap_info: true,
            ..app
        };
        let executable = BuildOptions {
            emit: Emit::Executable,
            ..trapping
        };
        assert_eq!(
            trapping.validate().unwrap_err().message,
            executable.validate().unwrap_err().message
        );
    } else {
        assert_eq!(
            app.validate().unwrap_err().message,
            "'--emit app' needs a macOS host: the bundle is signed with codesign"
        );
    }
    // The executable inside is built with the options of `--emit exe`.
    assert_eq!(app.bundle_executable().emit, Emit::Executable);
    assert_eq!(
        format!("{:?}", app.bundle_executable()),
        format!("{:?}", BuildOptions::default())
    );
}

#[test]
fn app_output_names_the_executable() {
    use std::path::Path;
    assert_eq!(
        app_executable_name(Path::new("out/Probe.app")),
        Some("Probe")
    );
    assert_eq!(
        app_executable_name(Path::new("My Tool.v2.app")),
        Some("My Tool.v2")
    );
    assert_eq!(
        BuildOptions {
            emit: Emit::App,
            ..BuildOptions::default()
        }
        .output_path(Path::new("dir/Main.tz")),
        Path::new("dir/Main.app")
    );
    for bad in ["Probe", "Probe.exe", ".app", "Probe.APP", "Probe.app.zip"] {
        assert_eq!(app_executable_name(Path::new(bad)), None, "{bad}");
        let error = app_output_error(Emit::App, Path::new(bad)).expect(bad);
        assert_eq!(error.code, "E2000");
        assert_eq!(
            error.kind,
            Some(DiagnosticKind::OutputExtensionInvalid.id())
        );
        assert_eq!(
            error.message,
            "app output must end with '.app'; its file name, without the extension, names the executable"
        );
    }
    // Other kinds of output have their own rules.
    assert!(app_output_error(Emit::Executable, Path::new("Probe")).is_none());
}

#[test]
fn emit_exe_ignores_macos_bundle_keys() {
    // The bundle keys change no build option: only `min_version` reaches the options (M17).
    let options = BuildOptions {
        triple: Some(Triple::Aarch64AppleDarwin),
        ..BuildOptions::default()
    };
    assert_eq!(
        format!("{:?}", options.with_manifest_macos(&probe())),
        format!(
            "{:?}",
            options.with_manifest_macos(&MacosSettings::default())
        )
    );
}
