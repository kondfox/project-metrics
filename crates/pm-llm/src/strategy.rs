//! How a classifier is asked (plan §6.2, §6.4): sampling k times for probabilities, one call per
//! question (Jev's isolation), and the confidence gate with its re-ask and fallback.

use std::collections::BTreeMap;

use crate::provider::Provider;
use crate::rubric::Reask;
use crate::schema::{Answer, ConfidenceSource, Question, Questions, Request, Response, Usage};
use crate::{Error, Result};

fn add_usage(total: &mut Option<Usage>, u: &Option<Usage>) {
    if let Some(u) = u {
        let t = total.get_or_insert_with(Usage::default);
        t.input_tokens += u.input_tokens;
        t.output_tokens = match (t.output_tokens, u.output_tokens) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
        };
    }
}

/// Ask `k` times; the share of each answer becomes its probability (`confidence_source = sampled`).
pub struct Sampled {
    pub inner: Box<dyn Provider>,
    pub k: u32,
}

impl Provider for Sampled {
    fn identity(&self) -> String {
        format!("{}:k{}", self.inner.identity(), self.k)
    }

    fn local(&self) -> bool {
        self.inner.local()
    }

    fn classify(&self, context: &str, req: &Request) -> Result<Response> {
        let runs: Vec<Response> = (0..self.k)
            .map(|_| self.inner.classify(context, req))
            .collect::<Result<_>>()?;
        let n = runs.len() as f64;
        let mut usage = None;
        for r in &runs {
            add_usage(&mut usage, &r.usage);
        }
        let mut answers = BTreeMap::new();
        for (id, q) in req.questions.iter() {
            let got: Vec<&Answer> = runs.iter().filter_map(|r| r.answers.get(id)).collect();
            if got.len() != runs.len() {
                return Err(Error::Answer(format!("no answer to `{id}`")));
            }
            let a = match q {
                Question::Noul { .. } => Answer::Noul {
                    noul: got
                        .iter()
                        .map(|a| match a {
                            Answer::Noul { noul } => *noul,
                            _ => 0.0,
                        })
                        .sum::<f64>()
                        / n,
                },
                Question::Score { criteria, .. } => {
                    let mut p: BTreeMap<String, f64> = (0..criteria.len()).map(|l| (l.to_string(), 0.0)).collect();
                    for a in &got {
                        if let Answer::Score { score, .. } = a {
                            *p.entry((score.round() as usize).to_string()).or_default() += 1.0 / n;
                        }
                    }
                    Answer::Score {
                        score: p.iter().map(|(l, x)| l.parse::<f64>().unwrap_or(0.0) * x).sum(),
                        confidence: Some(p.values().cloned().fold(0.0, f64::max)),
                        probabilities: p,
                    }
                }
                Question::Choice { .. } => {
                    let mut p: BTreeMap<String, f64> = BTreeMap::new();
                    for a in &got {
                        if let Answer::Choice { choice, .. } = a {
                            *p.entry(choice.clone()).or_default() += 1.0 / n;
                        }
                    }
                    let (top, share) = p
                        .iter()
                        .max_by(|a, b| a.1.total_cmp(b.1).then(b.0.cmp(a.0)))
                        .map(|(c, x)| (c.clone(), *x))
                        .unwrap_or_default();
                    Answer::Choice {
                        choice: top,
                        probabilities: p,
                        confidence: Some(share),
                    }
                }
            };
            answers.insert(id.clone(), a);
        }
        Ok(Response {
            model: runs[0].model.clone(),
            answers,
            usage,
            confidence_source: ConfidenceSource::Sampled,
        })
    }
}

/// One call per question, each with the same prompt prefix (`isolate_questions = true`).
pub struct Isolated {
    pub inner: Box<dyn Provider>,
}

impl Provider for Isolated {
    fn identity(&self) -> String {
        format!("{}:isolated", self.inner.identity())
    }

    fn local(&self) -> bool {
        self.inner.local()
    }

    fn classify(&self, context: &str, req: &Request) -> Result<Response> {
        let mut out: Option<Response> = None;
        for (id, q) in req.questions.iter() {
            let one = Request {
                state: req.state.clone(),
                questions: Questions(vec![(id.clone(), q.clone())]),
            };
            let r = self.inner.classify(context, &one)?;
            let a = r
                .answers
                .get(id)
                .cloned()
                .ok_or_else(|| Error::Answer(format!("no answer to `{id}`")))?;
            match &mut out {
                None => {
                    out = Some(Response {
                        answers: [(id.clone(), a)].into(),
                        ..r
                    })
                }
                Some(o) => {
                    o.answers.insert(id.clone(), a);
                    add_usage(&mut o.usage, &r.usage);
                }
            }
        }
        out.ok_or_else(|| Error::Answer("no questions".into()))
    }
}

