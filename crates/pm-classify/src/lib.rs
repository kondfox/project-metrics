//! Stack-agnostic, path-based file classification (metrics spec §1.4–1.5).
//!
//! Everything here is a pure function of the path. Paths are repo-relative with `/` separators,
//! as git prints them.

use std::fmt;
use std::str::FromStr;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

const LOCKFILES: &[&str] = &[
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "gradle.lockfile",
    "composer.lock",
];

const GENERATED_EXTS: &[&str] = &[
    ".png",
    ".jpg",
    ".jpeg",
    ".gif",
    ".svg",
    ".ico",
    ".webp",
    ".pdf",
    ".woff",
    ".woff2",
    ".ttf",
    ".eot",
    ".otf",
    ".map",
    ".snap",
    ".lock",
    ".class",
    ".jar",
    ".keystore",
    ".pem",
    ".crt",
    ".key",
    ".pfx",
    ".p12",
    ".xlsx",
    ".zip",
    ".mp4",
    ".mp3",
];

const GENERATED_DIRS: &[&str] = &[
    "/node_modules/",
    "/dist/",
    "/build/",
    "/.gradle/",
    "/generated/",
    "/vendor/",
    "/.next/",
    "/coverage/",
    "/__snapshots__/",
    "/.idea/",
    "/out/",
];

/// Source extension → technology (spec §1.5). The key set is also the definition of "source".
const TECH_BY_EXT: &[(&str, &str)] = &[
    (".ts", "TypeScript"),
    (".mts", "TypeScript"),
    (".cts", "TypeScript"),
    (".tsx", "React"),
    (".jsx", "React"),
    (".js", "JavaScript"),
    (".mjs", "JavaScript"),
    (".cjs", "JavaScript"),
    (".kt", "Kotlin"),
    (".kts", "Kotlin"),
    (".gradle", "Gradle"),
    (".java", "Java"),
    (".swift", "Swift"),
    (".dart", "Dart"),
    (".py", "Python"),
    (".go", "Go"),
    (".rb", "Ruby"),
    (".cs", "C#"),
    (".php", "PHP"),
    (".rs", "Rust"),
    (".css", "CSS"),
    (".scss", "CSS"),
    (".less", "CSS"),
    (".vue", "Vue"),
    (".svelte", "Svelte"),
    (".sql", "SQL"),
    (".tf", "Terraform"),
    (".tfvars", "Terraform"),
    (".sh", "Shell"),
    (".hbs", "Handlebars"),
    (".c", "C"),
    (".h", "C"),
    (".cpp", "C++"),
    (".m", "Objective-C"),
    (".mm", "Objective-C"),
];

const TEST_DIRS: &[&str] = &[
    "/test/",
    "/tests/",
    "/__tests__/",
    "/spec/",
    "/e2e/",
    "/cypress/",
    "/playwright/",
    "/androidtest/",
    "/integration_test/",
    "/test_driver/",
    "/uitests/",
];

static DOTNET_TEST_DIR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"/[^/]*\.(unit|integration|e2e)?tests?/").unwrap());

/// Which test-detection rules to apply.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TestRules {
    /// The spec's rules (§1.4).
    #[default]
    Spec,
    /// The private prototype's narrower rules. Used only by the parity harness to check the rest of
    /// the pipeline exactly; never selectable from `pmx.toml`.
    Prototype,
}

/// Class of a changed path (spec §1.4), evaluated in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileClass {
    Generated,
    Doc,
    Test,
    Prod,
    /// Not generated, not doc, not a source extension (config, html, json, …).
    Other,
}

/// Stack role (spec §1.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Frontend,
    Backend,
    Mobile,
    Qa,
    Infra,
    Data,
    Docs,
}

impl Role {
    pub const ALL: [Role; 7] = [
        Role::Frontend,
        Role::Backend,
        Role::Mobile,
        Role::Qa,
        Role::Infra,
        Role::Data,
        Role::Docs,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Frontend => "frontend",
            Role::Backend => "backend",
            Role::Mobile => "mobile",
            Role::Qa => "qa",
            Role::Infra => "infra",
            Role::Data => "data",
            Role::Docs => "docs",
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Role {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Role::ALL
            .into_iter()
            .find(|r| r.as_str() == s)
            .ok_or_else(|| format!("unknown role `{s}` (expected frontend, backend, mobile, qa, infra, data or docs)"))
    }
}

/// Path hints for one per-file role: a file matches if its extension is listed or the lower-cased
/// path (with a leading `/`) contains one of the substrings.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleHints {
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub paths: Vec<String>,
}

