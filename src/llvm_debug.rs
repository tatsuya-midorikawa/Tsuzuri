use super::*;

struct Source {
    text: String,
    starts: Vec<usize>,
    file: usize,
}

pub(super) struct DebugContext {
    sources: Vec<Source>,
    unit: usize,
    empty: usize,
    optimized: bool,
    wasm: bool,
    types: BTreeMap<Type, usize>,
    locations: BTreeMap<(usize, usize, usize), usize>,
}

fn metadata(next: &mut usize, definitions: &mut Vec<String>, text: String) -> usize {
    let id = *next;
    *next += 1;
    definitions.push(format!("!{id} = {text}"));
    id
}

fn quote(text: &str) -> String {
    let mut quoted = String::from("\"");
    for byte in text.bytes() {
        if (0x20..0x7f).contains(&byte) && !matches!(byte, b'"' | b'\\') {
            quoted.push(char::from(byte));
        } else {
            let _ = write!(quoted, "\\{byte:02X}");
        }
    }
    quoted.push('"');
    quoted
}

pub(super) fn wrapper(
    ir: String,
    module: &CheckedModule,
    function: &CheckedFunction,
    symbol: &str,
    globals: &mut Globals,
) -> String {
    let Some(scope) = globals.debug_subprogram(module, function, symbol) else {
        return ir;
    };
    let Some(location) = globals.debug_location(scope, function.span) else {
        return ir;
    };
    let mut output = String::with_capacity(ir.len());
    let mut inside = false;
    for line in ir.lines() {
        if line.starts_with("define ") && line.contains(&format!("{symbol}(")) {
            inside = true;
            let _ = writeln!(
                output,
                "{} !dbg !{scope} {{",
                line.trim_end_matches('{').trim_end()
            );
        } else {
            let instruction = line.trim_start();
            if inside
                && !line.contains("!dbg")
                && (instruction.starts_with("call ")
                    || instruction.contains(" = call ")
                    || instruction.starts_with("ret "))
            {
                let _ = writeln!(output, "{line}, !dbg !{location}");
            } else {
                let _ = writeln!(output, "{line}");
            }
            if instruction == "}" {
                inside = false;
            }
        }
    }
    output
}

