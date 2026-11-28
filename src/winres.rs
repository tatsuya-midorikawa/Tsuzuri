//! 32-bit Windows `.res` resource reader, writer, generator, and merger (W10).
//!
//! Generates `RT_MANIFEST`, `RT_ICON` / `RT_GROUP_ICON`, and `RT_VERSION` (`VS_VERSIONINFO`)
//! deterministically in Rust without external tools, reads `.res` files produced by `rc.exe`,
//! `llvm-rc`, or `zig rc`, and merges multiple inputs into a single `.res` while renumbering
//! numeric `RT_ICON` / `RT_CURSOR` IDs and diagnosing duplicate `(type, name, language)` triples.

use std::collections::BTreeMap;

pub const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
pub const MAX_INPUT_BYTES: usize = 64 * 1024 * 1024;
pub const LANGUAGE_EN_US: u16 = 0x0409;

pub const RT_CURSOR: u16 = 1;
pub const RT_ICON: u16 = 3;
pub const RT_GROUP_CURSOR: u16 = 12;
pub const RT_GROUP_ICON: u16 = 14;
pub const RT_VERSION: u16 = 16;
pub const RT_MANIFEST: u16 = 24;

const PROLOGUE: [u8; 32] = [
    0, 0, 0, 0, // DataSize = 0
    32, 0, 0, 0, // HeaderSize = 32
    0xff, 0xff, 0, 0, // Type = Id(0)
    0xff, 0xff, 0, 0, // Name = Id(0)
    0, 0, 0, 0, // DataVersion = 0
    0, 0, // MemoryFlags = 0
    0, 0, // LanguageId = 0
    0, 0, 0, 0, // Version = 0
    0, 0, 0, 0, // Characteristics = 0
];

/// One resource record of a 32-bit `.res` file (W10).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resource {
    pub kind: Name,
    pub name: Name,
    pub language: u16,
    pub memory_flags: u16,
    pub data_version: u32,
    pub version: u32,
    pub characteristics: u32,
    pub data: Vec<u8>,
}

/// A resource type or name identifier in a `.res` header: a 16-bit ordinal or UTF-16 text.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Name {
    Id(u16),
    Text(Vec<u16>),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum NameKey {
    Id(u16),
    Text(Vec<u16>),
}

impl NameKey {
    fn of(name: &Name) -> Self {
        match name {
            Name::Id(id) => Self::Id(*id),
            Name::Text(units) => Self::Text(
                units
                    .iter()
                    .map(|&unit| {
                        if (u16::from(b'a')..=u16::from(b'z')).contains(&unit) {
                            unit - 32
                        } else {
                            unit
                        }
                    })
                    .collect(),
            ),
        }
    }
}

/// Builds `RT_MANIFEST` ID 1 (language `0x0409`, flags `0x0030`) from UTF-8 XML bytes.
pub fn manifest(bytes: &[u8]) -> Result<Resource, String> {
    if bytes.is_empty() {
        return Err("not a valid manifest: file is empty".to_owned());
    }
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err("not a valid manifest: exceeds the 1 MiB limit".to_owned());
    }
    if std::str::from_utf8(bytes).is_err() {
        return Err("not a valid manifest: expected UTF-8 text".to_owned());
    }
    Ok(Resource {
        kind: Name::Id(RT_MANIFEST),
        name: Name::Id(1),
        language: LANGUAGE_EN_US,
        memory_flags: 0x0030,
        data_version: 0,
        version: 0,
        characteristics: 0,
        data: bytes.to_vec(),
    })
}

