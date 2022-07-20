use bincode;
use clap::ArgAction;
use clap::Parser;
use clap::ValueHint;
use lazy_static::lazy_static;
use rand::thread_rng;
use rand::Rng;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::hash::Hash;
use std::io::BufReader;
use std::io::Write;
use strum::IntoEnumIterator;

use regex::Regex;

mod lib;
use lib::*;

/// Search for a pattern in a file and display the lines that contain it.
#[derive(Parser)]
struct Cli {
    /// The path to input directory.
    #[clap(short, long, parse(from_os_str), value_hint = ValueHint::DirPath)]
    solutions_dir: std::path::PathBuf,

    #[clap(short, long, default_value_t = 5)]
    winning_score: i8,

    #[clap(short, long, action = ArgAction::Set,  default_value_t = true)]
    average_strategy: bool,
}

fn load(args: &mut Cli, p1score: i8, p2score: i8) -> (CFR, Strategy) {
    args.solutions_dir
        .push(format!("subgame_{}_{}", p1score, p2score));

    lazy_static! {
        static ref RE: Regex = Regex::new(r"strategy_([[:digit:]]+)\.bincode$").unwrap();
    }
    let max_iteration = fs::read_dir(&args.solutions_dir)
        .unwrap()
        .filter_map(|path| {
            let filename = path
                .unwrap()
                .path()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned();
            RE.captures(&filename)
                .map(|c| c.get(1).unwrap().as_str().parse::<usize>().unwrap())
        })
        .max()
        .unwrap();
    println!(
        "Loading iteration {} for subgame {} {}",
        max_iteration, p1score, p2score
    );

    args.solutions_dir
        .push(format!("debug_{}.bincode", max_iteration));
    let cfr = bincode::deserialize_from(BufReader::new(
        File::open(&args.solutions_dir).expect("couldn't open file"),
    ))
    .unwrap();
    args.solutions_dir.pop();
    args.solutions_dir
        .push(format!("strategy_{}.bincode", max_iteration));
    let strategy = bincode::deserialize_from(BufReader::new(
        File::open(&args.solutions_dir).expect("couldn't open file"),
    ))
    .unwrap();
    args.solutions_dir.pop();
    args.solutions_dir.pop();
    return (cfr, strategy);
}

fn outcome_probs(
    visit_probs: &HashMap<MetaState, f64>,
    tree: &GameTree,
) -> HashMap<(bool, bool), f64> {
    let mut result = HashMap::new();
    for (&state, &outcome) in tree.terminals.iter() {
        for p1goal in Outcome::iter() {
            for p2goal in Outcome::iter() {
                let metastate = MetaState {
                    state,
                    p1goal,
                    p2goal,
                };
                let visit_prob = visit_probs[&metastate];
                let (p1scored, p2scored) = (outcome == p1goal, outcome.reverse() == p2goal);
                *result.entry((p1scored, p2scored)).or_insert(0.0) += visit_prob;
            }
        }
    }
    return result;
}

fn print_exploit(strategy: &Strategy, game_tree: &GameTree, outcome_values: &OutcomeValues) {
    let expected_values = strategy.expected_values(&game_tree, &outcome_values);
    let mut avg_return = 0f64;
    for p1goal in Outcome::iter() {
        for p2goal in Outcome::iter() {
            let ret = expected_values[&MetaState {
                state: 0,
                p1goal,
                p2goal,
            }];
            println!(
                "Exploitability with goals {:?} {:?} {}",
                p1goal, p2goal, ret
            );
            avg_return += ret;
        }
    }
    println!("Overall exploitability : {}", avg_return / 9.0);
}

fn strategy_eq(strat1: &Strategy, strat2: &Strategy) -> bool {
    for (k, probs1) in strat1.probs.iter() {
        let probs2 = &strat2.probs[k];
        for i in 0..probs1.len() {
            if probs1[i] != probs2[i] {
                println!("Changed {} {}", probs1[i], probs2[i]);
                return false;
            }
        }
    }
    true
}

