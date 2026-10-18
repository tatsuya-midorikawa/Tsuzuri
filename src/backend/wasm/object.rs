//! A relocatable WebAssembly object, as `wasm-ld` reads it (the "linking" conventions of the
//! WebAssembly tool-conventions): the sections of a module whose indices and addresses are
//! relocations, a symbol table, and the segments of the data.
//!
//! The object imports the linear memory, the stack pointer and the indirect function table from
//! `env`, like the objects of Clang do, so that the link decides where they are.

use std::collections::HashMap;

use super::bin::{Vt, name, section, sleb, uleb, uleb_padded};

/// What a relocation computes. The values are the relocation types of the conventions.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    /// The index of a function (a call): a symbol.
    FunctionIndex = 0,
    /// The table slot of a function (its address as a value in code): a symbol.
    TableIndexSleb = 1,
    /// The table slot of a function in data: a symbol.
    TableIndexI32 = 2,
    /// The address of data (as an offset or a constant): a symbol and an addend.
    MemoryAddrSleb = 4,
    MemoryAddrI32 = 5,
    /// The index of a type: an index of the types.
    TypeIndex = 6,
    /// The index of a global: a symbol.
    GlobalIndex = 7,
    /// The index of a table: a symbol.
    TableNumber = 20,
}

impl Kind {
    fn has_addend(self) -> bool {
        matches!(self, Kind::MemoryAddrSleb | Kind::MemoryAddrI32)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Reloc {
    /// From the start of the body or the segment that holds the field.
    pub offset: u32,
    pub kind: Kind,
    /// A symbol of the table, or for `TypeIndex` the index of a type of the body.
    pub index: u32,
    pub addend: i32,
}

/// One function: its locals and code, and the relocations of the code.
pub(crate) struct Body {
    pub code: Vec<u8>,
    pub relocs: Vec<Reloc>,
    /// The types that `call_indirect` instructions of the code name: `TypeIndex` relocations index here.
    pub types: Vec<(Vec<Vt>, Vec<Vt>)>,
}

pub(crate) struct Segment {
    pub name: String,
    /// The base 2 logarithm of the alignment.
    pub align: u8,
    pub data: Vec<u8>,
    pub relocs: Vec<Reloc>,
}

pub(crate) const BINDING_WEAK: u32 = 0x01;
pub(crate) const BINDING_LOCAL: u32 = 0x02;
pub(crate) const VISIBILITY_HIDDEN: u32 = 0x04;
pub(crate) const UNDEFINED: u32 = 0x10;
pub(crate) const EXPLICIT_NAME: u32 = 0x40;
pub(crate) const NO_STRIP: u32 = 0x80;

pub(crate) enum Symbol {
    Function {
        name: String,
        flags: u32,
        /// The index in the function space of the object: imports first.
        index: u32,
    },
    Data {
        name: String,
        flags: u32,
        /// The segment, and the size of the object in it, which starts at the segment's start.
        segment: u32,
        size: u32,
    },
    Global {
        flags: u32,
        index: u32,
    },
    Table {
        flags: u32,
        index: u32,
    },
}

pub(crate) struct Import {
    pub module: String,
    pub field: String,
    pub signature: (Vec<Vt>, Vec<Vt>),
}

#[derive(Default)]
pub(crate) struct Object {
    pub imports: Vec<Import>,
    /// The signature of each defined function, and its body.
    pub functions: Vec<((Vec<Vt>, Vec<Vt>), Body)>,
    pub segments: Vec<Segment>,
    pub symbols: Vec<Symbol>,
    /// The functions (by index) whose address is taken: they get the slots 1, 2, ... of the table.
    pub address_taken: Vec<u32>,
    pub uses_table: bool,
    pub uses_stack_pointer: bool,
}

type Signature = (Vec<Vt>, Vec<Vt>);

#[derive(Default)]
struct Types {
    list: Vec<Signature>,
    map: HashMap<Signature, u32>,
}

impl Types {
    fn index(&mut self, signature: &Signature) -> u32 {
        if let Some(&index) = self.map.get(signature) {
            return index;
        }
        let index = self.list.len() as u32;
        self.list.push(signature.clone());
        self.map.insert(signature.clone(), index);
        index
    }
}

fn write_relocs(payload: &mut Vec<u8>, section_index: u32, relocs: &[(u32, Reloc)]) {
    uleb(payload, u64::from(section_index));
    uleb(payload, relocs.len() as u64);
    for (offset, reloc) in relocs {
        payload.push(reloc.kind as u8);
        uleb(payload, u64::from(*offset));
        uleb(payload, u64::from(reloc.index));
        if reloc.kind.has_addend() {
            sleb(payload, i64::from(reloc.addend));
        }
    }
}

/// The features that the object may use; the same set as the generic CPU of a recent Clang, which
/// the runtime objects of the build declare too.
const FEATURES: [&str; 8] = [
    "bulk-memory",
    "bulk-memory-opt",
    "call-indirect-overlong",
    "multivalue",
    "mutable-globals",
    "nontrapping-fptoint",
    "reference-types",
    "sign-ext",
];

impl Object {
    pub(crate) fn finish(self) -> Vec<u8> {
        let mut types = Types::default();
        let import_types: Vec<u32> = self
            .imports
            .iter()
            .map(|import| types.index(&import.signature))
            .collect();
        let defined_types: Vec<u32> = self
            .functions
            .iter()
            .map(|(signature, _)| types.index(signature))
            .collect();
        // The `call_indirect` types of the bodies, with the index each body's relocations need.
        let mut body_types: Vec<Vec<u32>> = Vec::with_capacity(self.functions.len());
        for (_, body) in &self.functions {
            body_types.push(
                body.types
                    .iter()
                    .map(|signature| types.index(signature))
                    .collect(),
            );
        }

        let mut out = Vec::new();
        out.extend_from_slice(b"\0asm\x01\0\0\0");
        let mut section_count = 0u32;
        fn push(out: &mut Vec<u8>, count: &mut u32, id: u8, payload: &[u8]) {
            section(out, id, payload);
            *count += 1;
        }

        let mut payload = Vec::new();
        uleb(&mut payload, types.list.len() as u64);
        for (params, results) in &types.list {
            payload.push(0x60);
            uleb(&mut payload, params.len() as u64);
            payload.extend(params.iter().map(|vt| vt.byte()));
            uleb(&mut payload, results.len() as u64);
            payload.extend(results.iter().map(|vt| vt.byte()));
        }
        push(&mut out, &mut section_count, 1, &payload);

        // Imports: the memory, the stack pointer and the table, then the functions.
        let mut payload = Vec::new();
        let fixed = 1 + usize::from(self.uses_stack_pointer) + usize::from(self.uses_table);
        uleb(&mut payload, (fixed + self.imports.len()) as u64);
        name(&mut payload, "env");
        name(&mut payload, "__linear_memory");
        payload.extend_from_slice(&[0x02, 0x00, 0x01]);
        if self.uses_stack_pointer {
            name(&mut payload, "env");
            name(&mut payload, "__stack_pointer");
            payload.extend_from_slice(&[0x03, 0x7f, 0x01]);
        }
        if self.uses_table {
            name(&mut payload, "env");
            name(&mut payload, "__indirect_function_table");
            payload.extend_from_slice(&[0x01, 0x70, 0x00, 0x01]);
        }
        for (import, &type_index) in self.imports.iter().zip(&import_types) {
            name(&mut payload, &import.module);
            name(&mut payload, &import.field);
            payload.push(0x00);
            uleb(&mut payload, u64::from(type_index));
        }
        push(&mut out, &mut section_count, 2, &payload);

        let mut payload = Vec::new();
        uleb(&mut payload, defined_types.len() as u64);
        for &type_index in &defined_types {
            uleb(&mut payload, u64::from(type_index));
        }
        push(&mut out, &mut section_count, 3, &payload);

        if !self.address_taken.is_empty() {
            let mut payload = Vec::new();
            uleb(&mut payload, 1);
            // An active segment of table 0, at slot 1.
            payload.extend_from_slice(&[0x00, 0x41, 0x01, 0x0b]);
            uleb(&mut payload, self.address_taken.len() as u64);
            for &function in &self.address_taken {
                uleb(&mut payload, u64::from(function));
            }
            push(&mut out, &mut section_count, 9, &payload);
        }
        if !self.segments.is_empty() {
            let mut payload = Vec::new();
            uleb(&mut payload, self.segments.len() as u64);
            push(&mut out, &mut section_count, 12, &payload);
        }

        // The code, with the relocations at their place in the section.
        let code_section = section_count;
        let mut payload = Vec::new();
        uleb(&mut payload, self.functions.len() as u64);
        let mut code_relocs: Vec<(u32, Reloc)> = Vec::new();
        for ((_, body), local_types) in self.functions.iter().zip(&body_types) {
            let mut code = body.code.clone();
            let mut size = Vec::new();
            uleb(&mut size, code.len() as u64);
            let start = payload.len() + size.len();
            for reloc in &body.relocs {
                let mut reloc = *reloc;
                if reloc.kind == Kind::TypeIndex {
                    reloc.index = local_types[reloc.index as usize];
                    // The placeholder holds the index too.
                    let mut padded = Vec::new();
                    uleb_padded(&mut padded, reloc.index);
                    code[reloc.offset as usize..reloc.offset as usize + 5].copy_from_slice(&padded);
                }
                code_relocs.push((start as u32 + reloc.offset, reloc));
            }
            payload.extend_from_slice(&size);
            payload.extend_from_slice(&code);
        }
        push(&mut out, &mut section_count, 10, &payload);

        // The data: every segment at an offset of the memory image of the object.
        let data_section = section_count;
        let mut payload = Vec::new();
        let mut data_relocs: Vec<(u32, Reloc)> = Vec::new();
        let mut segment_offsets = Vec::with_capacity(self.segments.len());
        if !self.segments.is_empty() {
            uleb(&mut payload, self.segments.len() as u64);
            let mut image = 0u64;
            for segment in &self.segments {
                let align = 1u64 << segment.align;
                image = image.div_ceil(align) * align;
                segment_offsets.push(image);
                payload.extend_from_slice(&[0x00, 0x41]);
                sleb(&mut payload, image as i64);
                payload.push(0x0b);
                uleb(&mut payload, segment.data.len() as u64);
                let start = payload.len() as u32;
                payload.extend_from_slice(&segment.data);
                for reloc in &segment.relocs {
                    data_relocs.push((start + reloc.offset, *reloc));
                }
                image += segment.data.len() as u64;
            }
            push(&mut out, &mut section_count, 11, &payload);
        }

        // The linking section: the symbols and the segments.
        let mut linking = Vec::new();
        uleb(&mut linking, 2);
        let mut table = Vec::new();
        uleb(&mut table, self.symbols.len() as u64);
        for symbol in &self.symbols {
            match symbol {
                Symbol::Function {
                    name: text,
                    flags,
                    index,
                } => {
                    table.push(0x00);
                    uleb(&mut table, u64::from(*flags));
                    uleb(&mut table, u64::from(*index));
                    if flags & UNDEFINED == 0 || flags & EXPLICIT_NAME != 0 {
                        name(&mut table, text);
                    }
                }
                Symbol::Data {
                    name: text,
                    flags,
                    segment,
                    size,
                } => {
                    table.push(0x01);
                    uleb(&mut table, u64::from(*flags));
                    name(&mut table, text);
                    if flags & UNDEFINED == 0 {
                        uleb(&mut table, u64::from(*segment));
                        uleb(&mut table, 0);
                        uleb(&mut table, u64::from(*size));
                    }
                }
                Symbol::Global { flags, index } => {
                    table.push(0x02);
                    uleb(&mut table, u64::from(*flags));
                    uleb(&mut table, u64::from(*index));
                }
                Symbol::Table { flags, index } => {
                    table.push(0x05);
                    uleb(&mut table, u64::from(*flags));
                    uleb(&mut table, u64::from(*index));
                }
            }
        }
        linking.push(8);
        uleb(&mut linking, table.len() as u64);
        linking.extend_from_slice(&table);
        if !self.segments.is_empty() {
            let mut info = Vec::new();
            uleb(&mut info, self.segments.len() as u64);
            for segment in &self.segments {
                name(&mut info, &segment.name);
                uleb(&mut info, u64::from(segment.align));
                uleb(&mut info, 0);
            }
            linking.push(5);
            uleb(&mut linking, info.len() as u64);
            linking.extend_from_slice(&info);
        }
        let mut custom = Vec::new();
        name(&mut custom, "linking");
        custom.extend_from_slice(&linking);
        push(&mut out, &mut section_count, 0, &custom);

        let mut custom = Vec::new();
        name(&mut custom, "reloc.CODE");
        write_relocs(&mut custom, code_section, &code_relocs);
        push(&mut out, &mut section_count, 0, &custom);
        if !self.segments.is_empty() {
            let mut custom = Vec::new();
            name(&mut custom, "reloc.DATA");
            write_relocs(&mut custom, data_section, &data_relocs);
            push(&mut out, &mut section_count, 0, &custom);
        }

        let mut custom = Vec::new();
        name(&mut custom, "target_features");
        uleb(&mut custom, FEATURES.len() as u64);
        for feature in FEATURES {
            custom.push(b'+');
            name(&mut custom, feature);
        }
        push(&mut out, &mut section_count, 0, &custom);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_the_sections_in_the_order_of_the_conventions() {
        let mut object = Object {
            uses_stack_pointer: true,
            ..Object::default()
        };
        object.functions.push((
            (vec![Vt::I32], vec![Vt::I32]),
            Body {
                code: vec![0x00, 0x20, 0x00, 0x0b],
                relocs: Vec::new(),
                types: Vec::new(),
            },
        ));
        object.symbols.push(Symbol::Function {
            name: "id".into(),
            flags: 0,
            index: 0,
        });
        let bytes = object.finish();
        assert_eq!(&bytes[..8], b"\0asm\x01\0\0\0");
        // Section ids in order: type, import, function, code, then the customs.
        let mut ids = Vec::new();
        let mut at = 8;
        while at < bytes.len() {
            ids.push(bytes[at]);
            let mut length = 0usize;
            let mut shift = 0;
            at += 1;
            loop {
                let byte = bytes[at];
                at += 1;
                length |= usize::from(byte & 0x7f) << shift;
                shift += 7;
                if byte & 0x80 == 0 {
                    break;
                }
            }
            at += length;
        }
        assert_eq!(ids, [1, 2, 3, 10, 0, 0, 0]);
    }
}