/// Builds `RT_ICON` `1..=n` (flags `0x1010`) followed by `RT_GROUP_ICON` `1` (flags `0x1030`)
/// from `.ico` file bytes.
pub fn icon(bytes: &[u8]) -> Result<Vec<Resource>, String> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err("not an icon file: exceeds the 64 MiB limit".to_owned());
    }
    if bytes.len() < 6 {
        return Err("not an icon file: truncated ICONDIR header".to_owned());
    }
    let reserved = u16::from_le_bytes([bytes[0], bytes[1]]);
    let ico_type = u16::from_le_bytes([bytes[2], bytes[3]]);
    let count = u16::from_le_bytes([bytes[4], bytes[5]]);
    if reserved != 0 {
        return Err("not an icon file: expected reserved 0 in the ICONDIR header".to_owned());
    }
    if ico_type != 1 {
        return Err("not an icon file: expected type 1 in the ICONDIR header".to_owned());
    }
    if count == 0 {
        return Err("not an icon file: ICONDIR contains no images".to_owned());
    }
    if count == u16::MAX {
        return Err("not an icon file: too many images in ICONDIR".to_owned());
    }
    let entries_end = 6usize
        .checked_add(
            usize::from(count)
                .checked_mul(16)
                .ok_or_else(|| "not an icon file: ICONDIR entry count overflows".to_owned())?,
        )
        .ok_or_else(|| "not an icon file: ICONDIR header size overflows".to_owned())?;
    if entries_end > bytes.len() {
        return Err("not an icon file: truncated ICONDIRENTRY table".to_owned());
    }

    let mut out = Vec::with_capacity(usize::from(count) + 1);
    let mut group_data = Vec::with_capacity(6 + usize::from(count) * 14);
    group_data.extend_from_slice(&0u16.to_le_bytes());
    group_data.extend_from_slice(&1u16.to_le_bytes());
    group_data.extend_from_slice(&count.to_le_bytes());

    for index in 0..usize::from(count) {
        let entry_offset = 6 + index * 16;
        let entry = &bytes[entry_offset..entry_offset + 16];
        let image_size = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]) as usize;
        let image_offset =
            u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]) as usize;
        let Some(image_end) = image_offset.checked_add(image_size) else {
            return Err("not an icon file: ICONDIRENTRY image range overflows".to_owned());
        };
        if image_size == 0 || image_offset < entries_end || image_end > bytes.len() {
            return Err("not an icon file: ICONDIRENTRY image range is out of bounds".to_owned());
        }
        let id = 1 + index as u16;
        out.push(Resource {
            kind: Name::Id(RT_ICON),
            name: Name::Id(id),
            language: LANGUAGE_EN_US,
            memory_flags: 0x1010,
            data_version: 0,
            version: 0,
            characteristics: 0,
            data: bytes[image_offset..image_end].to_vec(),
        });
        group_data.extend_from_slice(&entry[..12]);
        group_data.extend_from_slice(&id.to_le_bytes());
    }

    out.push(Resource {
        kind: Name::Id(RT_GROUP_ICON),
        name: Name::Id(1),
        language: LANGUAGE_EN_US,
        memory_flags: 0x1030,
        data_version: 0,
        version: 0,
        characteristics: 0,
        data: group_data,
    });
    Ok(out)
}

/// Builds `RT_VERSION` ID 1 (`VS_VERSIONINFO`, language `0x0409`, flags `0x0030`) matching the
/// exact layout produced by `rc.exe`, `llvm-rc`, and `zig rc`.
pub fn version_info(text: &str, parts: [u16; 4]) -> Resource {
    let mut fixed = Vec::with_capacity(52);
    let ms = (u32::from(parts[0]) << 16) | u32::from(parts[1]);
    let ls = (u32::from(parts[2]) << 16) | u32::from(parts[3]);
    for dword in [
        0xFEEF_04BDu32, // dwSignature
        0x0001_0000,    // dwStrucVersion
        ms,             // dwFileVersionMS
        ls,             // dwFileVersionLS
        ms,             // dwProductVersionMS
        ls,             // dwProductVersionLS
        0x0000_003F,    // dwFileFlagsMask
        0,              // dwFileFlags
        0x0004_0004,    // dwFileOS (VOS_NT_WINDOWS32)
        1,              // dwFileType (VFT_APP)
        0,              // dwFileSubtype
        0,              // dwFileDateMS
        0,              // dwFileDateLS
    ] {
        fixed.extend_from_slice(&dword.to_le_bytes());
    }

    let text_utf16_nul: Vec<u8> = text
        .encode_utf16()
        .chain(std::iter::once(0u16))
        .flat_map(u16::to_le_bytes)
        .collect();
    let text_words = u16::try_from(text_utf16_nul.len() / 2).expect("version string fits in u16");

    let file_ver = version_node("FileVersion", 1, text_words, &text_utf16_nul, &[]);
    let prod_ver = version_node("ProductVersion", 1, text_words, &text_utf16_nul, &[]);
    let string_table = version_node("040904b0", 1, 0, &[], &[file_ver, prod_ver]);
    let string_file_info = version_node("StringFileInfo", 1, 0, &[], &[string_table]);

    let translation_val = [0x09u8, 0x04, 0xb0, 0x04]; // 0x0409, 1200
    let translation = version_node("Translation", 0, 4, &translation_val, &[]);
    let var_file_info = version_node("VarFileInfo", 1, 0, &[], &[translation]);

    let root = version_node(
        "VS_VERSION_INFO",
        0,
        u16::try_from(fixed.len()).unwrap(),
        &fixed,
        &[string_file_info, var_file_info],
    );

    Resource {
        kind: Name::Id(RT_VERSION),
        name: Name::Id(1),
        language: LANGUAGE_EN_US,
        memory_flags: 0x0030,
        data_version: 0,
        version: 0,
        characteristics: 0,
        data: root,
    }
}

