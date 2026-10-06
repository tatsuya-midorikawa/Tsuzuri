use super::*;

struct Kernel<'a> {
    operation: Builtin,
    input: &'a Type,
    element: &'a Type,
    callback: &'a Type,
    direct: Option<&'a BorrowedCall>,
}

struct Data<'a> {
    length: &'a str,
    chunks: &'a str,
    input: &'a str,
    output: &'a str,
    identity: &'a str,
    callback: &'a str,
}

impl FunctionEmitter<'_, '_> {
    fn parallel_materialized_call(
        &mut self,
        target: ClosureTarget,
        callback: &str,
    ) -> Option<BorrowedCall> {
        if !self.specializations.can_borrow(target, self.module) {
            return None;
        }
        let function = &self.module.functions[target.function];
        let symbol = if target.bound == 0 {
            format!("@tz.fn.{}", function.qualified_name())
        } else {
            format!(
                "@tz.specialized.{}",
                self.specializations.request(Specialization {
                    function: target.function,
                    callbacks: Vec::new(),
                    borrowed: target.bound
                })?
            )
        };
        let environment = self.value(format!("extractvalue %tz.closure {callback}, 1"));
        let captures = (0..target.bound)
            .map(|index| self.closure_capture_value(function, target.bound, index, &environment))
            .collect();
        Some(BorrowedCall {
            target,
            symbol,
            captures,
            cleanup: Vec::new(),
        })
    }

    fn parallel_count(&mut self, length: &str) -> String {
        let chunks = self.value(format!("udiv i64 {length}, 4096"));
        let remainder = self.value(format!("urem i64 {length}, 4096"));
        let extra = self.value(format!("icmp ne i64 {remainder}, 0"));
        let extra = self.value(format!("zext i1 {extra} to i64"));
        let chunks = self.value(format!("add i64 {chunks}, {extra}"));
        let bounded = self.value(format!("icmp ugt i64 {chunks}, 1024"));
        self.value(format!("select i1 {bounded}, i64 1024, i64 {chunks}"))
    }

    fn parallel_bound(&mut self, index: &str, length: &str, chunks: &str) -> String {
        let base = self.value(format!("udiv i64 {length}, {chunks}"));
        let remainder = self.value(format!("urem i64 {length}, {chunks}"));
        let prefix = self.value(format!("mul i64 {base}, {index}"));
        let extra = self.value(format!("mul i64 {remainder}, {index}"));
        let extra = self.value(format!("udiv i64 {extra}, {chunks}"));
        self.value(format!("add i64 {prefix}, {extra}"))
    }

    fn parallel_apply(
        &mut self,
        callback: &str,
        callback_type: &Type,
        direct: Option<&BorrowedCall>,
        arguments: &[(Type, String)],
    ) -> String {
        if let Some(call) = direct {
            return self.emit_borrowed_call(
                call,
                &arguments
                    .iter()
                    .map(|(_, value)| value.clone())
                    .collect::<Vec<_>>(),
            );
        }
        let mut callback = self.clone_value(callback_type, callback);
        let mut ty = callback_type.clone();
        for (argument_type, value) in arguments {
            (callback, ty) = self.apply_value(&callback, &ty, Some((argument_type, value)), false);
        }
        callback
    }