impl RoleHints {
    fn matches(&self, lower_slashed: &str, ext: &str) -> bool {
        self.extensions.iter().any(|e| e == ext) || self.paths.iter().any(|p| lower_slashed.contains(p.as_str()))
    }
}

/// Per-file role rules for `role = "per-file"` repos (spec §1.5). First match wins:
/// test → qa, then data, infra, frontend; everything else is backend.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleRules {
    pub data: RoleHints,
    pub infra: RoleHints,
    pub frontend: RoleHints,
}

fn strings(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| s.to_string()).collect()
}

impl Default for RoleRules {
    fn default() -> Self {
        RoleRules {
            data: RoleHints {
                extensions: strings(&[".sql"]),
                paths: strings(&["/hex_queries/", "/dataflow/", "/sql/", "bigquery"]),
            },
            infra: RoleHints {
                extensions: strings(&[".tf", ".tfvars"]),
                paths: strings(&["/infrastructure/", "/environment/", "/terraform/", "/scripts/"]),
            },
            frontend: RoleHints {
                extensions: strings(&[".tsx", ".jsx", ".css", ".scss", ".less", ".hbs", ".vue", ".svelte"]),
                paths: strings(&["/ui-react/", "/app-ui-react/", "/delivery/", "web-ssr"]),
            },
        }
    }
}