/// The lowest confidence among the answers, when the provider's confidence means something.
pub fn min_confidence(r: &Response) -> Option<f64> {
    if r.confidence_source == ConfidenceSource::None {
        return None;
    }
    lowest_confidence(r)
}

/// The lowest confidence among the answers, whatever their source (`six-axis@3` gates every one).
pub fn lowest_confidence(r: &Response) -> Option<f64> {
    r.answers.values().filter_map(Answer::confidence).reduce(f64::min)
}

/// Two answers to the same request, averaged (`six-axis@3`'s re-ask): P(yes) and every option's or
/// level's probability are means; the choice is the option with the highest mean (the first
/// answer's on a tie).
pub fn average(req: &Request, a: &Response, b: &Response) -> Result<Response> {
    let mut answers = BTreeMap::new();
    for (id, _) in req.questions.iter() {
        let (x, y) = match (a.answers.get(id), b.answers.get(id)) {
            (Some(x), Some(y)) => (x, y),
            _ => return Err(Error::Answer(format!("no answer to `{id}`"))),
        };
        let mean = |p: &BTreeMap<String, f64>, q: &BTreeMap<String, f64>| -> BTreeMap<String, f64> {
            let mut m: BTreeMap<String, f64> = BTreeMap::new();
            for (k, v) in p.iter().chain(q) {
                *m.entry(k.clone()).or_default() += v / 2.0;
            }
            m
        };
        let avg = match (x, y) {
            (Answer::Noul { noul: p }, Answer::Noul { noul: q }) => Answer::Noul { noul: (p + q) / 2.0 },
            (
                Answer::Choice {
                    choice,
                    probabilities: p,
                    ..
                },
                Answer::Choice { probabilities: q, .. },
            ) => {
                let m = mean(p, q);
                let first = m.get(choice).copied().unwrap_or(0.0);
                let (top, share) = m
                    .iter()
                    .filter(|(_, v)| **v > first + 1e-12)
                    .max_by(|a, b| a.1.total_cmp(b.1))
                    .map(|(k, v)| (k.clone(), *v))
                    .unwrap_or((choice.clone(), first));
                Answer::Choice {
                    choice: top,
                    probabilities: m,
                    confidence: Some(share),
                }
            }
            (Answer::Score { probabilities: p, .. }, Answer::Score { probabilities: q, .. }) => {
                let m = mean(p, q);
                Answer::Score {
                    score: m.iter().map(|(l, x)| l.parse::<f64>().unwrap_or(0.0) * x).sum(),
                    confidence: Some(m.values().cloned().fold(0.0, f64::max)),
                    probabilities: m,
                }
            }
            _ => return Err(Error::Answer(format!("`{id}`: the two answers differ in type"))),
        };
        answers.insert(id.clone(), avg);
    }
    let mut usage = a.usage.clone();
    add_usage(&mut usage, &b.usage);
    Ok(Response {
        model: a.model.clone(),
        answers,
        usage,
        confidence_source: a.confidence_source,
    })
}

/// The same request with every Score question's levels listed top-down (Jev tends to favour the
/// first option).
pub fn reversed(req: &Request) -> Request {
    Request {
        state: req.state.clone(),
        questions: Questions(
            req.questions
                .iter()
                .map(|(id, q)| {
                    let q = match q {
                        Question::Score { instructions, criteria } => Question::Score {
                            instructions: instructions.clone(),
                            criteria: criteria.iter().rev().cloned().collect(),
                        },
                        other => other.clone(),
                    };
                    (id.clone(), q)
                })
                .collect(),
        ),
    }
}

/// Map answers to [`reversed`] back to the original level numbers.
pub fn unreverse(req: &Request, mut r: Response) -> Response {
    for (id, q) in req.questions.iter() {
        let Question::Score { criteria, .. } = q else { continue };
        let top = criteria.len().saturating_sub(1);
        if let Some(Answer::Score {
            score, probabilities, ..
        }) = r.answers.get_mut(id)
        {
            *score = top as f64 - *score;
            *probabilities = probabilities
                .iter()
                .filter_map(|(l, p)| Some(((top.checked_sub(l.parse().ok()?))?.to_string(), *p)))
                .collect();
        }
    }
    r
}

