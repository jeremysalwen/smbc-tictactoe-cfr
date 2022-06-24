use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
};
use strum::IntoEnumIterator;
use strum_macros::Display;
use strum_macros::EnumIter;

#[derive(Serialize, Deserialize, Debug)]
pub struct CFR {
    pub total_regrets: InfoStateRegrets,
    pub average_strategy: Strategy,
    pub t: usize,

    // Intermediate values, for debugging.
    pub expected_value: HashMap<MetaState, f64>,
    pub counterfactual_probs: HashMap<MetaState, f64>,
    pub metastate_regrets: HashMap<MetaState, f64>,
    pub infostate_regrets: InfoStateRegrets,
}
impl CFR {
    pub fn new() -> CFR {
        CFR {
            total_regrets: InfoStateRegrets::empty(),
            average_strategy: Strategy {
                probs: HashMap::new(),
            },
            t: 0,
            expected_value: HashMap::new(),
            counterfactual_probs: HashMap::new(),
            metastate_regrets: HashMap::new(),
            infostate_regrets: InfoStateRegrets::empty(),
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
                avg_probs[i] = (self.t as f64 / (self.t + 1) as f64) * avg_probs[i]
                    + 1.0 / (self.t + 1) as f64 * probs[i];
            }
        }
        self.t += 1;
    }

    pub fn cfr_round(&mut self, strategy: &Strategy, tree: &GameTree) -> Strategy {
        self.expected_value = strategy.expected_values(&tree, OutcomeValues::default());
        // for (s, value) in &ev {
        //     println!("State has value {}:", value);
        //     println!("Goals {:?} {:?}", s.p1goal, s.p2goal);
        //     println!("{:?}", game_tree.states[s.state]);
        // }
        // println!("Expected values: {:?}", ev);

        let mut avg_return = 0f64;
        for p1goal in Outcome::iter() {
            for p2goal in Outcome::iter() {
                let ret = self.expected_value[&MetaState {
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

        self.counterfactual_probs = strategy.counterfactual_probs(tree);
        // for (s, prob) in &counterfactual_probs {
        //     println!("State has CF prob {}:", prob);
        //     println!("Goals {:?} {:?}", s.p1goal, s.p2goal);
        //     println!("{:?}", game_tree.states[s.state]);
        // }
        self.metastate_regrets =
            strategy.metastate_regrets(tree, &self.expected_value, &self.counterfactual_probs);
        // for (s, regret) in &metastate_regrets {
        //     println!("State has regret {}:", regret);
        //     println!("Goals {:?} {:?}", s.p1goal, s.p2goal);
        //     println!("{:?}", game_tree.states[s.state]);
        // }

        self.infostate_regrets =
            InfoStateRegrets::from_metastate_regrets(&self.metastate_regrets, tree);
        // for (s, regret) in &infostate_regrets.0 {
        //     println!("State has regrets {:?}:", regret);
        //     println!("nchildren {}", game_tree.children[&s.state].len());
        //     println!("Goals {:?}", s.goal);
        //     println!("{:?}", game_tree.states[s.state]);
        // }
        self.total_regrets.add(&self.infostate_regrets);
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

#[derive(Serialize, Deserialize, Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub struct MetaState {
    pub state: StateId,
    pub p1goal: Outcome,
    pub p2goal: Outcome,
}

impl MetaState {
    pub fn info_state(&self, tree: &GameTree) -> InfoState {
        return InfoState {
            state: self.state,
            goal: if tree.states[self.state].current_player() == Player::Player1 {
                self.p1goal
            } else {
                self.p2goal
            },
        };
    }

    pub fn children(&self, tree: &GameTree) -> Vec<MetaState> {
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

    pub fn parent(&self, tree: &GameTree) -> Option<MetaState> {
        tree.parents.get(&self.state).map(|s| MetaState {
            state: *s,
            p1goal: self.p1goal,
            p2goal: self.p2goal,
        })
    }
    pub fn outcomes(&self, tree: &GameTree) -> Option<(bool, bool)> {
        tree.terminals
            .get(&self.state)
            .map(|outcome| (self.p1goal == *outcome, self.p2goal == outcome.reverse()))
    }
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Hash, Debug, Clone, Copy)]
pub struct InfoState {
    pub state: StateId,
    pub goal: Outcome,
}

pub struct OutcomeValues {
    pub both_win: f64,
    pub p1_win: f64,
    pub p2_win: f64,
    pub both_lose: f64,
}

impl OutcomeValues {
    pub fn default() -> OutcomeValues {
        OutcomeValues {
            both_win: 0f64,
            p1_win: 1f64,
            p2_win: -1f64,
            both_lose: 0f64,
        }
    }
    pub fn evaluate(&self, outcomes: (bool, bool)) -> f64 {
        match outcomes {
            (true, true) => self.both_win,
            (true, false) => self.p1_win,
            (false, true) => self.p2_win,
            (false, false) => self.both_lose,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct InfoStateRegrets(pub HashMap<InfoState, Vec<f64>>);

impl InfoStateRegrets {
    pub fn empty() -> Self {
        Self(HashMap::new())
    }
    pub fn from_metastate_regrets(
        metastate_regrets: &HashMap<MetaState, f64>,
        tree: &GameTree,
    ) -> InfoStateRegrets {
        let mut result = HashMap::new();
        for (id, state) in tree.states.iter().enumerate() {
            for goal in Outcome::iter() {
                let infostate = InfoState { state: id, goal };
                let mut regret = vec![];
                for child in &tree.children[&id] {
                    let mut action_regret = 0f64;
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

    pub fn add(&mut self, other: &InfoStateRegrets) {
        for (infostate, regrets) in &other.0 {
            let entry = self
                .0
                .entry(infostate.clone())
                .or_insert_with(|| vec![0.0; regrets.len()]);
            for i in 0..entry.len() {
                entry[i] += regrets[i];
            }
        }
    }

    pub fn regret_matching_strategy(&self, tree: &GameTree) -> Strategy {
        let mut result = HashMap::new();
        for (id, _) in tree.states.iter().enumerate() {
            for goal in Outcome::iter() {
                let infostate = InfoState { state: id, goal };

                let mut positive_regret = 0f64;
                for regret in &self.0[&infostate] {
                    positive_regret += f64::max(*regret, 0f64);
                }

                if positive_regret > 0.0 {
                    result.insert(
                        infostate,
                        self.0[&infostate]
                            .iter()
                            .map(|r| f64::max(*r, 0.0) / positive_regret)
                            .collect(),
                    );
                } else {
                    let nchildren = tree.children[&id].len();
                    result.insert(infostate, vec![1.0 / nchildren as f64; nchildren]);
                }
            }
        }
        return Strategy { probs: result };
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Strategy {
    pub probs: HashMap<InfoState, Vec<f64>>,
}

impl Strategy {
    pub fn uniform(tree: &GameTree) -> Strategy {
        Strategy {
            probs: Outcome::iter()
                .cartesian_product(tree.children.iter())
                .map(|(goal, (parent, children))| {
                    (
                        InfoState {
                            state: *parent,
                            goal: goal,
                        },
                        vec![1.0 / children.len() as f64; children.len()],
                    )
                })
                .collect(),
        }
    }

    pub fn expected_values(
        &self,
        tree: &GameTree,
        outcome_values: OutcomeValues,
    ) -> HashMap<MetaState, f64> {
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
                        let mut sum = 0f64;
                        let mut count = 0f64;
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

    pub fn counterfactual_probs(&self, tree: &GameTree) -> HashMap<MetaState, f64> {
        let mut counterfactual_probs1 = HashMap::<MetaState, f64>::new();
        let mut counterfactual_probs2 = HashMap::<MetaState, f64>::new();
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
    pub fn metastate_regrets(
        &self,
        tree: &GameTree,
        expected_value: &HashMap<MetaState, f64>,
        counterfactual_probs: &HashMap<MetaState, f64>,
    ) -> HashMap<MetaState, f64> {
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

    pub fn splice(
        player1_strategy: &Strategy,
        player2_strategy: &Strategy,
        tree: &GameTree,
    ) -> Strategy {
        let mut result = HashMap::new();
        for (infostate, probs) in &player1_strategy.probs {
            if tree.states[infostate.state].current_player() == Player::Player1 {
                result.insert(*infostate, probs.clone());
            }
        }
        for (infostate, probs) in &player2_strategy.probs {
            if tree.states[infostate.state].current_player() == Player::Player2 {
                result.insert(*infostate, probs.clone());
            }
        }
        return Strategy { probs: result };
    }
}

#[derive(Serialize, Deserialize)]
pub struct BestResponse {
    pub p1_value: HashMap<InfoState, f64>,
    pub p2_value: HashMap<InfoState, f64>,
    pub strategy: Strategy,
}

impl BestResponse {
    pub fn new(
        strategy: &Strategy,
        tree: &GameTree,
        counterfactual_probs: &HashMap<MetaState, f64>,
        outcome_values: &OutcomeValues,
    ) -> BestResponse {
        let mut p1_unnormalized_value = HashMap::<InfoState, f64>::new();
        let mut p2_unnormalized_value = HashMap::<InfoState, f64>::new();
        for (i, state) in tree.states.iter().enumerate().rev() {
            for p1goal in Outcome::iter() {
                for p2goal in Outcome::iter() {
                    let metastate = MetaState {
                        state: i,
                        p1goal,
                        p2goal,
                    };
                    let mut active_player_value = 0.0;
                    let mut passive_player_value = 0.0;

                    let (active_unnormalized_values, passive_unnormalized_values) = match state
                        .current_player()
                    {
                        Player::Player1 => (&mut p1_unnormalized_value, &mut p2_unnormalized_value),
                        Player::Player2 => (&mut p2_unnormalized_value, &mut p1_unnormalized_value),
                    };
                    let (active_goal, passive_goal) = match state.current_player() {
                        Player::Player1 => (p1goal, p2goal),
                        Player::Player2 => (p2goal, p1goal),
                    };
                    if let Some(outcomes) = metastate.outcomes(tree) {
                        let outcome_value = outcome_values.evaluate(outcomes);
                        active_player_value = outcome_value;
                        passive_player_value = outcome_value;
                    } else {
                        active_player_value = tree.children[&metastate.state]
                            .iter()
                            .map(|c| {
                                active_unnormalized_values[&InfoState {
                                    state: *c,
                                    goal: active_goal,
                                }]
                            })
                            .reduce(if state.current_player() == Player::Player1 {
                                f64::max
                            } else {
                                f64::min
                            })
                            .unwrap();
                        passive_player_value = itertools::zip(
                            strategy.probs[&metastate.info_state(tree)].iter(),
                            tree.children[&metastate.state].iter(),
                        )
                        .map(|(p, c)| {
                            p * passive_unnormalized_values[&InfoState {
                                state: *c,
                                goal: passive_goal,
                            }]
                        })
                        .sum();
                    };
                    *active_unnormalized_values
                        .entry(metastate.info_state(tree))
                        .or_insert(0.0) += counterfactual_probs[&metastate] * active_player_value;
                    *passive_unnormalized_values
                        .entry(metastate.info_state(tree))
                        .or_insert(0.0) += metastate
                        .parent(tree)
                        .map(|p| counterfactual_probs[&p])
                        .unwrap_or(1.0 / 9.0)
                        * passive_player_value;
                }
            }
        }
        let mut result = Strategy {
            probs: HashMap::new(),
        };
        for (i, state) in tree.states.iter().enumerate().rev() {
            for goal in Outcome::iter() {
                let infostate = InfoState { state: i, goal };

                let mut best_value = None;
                let mut best_index = None;

                for (i, c) in tree.children[&i].iter().enumerate() {
                    let value = match state.current_player() {
                        Player::Player1 => &p1_unnormalized_value,
                        Player::Player2 => &p2_unnormalized_value,
                    }[&InfoState {
                        state: *c,
                        goal: goal,
                    }];
                    if best_value.is_none()
                        || (state.current_player() == Player::Player1
                            && value > best_value.unwrap())
                        || (state.current_player() == Player::Player2
                            && value < best_value.unwrap())
                    {
                        best_value = Some(value);
                        best_index = Some(i);
                    }
                }
                let mut action_probs = vec![0.0; tree.children[&i].len()];
                if let Some(ind) = best_index {
                    action_probs[ind] = 1.0;
                }
                result.probs.insert(infostate, action_probs);
            }
        }
        return BestResponse {
            p1_value: p1_unnormalized_value,
            p2_value: p2_unnormalized_value,
            strategy: result,
        };
    }
}
#[derive(Serialize, Deserialize, Eq, PartialEq, Hash, Clone, Copy, EnumIter, Debug, Display)]
pub enum Outcome {
    Win,
    Lose,
    Tie,
}

impl Outcome {
    pub fn reverse(&self) -> Outcome {
        match self {
            Outcome::Win => Outcome::Lose,
            Outcome::Lose => Outcome::Win,
            Outcome::Tie => Outcome::Tie,
        }
    }
}

type StateId = usize;

pub struct GameTree {
    // Topologically sorted
    pub states: Vec<State>,
    pub ids: HashMap<State, StateId>,
    pub parents: HashMap<StateId, StateId>,
    pub children: HashMap<StateId, Vec<StateId>>,
    pub terminals: HashMap<StateId, Outcome>,
}

impl GameTree {
    pub fn new() -> GameTree {
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

pub fn forced_outcomes(all_states: &Vec<State>) -> HashMap<State, Option<Outcome>> {
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

#[derive(Serialize, Deserialize, Copy, Clone, Hash, Eq, PartialEq)]
pub enum Player {
    Player1,
    Player2,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct State {
    pub moves: [u8; 9],
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
    pub fn start() -> Self {
        State {
            moves: [0, 0, 0, 0, 0, 0, 0, 0, 0],
        }
    }

    pub fn current_player(&self) -> Player {
        let max = self.moves.iter().max().unwrap();
        if max % 2 == 0 {
            Player::Player1
        } else {
            Player::Player2
        }
    }
    pub fn is_final(&self) -> bool {
        self.outcome().is_some()
    }

    pub fn outcome(&self) -> Option<Outcome> {
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

    pub fn children(&self) -> Vec<State> {
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

    pub fn descendants(&self, result: &mut Vec<State>) {
        result.push(self.clone());
        for child in self.children() {
            child.descendants(result);
        }
    }

    pub fn drop_history(&self) -> State {
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
    pub fn rotate(&self, symmetry: u8) -> State {
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

    pub fn is_symmetry(&self, other: &State) -> bool {
        for i in 0..8 {
            if self == &other.rotate(i) {
                return true;
            }
        }
        return false;
    }
}
