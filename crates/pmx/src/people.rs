//! `pmx people` (plan §3.2): every author identity across all repos, and proposed merges into
//! `[people]`. Cross-repo per-person metrics (multi-stack, active devs) depend on this.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::NaiveDate;
use pm_config::{Exclusion, Identity, People, Workspace};
use pm_git::{Git, LogRange};

/// One `(email, name)` pair as recorded in commits since `range_start`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ident {
    pub email: String,
    pub name: String,
    pub commits: u64,
    pub repos: BTreeSet<String>,
    pub last: NaiveDate,
}

impl Ident {
    pub fn label(&self) -> String {
        format!("{} <{}>", self.name, self.email)
    }
}

/// Identities in every repo that is on disk, at its configured revision. Returns warnings for
/// repos that could not be read.
pub fn gather(ws: &Workspace) -> (Vec<Ident>, Vec<String>) {
    let range_start = ws.config.project.range_start;
    let mut map: BTreeMap<(String, String), Ident> = BTreeMap::new();
    let mut warnings = Vec::new();
    for repo in &ws.config.repos {
        let name = repo.display_name();
        let dir = ws.repo_dir(repo);
        if !dir.exists() {
            warnings.push(format!("{name}: not on disk yet (run `pmx collect` to clone it)"));
            continue;
        }
        let git = Git::open(&dir);
        let rev = repo
            .revision()
            .map(String::from)
            .unwrap_or_else(|| git.default_revision());
        let stamps = match git.identities(&LogRange::new(&rev).committed_since(range_start)) {
            Ok(s) => s,
            Err(e) => {
                warnings.push(format!("{name}: {e}"));
                continue;
            }
        };
        for s in stamps.into_iter().filter(|s| s.date >= range_start) {
            let e = map
                .entry((s.email.to_lowercase(), s.name.clone()))
                .or_insert_with(|| Ident {
                    email: s.email.clone(),
                    name: s.name.clone(),
                    commits: 0,
                    repos: BTreeSet::new(),
                    last: s.date,
                });
            e.commits += 1;
            e.repos.insert(name.clone());
            e.last = e.last.max(s.date);
        }
    }
    (map.into_values().collect(), warnings)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// Already listed for this person.
    Mapped(String),
    /// Proposed: add to this (existing or new) person.
    Proposed(String),
    Bot,
    External,
}

pub struct Proposal {
    /// `[people]` after applying the proposals.
    pub people: People,
    pub rows: Vec<(Ident, Status)>,
    /// Unlisted identities that look automated (not excluded yet).
    pub bot_hints: Vec<Ident>,
}

impl Proposal {
    pub fn changes(&self) -> usize {
        self.rows
            .iter()
            .filter(|(_, s)| matches!(s, Status::Proposed(_)))
            .count()
    }
}

/// Lower-case alphanumerics only: `Jane Doe`, `jane.doe` and `JaneDoe` all become `janedoe`.
fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Keys that identify the same person: the email, the normalized name, a `first.last` email
/// local part, a GitHub noreply login.
fn keys(email: &str, name: &str) -> Vec<String> {
    let email = email.trim().to_lowercase();
    let mut k = vec![format!("e:{email}")];
    let n = norm(name);
    if n.chars().count() >= 3 {
        k.push(format!("n:{n}"));
    }
    if let Some((local, domain)) = email.split_once('@') {
        if domain == "users.noreply.github.com" {
            let login = local.split_once('+').map(|(_, l)| l).unwrap_or(local);
            k.push(format!("n:{}", norm(login)));
        } else if local.contains(['.', '_', '-']) {
            k.push(format!("n:{}", norm(local)));
        }
    }
    k
}

/// Whole-word hints in the name or the email's local part (`deploy-bot`, `CI Runner`, `build@…`).
fn looks_automated(i: &Ident) -> bool {
    const WORDS: &[&str] = &[
        "bot",
        "ci",
        "build",
        "deploy",
        "deployer",
        "automation",
        "runner",
        "pipeline",
        "release",
        "robot",
    ];
    let local = i.email.split('@').next().unwrap_or("");
    format!("{} {local}", i.name)
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| WORDS.contains(&w))
}

struct Dsu(Vec<usize>);

impl Dsu {
    fn find(&mut self, i: usize) -> usize {
        if self.0[i] != i {
            let r = self.find(self.0[i]);
            self.0[i] = r;
        }
        self.0[i]
    }
    fn union(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.0[b] = a;
        }
    }
}

