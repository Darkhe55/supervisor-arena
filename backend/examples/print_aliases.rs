//! Quick demo: print aliases for a few sample inputs.
//! Run with: cargo run --example print_aliases

use supervisor_arena::supervisor::{AliasGenerator, AliasInput};

fn main() {
    let g = AliasGenerator::new([0x42_u8; 32]);
    let samples = [
        ("张伟", "computer_science", "MIT"),
        ("张伟", "mathematics", "MIT"),
        ("张伟", "computer_science", "Stanford"),
        ("李娜", "literature", "清华"),
        ("Michael Smith", "physics", "Caltech"),
        ("王芳", "economics", "北大"),
        ("Sarah Johnson", "medicine", "Harvard"),
        ("张伟", "history", "复旦"),
        ("张伟", "biology", "浙大"),
        ("Z", "philosophy", "UCLA"),
    ];

    println!(
        "{:<25} {:<22} {:<12} {:<28}  style",
        "submitted_name", "discipline", "college", "alias"
    );
    println!("{}", "-".repeat(96));
    for (n, d, c) in samples {
        let (alias, style) = g
            .generate(AliasInput { submitted_name: n, discipline: d, college: c }, 0)
            .expect("generate");
        println!(
            "{:<25} {:<22} {:<12} {:<28}  {:?}",
            n, d, c, alias, style
        );
    }
    println!(
        "\n(whitelist size = {} entries, retries = 0..32 on collision)",
        g.whitelist_size()
    );
}
