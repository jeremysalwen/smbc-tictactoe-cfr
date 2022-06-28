use bincode;
use clap::ArgAction;
use clap::Parser;
use clap::ValueHint;
use std::fs::File;
use std::io::BufReader;
use std::io::BufWriter;
use std::io::Write;
use strum::IntoEnumIterator;

mod lib;
use lib::*;

/// Search for a pattern in a file and display the lines that contain it.
#[derive(Parser)]
struct Cli {
    /// The path to input directory.
    #[clap(short, long, parse(from_os_str), value_hint = ValueHint::DirPath)]
    solutions_dir: std::path::PathBuf,

    #[clap(short, long)]
    iteration: usize,

    #[clap(short, long, action = ArgAction::Set,  default_value_t = true)]
    average_strategy: bool,
}

fn load(args: &mut Cli) -> (CFR, Strategy) {
    args.solutions_dir
        .push(format!("debug_{}.bincode", args.iteration));
    let cfr = bincode::deserialize_from(BufReader::new(
        File::open(&args.solutions_dir).expect("couldn't open file"),
    ))
    .unwrap();
    args.solutions_dir.pop();
    args.solutions_dir
        .push(format!("strategy_{}.bincode", args.iteration));
    let strategy = bincode::deserialize_from(BufReader::new(
        File::open(&args.solutions_dir).expect("couldn't open file"),
    ))
    .unwrap();
    args.solutions_dir.pop();
    return (cfr, strategy);
}
fn main() {
    let mut args = Cli::parse();
    println!("Constructing game tree...");

    let game_tree = GameTree::new();
    println!("{} States in the game tree", game_tree.states.len());
    println!("{} Terminal states", game_tree.terminals.len());

    let (cfr, strategy) = load(&mut args);

    println!("Average strat {}", args.average_strategy);
    let base_strategy = if args.average_strategy {
        &cfr.average_strategy
    } else {
        &strategy
    };
    let counterfactual_probs = base_strategy.counterfactual_probs(&game_tree);
    let best_response = BestResponse::new(
        base_strategy,
        &game_tree,
        &counterfactual_probs,
        &OutcomeValues::default(),
    );

    args.solutions_dir
        .push(format!("best_response_{}.bincode", args.iteration));
    bincode::serialize_into(
        BufWriter::new(File::create(&args.solutions_dir).expect("couldn't open file")),
        &best_response,
    )
    .unwrap();
    args.solutions_dir.pop();

    let p1_exploiter = Strategy::splice(base_strategy, &best_response.strategy, &game_tree);
    let p2_exploiter = Strategy::splice(&best_response.strategy, base_strategy, &game_tree);

    for (s, prob) in &p1_exploiter.probs {
        println!("State {} has probs {:?}:", s.state, prob);
        println!("Goals {:?}", s.goal);
        println!("{:?}", game_tree.states[s.state]);
    }
    for (name, spliced_strat) in [("First", p1_exploiter), ("Second", p2_exploiter)] {
        println!("Exploitability of {} player:", name);
        let expected_values = spliced_strat.expected_values(&game_tree, &OutcomeValues::default());
        let mut avg_return = 0f64;
        for p1goal in Outcome::iter() {
            for p2goal in Outcome::iter() {
                let ret = expected_values[&MetaState {
                    state: 0,
                    p1goal,
                    p2goal,
                }];
                println!(
                    "Exploitability of {} Player with goals {:?} {:?} {}",
                    name, p1goal, p2goal, ret
                );
                avg_return += ret;
            }
        }
        println!(
            "Overall exploitability of {} Player: {}",
            name,
            avg_return / 4.0
        );
    }
}
