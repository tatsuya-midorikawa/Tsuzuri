use super::*;
use crate::check::{TypedHole, TypedInterpolation, spec_text};
use crate::syntax::{FormatAlign, FormatKind, FormatSpec};

/// One hole's text on its way into the joined string.
struct Piece {
    data: String,
    length: String,
    /// What owns `data`; it is released once the text is copied.
    owned: Option<Owned>,
    /// `data` holds ASCII bytes that a UTF-16 result widens while copying.
    ascii: bool,
    pad: Option<Pad>,
}

enum Owned {
    Value(Type, String),
    Buffer(String),
}

/// Fill scalars around a hole's text: the counts, and the fill's encoding.
struct Pad {
    before: String,
    after: String,
    packed: u32,
    width: usize,
}

/// The most bytes `tz_soft_format_spec` writes for a number of this type, with room to spare.
fn format_capacity(ty: &Type, spec: &FormatSpec) -> usize {
    let digits = match ty {
        Type::Binary(16) => 5,
        Type::Binary(32) => 39,
        Type::Binary(64) => 309,
        Type::Binary(128) => 4933,
        Type::Decimal(32) => 97,
        Type::Decimal(64) => 385,
        Type::Decimal(128) => 6145,
        _ => return 160,
    };
    let precision = usize::from(spec.precision.unwrap_or(0));
    match (spec.kind, spec.precision) {
        (None | Some(FormatKind::Fixed), Some(_)) => 16 + digits + precision,
        (Some(FormatKind::Exponent), _) => 24 + precision,
        _ => 144,
    }
}

/// A fill scalar as `width` code units of the result's encoding, lowest unit first.
fn encode_fill(fill: char, utf8: bool) -> (u32, usize) {
    if utf8 {
        let mut bytes = [0u8; 4];
        let encoded = fill.encode_utf8(&mut bytes);
        let packed = encoded
            .bytes()
            .enumerate()
            .fold(0u32, |packed, (index, byte)| {
                packed | u32::from(byte) << (8 * index)
            });
        (packed, encoded.len())
    } else {
        let mut units = [0u16; 2];
        let encoded = fill.encode_utf16(&mut units);
        let packed = encoded
            .iter()
            .enumerate()
            .fold(0u32, |packed, (index, unit)| {
                packed | u32::from(*unit) << (16 * index)
            });
        (packed, encoded.len())
    }
}

impl FunctionEmitter<'_, '_> {
    /// Emits `$"a{x}b"`: each hole is evaluated and shown in order, then the
    /// texts and the shown holes are copied into one allocation.
    pub(super) fn interpolation(
        &mut self,
        interpolation: &TypedInterpolation,
        result: &Type,
    ) -> String {
        let utf8 = *result == Type::Utf8String;
        let (ty, unit, copy, allocate) = if utf8 {
            (
                "%tz.utf8string",
                "i8",
                "@tz.utf8string.copy",
                "@tz.utf8string.allocate",
            )
        } else {
            (
                "%tz.string",
                "i16",
                "@tz.string.copy",
                "@tz.string.allocate",
            )
        };
        let mut cleanup = Vec::new();
        let mut pieces = Vec::new();
        for hole in &interpolation.holes {
            pieces.push(self.hole_piece(hole, utf8, &mut cleanup));
        }
        let mut total = interpolation
            .texts
            .iter()
            .map(StringLiteral::len)
            .sum::<usize>()
            .to_string();
        for piece in &pieces {
            total = self.value(format!("add i64 {total}, {}", piece.length));
            if let Some(pad) = &piece.pad {
                for count in [&pad.before, &pad.after] {
                    let units = self.fill_units(count, pad.width);
                    total = self.value(format!("add i64 {total}, {units}"));
                }
            }
        }
        let allocated = self.value(format!("call {ty} {allocate}(i64 {total})"));
        let output = self.value(format!("extractvalue {ty} {allocated}, 0"));
        let mut offset = "0".to_string();
        for (index, text) in interpolation.texts.iter().enumerate() {
            if !text.is_empty() {
                let constant = self.string_constant(text);
                let target = self.value(format!(
                    "getelementptr inbounds {unit}, ptr {output}, i64 {offset}"
                ));
                self.instruction(format!(
                    "call void {copy}(ptr {target}, ptr {constant}, i64 {})",
                    text.len()
                ));
                offset = self.value(format!("add i64 {offset}, {}", text.len()));
            }
            let Some(piece) = pieces.get(index) else {
                continue;
            };
            if let Some(pad) = &piece.pad {
                offset = self.write_fill(&output, &offset, &pad.before, pad, utf8);
            }
            let target = self.value(format!(
                "getelementptr inbounds {unit}, ptr {output}, i64 {offset}"
            ));
            if piece.ascii && !utf8 {
                self.instruction(format!(
                    "call void @tz.format.widen(ptr {target}, ptr {}, i64 {})",
                    piece.data, piece.length
                ));
            } else {
                self.instruction(format!(
                    "call void {copy}(ptr {target}, ptr {}, i64 {})",
                    piece.data, piece.length
                ));
            }
            offset = self.value(format!("add i64 {offset}, {}", piece.length));
            if let Some(pad) = &piece.pad {
                offset = self.write_fill(&output, &offset, &pad.after, pad, utf8);
            }
        }
        for piece in pieces {
            match piece.owned {
                Some(Owned::Value(ty, value)) => self.drop_value(&ty, &value),
                Some(Owned::Buffer(buffer)) => {
                    self.instruction(format!("call void @tz.free(ptr {buffer})"));
                }
                None => {}
            }
        }
        self.release_operands(cleanup);
        allocated
    }

