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
    /// The base types under the scalar typedefs, which enumerations also use (G16 D4).
    bases: BTreeMap<Type, usize>,
    /// The unnamed untyped pointer.
    opaque: Option<usize>,
    /// The pointer to a function type that a function value's `code` member has.
    code: Option<usize>,
    locations: BTreeMap<(usize, usize, usize), usize>,
}

/// A member of a structure: its name, type, and size and alignment in bytes.
struct Member {
    name: String,
    ty: usize,
    size: usize,
    alignment: usize,
}

/// The DWARF encoding of a scalar type.
fn encoding(ty: &Type) -> Option<&'static str> {
    Some(match ty {
        Type::Integer(_, true) => "DW_ATE_signed",
        Type::Integer(_, false) | Type::Decimal(_) | Type::Unit => "DW_ATE_unsigned",
        Type::Char | Type::Utf8Char => "DW_ATE_UTF",
        Type::Binary(_) => "DW_ATE_float",
        Type::Bool => "DW_ATE_boolean",
        _ => return None,
    })
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
    let Some(scope) = globals.debug_subprogram(module, function, symbol, true) else {
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
            bases: BTreeMap::new(),
            opaque: None,
            code: None,
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

    /// The name that debuggers show for `function` (G16 D3). A monomorphized instance drops its
    /// `.$mono.N`, so a breakpoint by name reaches every instance. A test is `<module>.test@`, a
    /// lambda or task `<enclosing function>.lambda@` or `.task@`, followed by the `line:column`
    /// of the function's span.
    fn subprogram_name(&self, module: &CheckedModule, function: &CheckedFunction) -> String {
        let position = |function: &CheckedFunction| {
            self.source(function.span)
                .map(|(_, line, column)| format!("{line}:{column}"))
        };
        let plain = |function: &CheckedFunction| {
            if function.name.starts_with("$test.")
                && let Some(position) = position(function)
            {
                return format!("{}.test@{position}", function.module);
            }
            let name = function.qualified_name();
            name.split(".$mono.").next().unwrap_or(&name).to_owned()
        };
        let kind = if function.is_task {
            "task"
        } else if function.module == "$lambda" {
            "lambda"
        } else {
            return plain(function);
        };
        match (
            function
                .origin
                .parent
                .and_then(|id| module.functions.get(id)),
            position(function),
        ) {
            (Some(parent), Some(position)) => format!("{}.{kind}@{position}", plain(parent)),
            _ => function.qualified_name(),
        }
    }

    /// The subprogram of `function`'s definition `symbol`. An artificial one, for a wrapper that
    /// the compiler generates around the function, takes the symbol's name (G16 D3). The glue
    /// functions that the compiler generates for builtins, intrinsic methods, case constructors,
    /// and export bridges have none: like the runtime and the `tz.apply.*` adapters, they have no
    /// source of their own, so stepping does not stop in them (G16 D7).
    fn subprogram(
        &mut self,
        module: &CheckedModule,
        function: &CheckedFunction,
        symbol: &str,
        artificial: bool,
        next: &mut usize,
        definitions: &mut Vec<String>,
    ) -> Option<usize> {
        if !artificial
            && matches!(
                function.module.as_str(),
                "$builtin" | "$intrinsic" | "$case" | "$export"
            )
        {
            return None;
        }
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
        let name = if artificial {
            symbol.trim_start_matches('@').to_owned()
        } else {
            self.subprogram_name(module, function)
        };
        // No `scopeLine`: the prologue, which includes the parameters' stores in the `loop`
        // block, is at line 0, which debuggers skip, so a breakpoint on the function and a step
        // into it stop at the body's first line with the parameters bound (G16 D7).
        Some(metadata(
            next,
            definitions,
            format!(
                "distinct !DISubprogram(name: {}, scope: !{file}, file: !{file}, line: {line}, type: !{signature}, {}spFlags: DISPFlagDefinition | DISPFlagLocalToUnit{}, unit: !{}, retainedNodes: !{})",
                quote(&name),
                if artificial {
                    "flags: DIFlagArtificial, "
                } else {
                    ""
                },
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
        let display = ty.display(&module.types());
        let name = quote(&display);
        let (size, alignment) = layout(ty, module, self.wasm);
        let pointer = self.pointer();
        let definition = if let Some(encoding) = encoding(ty) {
            let base = format!(
                "!DIBasicType(name: {name}, size: {}, encoding: {encoding})",
                size * 8
            );
            if *ty == Type::Bool {
                base
            } else {
                // G16 D4: debuggers show a typedef's name, but choose a C name for a base type.
                let base = metadata(next, definitions, base);
                self.bases.insert(ty.clone(), base);
                format!("!DIDerivedType(tag: DW_TAG_typedef, name: {name}, baseType: !{base})")
            }
        } else if let Type::Reference(inner, _) = ty
            && ty.slice_element().is_none()
        {
            let base = self.ty(inner, module, next, definitions);
            let target = self.pointer_to(base, next, definitions);
            // Debuggers ignore the name of a pointer type, but show a typedef's.
            format!("!DIDerivedType(tag: DW_TAG_typedef, name: {name}, baseType: !{target})")
        } else if matches!(ty, Type::Handle(_) | Type::Shared(..)) {
            let target = self.opaque_pointer(next, definitions);
            format!("!DIDerivedType(tag: DW_TAG_typedef, name: {name}, baseType: !{target})")
        } else if let Type::Union(union, arguments) = ty {
            self.union_type((*union, arguments), ty, &display, module, next, definitions)
        } else {
            let members = match ty {
                Type::List(element) => {
                    let node = self.list_node(&display, element, module, next, definitions);
                    let length = self.ty(&Type::I64, module, next, definitions);
                    vec![
                        Member::new("head", node, pointer, pointer),
                        Member::new("length", length, 8, 8),
                    ]
                }
                Type::Function(..) | Type::Task(_) => {
                    let code = self.code_pointer(next, definitions);
                    let opaque = self.opaque_pointer(next, definitions);
                    vec![
                        Member::new("code", code, pointer, pointer),
                        Member::new("environment", opaque, pointer, pointer),
                        Member::new("clone", opaque, pointer, pointer),
                        Member::new("drop", opaque, pointer, pointer),
                    ]
                }
                // A14: the owned data and the vtable of a dyn value.
                Type::Dyn(_) => {
                    let opaque = self.opaque_pointer(next, definitions);
                    vec![
                        Member::new("data", opaque, pointer, pointer),
                        Member::new("vtable", opaque, pointer, pointer),
                    ]
                }
                _ => fields(ty, module)
                    .into_iter()
                    .map(|(field, field_type)| {
                        let (size, alignment) = layout(&field_type, module, self.wasm);
                        let member = self.ty(&field_type, module, next, definitions);
                        Member::new(&field, member, size, alignment)
                    })
                    .collect(),
            };
            let mut offset: usize = 0;
            let placed = members
                .into_iter()
                .map(|member| {
                    offset = offset.next_multiple_of(member.alignment);
                    let placed = (member, offset);
                    offset += placed.0.size;
                    placed
                })
                .collect();
            composite(
                "DW_TAG_structure_type",
                id,
                &name,
                (size, alignment),
                placed,
                next,
                definitions,
            )
        };
        definitions.push(format!("!{id} = {definition}"));
        id
    }

    fn pointer(&self) -> usize {
        if self.wasm { 4 } else { 8 }
    }

    /// An unnamed pointer to type `base`.
    fn pointer_to(
        &mut self,
        base: usize,
        next: &mut usize,
        definitions: &mut Vec<String>,
    ) -> usize {
        metadata(
            next,
            definitions,
            format!(
                "!DIDerivedType(tag: DW_TAG_pointer_type, baseType: !{base}, size: {})",
                self.pointer() * 8
            ),
        )
    }

    /// The unnamed pointer with no pointee type, which debuggers show as `void *`.
    fn opaque_pointer(&mut self, next: &mut usize, definitions: &mut Vec<String>) -> usize {
        if let Some(id) = self.opaque {
            return id;
        }
        let id = metadata(
            next,
            definitions,
            format!(
                "!DIDerivedType(tag: DW_TAG_pointer_type, baseType: null, size: {})",
                self.pointer() * 8
            ),
        );
        self.opaque = Some(id);
        id
    }

    /// The pointer to a function, so debuggers show the function that a function value calls.
    fn code_pointer(&mut self, next: &mut usize, definitions: &mut Vec<String>) -> usize {
        if let Some(id) = self.code {
            return id;
        }
        let function = metadata(
            next,
            definitions,
            "!DISubroutineType(types: !{null})".into(),
        );
        let id = metadata(
            next,
            definitions,
            format!(
                "!DIDerivedType(tag: DW_TAG_pointer_type, baseType: !{function}, size: {})",
                self.pointer() * 8
            ),
        );
        self.code = Some(id);
        id
    }

    /// The pointer to the node `{ ptr next, T value }` of the list `[|T|]` named `list`, as
    /// `FunctionEmitter::list_node_type` stores it.
    fn list_node(
        &mut self,
        list: &str,
        element: &Type,
        module: &CheckedModule,
        next: &mut usize,
        definitions: &mut Vec<String>,
    ) -> usize {
        let node = *next;
        let link = node + 1;
        *next += 2;
        let pointer = self.pointer();
        definitions.push(format!(
            "!{link} = !DIDerivedType(tag: DW_TAG_pointer_type, baseType: !{node}, size: {})",
            pointer * 8
        ));
        let (value_size, value_alignment) = layout(element, module, self.wasm);
        let value = self.ty(element, module, next, definitions);
        let offset = pointer.next_multiple_of(value_alignment);
        let alignment = pointer.max(value_alignment);
        let members = vec![
            (Member::new("next", link, pointer, pointer), 0),
            (
                Member::new("value", value, value_size, value_alignment),
                offset,
            ),
        ];
        let definition = composite(
            "DW_TAG_structure_type",
            node,
            &quote(&format!("{list}.node")),
            ((offset + value_size).next_multiple_of(alignment), alignment),
            members,
            next,
            definitions,
        );
        definitions.push(format!("!{node} = {definition}"));
        link
    }

    /// The definition of union `ty` named `display` (G16 D5). A union whose cases are all
    /// nullary is an enumeration of its `i32` tag. Another is a structure of the tag `$tag` and
    /// the payload `$payload`, a C union with a member for each case with a payload, at the
    /// offsets of `union_layout`. A recursive union is a typedef of a pointer to its node, which
    /// adds the `next`, `drop`, and `clone` pointers of `recursive_header` before them; a null
    /// pointer is its first nullary case.
    fn union_type(
        &mut self,
        (union, arguments): (usize, &[Type]),
        ty: &Type,
        display: &str,
        module: &CheckedModule,
        next: &mut usize,
        definitions: &mut Vec<String>,
    ) -> String {
        let pointer = self.pointer();
        let shape = union_layout(union, arguments, module);
        let recursive = module.types().recursive(ty);
        if matches!(shape, UnionLayout::Enum) && !recursive {
            return self.tag_type(union, display, module, next, definitions);
        }
        let tag = self.tag_type(union, &format!("{display}.$tag"), module, next, definitions);
        let tag = metadata(next, definitions, tag);
        let (storage_size, storage_alignment) = match &shape {
            UnionLayout::Enum => (1, 1),
            UnionLayout::Common(payload) => layout(payload, module, self.wasm),
            UnionLayout::General(count) => (16 * count, 16),
        };
        let payloads: Vec<_> = module.unions[union]
            .cases
            .iter()
            .zip(module.types().union_payloads(union, arguments))
            .filter_map(|((case, _), payload)| Some((case.clone(), payload?)))
            .collect();
        let payload = (!payloads.is_empty()).then(|| {
            let id = *next;
            *next += 1;
            let members = payloads
                .iter()
                .map(|(case, payload)| {
                    let (size, alignment) = layout(payload, module, self.wasm);
                    let member = self.ty(payload, module, next, definitions);
                    (Member::new(case, member, size, alignment), 0)
                })
                .collect();
            let definition = composite(
                "DW_TAG_union_type",
                id,
                &quote(&format!("{display}.$payload")),
                (storage_size, storage_alignment),
                members,
                next,
                definitions,
            );
            definitions.push(format!("!{id} = {definition}"));
            id
        });
        let header = if recursive { 3 * pointer } else { 0 };
        let mut members = Vec::new();
        if recursive {
            let opaque = self.opaque_pointer(next, definitions);
            for (index, field) in ["next", "drop", "clone"].into_iter().enumerate() {
                members.push((
                    Member::new(field, opaque, pointer, pointer),
                    index * pointer,
                ));
            }
        }
        members.push((Member::new("$tag", tag, 4, 4), header));
        let payload_offset = match shape {
            UnionLayout::General(_) if !recursive => 16,
            _ => (header + 4).next_multiple_of(storage_alignment),
        };
        if let Some(payload) = payload {
            members.push((
                Member::new("$payload", payload, storage_size, storage_alignment),
                payload_offset,
            ));
        }
        if !recursive {
            let (size, alignment) = layout(ty, module, self.wasm);
            return composite(
                "DW_TAG_structure_type",
                *self.types.get(ty).unwrap(),
                &quote(display),
                (size, alignment),
                members,
                next,
                definitions,
            );
        }
        // The node `{ ptr, ptr, ptr, i32, payload }` of `emit_program`'s type definitions.
        let node = *next;
        *next += 1;
        let alignment = pointer.max(storage_alignment);
        let definition = composite(
            "DW_TAG_structure_type",
            node,
            &quote(&format!("{display}.node")),
            (
                (payload_offset + storage_size).next_multiple_of(alignment),
                alignment,
            ),
            members,
            next,
            definitions,
        );
        definitions.push(format!("!{node} = {definition}"));
        let pointer = self.pointer_to(node, next, definitions);
        format!(
            "!DIDerivedType(tag: DW_TAG_typedef, name: {}, baseType: !{pointer})",
            quote(display)
        )
    }

    /// The definition of the enumeration named `name` of the `i32` tag of `union`, with an
    /// enumerator per case.
    fn tag_type(
        &mut self,
        union: usize,
        name: &str,
        module: &CheckedModule,
        next: &mut usize,
        definitions: &mut Vec<String>,
    ) -> String {
        self.ty(&Type::I32, module, next, definitions);
        let base = self.bases[&Type::I32];
        let enumerators: Vec<_> = module.unions[union]
            .cases
            .iter()
            .enumerate()
            .map(|(value, (case, _))| {
                metadata(
                    next,
                    definitions,
                    format!("!DIEnumerator(name: {}, value: {value})", quote(case)),
                )
            })
            .collect();
        let elements = list(&enumerators, next, definitions);
        format!(
            "distinct !DICompositeType(tag: DW_TAG_enumeration_type, name: {}, size: 32, align: 32, baseType: !{base}, elements: !{elements})",
            quote(name)
        )
    }
}

impl Member {
    fn new(name: &str, ty: usize, size: usize, alignment: usize) -> Self {
        Self {
            name: name.to_owned(),
            ty,
            size,
            alignment,
        }
    }
}

/// The definition of composite type `id` named `name` (quoted), of `(size, alignment)` in
/// bytes, whose members are at the given byte offsets.
fn composite(
    tag: &str,
    id: usize,
    name: &str,
    (size, alignment): (usize, usize),
    members: Vec<(Member, usize)>,
    next: &mut usize,
    definitions: &mut Vec<String>,
) -> String {
    let members: Vec<_> = members
        .into_iter()
        .map(|(member, offset)| {
            metadata(next, definitions, format!(
                "!DIDerivedType(tag: DW_TAG_member, name: {}, scope: !{id}, baseType: !{}, size: {}, align: {}, offset: {})",
                quote(&member.name),
                member.ty,
                member.size * 8,
                member.alignment * 8,
                offset * 8
            ))
        })
        .collect();
    let elements = list(&members, next, definitions);
    format!(
        "distinct !DICompositeType(tag: {tag}, name: {name}, size: {}, align: {}, elements: !{elements})",
        size * 8,
        alignment * 8
    )
}

/// The metadata tuple of `ids`.
fn list(ids: &[usize], next: &mut usize, definitions: &mut Vec<String>) -> usize {
    metadata(
        next,
        definitions,
        format!(
            "!{{{}}}",
            ids.iter()
                .map(|id| format!("!{id}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    )
}

/// The members of a structure that `DebugContext::ty` builds from Tsuzuri types.
fn fields(ty: &Type, module: &CheckedModule) -> Vec<(String, Type)> {
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
        // A16 D11: a structure with members `0` to `N-1`, as a tuple.
        Type::FixedArray(element, _) => (0..fixed_length(ty))
            .map(|index| (index.to_string(), (**element).clone()))
            .collect(),
        Type::String => sequence(Type::Integer(16, false)),
        Type::Utf8String => sequence(Type::Integer(8, false)),
        Type::Array(element) => sequence((**element).clone()),
        Type::Reference(..) if ty.slice_element().is_some() => {
            sequence(ty.slice_element().unwrap().clone())
        }
        Type::Vec(element) => {
            let mut result = sequence((**element).clone());
            result.push(("capacity".into(), Type::I64));
            result
        }
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
        Type::Simd(vector) => (vector.bytes(), vector.bytes()),
        Type::Integer(bits, _) | Type::Binary(bits) | Type::Decimal(bits) => {
            (usize::from(*bits) / 8, usize::from(*bits) / 8)
        }
        Type::Bool | Type::Unit => (1, 1),
        Type::Char => (2, 2),
        Type::Utf8Char => (4, 4),
        Type::Array(_) | Type::List(_) | Type::String | Type::Utf8String => (16, 8),
        Type::Vec(_) => (24, 8),
        Type::Reference(..) if ty.slice_element().is_some() => (16, 8),
        Type::Reference(..) | Type::Handle(_) | Type::Shared(..) => (pointer, pointer),
        Type::Function(..) | Type::Task(_) => (4 * pointer, pointer),
        Type::Dyn(_) => (2 * pointer, pointer),
        Type::Record(id, arguments) => aggregate(
            module
                .types()
                .record_fields(*id, arguments)
                .into_iter()
                .chain(drop_flag(ty, module).map(|_| Type::Integer(8, false))),
            module,
            wasm,
        ),
        Type::Tuple(elements) => aggregate(elements.iter().cloned(), module, wasm),
        Type::FixedArray(element, _) => {
            let (size, alignment) = layout(element, module, wasm);
            (
                size.next_multiple_of(alignment) * fixed_length(ty),
                alignment,
            )
        }
        Type::Union(..) if module.types().recursive(ty) => (pointer, pointer),
        Type::Union(id, arguments) => match union_layout(*id, arguments, module) {
            UnionLayout::Enum => (4, 4),
            UnionLayout::Common(payload) => aggregate(
                [Type::Integer(32, true), payload]
                    .into_iter()
                    .chain(drop_flag(ty, module).map(|_| Type::Integer(8, false))),
                module,
                wasm,
            ),
            UnionLayout::General(count) if drop_flag(ty, module).is_some() => (32 + count * 16, 16),
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
        artificial: bool,
    ) -> Option<usize> {
        self.debug.as_mut()?.subprogram(
            module,
            function,
            symbol,
            artificial,
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
