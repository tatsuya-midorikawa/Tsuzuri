use super::*;

impl FunctionEmitter<'_, '_> {
    pub(super) fn string_buffer_builtin(
        &mut self,
        instance: &BuiltinInstance,
        signature: &Type,
    ) -> String {
        use Builtin::*;
        let Type::Function(parameters, _) = signature else {
            unreachable!()
        };
        let result = signature.after_arguments(instance.builtin.scheme().parameters.len());
        if matches!(instance.builtin, StringCompare | Utf8StringCompare) {
            let Type::Reference(text, _) = &parameters[0] else {
                unreachable!()
            };
            let ty = self.ty(text);
            let left = self.value(format!("load {ty}, ptr %arg0"));
            let right = self.value(format!("load {ty}, ptr %arg1"));
            let order = self.value(format!(
                "call i32 @{}.compare({ty} {left}, {ty} {right})",
                &ty[1..]
            ));
            return self.value(format!("sext i32 {order} to i64"));
        }
        if instance.builtin == Utf8StringDecodeAt {
            let text = self.value("load %tz.utf8string, ptr %arg0");
            let length = self.value(format!("extractvalue %tz.utf8string {text}, 1"));
            let valid = self.value(format!("icmp ult i64 %arg1, {length}"));
            self.guard(&valid, TrapKind::BoundsCheck);
            let data = self.value(format!("extractvalue %tz.utf8string {text}, 0"));
            return self.value(format!("call {{ i32, i64 }} @tz.string.decode_utf8(ptr {data}, i64 {length}, i64 %arg1, i1 false)"));
        }
        let input_type = self.ty(&parameters[0]);
        let data = self.value(format!("extractvalue {input_type} %arg0, 0"));
        let length = self.value(format!("extractvalue {input_type} %arg0, 1"));
        if instance.builtin == StringFromCodeUnits {
            let valid = self.value(format!("icmp ule i64 {length}, 9007199254740991"));
            self.guard(&valid, TrapKind::AllocationSize);
        }
        if instance.builtin == Utf8StringFromBytes {
            let index_slot = self.spill(&Type::I64, "0");
            let test = self.label();
            let decode = self.label();
            let invalid = self.label();
            let advance = self.label();
            let done = self.label();
            self.jump(&test);
            self.begin(&test);
            let index = self.value(format!("load i64, ptr {index_slot}"));
            let more = self.value(format!("icmp ult i64 {index}, {length}"));
            self.branch(&more, &decode, &done);
            self.begin(&decode);
            let decoded = self.value(format!("call {{ i32, i64 }} @tz.string.try_decode_utf8(ptr {data}, i64 {length}, i64 {index}, i1 false)"));
            let scalar = self.value(format!("extractvalue {{ i32, i64 }} {decoded}, 0"));
            let valid = self.value(format!("icmp sge i32 {scalar}, 0"));
            self.branch(&valid, &advance, &invalid);
            self.begin(&invalid);
            self.instruction(format!("call void @tz.free(ptr {data})"));
            let Type::Union(id, _) = &result else {
                unreachable!()
            };
            let none = self.module.unions[*id]
                .cases
                .iter()
                .position(|(name, _)| name == "None")
                .unwrap();
            let empty = self.construct_value(&result, none, None);
            self.instruction(format!("ret {} {empty}", self.ty(&result)));
            self.begin(&advance);
            let next = self.value(format!("extractvalue {{ i32, i64 }} {decoded}, 1"));
            self.instruction(format!("store i64 {next}, ptr {index_slot}"));
            self.jump(&test);
            self.begin(&done);
        }
        let value_type = if instance.builtin == Utf8StringFromBytes {
            Type::Utf8String
        } else {
            result.clone()
        };
        let value = self.value(format!(
            "insertvalue {} zeroinitializer, ptr {data}, 0",
            self.ty(&value_type)
        ));
        let value = self.value(format!(
            "insertvalue {} {value}, i64 {length}, 1",
            self.ty(&value_type)
        ));
        if instance.builtin == Utf8StringFromBytes {
            self.integer_some(&result, &value_type, &value)
        } else {
            value
        }
    }

