//! Spike stub for #21: reports the argv and stdin it was started with, as one JSON-ish line.
use std::io::{Read as _, Write as _};

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let mut out = std::io::stdout().lock();
    writeln!(out, "STUB args={args:?} stdin={:?}", input.trim())
}
