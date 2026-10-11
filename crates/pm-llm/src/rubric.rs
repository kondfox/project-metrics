//! The six-axis rubric as a versioned question set, and the code that turns its answers into
//! R1–R6 (plan §6.4, §6.4.1): counting and arithmetic stay here, never in the model.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::prompt::Template;
use crate::schema::{Answer, Options, Question, Questions, Request};
use crate::{Error, Result};

/// `six-axis@2` (anchors from `rubrics/six-axis@1.md`, unchanged). Bundled so a pmx binary always
/// scores with the version it reports; kept so `@2` answers stay readable.
pub const SIX_AXIS_V2: &str = include_str!("../rubrics/six-axis@2.toml");
/// `six-axis@3` (plan D36, wiki velocity-metrics.md).
pub const SIX_AXIS_V3: &str = include_str!("../rubrics/six-axis@3.toml");
/// The rubric `collect` scores with unless `[velocity] rubric` names another.
pub const DEFAULT_RUBRIC: &str = "six-axis@3";
/// Every bundled rubric id.
pub const RUBRICS: [&str; 2] = ["six-axis@2", "six-axis@3"];

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

/// `six-axis@3`: the yes/no anchors of R2–R6, levels 1–3 each.
pub const ANCHORS: [[&str; 3]; 5] = [
    ["r2_1", "r2_2", "r2_3"],
    ["r3_1", "r3_2", "r3_3"],
    ["r4_1", "r4_2", "r4_3"],
    ["r5_1", "r5_2", "r5_3"],
    ["r6_1", "r6_2", "r6_3"],
];
/// `six-axis@3`: the layer option that is no layer.
pub const NO_LAYER: &str = "none";

/// The id of the layer question about the `n`-th asked file (1-based).
pub fn file_question(n: usize) -> String {
    format!("file_{n}")
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    id: String,
    #[serde(default)]
    template: Option<String>,
    #[serde(default)]
    delta: Option<f64>,
    #[serde(default)]
    tau: Option<f64>,
    #[serde(default)]
    max_layer_files: Option<usize>,
    context: String,
    question: Vec<Entry>,
    #[serde(default)]
    layer: Option<LayerFileEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LayerFileEntry {
    instructions: String,
    options: Vec<(String, String)>,
}

#[derive(Deserialize)]
struct Entry {
    id: String,
    #[serde(flatten)]
    question: Question,
}

/// How answers become levels.
#[derive(Clone, Debug, PartialEq)]
pub enum Scheme {
    /// `six-axis@2`: per-layer yes/no for R1, level questions for R2/R3/R4/R6 with "take the lower
    /// within `delta`", R5 from file and module counts plus one yes/no.
    Levels { delta: f64 },
    /// `six-axis@3`: R2–R6 = the highest anchor with P(yes) ≥ `tau`; R1 = distinct layers of a
    /// per-file choice, asked for at most `max_files` files.
    Anchors {
        tau: f64,
        max_files: usize,
        /// The per-file question; `{repo}` and `{path}` are filled in.
        instructions: String,
        options: Options,
    },
}

/// How the confidence gate re-asks (plan §6.4, §6.4.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reask {
    /// `six-axis@2`: the Score levels reordered; providers without a confidence are never gated.
    Reordered,
    /// `six-axis@3`: the same request in a fresh call, the two answers averaged; every source.
    Averaged,
}

/// One changed file, as the question set needs it (generated files already left out).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayerFile {
    pub repo: String,
    pub path: String,
    /// Class `prod` or `other`: asked for a layer. `test` and `doc` files are `none` unasked.
    pub asked: bool,
    /// Added plus deleted lines: the most changed files are asked first.
    pub lines: u64,
}