    fn fill_units(&mut self, count: &str, width: usize) -> String {
        if width == 1 {
            count.to_owned()
        } else {
            self.value(format!("mul i64 {count}, {width}"))
        }
    }

    /// Writes `count` fill scalars at `offset` and returns the new offset.
    fn write_fill(
        &mut self,
        output: &str,
        offset: &str,
        count: &str,
        pad: &Pad,
        utf8: bool,
    ) -> String {
        let (unit, helper) = if utf8 {
            ("i8", "@tz.format.fill.u8")
        } else {
            ("i16", "@tz.format.fill.u16")
        };
        let target = self.value(format!(
            "getelementptr inbounds {unit}, ptr {output}, i64 {offset}"
        ));
        let written = self.value(format!(
            "call i64 {helper}(ptr {target}, i64 {count}, i32 {}, i64 {})",
            pad.packed, pad.width
        ));
        self.value(format!("add i64 {offset}, {written}"))
    }

    /// Evaluates one hole and returns its text; the borrowed operand is
    /// released by the caller once the join has copied everything.
    fn hole_piece(
        &mut self,
        hole: &TypedHole,
        utf8: bool,
        cleanup: &mut Vec<(Type, String, String, Vec<Frame>)>,
    ) -> Piece {
        let Type::Reference(inner, _) = &hole.operand.ty else {
            unreachable!("a hole operand is a borrow")
        };
        let inner = inner.as_ref();
        let operand = if let TypedExprKind::BorrowOperand(value) = &hole.operand.kind {
            self.operand_borrow(&hole.operand, value.as_ref(), cleanup)
        } else {
            self.expression(&hole.operand)
        };
        if hole.custom {
            return self.format_piece(hole, &operand, utf8);
        }
        let text = if utf8 { Type::Utf8String } else { Type::String };
        let formatted = hole
            .spec
            .is_some_and(|spec| spec.plus || spec.precision.is_some() || spec.kind.is_some());
        let mut piece = if !formatted && *inner == text {
            let ty = self.ty(&text);
            let value = self.value(format!("load {ty}, ptr {operand}"));
            Piece {
                data: self.value(format!("extractvalue {ty} {value}, 0")),
                length: self.value(format!("extractvalue {ty} {value}, 1")),
                owned: None,
                ascii: false,
                pad: None,
            }
        } else if let Some(spec) = hole.spec.filter(|_| formatted) {
            self.number_piece(&operand, inner, &spec)
        } else {
            let method = hole.method.as_ref().expect("a shown hole has a method");
            let shown = self.comparison_values(method, &[operand]);
            self.remember_temporary(&Type::String, &shown, &[]);
            self.shown_piece(shown, utf8)
        };
        if let Some(spec) = hole.spec.filter(|spec| spec.width > 0) {
            piece.pad = Some(self.pad_for(&piece, &spec, inner, utf8));
        }
        piece
    }

    /// The text a `Format` instance returns for a hole: the instance gets the
    /// borrowed value and the spec text, and does its own padding.
    fn format_piece(&mut self, hole: &TypedHole, operand: &str, utf8: bool) -> Piece {
        let spec = hole.spec.expect("a Format hole has a spec");
        let method = hole.method.as_ref().expect("a Format hole has a method");
        // The spec is a borrowed constant; the callee only reads it.
        let units: Vec<u16> = spec_text(&spec).encode_utf16().collect();
        let length = units.len();
        let data = self.string_constant(&StringLiteral::Utf16(units));
        let slot = self.fresh();
        self.allocas
            .push(format!("{slot} = alloca %tz.string, align 8"));
        let with_data = self.value(format!("insertvalue %tz.string undef, ptr {data}, 0"));
        let value = self.value(format!(
            "insertvalue %tz.string {with_data}, i64 {length}, 1"
        ));
        self.instruction(format!("store %tz.string {value}, ptr {slot}"));
        let shown = self.comparison_values(method, &[operand.to_owned(), slot]);
        self.remember_temporary(&Type::String, &shown, &[]);
        self.shown_piece(shown, utf8)
    }

