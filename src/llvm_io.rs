use super::*;

pub(super) fn entry(mut emitter: FunctionEmitter<'_, '_>) -> String {
    let main = emitter.function;
    let Type::Record(_, arguments) = &main.signature.result else {
        unreachable!()
    };
    let payload = &arguments[0];
    let result_type = emitter.ty(&main.signature.result);
    let action = emitter.value(format!(
        "call {result_type} @tz.fn.{}()",
        main.qualified_name()
    ));
    let work = emitter.value(format!("extractvalue {result_type} {action}, 0"));
    let code = emitter.value(format!("extractvalue %tz.closure {work}, 0"));
    let environment = emitter.value(format!("extractvalue %tz.closure {work}, 1"));
    let result = emitter.value(format!(
        "call {} {code}(i8 0, ptr {environment}, i1 false)",
        emitter.ty(payload)
    ));
    if *payload == Type::Integer(32, true) {
        // An `IO<i32>` entry returns its value as the process exit code.
        emitter.instruction(format!("ret i32 {result}"));
    } else {
        emitter.drop_value(payload, &result);
        emitter.instruction("ret i32 0");
    }
    let mut output = String::from("define i32 @tsuzuri_main() {\nentry:\n");
    for line in emitter.allocas {
        let _ = writeln!(output, "  {line}");
    }
    for line in emitter.lines {
        let _ = writeln!(output, "{line}");
    }
    output.push_str("}\n");
    output
}

impl FunctionEmitter<'_, '_> {
    pub(super) fn io_builtin(&mut self, builtin: Builtin, signature: &Type) -> String {
        let (name, parameters) = match builtin {
            Builtin::IOReadLine => ("read_line", "ptr"),
            Builtin::IOWrite => ("write", "i32, ptr, i64"),
            _ => unreachable!(),
        };
        let attributes = if self.globals.wasm {
            format!(" \"wasm-import-module\"=\"tsuzuri_io\" \"wasm-import-name\"=\"{name}\"")
        } else {
            String::new()
        };
        self.intrinsics.insert(format!(
            "declare i32 @tsuzuri_io_{name}({parameters}){attributes}"
        ));
        if builtin == Builtin::IOWrite {
            let text = self.value("load %tz.utf8string, ptr %arg1");
            let pointer = self.value(format!("extractvalue %tz.utf8string {text}, 0"));
            let length = self.value(format!("extractvalue %tz.utf8string {text}, 1"));
            return self.value(format!(
                "call i32 @tsuzuri_io_write(i32 %arg0, ptr {pointer}, i64 {length})"
            ));
        }
        let bytes = Type::Array(Box::new(Type::Integer(8, false)));
        let slot = self.host_result_slot(&bytes);
        let status = self.value(format!("call i32 @tsuzuri_io_read_line(ptr {slot})"));
        let valid = self.value(format!("icmp ult i32 {status}, 3"));
        self.guard(&valid, TrapKind::BoundsCheck);
        let value = self.read_host_result(&bytes, &slot);
        let result_type = self.ty(&signature.after_arguments(1));
        let result = self.value(format!(
            "insertvalue {result_type} zeroinitializer, i32 {status}, 0"
        ));
        self.value(format!(
            "insertvalue {result_type} {result}, %tz.array {value}, 1"
        ))
    }