pub fn propose(current: &People, idents: &[Ident]) -> Proposal {
    let identity = Identity::new(current);
    let mut rows: Vec<(Ident, Status)> = Vec::new();
    let mut open: Vec<&Ident> = Vec::new();
    for i in idents {
        let r = identity.resolve(&i.email, &i.name);
        let status = match (r.excluded, identity.mapped(&i.email)) {
            (Some(Exclusion::Bot), _) => Some(Status::Bot),
            (Some(Exclusion::External), _) => Some(Status::External),
            (None, Some(p)) => Some(Status::Mapped(p.to_string())),
            (None, None) => None,
        };
        match status {
            Some(s) => rows.push((i.clone(), s)),
            None => open.push(i),
        }
    }

    // Nodes: existing persons first, then open identities. Union on shared keys.
    let persons: Vec<&String> = current.persons.keys().collect();
    let n = persons.len() + open.len();
    let mut dsu = Dsu((0..n).collect());
    let mut by_key: HashMap<String, usize> = HashMap::new();
    let mut link = |dsu: &mut Dsu, key: String, node: usize| match by_key.get(&key) {
        Some(&other) => dsu.union(other, node),
        None => {
            by_key.insert(key, node);
        }
    };
    for (pi, p) in persons.iter().enumerate() {
        link(&mut dsu, format!("n:{}", norm(p)), pi);
        for id in &current.persons[*p] {
            for k in keys(id, "") {
                link(&mut dsu, k, pi);
            }
        }
    }
    for (oi, i) in open.iter().enumerate() {
        for k in keys(&i.email, &i.name) {
            link(&mut dsu, k, persons.len() + oi);
        }
    }

    // Name each group: the existing person in it (persons are roots or merge into the first), or
    // the most active full-looking name.
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for oi in 0..open.len() {
        let root = dsu.find(persons.len() + oi);
        groups.entry(root).or_default().push(oi);
    }
    let mut people = current.clone();
    let mut bot_hints = Vec::new();
    for (root, members) in groups {
        let existing = (0..persons.len())
            .find(|&pi| dsu.find(pi) == root)
            .map(|pi| persons[pi].clone());
        let canonical = existing.unwrap_or_else(|| {
            members
                .iter()
                .map(|&oi| open[oi])
                .max_by_key(|i| {
                    (
                        i.name.trim().contains(' '),
                        i.commits,
                        std::cmp::Reverse(i.name.clone()),
                    )
                })
                .map(|i| i.name.trim().to_string())
                .unwrap_or_default()
        });
        let ids = people.persons.entry(canonical.clone()).or_default();
        for &oi in &members {
            let i = open[oi];
            if !ids.iter().any(|x| x.eq_ignore_ascii_case(&i.email)) {
                ids.push(i.email.clone());
            }
            if looks_automated(i) {
                bot_hints.push(i.clone());
            }
            rows.push((i.clone(), Status::Proposed(canonical.clone())));
        }
    }
    rows.sort_by(|a, b| {
        person_of(&a.1)
            .cmp(&person_of(&b.1))
            .then(b.0.commits.cmp(&a.0.commits))
    });
    Proposal {
        people,
        rows,
        bot_hints,
    }
}

fn person_of(s: &Status) -> String {
    match s {
        Status::Mapped(p) | Status::Proposed(p) => format!("0{p}"),
        Status::External => "1".into(),
        Status::Bot => "2".into(),
    }
}

/// Move `ids` to `person` (creating it), removing them from anyone else.
pub fn merge(people: &mut People, person: &str, ids: &[String]) {
    for list in people.persons.values_mut() {
        list.retain(|x| !ids.iter().any(|i| i.eq_ignore_ascii_case(x)));
    }
    people.persons.retain(|_, v| !v.is_empty());
    let list = people.persons.entry(person.to_string()).or_default();
    for id in ids {
        if !list.iter().any(|x| x.eq_ignore_ascii_case(id)) {
            list.push(id.clone());
        }
    }
}

pub fn add_unique(list: &mut Vec<String>, items: &[String]) {
    for i in items {
        if !list.iter().any(|x| x.eq_ignore_ascii_case(i)) {
            list.push(i.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(email: &str, name: &str, commits: u64) -> Ident {
        Ident {
            email: email.into(),
            name: name.into(),
            commits,
            repos: BTreeSet::new(),
            last: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
        }
    }

    #[test]
    fn proposes_merges() {
        let mut current = People::default();
        current
            .persons
            .insert("Jane Doe".into(), vec!["jane@work.example".into()]);
        current.externals = vec!["Sam Contractor".into()];
        let idents = vec![
            id("jane@work.example", "Jane Doe", 40),
            id("jane.doe@home.example", "jane", 3),
            id("bob@work.example", "Bob Builder", 20),
            id("12345+bobbuilder@users.noreply.github.com", "bobbuilder", 5),
            id("b.builder@old.example", "bob", 1),
            id("pat@work.example", "Pat", 2),
            id("sam@agency.example", "Sam Contractor", 9),
            id(
                "49699333+dependabot[bot]@users.noreply.github.com",
                "dependabot[bot]",
                30,
            ),
            id("deploy@work.example", "Deploy Script", 4),
        ];
        let p = propose(&current, &idents);
        let status = |email: &str| p.rows.iter().find(|(i, _)| i.email == email).unwrap().1.clone();
        assert_eq!(status("jane@work.example"), Status::Mapped("Jane Doe".into()));
        // first.last local part matches the existing person's name.
        assert_eq!(status("jane.doe@home.example"), Status::Proposed("Jane Doe".into()));
        assert_eq!(
            status("12345+bobbuilder@users.noreply.github.com"),
            Status::Proposed("Bob Builder".into())
        );
        // Nothing ties "bob" <b.builder@…> to Bob Builder: it stays its own person for the user to merge.
        assert_eq!(status("b.builder@old.example"), Status::Proposed("bob".into()));
        assert_eq!(status("pat@work.example"), Status::Proposed("Pat".into()));
        assert_eq!(status("sam@agency.example"), Status::External);
        assert_eq!(status("49699333+dependabot[bot]@users.noreply.github.com"), Status::Bot);
        assert_eq!(
            p.people.persons["Jane Doe"],
            vec!["jane@work.example", "jane.doe@home.example"]
        );
        assert_eq!(p.people.persons["Bob Builder"].len(), 2);
        assert_eq!(p.bot_hints.len(), 1);
        assert_eq!(p.bot_hints[0].email, "deploy@work.example");
        assert_eq!(p.changes(), 6);
    }

    #[test]
    fn merge_moves_ids() {
        let mut people = People::default();
        people.persons.insert("A".into(), vec!["x@example.com".into()]);
        people.persons.insert("B".into(), vec!["y@example.com".into()]);
        merge(&mut people, "B", &["X@example.com".into()]);
        assert!(!people.persons.contains_key("A"));
        assert_eq!(people.persons["B"], vec!["y@example.com", "X@example.com"]);
    }
}
