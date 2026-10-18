//! DWARF line information for `-g` (WP12).
//!
//! The IR of a `-g` build names the source of an instruction with `!dbg !N`. `!N` is a
//! `!DILocation(line:, column:, scope:)`, whose scope leads to the `!DISubprogram` and the `!DIFile`
//! of the function. Cranelift reports the machine code that each location produced, and this
//! module turns the two into the `.debug_line` and `.debug_info` sections of the object: one
//! compile unit, and for each function a subprogram and a line sequence. Function names, files,
//! lines and columns are covered; types and variables are not, so a debugger can break on a
//! function or a line and show where it stopped, but cannot print the program's variables. The
//! LLVM backend stays the one with full debug information.

use std::collections::HashMap;

use cranelift_module::FuncId;
use cranelift_object::ObjectProduct;
use cranelift_object::object::write::{
    Relocation as ObjectRelocation, SectionId as ObjectSection, StandardSegment, SymbolSection,
};
use cranelift_object::object::{RelocationEncoding, RelocationFlags, RelocationKind, SectionKind};
use gimli::write::{
    Address, AttributeValue, DwarfUnit, EndianVec, FileId, LineProgram, LineString, Range,
    RangeList, RelocateWriter, Relocation, RelocationTarget, Sections,
};
use gimli::{LineEncoding, LittleEndian, SectionId};

use super::error::{Failure, Res, bail};
use super::target::Format;

/// The machine code of one function and where it came from.
pub(crate) struct FunctionLines {
    pub function: FuncId,
    /// The metadata id of the function's `!DISubprogram`.
    pub subprogram: u32,
    /// The size of the code in bytes.
    pub size: u32,
    /// `(offset, metadata id of the !DILocation)` of each run of code that has a location, by
    /// offset.
    pub runs: Vec<(u32, u32)>,
}

/// The sections of one DWARF unit, as bytes and relocations.
pub(crate) struct Dwarf {
    sections: Vec<(SectionId, Section)>,
}

impl Dwarf {
    #[cfg(test)]
    pub(crate) fn section(&self, id: SectionId) -> Option<&[u8]> {
        self.sections
            .iter()
            .find(|(section, _)| *section == id)
            .map(|(_, section)| section.data.slice())
    }
}

/// A section that records the relocations that its writer asks for: a function's address and the
/// offset into another section.
#[derive(Clone)]
struct Section {
    data: EndianVec<LittleEndian>,
    relocations: Vec<Relocation>,
}

impl Section {
    fn new() -> Self {
        Self {
            data: EndianVec::new(LittleEndian),
            relocations: Vec::new(),
        }
    }
}

impl RelocateWriter for Section {
    type Writer = EndianVec<LittleEndian>;

    fn writer(&self) -> &Self::Writer {
        &self.data
    }

    fn writer_mut(&mut self) -> &mut Self::Writer {
        &mut self.data
    }

    fn relocate(&mut self, relocation: Relocation) {
        self.relocations.push(relocation);
    }
}

