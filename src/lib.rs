use itertools::Itertools;
use rand::{
    distributions::{Distribution, Standard},
    Rng,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
};
use strum::IntoEnumIterator;
use strum_macros::Display;
use strum_macros::EnumIter;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CFRDiscounting {
    pub alpha: f64,
    pub beta: f64,
    pub gamma: f64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CFR {
    pub discounting: Option<CFRDiscounting>,
    pub total_regrets: InfoStateRegrets,
    pub average_strategy: Strategy,
    pub t: usize,

    // Intermediate values, for debugging.
    pub expected_value: HashMap<MetaState, f64>,
    pub counterfactual_probs: HashMap<MetaState, f64>,
    pub metastate_regrets: HashMap<MetaState, f64>,
    pub infostate_regrets: InfoStateRegrets,

    pub player_to_update: Option<Player>,
}
impl CFR {
    pub fn new(discounting: Option<CFRDiscounting>, alternating_updates: bool) -> CFR {
        CFR {
            discounting,
            total_regrets: InfoStateRegrets::empty(),
            average_strategy: Strategy {
                probs: HashMap::new(),
            },
            t: 0,
            expected_value: HashMap::new(),
            counterfactual_probs: HashMap::new(),
            metastate_regrets: HashMap::new(),
            infostate_regrets: InfoStateRegrets::empty(),
            player_to_update: if alternating_updates {
                Some(Player::Player1)
            } else {
                None
            },
        }
    }

    fn update_avg_strategy(&mut self, tree: &GameTree, strategy: &Strategy) {
        let gamma = if let Some(discount) = &self.discounting {
            discount.gamma
        } else {
            1.0
        };
        for (infostate, probs) in &strategy.probs {
            if self
                .player_to_update
                .map(|p| p == tree.current_player[&infostate.state])
                .unwrap_or(true)
            {
                let avg_probs = self
                    .average_strategy
                    .probs
                    .entry(infostate.clone())
                    .or_insert_with(|| vec![0.0; probs.len()]);
                for i in 0..probs.len() {
                    let ratio = (self.t as f64 / (self.t + 1) as f64).powf(gamma);
                    avg_probs[i] = ratio * avg_probs[i] + (1.0 - ratio) * probs[i];
                }
            }
        }
        if self
            .player_to_update
            .map(|p| p == Player::Player2)
            .unwrap_or(true)
        {
            self.t += 1;
        }
    }

    pub fn cfr_round(
        &mut self,
        strategy: &Strategy,
        tree: &GameTree,
        outcome_values: &OutcomeValues,
    ) -> Strategy {
        self.expected_value = strategy.expected_values(&tree, outcome_values);
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
        println!("Overall expected value {}", avg_return / 9.0);

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

        if let Some(discount) = &self.discounting {
            self.total_regrets
                .discount(&tree, self.player_to_update, discount, self.t);
        }
        self.infostate_regrets
            .for_player(&tree, self.player_to_update);
        self.total_regrets.add(&self.infostate_regrets);
        let strategy = self.total_regrets.regret_matching_strategy(tree);
        // for (s, prob) in &strategy.probs {
        //     println!("State has probs {:?}:", prob);
        //     println!("nchildren {}", game_tree.children[&s.state].len());
        //     println!("Goals {:?}", s.goal);
        //     println!("{:?}", game_tree.states[s.state]);
        // }

        self.update_avg_strategy(&tree, &strategy);

        self.player_to_update = self.player_to_update.map(|p| p.opponent());
        return strategy;
    }

    pub fn overall_ev(&self) -> f64 {
        let mut avg_return = 0f64;
        for p1goal in Outcome::iter() {
            for p2goal in Outcome::iter() {
                avg_return += self.expected_value[&MetaState {
                    state: 0,
                    p1goal,
                    p2goal,
                }];
            }
        }
        return avg_return / 9.0;
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

#[derive(Debug)]
pub struct OutcomeValues {
    pub both_win: f64,
    pub p1_win: f64,
    pub p2_win: f64,
    pub both_lose: f64,
    // Epsilon reward for making "smaller" move numbers.
    pub first_move_epsilon: f64,
}

impl OutcomeValues {
    pub fn default() -> OutcomeValues {
        OutcomeValues {
            both_win: 0f64,
            p1_win: 1f64,
            p2_win: -1f64,
            both_lose: 0f64,
            first_move_epsilon: 0f64,
        }
    }
    pub fn evaluate(&self, state: &MetaState, tree: &GameTree, outcomes: (bool, bool)) -> f64 {
        let mut result = match outcomes {
            (true, true) => self.both_win,
            (true, false) => self.p1_win,
            (false, true) => self.p2_win,
            (false, false) => self.both_lose,
        };

        if self.first_move_epsilon != 0.0 {
            let (p1movesum, p2movesum) = tree.states[state.state].move_sums();
            result += self.first_move_epsilon * (p2movesum - p1movesum) as f64;
        }
        return result;
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

    pub fn discount(
        &mut self,
        game_tree: &GameTree,
        player: Option<Player>,
        discount: &CFRDiscounting,
        t: usize,
    ) {
        for (infostate, regrets) in self.0.iter_mut() {
            if player
                .map(|p| p == game_tree.current_player[&infostate.state])
                .unwrap_or(true)
            {
                for regret in regrets {
                    if *regret >= 0.0 {
                        let exp = ((t + 1) as f64).powf(discount.alpha);
                        *regret *= exp / (exp + 1.0);
                    } else {
                        let exp = ((t + 1) as f64).powf(discount.beta);
                        *regret *= exp / (exp + 1.0);
                    }
                }
            }
        }
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

    pub fn for_player(&mut self, tree: &GameTree, player: Option<Player>) {
        for (infostate, regrets) in &mut self.0 {
            if player
                .map(|p| p != tree.current_player[&infostate.state])
                .unwrap_or(false)
            {
                for r in regrets {
                    *r = 0.0;
                }
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
        outcome_values: &OutcomeValues,
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
                        result.insert(
                            metastate,
                            outcome_values.evaluate(&metastate, tree, outcomes),
                        );
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

    pub fn max_difference(&self, other: &Strategy) -> f64 {
        let mut max = 0.0;
        for (k, v) in self.probs.iter() {
            let other_probs = &other.probs[k];
            for i in 0..v.len() {
                max = f64::max(f64::abs(v[i] - other_probs[i]), max);
            }
        }
        return max;
    }

    pub fn visit_probs(&self, tree: &GameTree) -> HashMap<MetaState, f64> {
        let mut result = HashMap::<MetaState, f64>::new();
        for (id, state) in tree.states.iter().enumerate() {
            for p1goal in Outcome::iter() {
                for p2goal in Outcome::iter() {
                    let metastate = MetaState {
                        state: id,
                        p1goal,
                        p2goal,
                    };
                    let info_state = metastate.info_state(tree);
                    let prob = *result.entry(metastate).or_insert(1.0 / 9.0);
                    for (child_prob, child) in
                        itertools::zip(self.probs[&info_state].iter(), tree.children[&id].iter())
                    {
                        result.insert(
                            MetaState {
                                state: *child,
                                p1goal,
                                p2goal,
                            },
                            prob * child_prob,
                        );
                    }
                }
            }
        }
        return result;
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
        let mut p1_normalizing_sum = HashMap::<InfoState, f64>::new();
        let mut p2_normalizing_sum = HashMap::<InfoState, f64>::new();
        for (i, state) in tree.states.iter().enumerate().rev() {
            for p1goal in Outcome::iter() {
                for p2goal in Outcome::iter() {
                    let metastate = MetaState {
                        state: i,
                        p1goal,
                        p2goal,
                    };
                    let active_player_value;
                    let passive_player_value;

                    let (active_unnormalized_values, passive_unnormalized_values) = match state
                        .current_player()
                    {
                        Player::Player1 => (&mut p1_unnormalized_value, &mut p2_unnormalized_value),
                        Player::Player2 => (&mut p2_unnormalized_value, &mut p1_unnormalized_value),
                    };
                    let (active_normalizing_sum, passive_normalizing_sum) =
                        match state.current_player() {
                            Player::Player1 => (&mut p1_normalizing_sum, &mut p2_normalizing_sum),
                            Player::Player2 => (&mut p2_normalizing_sum, &mut p1_normalizing_sum),
                        };
                    let (active_goal, passive_goal) = match state.current_player() {
                        Player::Player1 => (p1goal, p2goal),
                        Player::Player2 => (p2goal, p1goal),
                    };
                    if let Some(outcomes) = metastate.outcomes(tree) {
                        let outcome_value = outcome_values.evaluate(&metastate, tree, outcomes);
                        active_player_value = outcome_value;
                        passive_player_value = outcome_value;
                    } else {
                        active_player_value = tree.children[&metastate.state]
                            .iter()
                            .map(|c| {
                                let infostate = InfoState {
                                    state: *c,
                                    goal: active_goal,
                                };
                                let denom = active_normalizing_sum[&infostate];
                                active_unnormalized_values[&infostate]
                                    / if denom == 0.0 { 1.0 } else { denom }
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
                            let infostate = InfoState {
                                state: *c,
                                goal: passive_goal,
                            };
                            let denom = passive_normalizing_sum[&infostate];
                            p * passive_unnormalized_values[&infostate]
                                / if denom == 0.0 { 1.0 } else { denom }
                        })
                        .sum();
                    };
                    *active_unnormalized_values
                        .entry(metastate.info_state(tree))
                        .or_insert(0.0) += counterfactual_probs[&metastate] * active_player_value;
                    *active_normalizing_sum
                        .entry(metastate.info_state(tree))
                        .or_insert(0.0) += counterfactual_probs[&metastate];
                    *passive_unnormalized_values
                        .entry(InfoState {
                            state: metastate.state,
                            goal: passive_goal,
                        })
                        .or_insert(0.0) += metastate
                        .parent(tree)
                        .map(|p| counterfactual_probs[&p])
                        .unwrap_or(1.0 / 9.0)
                        * passive_player_value;
                    *passive_normalizing_sum
                        .entry(InfoState {
                            state: metastate.state,
                            goal: passive_goal,
                        })
                        .or_insert(0.0) += metastate
                        .parent(tree)
                        .map(|p| counterfactual_probs[&p])
                        .unwrap_or(1.0 / 9.0)
                }
            }
        }
        let mut result = Strategy {
            probs: HashMap::new(),
        };
        for (i, state) in tree.states.iter().enumerate().rev() {
            for goal in Outcome::iter() {
                let infostate = InfoState { state: i, goal };
                *p1_unnormalized_value.get_mut(&infostate).unwrap() /=
                    p1_normalizing_sum[&infostate];
                *p2_unnormalized_value.get_mut(&infostate).unwrap() /=
                    p2_normalizing_sum[&infostate];

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

impl Distribution<Outcome> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Outcome {
        match rng.gen_range(0..=2) {
            // rand 0.8
            0 => Outcome::Win,
            1 => Outcome::Lose,
            _ => Outcome::Tie,
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
    pub current_player: HashMap<StateId, Player>,
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
        let current_player = all_states
            .iter()
            .enumerate()
            .map(|(i, s)| (i, s.current_player()))
            .collect();

        return GameTree {
            states: all_states,
            ids,
            parents,
            children,
            terminals,
            current_player,
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

#[derive(Serialize, Deserialize, Debug, Copy, Clone, Hash, Eq, PartialEq, EnumIter)]
pub enum Player {
    Player1,
    Player2,
}

impl Player {
    pub fn opponent(&self) -> Player {
        match self {
            Player::Player1 => Player::Player2,
            Player::Player2 => Player::Player1,
        }
    }
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

    pub fn move_sums(&self) -> (i64, i64) {
        let (mut p1sum, mut p2sum) = (0, 0);
        for (i, &m) in self.moves.iter().enumerate() {
            if m != 0 {
                if m % 2 == 1 {
                    p1sum += 9i64.pow((8 - m) as u32) * i as i64;
                } else {
                    p2sum += 9i64.pow((8 - m) as u32) * i as i64;
                }
            }
        }
        return (p1sum, p2sum);
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct Subgame {
    pub p1score: i8,
    pub p2score: i8,
}

pub fn exploitability_bound(
    game_tree: &GameTree,
    strategy: &Strategy,
    outcome_values: &OutcomeValues,
) -> f64 {
    let counterfactual_probs = strategy.counterfactual_probs(&game_tree);
    let best_response = BestResponse::new(
        &strategy,
        &game_tree,
        &counterfactual_probs,
        &outcome_values,
    );
    let p1_exploiter = Strategy::splice(&strategy, &best_response.strategy, &game_tree);
    let p2_exploiter = Strategy::splice(&best_response.strategy, &strategy, &game_tree);
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