fn main() {
    let mut args = Cli::parse();
    println!("Constructing game tree...");

    let game_tree = GameTree::new();
    println!("{} States in the game tree", game_tree.states.len());
    println!("{} Terminal states", game_tree.terminals.len());

    let mut solutions = HashMap::<Subgame, CFR>::new();
    let mut strategies = HashMap::<Subgame, Strategy>::new();
    let mut max_ev = HashMap::<Subgame, f64>::new();
    let mut min_ev = HashMap::<Subgame, f64>::new();

    let winning_score = args.winning_score;

    let value_of_score = |evs: &HashMap<Subgame, f64>, p1score, p2score| match (
        p1score >= winning_score,
        p2score >= winning_score,
    ) {
        (true, true) => -*evs
            .get(&Subgame {
                p1score: winning_score - 1,
                p2score: winning_score - 1,
            })
            .unwrap(),
        (true, false) => 1.0,
        (false, true) => -1.0,
        (false, false) => -*evs
            .get(&Subgame {
                p1score: p2score,
                p2score: p1score,
            })
            .unwrap(),
    };

    for larger_score in (0..args.winning_score.clone()).rev() {
        for smaller_score in (0..=larger_score).rev() {
            for (p1score, p2score) in [(larger_score, smaller_score), (smaller_score, larger_score)]
            {
                let subgame = Subgame {
                    p1score: p1score as i8,
                    p2score: p2score as i8,
                };
                let (cfr, strategy) = load(&mut args, p1score, p2score);
                solutions.insert(subgame.clone(), cfr);
                strategies.insert(subgame.clone(), strategy);

                //Initialize with bad results so we will always improve.
                max_ev.insert(subgame.clone(), -1.0);
                min_ev.insert(subgame.clone(), 1.0);
            }
            loop {
                let mut changed = false;

                let mut p1_best_outcomes = [
                    HashMap::<(bool, bool), f64>::new(),
                    HashMap::<(bool, bool), f64>::new(),
                ];
                let mut p2_best_outcomes = [
                    HashMap::<(bool, bool), f64>::new(),
                    HashMap::<(bool, bool), f64>::new(),
                ];
                for (i, p1score, p2score) in [
                    (0, larger_score, smaller_score),
                    (1, smaller_score, larger_score),
                ] {
                    let subgame = Subgame {
                        p1score: p1score as i8,
                        p2score: p2score as i8,
                    };
                    let solution = &solutions[&subgame];
                    let strategy = &strategies[&subgame];

                    let bot_strategy = if args.average_strategy {
                        &solution.average_strategy
                    } else {
                        strategy
                    };
                    let max_outcome_values = OutcomeValues {
                        both_win: value_of_score(&max_ev, p1score + 1, p2score + 1),
                        p1_win: value_of_score(&max_ev, p1score + 1, p2score),
                        p2_win: value_of_score(&max_ev, p1score, p2score + 1),
                        both_lose: value_of_score(&max_ev, p1score, p2score),
                        first_move_epsilon: 0.0,
                    };
                    let min_outcome_values = OutcomeValues {
                        both_win: value_of_score(&min_ev, p1score + 1, p2score + 1),
                        p1_win: value_of_score(&min_ev, p1score + 1, p2score),
                        p2_win: value_of_score(&min_ev, p1score, p2score + 1),
                        both_lose: value_of_score(&min_ev, p1score, p2score),
                        first_move_epsilon: 0.0,
                    };
                    // println!(
                    //     "Min outcome {:#?} Mx outcome {:#?}",
                    //     min_outcome_values, max_outcome_values
                    // );
                    let counterfactual_probs = bot_strategy.counterfactual_probs(&game_tree);
                    let p1_best_response = BestResponse::new(
                        &bot_strategy,
                        &game_tree,
                        &counterfactual_probs,
                        &max_outcome_values,
                    );
                    let p2_best_response = BestResponse::new(
                        &bot_strategy,
                        &game_tree,
                        &counterfactual_probs,
                        &min_outcome_values,
                    );
                    let p1_exploiter =
                        Strategy::splice(&bot_strategy, &p2_best_response.strategy, &game_tree);
                    let p2_exploiter =
                        Strategy::splice(&p1_best_response.strategy, &bot_strategy, &game_tree);

                    p1_best_outcomes[i] =
                        outcome_probs(&p2_exploiter.visit_probs(&game_tree), &game_tree);
                    p2_best_outcomes[i] =
                        outcome_probs(&p1_exploiter.visit_probs(&game_tree), &game_tree);
                }

                let mut evt = [[0.0, 0.0], [0.0, 0.0]];
                for (i, p1score, p2score) in [
                    (0, larger_score, smaller_score),
                    (1, smaller_score, larger_score),
                ] {
                    if i == 1 && larger_score == smaller_score {
                        continue;
                    }
                    let subgame = Subgame {
                        p1score: p1score as i8,
                        p2score: p2score as i8,
                    };

                    let outcome_sums =
                        |ev: &HashMap<Subgame, f64>,
                         p1score: i8,
                         p2score: i8,
                         outcome_probs: &HashMap<(bool, bool), f64>,
                         outcomes: &[(bool, bool)]| {
                            let mut sum = 0.0;
                            for (p1point, p2point) in outcomes {
                                let weight = outcome_probs[&(*p1point, *p2point)];
                                let value = value_of_score(
                                    &ev,
                                    p1score + *p1point as i8,
                                    p2score + *p2point as i8,
                                );
                                sum += weight * value;
                            }
                            return sum;
                        };

                    for (j, best_outcomes, other_best_outcomes, evs, other_evs) in [
                        (0, &p1_best_outcomes, &p2_best_outcomes, &max_ev, &min_ev),
                        (1, &p2_best_outcomes, &p1_best_outcomes, &min_ev, &max_ev),
                    ] {
                        let mut direct_sum = outcome_sums(
                            other_evs,
                            p1score,
                            p2score,
                            &best_outcomes[i],
                            &[(true, false), (false, true)],
                        );
                        let mut other_sum = outcome_sums(
                            evs,
                            p2score,
                            p1score,
                            &other_best_outcomes[i ^ 1],
                            &[(true, false), (false, true)],
                        );

                        let mut transition_prob = best_outcomes[i][&(false, false)];
                        let mut reverse_transition_prob =
                            other_best_outcomes[i ^ 1][&(false, false)];

                        // If we are both at a score of 4, then both winning isn't an escape from this subgame.
                        if p1score == winning_score - 1 && p2score == winning_score - 1 {
                            transition_prob += best_outcomes[i][&(true, true)];
                            reverse_transition_prob += other_best_outcomes[i ^ 1][&(true, true)];
                        } else {
                            direct_sum += outcome_sums(
                                other_evs,
                                p1score,
                                p2score,
                                &best_outcomes[i],
                                &[(true, true)],
                            );
                            other_sum += outcome_sums(
                                evs,
                                p2score,
                                p1score,
                                &other_best_outcomes[i ^ 1],
                                &[(true, true)],
                            );
                        }

                        // Other sum negated due to players reversing in opposite subgame.
                        let value_of_subgame = (direct_sum - other_sum * transition_prob)
                            / (1.0 - transition_prob * reverse_transition_prob);
                        println!(
                            "After iteration of best response, {} value of subgame ({},{}) is {}",
                            if j == 0 { "max" } else { "min" },
                            p1score,
                            p2score,
                            value_of_subgame
                        );
                        evt[i][j] = value_of_subgame;
                    }
                }
                for (i, p1score, p2score) in [
                    (0, larger_score, smaller_score),
                    (1, smaller_score, larger_score),
                ] {
                    if i == 1 && larger_score == smaller_score {
                        continue;
                    }
                    let subgame = Subgame {
                        p1score: p1score as i8,
                        p2score: p2score as i8,
                    };

                    for (j, best_outcomes, evs) in [
                        (0, &p1_best_outcomes, &mut max_ev),
                        (1, &p2_best_outcomes, &mut min_ev),
                    ] {
                        let prev_ev = evs.get_mut(&subgame.clone()).unwrap();
                        if j == 0 {
                            if evt[i][j] > *prev_ev {
                                *prev_ev = evt[i][j];
                                changed = true;
                            }
                        } else {
                            if evt[i][j] < *prev_ev {
                                *prev_ev = evt[i][j];
                                changed = true;
                            }
                        }
                    }
                }
                if !changed {
                    println!("Found TOTAL exploitability starting at a pair of subgames!");
                    for (i, p1score, p2score) in [
                        (0, larger_score, smaller_score),
                        (0, smaller_score, larger_score),
                    ] {
                        if i == 1 && larger_score == smaller_score {
                            continue;
                        }
                        let max = max_ev[&Subgame { p1score, p2score }];
                        let min = min_ev[&Subgame { p1score, p2score }];
                        println!(
                            "({}, {}) Max value {} Min Value {} Exploitability {}",
                            p1score,
                            p2score,
                            max,
                            min,
                            max - min
                        );
                    }
                    break;
                }
            }
        }
    }
}