/// Adds the DWARF of a `-g` module to the object. A module without a compile unit has none.
pub(crate) fn write(
    product: &mut ObjectProduct,
    text: &str,
    functions: &[FunctionLines],
    format: Format,
) -> Res<()> {
    if format == Format::Coff {
        bail!("DWARF debug information in a Windows object");
    }
    let Some(dwarf) = build(text, functions)? else {
        return Ok(());
    };
    let mut ids: HashMap<SectionId, ObjectSection> = HashMap::new();
    let segment = product.object.segment_name(StandardSegment::Debug).to_vec();
    for (id, section) in &dwarf.sections {
        let name = if format == Format::MachO {
            // `.debug_line` is `__debug_line` in the `__DWARF` segment.
            format!("__{}", &id.name()[1..])
        } else {
            id.name().to_owned()
        };
        let object =
            product
                .object
                .add_section(segment.clone(), name.into_bytes(), SectionKind::Debug);
        ids.insert(*id, object);
        // Mach-O has no relocation against a DWARF section: Clang writes the offset itself, and
        // `dsymutil` and the debuggers read it as it is.
        let mut data = section.data.slice().to_vec();
        if format == Format::MachO {
            for relocation in &section.relocations {
                if let RelocationTarget::Section(_) = relocation.target {
                    let size = usize::from(relocation.size);
                    let at = relocation.offset;
                    data[at..at + size]
                        .copy_from_slice(&(relocation.addend as u64).to_le_bytes()[..size]);
                }
            }
        }
        product.object.set_section_data(object, data, 1);
    }
    for (id, section) in &dwarf.sections {
        let object = ids[id];
        for relocation in &section.relocations {
            let (symbol, addend) = match relocation.target {
                RelocationTarget::Symbol(index) => {
                    let function = product.function_symbol(functions[index].function);
                    if format == Format::MachO {
                        // `dsymutil` takes the value stored at a DWARF address to be the address in
                        // the object, as in Clang's section-relative relocations, and moves it by
                        // what the debug map says: a relocation against the function's own symbol
                        // would leave every function at the start of the section.
                        let symbol = product.object.symbol(function);
                        let SymbolSection::Section(section) = symbol.section else {
                            bail!("a function that is not in a section");
                        };
                        let offset = symbol.value as i64;
                        (
                            product.object.section_symbol(section),
                            offset + relocation.addend,
                        )
                    } else {
                        (function, relocation.addend)
                    }
                }
                RelocationTarget::Section(_) if format == Format::MachO => continue,
                RelocationTarget::Section(target) => {
                    let Some(&target) = ids.get(&target) else {
                        bail!("a DWARF offset into the empty section {}", target.name());
                    };
                    (product.object.section_symbol(target), relocation.addend)
                }
            };
            product
                .object
                .add_relocation(
                    object,
                    ObjectRelocation {
                        offset: relocation.offset as u64,
                        symbol,
                        addend,
                        flags: RelocationFlags::Generic {
                            kind: RelocationKind::Absolute,
                            encoding: RelocationEncoding::Generic,
                            size: relocation.size * 8,
                        },
                    },
                )
                .map_err(|error| Failure::new(format!("the object writer: {error}")))?;
        }
    }
    Ok(())
}

