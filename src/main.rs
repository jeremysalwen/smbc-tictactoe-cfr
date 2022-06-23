use itertools::Itertools;
use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

fn main() {
    println!("Hello, world!");

    let game_tree = GameTree::new();
    println!("{} States in the game tree", game_tree.states.len());
    println!("{} Terminal states", game_tree.terminals.len());

    let uniform = Strategy::uniform(&game_tree);

    let mut cfr = CFR::new();
    let mut strategy = uniform.clone();
    for i in 0..2000 {
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
    let mut avg_return = 0f32;
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

struct CFR {
    total_regrets: InfoStateRegrets,
    average_strategy: Strategy,
    t: usize,
}
impl CFR {
    fn new() -> CFR {
        CFR {
            total_regrets: InfoStateRegrets(HashMap::new()),
            average_strategy: Strategy {
                probs: HashMap::new(),
            },
            t: 0,
        }
    }
    fn update_avg_strategy(&mut self, strategy: &Strategy) {
        for (infostate, probs) in &strategy.probs {
            let avg_probs = self
                .average_strategy
                .probs
                .entry(infostate.clone())
                .or_insert_with(|| vec![0.0; probs.len()]);
            for i in 0..probs.len() {
                avg_probs[i] = (self.t as f32 / (self.t + 1) as f32) * avg_probs[i]
                    + 1.0 / (self.t + 1) as f32 * probs[i];
            }
        }
        self.t += 1;
    }

    fn cfr_round(&mut self, strategy: &Strategy, tree: &GameTree) -> Strategy {
        let ev = strategy.expected_values(&tree, OutcomeValues::default());
        // for (s, value) in &ev {
        //     println!("State has value {}:", value);
        //     println!("Goals {:?} {:?}", s.p1goal, s.p2goal);
        //     println!("{:?}", game_tree.states[s.state]);
        // }
        // println!("Expected values: {:?}", ev);

        let mut avg_return = 0f32;
        for p1goal in Outcome::iter() {
            for p2goal in Outcome::iter() {
                let ret = ev[&MetaState {
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

        let counterfactual_probs = strategy.counterfactual_probs(tree);
        // for (s, prob) in &counterfactual_probs {
        //     println!("State has CF prob {}:", prob);
        //     println!("Goals {:?} {:?}", s.p1goal, s.p2goal);
        //     println!("{:?}", game_tree.states[s.state]);
        // }
        let metastate_regrets = strategy.metastate_regrets(tree, &ev, &counterfactual_probs);
        // for (s, regret) in &metastate_regrets {
        //     println!("State has regret {}:", regret);
        //     println!("Goals {:?} {:?}", s.p1goal, s.p2goal);
        //     println!("{:?}", game_tree.states[s.state]);
        // }

        let infostate_regrets = InfoStateRegrets::from_metastate_regrets(&metastate_regrets, tree);
        // for (s, regret) in &infostate_regrets.0 {
        //     println!("State has regrets {:?}:", regret);
        //     println!("nchildren {}", game_tree.children[&s.state].len());
        //     println!("Goals {:?}", s.goal);
        //     println!("{:?}", game_tree.states[s.state]);
        // }
        self.total_regrets.add(&infostate_regrets);
        let strategy = self.total_regrets.regret_matching_strategy(tree);
        // for (s, prob) in &strategy.probs {
        //     println!("State has probs {:?}:", prob);
        //     println!("nchildren {}", game_tree.children[&s.state].len());
        //     println!("Goals {:?}", s.goal);
        //     println!("{:?}", game_tree.states[s.state]);
        // }

        self.update_avg_strategy(&strategy);

        return strategy;
    }
}

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy)]
struct MetaState {
    state: StateId,
    p1goal: Outcome,
    p2goal: Outcome,
}

impl MetaState {
    fn info_state(&self, tree: &GameTree) -> InfoState {
        return InfoState {
            state: self.state,
            goal: if tree.states[self.state].current_player() == Player::Player1 {
                self.p1goal
            } else {
                self.p2goal
            },
        };
    }

    fn children(&self, tree: &GameTree) -> Vec<MetaState> {
        tree.children
            .get(&self.state)
            .unwrap()
            .iter()
            .map(|s| MetaState {
                state: *s,
                p1goal: self.p1goal,
                p2goal: self.p2goal,
            })
            .collect()
    }
    fn outcomes(&self, tree: &GameTree) -> Option<(bool, bool)> {
        tree.terminals
            .get(&self.state)
            .map(|outcome| (self.p1goal == *outcome, self.p2goal == outcome.reverse()))
    }
}

#[derive(Eq, PartialEq, Hash, Debug, Clone, Copy)]
struct InfoState {
    state: StateId,
    goal: Outcome,
}

struct OutcomeValues {
    both_win: f32,
    p1_win: f32,
    p2_win: f32,
    both_lose: f32,
}

impl OutcomeValues {
    fn default() -> OutcomeValues {
        OutcomeValues {
            both_win: 0f32,
            p1_win: 1f32,
            p2_win: -1f32,
            both_lose: 0f32,
        }
    }
    fn evaluate(&self, outcomes: (bool, bool)) -> f32 {
        match outcomes {
            (true, true) => self.both_win,
            (true, false) => self.p1_win,
            (false, true) => self.p2_win,
            (false, false) => self.both_lose,
        }
    }
}

struct InfoStateRegrets(HashMap<InfoState, Vec<f32>>);

impl InfoStateRegrets {
    fn from_metastate_regrets(
        metastate_regrets: &HashMap<MetaState, f32>,
        tree: &GameTree,
    ) -> InfoStateRegrets {
        let mut result = HashMap::new();
        for (id, state) in tree.states.iter().enumerate() {
            for goal in Outcome::iter() {
                let infostate = InfoState { state: id, goal };
                let mut regret = vec![];
                for child in &tree.children[&id] {
                    let mut action_regret = 0f32;
                    for other_goal in Outcome::iter() {
                        let (p1goal, p2goal) = if state.current_player() == Player::Player1 {
                            (goal, other_goal)
                        } else {
                            (other_goal, goal)
                        };
                        let child_metastate = MetaState {
                            state: *child,
                            p1goal,
                            p2goal,
                        };
                        action_regret += metastate_regrets[&child_metastate];
                    }
                    regret.push(action_regret);
                }
                result.insert(infostate, regret);
            }
        }
        return InfoStateRegrets(result);
    }

    fn add(&mut self, other: &InfoStateRegrets) {
        for (infostate, regrets) in &other.0 {
            let mut entry = self
                .0
                .entry(infostate.clone())
                .or_insert_with(|| vec![0.0; regrets.len()]);
            for i in 0..entry.len() {
                entry[i] += regrets[i];
            }
        }
    }

    fn regret_matching_strategy(&self, tree: &GameTree) -> Strategy {
        let mut result = HashMap::new();
        for (id, _) in tree.states.iter().enumerate() {
            for goal in Outcome::iter() {
                let infostate = InfoState { state: id, goal };

                let mut positive_regret = 0f32;
                for regret in &self.0[&infostate] {
                    positive_regret += f32::max(*regret, 0f32);
                }

                if positive_regret > 0.0 {
                    result.insert(
                        infostate,
                        self.0[&infostate]
                            .iter()
                            .map(|r| f32::max(*r, 0.0) / positive_regret)
                            .collect(),
                    );
                } else {
                    let nchildren = tree.children[&id].len();
                    result.insert(infostate, vec![1.0 / nchildren as f32; nchildren]);
                }
            }
        }
        return Strategy { probs: result };
    }
}

#[derive(Debug, Clone)]
struct Strategy {
    probs: HashMap<InfoState, Vec<f32>>,
}

impl Strategy {
    fn uniform(tree: &GameTree) -> Strategy {
        Strategy {
            probs: Outcome::iter()
                .cartesian_product(tree.children.iter())
                .map(|(goal, (parent, children))| {
                    (
                        InfoState {
                            state: *parent,
                            goal: goal,
                        },
                        vec![1.0 / children.len() as f32; children.len()],
                    )
                })
                .collect(),
        }
    }

    fn expected_values(
        &self,
        tree: &GameTree,
        outcome_values: OutcomeValues,
    ) -> HashMap<MetaState, f32> {
        let mut result = HashMap::new();
        for (i, _) in tree.states.iter().enumerate().rev() {
            for p1goal in Outcome::iter() {
                for p2goal in Outcome::iter() {
                    let metastate = MetaState {
                        state: i,
                        p1goal,
                        p2goal,
                    };
                    if let Some(outcomes) = metastate.outcomes(tree) {
                        result.insert(metastate, outcome_values.evaluate(outcomes));
                    } else {
                        let infostate = metastate.info_state(tree);
                        let mut sum = 0f32;
                        let mut count = 0f32;
                        for (p, child) in itertools::zip(
                            self.probs[&infostate].iter(),
                            metastate.children(tree).iter(),
                        ) {
                            let child_value = *result.get(child).unwrap();
                            sum += child_value * p;
                            count += p;
                        }
                        result.insert(metastate, sum / count);
                    }
                }
            }
        }
        return result;
    }

    fn counterfactual_probs(&self, tree: &GameTree) -> HashMap<MetaState, f32> {
        let mut counterfactual_probs1 = HashMap::<MetaState, f32>::new();
        let mut counterfactual_probs2 = HashMap::<MetaState, f32>::new();
        for (id, state) in tree.states.iter().enumerate() {
            for p1goal in Outcome::iter() {
                for p2goal in Outcome::iter() {
                    let metastate = MetaState {
                        state: id,
                        p1goal,
                        p2goal,
                    };
                    let info_state = metastate.info_state(tree);
                    let (active_hashmap, passive_hashmap) = match state.current_player() {
                        Player::Player1 => (&mut counterfactual_probs2, &mut counterfactual_probs1),
                        Player::Player2 => (&mut counterfactual_probs1, &mut counterfactual_probs2),
                    };
                    if id == 0 {}
                    let (active_prob, passive_prob) = (
                        *active_hashmap.entry(metastate).or_insert(1.0 / 9.0),
                        *passive_hashmap.entry(metastate).or_insert(1.0 / 9.0),
                    );
                    for (prob, child) in
                        itertools::zip(self.probs[&info_state].iter(), tree.children[&id].iter())
                    {
                        let child_metastate = MetaState {
                            state: *child,
                            p1goal,
                            p2goal,
                        };
                        active_hashmap.insert(child_metastate, active_prob * prob);
                        passive_hashmap.insert(child_metastate, passive_prob);
                    }
                }
            }
        }
        // Reuse counterfactual_probs1 to combine the two hashmaps.
        for (id, state) in tree.states.iter().enumerate() {
            for p1goal in Outcome::iter() {
                for p2goal in Outcome::iter() {
                    let metastate = MetaState {
                        state: id,
                        p1goal,
                        p2goal,
                    };
                    if state.current_player() == Player::Player2 {
                        counterfactual_probs1.insert(metastate, counterfactual_probs2[&metastate]);
                    }
                }
            }
        }
        return counterfactual_probs1;
    }
    fn metastate_regrets(
        &self,
        tree: &GameTree,
        expected_value: &HashMap<MetaState, f32>,
        counterfactual_probs: &HashMap<MetaState, f32>,
    ) -> HashMap<MetaState, f32> {
        let mut result = HashMap::new();
        for (id, state) in tree.states.iter().enumerate() {
            for p1goal in Outcome::iter() {
                for p2goal in Outcome::iter() {
                    let metastate = MetaState {
                        state: id,
                        p1goal,
                        p2goal,
                    };
                    let counterfactual_value =
                        expected_value[&metastate] * counterfactual_probs[&metastate];

                    for child in tree.children[&id].iter() {
                        let child_metastate = MetaState {
                            state: *child,
                            p1goal,
                            p2goal,
                        };
                        let regret = expected_value[&child_metastate]
                            * counterfactual_probs[&metastate]
                            - counterfactual_value;

                        result.insert(
                            child_metastate,
                            if state.current_player() == Player::Player1 {
                                regret
                            } else {
                                -regret
                            },
                        );
                    }
                }
            }
        }
        return result;
    }

    fn exploiter(&self) -> Strategy {
        let mut result = HashMap::new();
        for (i, _) in tree.states.iter().enumerate().rev() {
            for p1goal in Outcome::iter() {
                for p2goal in Outcome::iter() {
                    let metastate = MetaState {
                        state: i,
                        p1goal,
                        p2goal,
                    };
                    if let Some(outcomes) = metastate.outcomes(tree) {
                        result.insert(metastate, outcome_values.evaluate(outcomes));
                    } else {
                        let infostate = metastate.info_state(tree);
                        let mut sum = 0f32;
                        let mut count = 0f32;
                        for (p, child) in itertools::zip(
                            self.probs[&infostate].iter(),
                            metastate.children(tree).iter(),
                        ) {
                            let child_value = *result.get(child).unwrap();
                            sum += child_value * p;
                            count += p;
                        }
                        result.insert(metastate, sum / count);
                    }
                }
            }
        }
        return result;
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, EnumIter, Debug)]
enum Outcome {
    Win,
    Lose,
    Tie,
}

impl Outcome {
    fn reverse(&self) -> Outcome {
        match self {
            Outcome::Win => Outcome::Lose,
            Outcome::Lose => Outcome::Win,
            Outcome::Tie => Outcome::Tie,
        }
    }
}

type StateId = usize;

struct GameTree {
    // Topologically sorted
    states: Vec<State>,
    ids: HashMap<State, StateId>,
    parents: HashMap<StateId, StateId>,
    children: HashMap<StateId, Vec<StateId>>,
    terminals: HashMap<StateId, Outcome>,
}

impl GameTree {
    fn new() -> GameTree {
        let board = State::start();
        let mut all_states = vec![];
        board.descendants(&mut all_states);
        let solved = forced_outcomes(&all_states);

        let mut redundant_states = HashSet::<State>::new();
        for (state, outcome) in solved.iter() {
            if outcome.is_some() {
                for child in state.children() {
                    redundant_states.insert(child);
                }
            }
        }

        all_states.retain(|s| !redundant_states.contains(&s));
        let ids: HashMap<State, StateId> = all_states
            .iter()
            .enumerate()
            .map(|(i, s)| (*s, i))
            .collect();

        let mut parents = HashMap::new();
        let mut children = HashMap::new();
        let mut terminals = HashMap::new();
        for (id, state) in all_states.iter().enumerate() {
            if let Some(outcome) = solved.get(state).unwrap() {
                terminals.insert(id, *outcome);
            }
            children.insert(id, Vec::new());
            for child in state.children() {
                if let Some(&child_id) = ids.get(&child) {
                    parents.insert(child_id, id);
                    children.get_mut(&id).unwrap().push(child_id);
                }
            }
        }

        return GameTree {
            states: all_states,
            ids,
            parents,
            children,
            terminals,
        };
    }
}

fn forced_outcomes(all_states: &Vec<State>) -> HashMap<State, Option<Outcome>> {
    let mut result = HashMap::new();
    for state in all_states.iter().rev() {
        match state.outcome() {
            Some(o) => {
                result.insert(state.clone(), Some(o));
            }
            None => {
                let mut outcome = None;
                for child in state.children() {
                    let child_outcome = result[&child];
                    if child_outcome == None || (outcome.is_some() && child_outcome != outcome) {
                        outcome = None;
                        break;
                    } else {
                        outcome = child_outcome;
                    }
                }
                result.insert(state.clone(), outcome);
            }
        }
    }
    return result;
}

#[derive(Copy, Clone, Hash, Eq, PartialEq)]
enum Player {
    Player1,
    Player2,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct State {
    moves: [u8; 9],
}

fn fmt_digit(f: &mut std::fmt::Formatter<'_>, digit: u8) -> std::fmt::Result {
    if digit == 0 {
        f.write_fmt(format_args!(". "))
    } else if digit % 2 == 00 {
        f.write_fmt(format_args!("\x1b[31m{}\x1b[0m ", digit))
    } else {
        f.write_fmt(format_args!("\x1b[32m{}\x1b[0m ", digit))
    }
}

impl Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for i in 0..9 {
            fmt_digit(f, self.moves[i])?;
            if i % 3 == 2 {
                f.write_fmt(format_args!("\n"))?;
            }
        }
        Ok(())
    }
}

impl State {
    fn start() -> Self {
        State {
            moves: [0, 0, 0, 0, 0, 0, 0, 0, 0],
        }
    }

    fn current_player(&self) -> Player {
        let max = self.moves.iter().max().unwrap();
        if max % 2 == 0 {
            Player::Player1
        } else {
            Player::Player2
        }
    }
    fn is_final(&self) -> bool {
        self.outcome().is_some()
    }

    fn outcome(&self) -> Option<Outcome> {
        for triple in [
            [0, 1, 2],
            [3, 4, 5],
            [6, 7, 8],
            [0, 3, 6],
            [1, 4, 7],
            [2, 5, 8],
            [0, 4, 8],
            [2, 4, 6],
        ] {
            if triple
                .into_iter()
                .all(|i| self.moves[i] != 0 && (self.moves[i] & 1) != 0)
            {
                return Some(Outcome::Win);
            }
            if triple
                .into_iter()
                .all(|i| self.moves[i] != 0 && (self.moves[i] & 1) == 0)
            {
                return Some(Outcome::Lose);
            }
        }
        if self.moves.into_iter().all(|m| m != 0) {
            return Some(Outcome::Tie);
        }
        return None;
    }

    fn children(&self) -> Vec<State> {
        let mut result = vec![];
        if self.outcome().is_none() {
            let move_num = self.moves.into_iter().max().unwrap();
            for i in 0..9 {
                if self.moves[i] == 0 {
                    let mut clone = self.clone();
                    clone.moves[i] = move_num + 1;
                    // if clone.moves[i] == 3 {
                    //     clone.moves[i] = 1;
                    // }
                    if !result
                        .iter()
                        .any(|s: &State| clone.drop_history().is_symmetry(&s.drop_history()))
                    {
                        result.push(clone);
                    }
                }
            }
        }
        return result;
    }

    fn descendants(&self, result: &mut Vec<State>) {
        result.push(self.clone());
        for child in self.children() {
            child.descendants(result);
        }
    }

    fn drop_history(&self) -> State {
        State {
            moves: self
                .moves
                .into_iter()
                .map(|m| if m == 0 { 0 } else { (m - 1) % 2 + 1 })
                .collect::<Vec<u8>>()
                .try_into()
                .unwrap(),
        }
    }
    fn rotate(&self, symmetry: u8) -> State {
        let mut moves = [0u8; 9];
        for x in 0..3 {
            for y in 0..3 {
                match symmetry {
                    0 => {
                        moves[x * 3 + y] = self.moves[x * 3 + y];
                    }
                    1 => {
                        moves[x * 3 + y] = self.moves[y * 3 + x];
                    }
                    2 => {
                        moves[x * 3 + y] = self.moves[(2 - x) * 3 + y];
                    }
                    3 => {
                        moves[x * 3 + y] = self.moves[(2 - y) * 3 + x];
                    }
                    4 => {
                        moves[x * 3 + y] = self.moves[x * 3 + (2 - y)];
                    }
                    5 => {
                        moves[x * 3 + y] = self.moves[y * 3 + (2 - x)];
                    }
                    6 => {
                        moves[x * 3 + y] = self.moves[(2 - x) * 3 + (2 - y)];
                    }
                    7 => {
                        moves[x * 3 + y] = self.moves[(2 - y) * 3 + (2 - x)];
                    }
                    8u8..=u8::MAX => todo!(),
                }
            }
        }
        return State { moves: moves };
    }

    fn is_symmetry(&self, other: &State) -> bool {
        for i in 0..8 {
            if self == &other.rotate(i) {
                return true;
            }
        }
        return false;
    }
}
