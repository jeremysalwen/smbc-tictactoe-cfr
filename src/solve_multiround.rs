use strum::IntoEnumIterator;

use bincode;
use clap::ArgAction;
use clap::Parser;
use clap::ValueHint;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;

mod lib;
use lib::*;

/// Search for a pattern in a file and display the lines that contain it.
#[derive(Parser)]
struct Cli {
    /// The score which wins the game
    #[clap(long, default_value_t = 5)]
    winning_score: i8,

    /// The maximum total exploitability to solve for
    #[clap(long, default_value_t = 0.000001)]
    maximum_subgame_exploitability: f64,
    #[clap(long, default_value_t = 10)]
    check_exploitability_every: i32,

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

    #[clap(short,long, action = ArgAction::Set,  default_value_t = true)]
    alternate_updates: bool,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
struct Subgame {
    p1score: i8,
    p2score: i8,
}

fn exploitability_bound(game_tree: &GameTree, cfr: &CFR, outcome_values: &OutcomeValues) -> f64 {
    let counterfactual_probs = cfr.average_strategy.counterfactual_probs(&game_tree);
    let best_response = BestResponse::new(
        &cfr.average_strategy,
        &game_tree,
        &counterfactual_probs,
        &outcome_values,
    );
    let p1_exploiter = Strategy::splice(&cfr.average_strategy, &best_response.strategy, &game_tree);
    let p2_exploiter = Strategy::splice(&best_response.strategy, &cfr.average_strategy, &game_tree);
    let mut returns = Vec::new();
    for spliced_strat in [p1_exploiter, p2_exploiter] {
        let expected_values = spliced_strat.expected_values(&game_tree, &outcome_values);
        let mut avg_return = 0f64;
        for p1goal in Outcome::iter() {
            for p2goal in Outcome::iter() {
                avg_return += expected_values[&MetaState {
                    state: 0,
                    p1goal,
                    p2goal,
                }];
            }
        }
        returns.push(avg_return / 9.0);
    }
    return f64::abs(returns[1] - returns[0]);
}

fn main() {
    let mut args = Cli::parse();
    std::fs::create_dir_all(&args.output_dir).unwrap();
    println!("Constructing game tree...");

    let game_tree = GameTree::new();
    println!("{} States in the game tree", game_tree.states.len());
    println!("{} Terminal states", game_tree.terminals.len());

    let discounting = if args.discount {
        Some(CFRDiscounting {
            alpha: args.discount_alpha,
            beta: args.discount_beta,
            gamma: args.discount_gamma,
        })
    } else {
        None
    };

    let mut solutions = HashMap::<Subgame, CFR>::new();
    let mut strategies = HashMap::<Subgame, Strategy>::new();
    let mut evs = HashMap::<Subgame, f64>::new();

    for larger_score in (0..args.winning_score).rev() {
        for smaller_score in (0..=larger_score).rev() {
            for i in 0..i32::MAX {
                let mut converged =
                    i % args.check_exploitability_every == args.check_exploitability_every - 1;
                for (p1score, p2score) in
                    [(larger_score, smaller_score), (smaller_score, larger_score)]
                {
                    let subgame = Subgame { p1score, p2score };
                    let solution = solutions
                        .entry(subgame.clone())
                        .or_insert_with(|| CFR::new(discounting.clone(), args.alternate_updates));
                    let strategy = strategies
                        .entry(subgame.clone())
                        .or_insert_with(|| Strategy::uniform(&game_tree).clone());

                    let value_of_score = |p1score, p2score| match (
                        p1score >= args.winning_score,
                        p2score >= args.winning_score,
                    ) {
                        (true, true) => -*evs
                            .get(&Subgame {
                                p1score: args.winning_score - 1,
                                p2score: args.winning_score - 1,
                            })
                            .unwrap_or(&0.0),
                        (true, false) => 1.0,
                        (false, true) => -1.0,
                        (false, false) => -*evs
                            .get(&Subgame {
                                p1score: p2score,
                                p2score: p1score,
                            })
                            .unwrap_or(&0.0),
                    };
                    let outcome_values = OutcomeValues {
                        both_win: value_of_score(p1score + 1, p2score + 1),
                        p1_win: value_of_score(p1score + 1, p2score),
                        p2_win: value_of_score(p1score, p2score + 1),
                        both_lose: value_of_score(p1score, p2score),
                        first_move_epsilon: args.small_move_epsilon
                            * (1.0 - args.small_move_epsilon_decay).powf(i as f64),
                    };
                    println!("Outcome Values are: {:?} {:?},", outcome_values, evs);

                    println!(
                        "Computing CFR iteration {} for subgame ({}, {})...",
                        i, p1score, p2score
                    );
                    let new_strategy = solution.cfr_round(&strategy, &game_tree, &outcome_values);

                    println!(
                        "Max prob difference from old strategy {}",
                        new_strategy.max_difference(&strategy)
                    );
                    *strategy = new_strategy;

                    if i % args.check_exploitability_every == args.check_exploitability_every - 1 {
                        let exploitability =
                            exploitability_bound(&game_tree, &solution, &outcome_values);
                        println!("Exploitability is {}", exploitability);

                        if exploitability > args.maximum_subgame_exploitability {
                            converged = false;
                        }

                        // Compute expected values based on average strategy instead of latest.
                        let expected_values = solution
                            .average_strategy
                            .expected_values(&game_tree, &outcome_values);
                        let mut avg_return = 0f64;
                        for p1goal in Outcome::iter() {
                            for p2goal in Outcome::iter() {
                                avg_return += expected_values[&MetaState {
                                    state: 0,
                                    p1goal,
                                    p2goal,
                                }];
                            }
                        }
                        evs.insert(subgame, avg_return / 9.0);
                    }
                }
                if converged {
                    for (p1score, p2score) in
                        [(larger_score, smaller_score), (smaller_score, larger_score)]
                    {
                        let subgame = Subgame { p1score, p2score };
                        let solution = &solutions[&subgame.clone()];
                        let strategy = &strategies[&subgame.clone()];

                        println!(
                            "Subgame ({}, {}) converged, saving it to file...",
                            p1score, p2score
                        );
                        args.output_dir
                            .push(format!("subgame_{}_{}", p1score, p2score));
                        std::fs::create_dir_all(&args.output_dir).unwrap();
                        args.output_dir.push(format!("debug_{}.bincode", i));
                        let json_file = BufWriter::new(
                            File::create(&args.output_dir).expect("couldn't create file"),
                        );
                        args.output_dir.pop();
                        bincode::serialize_into(json_file, &solution).expect("could not serialize");

                        args.output_dir.push(format!("strategy_{}.bincode", i));
                        let json_file = BufWriter::new(
                            File::create(&args.output_dir).expect("couldn't create file"),
                        );
                        args.output_dir.pop();
                        args.output_dir.pop();
                        bincode::serialize_into(json_file, &strategy).expect("could not serialize");
                    }

                    break;
                }
            }
        }
    }

    println!("Finished solving!");

    println!("EV's for the subgames:");
    println!("{:#?}", &evs);

    println!("EVs for the overall game:");
    let first_round_cfr = &solutions[&Subgame {
        p1score: 0,
        p2score: 0,
    }];
    let expected_values = &first_round_cfr.expected_value;
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
