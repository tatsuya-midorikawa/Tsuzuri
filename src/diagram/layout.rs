//! `layout` (G30, D7): the native layout of a record or union as tables, with the offsets of the
//! fields while their sizes are known.

use super::input::LayoutRow;

/// The note under every layout: the numbers are those of a 64-bit native target.
pub const WASM_NOTE: &str = "Native only. On wasm32 a pointer is 4 bytes and the `null_pointer` niche does not exist, so these numbers are not valid there for any type that holds a pointer.";

fn primitive(ty: &str) -> Option<(u64, u64)> {
    match ty {
        "i8" | "u8" | "bool" => Some((1, 1)),
        "i16" | "u16" => Some((2, 2)),
        "i32" | "u32" | "f32" => Some((4, 4)),
        "i64" | "u64" | "f64" => Some((8, 8)),
        _ => None,
    }
}

/// A type in a table cell: inline code, so that Markdown does not read `<i64>` of `Vec<i64>` as
/// HTML, with the `|` of `[|i64|]` escaped for the table.
fn cell_type(ty: &str) -> String {
    format!("`{}`", ty.replace('|', "\\|"))
}

fn bound((low, high): (u64, u64)) -> String {
    if low == high {
        low.to_string()
    } else {
        format!("{low}..{high}")
    }
}

/// The Markdown of one layout row.
pub fn markdown(row: &LayoutRow) -> String {
    let flag = |value: bool| if value { "yes" } else { "no" };
    let mut lines = vec![
        format!("# Layout of {} (native, 64-bit)", row.id),
        String::new(),
        "| item | value |".to_owned(),
        "| --- | --- |".to_owned(),
        format!("| kind | {} |", row.kind),
        format!("| size (bytes) | {} |", bound(row.size)),
        format!("| align (bytes) | {} |", bound(row.align)),
    ];
    let record = row.kind == "record";
    if record {
        lines.push(format!("| reordered | {} |", flag(row.reordered)));
    } else {
        lines.push(format!(
            "| layout | {} |",
            row.layout.as_deref().unwrap_or("-")
        ));
        lines.push(format!(
            "| tag bytes | {} |",
            row.tag_bytes
                .map_or_else(|| "-".to_owned(), |bytes| bytes.to_string())
        ));
        lines.push(format!(
            "| niche | {} |",
            row.niche.as_deref().unwrap_or("none")
        ));
    }
    lines.push(format!("| drop flag | {} |", flag(row.drop_flag)));
    lines.push(String::new());
    if record {
        lines.push("| slot | field | type | size | align | offset |".to_owned());
        lines.push("| --- | --- | --- | --- | --- | --- |".to_owned());
        let mut fields: Vec<_> = row.fields.iter().collect();
        fields.sort_by_key(|field| field.slot);
        let (mut offset, mut known) = (0u64, true);
        for field in fields {
            match primitive(&field.ty).filter(|_| known) {
                Some((size, align)) => {
                    let start = offset.div_ceil(align) * align;
                    lines.push(format!(
                        "| {} | {} | {} | {size} | {align} | {start} |",
                        field.slot,
                        field.name,
                        cell_type(&field.ty)
                    ));
                    offset = start + size;
                }
                None => {
                    known = false;
                    lines.push(format!(
                        "| {} | {} | {} | ? | ? | ? |",
                        field.slot,
                        field.name,
                        cell_type(&field.ty)
                    ));
                }
            }
        }
        // A Drop record's live flag (`i8 1`, `llvm_layout::drop_flag`) follows its fields.
        if known && row.drop_flag {
            lines.push(format!("| - | (live flag) | `i8` | 1 | 1 | {offset} |"));
            offset += 1;
        }
        if known && row.size.0 == row.size.1 {
            lines.push(format!(
                "| - | (tail padding) | - | {} | - | {offset} |",
                row.size.0.saturating_sub(offset)
            ));
        }
    } else {
        lines.push("| case | payload |".to_owned());
        lines.push("| --- | --- |".to_owned());
        for case in &row.cases {
            lines.push(format!(
                "| {} | {} |",
                case.name,
                case.payload
                    .as_deref()
                    .map_or_else(|| "-".to_owned(), cell_type)
            ));
        }
    }
    lines.push(String::new());
    lines.push(WASM_NOTE.to_owned());
    lines.push(String::new());
    lines.join("\n")
}
