use strum::IntoEnumIterator;

use bincode;
use clap::ArgAction;
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
    #[clap(long, default_value_t = 10)]
    iterations: usize,

    #[clap(long, action = ArgAction::Set,  default_value_t = false)]
    only_save_last: bool,

    /// The path to the output directory
    #[clap(short, long, parse(from_os_str), value_hint = ValueHint::DirPath)]
    output_dir: std::path::PathBuf,

    // Epsilon reward for playing "small" moves to encourage regularization.
    #[clap(short, long, default_value_t = 0.0)]
    small_move_epsilon: f64,
    #[clap(short, long, default_value_t = 0.0)]
    small_move_epsilon_decay: f64,

    #[clap(long, action = ArgAction::Set,  default_value_t = true)]
    discount: bool,

    #[clap(long, default_value_t = 1.5)]
    discount_alpha: f64,
    #[clap(long, default_value_t = 0.0)]
    discount_beta: f64,
    #[clap(long, default_value_t = 2.0)]
    discount_gamma: f64,
}

fn main() {
    let mut args = Cli::parse();
    println!("Constructing game tree...");

    let game_tree = GameTree::new();
    println!("{} States in the game tree", game_tree.states.len());
    println!("{} Terminal states", game_tree.terminals.len());

    let mut outcome_values = OutcomeValues {
        both_win: 0f64,
        p1_win: 1f64,
        p2_win: -1f64,
        both_lose: 0f64,
        first_move_epsilon: args.small_move_epsilon,
    };
    let uniform = Strategy::uniform(&game_tree);

    let discounting = if args.discount {
        Some(CFRDiscounting {
            alpha: args.discount_alpha,
            beta: args.discount_beta,
            gamma: args.discount_gamma,
        })
    } else {
        None
    };
    let mut cfr = CFR::new(discounting);
    let mut strategy = uniform.clone();
    for i in 0..args.iterations {
        println!("Computing CFR iteration {}...", i);
        let new_strategy = cfr.cfr_round(&strategy, &game_tree, &outcome_values);

        println!(
            "Max prob difference from old strategy {}",
            new_strategy.max_difference(&strategy)
        );

        if !args.only_save_last || i == args.iterations - 1 {
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
        }
        strategy = new_strategy;

        outcome_values.first_move_epsilon *= 1.0-args.small_move_epsilon_decay;
        println!("Small move regularization epsilon is {}", outcome_values.first_move_epsilon);
    }
    println!("Finished solving!");
    // for (s, prob) in &cfr.average_strategy.probs {
    //     println!("State has probs {:?}:", prob);
    //     println!("Goals {:?}", s.goal);
    //     println!("{:?}", game_tree.states[s.state]);
    // }
    let expected_values = cfr
        .average_strategy
        .expected_values(&game_tree, &outcome_values);
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
    println!("Overall expected value {}", avg_return / 9.0);
}
