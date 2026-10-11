//! Integration tests, compiled as one binary: the end-to-end CLI scenarios, the eval seed
//! scorer and the committed journals, over shared helpers in `support`.

mod annotate;
mod board_guard;
mod cli;
mod eval_seed;
mod journal;
#[cfg(test)]
mod support;