    pub(super) fn make_vector(&mut self, data: &str, length: &str, capacity: &str) -> String {
        let value = self.value(format!(
            "insertvalue %tz.vec zeroinitializer, ptr {data}, 0"
        ));
        let value = self.value(format!("insertvalue %tz.vec {value}, i64 {length}, 1"));
        self.value(format!("insertvalue %tz.vec {value}, i64 {capacity}, 2"))
    }

    fn vector_buffer(&mut self, element: &Type, capacity: &str) -> String {
        let size = self.allocation_size(&self.ty(element), capacity);
        let zero = self.value(format!("icmp eq i64 {capacity}, 0"));
        let empty = self.label();
        let allocate = self.label();
        let done = self.label();
        self.branch(&zero, &empty, &allocate);
        self.begin(&empty);
        self.jump(&done);
        self.begin(&allocate);
        let pointer = self.value(format!("call ptr @tz.alloc(i64 {size})"));
        self.jump(&done);
        self.begin(&done);
        self.value(format!(
            "phi ptr [ null, %{empty} ], [ {pointer}, %{allocate} ]"
        ))
    }

    pub(super) fn clone_vector(&mut self, element: &Type, source: &str) -> String {
        let data = self.value(format!("extractvalue %tz.vec {source}, 0"));
        let length = self.value(format!("extractvalue %tz.vec {source}, 1"));
        let capacity = self.value(format!("extractvalue %tz.vec {source}, 2"));
        let output = self.vector_buffer(element, &capacity);
        self.array_loop(&length, |emitter, index| {
            let pointer = emitter.element_pointer(element, &data, index);
            let value = emitter.value(format!("load {}, ptr {pointer}", emitter.ty(element)));
            let value = emitter.clone_value(element, &value);
            let pointer = emitter.element_pointer(element, &output, index);
            emitter.instruction(format!(
                "store {} {value}, ptr {pointer}",
                emitter.ty(element)
            ));
        });
        self.make_vector(&output, &length, &capacity)
    }

