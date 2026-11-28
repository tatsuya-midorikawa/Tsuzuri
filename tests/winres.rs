use tsuzuri::winres::{self, Name, RT_GROUP_ICON, RT_ICON};

const RT_DIALOG: u16 = 5;

const APP_MANIFEST: &[u8] = include_bytes!("fixtures/windows_resources/app.manifest");
const APP_ICO: &[u8] = include_bytes!("fixtures/windows_resources/app.ico");
const REF_MANIFEST_RES: &[u8] = include_bytes!("fixtures/windows_resources/manifest.res");
const REF_ICON_RES: &[u8] = include_bytes!("fixtures/windows_resources/icon.res");
const REF_VERSION_RES: &[u8] = include_bytes!("fixtures/windows_resources/version.res");
const REF_VERSION_3PART_RES: &[u8] = include_bytes!("fixtures/windows_resources/version_3part.res");
const REF_ALL_RES: &[u8] = include_bytes!("fixtures/windows_resources/all.res");
const REF_DIALOG_RES: &[u8] = include_bytes!("fixtures/windows_resources/dialog.res");

#[test]
fn manifest_icon_and_version_match_the_resource_compiler() {
    let manifest_res = winres::manifest(APP_MANIFEST).unwrap();
    let written_manifest = winres::write(std::slice::from_ref(&manifest_res));
    assert_eq!(written_manifest, REF_MANIFEST_RES);
    assert_eq!(
        written_manifest,
        winres::write(std::slice::from_ref(&manifest_res))
    );

    let icon_res = winres::icon(APP_ICO).unwrap();
    let written_icon = winres::write(&icon_res);
    assert_eq!(written_icon, REF_ICON_RES);
    assert_eq!(written_icon, winres::write(&icon_res));

    let version_res = winres::version_info("1.2.3.4", [1, 2, 3, 4]);
    let written_version = winres::write(std::slice::from_ref(&version_res));
    assert_eq!(written_version, REF_VERSION_RES);
    assert_eq!(
        written_version,
        winres::write(std::slice::from_ref(&version_res))
    );

    let version_3part = winres::version_info("1.2.3", [1, 2, 3, 0]);
    assert_eq!(
        winres::write(std::slice::from_ref(&version_3part)),
        REF_VERSION_3PART_RES
    );

    let combined = winres::merge(vec![
        ("[windows] manifest".to_owned(), vec![manifest_res]),
        ("[windows] icon".to_owned(), icon_res),
        ("[windows] version".to_owned(), vec![version_res]),
    ])
    .unwrap();
    assert_eq!(winres::write(&combined), REF_ALL_RES);
}

#[test]
fn read_round_trips() {
    for (name, reference) in [
        ("manifest.res", REF_MANIFEST_RES),
        ("icon.res", REF_ICON_RES),
        ("version.res", REF_VERSION_RES),
        ("version_3part.res", REF_VERSION_3PART_RES),
        ("all.res", REF_ALL_RES),
        ("dialog.res", REF_DIALOG_RES),
    ] {
        let parsed = winres::read(reference).expect(name);
        let rewritten = winres::write(&parsed);
        assert_eq!(rewritten, reference, "round-trip mismatch for {name}");
    }

    // String resource names round-trip with exact 4-byte alignment as well.
    let named = winres::Resource {
        kind: Name::Text("CUSTOMTYPE".encode_utf16().collect()),
        name: Name::Text("MAIN".encode_utf16().collect()),
        language: 0x0409,
        memory_flags: 0x0030,
        data_version: 0,
        version: 0,
        characteristics: 0,
        data: b"payload".to_vec(),
    };
    let bytes = winres::write(std::slice::from_ref(&named));
    assert_eq!(winres::read(&bytes).unwrap(), vec![named]);
}