/// Extension of the last path segment, including the dot, or `""`. Expects a lower-cased path.
pub fn ext_of(path: &str) -> &str {
    match (path.rfind('.'), path.rfind('/')) {
        (Some(dot), Some(slash)) if dot > slash => &path[dot..],
        (Some(dot), None) => &path[dot..],
        _ => "",
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn slashed_lower(path: &str) -> String {
    let mut s = String::with_capacity(path.len() + 1);
    s.push('/');
    s.push_str(&path.to_lowercase());
    s
}

/// The classifier: test rules plus per-file role rules.
#[derive(Clone, Debug, Default)]
pub struct Classifier {
    pub tests: TestRules,
    pub roles: RoleRules,
}

impl Classifier {
    pub fn new(tests: TestRules, roles: RoleRules) -> Self {
        Classifier { tests, roles }
    }

    pub fn is_generated(&self, path: &str) -> bool {
        let pl = slashed_lower(path);
        LOCKFILES.contains(&file_name(&pl))
            || GENERATED_EXTS.contains(&ext_of(&pl))
            || GENERATED_DIRS.iter().any(|d| pl.contains(d))
    }

    /// Doc rule only (no generated check): `.md`/`.mdx`, any `docs/` segment, or a `readme*` file.
    fn matches_doc(&self, path: &str) -> bool {
        let pl = slashed_lower(path);
        matches!(ext_of(&pl), ".md" | ".mdx") || pl.contains("/docs/") || file_name(&pl).starts_with("readme")
    }

    /// Test rule only (no generated or source check).
    pub fn matches_test(&self, path: &str) -> bool {
        match self.tests {
            TestRules::Spec => is_test_spec(path),
            TestRules::Prototype => is_test_prototype(path),
        }
    }

    pub fn is_doc(&self, path: &str) -> bool {
        !self.is_generated(path) && self.matches_doc(path)
    }

    pub fn is_source(&self, path: &str) -> bool {
        !self.is_generated(path)
            && !self.matches_doc(path)
            && TECH_BY_EXT.iter().any(|(e, _)| *e == ext_of(&path.to_lowercase()))
    }

    pub fn classify(&self, path: &str) -> FileClass {
        if self.is_generated(path) {
            FileClass::Generated
        } else if self.matches_doc(path) {
            FileClass::Doc
        } else if !TECH_BY_EXT.iter().any(|(e, _)| *e == ext_of(&path.to_lowercase())) {
            FileClass::Other
        } else if self.matches_test(path) {
            FileClass::Test
        } else {
            FileClass::Prod
        }
    }

    /// Does this changed path count as a test file for tests-with-code (spec §3.2)? Any
    /// non-generated path matching the test rules, so `.feature` specs and test fixtures count.
    /// The prototype skipped the generated check.
    pub fn touches_test(&self, path: &str) -> bool {
        match self.tests {
            TestRules::Spec => !self.is_generated(path) && self.matches_test(path),
            TestRules::Prototype => self.matches_test(path),
        }
    }

    /// Does this changed path count as a doc file for docs-with-code (spec §3.3)?
    pub fn touches_doc(&self, path: &str) -> bool {
        match self.tests {
            TestRules::Spec => self.is_doc(path),
            TestRules::Prototype => self.matches_doc(path),
        }
    }

    /// Technology of a source file, `None` for anything else.
    pub fn technology(&self, path: &str) -> Option<&'static str> {
        if !self.is_source(path) {
            return None;
        }
        let lower = path.to_lowercase();
        let ext = ext_of(&lower);
        TECH_BY_EXT.iter().find(|(e, _)| *e == ext).map(|(_, t)| *t)
    }

    /// Role of a source file in a `per-file` repo.
    pub fn role_of_file(&self, path: &str) -> Role {
        if self.matches_test(path) {
            return Role::Qa;
        }
        let pl = slashed_lower(path);
        let ext = ext_of(&pl);
        if self.roles.data.matches(&pl, ext) {
            Role::Data
        } else if self.roles.infra.matches(&pl, ext) {
            Role::Infra
        } else if self.roles.frontend.matches(&pl, ext) {
            Role::Frontend
        } else {
            Role::Backend
        }
    }
}

fn is_test_spec(path: &str) -> bool {
    let pl = slashed_lower(path);
    if TEST_DIRS.iter().any(|d| pl.contains(d)) || DOTNET_TEST_DIR.is_match(&pl) {
        return true;
    }
    let name = file_name(&pl);
    if [".test.", ".spec.", ".e2e-spec.", ".cy."]
        .iter()
        .any(|m| name.contains(m))
        || name.ends_with(".feature")
        || (name.starts_with("test_") && name.ends_with(".py"))
        || name.ends_with("_test.py")
        || name == "conftest.py"
        || name.ends_with("_test.go")
        || name.ends_with("_test.dart")
        || name.ends_with("_spec.rb")
    {
        return true;
    }
    // JUnit/XCTest/NUnit naming is CamelCase, so it is matched case-sensitively: `UserTest.kt`
    // is a test, `Latest.kt` is not.
    let orig = file_name(path);
    let Some((stem, ext)) = orig.rsplit_once('.') else {
        return false;
    };
    (matches!(ext, "java" | "kt" | "cs" | "swift") && (stem.ends_with("Test") || stem.ends_with("Tests")))
        || (ext == "java" && stem.ends_with("IT"))
}

fn is_test_prototype(path: &str) -> bool {
    let pl = slashed_lower(path);
    pl.contains(".test.")
        || pl.contains(".spec.")
        || pl.ends_with(".feature")
        || [
            "/e2e/",
            "/__tests__/",
            "/test/",
            "/tests/",
            "/androidtest/",
            "/cypress/",
        ]
        .iter()
        .any(|d| pl.contains(d))
        || pl.ends_with("test.kt")
        || pl.ends_with("tests.kt")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> Classifier {
        Classifier::default()
    }

    #[test]
    fn classes() {
        let c = spec();
        let cases = [
            ("package-lock.json", FileClass::Generated),
            ("web/yarn.lock", FileClass::Generated),
            ("assets/logo.SVG", FileClass::Generated),
            ("web/node_modules/x/index.js", FileClass::Generated),
            ("app/build/gen.kt", FileClass::Generated),
            ("README.md", FileClass::Doc),
            ("readme.txt", FileClass::Doc),
            ("docs/setup.ts", FileClass::Doc),
            ("src/docs/a.ts", FileClass::Doc),
            ("CHANGELOG.mdx", FileClass::Doc),
            ("src/app.ts", FileClass::Prod),
            ("src/app.test.ts", FileClass::Test),
            ("src/main.rs", FileClass::Prod),
            ("config/app.json", FileClass::Other),
            ("templates/index.html", FileClass::Other),
            ("Makefile", FileClass::Other),
            ("infra/main.tf", FileClass::Prod),
        ];
        for (p, want) in cases {
            assert_eq!(c.classify(p), want, "{p}");
        }
    }

    #[test]
    fn spec_test_detection() {
        let c = spec();
        let tests = [
            "src/__tests__/a.ts",
            "pkg/foo/foo_test.go",
            "tests/test_api.py",
            "app/test_models.py",
            "app/models_test.py",
            "app/conftest.py",
            "lib/widget_test.dart",
            "spec/models/user_spec.rb",
            "src/Api.Tests/UserServiceTests.cs",
            "src/MyApp.UnitTests/Foo.cs",
            "src/MyApp.IntegrationTest/Foo.cs",
            "app/src/main/java/UserTest.java",
            "app/src/main/kotlin/UserTests.kt",
            "Sources/AppUITests/LoginTests.swift",
            "src/main/java/OrderServiceIT.java",
            "web/src/a.spec.tsx",
            "api/test/app.e2e-spec.ts",
            "web/cypress/login.cy.ts",
            "features/login.feature",
            "android/app/src/androidTest/Foo.kt",
            "e2e/flows/login.ts",
            "playwright/login.ts",
            "integration_test/app_test.dart",
        ];
        for p in tests {
            assert!(c.matches_test(p), "should be test: {p}");
        }
        let not_tests = [
            "src/latest.kt",
            "src/Latest.java",
            "src/contest.py",
            "src/testing/utils.ts",
            "src/attest.go",
            "src/MyApp/Program.cs",
            "src/specs.ts",
        ];
        for p in not_tests {
            assert!(!c.matches_test(p), "should not be test: {p}");
        }
    }

    #[test]
    fn prototype_test_rules_are_narrower() {
        let p = Classifier::new(TestRules::Prototype, RoleRules::default());
        assert!(p.matches_test("src/app.test.ts"));
        assert!(p.matches_test("src/latest.kt"));
        assert!(!p.matches_test("pkg/foo_test.go"));
        assert!(!p.matches_test("tests_py/test_api.py"));
    }

    #[test]
    fn touches_ignore_generated_in_spec_mode() {
        let c = spec();
        assert!(!c.touches_test("src/__snapshots__/a.test.ts.snap"));
        assert!(c.touches_test("tests/fixtures/user.json"));
        assert!(!c.touches_doc("node_modules/x/README.md"));
        let p = Classifier::new(TestRules::Prototype, RoleRules::default());
        assert!(p.touches_test("src/__snapshots__/a.test.ts.snap"));
        assert!(p.touches_doc("node_modules/x/README.md"));
    }

    #[test]
    fn technology() {
        let c = spec();
        assert_eq!(c.technology("a/b.TSX"), Some("React"));
        assert_eq!(c.technology("a/b.mm"), Some("Objective-C"));
        assert_eq!(c.technology("a/b.json"), None);
        assert_eq!(c.technology("docs/b.ts"), None);
    }

    #[test]
    fn per_file_roles() {
        let c = spec();
        let cases = [
            ("api/src/user.test.ts", Role::Qa),
            ("db/migrations/001.sql", Role::Data),
            ("jobs/dataflow/main.py", Role::Data),
            ("infra/main.tf", Role::Infra),
            ("tools/scripts/deploy.sh", Role::Infra),
            ("web/src/App.tsx", Role::Frontend),
            ("web/src/theme.scss", Role::Frontend),
            ("api/src/user.ts", Role::Backend),
        ];
        for (p, want) in cases {
            assert_eq!(c.role_of_file(p), want, "{p}");
        }
    }

    #[test]
    fn ext() {
        assert_eq!(ext_of("a/b.c/d"), "");
        assert_eq!(ext_of("a/.gitignore"), ".gitignore");
        assert_eq!(ext_of("x.ts"), ".ts");
        assert_eq!(ext_of("makefile"), "");
    }

    #[test]
    fn role_parse() {
        assert_eq!("qa".parse::<Role>().unwrap(), Role::Qa);
        assert!("fullstack".parse::<Role>().is_err());
    }
}
