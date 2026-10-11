//! Jev's question/answer schema (plan §6.1): the contract every provider fills.

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// One question about the state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Question {
    /// Yes or no, answered as the probability of yes.
    Noul { instructions: String },
    /// Levels `0..criteria.len()`, one anchor each.
    Score {
        instructions: String,
        criteria: Vec<String>,
    },
    /// One of the named options.
    Choice { instructions: String, criteria: Options },
}

impl Question {
    pub fn instructions(&self) -> &str {
        match self {
            Question::Noul { instructions }
            | Question::Score { instructions, .. }
            | Question::Choice { instructions, .. } => instructions,
        }
    }
}

/// Questions by id, in the order they are asked (a JSON object on the wire).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Questions(pub Vec<(String, Question)>);

impl Questions {
    pub fn get(&self, id: &str) -> Option<&Question> {
        self.0.iter().find(|(k, _)| k == id).map(|(_, q)| q)
    }

    pub fn iter(&self) -> impl Iterator<Item = &(String, Question)> {
        self.0.iter()
    }
}

impl Serialize for Questions {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(Some(self.0.len()))?;
        for (k, q) in &self.0 {
            m.serialize_entry(k, q)?;
        }
        m.end()
    }
}

impl<'de> Deserialize<'de> for Questions {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Questions;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a map of questions")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Questions, A::Error> {
                let mut out = Vec::new();
                while let Some((k, v)) = a.next_entry()? {
                    out.push((k, v));
                }
                Ok(Questions(out))
            }
        }
        d.deserialize_map(V)
    }
}

/// A choice question's options (name → description), in the order they are listed (a JSON object
/// on the wire).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Options(pub Vec<(String, String)>);

impl Options {
    pub fn contains_key(&self, name: &str) -> bool {
        self.0.iter().any(|(k, _)| k == name)
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.0.iter().map(|(k, _)| k)
    }

    pub fn iter(&self) -> impl Iterator<Item = &(String, String)> {
        self.0.iter()
    }
}

impl Serialize for Options {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(Some(self.0.len()))?;
        for (k, d) in &self.0 {
            m.serialize_entry(k, d)?;
        }
        m.end()
    }
}

impl<'de> Deserialize<'de> for Options {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Options;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a map of options")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Options, A::Error> {
                let mut out = Vec::new();
                while let Some((k, v)) = a.next_entry()? {
                    out.push((k, v));
                }
                Ok(Options(out))
            }
        }
        d.deserialize_map(V)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Request {
    /// The work package, as text or JSON.
    pub state: String,
    pub questions: Questions,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Answer {
    Noul {
        noul: f64,
    },
    Score {
        /// Expected level (Jev); the chosen level for single-answer providers.
        score: f64,
        /// Level (`"0"`, `"1"`, …) → probability.
        #[serde(default)]
        probabilities: BTreeMap<String, f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        confidence: Option<f64>,
    },
    Choice {
        choice: String,
        #[serde(default)]
        probabilities: BTreeMap<String, f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        confidence: Option<f64>,
    },
}

impl Answer {
    /// max(P, 1 − P) for yes/no; the chosen level's or option's probability otherwise.
    pub fn confidence(&self) -> Option<f64> {
        match self {
            Answer::Score { confidence, .. } | Answer::Choice { confidence, .. } => *confidence,
            Answer::Noul { noul } => Some(noul.max(1.0 - noul)),
        }
    }
}

/// Where probabilities and confidence come from (pmx extension, plan §6.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfidenceSource {
    Model,
    Logprobs,
    Sampled,
    /// The model wrote each probability in its answer (`claude-code`, `anthropic` under
    /// `pmx-prompt@2`). A model that only writes 0 or 1 degrades to plain yes/no.
    Stated,
    /// One answer, one-hot probabilities: confidence rules fall back to the plain answer.
    #[default]
    None,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Response {
    /// The model that answered, as the provider reports it.
    pub model: String,
    pub answers: BTreeMap<String, Answer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    #[serde(default)]
    pub confidence_source: ConfidenceSource,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jev_shapes_round_trip() {
        let req = r#"{"state":"s","questions":{
            "z_first":{"type":"score","instructions":"i","criteria":["a","b","c","d"]},
            "a_second":{"type":"noul","instructions":"n"},
            "kind":{"type":"choice","instructions":"k","criteria":{"fix":"f","feature":"g"}}}}"#;
        let r: Request = serde_json::from_str(req).unwrap();
        let ids: Vec<&str> = r.questions.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(ids, ["z_first", "a_second", "kind"], "asked in the given order");
        match r.questions.get("kind") {
            Some(Question::Choice { criteria, .. }) => {
                assert_eq!(
                    criteria.keys().collect::<Vec<_>>(),
                    ["fix", "feature"],
                    "options keep their order"
                )
            }
            q => panic!("{q:?}"),
        }
        let again: Request = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert_eq!(again, r);

        let resp = r#"{"model":"jev-1.13.0","answers":{
            "z_first":{"type":"score","score":1.2,"probabilities":{"0":0.1,"1":0.6,"2":0.3,"3":0},"confidence":0.55},
            "a_second":{"type":"noul","noul":0.91}},
            "usage":{"input_tokens":7412}}"#;
        let r: Response = serde_json::from_str(resp).unwrap();
        assert_eq!(r.confidence_source, ConfidenceSource::None);
        assert_eq!(r.answers["z_first"].confidence(), Some(0.55));
        assert!((r.answers["a_second"].confidence().unwrap() - 0.91).abs() < 1e-9);
    }
}
