//! `method`: the command line's old name, kept as an alias for
//! `method-budget`. It runs the same code and adds one note on stderr, except
//! for `mcp` and `merge-driver`, which other programs' configs run and where
//! the note would be noise (src/lib.rs).

#![forbid(unsafe_code)]

fn main() -> std::process::ExitCode {
    method_cli::main(method_cli::Invocation::Alias)
}
