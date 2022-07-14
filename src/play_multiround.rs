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
    winning_score: i32,

    #[clap(short, long, action = ArgAction::Set,  default_value_t = true)]
    average_strategy: bool,
}

fn load(args: &mut Cli, p1score: i32, p2score: i32) -> (CFR, Strategy) {
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

fn main() {
    let mut args = Cli::parse();
    println!("Constructing game tree...");

    let game_tree = GameTree::new();
    println!("{} States in the game tree", game_tree.states.len());
    println!("{} Terminal states", game_tree.terminals.len());

    let (mut cfr, mut strategy) = load(&mut args, 0, 0);

    let mut bot_strategy = if args.average_strategy {
        cfr.average_strategy
    } else {
        strategy
    };

    let mut rng = thread_rng();

    let mut metastate = MetaState {
        state: 0,
        p1goal: rand::random(),
        p2goal: rand::random(),
    };
    let (mut humanscore, mut cpuscore) = (0, 0);
    let mut humanplayer = if rng.gen_bool(0.5) {
        Player::Player1
    } else {
        Player::Player2
    };

    loop {
        println!("{:?}", game_tree.states[metastate.state]);
        println!("Current score: You {} Bot {}", humanscore, cpuscore);
        println!(
            "Your goal is: {}",
            match humanplayer {
                Player::Player1 => metastate.p1goal,
                Player::Player2 => metastate.p2goal,
            }
        );

        if game_tree.states[metastate.state].current_player() == humanplayer {
            print!("Enter your move ( 1 through 9)> ");
            std::io::stdout().flush().unwrap();
            let mut line = String::new();
            std::io::stdin().read_line(&mut line).unwrap();
            match line.trim().parse::<usize>() {
                Ok(position) => {
                    if position > 9 || position < 1 {
                        println!("Bad position {}", position);
                        continue;
                    }
                    let mut child_state = game_tree.states[metastate.state].clone();
                    child_state.moves[position - 1] = child_state.moves.iter().max().unwrap() + 1;
                    let mut found = false;
                    for symmetric_child in game_tree.children[&metastate.state].iter() {
                        if child_state
                            .drop_history()
                            .is_symmetry(&game_tree.states[*symmetric_child].drop_history())
                        {
                            metastate.state = *symmetric_child;
                            found = true;
                        }
                    }
                    if !found {
                        println!("Invalid move!");
                    }
                }
                Err(_) => {
                    println!("Invalid move!");
                }
            };
        } else {
            let probs = &bot_strategy.probs[&metastate.info_state(&game_tree)];
            let weighted_index = rand::distributions::WeightedIndex::new(probs).unwrap();
            let choice = rng.sample(weighted_index);
            metastate.state = game_tree.children[&metastate.state][choice];
        }
        match game_tree.terminals.get(&metastate.state) {
            Some(outcome) => {
                println!("==============================");
                println!(
                    "Round ended.  You {}.",
                    if humanplayer == Player::Player1 {
                        outcome.clone()
                    } else {
                        outcome.reverse()
                    }
                );
                println!(
                    "The bot's goal was {}",
                    if humanplayer == Player::Player1 {
                        metastate.p2goal
                    } else {
                        metastate.p1goal
                    }
                );
                match humanplayer {
                    Player::Player1 => {
                        humanscore += if outcome == &metastate.p1goal { 1 } else { 0 };
                        cpuscore += if outcome == &metastate.p2goal.reverse() {
                            1
                        } else {
                            0
                        };
                    }
                    Player::Player2 => {
                        humanscore += if outcome == &metastate.p2goal.reverse() {
                            1
                        } else {
                            0
                        };
                        cpuscore += if outcome == &metastate.p1goal { 1 } else { 0 };
                    }
                }
                if cpuscore >= args.winning_score {
                    println!("The bot wins the match!");
                    break;
                }
                if humanscore >= args.winning_score {
                    println!("You win the match!");
                    break;
                }
                metastate = MetaState {
                    state: 0,
                    p1goal: rand::random(),
                    p2goal: rand::random(),
                };
                humanplayer = humanplayer.opponent();
                (cfr, strategy) = load(
                    &mut args,
                    if humanplayer == Player::Player1 {
                        humanscore
                    } else {
                        cpuscore
                    },
                    if humanplayer == Player::Player1 {
                        cpuscore
                    } else {
                        humanscore
                    },
                );

                bot_strategy = if args.average_strategy {
                    cfr.average_strategy
                } else {
                    strategy
                };
            }
            None => {}
        }
    }
}