    pub(super) fn vector_builtin(
        &mut self,
        instance: &BuiltinInstance,
        signature: &Type,
    ) -> String {
        use Builtin::*;
        let element = &instance.types[0];
        match instance.builtin {
            VecEmpty => "zeroinitializer".into(),
            VecWithCapacity => {
                let data = self.vector_buffer(element, "%arg0");
                self.make_vector(&data, "0", "%arg0")
            }
            VecLength | VecCapacity | VecIsEmpty => {
                let value = self.value("load %tz.vec, ptr %arg0");
                let part = if instance.builtin == VecCapacity {
                    2
                } else {
                    1
                };
                let size = self.value(format!("extractvalue %tz.vec {value}, {part}"));
                if instance.builtin == VecIsEmpty {
                    self.value(format!("icmp eq i64 {size}, 0"))
                } else {
                    size
                }
            }
            VecClone => {
                let value = self.value("load %tz.vec, ptr %arg0");
                self.clone_vector(element, &value)
            }
            VecReserve => self.reserve_vector(element, "%arg0", "%arg1"),
            VecPush => {
                let value = self.reserve_vector(element, "%arg0", "1");
                let data = self.value(format!("extractvalue %tz.vec {value}, 0"));
                let length = self.value(format!("extractvalue %tz.vec {value}, 1"));
                let pointer = self.element_pointer(element, &data, &length);
                self.instruction(format!("store {} %arg1, ptr {pointer}", self.ty(element)));
                let length = self.value(format!("add i64 {length}, 1"));
                self.value(format!("insertvalue %tz.vec {value}, i64 {length}, 1"))
            }
            VecToArray => {
                let data = self.value("extractvalue %tz.vec %arg0, 0");
                let length = self.value("extractvalue %tz.vec %arg0, 1");
                let value = self.value(format!(
                    "insertvalue %tz.array zeroinitializer, ptr {data}, 0"
                ));
                self.value(format!("insertvalue %tz.array {value}, i64 {length}, 1"))
            }
            VecOfArray => {
                let data = self.value("extractvalue %tz.array %arg0, 0");
                let length = self.value("extractvalue %tz.array %arg0, 1");
                let zero = self.value(format!("icmp eq i64 {length}, 0"));
                let empty = self.label();
                let nonempty = self.label();
                let done = self.label();
                self.branch(&zero, &empty, &nonempty);
                self.begin(&empty);
                self.instruction(format!("call void @tz.free(ptr {data})"));
                self.jump(&done);
                self.begin(&nonempty);
                let value = self.make_vector(&data, &length, &length);
                self.jump(&done);
                self.begin(&done);
                self.value(format!(
                    "phi %tz.vec [ zeroinitializer, %{empty} ], [ {value}, %{nonempty} ]"
                ))
            }
            VecSet | VecSwap | VecAt | VecGet => {
                let borrowed = matches!(instance.builtin, VecAt | VecGet);
                let value = if borrowed {
                    self.value("load %tz.vec, ptr %arg0")
                } else {
                    "%arg0".into()
                };
                let vector = Type::Vec(Box::new(element.clone()));
                let result_type =
                    signature.after_arguments(instance.builtin.scheme().parameters.len());
                if instance.builtin == VecGet {
                    let length = self.value(format!("extractvalue %tz.vec {value}, 1"));
                    let invalid = self.value(format!("icmp uge i64 %arg1, {length}"));
                    self.integer_none_on(&result_type, &invalid);
                }
                let pointer = self.checked_element_pointer(&vector, &value, "%arg1");
                if instance.builtin == VecAt {
                    return self.borrowed_element(element, &pointer);
                }
                let previous = self.value(format!("load {}, ptr {pointer}", self.ty(element)));
                match instance.builtin {
                    VecGet => {
                        let cloned = self.clone_value(element, &previous);
                        self.integer_some(&result_type, element, &cloned)
                    }
                    VecSet => {
                        self.drop_value(element, &previous);
                        self.instruction(format!(
                            "store {} %arg2, ptr {pointer}",
                            self.ty(element)
                        ));
                        value
                    }
                    VecSwap => {
                        let other = self.checked_element_pointer(&vector, &value, "%arg2");
                        let second = self.value(format!("load {}, ptr {other}", self.ty(element)));
                        self.instruction(format!(
                            "store {} {second}, ptr {pointer}",
                            self.ty(element)
                        ));
                        self.instruction(format!(
                            "store {} {previous}, ptr {other}",
                            self.ty(element)
                        ));
                        value
                    }
                    _ => unreachable!(),
                }
            }
            VecTruncate | VecClear => {
                let requested = if instance.builtin == VecClear {
                    "0"
                } else {
                    "%arg1"
                };
                let valid = self.value(format!("icmp sge i64 {requested}, 0"));
                self.guard(&valid, TrapKind::BoundsCheck);
                let data = self.value("extractvalue %tz.vec %arg0, 0");
                let length = self.value("extractvalue %tz.vec %arg0, 1");
                let shorter = self.value(format!("icmp ult i64 {requested}, {length}"));
                let next_length = self.value(format!(
                    "select i1 {shorter}, i64 {requested}, i64 {length}"
                ));
                let count = self.value(format!("sub i64 {length}, {next_length}"));
                if element.needs_drop(&self.module.types()) {
                    self.array_loop(&count, |emitter, index| {
                        let index = emitter.value(format!("add i64 {next_length}, {index}"));
                        let pointer = emitter.element_pointer(element, &data, &index);
                        let value =
                            emitter.value(format!("load {}, ptr {pointer}", emitter.ty(element)));
                        emitter.drop_value(element, &value);
                        emitter.instruction(format!(
                            "store {} zeroinitializer, ptr {pointer}",
                            emitter.ty(element)
                        ));
                    });
                }
                self.value(format!("insertvalue %tz.vec %arg0, i64 {next_length}, 1"))
            }
            VecPop => {
                let result = signature.after_arguments(1);
                let Type::Tuple(fields) = &result else {
                    unreachable!()
                };
                let option = &fields[1];
                let length = self.value("extractvalue %tz.vec %arg0, 1");
                let zero = self.value(format!("icmp eq i64 {length}, 0"));
                let empty = self.label();
                let populated = self.label();
                self.branch(&zero, &empty, &populated);
                self.begin(&empty);
                let Type::Union(id, _) = option else {
                    unreachable!()
                };
                let none = self.module.unions[*id]
                    .cases
                    .iter()
                    .position(|(name, _)| name == "None")
                    .unwrap();
                let none = self.construct_value(option, none, None);
                let tuple = self.value(format!(
                    "insertvalue {} zeroinitializer, %tz.vec %arg0, 0",
                    self.ty(&result)
                ));
                let tuple = self.value(format!(
                    "insertvalue {} {tuple}, {} {none}, 1",
                    self.ty(&result),
                    self.ty(option)
                ));
                self.instruction(format!("ret {} {tuple}", self.ty(&result)));
                self.begin(&populated);
                let index = self.value(format!("sub i64 {length}, 1"));
                let data = self.value("extractvalue %tz.vec %arg0, 0");
                let pointer = self.element_pointer(element, &data, &index);
                let value = self.value(format!("load {}, ptr {pointer}", self.ty(element)));
                self.instruction(format!(
                    "store {} zeroinitializer, ptr {pointer}",
                    self.ty(element)
                ));
                let option_value = self.integer_some(option, element, &value);
                let vector = self.value(format!("insertvalue %tz.vec %arg0, i64 {index}, 1"));
                let tuple = self.value(format!(
                    "insertvalue {} zeroinitializer, %tz.vec {vector}, 0",
                    self.ty(&result)
                ));
                self.value(format!(
                    "insertvalue {} {tuple}, {} {option_value}, 1",
                    self.ty(&result),
                    self.ty(option)
                ))
            }
            _ => unreachable!("vector builtins only"),
        }
    }

