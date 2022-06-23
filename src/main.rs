use strum::IntoEnumIterator;
mod lib;
use lib::*;

fn main() {
    println!("Hello, world!");

    let game_tree = GameTree::new();
    println!("{} States in the game tree", game_tree.states.len());
    println!("{} Terminal states", game_tree.terminals.len());

    let uniform = Strategy::uniform(&game_tree);

    let mut cfr = CFR::new();
    let mut strategy = uniform.clone();
    for _ in 0..2000 {
        let new_strategy = cfr.cfr_round(&strategy, &game_tree);
        strategy = new_strategy;
    }
    println!("Approximate nash strategy!");
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