impl DebugContext {
    pub(super) fn new(
        sources: &[TrapSource<'_>],
        optimized: bool,
        wasm: bool,
        next: &mut usize,
        definitions: &mut Vec<String>,
    ) -> Self {
        let sources: Vec<_> =
            sources
                .iter()
                .map(|source| {
                    let path = std::path::Path::new(source.path);
                    let filename = path.file_name().map_or(source.path.to_owned(), |name| {
                        name.to_string_lossy().into_owned()
                    });
                    let directory = path
                        .parent()
                        .map_or_else(String::new, |parent| parent.to_string_lossy().into_owned());
                    let file = metadata(
                        next,
                        definitions,
                        format!(
                            "!DIFile(filename: {}, directory: {})",
                            quote(&filename),
                            quote(&directory)
                        ),
                    );
                    Source {
                        text: source.text.to_owned(),
                        starts: std::iter::once(0)
                            .chain(
                                source.text.bytes().enumerate().filter_map(|(index, byte)| {
                                    (byte == b'\n').then_some(index + 1)
                                }),
                            )
                            .collect(),
                        file,
                    }
                })
                .collect();
        let empty = metadata(next, definitions, "!{}".into());
        let unit = metadata(
            next,
            definitions,
            format!(
                "distinct !DICompileUnit(language: DW_LANG_C, file: !{}, producer: {}, isOptimized: {optimized}, runtimeVersion: 0, emissionKind: FullDebug, enums: !{empty})",
                sources[0].file,
                quote(concat!("Tsuzuri ", env!("CARGO_PKG_VERSION")))
            ),
        );
        let dwarf = metadata(
            next,
            definitions,
            "!{i32 2, !\"Dwarf Version\", i32 4}".into(),
        );
        let version = metadata(
            next,
            definitions,
            "!{i32 2, !\"Debug Info Version\", i32 3}".into(),
        );
        definitions.push(format!(
            "!llvm.dbg.cu = !{{!{unit}}}\n!llvm.module.flags = !{{!{dwarf}, !{version}}}"
        ));
        Self {
            sources,
            unit,
            empty,
            optimized,
            wasm,
            types: BTreeMap::new(),
            locations: BTreeMap::new(),
        }
    }

    fn source(&self, span: Span) -> Option<(usize, usize, usize)> {
        let source = self.sources.get(span.source.unwrap_or(0))?;
        if span.start > source.text.len() || !source.text.is_char_boundary(span.start) {
            return None;
        }
        let line = source.starts.partition_point(|start| *start <= span.start) - 1;
        Some((
            source.file,
            line + 1,
            source.text[source.starts[line]..span.start].chars().count() + 1,
        ))
    }

    fn location(
        &mut self,
        scope: usize,
        span: Span,
        next: &mut usize,
        definitions: &mut Vec<String>,
    ) -> Option<usize> {
        let (_, line, column) = self.source(span)?;
        let key = (scope, line, column);
        Some(*self.locations.entry(key).or_insert_with(|| {
            metadata(
                next,
                definitions,
                format!("!DILocation(line: {line}, column: {column}, scope: !{scope})"),
            )
        }))
    }

    fn subprogram(
        &mut self,
        module: &CheckedModule,
        function: &CheckedFunction,
        symbol: &str,
        next: &mut usize,
        definitions: &mut Vec<String>,
    ) -> Option<usize> {
        let (file, line, _) = self.source(function.span)?;
        let mut signature = vec![self.ty(&function.signature.result, module, next, definitions)];
        signature.extend(
            function
                .parameters
                .iter()
                .map(|parameter| self.ty(&parameter.ty, module, next, definitions)),
        );
        let elements = metadata(
            next,
            definitions,
            format!(
                "!{{{}}}",
                signature
                    .iter()
                    .map(|id| format!("!{id}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
        let signature = metadata(
            next,
            definitions,
            format!("!DISubroutineType(types: !{elements})"),
        );
        let name = if (function.is_task || function.name.starts_with("$lambda"))
            && let Some(parent) = function
                .origin
                .parent
                .and_then(|id| module.functions.get(id))
        {
            format!("{}.{}", parent.qualified_name(), function.name)
        } else {
            function.qualified_name()
        };
        Some(metadata(
            next,
            definitions,
            format!(
                "distinct !DISubprogram(name: {}, linkageName: {}, scope: !{file}, file: !{file}, line: {line}, type: !{signature}, scopeLine: {line}, spFlags: DISPFlagDefinition | DISPFlagLocalToUnit{}, unit: !{}, retainedNodes: !{})",
                quote(&name),
                quote(symbol.trim_start_matches('@')),
                if self.optimized {
                    " | DISPFlagOptimized"
                } else {
                    ""
                },
                self.unit,
                self.empty
            ),
        ))
    }

    fn ty(
        &mut self,
        ty: &Type,
        module: &CheckedModule,
        next: &mut usize,
        definitions: &mut Vec<String>,
    ) -> usize {
        if let Some(id) = self.types.get(ty) {
            return *id;
        }
        let id = *next;
        *next += 1;
        self.types.insert(ty.clone(), id);
        let name = quote(&ty.display(&module.types()));
        let (size, alignment) = layout(ty, module, self.wasm);
        let pointer = if self.wasm { 32 } else { 64 };
        let scalar = match ty {
            Type::Integer(_, true) => Some("DW_ATE_signed"),
            Type::Integer(_, false)
            | Type::Decimal(_)
            | Type::Unit
            | Type::Char
            | Type::Utf8Char => Some("DW_ATE_unsigned"),
            Type::Binary(_) => Some("DW_ATE_float"),
            Type::Bool => Some("DW_ATE_boolean"),
            _ => None,
        };
        let definition = if let Some(encoding) = scalar {
            format!(
                "!DIBasicType(name: {name}, size: {}, encoding: {encoding})",
                size * 8
            )
        } else if let Type::Reference(inner, _) = ty
            && ty.shared_array_element().is_none()
        {
            let base = self.ty(inner, module, next, definitions);
            format!(
                "!DIDerivedType(tag: DW_TAG_pointer_type, name: {name}, baseType: !{base}, size: {pointer})"
            )
        } else if matches!(ty, Type::Handle(_))
            || (matches!(ty, Type::Union(..)) && module.types().recursive(ty))
        {
            format!(
                "!DIDerivedType(tag: DW_TAG_pointer_type, name: {name}, baseType: null, size: {pointer})"
            )
        } else {
            let fields = fields(ty, module);
            let mut members = Vec::new();
            let mut offset: usize = 0;
            for (field, field_type) in fields {
                let (field_size, field_alignment) = layout(&field_type, module, self.wasm);
                offset = offset.next_multiple_of(field_alignment);
                let member_type = self.ty(&field_type, module, next, definitions);
                members.push(metadata(next, definitions, format!("!DIDerivedType(tag: DW_TAG_member, name: {}, scope: !{id}, baseType: !{member_type}, size: {}, align: {}, offset: {})", quote(&field), field_size * 8, field_alignment * 8, offset * 8)));
                offset += field_size;
            }
            let elements = metadata(
                next,
                definitions,
                format!(
                    "!{{{}}}",
                    members
                        .iter()
                        .map(|id| format!("!{id}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
            format!(
                "distinct !DICompositeType(tag: DW_TAG_structure_type, name: {name}, size: {}, align: {}, elements: !{elements})",
                size * 8,
                alignment * 8
            )
        };
        definitions.push(format!("!{id} = {definition}"));
        id
    }
}

fn fields(ty: &Type, module: &CheckedModule) -> Vec<(String, Type)> {
    let pointer = || Type::Reference(Box::new(Type::Unit), false);
    let sequence = |element: Type| {
        vec![
            ("data".into(), Type::Reference(Box::new(element), false)),
            ("length".into(), Type::I64),
        ]
    };
    match ty {
        Type::Record(id, arguments) => module.records[*id]
            .fields
            .iter()
            .map(|(name, _)| name.clone())
            .zip(module.types().record_fields(*id, arguments))
            .collect(),
        Type::Tuple(elements) => elements
            .iter()
            .enumerate()
            .map(|(index, ty)| (index.to_string(), ty.clone()))
            .collect(),
        Type::String => sequence(Type::Integer(16, false)),
        Type::Utf8String => sequence(Type::Integer(8, false)),
        Type::Array(element) => sequence((**element).clone()),
        Type::Reference(_, false) if ty.shared_array_element().is_some() => {
            sequence(ty.shared_array_element().unwrap().clone())
        }
        Type::List(_) => vec![("head".into(), pointer()), ("length".into(), Type::I64)],
        Type::Vec(element) => {
            let mut result = sequence((**element).clone());
            result.push(("capacity".into(), Type::I64));
            result
        }
        Type::Function(..) | Type::Task(_) => ["code", "environment", "clone", "drop"]
            .into_iter()
            .map(|name| (name.into(), pointer()))
            .collect(),
        _ => Vec::new(),
    }
}

fn aggregate(
    types: impl IntoIterator<Item = Type>,
    module: &CheckedModule,
    wasm: bool,
) -> (usize, usize) {
    let mut size: usize = 0;
    let mut alignment = 1;
    for ty in types {
        let (field_size, field_alignment) = layout(&ty, module, wasm);
        size = size.next_multiple_of(field_alignment) + field_size;
        alignment = alignment.max(field_alignment);
    }
    (size.next_multiple_of(alignment), alignment)
}

fn layout(ty: &Type, module: &CheckedModule, wasm: bool) -> (usize, usize) {
    let pointer = if wasm { 4 } else { 8 };
    match ty {
        Type::Simd(vector) => {
            if vector.kind == crate::simd::SimdKind::Mask {
                let bytes = usize::from(vector.lanes()).div_ceil(8);
                (bytes, bytes)
            } else {
                (16, 16)
            }
        }
        Type::Integer(bits, _) | Type::Binary(bits) | Type::Decimal(bits) => {
            (usize::from(*bits) / 8, usize::from(*bits) / 8)
        }
        Type::Bool | Type::Unit => (1, 1),
        Type::Char => (2, 2),
        Type::Utf8Char => (4, 4),
        Type::Array(_) | Type::List(_) | Type::String | Type::Utf8String => (16, 8),
        Type::Vec(_) => (24, 8),
        Type::Reference(_, false) if ty.shared_array_element().is_some() => (16, 8),
        Type::Reference(..) | Type::Handle(_) => (pointer, pointer),
        Type::Function(..) | Type::Task(_) => (4 * pointer, pointer),
        Type::Record(id, arguments) => {
            aggregate(module.types().record_fields(*id, arguments), module, wasm)
        }
        Type::Tuple(elements) => aggregate(elements.iter().cloned(), module, wasm),
        Type::Union(..) if module.types().recursive(ty) => (pointer, pointer),
        Type::Union(id, arguments) => match union_layout(*id, arguments, module) {
            UnionLayout::Enum => (4, 4),
            UnionLayout::Common(payload) => {
                aggregate([Type::Integer(32, true), payload], module, wasm)
            }
            UnionLayout::General(count) => (16 + count * 16, 16),
        },
        _ => (0, 1),
    }
}

impl Globals {
    pub(super) fn debug_subprogram(
        &mut self,
        module: &CheckedModule,
        function: &CheckedFunction,
        symbol: &str,
    ) -> Option<usize> {
        self.debug.as_mut()?.subprogram(
            module,
            function,
            symbol,
            &mut self.next_metadata,
            &mut self.definitions,
        )
    }

    pub(super) fn debug_location(&mut self, scope: usize, span: Span) -> Option<usize> {
        self.debug
            .as_mut()?
            .location(scope, span, &mut self.next_metadata, &mut self.definitions)
    }

    pub(super) fn debug_variable(
        &mut self,
        module: &CheckedModule,
        scope: usize,
        local: &Local,
        parameter: Option<usize>,
    ) -> Option<(usize, usize)> {
        let context = self.debug.as_mut()?;
        let (file, line, _) = context.source(local.span)?;
        let ty = context.ty(
            &local.ty,
            module,
            &mut self.next_metadata,
            &mut self.definitions,
        );
        let arg = parameter.map_or_else(String::new, |index| format!(", arg: {index}"));
        let variable = metadata(
            &mut self.next_metadata,
            &mut self.definitions,
            format!(
                "!DILocalVariable(name: {}{arg}, scope: !{scope}, file: !{file}, line: {line}, type: !{ty})",
                quote(&local.name)
            ),
        );
        let location = context.location(
            scope,
            local.span,
            &mut self.next_metadata,
            &mut self.definitions,
        )?;
        Some((variable, location))
    }
}