    /// The operating-system primitives of the standard `File`, `Dir`, `Env`, `Time`, and
    /// `Random` modules (E08). Each declares a C function that `src/runtime/os.c` defines,
    /// when the program reaches it. A status of 0 is success; otherwise it is
    /// `(kind << 32) | code` with `kind` in 1..=7, and any other value traps like `read_line`.
    /// `Os.__open` returns a handle (not negative) or the negated status.
    pub(super) fn os_builtin(&mut self, builtin: Builtin, signature: &Type) -> String {
        let (name, parameters) = match builtin {
            Builtin::OsRead => ("read", "ptr, i32, ptr, i64"),
            Builtin::OsArgs => ("args", "ptr"),
            Builtin::OsWrite => ("write", "i32, ptr, i64, ptr, i64"),
            Builtin::OsRandom => ("random", "ptr, i64"),
            Builtin::OsClock => ("clock", "i32"),
            Builtin::OsSleep => ("sleep", "i64"),
            Builtin::OsOpen => ("open", "i32, ptr, i64"),
            Builtin::OsHandle => ("handle", "ptr, i32, i64, i64, ptr, i64"),
            Builtin::OsClose => ("close", "i64"),
            Builtin::OsSpawn => ("spawn", "ptr, ptr, i64, ptr, i64, ptr, i64"),
            _ => unreachable!(),
        };
        self.intrinsics
            .insert(format!("declare i64 @tsuzuri_os_{name}({parameters})"));
        let owned = matches!(
            builtin,
            Builtin::OsRead
                | Builtin::OsArgs
                | Builtin::OsRandom
                | Builtin::OsHandle
                | Builtin::OsSpawn
        );
        let bytes = Type::Array(Box::new(Type::Integer(8, false)));
        let slot = owned.then(|| self.host_result_slot(&bytes));
        let call = match builtin {
            Builtin::OsRead | Builtin::OsOpen => {
                let text = self.value("load %tz.utf8string, ptr %arg1");
                let pointer = self.value(format!("extractvalue %tz.utf8string {text}, 0"));
                let length = self.value(format!("extractvalue %tz.utf8string {text}, 1"));
                if builtin == Builtin::OsOpen {
                    format!("call i64 @tsuzuri_os_open(i32 %arg0, ptr {pointer}, i64 {length})")
                } else {
                    format!(
                        "call i64 @tsuzuri_os_read(ptr {}, i32 %arg0, ptr {pointer}, i64 {length})",
                        slot.as_ref().unwrap()
                    )
                }
            }
            Builtin::OsArgs => {
                self.intrinsics
                    .insert("declare void @tsuzuri_os_set_args(i32, ptr)".to_owned());
                format!("call i64 @tsuzuri_os_args(ptr {})", slot.as_ref().unwrap())
            }
            Builtin::OsRandom => format!(
                "call i64 @tsuzuri_os_random(ptr {}, i64 %arg0)",
                slot.as_ref().unwrap()
            ),
            Builtin::OsWrite | Builtin::OsHandle => {
                let (array, first) = if builtin == Builtin::OsWrite {
                    ("%arg2", None)
                } else {
                    ("%arg3", slot.as_ref())
                };
                // A shared array is passed as its descriptor, not as a pointer to it.
                let data_pointer = self.value(format!("extractvalue %tz.array {array}, 0"));
                let data_length = self.value(format!("extractvalue %tz.array {array}, 1"));
                if let Some(slot) = first {
                    format!(
                        "call i64 @tsuzuri_os_handle(ptr {slot}, i32 %arg0, i64 %arg1, i64 %arg2, ptr {data_pointer}, i64 {data_length})"
                    )
                } else {
                    let text = self.value("load %tz.utf8string, ptr %arg1");
                    let pointer = self.value(format!("extractvalue %tz.utf8string {text}, 0"));
                    let length = self.value(format!("extractvalue %tz.utf8string {text}, 1"));
                    format!(
                        "call i64 @tsuzuri_os_write(i32 %arg0, ptr {pointer}, i64 {length}, ptr {data_pointer}, i64 {data_length})"
                    )
                }
            }
            Builtin::OsSpawn => {
                let text = self.value("load %tz.utf8string, ptr %arg0");
                let program = self.value(format!("extractvalue %tz.utf8string {text}, 0"));
                let program_length = self.value(format!("extractvalue %tz.utf8string {text}, 1"));
                let arguments = self.value("extractvalue %tz.array %arg1, 0");
                let arguments_length = self.value("extractvalue %tz.array %arg1, 1");
                let input = self.value("extractvalue %tz.array %arg2, 0");
                let input_length = self.value("extractvalue %tz.array %arg2, 1");
                format!(
                    "call i64 @tsuzuri_os_spawn(ptr {}, ptr {program}, i64 {program_length}, ptr {arguments}, i64 {arguments_length}, ptr {input}, i64 {input_length})",
                    slot.as_ref().unwrap()
                )
            }
            Builtin::OsClock => "call i64 @tsuzuri_os_clock(i32 %arg0)".to_owned(),
            Builtin::OsClose => "call i64 @tsuzuri_os_close(i64 %arg0)".to_owned(),
            _ => "call i64 @tsuzuri_os_sleep(i64 %arg0)".to_owned(),
        };
        let result = self.value(call);
        if builtin == Builtin::OsClock {
            return result;
        }
        // A handle is not an error status; only the negated status of a failure is checked.
        let status = if builtin == Builtin::OsOpen {
            let failed = self.value(format!("icmp slt i64 {result}, 0"));
            let negated = self.value(format!("sub i64 0, {result}"));
            self.value(format!("select i1 {failed}, i64 {negated}, i64 0"))
        } else {
            result.clone()
        };
        let succeeded = self.value(format!("icmp eq i64 {status}, 0"));
        let kind = self.value(format!("lshr i64 {status}, 32"));
        let offset = self.value(format!("sub i64 {kind}, 1"));
        let known = self.value(format!("icmp ult i64 {offset}, 7"));
        let valid = self.value(format!("or i1 {succeeded}, {known}"));
        self.guard(&valid, TrapKind::BoundsCheck);
        let Some(slot) = slot else {
            return result;
        };
        let value = self.read_host_result(&bytes, &slot);
        let arguments = match builtin {
            Builtin::OsRead => 2,
            Builtin::OsHandle => 4,
            Builtin::OsSpawn => 3,
            _ => 1,
        };
        let result_type = self.ty(&signature.after_arguments(arguments));
        let tuple = self.value(format!(
            "insertvalue {result_type} zeroinitializer, i64 {result}, 0"
        ));
        self.value(format!(
            "insertvalue {result_type} {tuple}, %tz.array {value}, 1"
        ))
    }