#[test]
fn merge_renumbers_icons() {
    let icon_res = winres::icon(APP_ICO).unwrap();
    let dialog_res = winres::read(REF_DIALOG_RES).unwrap();

    let merged = winres::merge(vec![
        ("[windows] icon".to_owned(), icon_res),
        ("'dialog.rc'".to_owned(), dialog_res),
    ])
    .unwrap();

    let icon_ids: Vec<u16> = merged
        .iter()
        .filter(|r| r.kind == Name::Id(RT_ICON))
        .map(|r| match r.name {
            Name::Id(id) => id,
            Name::Text(_) => panic!("expected numeric RT_ICON id"),
        })
        .collect();
    assert_eq!(icon_ids, vec![1, 2, 3, 4]);

    let group_ids = |group_name: u16| -> Vec<u16> {
        let group = merged
            .iter()
            .find(|r| r.kind == Name::Id(RT_GROUP_ICON) && r.name == Name::Id(group_name))
            .expect("group icon");
        let count = usize::from(u16::from_le_bytes([group.data[4], group.data[5]]));
        (0..count)
            .map(|i| {
                let off = 6 + i * 14 + 12;
                u16::from_le_bytes([group.data[off], group.data[off + 1]])
            })
            .collect()
    };
    assert_eq!(group_ids(1), vec![1, 2]);
    assert_eq!(group_ids(2), vec![3, 4]);
    assert!(
        merged
            .iter()
            .any(|r| r.kind == Name::Id(RT_DIALOG) && r.name == Name::Id(100))
    );
}

#[test]
fn merge_rejects_duplicates() {
    let version_key = winres::version_info("1.2.3.4", [1, 2, 3, 4]);
    let version_rc = winres::read(REF_VERSION_RES).unwrap();
    let err = winres::merge(vec![
        ("[windows] version".to_owned(), vec![version_key]),
        ("'version.rc'".to_owned(), version_rc),
    ])
    .unwrap_err();
    assert_eq!(
        err,
        "resource VERSIONINFO 1 (language 1033) is defined by both [windows] version and 'version.rc'; remove one"
    );

    // Case-insensitive text name collision is also caught.
    let make_named = |text: &str| winres::Resource {
        kind: Name::Id(10),
        name: Name::Text(text.encode_utf16().collect()),
        language: 0x0409,
        memory_flags: 0x0030,
        data_version: 0,
        version: 0,
        characteristics: 0,
        data: vec![1],
    };
    let named_err = winres::merge(vec![
        ("'a.rc'".to_owned(), vec![make_named("MyData")]),
        ("'b.rc'".to_owned(), vec![make_named("MYDATA")]),
    ])
    .unwrap_err();
    assert!(
        named_err.contains("'a.rc'") && named_err.contains("'b.rc'"),
        "{named_err}"
    );
}

#[test]
fn invalid_inputs() {
    // Cursor type = 2 instead of icon type = 1.
    let mut cursor_ico = APP_ICO.to_vec();
    cursor_ico[2..4].copy_from_slice(&2u16.to_le_bytes());
    let err = winres::icon(&cursor_ico).unwrap_err();
    assert_eq!(
        err,
        "not an icon file: expected type 1 in the ICONDIR header"
    );

    // Out-of-bounds ICONDIRENTRY offset.
    let mut oob_ico = APP_ICO.to_vec();
    oob_ico[18..22].copy_from_slice(&999_999u32.to_le_bytes());
    assert!(winres::icon(&oob_ico).is_err());

    // 16-bit .res header / arbitrary garbage.
    let res16 = [0xffu8, 0x03, 0xff, 0x01, 0x30, 0x10, 0x04, 0x00];
    assert!(winres::read(&res16).is_err());

    // Invalid UTF-8 manifest and empty manifest.
    assert!(winres::manifest(&[0xff, 0xfe, 0x00]).is_err());
    assert!(winres::manifest(&[]).is_err());

    // UTF-8 BOM manifest is accepted.
    let mut bom_manifest = vec![0xEF, 0xBB, 0xBF];
    bom_manifest.extend_from_slice(APP_MANIFEST);
    assert!(winres::manifest(&bom_manifest).is_ok());

    // Manifest > 1 MiB and icon / .res > 64 MiB are rejected.
    let huge_manifest = vec![b'a'; winres::MAX_MANIFEST_BYTES + 1];
    assert!(winres::manifest(&huge_manifest).is_err());
    let huge_input = vec![0u8; winres::MAX_INPUT_BYTES + 1];
    assert!(winres::icon(&huge_input).is_err());
    assert!(winres::read(&huge_input).is_err());
}