    /// Formats a number from its spec into ASCII, on the stack when the text is
    /// short and on the heap otherwise.
    fn number_piece(&mut self, operand: &str, ty: &Type, spec: &FormatSpec) -> Piece {
        let capacity = format_capacity(ty, spec);
        let (buffer, owned) = if capacity <= 512 {
            let slot = self.fresh();
            self.allocas
                .push(format!("{slot} = alloca [{capacity} x i8], align 16"));
            (slot, None)
        } else {
            let buffer = self.value(format!("call ptr @tz.alloc(i64 {capacity})"));
            (buffer.clone(), Some(Owned::Buffer(buffer)))
        };
        let style = match spec.kind {
            None if spec.precision.is_some() => 1,
            None => 0,
            Some(FormatKind::Fixed) => 1,
            Some(FormatKind::Exponent) => 2,
            Some(FormatKind::LowerHex) => 3,
            Some(FormatKind::UpperHex) => 4,
            Some(FormatKind::Octal) => 5,
            Some(FormatKind::Binary) => 6,
        };
        let flags = u32::from(spec.plus) | (style << 4);
        let count = self.value(format!(
            "call i32 @tz_soft_format_spec(ptr {buffer}, ptr {operand}, i32 {}, i32 {flags}, i32 {})",
            numeric_kind(ty),
            spec.precision.unwrap_or(0)
        ));
        Piece {
            data: buffer,
            length: self.value(format!("zext i32 {count} to i64")),
            owned,
            ascii: true,
            pad: None,
        }
    }

    /// The text of a `Display.display` result in the encoding of the result.
    fn shown_piece(&mut self, shown: String, utf8: bool) -> Piece {
        let data = self.value(format!("extractvalue %tz.string {shown}, 0"));
        let length = self.value(format!("extractvalue %tz.string {shown}, 1"));
        if !utf8 {
            return Piece {
                data,
                length,
                owned: Some(Owned::Value(Type::String, shown)),
                ascii: false,
                pad: None,
            };
        }
        let converted = self.value(format!(
            "call %tz.utf8string @tz.utf8string.from_string(ptr {data}, i64 {length})"
        ));
        self.drop_value(&Type::String, &shown);
        self.remember_temporary(&Type::Utf8String, &converted, &[]);
        Piece {
            data: self.value(format!("extractvalue %tz.utf8string {converted}, 0")),
            length: self.value(format!("extractvalue %tz.utf8string {converted}, 1")),
            owned: Some(Owned::Value(Type::Utf8String, converted)),
            ascii: false,
            pad: None,
        }
    }

    /// How many fill scalars go before and after the text of a hole with a width.
    fn pad_for(&mut self, piece: &Piece, spec: &FormatSpec, inner: &Type, utf8: bool) -> Pad {
        let scalars = if piece.ascii {
            piece.length.clone()
        } else {
            let helper = if utf8 {
                "@tz.format.scalars.utf8"
            } else {
                "@tz.format.scalars.utf16"
            };
            self.value(format!(
                "call i64 {helper}(ptr {}, i64 {})",
                piece.data, piece.length
            ))
        };
        let short = self.value(format!("icmp ult i64 {scalars}, {}", spec.width));
        let missing = self.value(format!("sub i64 {}, {scalars}", spec.width));
        let total = self.value(format!("select i1 {short}, i64 {missing}, i64 0"));
        let align = spec.align.unwrap_or(if inner.is_numeric() {
            FormatAlign::Right
        } else {
            FormatAlign::Left
        });
        let (before, after) = match align {
            FormatAlign::Left => ("0".to_owned(), total),
            FormatAlign::Right => (total, "0".to_owned()),
            FormatAlign::Center => {
                let before = self.value(format!("lshr i64 {total}, 1"));
                let after = self.value(format!("sub i64 {total}, {before}"));
                (before, after)
            }
        };
        let (packed, width) = encode_fill(spec.fill, utf8);
        Pad {
            before,
            after,
            packed,
            width,
        }
    }

    fn display_element(&mut self, method: &TypedExpr, pointer: &str) -> String {
        let Type::Function(parameters, _) = &method.ty else {
            unreachable!()
        };
        let Type::Reference(element, _) = &parameters[0] else {
            unreachable!()
        };
        let value = self.borrowed_element(element, pointer);
        self.comparison_values(method, &[value])
    }