fn version_node(
    key: &str,
    value_type: u16,
    value_length: u16,
    value: &[u8],
    children: &[Vec<u8>],
) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes()); // placeholder for wLength
    out.extend_from_slice(&value_length.to_le_bytes());
    out.extend_from_slice(&value_type.to_le_bytes());
    for unit in key.encode_utf16().chain(std::iter::once(0u16)) {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    if !value.is_empty() || !children.is_empty() {
        pad4(&mut out);
    }
    out.extend_from_slice(value);
    for child in children {
        pad4(&mut out);
        out.extend_from_slice(child);
    }
    let len = u16::try_from(out.len()).expect("VS_VERSIONINFO node fits in u16");
    out[0..2].copy_from_slice(&len.to_le_bytes());
    out
}

/// Parses a 32-bit `.res` binary into individual [`Resource`] entries.
pub fn read(bytes: &[u8]) -> Result<Vec<Resource>, String> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err("not a 32-bit .res file: exceeds the 64 MiB limit".to_owned());
    }
    if bytes.len() < PROLOGUE.len() || bytes[..PROLOGUE.len()] != PROLOGUE {
        return Err("not a 32-bit .res file: missing the 32-byte empty resource header".to_owned());
    }
    let mut out = Vec::new();
    let mut offset = PROLOGUE.len();
    while offset < bytes.len() {
        if bytes.len() - offset < 8 {
            return Err("invalid .res file: truncated resource entry header".to_owned());
        }
        let data_size = u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        let header_size = u32::from_le_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]) as usize;
        if header_size < 32 || header_size % 4 != 0 {
            return Err("invalid .res file: malformed resource header size".to_owned());
        }
        let Some(header_end) = offset.checked_add(header_size) else {
            return Err("invalid .res file: resource header size overflows".to_owned());
        };
        if header_end > bytes.len() {
            return Err("invalid .res file: resource header extends past end of file".to_owned());
        }

        let names_end = header_end - 16;
        let mut cursor = offset + 8;
        let kind = read_name(bytes, &mut cursor, names_end)?;
        let name = read_name(bytes, &mut cursor, names_end)?;
        let aligned = offset + align4(cursor - offset)?;
        if aligned != names_end {
            return Err("invalid .res file: unexpected bytes in resource header".to_owned());
        }
        if bytes[cursor..aligned].iter().any(|&b| b != 0) {
            return Err("invalid .res file: non-zero header padding".to_owned());
        }

        let meta = &bytes[names_end..header_end];
        let data_version = u32::from_le_bytes([meta[0], meta[1], meta[2], meta[3]]);
        let memory_flags = u16::from_le_bytes([meta[4], meta[5]]);
        let language = u16::from_le_bytes([meta[6], meta[7]]);
        let version = u32::from_le_bytes([meta[8], meta[9], meta[10], meta[11]]);
        let characteristics = u32::from_le_bytes([meta[12], meta[13], meta[14], meta[15]]);

        let Some(data_end) = header_end.checked_add(data_size) else {
            return Err("invalid .res file: resource data size overflows".to_owned());
        };
        let padded_data = align4(data_size)?;
        let Some(record_end) = header_end.checked_add(padded_data) else {
            return Err("invalid .res file: padded resource record overflows".to_owned());
        };
        if record_end > bytes.len() {
            return Err("invalid .res file: resource data extends past end of file".to_owned());
        }

        if !(data_size == 0 && kind == Name::Id(0) && name == Name::Id(0)) {
            out.push(Resource {
                kind,
                name,
                language,
                memory_flags,
                data_version,
                version,
                characteristics,
                data: bytes[header_end..data_end].to_vec(),
            });
        }
        offset = record_end;
    }
    Ok(out)
}