/// The line table and the compile unit of the functions. `Address::Symbol { symbol }` is an index
/// into `functions`.
pub(crate) fn build(text: &str, functions: &[FunctionLines]) -> Res<Option<Dwarf>> {
    let metadata = Metadata::index(text);
    let Some(unit) = metadata.unit() else {
        return Ok(None);
    };
    let encoding = gimli::Encoding {
        format: gimli::Format::Dwarf32,
        version: 4,
        address_size: 8,
    };
    // The directories of the files are relative to this one, as in the LLVM backend's output.
    let directory = if unit.file.directory.is_empty() {
        b".".to_vec()
    } else {
        unit.file.directory.clone()
    };
    let mut dwarf = DwarfUnit::new(encoding);
    dwarf.unit.line_program = LineProgram::new(
        encoding,
        LineEncoding::default(),
        LineString::String(directory.clone()),
        LineString::String(unit.file.name.clone()),
        None,
    );
    let root = dwarf.unit.root();
    let die = dwarf.unit.get_mut(root);
    die.set(
        gimli::DW_AT_producer,
        AttributeValue::String(unit.producer.clone()),
    );
    die.set(
        gimli::DW_AT_language,
        AttributeValue::Language(gimli::DW_LANG_C),
    );
    die.set(
        gimli::DW_AT_name,
        AttributeValue::String(unit.file.name.clone()),
    );
    die.set(gimli::DW_AT_stmt_list, AttributeValue::LineProgramRef);
    die.set(gimli::DW_AT_comp_dir, AttributeValue::String(directory));
    die.set(
        gimli::DW_AT_low_pc,
        AttributeValue::Address(Address::Constant(0)),
    );

    let mut files = Files::default();
    let mut places: HashMap<u32, Option<Place>> = HashMap::new();
    let mut ranges = Vec::new();
    for (index, function) in functions.iter().enumerate() {
        let Some(subprogram) = metadata.subprogram(function.subprogram) else {
            bail!(
                "the debug information !{} that is not a subprogram",
                function.subprogram
            );
        };
        let start = Address::Symbol {
            symbol: index,
            addend: 0,
        };
        let Some(declared) = files.get(&mut dwarf.unit.line_program, &metadata, subprogram.file)
        else {
            bail!(
                "the source file !{} of the subprogram !{}",
                subprogram.file,
                function.subprogram
            );
        };
        let program = &mut dwarf.unit.line_program;
        program.begin_sequence(Some(start));
        // Like the LLVM backend's, the prologue is line 0: the code that sets up the frame has no
        // place in the source.
        if function.runs.first().is_none_or(|run| run.0 != 0) {
            let row = program.row();
            row.address_offset = 0;
            row.file = declared;
            row.line = 0;
            row.column = 0;
            row.is_statement = true;
            program.generate_row();
        }
        let mut last: Option<(u32, u32, u32)> = None;
        let mut prologue_ended = false;
        for &(offset, location) in &function.runs {
            let place = *places
                .entry(location)
                .or_insert_with(|| metadata.place(location));
            let Some(place) = place else {
                bail!("the debug location !{location} that is not a line and a scope");
            };
            let key = (place.file, place.line, place.column);
            if last == Some(key) {
                continue;
            }
            let Some(file) = files.get(program, &metadata, place.file) else {
                bail!(
                    "the source file !{} of the location !{location}",
                    place.file
                );
            };
            let statement = last.is_none_or(|(_, line, _)| line != place.line);
            let row = program.row();
            row.address_offset = u64::from(offset);
            row.file = file;
            row.line = u64::from(place.line);
            row.column = u64::from(place.column);
            row.is_statement = statement;
            // The first line of the body is where a breakpoint on the function stops.
            row.prologue_end = !prologue_ended && place.line != 0;
            prologue_ended |= place.line != 0;
            program.generate_row();
            last = Some(key);
        }
        program.end_sequence(u64::from(function.size));

        let entry = dwarf.unit.add(root, gimli::DW_TAG_subprogram);
        let entry = dwarf.unit.get_mut(entry);
        entry.set(gimli::DW_AT_name, AttributeValue::String(subprogram.name));
        entry.set(
            gimli::DW_AT_decl_file,
            AttributeValue::FileIndex(Some(declared)),
        );
        entry.set(
            gimli::DW_AT_decl_line,
            AttributeValue::Udata(u64::from(subprogram.line)),
        );
        entry.set(gimli::DW_AT_low_pc, AttributeValue::Address(start));
        entry.set(
            gimli::DW_AT_high_pc,
            AttributeValue::Udata(u64::from(function.size)),
        );
        if subprogram.external {
            entry.set(gimli::DW_AT_external, AttributeValue::Flag(true));
        }
        if function.size != 0 {
            ranges.push(Range::StartLength {
                begin: start,
                length: u64::from(function.size),
            });
        }
    }
    if !ranges.is_empty() {
        let list = dwarf.unit.ranges.add(RangeList(ranges));
        dwarf
            .unit
            .get_mut(root)
            .set(gimli::DW_AT_ranges, AttributeValue::RangeListRef(list));
    }

    let mut sections = Sections::new(Section::new());
    dwarf
        .write(&mut sections)
        .map_err(|error| Failure::new(format!("the DWARF writer: {error}")))?;
    let mut out = Vec::new();
    let _ = sections.for_each(|id, section| -> Result<(), ()> {
        if !section.data.slice().is_empty() {
            out.push((id, section.clone()));
        }
        Ok(())
    });
    Ok(Some(Dwarf { sections: out }))
}

/// The files of the line table, by the metadata id of their `!DIFile`.
#[derive(Default)]
struct Files {
    ids: HashMap<u32, Option<FileId>>,
}

impl Files {
    fn get(
        &mut self,
        program: &mut LineProgram,
        metadata: &Metadata<'_>,
        node: u32,
    ) -> Option<FileId> {
        *self.ids.entry(node).or_insert_with(|| {
            let file = metadata.file(node)?;
            let directory = if file.directory.is_empty() {
                program.default_directory()
            } else {
                program.add_directory(LineString::String(file.directory))
            };
            Some(program.add_file(LineString::String(file.name), directory, None))
        })
    }
}