    /// The network primitives of the standard `Net` module (E09). Each declares a C function that
    /// `src/runtime/net.c` defines when the program reaches it. A status of 0 is success; otherwise it
    /// is `(kind << 32) | code` with `kind` in 1..=7, and any other value traps like `os_builtin`.
    /// `Net.__open`, `Net.__accept`, and `Net.__send` return a count or handle (not negative) or the
    /// negated status, and `Net.__classify` an index below 7.
    pub(super) fn net_builtin(&mut self, builtin: Builtin, signature: &Type) -> String {
        if matches!(
            builtin,
            Builtin::NetWatch | Builtin::NetUnwatch | Builtin::NetConnect
        ) {
            return self.net_async_builtin(builtin);
        }
        let (name, parameters) = match builtin {
            Builtin::NetResolve => ("resolve", "ptr, ptr, i64, i64"),
            Builtin::NetOpen => ("open", "ptr, i32, i64, i64, i64, i64"),
            Builtin::NetAccept => ("accept", "ptr, i64, i64"),
            Builtin::NetRead => ("read", "ptr, i32, i64, i64, i64"),
            Builtin::NetWrite => ("write", "i32, i64, ptr, i64, i64, i64, i64, i64"),
            Builtin::NetClose => ("close", "i32, i64"),
            Builtin::NetClassify => ("classify", "i32"),
            Builtin::NetNames => ("names", "ptr, i64"),
            Builtin::NetSend => ("send", "i32, i64, ptr, i64, i64, i64, i64, i64"),
            _ => unreachable!(),
        };
        self.intrinsics
            .insert(format!("declare i64 @tsuzuri_net_{name}({parameters})"));
        let owned = matches!(
            builtin,
            Builtin::NetResolve
                | Builtin::NetOpen
                | Builtin::NetAccept
                | Builtin::NetRead
                | Builtin::NetNames
        );
        let bytes = Type::Array(Box::new(Type::Integer(8, false)));
        let slot = owned.then(|| self.host_result_slot(&bytes));
        let call = match builtin {
            Builtin::NetResolve => {
                let text = self.value("load %tz.utf8string, ptr %arg0");
                let pointer = self.value(format!("extractvalue %tz.utf8string {text}, 0"));
                let length = self.value(format!("extractvalue %tz.utf8string {text}, 1"));
                format!(
                    "call i64 @tsuzuri_net_resolve(ptr {}, ptr {pointer}, i64 {length}, i64 %arg1)",
                    slot.as_ref().unwrap()
                )
            }
            Builtin::NetOpen => format!(
                "call i64 @tsuzuri_net_open(ptr {}, i32 %arg0, i64 %arg1, i64 %arg2, i64 %arg3, i64 %arg4)",
                slot.as_ref().unwrap()
            ),
            Builtin::NetAccept => format!(
                "call i64 @tsuzuri_net_accept(ptr {}, i64 %arg0, i64 %arg1)",
                slot.as_ref().unwrap()
            ),
            Builtin::NetRead => format!(
                "call i64 @tsuzuri_net_read(ptr {}, i32 %arg0, i64 %arg1, i64 %arg2, i64 %arg3)",
                slot.as_ref().unwrap()
            ),
            Builtin::NetNames => format!(
                "call i64 @tsuzuri_net_names(ptr {}, i64 %arg0)",
                slot.as_ref().unwrap()
            ),
            Builtin::NetWrite | Builtin::NetSend => {
                // A shared array is passed as its descriptor, not as a pointer to it.
                let data = self.value("extractvalue %tz.array %arg2, 0");
                let length = self.value("extractvalue %tz.array %arg2, 1");
                format!(
                    "call i64 @tsuzuri_net_{name}(i32 %arg0, i64 %arg1, ptr {data}, i64 {length}, i64 %arg3, i64 %arg4, i64 %arg5, i64 %arg6)"
                )
            }
            Builtin::NetClose => "call i64 @tsuzuri_net_close(i32 %arg0, i64 %arg1)".to_owned(),
            _ => "call i64 @tsuzuri_net_classify(i32 %arg0)".to_owned(),
        };
        let result = self.value(call);
        if builtin == Builtin::NetClassify {
            let valid = self.value(format!("icmp ult i64 {result}, 7"));
            self.guard(&valid, TrapKind::BoundsCheck);
            return result;
        }
        // A handle or a count is not an error status; only the negated status of a failure is checked.
        let status = if matches!(
            builtin,
            Builtin::NetOpen | Builtin::NetAccept | Builtin::NetSend
        ) {
            let failed = self.value(format!("icmp slt i64 {result}, 0"));
            let negated = self.value(format!("sub i64 0, {result}"));
            self.value(format!("select i1 {failed}, i64 {negated}, i64 0"))
        } else {
            result.clone()
        };
        let succeeded = self.value(format!("icmp eq i64 {status}, 0"));
        let kind = self.value(format!("lshr i64 {status}, 32"));
        let offset = self.value(format!("sub i64 {kind}, 1"));
        let known = self.value(format!("icmp ult i64 {offset}, 7"));
        let valid = self.value(format!("or i1 {succeeded}, {known}"));
        self.guard(&valid, TrapKind::BoundsCheck);
        let Some(slot) = slot else {
            return result;
        };
        let value = self.read_host_result(&bytes, &slot);
        let arguments = match builtin {
            Builtin::NetResolve | Builtin::NetAccept => 2,
            Builtin::NetNames => 1,
            Builtin::NetOpen => 5,
            _ => 4,
        };
        let result_type = self.ty(&signature.after_arguments(arguments));
        let tuple = self.value(format!(
            "insertvalue {result_type} zeroinitializer, i64 {result}, 0"
        ));
        self.value(format!(
            "insertvalue {result_type} {tuple}, %tz.array {value}, 1"
        ))
    }