    pub(super) fn structural_display(&mut self, arguments: &[TypedExpr]) -> String {
        let Type::Reference(ty, _) = &arguments[0].ty else {
            unreachable!()
        };
        let input = self.expression(&arguments[0]);
        let methods = &arguments[1..];
        let (parts, kind) = if let Type::Tuple(elements) = ty.as_ref() {
            let (parts, data) = self.allocate_array(&Type::String, &elements.len().to_string());
            for (index, method) in methods.iter().enumerate() {
                let pointer = self.value(format!(
                    "getelementptr inbounds {}, ptr {input}, i32 0, i32 {index}",
                    self.ty(ty)
                ));
                let text = self.display_element(method, &pointer);
                let target = self.element_pointer(&Type::String, &data, &index.to_string());
                self.instruction(format!("store %tz.string {text}, ptr {target}"));
            }
            (parts, 0)
        } else {
            let (element, linked) = match ty.as_ref() {
                Type::Array(element) => (element.as_ref(), false),
                Type::List(element) => (element.as_ref(), true),
                _ => unreachable!(),
            };
            let aggregate = self.ty(ty);
            let input = if linked {
                self.value(format!("load {aggregate}, ptr {input}"))
            } else {
                input
            };
            let data = self.value(format!("extractvalue {aggregate} {input}, 0"));
            let length = self.value(format!("extractvalue {aggregate} {input}, 1"));
            let (parts, target) = self.allocate_array(&Type::String, &length);
            let cursor =
                linked.then(|| self.spill(&Type::Reference(Box::new(Type::Unit), false), &data));
            self.array_loop(&length, |emitter, index| {
                let pointer = if let Some(cursor) = &cursor {
                    let node = emitter.value(format!("load ptr, ptr {cursor}"));
                    let next = emitter.value(format!("load ptr, ptr {node}"));
                    emitter.instruction(format!("store ptr {next}, ptr {cursor}"));
                    emitter.list_element_pointer(element, &node)
                } else {
                    emitter.element_pointer(element, &data, index)
                };
                let text = emitter.display_element(&methods[0], &pointer);
                let output = emitter.element_pointer(&Type::String, &target, index);
                emitter.instruction(format!("store %tz.string {text}, ptr {output}"));
            });
            (parts, if linked { 2 } else { 1 })
        };
        self.value(format!(
            "call %tz.string @tz.display.join(%tz.array {parts}, i32 {kind})"
        ))
    }

    pub(super) fn display_quoted(&mut self, ty: &Type) -> String {
        let mut temporary = None;
        let (data, length, kind) = match ty {
            Type::String | Type::Utf8String => {
                let text = self.value(format!("load {}, ptr %arg0", self.ty(ty)));
                let mut data = self.value(format!("extractvalue {} {text}, 0", self.ty(ty)));
                let mut length = self.value(format!("extractvalue {} {text}, 1", self.ty(ty)));
                if *ty == Type::Utf8String {
                    let decoded = self.value(format!(
                        "call %tz.string @tz.string.from_utf8(ptr {data}, i64 {length})"
                    ));
                    data = self.value(format!("extractvalue %tz.string {decoded}, 0"));
                    length = self.value(format!("extractvalue %tz.string {decoded}, 1"));
                    temporary = Some(data.clone());
                }
                (data, length, usize::from(*ty == Type::Utf8String))
            }
            Type::Char => ("%arg0".into(), "1".into(), 2),
            Type::Utf8Char => {
                let scalar = self.value("load i32, ptr %arg0");
                let supplementary = self.value(format!("icmp ugt i32 {scalar}, 65535"));
                let offset = self.value(format!("sub i32 {scalar}, 65536"));
                let high = self.value(format!("lshr i32 {offset}, 10"));
                let high = self.value(format!("add i32 {high}, 55296"));
                let first = self.value(format!(
                    "select i1 {supplementary}, i32 {high}, i32 {scalar}"
                ));
                let first = self.value(format!("trunc i32 {first} to i16"));
                let low = self.value(format!("and i32 {offset}, 1023"));
                let low = self.value(format!("add i32 {low}, 56320"));
                let low = self.value(format!("trunc i32 {low} to i16"));
                let data = self.slot(&Type::Tuple(vec![Type::Integer(16, false); 2]));
                self.instruction(format!("store i16 {first}, ptr {data}"));
                let next = self.value(format!("getelementptr inbounds i16, ptr {data}, i64 1"));
                self.instruction(format!("store i16 {low}, ptr {next}"));
                let length = self.value(format!("select i1 {supplementary}, i64 2, i64 1"));
                (data, length, 3)
            }
            _ => unreachable!(),
        };
        let result = self.value(format!(
            "call %tz.string @tz.display.quote(ptr {data}, i64 {length}, i32 {kind})"
        ));
        if let Some(data) = temporary {
            self.instruction(format!("call void @tz.free(ptr {data})"));
        }
        result
    }
}