fn read_name(bytes: &[u8], cursor: &mut usize, limit: usize) -> Result<Name, String> {
    if limit.saturating_sub(*cursor) < 2 {
        return Err("invalid .res file: truncated resource name".to_owned());
    }
    let first = u16::from_le_bytes([bytes[*cursor], bytes[*cursor + 1]]);
    if first == 0xFFFF {
        if limit - *cursor < 4 {
            return Err("invalid .res file: truncated ordinal resource name".to_owned());
        }
        let id = u16::from_le_bytes([bytes[*cursor + 2], bytes[*cursor + 3]]);
        *cursor += 4;
        return Ok(Name::Id(id));
    }
    let mut units = Vec::new();
    loop {
        if limit.saturating_sub(*cursor) < 2 {
            return Err("invalid .res file: unterminated string resource name".to_owned());
        }
        let unit = u16::from_le_bytes([bytes[*cursor], bytes[*cursor + 1]]);
        *cursor += 2;
        if unit == 0 {
            break;
        }
        units.push(unit);
    }
    if units.is_empty() {
        return Err("invalid .res file: empty string resource name".to_owned());
    }
    Ok(Name::Text(units))
}

/// Merges named resource inputs in order, renumbering numeric `RT_ICON` / `RT_CURSOR` IDs per
/// input (and updating matching `RT_GROUP_ICON` / `RT_GROUP_CURSOR` directory entries) and
/// rejecting duplicate `(type, name, language)` triples.
pub fn merge(inputs: Vec<(String, Vec<Resource>)>) -> Result<Vec<Resource>, String> {
    let mut max_icon_id: u16 = 0;
    let mut max_cursor_id: u16 = 0;
    let mut seen: BTreeMap<(NameKey, NameKey, u16), String> = BTreeMap::new();
    let mut merged = Vec::new();

    for (label, mut resources) in inputs {
        let icon_map = assign_ids(&resources, RT_ICON, &mut max_icon_id, &label)?;
        let cursor_map = assign_ids(&resources, RT_CURSOR, &mut max_cursor_id, &label)?;

        for res in &mut resources {
            if res.kind == Name::Id(RT_ICON)
                && let Name::Id(old_id) = res.name
            {
                res.name = Name::Id(icon_map[&old_id]);
            } else if res.kind == Name::Id(RT_CURSOR)
                && let Name::Id(old_id) = res.name
            {
                res.name = Name::Id(cursor_map[&old_id]);
            } else if res.kind == Name::Id(RT_GROUP_ICON) {
                rewrite_group_ids(&mut res.data, &icon_map, "GROUP_ICON", &label)?;
            } else if res.kind == Name::Id(RT_GROUP_CURSOR) {
                rewrite_group_ids(&mut res.data, &cursor_map, "GROUP_CURSOR", &label)?;
            }

            let key = (NameKey::of(&res.kind), NameKey::of(&res.name), res.language);
            if let Some(previous) = seen.get(&key) {
                return Err(format!(
                    "resource {} {} (language {}) is defined by both {} and {}; remove one",
                    format_kind(&res.kind),
                    format_name(&res.name),
                    res.language,
                    previous,
                    label
                ));
            }
            seen.insert(key, label.clone());
        }
        merged.extend(resources);
    }

    Ok(merged)
}

fn assign_ids(
    resources: &[Resource],
    target_kind: u16,
    max_id: &mut u16,
    label: &str,
) -> Result<BTreeMap<u16, u16>, String> {
    let mut map = BTreeMap::new();
    for res in resources {
        if res.kind == Name::Id(target_kind)
            && let Name::Id(old_id) = res.name
            && !map.contains_key(&old_id)
        {
            *max_id = max_id.checked_add(1).ok_or_else(|| {
                format!(
                    "too many {} resources when merging {}",
                    if target_kind == RT_ICON {
                        "ICON"
                    } else {
                        "CURSOR"
                    },
                    label
                )
            })?;
            map.insert(old_id, *max_id);
        }
    }
    Ok(map)
}

