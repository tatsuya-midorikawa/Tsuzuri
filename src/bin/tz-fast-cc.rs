//! `tz-fast-cc IN.ll -o OUT.o`: translates one textual LLVM IR file with the fast backend. It exists for
//! `tests/fast_backend.mjs`, whose wrapper stands in for `clang` on `.ll` inputs; it is built only with
//! `--features fast-backend`.

use std::process::ExitCode;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let (input, output) = match arguments.as_slice() {
        [input, flag, output] if flag == "-o" => (input, output),
        _ => {
            eprintln!("usage: tz-fast-cc IN.ll -o OUT.o");
            return ExitCode::from(2);
        }
    };
    let text = match std::fs::read_to_string(input) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("tz-fast-cc: cannot read {input}: {error}");
            return ExitCode::from(2);
        }
    };
    match tsuzuri::backend::compile(&text) {
        Ok(object) => match std::fs::write(output, object) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("tz-fast-cc: cannot write {output}: {error}");
                ExitCode::from(2)
            }
        },
        Err(error) => {
            eprintln!("tz-fast-cc: {}: {}", error.code, error.message);
            ExitCode::from(3)
        }
    }
}
