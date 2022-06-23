use strum::IntoEnumIterator;

use bincode;
use clap::Parser;
use clap::ValueHint;
use std::fs::File;
use std::io::BufWriter;

mod lib;
use lib::*;

/// Search for a pattern in a file and display the lines that contain it.
#[derive(Parser)]
struct Cli {
    /// The number of iterations of CFR to run.
    #[clap(short, long, default_value_t = 10)]
    iterations: usize,
    /// The path to the output directory
    #[clap(short, long, parse(from_os_str), value_hint = ValueHint::DirPath)]
    output_dir: std::path::PathBuf,

}

fn main() {
    let mut args = Cli::parse();
    println!("Constructing game tree...");

    let game_tree = GameTree::new();
    println!("{} States in the game tree", game_tree.states.len());
    println!("{} Terminal states", game_tree.terminals.len());

    let uniform = Strategy::uniform(&game_tree);

    let mut cfr = CFR::new();
    let mut strategy = uniform.clone();
    for i in 0..args.iterations {
        println!("Computing CFR iteration {}...", i);
        let new_strategy = cfr.cfr_round(&strategy, &game_tree);


        println!("Saving iteration to file...");
        args.output_dir.push(format!("debug_{}.bincode", i));
        let json_file =
            BufWriter::new(File::create(&args.output_dir).expect("couldn't create file"));
        args.output_dir.pop();
        bincode::serialize_into(json_file, &cfr).expect("could not serialize");

        args.output_dir.push(format!("strategy_{}.bincode", i));
        let json_file =
            BufWriter::new(File::create(&args.output_dir).expect("couldn't create file"));
        args.output_dir.pop();
        bincode::serialize_into(json_file, &strategy).expect("could not serialize");

        strategy = new_strategy;
    }
    println!("Finished solving!");
    for (s, prob) in &cfr.average_strategy.probs {
        println!("State has probs {:?}:", prob);
        println!("Goals {:?}", s.goal);
        println!("{:?}", game_tree.states[s.state]);
    }
    let expected_values = cfr
        .average_strategy
        .expected_values(&game_tree, OutcomeValues::default());
    let mut avg_return = 0f64;
    for p1goal in Outcome::iter() {
        for p2goal in Outcome::iter() {
            let ret = expected_values[&MetaState {
                state: 0,
                p1goal,
                p2goal,
            }];
            println!(
                "EV for first player with goals {:?} {:?} {}",
                p1goal, p2goal, ret
            );
            avg_return += ret;
        }
    }
    println!("Overall expected value {}", avg_return / 4.0);
}
