use std::env;
use std::process::exit;

use btclib::types::Block;
use btclib::util::Savable;

fn main() {
    let (path, steps) = if let (Some(arg), Some(arg2)) =
        (env::args().nth(1), env::args().nth(2))
    {
        (arg, arg2)
    } else {
        eprintln!("Usage: miner <block_file> <steps>");
        exit(1);
    };

    let steps: usize = if let Ok(s @ 1..=usize::MAX) = steps.parse() {
        s
    } else {
        eprintln!("<steps> should be a positive integer");
        exit(1);
    };

    let og_block = Block::load_from_file(path).expect("failed to load block");
    let mut block = og_block.clone();

    while !block.header.mine(steps) {
        println!("mining...");
    }

    // print original block and its hash
    println!("original: {:#?}", og_block);
    println!("hash: {}", og_block.header.hash());
    // print mined block and its hash
    println!("final: {:#?}", block);
    println!("hash: {}", block.header.hash());
}