impl LayerFile {
    pub fn new(repo: &str, path: &str, asked: bool, lines: u64) -> LayerFile {
        LayerFile {
            repo: repo.into(),
            path: path.into(),
            asked,
            lines,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuestionSet {
    /// e.g. `six-axis@3`; part of the answer cache key.
    pub id: String,
    /// The prompt template the set is written for (generative providers).
    pub template: Template,
    pub scheme: Scheme,
    /// Shared instructions, sent before the state.
    pub context: String,
    /// The questions every package gets (`@3` adds one per file, see [`QuestionSet::request`]).
    pub questions: Questions,
}

impl QuestionSet {
    pub fn parse(text: &str) -> Result<QuestionSet> {
        let f: File = toml::from_str(text).map_err(|e| Error::Rubric(e.to_string()))?;
        let bad = |m: String| Error::Rubric(format!("{}: {m}", f.id));
        let template = match &f.template {
            Some(t) => Template::parse(t)?,
            None => Template::V1,
        };
        let scheme = match (f.delta, f.tau, &f.layer) {
            (Some(delta), None, None) => Scheme::Levels { delta },
            (None, Some(tau), Some(l)) => {
                let options = Options(l.options.clone());
                if !options.contains_key(NO_LAYER) {
                    return Err(bad(format!("the layer options need `{NO_LAYER}`")));
                }
                Scheme::Anchors {
                    tau,
                    max_files: f
                        .max_layer_files
                        .ok_or_else(|| bad("`max_layer_files` is missing".into()))?,
                    instructions: l.instructions.clone(),
                    options,
                }
            }
            _ => return Err(bad("either `delta` (levels) or `tau` with a `[layer]` (anchors)".into())),
        };
        let set = QuestionSet {
            id: f.id.clone(),
            template,
            scheme,
            context: f.context.trim().to_string(),
            questions: Questions(f.question.into_iter().map(|e| (e.id, e.question)).collect()),
        };
        let required: Vec<&str> = match set.scheme {
            Scheme::Levels { .. } => LAYERS.iter().chain(&[R2, R3, R4, R5_APP_WIDE, R6]).copied().collect(),
            Scheme::Anchors { .. } => ANCHORS.iter().flatten().copied().collect(),
        };
        for id in required {
            match (set.questions.get(id), &set.scheme) {
                (None, _) => return Err(bad(format!("question `{id}` is missing"))),
                (Some(Question::Noul { .. }), Scheme::Anchors { .. }) | (Some(_), Scheme::Levels { .. }) => {}
                (Some(_), Scheme::Anchors { .. }) => return Err(bad(format!("`{id}` must be a yes/no question"))),
            }
        }
        Ok(set)
    }

    /// A bundled rubric by id.
    pub fn bundled(id: &str) -> Result<QuestionSet> {
        match id {
            "six-axis@2" => QuestionSet::parse(SIX_AXIS_V2),
            "six-axis@3" => QuestionSet::parse(SIX_AXIS_V3),
            other => Err(Error::Rubric(format!(
                "unknown rubric `{other}` (bundled: {})",
                RUBRICS.join(", ")
            ))),
        }
    }

    pub fn v2() -> QuestionSet {
        QuestionSet::parse(SIX_AXIS_V2).expect("the bundled rubric parses")
    }

    pub fn v3() -> QuestionSet {
        QuestionSet::parse(SIX_AXIS_V3).expect("the bundled rubric parses")
    }

    /// The rubric `collect` uses by default.
    pub fn six_axis() -> QuestionSet {
        QuestionSet::bundled(DEFAULT_RUBRIC).expect("the bundled rubric parses")
    }

    pub fn reask(&self) -> Reask {
        match self.scheme {
            Scheme::Levels { .. } => Reask::Reordered,
            Scheme::Anchors { .. } => Reask::Averaged,
        }
    }

    /// The files asked for a layer, most changed lines first (ties keep their order), and how many
    /// eligible files were left unasked (unknown). Empty under `@2`.
    pub fn layer_files<'f>(&self, files: &'f [LayerFile]) -> (Vec<&'f LayerFile>, u32) {
        let Scheme::Anchors { max_files, .. } = &self.scheme else {
            return (Vec::new(), 0);
        };
        let mut asked: Vec<&LayerFile> = files.iter().filter(|f| f.asked).collect();
        asked.sort_by_key(|f| std::cmp::Reverse(f.lines));
        let unknown = asked.len().saturating_sub(*max_files) as u32;
        asked.truncate(*max_files);
        (asked, unknown)
    }

    /// The request for one package: the set's questions, plus (`@3`) one layer choice per asked file.
    pub fn request(&self, state: String, files: &[LayerFile]) -> Request {
        let mut questions = self.questions.clone();
        if let Scheme::Anchors {
            instructions, options, ..
        } = &self.scheme
        {
            for (i, f) in self.layer_files(files).0.into_iter().enumerate() {
                questions.0.push((
                    file_question(i + 1),
                    Question::Choice {
                        instructions: instructions.replace("{repo}", &f.repo).replace("{path}", &f.path),
                        criteria: options.clone(),
                    },
                ));
            }
        }
        Request { state, questions }
    }

    /// R1–R6 from the answers and the counts measured in code.
    pub fn axes(&self, answers: &BTreeMap<String, Answer>, facts: &Facts) -> Result<Axes> {
        let get = |id: &str| {
            answers
                .get(id)
                .ok_or_else(|| Error::Answer(format!("no answer to `{id}`")))
        };
        let p_yes = |id: &str| -> Result<f64> {
            match get(id)? {
                Answer::Noul { noul } => Ok(*noul),
                _ => Err(Error::Answer(format!("`{id}` is not a yes/no answer"))),
            }
        };
        let tau = match &self.scheme {
            Scheme::Levels { delta } => return self.axes_v2(answers, facts, *delta),
            Scheme::Anchors { tau, .. } => *tau,
        };
        let mut levels = [0u8; 5];
        for (axis, ids) in ANCHORS.iter().enumerate() {
            for (k, id) in ids.iter().enumerate() {
                // Small epsilon: probabilities come from JSON and are rarely exact.
                if p_yes(id)? >= tau - 1e-9 {
                    levels[axis] = k as u8 + 1;
                }
            }
        }
        let mut layers = BTreeSet::new();
        for n in 1..=facts.layer_files as usize {
            let id = file_question(n);
            match get(&id)? {
                Answer::Choice { choice, .. } => {
                    if choice != NO_LAYER {
                        layers.insert(choice.as_str());
                    }
                }
                _ => return Err(Error::Answer(format!("`{id}` is not a choice answer"))),
            }
        }
        let [r2, r3, r4, r5, r6] = levels;
        Ok(Axes([layer_score(layers.len() as u32), r2, r3, r4, r5, r6]))
    }

    fn axes_v2(&self, answers: &BTreeMap<String, Answer>, facts: &Facts, delta: f64) -> Result<Axes> {
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
                } => Ok(take_lower(*score, probabilities, levels, delta)),
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

/// Counts measured in code.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Facts {
    /// Changed files, generated ones excluded (`@2` R5).
    pub files: u32,
    /// Distinct modules among them, repo plus the first two directories (`@2` R5).
    pub modules: u32,
    /// Files asked for a layer, `file_1` … `file_<n>` (`@3` R1).
    #[serde(default)]
    pub layer_files: u32,
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
    fn bundled_rubrics_parse() {
        let s = QuestionSet::v2();
        assert_eq!(s.id, "six-axis@2");
        assert_eq!(s.template, Template::V1);
        assert_eq!(s.questions.0.len(), 13);
        assert!(matches!(s.questions.get(R2), Some(Question::Score { criteria, .. }) if criteria.len() == 4));
        let s = QuestionSet::six_axis();
        assert_eq!(s.id, "six-axis@3");
        assert_eq!(s.template, Template::V2);
        assert_eq!(s.reask(), Reask::Averaged);
        assert_eq!(s.questions.0.len(), 15);
        match &s.scheme {
            Scheme::Anchors {
                tau,
                max_files,
                options,
                ..
            } => {
                assert_eq!((*tau, *max_files), (0.65, 40));
                assert_eq!(options.keys().next().unwrap(), "transport");
                assert_eq!(options.0.len(), 9);
            }
            other => panic!("{other:?}"),
        }
        assert!(
            QuestionSet::bundled("six-axis@9")
                .unwrap_err()
                .to_string()
                .contains("six-axis@2, six-axis@3")
        );
    }

    #[test]
    fn layer_files_most_changed_first() {
        let set = QuestionSet::v3();
        let mut files: Vec<LayerFile> = (0..45)
            .map(|i| LayerFile::new("r", &format!("f{i}"), true, i))
            .collect();
        files.push(LayerFile::new("r", "t.test.ts", false, 999));
        let (asked, unknown) = set.layer_files(&files);
        assert_eq!((asked.len(), unknown), (40, 5));
        assert_eq!(asked[0].path, "f44");
        let req = set.request("s".into(), &files);
        assert_eq!(req.questions.0.len(), 55);
        assert_eq!(
            req.questions.get("file_1").unwrap().instructions(),
            "Which architectural layer does the changed file `r/f44` belong to?"
        );
        assert!(req.questions.get("file_41").is_none());
        // `@2` asks no file.
        assert_eq!(QuestionSet::v2().request("s".into(), &files).questions.0.len(), 13);
    }

    /// `@3` answers: the anchors that are yes (P = `p`), the others at 0.1, and the files' layers.
    fn v3_answers(yes: &[&str], p: f64, layers: &[&str]) -> (BTreeMap<String, Answer>, Facts) {
        let mut a = BTreeMap::new();
        for id in ANCHORS.iter().flatten() {
            let noul = if yes.contains(id) { p } else { 0.1 };
            a.insert(id.to_string(), Answer::Noul { noul });
        }
        for (i, l) in layers.iter().enumerate() {
            a.insert(
                file_question(i + 1),
                Answer::Choice {
                    choice: l.to_string(),
                    probabilities: [(l.to_string(), 0.9)].into(),
                    confidence: Some(0.9),
                },
            );
        }
        let facts = Facts {
            layer_files: layers.len() as u32,
            ..Default::default()
        };
        (a, facts)
    }

    #[test]
    fn worked_examples_score_as_documented() {
        let set = QuestionSet::v3();
        // 1. Nickname on a customer.
        let (a, f) = v3_answers(
            &["r2_1", "r3_1", "r5_1", "r6_1"],
            0.9,
            &["database", "persistence", "transport", "ui"],
        );
        let axes = set.axes(&a, &f).unwrap();
        assert_eq!(axes, Axes([2, 1, 1, 0, 1, 1]));
        assert_eq!(axes.size(), ("M", 3));
        // 2. Due-date ordering everywhere (the test file is `none` unasked).
        let (a, f) = v3_answers(&["r4_1", "r4_2", "r5_3", "r6_1", "r6_2"], 0.9, &["persistence"]);
        let axes = set.axes(&a, &f).unwrap();
        assert_eq!(axes, Axes([0, 0, 0, 2, 3, 2]));
        assert_eq!(axes.size(), ("M", 3));
        // 3. Typo in a button label.
        let (a, f) = v3_answers(&[], 0.9, &["ui"]);
        assert_eq!(set.axes(&a, &f).unwrap().size(), ("XS", 1));
    }

    #[test]
    fn highest_clear_yes() {
        let set = QuestionSet::v3();
        // Between 0.5 and τ: ambiguous, does not raise the level.
        let (a, f) = v3_answers(&["r2_1", "r2_2"], 0.6, &[]);
        assert_eq!(set.axes(&a, &f).unwrap().0[1], 0);
        let (a, f) = v3_answers(&["r2_1", "r2_2"], 0.65, &[]);
        assert_eq!(set.axes(&a, &f).unwrap().0[1], 2);
        // The highest clear yes counts, even when a lower anchor is no.
        let (a, f) = v3_answers(&["r3_3"], 0.8, &[]);
        assert_eq!(set.axes(&a, &f).unwrap().0[2], 3);
        // A 0/1 answer works as plain yes/no.
        let (a, f) = v3_answers(&["r6_2"], 1.0, &[]);
        assert_eq!(set.axes(&a, &f).unwrap().0[5], 2);
        // R1: distinct layers, `none` excluded; answers beyond the asked files are ignored.
        let (mut a, mut f) = v3_answers(&[], 0.9, &["ui", "ui", "none", "service", "domain", "persistence"]);
        assert_eq!(set.axes(&a, &f).unwrap().0[0], 2);
        f.layer_files = 2;
        assert_eq!(set.axes(&a, &f).unwrap().0[0], 0);
        f.layer_files = 7;
        assert!(set.axes(&a, &f).unwrap_err().to_string().contains("file_7"));
        a.remove("r4_2");
        f.layer_files = 0;
        assert!(set.axes(&a, &f).unwrap_err().to_string().contains("r4_2"));
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
    fn axes_from_answers_v2() {
        let set = QuestionSet::v2();
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
        let facts = Facts {
            files: 5,
            modules: 2,
            layer_files: 0,
        };
        let axes = set.axes(&a, &facts).unwrap();
        assert_eq!(axes, Axes([2, 1, 2, 0, 2, 2]));
        assert_eq!(axes.sum(), 9);
        assert_eq!(axes.size(), ("L", 5));
        assert_eq!(
            set.axes(
                &a,
                &Facts {
                    files: 1,
                    modules: 1,
                    layer_files: 0
                }
            )
            .unwrap()
            .0[4],
            0
        );
        assert_eq!(
            set.axes(
                &a,
                &Facts {
                    files: 3,
                    modules: 1,
                    layer_files: 0
                }
            )
            .unwrap()
            .0[4],
            1
        );
        a.insert(R5_APP_WIDE.into(), Answer::Noul { noul: 0.7 });
        assert_eq!(
            set.axes(
                &a,
                &Facts {
                    files: 1,
                    modules: 1,
                    layer_files: 0
                }
            )
            .unwrap()
            .0[4],
            3
        );
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
