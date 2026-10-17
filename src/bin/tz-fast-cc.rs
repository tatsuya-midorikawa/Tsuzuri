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
    let mut arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.first().is_some_and(|flag| flag == "--read") {
        return read_only(&arguments[1..]);
    }
    // `--target TRIPLE` translates for another machine, to look at its code without running it.
    let mut platform = tsuzuri::target::Platform::HOST;
    if let Some(position) = arguments.iter().position(|argument| argument == "--target") {
        let Some(name) = arguments.get(position + 1).cloned() else {
            eprintln!("usage: tz-fast-cc IN.ll -o OUT.o [--target TRIPLE]");
            return ExitCode::from(2);
        };
        match tsuzuri::target::parse_target(&name) {
            Ok((tsuzuri::driver::Target::Native, triple)) => {
                platform = tsuzuri::target::Platform::new(triple);
            }
            _ => {
                eprintln!("tz-fast-cc: --target takes a native target");
                return ExitCode::from(2);
            }
        }
        arguments.drain(position..position + 2);
    }
    let (input, output) = match arguments.as_slice() {
        [input, flag, output] if flag == "-o" => (input, output),
        _ => {
            eprintln!("usage: tz-fast-cc IN.ll -o OUT.o [--target TRIPLE]");
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
    let threads = std::env::var("TSUZURI_THREADS")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map_or(1, usize::from)
                .min(8)
        });
    let options = tsuzuri::backend::Options {
        platform,
        ..tsuzuri::backend::Options::host(threads)
    };
    match tsuzuri::backend::compile(&text, &options) {
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