/// A source position: the `!DIFile` node, line and column.
#[derive(Clone, Copy)]
struct Place {
    file: u32,
    line: u32,
    column: u32,
}

struct SourceFile {
    name: Vec<u8>,
    directory: Vec<u8>,
}

struct Unit {
    file: SourceFile,
    producer: Vec<u8>,
}

struct Subprogram {
    name: Vec<u8>,
    file: u32,
    line: u32,
    external: bool,
}

/// The `!N = ...` nodes of a module, found in one pass and read when something asks for them.
struct Metadata<'a> {
    text: &'a str,
    /// The offset of the text after `!N = `, by `N`; `u32::MAX` where the module has no such node.
    nodes: Vec<u32>,
    unit: Option<u32>,
}

impl<'a> Metadata<'a> {
    fn index(text: &'a str) -> Self {
        let mut nodes: Vec<u32> = Vec::new();
        let mut unit = None;
        for (at, _) in text.match_indices("\n!") {
            let start = at + 2;
            let rest = &text[start..];
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            if digits > 0 && rest[digits..].starts_with(" = ") {
                let Ok(id) = rest[..digits].parse::<usize>() else {
                    continue;
                };
                // A node is at least four bytes: a huge number is not a node of this module.
                if id > text.len() / 4 {
                    continue;
                }
                if id >= nodes.len() {
                    nodes.resize(id + 1, u32::MAX);
                }
                nodes[id] = (start + digits + 3) as u32;
            } else if let Some(list) = rest.strip_prefix("llvm.dbg.cu = !{!") {
                let digits = list.bytes().take_while(u8::is_ascii_digit).count();
                unit = list[..digits].parse().ok();
            }
        }
        Self { text, nodes, unit }
    }

    /// The kind and the fields of node `id`: `DILocation` and `line: 1, column: 2, scope: !3`.
    fn node(&self, id: u32) -> Option<(&'a str, &'a str)> {
        let at = *self.nodes.get(id as usize)?;
        if at == u32::MAX {
            return None;
        }
        let rest = &self.text[at as usize..];
        let line = rest.find('\n').map_or(rest, |end| &rest[..end]);
        split_node(line)
    }

    fn unit(&self) -> Option<Unit> {
        let (kind, fields) = self.node(self.unit?)?;
        if kind != "DICompileUnit" {
            return None;
        }
        let file = self.file(reference(field(fields, "file")?)?)?;
        let producer = field(fields, "producer")
            .and_then(string)
            .unwrap_or_else(|| b"Tsuzuri".to_vec());
        Some(Unit { file, producer })
    }

    fn file(&self, id: u32) -> Option<SourceFile> {
        let (kind, fields) = self.node(id)?;
        if kind != "DIFile" {
            return None;
        }
        let name = string(field(fields, "filename")?)?;
        if name.is_empty() {
            return None;
        }
        let directory = field(fields, "directory")
            .and_then(string)
            .unwrap_or_default();
        Some(SourceFile { name, directory })
    }

    fn subprogram(&self, id: u32) -> Option<Subprogram> {
        let (kind, fields) = self.node(id)?;
        if kind != "DISubprogram" {
            return None;
        }
        let name = string(field(fields, "name")?)?;
        let file = reference(field(fields, "file")?)?;
        let line = field(fields, "line").and_then(number).unwrap_or(0);
        let external =
            !field(fields, "spFlags").is_some_and(|flags| flags.contains("DISPFlagLocalToUnit"));
        Some(Subprogram {
            name,
            file,
            line,
            external,
        })
    }

