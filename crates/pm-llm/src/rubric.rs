//! The six-axis rubric as a versioned question set, and the code that turns its answers into
//! R1–R6 (plan §6.4): counting and arithmetic stay here, never in the model.

use serde::{Deserialize, Serialize};

use crate::schema::{Answer, Question, Questions, Request};
use crate::{Error, Result};

/// `six-axis@2` (anchors from `rubrics/six-axis@1.md`, unchanged). Bundled so a pmx binary always
/// scores with the version it reports.
pub const SIX_AXIS_V2: &str = include_str!("../rubrics/six-axis@2.toml");

pub const LAYERS: [&str; 8] = [
    "layer_transport",
    "layer_validation",
    "layer_service",
    "layer_domain",
    "layer_persistence",
    "layer_database",
    "layer_ui",
    "layer_navigation",
];
pub const R2: &str = "r2_data_model";
pub const R3: &str = "r3_contract";
pub const R4: &str = "r4_algorithmic";
pub const R5_APP_WIDE: &str = "r5_app_wide";
pub const R6: &str = "r6_verification";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    id: String,
    delta: f64,
    context: String,
    question: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    id: String,
    #[serde(flatten)]
    question: Question,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuestionSet {
    /// e.g. `six-axis@2`; part of the answer cache key.
    pub id: String,
    /// "When two anchors both fit, take the lower one": the margin in probability.
    pub delta: f64,
    /// Shared instructions, sent before the state.
    pub context: String,
    pub questions: Questions,
}

impl QuestionSet {
    pub fn parse(text: &str) -> Result<QuestionSet> {
        let f: File = toml::from_str(text).map_err(|e| Error::Rubric(e.to_string()))?;
        let set = QuestionSet {
            id: f.id,
            delta: f.delta,
            context: f.context.trim().to_string(),
            questions: Questions(f.question.into_iter().map(|e| (e.id, e.question)).collect()),
        };
        let has = |id: &str| set.questions.get(id).is_some();
        for id in LAYERS.iter().chain(&[R2, R3, R4, R5_APP_WIDE, R6]) {
            if !has(id) {
                return Err(Error::Rubric(format!("{}: question `{id}` is missing", set.id)));
            }
        }
        Ok(set)
    }

    pub fn six_axis() -> QuestionSet {
        QuestionSet::parse(SIX_AXIS_V2).expect("the bundled rubric parses")
    }

    pub fn request(&self, state: String) -> Request {
        Request {
            state,
            questions: self.questions.clone(),
        }
    }

    /// R1–R6 from the answers and the counts measured in code.
    pub fn axes(&self, answers: &std::collections::BTreeMap<String, Answer>, facts: &Facts) -> Result<Axes> {
        let get = |id: &str| {
            answers
                .get(id)
                .ok_or_else(|| Error::Answer(format!("no answer to `{id}`")))
        };
        let yes = |id: &str| -> Result<bool> {
            match get(id)? {
                Answer::Noul { noul } => Ok(*noul >= 0.5),
                _ => Err(Error::Answer(format!("`{id}` is not a yes/no answer"))),
            }
        };
        let level = |id: &str| -> Result<u8> {
            let levels = match self.questions.get(id) {
                Some(Question::Score { criteria, .. }) => criteria.len(),
                _ => return Err(Error::Rubric(format!("`{id}` is not a score question"))),
            };
            match get(id)? {
                Answer::Score {
                    score, probabilities, ..
                } => Ok(take_lower(*score, probabilities, levels, self.delta)),
                _ => Err(Error::Answer(format!("`{id}` is not a score answer"))),
            }
        };
        let mut layers = 0;
        for id in LAYERS {
            layers += yes(id)? as u32;
        }
        let r5 = if yes(R5_APP_WIDE)? {
            3
        } else if facts.modules > 1 {
            2
        } else if facts.files > 1 {
            1
        } else {
            0
        };
        Ok(Axes([
            layer_score(layers),
            level(R2)?,
            level(R3)?,
            level(R4)?,
            r5,
            level(R6)?,
        ]))
    }
}

/// R1 from the number of layers touched: 1 → 0, 2 → 1, 3–4 → 2, 5+ → 3.
pub fn layer_score(layers: u32) -> u8 {
    match layers {
        0..=1 => 0,
        2 => 1,
        3..=4 => 2,
        _ => 3,
    }
}

/// The most likely level, unless a lower level is within `delta` of it: then the lowest such level.
/// Without probabilities, the rounded score.
pub fn take_lower(
    score: f64,
    probabilities: &std::collections::BTreeMap<String, f64>,
    levels: usize,
    delta: f64,
) -> u8 {
    let max = levels.saturating_sub(1) as u8;
    let p: Vec<f64> = (0..levels)
        .map(|l| probabilities.get(&l.to_string()).copied().unwrap_or(0.0))
        .collect();
    if p.iter().all(|x| *x == 0.0) {
        return (score.round().max(0.0) as u8).min(max);
    }
    let top = p.iter().cloned().fold(f64::MIN, f64::max);
    // Small epsilon: probabilities come from JSON and are rarely exact.
    (0..levels)
        .find(|l| p[*l] >= top - delta - 1e-9)
        .map(|l| l as u8)
        .unwrap_or(0)
}

