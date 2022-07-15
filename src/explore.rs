use bincode;
use clap::Parser;
use clap::ValueHint;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::io::Write;

mod lib;
use lib::*;

/// Search for a pattern in a file and display the lines that contain it.
#[derive(Parser)]
struct Cli {
    /// The path to input directory.
    #[clap(short, long, parse(from_os_str), value_hint = ValueHint::DirPath)]
    solutions_dir: std::path::PathBuf,
}

fn best_response(args: &mut Cli, i: usize) -> BestResponse {
    let mut result = BestResponse {
        p1_value: HashMap::new(),
        p2_value: HashMap::new(),
        strategy: Strategy {
            probs: HashMap::new(),
        },
    };
    args.solutions_dir
        .push(format!("best_response_{}.bincode", i));
    if let Ok(f) = File::open(&args.solutions_dir) {
        result = bincode::deserialize_from(BufReader::new(f)).unwrap();
    }
    args.solutions_dir.pop();
    return result;
}
fn load_iteration(args: &mut Cli, i: usize) -> (CFR, Strategy, BestResponse) {
    args.solutions_dir.push(format!("debug_{}.bincode", i));
    let cfr = bincode::deserialize_from(BufReader::new(
        File::open(&args.solutions_dir).expect("couldn't open file"),
    ))
    .unwrap();
    args.solutions_dir.pop();
    args.solutions_dir.push(format!("strategy_{}.bincode", i));
    let strategy = bincode::deserialize_from(BufReader::new(
        File::open(&args.solutions_dir).expect("couldn't open file"),
    ))
    .unwrap();
    args.solutions_dir.pop();
    return (cfr, strategy, best_response(args, i));
}

fn char_to_outcome(c: char) -> Option<Outcome> {
    match c {
        'w' => Some(Outcome::Win),
        't' => Some(Outcome::Tie),
        'l' => Some(Outcome::Lose),
        _ => None,
    }
}

fn main() {
    let mut args = Cli::parse();
    println!("Constructing game tree...");

    let game_tree = GameTree::new();
    println!("{} States in the game tree", game_tree.states.len());
    println!("{} Terminal states", game_tree.terminals.len());

    let mut iteration = 0;

    let (mut cfr, mut strategy, mut best_response) = load_iteration(&mut args, iteration);

    let mut metastate = MetaState {
        state: 0,
        p1goal: Outcome::Win,
        p2goal: Outcome::Win,
    };

    loop {
        println!("Iteration {}", iteration);
        println!(
            "P1 Goal: {} P2 Goal: {}",
            metastate.p1goal, metastate.p2goal
        );
        println!("{:?}", game_tree.states[metastate.state]);
        println!(
            "EV: {} CF Prob {} Parent CF Prob {:?}",
            cfr.expected_value[&metastate],
            cfr.counterfactual_probs[&metastate],
            metastate
                .parent(&game_tree)
                .map(|m| cfr.counterfactual_probs[&m])
        );
        print!("Regrets: [");
        for child in metastate.children(&game_tree) {
            print!("{:1.4}, ", cfr.metastate_regrets[&child]);
        }
        println!("]\n");

        let infostate = metastate.info_state(&game_tree);
        println!(
            "Infostate regrets {:?}",
            cfr.infostate_regrets.0[&infostate]
        );
        println!("Total regrets {:?}", cfr.total_regrets.0[&infostate]);
        println!("Current strategy {:?}", strategy.probs[&infostate]);
        println!(
            "Average strategy {:?}",
            cfr.average_strategy.probs[&infostate]
        );
        println!(
            "Best response value for P1 {:?} P2 {:?}",
            best_response.p1_value.get(&InfoState {
                state: metastate.state,
                goal: metastate.p1goal
            }),
            best_response.p2_value.get(&InfoState {
                state: metastate.state,
                goal: metastate.p2goal
            }),
        );
        println!(
            "Best response strategy: {:?}",
            best_response
                .strategy
                .probs
                .get(&infostate)
                .unwrap_or(&vec![])
        );

        print!("> ");
        std::io::stdout().flush().unwrap();
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();
        match line.chars().nth(0).unwrap_or(' ') {
            'q' => break,
            'i' => {
                match line[1..].trim().parse() {
                    Ok(i) => {iteration = i;
                        (cfr, strategy, best_response) = load_iteration(&mut args, iteration);},
                        Err(e) => println!("invalid iteration {}", e)
                };
            }
            'm' => {
                if let Ok(position)  = line[1..].trim().parse::<usize>(){
                    if position > 9 || position <1 {
                        println!("Bad position {}", position);
                        continue;
                    }
                    let mut child_state = game_tree.states[metastate.state].clone();
                    child_state.moves[position-1]=child_state.moves.iter().max().unwrap()+1;
                    let mut found = false;
                    for symmetric_child in game_tree.children[&metastate.state].iter() {
                        if child_state.drop_history().is_symmetry(&game_tree.states[*symmetric_child].drop_history()) {
                            metastate.state = *symmetric_child;
                            found = true;
                        }
                    }
                    if !found {
                        println!("Invalid move!");
                    }
                } else {
                    println!("Cannot parse move.");
                }
            }
            'u' => {
                metastate.state = *game_tree.parents.get(&metastate.state).unwrap_or(&0);
            }
            'g' => {
                let p1goal = line.chars().nth(2).and_then(char_to_outcome);
                let p2goal = line.chars().nth(4).and_then(char_to_outcome);
                match (p1goal, p2goal) {
                     (Some(p1), Some(p2)) => {metastate.p1goal = p1; metastate.p2goal = p2}
                    _ => {println!("Invalid arguments.  Must be like 'g l t'");}
                }
            }
            _ => println!("Unrecognized command.  Valid commands are q (quit) i 3 (jump to iteration 3) m 5 (move at position 5), u (undo move), and g w l (set goals to P1 win, P2 lose)"),
        }
    }
}