    fn reserve_vector(&mut self, element: &Type, value: &str, additional: &str) -> String {
        let length = self.value(format!("extractvalue %tz.vec {value}, 1"));
        let capacity = self.value(format!("extractvalue %tz.vec {value}, 2"));
        let available = self.value(format!("sub i64 9223372036854775807, {length}"));
        let valid = self.value(format!("icmp ule i64 {additional}, {available}"));
        self.guard(&valid, TrapKind::AllocationSize);
        let required = self.value(format!("add i64 {length}, {additional}"));
        let enough = self.value(format!("icmp ule i64 {required}, {capacity}"));
        let unchanged = self.label();
        let grow = self.label();
        let test = self.label();
        let double = self.label();
        let resize = self.label();
        let done = self.label();
        self.branch(&enough, &unchanged, &grow);
        self.begin(&unchanged);
        self.jump(&done);
        self.begin(&grow);
        let empty = self.value(format!("icmp eq i64 {capacity}, 0"));
        let initial = self.value(format!("select i1 {empty}, i64 4, i64 {capacity}"));
        let slot = self.spill(&Type::I64, &initial);
        self.jump(&test);
        self.begin(&test);
        let next_capacity = self.value(format!("load i64, ptr {slot}"));
        let enough = self.value(format!("icmp uge i64 {next_capacity}, {required}"));
        self.branch(&enough, &resize, &double);
        self.begin(&double);
        let fits = self.value(format!("icmp ule i64 {next_capacity}, 4611686018427387903"));
        self.guard(&fits, TrapKind::AllocationSize);
        let doubled = self.value(format!("mul i64 {next_capacity}, 2"));
        self.instruction(format!("store i64 {doubled}, ptr {slot}"));
        self.jump(&test);
        self.begin(&resize);
        let data = self.value(format!("extractvalue %tz.vec {value}, 0"));
        let old_size = self.allocation_size(&self.ty(element), &capacity);
        let new_size = self.allocation_size(&self.ty(element), &next_capacity);
        let data = self.value(format!(
            "call ptr @tz.realloc(ptr {data}, i64 {old_size}, i64 {new_size})"
        ));
        let resized = self.make_vector(&data, &length, &next_capacity);
        let resized_block = self.block.clone();
        self.jump(&done);
        self.begin(&done);
        self.value(format!(
            "phi %tz.vec [ {value}, %{unchanged} ], [ {resized}, %{resized_block} ]"
        ))
    }

    pub(super) fn borrowed_element(&mut self, element: &Type, pointer: &str) -> String {
        if matches!(element, Type::Array(_)) {
            self.value(format!("load %tz.array, ptr {pointer}"))
        } else {
            pointer.to_owned()
        }
    }

