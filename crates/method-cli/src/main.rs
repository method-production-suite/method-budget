//! `method-budget`: the Method Budget command line (src/lib.rs, docs/CLI.md).

#![forbid(unsafe_code)]

fn main() -> std::process::ExitCode {
    method_cli::main(method_cli::Invocation::Current)
}