    /// The position of a `!DILocation`, with the file of its scope.
    fn place(&self, id: u32) -> Option<Place> {
        let (kind, fields) = self.node(id)?;
        if kind != "DILocation" {
            return None;
        }
        let line = number(field(fields, "line")?)?;
        let column = field(fields, "column").and_then(number).unwrap_or(0);
        let mut scope = reference(field(fields, "scope")?)?;
        // A subprogram and a lexical block name their file; a block's scope leads to one that does.
        for _ in 0..16 {
            let (_, fields) = self.node(scope)?;
            if let Some(file) = field(fields, "file").and_then(reference) {
                return Some(Place { file, line, column });
            }
            scope = reference(field(fields, "scope")?)?;
        }
        None
    }
}

/// `[distinct ]!DIKind(fields)` as the kind and the text between the parentheses.
fn split_node(line: &str) -> Option<(&str, &str)> {
    let line = line.trim_end();
    let line = line.strip_prefix("distinct ").unwrap_or(line);
    let line = line.strip_prefix('!')?;
    let open = line.find('(')?;
    let fields = line[open + 1..].strip_suffix(')')?;
    Some((&line[..open], fields))
}

/// The value of `key: value` among the comma-separated fields, as spelled.
fn field<'a>(fields: &'a str, key: &str) -> Option<&'a str> {
    let mut rest = fields;
    loop {
        let colon = rest.find(':')?;
        let name = rest[..colon].trim();
        let value = rest[colon + 1..].trim_start();
        let end = value_end(value);
        if name == key {
            return Some(value[..end].trim_end());
        }
        rest = value.get(end + 1..)?;
    }
}

/// The length of the value at the start of `text`: up to the first `,` outside of a string and of
/// brackets.
fn value_end(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut depth = 0u32;
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'"' => {
                at += 1;
                while at < bytes.len() && bytes[at] != b'"' {
                    if bytes[at] == b'\\' {
                        at += 1;
                    }
                    at += 1;
                }
            }
            b'(' | b'{' | b'[' => depth += 1,
            b')' | b'}' | b']' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => return at,
            _ => {}
        }
        at += 1;
    }
    bytes.len()
}

fn number(value: &str) -> Option<u32> {
    value.parse().ok()
}

/// The id of a `!N` reference.
fn reference(value: &str) -> Option<u32> {
    value.strip_prefix('!')?.parse().ok()
}