fn rewrite_group_ids(
    data: &mut [u8],
    map: &BTreeMap<u16, u16>,
    group_kind: &str,
    label: &str,
) -> Result<(), String> {
    if data.len() < 6 {
        return Err(format!("invalid {group_kind} header in {label}"));
    }
    let count = usize::from(u16::from_le_bytes([data[4], data[5]]));
    let Some(expected_len) = count.checked_mul(14).and_then(|n| n.checked_add(6)) else {
        return Err(format!("invalid {group_kind} entry count in {label}"));
    };
    if data.len() != expected_len {
        return Err(format!("invalid {group_kind} directory size in {label}"));
    }
    for index in 0..count {
        let id_offset = 6 + index * 14 + 12;
        let old_id = u16::from_le_bytes([data[id_offset], data[id_offset + 1]]);
        let Some(&new_id) = map.get(&old_id) else {
            return Err(format!(
                "{group_kind} in {label} references missing image ID {old_id}"
            ));
        };
        data[id_offset..id_offset + 2].copy_from_slice(&new_id.to_le_bytes());
    }
    Ok(())
}

/// Encodes `resources` into a complete 32-bit `.res` binary starting with the 32-byte empty header.
pub fn write(resources: &[Resource]) -> Vec<u8> {
    let mut out = Vec::from(PROLOGUE);
    for res in resources {
        let record_start = out.len();
        let data_size = u32::try_from(res.data.len()).expect("resource data fits in u32");
        out.extend_from_slice(&data_size.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // placeholder for HeaderSize
        write_name(&mut out, &res.kind);
        write_name(&mut out, &res.name);
        while (out.len() - record_start) % 4 != 0 {
            out.push(0);
        }
        out.extend_from_slice(&res.data_version.to_le_bytes());
        out.extend_from_slice(&res.memory_flags.to_le_bytes());
        out.extend_from_slice(&res.language.to_le_bytes());
        out.extend_from_slice(&res.version.to_le_bytes());
        out.extend_from_slice(&res.characteristics.to_le_bytes());
        let header_size =
            u32::try_from(out.len() - record_start).expect("resource header fits in u32");
        out[record_start + 4..record_start + 8].copy_from_slice(&header_size.to_le_bytes());
        out.extend_from_slice(&res.data);
        while (out.len() - record_start) % 4 != 0 {
            out.push(0);
        }
    }
    out
}

fn write_name(out: &mut Vec<u8>, name: &Name) {
    match name {
        Name::Id(id) => {
            out.extend_from_slice(&0xFFFFu16.to_le_bytes());
            out.extend_from_slice(&id.to_le_bytes());
        }
        Name::Text(units) => {
            for &unit in units {
                out.extend_from_slice(&unit.to_le_bytes());
            }
            out.extend_from_slice(&0u16.to_le_bytes());
        }
    }
}

fn pad4(buf: &mut Vec<u8>) {
    while buf.len() & 3 != 0 {
        buf.push(0);
    }
}

fn align4(len: usize) -> Result<usize, String> {
    len.checked_add(3)
        .map(|n| n & !3)
        .ok_or_else(|| "invalid .res file: size alignment overflows".to_owned())
}

fn format_kind(kind: &Name) -> String {
    match kind {
        Name::Id(1) => "CURSOR".to_owned(),
        Name::Id(2) => "BITMAP".to_owned(),
        Name::Id(3) => "ICON".to_owned(),
        Name::Id(4) => "MENU".to_owned(),
        Name::Id(5) => "DIALOG".to_owned(),
        Name::Id(6) => "STRINGTABLE".to_owned(),
        Name::Id(9) => "ACCELERATORS".to_owned(),
        Name::Id(10) => "RCDATA".to_owned(),
        Name::Id(12) => "GROUP_CURSOR".to_owned(),
        Name::Id(14) => "GROUP_ICON".to_owned(),
        Name::Id(16) => "VERSIONINFO".to_owned(),
        Name::Id(24) => "MANIFEST".to_owned(),
        Name::Id(other) => format!("type {other}"),
        Name::Text(units) => format!("'{}'", String::from_utf16_lossy(units)),
    }
}

fn format_name(name: &Name) -> String {
    match name {
        Name::Id(id) => id.to_string(),
        Name::Text(units) => format!("'{}'", String::from_utf16_lossy(units)),
    }
}