    pub(super) fn bulk_collection_builtin(
        &mut self,
        instance: &BuiltinInstance,
        signature: &Type,
    ) -> String {
        let Type::Function(parameters, _) = signature else {
            unreachable!()
        };
        // The instance types follow the order in which the scheme first names its variables, and
        // `List.fold_ref` names its state before the element, so its element comes from the list.
        let element = match (instance.builtin, parameters.get(2)) {
            (Builtin::ListFoldRef, Some(Type::Reference(list, false))) => match list.as_ref() {
                Type::List(element) => element.as_ref(),
                _ => unreachable!("List.fold_ref reads a borrowed list"),
            },
            _ => &instance.types[0],
        };
        match instance.builtin {
            Builtin::ArrayConcat => {
                let rows = self.value("extractvalue %tz.array %arg0, 0");
                let count = self.value("extractvalue %tz.array %arg0, 1");
                let total = self.spill(&Type::I64, "0");
                self.array_loop(&count, |emitter, index| {
                    let pointer = emitter.element_pointer(
                        &Type::Array(Box::new(element.clone())),
                        &rows,
                        index,
                    );
                    let row = emitter.value(format!("load %tz.array, ptr {pointer}"));
                    let length = emitter.value(format!("extractvalue %tz.array {row}, 1"));
                    let before = emitter.value(format!("load i64, ptr {total}"));
                    let available = emitter.value(format!("sub i64 9223372036854775807, {before}"));
                    let valid = emitter.value(format!("icmp ule i64 {length}, {available}"));
                    emitter.guard(&valid, TrapKind::AllocationSize);
                    let after = emitter.value(format!("add i64 {before}, {length}"));
                    emitter.instruction(format!("store i64 {after}, ptr {total}"));
                });
                let length = self.value(format!("load i64, ptr {total}"));
                let (result, output) = self.allocate_array(element, &length);
                let offset = self.spill(&Type::I64, "0");
                self.array_loop(&count, |emitter, index| {
                    let pointer = emitter.element_pointer(
                        &Type::Array(Box::new(element.clone())),
                        &rows,
                        index,
                    );
                    let row = emitter.value(format!("load %tz.array, ptr {pointer}"));
                    let data = emitter.value(format!("extractvalue %tz.array {row}, 0"));
                    let length = emitter.value(format!("extractvalue %tz.array {row}, 1"));
                    let start = emitter.value(format!("load i64, ptr {offset}"));
                    emitter.array_loop(&length, |emitter, index| {
                        let source = emitter.element_pointer(element, &data, index);
                        let value =
                            emitter.value(format!("load {}, ptr {source}", emitter.ty(element)));
                        let value = emitter.clone_value(element, &value);
                        let target_index = emitter.value(format!("add i64 {start}, {index}"));
                        let target = emitter.element_pointer(element, &output, &target_index);
                        emitter.instruction(format!(
                            "store {} {value}, ptr {target}",
                            emitter.ty(element)
                        ));
                    });
                    let next = emitter.value(format!("add i64 {start}, {length}"));
                    emitter.instruction(format!("store i64 {next}, ptr {offset}"));
                });
                result
            }
            Builtin::ArrayToList => {
                let data = self.value("extractvalue %tz.array %arg0, 0");
                let length = self.value("extractvalue %tz.array %arg0, 1");
                let (head, tail) = self.list_builder();
                self.array_loop(&length, |emitter, index| {
                    let pointer = emitter.element_pointer(element, &data, index);
                    let value =
                        emitter.value(format!("load {}, ptr {pointer}", emitter.ty(element)));
                    let value = emitter.clone_value(element, &value);
                    emitter.append_list(element, &tail, &value);
                });
                self.finish_list(&head, &length)
            }
            Builtin::ArraySortBy => self.stable_array_sort(element, &parameters[0]),
            Builtin::ListMap
            | Builtin::ListMapRef
            | Builtin::ListReverse
            | Builtin::ListToArray
            | Builtin::ListFoldRef => {
                // Callbacks come first, as in `List.map f xs` and `List.fold_ref f state xs`.
                let source = match instance.builtin {
                    Builtin::ListMap | Builtin::ListMapRef => "%arg1",
                    Builtin::ListFoldRef => "%arg2",
                    _ => "%arg0",
                };
                let list = self.value(format!("load %tz.list, ptr {source}"));
                let head = self.value(format!("extractvalue %tz.list {list}, 0"));
                let length = self.value(format!("extractvalue %tz.list {list}, 1"));
                if instance.builtin == Builtin::ListToArray {
                    let (array, data) = self.allocate_array(element, &length);
                    let index = self.spill(&Type::I64, "0");
                    self.list_loop(&head, &length, |emitter, node| {
                        let pointer = emitter.list_element_pointer(element, node);
                        let value =
                            emitter.value(format!("load {}, ptr {pointer}", emitter.ty(element)));
                        let value = emitter.clone_value(element, &value);
                        let offset = emitter.value(format!("load i64, ptr {index}"));
                        let output = emitter.element_pointer(element, &data, &offset);
                        emitter.instruction(format!(
                            "store {} {value}, ptr {output}",
                            emitter.ty(element)
                        ));
                        let next = emitter.value(format!("add i64 {offset}, 1"));
                        emitter.instruction(format!("store i64 {next}, ptr {index}"));
                    });
                    return array;
                }
                if instance.builtin == Builtin::ListFoldRef {
                    let state_type = &parameters[1];
                    let state = self.spill(state_type, "%arg1");
                    self.list_loop(&head, &length, |emitter, node| {
                        let pointer = emitter.list_element_pointer(element, node);
                        let borrowed = emitter.borrowed_element(element, &pointer);
                        let previous =
                            emitter.value(format!("load {}, ptr {state}", emitter.ty(state_type)));
                        let (partial, rest) = emitter.apply_value(
                            "%arg0",
                            &parameters[0],
                            Some((state_type, &previous)),
                            true,
                        );
                        let borrowed_type = Type::Reference(Box::new(element.clone()), false);
                        let next = emitter
                            .apply_value(&partial, &rest, Some((&borrowed_type, &borrowed)), false)
                            .0;
                        emitter.instruction(format!(
                            "store {} {next}, ptr {state}",
                            emitter.ty(state_type)
                        ));
                    });
                    self.drop_value(&parameters[0], "%arg0");
                    return self.value(format!("load {}, ptr {state}", self.ty(state_type)));
                }
                let output_element =
                    if matches!(instance.builtin, Builtin::ListMap | Builtin::ListMapRef) {
                        &instance.types[1]
                    } else {
                        element
                    };
                let (output_head, tail) = self.list_builder();
                self.list_loop(&head, &length, |emitter, node| {
                    let pointer = emitter.list_element_pointer(element, node);
                    let value = if instance.builtin == Builtin::ListMapRef {
                        let borrowed = emitter.borrowed_element(element, &pointer);
                        let borrowed_type = Type::Reference(Box::new(element.clone()), false);
                        emitter.apply_value("%arg0", &parameters[0], Some((&borrowed_type, &borrowed)), true).0
                    } else {
                        let value = emitter.value(format!("load {}, ptr {pointer}", emitter.ty(element)));
                        let value = emitter.clone_value(element, &value);
                        if instance.builtin == Builtin::ListMap {
                            emitter.apply_value("%arg0", &parameters[0], Some((element, &value)), true).0
                        } else { value }
                    };
                    if instance.builtin == Builtin::ListReverse {
                        let previous = emitter.value(format!("load ptr, ptr {output_head}"));
                        let node_type = emitter.list_node_type(element);
                        let next = emitter.value(format!("call ptr @tz.alloc(i64 ptrtoint (ptr getelementptr ({node_type}, ptr null, i32 1) to i64))"));
                        emitter.instruction(format!("store ptr {previous}, ptr {next}"));
                        let slot = emitter.list_element_pointer(element, &next);
                        emitter.instruction(format!("store {} {value}, ptr {slot}", emitter.ty(element)));
                        emitter.instruction(format!("store ptr {next}, ptr {output_head}"));
                    } else { emitter.append_list(output_element, &tail, &value); }
                });
                if matches!(instance.builtin, Builtin::ListMap | Builtin::ListMapRef) {
                    self.drop_value(&parameters[0], "%arg0");
                }
                self.finish_list(&output_head, &length)
            }
            _ => unreachable!("bulk collection builtins only"),
        }
    }

