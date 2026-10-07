//! Integration tests, compiled as one binary: the end-to-end CLI scenarios and the eval seed
//! scorer, over shared helpers in `support`.

mod annotate;
mod cli;
mod eval_seed;
#[cfg(test)]
mod support;
