//! `tz-fast-cc IN.ll -o OUT.o`: translates one textual LLVM IR file with the fast backend. It exists for
//! `tests/fast_backend.mjs`, whose wrapper stands in for `clang` on `.ll` inputs; it is built only with
//! `--features fast-backend`.
//!
//! `tz-fast-cc --read IN.ll...` only reads the files, for finding what the translator does not know.

use std::process::ExitCode;

fn read_only(files: &[String]) -> ExitCode {
    let mut failed = false;
    for file in files {
        let text = match std::fs::read_to_string(file) {
            Ok(text) => text,
            Err(error) => {
                eprintln!("tz-fast-cc: cannot read {file}: {error}");
                return ExitCode::from(2);
            }
        };
        match tsuzuri::backend::read_module(&text) {
            Ok(stats) => println!("{file}: ok {stats:?}"),
            Err(error) => {
                println!("{file}: {}: {}", error.code, error.message);
                failed = true;
            }
        }
    }
    if failed {
        ExitCode::from(3)
    } else {
        ExitCode::SUCCESS
    }
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.first().is_some_and(|flag| flag == "--read") {
        return read_only(&arguments[1..]);
    }
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