    fn parallel_launch(&mut self, kernel: Kernel<'_>, data: Data<'_>) {
        let work = self.label();
        let done = self.label();
        let nonempty = self.value(format!("icmp ne i64 {}, 0", data.chunks));
        self.branch(&nonempty, &work, &done);
        self.begin(&work);
        let snapshots = if kernel.direct.is_none() {
            let (array, pointer) = self.allocate_array(kernel.callback, data.chunks);
            self.array_loop(data.chunks, |emitter, chunk| {
                let value = emitter.clone_value(kernel.callback, data.callback);
                let slot = emitter.value(format!(
                    "getelementptr inbounds %tz.closure, ptr {pointer}, i64 {chunk}"
                ));
                emitter.instruction(format!("store %tz.closure {value}, ptr {slot}"));
            });
            Some((array, pointer))
        } else {
            None
        };
        let identity_type = match kernel.operation {
            Builtin::ParallelReduce => self.ty(kernel.element),
            Builtin::ParallelForEachChunk => "i64".into(),
            _ => "i8".into(),
        };
        let mut fields = vec![
            ("i64".to_owned(), data.length.to_owned()),
            ("i64".into(), data.chunks.into()),
            ("ptr".into(), data.input.into()),
            ("ptr".into(), data.output.into()),
            (identity_type, data.identity.into()),
            (
                "ptr".into(),
                snapshots
                    .as_ref()
                    .map_or("null", |(_, pointer)| pointer)
                    .to_owned(),
            ),
        ];
        if let Some(call) = kernel.direct {
            fields.extend(
                call.captures
                    .iter()
                    .zip(
                        &self.module.functions[call.target.function]
                            .signature
                            .parameters,
                    )
                    .map(|(value, ty)| (self.ty(ty), value.clone())),
            );
        }
        let context_type = format!(
            "{{ {} }}",
            fields
                .iter()
                .map(|(ty, _)| ty.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        let context = self.fresh();
        self.allocas
            .push(format!("{context} = alloca {context_type}, align 16"));
        for (index, (ty, value)) in fields.iter().enumerate() {
            let pointer = self.value(format!(
                "getelementptr inbounds {context_type}, ptr {context}, i32 0, i32 {index}"
            ));
            self.instruction(format!("store {ty} {value}, ptr {pointer}"));
        }
        let id = self.globals.parallel_kernels;
        self.globals.parallel_kernels += 1;
        let symbol = format!("@tz.parallel.chunk.{id}");
        let span = self.current_span;
        if let Some(marks) = &mut self.globals.traps {
            marks.sources.insert(symbol.clone(), (span, false));
        }
        let mut emitter = FunctionEmitter::new(
            self.module,
            self.function,
            self.function_id,
            self.builtins,
            self.intrinsics,
            self.globals,
            self.specializations,
        );
        emitter.current_span = span;
        let loaded: Vec<_> = fields
            .iter()
            .enumerate()
            .map(|(index, (ty, _))| {
                let pointer = emitter.value(format!(
                    "getelementptr inbounds {context_type}, ptr %context, i32 0, i32 {index}"
                ));
                emitter.value(format!("load {ty}, ptr {pointer}"))
            })
            .collect();
        let direct = kernel.direct.map(|call| BorrowedCall {
            target: call.target,
            symbol: call.symbol.clone(),
            captures: loaded[6..].to_vec(),
            cleanup: Vec::new(),
        });
        let (callback, snapshot) = if direct.is_none() {
            let pointer = emitter.value(format!(
                "getelementptr inbounds %tz.closure, ptr {}, i64 %chunk",
                loaded[5]
            ));
            let callback = emitter.value(format!("load %tz.closure, ptr {pointer}"));
            (callback, Some(pointer))
        } else {
            (String::new(), None)
        };
        if kernel.operation == Builtin::ParallelForEachChunk {
            emitter.parallel_chunk_body(&kernel, &loaded, &callback, direct.as_ref());
        } else {
            emitter.parallel_items(&kernel, &loaded, &callback, direct.as_ref());
        }
        if let Some(pointer) = snapshot {
            emitter.drop_value(kernel.callback, &callback);
            emitter.instruction(format!("store %tz.closure zeroinitializer, ptr {pointer}"));
        }
        emitter.instruction("ret void");
        let definition = emitter.auxiliary(&format!("void {symbol}(ptr %context, i64 %chunk)"));
        self.intrinsics.insert(definition);
        self.instruction(format!(
            "call void @tsuzuri_task_parallel(ptr {symbol}, ptr {context}, i64 {})",
            data.chunks
        ));
        if let Some((snapshots, _)) = snapshots {
            self.drop_value(&Type::Array(Box::new(kernel.callback.clone())), &snapshots);
        }
        self.jump(&done);
        self.begin(&done);
    }

    /// One chunk of `Parallel.for_each_chunk`: elements `[chunk * size, min(length, (chunk + 1) * size))`
    /// lent to the callback as an exclusive slice, with the chunk's first index (C08 Phase 2).
    fn parallel_chunk_body(
        &mut self,
        kernel: &Kernel<'_>,
        loaded: &[String],
        callback: &str,
        direct: Option<&BorrowedCall>,
    ) {
        let (length, data, size) = (&loaded[0], &loaded[2], &loaded[4]);
        let first = self.value(format!("mul i64 %chunk, {size}"));
        let end = self.value(format!("add i64 {first}, {size}"));
        let past = self.value(format!("icmp ugt i64 {end}, {length}"));
        let last = self.value(format!("select i1 {past}, i64 {length}, i64 {end}"));
        let count = self.value(format!("sub i64 {last}, {first}"));
        let pointer = self.element_pointer(kernel.element, data, &first);
        let view = self.value(format!(
            "insertvalue %tz.array zeroinitializer, ptr {pointer}, 0"
        ));
        let view = self.value(format!("insertvalue %tz.array {view}, i64 {count}, 1"));
        let view_type = Type::Reference(
            Box::new(Type::ArrayView(Box::new(kernel.element.clone()))),
            true,
        );
        self.parallel_apply(
            callback,
            kernel.callback,
            direct,
            &[(Type::I64, first), (view_type, view)],
        );
    }

    /// The element loop of one chunk of `Parallel.init`, `map`, `map_ref`, and `reduce`.
    fn parallel_items(
        &mut self,
        kernel: &Kernel<'_>,
        loaded: &[String],
        callback: &str,
        direct: Option<&BorrowedCall>,
    ) {
        let emitter = self;
        let first = emitter.parallel_bound("%chunk", &loaded[0], &loaded[1]);
        let next = emitter.value("add i64 %chunk, 1");
        let last = emitter.parallel_bound(&next, &loaded[0], &loaded[1]);
        let count = emitter.value(format!("sub i64 {last}, {first}"));
        let reduce = kernel.operation == Builtin::ParallelReduce;
        let accumulator = if reduce {
            let initial = emitter.clone_value(kernel.element, &loaded[4]);
            Some(emitter.spill(kernel.element, &initial))
        } else {
            None
        };
        emitter.array_loop(&count, |emitter, offset| {
            let index = emitter.value(format!("add i64 {first}, {offset}"));
            let (argument_type, argument) = if kernel.operation == Builtin::ParallelInit {
                (Type::I64, index.clone())
            } else {
                let pointer = emitter.value(format!(
                    "getelementptr inbounds {}, ptr {}, i64 {index}",
                    emitter.ty(kernel.input),
                    loaded[2]
                ));
                if kernel.operation == Builtin::ParallelMapRef {
                    let value = if matches!(kernel.input, Type::Array(_)) {
                        emitter.value(format!("load %tz.array, ptr {pointer}"))
                    } else {
                        pointer
                    };
                    (
                        Type::Reference(Box::new(kernel.input.clone()), false),
                        value,
                    )
                } else {
                    let value =
                        emitter.value(format!("load {}, ptr {pointer}", emitter.ty(kernel.input)));
                    (
                        kernel.input.clone(),
                        emitter.clone_value(kernel.input, &value),
                    )
                }
            };
            let mut arguments = Vec::new();
            if let Some(accumulator) = &accumulator {
                let before = emitter.value(format!(
                    "load {}, ptr {accumulator}",
                    emitter.ty(kernel.element)
                ));
                arguments.push((kernel.element.clone(), before));
            }
            arguments.push((argument_type, argument));
            let value = emitter.parallel_apply(callback, kernel.callback, direct, &arguments);
            let pointer = if let Some(accumulator) = &accumulator {
                accumulator.clone()
            } else {
                emitter.value(format!(
                    "getelementptr inbounds {}, ptr {}, i64 {index}",
                    emitter.ty(kernel.element),
                    loaded[3]
                ))
            };
            emitter.instruction(format!(
                "store {} {value}, ptr {pointer}",
                emitter.ty(kernel.element)
            ));
        });
        if let Some(accumulator) = accumulator {
            let partial = emitter.value(format!(
                "load {}, ptr {accumulator}",
                emitter.ty(kernel.element)
            ));
            let pointer = emitter.value(format!(
                "getelementptr inbounds {}, ptr {}, i64 %chunk",
                emitter.ty(kernel.element),
                loaded[3]
            ));
            emitter.instruction(format!(
                "store {} {partial}, ptr {pointer}",
                emitter.ty(kernel.element)
            ));
        }
    }

    pub(super) fn parallel_expression(
        &mut self,
        operation: Builtin,
        arguments: &[TypedExpr],
        result_type: &Type,
    ) -> String {
        if operation == Builtin::ParallelForEachChunk {
            return self.parallel_chunks(arguments);
        }
        let reduce = operation == Builtin::ParallelReduce;
        let initialize = operation == Builtin::ParallelInit;
        let callback_index = usize::from(reduce || initialize);
        let callback_type = &arguments[callback_index].ty;
        let mut direct = None;
        let mut materialized = None;
        let mut values = Vec::new();
        for (index, argument) in arguments.iter().enumerate() {
            if index == callback_index {
                let arity = if reduce { 2 } else { 1 };
                let target =
                    call_specialization::target(argument, &self.known_closures, self.module)
                        .filter(|target| {
                            target.bound + arity
                                == self.module.functions[target.function].parameters.len()
                        });
                if initialize || target.is_some_and(|target| target.bound == 0) {
                    direct = self.prepare_known_call(argument, arity);
                } else {
                    materialized = target;
                }
                values.push(if direct.is_some() {
                    String::new()
                } else {
                    self.expression(argument)
                });
            } else {
                values.push(self.expression(argument));
            }
        }
        if let Some(target) = materialized {
            direct = self.parallel_materialized_call(target, &values[callback_index]);
        }
        for value in &values {
            self.forget_temporary(value);
        }
        let callback = &values[callback_index];
        let (length, input, input_type) = if initialize {
            (values[0].clone(), "null".into(), Type::I64)
        } else {
            let value = values.last().unwrap();
            let length = self.value(format!("extractvalue %tz.array {value}, 1"));
            let input = self.value(format!("extractvalue %tz.array {value}, 0"));
            let Type::Reference(array, _) = &arguments.last().unwrap().ty else {
                unreachable!()
            };
            let Type::Array(element) = array.as_ref() else {
                unreachable!()
            };
            (length, input, element.as_ref().clone())
        };
        let element = if reduce {
            result_type.clone()
        } else {
            let Type::Array(element) = result_type else {
                unreachable!()
            };
            element.as_ref().clone()
        };
        self.allocation_size(&self.ty(&element), &length);
        let chunks = self.parallel_count(&length);
        let result = if reduce {
            let result = self.slot(&element);
            let empty = self.value(format!("icmp eq i64 {chunks}, 0"));
            let no_work = self.label();
            let work = self.label();
            let done = self.label();
            self.branch(&empty, &no_work, &work);
            self.begin(&no_work);
            self.instruction(format!(
                "store {} {}, ptr {result}",
                self.ty(&element),
                values[0]
            ));
            self.jump(&done);
            self.begin(&work);
            let (partials, output) = self.allocate_array(&element, &chunks);
            self.parallel_launch(
                Kernel {
                    operation,
                    input: &input_type,
                    element: &element,
                    callback: callback_type,
                    direct: direct.as_ref(),
                },
                Data {
                    length: &length,
                    chunks: &chunks,
                    input: &input,
                    output: &output,
                    identity: &values[0],
                    callback,
                },
            );
            let initial = self.clone_value(&element, &values[0]);
            self.instruction(format!(
                "store {} {initial}, ptr {result}",
                self.ty(&element)
            ));
            self.array_loop(&chunks, |emitter, chunk| {
                let pointer = emitter.value(format!(
                    "getelementptr inbounds {}, ptr {output}, i64 {chunk}",
                    emitter.ty(&element)
                ));
                let partial =
                    emitter.value(format!("load {}, ptr {pointer}", emitter.ty(&element)));
                emitter.instruction(format!(
                    "store {} zeroinitializer, ptr {pointer}",
                    emitter.ty(&element)
                ));
                let before = emitter.value(format!("load {}, ptr {result}", emitter.ty(&element)));
                let value = emitter.parallel_apply(
                    callback,
                    callback_type,
                    direct.as_ref(),
                    &[(element.clone(), before), (element.clone(), partial)],
                );
                emitter.instruction(format!(
                    "store {} {value}, ptr {result}",
                    emitter.ty(&element)
                ));
            });
            self.drop_value(&Type::Array(Box::new(element.clone())), &partials);
            self.drop_value(&element, &values[0]);
            self.jump(&done);
            self.begin(&done);
            self.value(format!("load {}, ptr {result}", self.ty(&element)))
        } else {
            let (result, output) = self.allocate_array(&element, &length);
            self.parallel_launch(
                Kernel {
                    operation,
                    input: &input_type,
                    element: &element,
                    callback: callback_type,
                    direct: direct.as_ref(),
                },
                Data {
                    length: &length,
                    chunks: &chunks,
                    input: &input,
                    output: &output,
                    identity: "0",
                    callback,
                },
            );
            result
        };
        if let Some(call) = &direct {
            self.finish_borrowed_call(call);
        }
        if !callback.is_empty() {
            self.drop_value(callback_type, callback);
        }
        result
    }

    /// `Parallel.for_each_chunk size callback values`: `ceil(length / size)` chunks of an
    /// exclusive slice, disjoint by construction, each lent to one callback run (C08 Phase 2).
    fn parallel_chunks(&mut self, arguments: &[TypedExpr]) -> String {
        let callback_type = &arguments[1].ty;
        let size = self.expression(&arguments[0]);
        let target = call_specialization::target(&arguments[1], &self.known_closures, self.module)
            .filter(|target| {
                target.bound + 2 == self.module.functions[target.function].parameters.len()
            });
        let mut direct = None;
        let mut materialized = None;
        if target.is_some_and(|target| target.bound == 0) {
            direct = self.prepare_known_call(&arguments[1], 2);
        } else {
            materialized = target;
        }
        let callback = if direct.is_some() {
            String::new()
        } else {
            self.expression(&arguments[1])
        };
        let view = self.expression(&arguments[2]);
        if let Some(target) = materialized {
            direct = self.parallel_materialized_call(target, &callback);
        }
        for value in [&size, &callback, &view] {
            self.forget_temporary(value);
        }
        let positive = self.value(format!("icmp sgt i64 {size}, 0"));
        self.guard(&positive, TrapKind::RangeStepZero);
        let element = arguments[2]
            .ty
            .slice_element()
            .expect("the chunked input is a slice")
            .clone();
        let length = self.value(format!("extractvalue %tz.array {view}, 1"));
        let input = self.value(format!("extractvalue %tz.array {view}, 0"));
        let whole = self.value(format!("udiv i64 {length}, {size}"));
        let remainder = self.value(format!("urem i64 {length}, {size}"));
        let partial = self.value(format!("icmp ne i64 {remainder}, 0"));
        let partial = self.value(format!("zext i1 {partial} to i64"));
        let chunks = self.value(format!("add i64 {whole}, {partial}"));
        self.parallel_launch(
            Kernel {
                operation: Builtin::ParallelForEachChunk,
                input: &element,
                element: &element,
                callback: callback_type,
                direct: direct.as_ref(),
            },
            Data {
                length: &length,
                chunks: &chunks,
                input: &input,
                output: "null",
                identity: &size,
                callback: &callback,
            },
        );
        if let Some(call) = &direct {
            self.finish_borrowed_call(call);
        }
        if !callback.is_empty() {
            self.drop_value(callback_type, &callback);
        }
        "0".into()
    }
}
