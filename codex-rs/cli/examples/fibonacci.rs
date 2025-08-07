//! Print the Fibonacci sequence.
//!
//! Usage:
//!   cargo run -p codex-cli --example fibonacci -- [COUNT]
//!
//! If COUNT is omitted, the first 10 numbers are printed.

use std::env;
use std::process;

fn fibonacci(count: usize) -> Vec<u128> {
    let mut seq = Vec::with_capacity(count);
    if count == 0 {
        return seq;
    }
    // Start with 0, 1
    let mut a: u128 = 0;
    let mut b: u128 = 1;
    for _ in 0..count {
        seq.push(a);
        let next = a.saturating_add(b);
        a = b;
        b = next;
    }
    seq
}

fn parse_count_arg() -> Result<usize, String> {
    let mut args = env::args().skip(1);
    match args.next() {
        None => Ok(10), // default
        Some(s) => s
            .parse::<usize>()
            .map_err(|_| format!("Invalid COUNT: '{}' (expected a non-negative integer)", s)),
    }
}

fn main() {
    let count = match parse_count_arg() {
        Ok(n) => n,
        Err(e) => {
            eprintln!(
                "{}\nUsage: cargo run -p codex-cli --example fibonacci -- [COUNT]",
                e
            );
            process::exit(2);
        }
    };

    for n in fibonacci(count) {
        println!("{}", n);
    }
}