/// Counts measured in code for R5.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Facts {
    /// Changed files, generated ones excluded.
    pub files: u32,
    /// Distinct modules among them (repo plus the first two directories).
    pub modules: u32,
}

/// R1–R6, each 0–3.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Axes(pub [u8; 6]);

impl Axes {
    pub fn sum(&self) -> u8 {
        self.0.iter().sum()
    }

    /// Size and Fibonacci points of the sum (spec §4).
    pub fn size(&self) -> (&'static str, u32) {
        match self.sum() {
            0..=2 => ("XS", 1),
            3..=5 => ("S", 2),
            6..=8 => ("M", 3),
            9..=11 => ("L", 5),
            12..=14 => ("XL", 8),
            _ => ("XXL", 13),
        }
    }

    pub fn points(&self) -> u32 {
        self.size().1
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn probs(p: [f64; 4]) -> BTreeMap<String, f64> {
        (0..4).map(|i| (i.to_string(), p[i])).collect()
    }

    #[test]
    fn bundled_rubric_parses() {
        let s = QuestionSet::six_axis();
        assert_eq!(s.id, "six-axis@2");
        assert_eq!(s.questions.0.len(), 13);
        assert!(matches!(s.questions.get(R2), Some(Question::Score { criteria, .. }) if criteria.len() == 4));
    }

    #[test]
    fn lower_anchor_wins_within_delta() {
        assert_eq!(take_lower(0.0, &probs([0.1, 0.45, 0.4, 0.05]), 4, 0.1), 1);
        assert_eq!(take_lower(0.0, &probs([0.0, 0.25, 0.3, 0.45]), 4, 0.1), 3);
        assert_eq!(take_lower(0.0, &probs([0.0, 0.4, 0.15, 0.45]), 4, 0.1), 1);
        assert_eq!(take_lower(0.0, &probs([0.0, 0.0, 1.0, 0.0]), 4, 0.1), 2);
        // No probabilities: the plain answer.
        assert_eq!(take_lower(2.4, &BTreeMap::new(), 4, 0.1), 2);
        assert_eq!(take_lower(7.0, &BTreeMap::new(), 4, 0.1), 3);
    }

    #[test]
    fn axes_from_answers() {
        let set = QuestionSet::six_axis();
        let mut a: BTreeMap<String, Answer> = BTreeMap::new();
        for (i, id) in LAYERS.iter().enumerate() {
            a.insert(
                id.to_string(),
                Answer::Noul {
                    noul: if i < 3 { 0.9 } else { 0.1 },
                },
            );
        }
        let level = |l: usize| Answer::Score {
            score: l as f64,
            probabilities: BTreeMap::new(),
            confidence: None,
        };
        a.insert(R2.into(), level(1));
        a.insert(R3.into(), level(2));
        a.insert(R4.into(), level(0));
        a.insert(R6.into(), level(2));
        a.insert(R5_APP_WIDE.into(), Answer::Noul { noul: 0.2 });
        let facts = Facts { files: 5, modules: 2 };
        let axes = set.axes(&a, &facts).unwrap();
        assert_eq!(axes, Axes([2, 1, 2, 0, 2, 2]));
        assert_eq!(axes.sum(), 9);
        assert_eq!(axes.size(), ("L", 5));
        assert_eq!(set.axes(&a, &Facts { files: 1, modules: 1 }).unwrap().0[4], 0);
        assert_eq!(set.axes(&a, &Facts { files: 3, modules: 1 }).unwrap().0[4], 1);
        a.insert(R5_APP_WIDE.into(), Answer::Noul { noul: 0.7 });
        assert_eq!(set.axes(&a, &Facts { files: 1, modules: 1 }).unwrap().0[4], 3);
        a.remove(R4);
        assert!(set.axes(&a, &facts).unwrap_err().to_string().contains("r4_algorithmic"));
    }

    #[test]
    fn sizes() {
        let s = |n: u8| {
            Axes([
                n.min(3),
                n.saturating_sub(3).min(3),
                n.saturating_sub(6).min(3),
                n.saturating_sub(9).min(3),
                n.saturating_sub(12).min(3),
                n.saturating_sub(15).min(3),
            ])
        };
        assert_eq!(s(0).size(), ("XS", 1));
        assert_eq!(s(2).size(), ("XS", 1));
        assert_eq!(s(3).size(), ("S", 2));
        assert_eq!(s(8).size(), ("M", 3));
        assert_eq!(s(9).size(), ("L", 5));
        assert_eq!(s(14).size(), ("XL", 8));
        assert_eq!(s(18).size(), ("XXL", 13));
    }
}