/// The bytes of a quoted string, where `\XX` is a byte in hexadecimal and `\\` a backslash.
fn string(value: &str) -> Option<Vec<u8>> {
    let inner = value.strip_prefix('"')?.strip_suffix('"')?;
    let bytes = inner.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'\\' {
            if bytes.get(at + 1) == Some(&b'\\') {
                out.push(b'\\');
                at += 2;
                continue;
            }
            if let Some(byte) = inner
                .get(at + 1..at + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
            {
                out.push(byte);
                at += 3;
                continue;
            }
        }
        out.push(bytes[at]);
        at += 1;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODULE: &str = r#"
define i64 @f() !dbg !5 {
  ret i64 0, !dbg !6
}
!0 = !DIFile(filename: "Main.tz", directory: "proj")
!1 = !DIFile(filename: "Str\2C.tz", directory: "")
!2 = distinct !DICompileUnit(language: DW_LANG_C, file: !0, producer: "Tsuzuri 0.1.0", isOptimized: true)
!llvm.dbg.cu = !{!2}
!5 = distinct !DISubprogram(name: "Main.f, (g)", scope: !0, file: !0, line: 3, type: !4, spFlags: DISPFlagDefinition | DISPFlagLocalToUnit, unit: !2)
!6 = !DILocation(line: 4, column: 7, scope: !5)
!7 = !DILexicalBlock(scope: !5, file: !1, line: 9)
!8 = !DILocation(line: 9, scope: !7)
"#;

    #[test]
    fn reads_the_nodes_of_a_module() {
        let metadata = Metadata::index(MODULE);
        let unit = metadata.unit().expect("a compile unit");
        assert_eq!(unit.file.name, b"Main.tz");
        assert_eq!(unit.file.directory, b"proj");
        assert_eq!(unit.producer, b"Tsuzuri 0.1.0");
        let subprogram = metadata.subprogram(5).expect("a subprogram");
        assert_eq!(subprogram.name, b"Main.f, (g)");
        assert_eq!((subprogram.file, subprogram.line), (0, 3));
        assert!(!subprogram.external);
        let place = metadata.place(6).expect("a location");
        assert_eq!((place.file, place.line, place.column), (0, 4, 7));
        // No column is column 0, and a lexical block names its own file.
        let place = metadata.place(8).expect("a location in a block");
        assert_eq!((place.file, place.line, place.column), (1, 9, 0));
        assert_eq!(metadata.file(1).unwrap().name, b"Str,.tz");
        assert!(metadata.node(3).is_none());
        assert!(metadata.node(99).is_none());
    }

    #[test]
    fn splits_fields_outside_strings_and_brackets() {
        let fields = r#"name: "a, b: \"c\"", flags: DIFlagA | DIFlagB, tag: !{!1, !2}, line: 7"#;
        assert_eq!(field(fields, "name"), Some(r#""a, b: \"c\"""#));
        assert_eq!(field(fields, "flags"), Some("DIFlagA | DIFlagB"));
        assert_eq!(field(fields, "tag"), Some("!{!1, !2}"));
        assert_eq!(field(fields, "line"), Some("7"));
        assert_eq!(field(fields, "scope"), None);
        assert_eq!(
            string(r#""\5C\22x\5C\5C\C3\A9""#).unwrap(),
            b"\\\"x\\\\\xC3\xA9"
        );
    }

    #[test]
    fn a_module_without_a_compile_unit_has_no_dwarf() {
        assert!(
            build("define void @f() {\n  ret void\n}\n", &[])
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn writes_a_line_table_and_a_subprogram_for_each_function() {
        use gimli::read::{Dwarf as Read, EndianSlice, Unit as ReadUnit};
        let function = FunctionLines {
            function: FuncId::from_u32(0),
            subprogram: 5,
            size: 40,
            runs: vec![(8, 6), (16, 6), (20, 8), (32, 6)],
        };
        let dwarf = build(MODULE, &[function]).unwrap().expect("a unit");
        let load = |id: SectionId| -> EndianSlice<'_, LittleEndian> {
            EndianSlice::new(dwarf.section(id).unwrap_or(&[]), LittleEndian)
        };
        let read = Read::load(|id| Ok::<_, gimli::Error>(load(id))).unwrap();
        let mut units = read.units();
        let header = units.next().unwrap().expect("a unit");
        assert_eq!(header.version(), 4);
        let unit: ReadUnit<_> = read.unit(header).unwrap();
        let mut entries = unit.entries();
        let mut subprograms = Vec::new();
        while let Some((_, entry)) = entries.next_dfs().unwrap() {
            if entry.tag() == gimli::DW_TAG_subprogram {
                let name = entry.attr_value(gimli::DW_AT_name).unwrap().unwrap();
                subprograms.push(read.attr_string(&unit, name).unwrap().slice().to_vec());
            }
        }
        assert_eq!(subprograms, [b"Main.f, (g)".to_vec()]);
        let program = unit.line_program.clone().expect("a line program");
        let mut rows = program.rows();
        let mut seen = Vec::new();
        while let Some((_, row)) = rows.next_row().unwrap() {
            seen.push((
                row.address(),
                row.line().map_or(0, |line| line.get()),
                row.column(),
                row.is_stmt(),
                row.end_sequence(),
            ));
        }
        // The addresses are relocations against the function symbol: offsets from its start.
        assert_eq!(
            seen,
            [
                (0, 0, gimli::ColumnType::LeftEdge, true, false),
                (
                    8,
                    4,
                    gimli::ColumnType::Column(std::num::NonZeroU64::new(7).unwrap()),
                    true,
                    false
                ),
                (20, 9, gimli::ColumnType::LeftEdge, true, false),
                (
                    32,
                    4,
                    gimli::ColumnType::Column(std::num::NonZeroU64::new(7).unwrap()),
                    true,
                    false
                ),
                (
                    40,
                    4,
                    gimli::ColumnType::Column(std::num::NonZeroU64::new(7).unwrap()),
                    true,
                    true
                ),
            ]
        );
    }
}
