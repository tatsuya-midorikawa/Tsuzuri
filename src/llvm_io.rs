use super::*;

pub(super) fn entry(module: &CheckedModule) -> String {
    let main = &module.functions[module.entry.unwrap()];
    let result = llvm_type(&main.signature.result, module);
    format!(
        "define i32 @tsuzuri_main() {{\nentry:\n  %action = call {result} @tz.fn.{}()\n  %work = extractvalue {result} %action, 0\n  %code = extractvalue %tz.closure %work, 0\n  %environment = extractvalue %tz.closure %work, 1\n  %result = call i8 %code(i8 0, ptr %environment, i1 false)\n  ret i32 0\n}}\n",
        main.qualified_name()
    )
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
}
