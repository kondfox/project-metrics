//! Who a commit author is, and whether they count (spec §1.3).

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;

use crate::People;

static BOT_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\[bot\]|dependabot|renovate|github-action|semantic-release|gitlab[ -]?ci|gitlab[ -]?runner|jenkins|buildkite|^claude$|anthropic",
    )
    .unwrap()
});

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exclusion {
    Bot,
    External,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Author {
    /// Canonical person: the `[people]` name the email maps to, else the author name as recorded.
    pub person: String,
    pub excluded: Option<Exclusion>,
}

/// Resolves raw `(email, name)` pairs against `[people]`.
#[derive(Clone, Debug, Default)]
pub struct Identity {
    by_id: HashMap<String, String>,
    bots: HashSet<String>,
    externals: HashSet<String>,
}

impl Identity {
    pub fn new(people: &People) -> Identity {
        let lower = |xs: &[String]| xs.iter().map(|x| x.trim().to_lowercase()).collect::<HashSet<_>>();
        let mut by_id = HashMap::new();
        for (person, ids) in &people.persons {
            for id in ids {
                by_id.insert(id.trim().to_lowercase(), person.clone());
            }
        }
        Identity {
            by_id,
            bots: lower(&people.bots),
            externals: lower(&people.externals),
        }
    }

    pub fn resolve(&self, email: &str, name: &str) -> Author {
        let e = email.trim().to_lowercase();
        let n = name.trim().to_lowercase();
        let person = self.by_id.get(&e).cloned().unwrap_or_else(|| name.to_string());
        let excluded = if self.bots.contains(&e)
            || self.bots.contains(&n)
            || BOT_PATTERN.is_match(name.trim())
            || BOT_PATTERN.is_match(email.trim())
        {
            Some(Exclusion::Bot)
        } else if self.externals.contains(&e)
            || self.externals.contains(&n)
            || self.externals.contains(&person.trim().to_lowercase())
        {
            Some(Exclusion::External)
        } else {
            None
        };
        Author { person, excluded }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn people() -> People {
        let mut p = People::default();
        p.persons.insert(
            "Jane Doe".into(),
            vec!["jane@example.com".into(), "Jane.Doe@Corp.example".into(), "jdoe".into()],
        );
        p.persons
            .insert("Sam Contractor".into(), vec!["sam@agency.example".into()]);
        p.bots = vec!["ci@example.com".into()];
        p.externals = vec!["Sam Contractor".into(), "pat@outside.example".into()];
        p
    }

    #[test]
    fn merges_identities() {
        let id = Identity::new(&people());
        assert_eq!(id.resolve("JANE.DOE@corp.example", "jd").person, "Jane Doe");
        assert_eq!(id.resolve("jane@example.com", "Jane").excluded, None);
        assert_eq!(id.resolve("other@example.com", "Other Dev").person, "Other Dev");
    }

    #[test]
    fn bots() {
        let id = Identity::new(&people());
        for (e, n) in [
            ("ci@example.com", "CI"),
            ("49699333+dependabot[bot]@users.noreply.github.com", "dependabot[bot]"),
            ("runner@example.com", "GitLab Runner"),
            ("x@example.com", " Claude "),
            ("noreply@anthropic.com", "Assistant"),
            ("x@example.com", "renovate"),
        ] {
            assert_eq!(id.resolve(e, n).excluded, Some(Exclusion::Bot), "{e} {n}");
        }
        assert_eq!(id.resolve("claude.monet@example.com", "Claude Monet").excluded, None);
    }

    #[test]
    fn externals_by_name_email_or_canonical_name() {
        let id = Identity::new(&people());
        assert_eq!(
            id.resolve("sam@agency.example", "S").excluded,
            Some(Exclusion::External)
        );
        assert_eq!(
            id.resolve("x@y.example", "sam contractor").excluded,
            Some(Exclusion::External)
        );
        assert_eq!(
            id.resolve("PAT@outside.example", "Pat").excluded,
            Some(Exclusion::External)
        );
    }
}