/// How one package was answered.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Gated {
    pub response: Response,
    /// Identity of the classifier whose answer was kept.
    pub classifier: String,
    /// Asked again: with the levels reordered (`@2`), or in a fresh call averaged with the first
    /// answer (`@3`).
    #[serde(default)]
    pub reasked: bool,
    /// Sent to the fallback classifier.
    #[serde(default)]
    pub escalated: bool,
    /// Still below the gate at the end (no fallback allowed or the fallback was unsure too).
    #[serde(default)]
    pub low_confidence: bool,
}

/// The confidence gate (plan §6.4, §6.4.1). Below `gate`, re-ask once: `Reordered` (`six-axis@2`)
/// lists the levels top-down and keeps the second answer, and never gates a provider without a
/// confidence (`confidence_source = none`); `Averaged` (`six-axis@3`) asks again in a fresh call
/// and averages the two, whatever the source. Still below, ask `fallback` (when given: the caller
/// has checked the data policy).
pub fn classify_gated(
    primary: &dyn Provider,
    fallback: Option<&dyn Provider>,
    gate: Option<f64>,
    reask: Reask,
    context: &str,
    req: &Request,
) -> Result<Gated> {
    let first = primary.classify(context, req)?;
    let confidence = |r: &Response| match reask {
        Reask::Reordered => min_confidence(r),
        Reask::Averaged => lowest_confidence(r),
    };
    let below = |r: &Response| gate.is_some_and(|g| confidence(r).is_some_and(|c| c < g - 1e-9));
    let done = |response: Response, by: &dyn Provider, reasked: bool, escalated: bool, low: bool| Gated {
        response,
        classifier: by.identity(),
        reasked,
        escalated,
        low_confidence: low,
    };
    if !below(&first) {
        return Ok(done(first, primary, false, false, false));
    }
    let second = match reask {
        Reask::Reordered => unreverse(req, primary.classify(context, &reversed(req))?),
        Reask::Averaged => average(req, &first, &primary.classify(context, req)?)?,
    };
    if !below(&second) {
        return Ok(done(second, primary, true, false, false));
    }
    if let Some(fb) = fallback {
        let third = fb.classify(context, req)?;
        let low = below(&third);
        return Ok(done(third, fb, true, true, low));
    }
    let best = match reask {
        // Keep the surer of the two.
        Reask::Reordered if min_confidence(&second) > min_confidence(&first) => second,
        Reask::Reordered => first,
        Reask::Averaged => second,
    };
    Ok(done(best, primary, true, false, true))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::rubric::QuestionSet;

    /// Answers from a queue; records the requests.
    struct Scripted {
        name: &'static str,
        answers: Mutex<Vec<Response>>,
        seen: Mutex<Vec<Request>>,
    }

    impl Scripted {
        fn new(name: &'static str, answers: Vec<Response>) -> Scripted {
            Scripted {
                name,
                answers: Mutex::new(answers),
                seen: Mutex::new(Vec::new()),
            }
        }
    }

    impl Provider for Scripted {
        fn identity(&self) -> String {
            self.name.into()
        }
        fn local(&self) -> bool {
            true
        }
        fn classify(&self, _: &str, req: &Request) -> Result<Response> {
            self.seen.lock().unwrap().push(req.clone());
            let mut a = self.answers.lock().unwrap();
            let mut r = if a.len() > 1 { a.remove(0) } else { a[0].clone() };
            r.answers.retain(|k, _| req.questions.get(k).is_some());
            Ok(r)
        }
    }

    fn score_q() -> Request {
        Request {
            state: "s".into(),
            questions: Questions(vec![
                (
                    "r".into(),
                    Question::Score {
                        instructions: "i".into(),
                        criteria: vec!["a".into(), "b".into(), "c".into(), "d".into()],
                    },
                ),
                (
                    "n".into(),
                    Question::Noul {
                        instructions: "y?".into(),
                    },
                ),
            ]),
        }
    }

    fn resp(level: f64, p: [f64; 4], conf: Option<f64>, noul: f64, src: ConfidenceSource) -> Response {
        Response {
            model: "m".into(),
            answers: [
                (
                    "r".to_string(),
                    Answer::Score {
                        score: level,
                        probabilities: (0..4).map(|i| (i.to_string(), p[i])).collect(),
                        confidence: conf,
                    },
                ),
                ("n".to_string(), Answer::Noul { noul }),
            ]
            .into(),
            usage: Some(Usage {
                input_tokens: 10,
                output_tokens: Some(1),
            }),
            confidence_source: src,
        }
    }

    #[test]
    fn samples_become_probabilities() {
        let one = |l: f64, noul: f64| resp(l, [0.0; 4], None, noul, ConfidenceSource::None);
        let s = Sampled {
            inner: Box::new(Scripted::new("x", vec![one(1.0, 1.0), one(2.0, 0.0), one(1.0, 0.5)])),
            k: 3,
        };
        let r = s.classify("", &score_q()).unwrap();
        assert_eq!(r.confidence_source, ConfidenceSource::Sampled);
        assert_eq!(r.usage.unwrap().input_tokens, 30);
        match &r.answers["r"] {
            Answer::Score {
                probabilities,
                confidence,
                ..
            } => {
                assert!((probabilities["1"] - 2.0 / 3.0).abs() < 1e-9);
                assert!((confidence.unwrap() - 2.0 / 3.0).abs() < 1e-9);
            }
            a => panic!("{a:?}"),
        }
        assert_eq!(r.answers["n"], Answer::Noul { noul: 0.5 });
        assert_eq!(s.identity(), "x:k3");
    }

    #[test]
    fn isolates_questions() {
        let inner = Scripted::new("x", vec![resp(2.0, [0.0; 4], None, 0.9, ConfidenceSource::None)]);
        let iso = Isolated { inner: Box::new(inner) };
        let r = iso.classify("", &score_q()).unwrap();
        assert_eq!(r.answers.len(), 2);
        assert_eq!(r.usage.unwrap().input_tokens, 20);
    }

    #[test]
    fn reverses_and_maps_back() {
        let req = score_q();
        let rev = reversed(&req);
        match rev.questions.get("r").unwrap() {
            Question::Score { criteria, .. } => assert_eq!(criteria[0], "d"),
            q => panic!("{q:?}"),
        }
        // Level 0 of the reversed list is level 3.
        let r = unreverse(
            &req,
            resp(0.0, [0.9, 0.1, 0.0, 0.0], Some(0.9), 0.5, ConfidenceSource::Model),
        );
        match &r.answers["r"] {
            Answer::Score {
                score, probabilities, ..
            } => {
                assert_eq!(*score, 3.0);
                assert_eq!(probabilities["3"], 0.9);
                assert_eq!(probabilities["2"], 0.1);
            }
            a => panic!("{a:?}"),
        }
    }

    #[test]
    fn gate_reasks_then_falls_back() {
        let unsure = resp(1.0, [0.3, 0.4, 0.3, 0.0], Some(0.4), 0.95, ConfidenceSource::Model);
        let sure = resp(1.0, [0.0, 0.9, 0.1, 0.0], Some(0.9), 0.95, ConfidenceSource::Model);
        let req = score_q();

        // Sure at once: one call.
        let p = Scripted::new("p", vec![sure.clone()]);
        let g = classify_gated(&p, None, Some(0.5), Reask::Reordered, "", &req).unwrap();
        assert!(!g.reasked && !g.escalated && !g.low_confidence);
        assert_eq!(p.seen.lock().unwrap().len(), 1);

        // Unsure, then sure with the levels reversed.
        let p = Scripted::new("p", vec![unsure.clone(), sure.clone()]);
        let g = classify_gated(&p, None, Some(0.5), Reask::Reordered, "", &req).unwrap();
        assert!(g.reasked && !g.escalated);
        match p.seen.lock().unwrap()[1].questions.get("r").unwrap() {
            Question::Score { criteria, .. } => assert_eq!(criteria[0], "d", "asked top-down"),
            q => panic!("{q:?}"),
        }

        // Unsure twice: the fallback answers.
        let p = Scripted::new("p", vec![unsure.clone()]);
        let fb = Scripted::new("fb", vec![sure.clone()]);
        let g = classify_gated(&p, Some(&fb), Some(0.5), Reask::Reordered, "", &req).unwrap();
        assert!(g.escalated && !g.low_confidence);
        assert_eq!(g.classifier, "fb");

        // No fallback: kept, flagged.
        let g = classify_gated(&p, None, Some(0.5), Reask::Reordered, "", &req).unwrap();
        assert!(g.low_confidence && g.classifier == "p");

        // No confidence to judge: never gated.
        let none = resp(1.0, [0.0, 1.0, 0.0, 0.0], None, 0.5, ConfidenceSource::None);
        let p = Scripted::new("p", vec![none]);
        let g = classify_gated(&p, None, Some(0.99), Reask::Reordered, "", &req).unwrap();
        assert!(!g.reasked);
    }

    #[test]
    fn bundled_rubric_survives_reversal() {
        let set = QuestionSet::v2();
        let req = set.request("s".into(), &[]);
        assert_eq!(reversed(&reversed(&req)), req);
    }

    fn choice_q() -> Request {
        Request {
            state: "s".into(),
            questions: Questions(vec![
                (
                    "n".into(),
                    Question::Noul {
                        instructions: "y?".into(),
                    },
                ),
                (
                    "file_1".into(),
                    Question::Choice {
                        instructions: "layer?".into(),
                        criteria: crate::schema::Options(vec![
                            ("ui".into(), "u".into()),
                            ("service".into(), "s".into()),
                        ]),
                    },
                ),
            ]),
        }
    }

    fn stated(noul: f64, choice: &str, p: f64, src: ConfidenceSource) -> Response {
        Response {
            model: "m".into(),
            answers: [
                ("n".to_string(), Answer::Noul { noul }),
                (
                    "file_1".to_string(),
                    Answer::Choice {
                        choice: choice.into(),
                        probabilities: [(choice.to_string(), p)].into(),
                        confidence: Some(p),
                    },
                ),
            ]
            .into(),
            usage: None,
            confidence_source: src,
        }
    }

    #[test]
    fn averages_two_answers() {
        let req = choice_q();
        let r = average(
            &req,
            &stated(0.6, "ui", 0.6, ConfidenceSource::Stated),
            &stated(0.9, "service", 0.8, ConfidenceSource::Stated),
        )
        .unwrap();
        assert!(matches!(r.answers["n"], Answer::Noul { noul } if (noul - 0.75).abs() < 1e-9));
        match &r.answers["file_1"] {
            Answer::Choice { choice, confidence, .. } => {
                assert_eq!(choice, "service");
                assert!((confidence.unwrap() - 0.4).abs() < 1e-9);
            }
            a => panic!("{a:?}"),
        }
        // The same option twice: its mean.
        let r = average(
            &req,
            &stated(0.6, "ui", 0.6, ConfidenceSource::Stated),
            &stated(0.6, "ui", 0.8, ConfidenceSource::Stated),
        )
        .unwrap();
        assert!(
            matches!(&r.answers["file_1"], Answer::Choice { choice, confidence: Some(c), .. } if choice == "ui" && (c - 0.7).abs() < 1e-9)
        );
    }

    #[test]
    fn averaged_gate_covers_every_source() {
        let req = choice_q();
        // Unsure yes/no (max(P, 1 − P) = 0.55), from a source without a confidence of its own.
        let unsure = stated(0.55, "ui", 0.9, ConfidenceSource::None);
        let sure = stated(0.95, "ui", 0.9, ConfidenceSource::None);
        let p = Scripted::new("p", vec![unsure.clone(), sure.clone()]);
        let g = classify_gated(&p, None, Some(0.7), Reask::Averaged, "", &req).unwrap();
        assert!(g.reasked && !g.low_confidence, "{g:?}");
        assert!(matches!(g.response.answers["n"], Answer::Noul { noul } if (noul - 0.75).abs() < 1e-9));
        let seen = p.seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[1], req, "the same request, a fresh call");
        drop(seen);

        // Still unsure: the fallback, then flagged when there is none.
        let p = Scripted::new("p", vec![unsure.clone()]);
        let fb = Scripted::new("fb", vec![sure.clone()]);
        let g = classify_gated(&p, Some(&fb), Some(0.7), Reask::Averaged, "", &req).unwrap();
        assert!(g.escalated && g.classifier == "fb" && !g.low_confidence);
        let g = classify_gated(&p, None, Some(0.7), Reask::Averaged, "", &req).unwrap();
        assert!(g.low_confidence && g.reasked);
        // No gate: one call.
        let p = Scripted::new("p", vec![unsure]);
        assert!(
            !classify_gated(&p, None, None, Reask::Averaged, "", &req)
                .unwrap()
                .reasked
        );
    }
}