    fn stable_array_sort(&mut self, element: &Type, comparator: &Type) -> String {
        let array_type = Type::Array(Box::new(element.clone()));
        let copied = self.clone_value(&array_type, "%arg1");
        let data = self.value(format!("extractvalue %tz.array {copied}, 0"));
        let length = self.value("extractvalue %tz.array %arg1, 1");
        let (_, scratch) = self.allocate_array(element, &length);
        let pointer_type = Type::Reference(Box::new(Type::Unit), false);
        let source_slot = self.spill(&pointer_type, &data);
        let target_slot = self.spill(&pointer_type, &scratch);
        let width_slot = self.spill(&Type::I64, "1");
        let base_slot = self.spill(&Type::I64, "0");
        let left_slot = self.slot(&Type::I64);
        let right_slot = self.slot(&Type::I64);
        let output_slot = self.slot(&Type::I64);
        let pass = self.label();
        let runs = self.label();
        let merge = self.label();
        let choose = self.label();
        let tails = self.label();
        let next_run = self.label();
        let next_pass = self.label();
        let done = self.label();
        self.jump(&pass);
        self.begin(&pass);
        let width = self.value(format!("load i64, ptr {width_slot}"));
        let more = self.value(format!("icmp ult i64 {width}, {length}"));
        self.branch(&more, &runs, &done);
        self.begin(&runs);
        let source = self.value(format!("load ptr, ptr {source_slot}"));
        let target = self.value(format!("load ptr, ptr {target_slot}"));
        let base = self.value(format!("load i64, ptr {base_slot}"));
        let remaining = self.value(format!("sub i64 {length}, {base}"));
        let short = self.value(format!("icmp ult i64 {remaining}, {width}"));
        let count = self.value(format!("select i1 {short}, i64 {remaining}, i64 {width}"));
        let middle = self.value(format!("add i64 {base}, {count}"));
        let remaining = self.value(format!("sub i64 {length}, {middle}"));
        let short = self.value(format!("icmp ult i64 {remaining}, {width}"));
        let count = self.value(format!("select i1 {short}, i64 {remaining}, i64 {width}"));
        let finish = self.value(format!("add i64 {middle}, {count}"));
        self.instruction(format!("store i64 {base}, ptr {left_slot}"));
        self.instruction(format!("store i64 {middle}, ptr {right_slot}"));
        self.instruction(format!("store i64 {base}, ptr {output_slot}"));
        self.jump(&merge);
        self.begin(&merge);
        let left = self.value(format!("load i64, ptr {left_slot}"));
        let right = self.value(format!("load i64, ptr {right_slot}"));
        let output = self.value(format!("load i64, ptr {output_slot}"));
        let has_left = self.value(format!("icmp ult i64 {left}, {middle}"));
        let has_right = self.value(format!("icmp ult i64 {right}, {finish}"));
        let both = self.value(format!("and i1 {has_left}, {has_right}"));
        self.branch(&both, &choose, &tails);
        self.begin(&choose);
        let left_pointer = self.element_pointer(element, &source, &left);
        let right_pointer = self.element_pointer(element, &source, &right);
        let left_borrow = self.borrowed_element(element, &left_pointer);
        let right_borrow = self.borrowed_element(element, &right_pointer);
        let borrowed_type = Type::Reference(Box::new(element.clone()), false);
        let (partial, rest) = self.apply_value(
            "%arg0",
            comparator,
            Some((&borrowed_type, &left_borrow)),
            true,
        );
        let comparison = self
            .apply_value(
                &partial,
                &rest,
                Some((&borrowed_type, &right_borrow)),
                false,
            )
            .0;
        let mut take_right = self.value(format!("icmp sgt i64 {comparison}, 0"));
        if matches!(element, Type::Binary(32 | 64)) {
            let left_value = self.value(format!("load {}, ptr {left_pointer}", self.ty(element)));
            let right_value = self.value(format!("load {}, ptr {right_pointer}", self.ty(element)));
            let left_nan = self.value(format!(
                "fcmp uno {} {left_value}, {left_value}",
                self.ty(element)
            ));
            let right_nan = self.value(format!(
                "fcmp uno {} {right_value}, {right_value}",
                self.ty(element)
            ));
            let unordered = self.value(format!("or i1 {left_nan}, {right_nan}"));
            let right_number = self.value(format!("xor i1 {right_nan}, true"));
            let nan_order = self.value(format!("and i1 {left_nan}, {right_number}"));
            take_right = self.value(format!(
                "select i1 {unordered}, i1 {nan_order}, i1 {take_right}"
            ));
        }
        let selected = self.value(format!(
            "select i1 {take_right}, ptr {right_pointer}, ptr {left_pointer}"
        ));
        let value = self.value(format!("load {}, ptr {selected}", self.ty(element)));
        let output_pointer = self.element_pointer(element, &target, &output);
        self.instruction(format!(
            "store {} {value}, ptr {output_pointer}",
            self.ty(element)
        ));
        self.instruction(format!(
            "store {} zeroinitializer, ptr {selected}",
            self.ty(element)
        ));
        let advanced_left = self.value(format!("add i64 {left}, 1"));
        let advanced_right = self.value(format!("add i64 {right}, 1"));
        let next_left = self.value(format!(
            "select i1 {take_right}, i64 {left}, i64 {advanced_left}"
        ));
        let next_right = self.value(format!(
            "select i1 {take_right}, i64 {advanced_right}, i64 {right}"
        ));
        let next_output = self.value(format!("add i64 {output}, 1"));
        self.instruction(format!("store i64 {next_left}, ptr {left_slot}"));
        self.instruction(format!("store i64 {next_right}, ptr {right_slot}"));
        self.instruction(format!("store i64 {next_output}, ptr {output_slot}"));
        self.jump(&merge);
        self.begin(&tails);
        self.move_array_run(element, &source, &target, &left, &middle, &output);
        let remaining_left = self.value(format!("sub i64 {middle}, {left}"));
        let right_output = self.value(format!("add i64 {output}, {remaining_left}"));
        self.move_array_run(element, &source, &target, &right, &finish, &right_output);
        self.jump(&next_run);
        self.begin(&next_run);
        self.instruction(format!("store i64 {finish}, ptr {base_slot}"));
        let more = self.value(format!("icmp ult i64 {finish}, {length}"));
        self.branch(&more, &runs, &next_pass);
        self.begin(&next_pass);
        self.instruction(format!("store ptr {target}, ptr {source_slot}"));
        self.instruction(format!("store ptr {source}, ptr {target_slot}"));
        let remaining = self.value(format!("sub i64 {length}, {width}"));
        let last = self.value(format!("icmp uge i64 {width}, {remaining}"));
        let doubled = self.value(format!("add i64 {width}, {width}"));
        let width = self.value(format!("select i1 {last}, i64 {length}, i64 {doubled}"));
        self.instruction(format!("store i64 {width}, ptr {width_slot}"));
        self.instruction(format!("store i64 0, ptr {base_slot}"));
        self.jump(&pass);
        self.begin(&done);
        let final_data = self.value(format!("load ptr, ptr {source_slot}"));
        let empty_data = self.value(format!("load ptr, ptr {target_slot}"));
        self.instruction(format!("call void @tz.free(ptr {empty_data})"));
        self.drop_value(comparator, "%arg0");
        self.value(format!(
            "insertvalue %tz.array {copied}, ptr {final_data}, 0"
        ))
    }

    fn move_array_run(
        &mut self,
        element: &Type,
        source: &str,
        target: &str,
        start: &str,
        finish: &str,
        output: &str,
    ) {
        let count = self.value(format!("sub i64 {finish}, {start}"));
        self.array_loop(&count, |emitter, index| {
            let from = emitter.value(format!("add i64 {start}, {index}"));
            let to = emitter.value(format!("add i64 {output}, {index}"));
            let source = emitter.element_pointer(element, source, &from);
            let target = emitter.element_pointer(element, target, &to);
            let value = emitter.value(format!("load {}, ptr {source}", emitter.ty(element)));
            emitter.instruction(format!(
                "store {} {value}, ptr {target}",
                emitter.ty(element)
            ));
            emitter.instruction(format!(
                "store {} zeroinitializer, ptr {source}",
                emitter.ty(element)
            ));
        });
    }
}