    /// The async operations of `Net` (E09 Phase 2): a call that starts, or takes back, an operation of
    /// `Async.host`, and returns unit. Natively the runtime completes the operation with the `tsuzuri_async_post`
    /// that is passed in, so `net.c` itself needs no reactor; the driver rejects a program that starts one
    /// without `Async.block_on`. On WebAssembly (`--wasm-feature net`) the host completes it with
    /// `tsuzuri_async_complete`, so no function is passed.
    fn net_async_builtin(&mut self, builtin: Builtin) -> String {
        let wasm = self.globals.wasm;
        let (declaration, call) = match (builtin, wasm) {
            (Builtin::NetWatch, false) => (
                "declare void @tsuzuri_net_watch(ptr, i64, i64, i32, i64)",
                "call void @tsuzuri_net_watch(ptr @tsuzuri_async_post, i64 %arg0, i64 %arg1, i32 %arg2, i64 %arg3)",
            ),
            (Builtin::NetWatch, true) => (
                "declare void @tsuzuri_net_watch(i64, i64, i32, i64)",
                "call void @tsuzuri_net_watch(i64 %arg0, i64 %arg1, i32 %arg2, i64 %arg3)",
            ),
            (Builtin::NetConnect, false) => (
                "declare void @tsuzuri_net_connect(ptr, i64, i64, i64, i64, i64)",
                "call void @tsuzuri_net_connect(ptr @tsuzuri_async_post, i64 %arg0, i64 %arg1, i64 %arg2, i64 %arg3, i64 %arg4)",
            ),
            (Builtin::NetConnect, true) => (
                "declare void @tsuzuri_net_connect(i64, i64, i64, i64, i64)",
                "call void @tsuzuri_net_connect(i64 %arg0, i64 %arg1, i64 %arg2, i64 %arg3, i64 %arg4)",
            ),
            _ => (
                "declare void @tsuzuri_net_unwatch(i64)",
                "call void @tsuzuri_net_unwatch(i64 %arg0)",
            ),
        };
        self.intrinsics.insert(declaration.to_owned());
        if builtin != Builtin::NetUnwatch && !wasm {
            self.intrinsics
                .insert("declare i32 @tsuzuri_async_post(i64, i64)".to_owned());
        }
        self.instruction(call);
        "0".to_owned()
    }
}
