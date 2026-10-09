# Full-Stack & Expertise: Prior-Art Research

**Summary.** There is a deep, well-validated research literature on measuring *what a developer
knows* from version history, but almost none of it directly measures *knowledge breadth* or
"becoming full-stack" — that framing is our own recombination. The reusable core is a family of
**authorship/interaction expertise models** (Degree-of-Authorship / Degree-of-Knowledge from Fritz &
Murphy; the Expertise Browser's "experience atoms" from Mockus & Herbsleb; line-ownership from Bird
et al.) that assign, per developer and per file/module, a scalar "how much do they know this" score
from git blame, commit counts, and first-authorship. **Truck/bus-factor** algorithms (Avelino et al.;
Cosentino et al.) sit on top of these to compute who "covers" each file and how concentrated
knowledge is. Orthogonally, an **ecological / information-theoretic** strand (Posnett, Bird & Devanbu's
"dual measures of focus"; entropy-of-contribution work) quantifies how *spread out* a developer's
activity is across artifacts — which is exactly the primitive we need, only they use it to predict
*defects* (focused = fewer bugs), not to reward breadth. A separate **code-review-as-knowledge-sharing**
strand (Rigby & Bird; onboarding/mentoring studies) shows review measurably widens the set of files a
developer knows and is the natural place to detect mentoring. Every one of these carries the same
health warning: authorship ≠ understanding, all of them are gameable by trivial/scattered commits, and
per-individual use raises Goodhart and ethics problems — so breadth must be weighted by *durable,
sustained* contribution, not commit counts. The sections below give each metric's idea, its
git-computable formula, what it is validated to predict, and its gaming caveats, then map them onto our
three concrete needs.

---

## 1. Code-expertise & knowledge models from version history

### 1.1 Degree-of-Authorship (DOA) and Degree-of-Knowledge (DOK) — Fritz, Ou, Murphy-Hill, Murphy

**Idea.** A developer's knowledge of a source file is predicted by two things: *authorship* (did they
create/change it, and has it since been changed by others?) and *interaction* (have they recently been
reading/editing it in the IDE?). DOA captures the authorship half from version history; DOK adds the
interaction half.

**Formula (DOA — the git-computable part).** For developer `d` and file `f`:

```
DOA(d, f) = 3.293 + 1.098·FA + 0.164·DL − 0.321·ln(1 + AC)
```

- **FA (First Authorship)** = 1 if `d` created `f`, else 0. (Strongest positive term — creating a file
  is the biggest single predictor of knowing it.)
- **DL (Deliveries)** = number of changes `d` made to `f` (their own commits touching it). Positive but
  with diminishing weight.
- **AC (Acceptances)** = number of changes to `f` made by *other* developers since. Negative term —
  knowledge decays as others modify "your" file (with a log to dampen it).

The coefficients are from a regression fit in the original study; in practice DOA is used
**normalized** per file: `DOA_norm(d,f) = DOA(d,f) / max_d DOA(d,f)`, giving 1.0 to the top author of
each file.

**DOK.** `DOK = weighted combination of DOA (authorship) + interaction frequency/recency` (interaction
data came from Mylyn task-context logs). The interaction half is **not** recoverable from git — only
DOA is git-derivable — so for a git-only system DOA is the usable model.

**Validated to predict.** In case studies DOK/DOA outperformed authorship-only expertise finders at
identifying the right expert for a file, at flagging knowledge-transfer needs, and at identifying
"changes of interest." DOA alone is a strong expert-identification signal and is the standard authorship
score reused by truck-factor tools.

**Gaming / validity caveats.** FA over-rewards whoever scaffolds a file first (generators, project
setup). AC decay can wrongly demote someone who still understands code others have since touched.
Squash/rebase and file renames/moves corrupt the history the terms are counted from. Authorship is a
proxy for knowledge, not knowledge itself.

### 1.2 Expertise Browser & "experience atoms" — Mockus & Herbsleb (ICSE 2002)

**Idea.** Quantify expertise by counting the atomic units of *change* a person has made. An
**Experience Atom (EA)** is the smallest meaningful unit of change (e.g., a delta/modification request)
a person made to a work product. Sum EAs over any scope — file, module, subsystem, whole product — to
get that person's experience there.

**Computation (git-mappable).** Per developer, count their modifications (commits/deltas) to each
artifact; aggregate up the directory/module tree. The tool lets you (a) find the top experts for a
given artifact and (b) see one person's *expertise profile* across the whole product — explicitly
distinguishing "worked briefly here once" from "extensive experience," and surfacing people with
**broad** expertise across many subsystems. That breadth view is the closest prior art to our
"full-stack" question.

**Validated to predict.** EA counts were validated against developers' self-reported expertise and used
operationally at scale (Avaya/Lucent) to locate experts across a large product and across organizational
/ geographic boundaries.

**Gaming / caveats.** EA counts are volume-based — many tiny changes inflate them; they don't weight how
*central* or durable a change was. Coarse change granularity (one big commit vs. many) distorts counts.

**Related — Expertise Recommender / bug-triage expertise.**
- *Who Should Fix This Bug?* (Anvik, Hiew, Murphy, ICSE 2006): trains a classifier (SVM/Naïve Bayes/C4.5)
  on the history of *who resolved what kind of report* to recommend an assignee — expertise inferred
  from resolution history rather than blame.
- Anvik & Murphy's later work (TOSEM 2011) builds developer-expertise recommenders for
  development-oriented decisions from repository history. Useful as evidence that "past activity predicts
  who can handle this area" generalizes beyond files to issue categories.

### 1.3 Line ownership / code familiarity — Bird, Nagappan, Murphy, Gall, Devanbu ("Don't Touch My Code!", FSE 2011)

**Idea.** Define **ownership** of a component as the proportion of commits (or lines) contributed by a
developer, then relate the ownership *distribution* to defects.

**Metrics (git blame / commit counts).** For a component:
- **Ownership(d) = (# commits by d) / (total commits to component)** — d's share.
- **Owner / major contributors** = developers with ownership ≥ 5%.
- **Minor contributors** = ownership < 5%.
- **Total contributors**, and the **top-owner's proportion**.

**Validated to predict.** In Windows Vista/7: higher **top-owner proportion** and *fewer minor
contributors* → fewer pre-release faults and post-release failures. Many low-expertise (minor)
contributors is a defect risk. Strong, replicated result linking ownership concentration to quality.

**Caveats & tension with our goal.** This literature *rewards concentration* — precisely the opposite of
what a breadth/cross-training program encourages. That tension is the central caveat for us: broadening
people into new stacks means deliberately creating "minor contributors," which this body of work
associates with more defects. It is the empirical argument for **pairing breadth with mentoring/review**
(Section 4) rather than letting novices touch unfamiliar stacks unsupervised. (See also Rahman & Devanbu
line-level ownership→defects, and Greiler/Bird's revisit of ownership under modern code review.)

### 1.4 Knowledge maps / knowledge distribution — CodeScene, Microsoft

**Idea (industrial, git-native).** Build a **knowledge map**: color each file by its "main developer"
(the one who contributed most of its code, over the file's deep history), then aggregate to
**individual** and **team** knowledge-distribution views, key-person risk, and "system mastery loss if X
leaves." CodeScene aggregates individual knowledge into **teams**, which it argues is more actionable
than per-individual maps. This is the productized, git-only descendant of DOA/ownership + Expertise
Browser, and the closest existing UI to what we want to build.

**Caveats.** Main-developer coloring hides everyone but the top contributor; "contributed most of the
code" is churn-weighted and inflated by generated/moved code unless filtered.

---

## 2. Truck-factor / bus-factor algorithms

**Purpose.** Estimate the minimum number of developers who could leave before the project is
"incapacitated" (loses knowledge of >½ its files) — i.e., how concentrated knowledge is. Relevant to us
because these algorithms formalize **"who knows each file"** and **codebase coverage**, which we invert
to ask "is each stack covered by more than one person, and is coverage spreading?"

### 2.1 Avelino, Passos, Hora, Valente — "A Novel Approach for Estimating Truck Factors" (ICPC 2016)

**Compute "who knows a file" (authors of a file).** Using DOA (Section 1.1): developer `d` is an
**author** of file `f` iff
```
DOA_norm(d,f) ≥ 0.75   AND   DOA(d,f) ≥ 3.293   (absolute floor)
```
i.e., they are within 75% of the file's top author *and* clear a minimum absolute bar. A file's authors
are its "knowers."

**Greedy TF algorithm.**
1. Compute the author set of every file.
2. Repeatedly remove the developer who is an author of the **largest number of files**.
3. After each removal, recount files that now have **no author** ("abandoned/orphaned").
4. **Stop** when > 50% of files are orphaned. The **Truck Factor = number of developers removed**.

**Validated.** Run on 133 popular GitHub projects; results validated by surveying the projects'
developers. Widely reused; open-source tool (`aserg-ufmg/Truck-Factor`).

### 2.2 Cosentino, Izquierdo, Cabot — "Assessing the Bus Factor of Git Repositories" (SANER 2015)

Given a git repo, computes bus factor for **any file, directory, branch, or the whole project**, using
configurable "knows a file" heuristics (commit-share / last-change / multiple strategies), and can
**simulate** the effect of specific developers leaving. Comparative study (Ferreira, Valente, Ferreira,
SQJ 2019) found Avelino's removal approach and Cosentino's among the most accurate; the systematic
picture is that most projects have alarmingly low TF (≈65% of studied systems have TF ≤ 2).

**Reusable primitive for us.** Both give a reusable definition of **stack coverage**: restrict the file
set to one stack (Section 5), then TF-of-that-stack answers "how many people actually know our
frontend?" A rising per-stack TF over time is direct evidence cross-training is working — and it is
harder to game than any single-developer breadth score, because it is a whole-team structural property.

**Caveats.** All TF variants inherit DOA/ownership fragility (renames, squash, generated code, bots as
"developers"); the ">50% orphaned" threshold and the "author" cutoffs are tunable knobs that change
results.

---

## 3. Knowledge-breadth / diversity measures (the core of "T-shaped")

### 3.1 Dual ecological measures of focus — Posnett, D'Souza, Devanbu, Filkov (ISSE/ICSE 2013)

**Idea.** Model the developer↔artifact contribution graph like a predator–prey food web and, using
information theory, derive **two** normalized measures from the same data:
- **Developer Focus (DF)** — how concentrated one developer's activity is across the modules they touch
  (low entropy = focused specialist; high entropy = spread across many modules).
- **Module/Artifact Focus** — how concentrated a module's incoming attention is across developers.

These are cross-entropy / Kullback–Leibler formulations; each is normalized so developer-side and
artifact-side focus are comparable.

**Formula sketch (git-computable).** For developer `d` with activity `p_i` = fraction of d's commits (or
changed lines) landing in module/area `i`, Shannon entropy `H(d) = −Σ p_i·log p_i`. **Breadth ↑ as
H ↑.** Normalize by `log N` (number of areas) for a 0–1 **breadth index**, or use **Gini–Simpson**
`1 − Σ p_i²` for a less tail-sensitive alternative. This is *exactly* the primitive for per-developer
stack-breadth — we just invert the sign of what's "good."

**Validated to predict.** More focused developers introduce **fewer** defects; defocused (high-breadth)
activity correlates with more defects. **Critical caveat for us:** the published evidence frames breadth
as a *risk*, not a virtue. Our program deliberately pushes people toward higher entropy, so we must (a)
pair it with mentoring/quality guardrails and (b) never present the raw breadth score as "better =
higher" without the defect counter-signal.

### 3.2 Entropy-of-contribution & fault-proneness

Independent work (e.g., entropy-based developer-contribution metrics; Hassan's change-entropy)
measures the **balance** of contributions to a file as entropy and finds **imbalanced** (low-entropy)
files more fault-prone, and *scattered* changes (high change-entropy) more fault-prone. Same
double-edged lesson: entropy is a clean, git-only breadth/spread primitive, but the literature attaches
it to *defect risk*, so it argues for guardrails around breadth, not against measuring it.

### 3.3 "T-shaped engineer" — practitioner framing, little formal measurement

The T-shaped concept (deep in one area, broad across several) is heavily discussed by practitioners but
**not** given a validated git metric. Practitioner "metrics" are outcome-oriented, not history-derived:
reduction in work blocked by specialist unavailability; number of engineers who can contribute to
multiple areas; drop in knowledge-transfer time when a specialist is away. Useful as *validation
targets* for our git metric (does rising breadth actually reduce blocked-on-specialist waits?), not as
computation methods. **Gap = our contribution:** operationalize the vertical/horizontal bars as
per-area DOA depth (vertical) + across-area entropy (horizontal).

### 3.4 Focus vs. breadth & context-switching

Related evidence that breadth has costs: context-switching / fragmented attention reduces throughput
(Meyer/Fritz developer-productivity studies; task-switching literature). Relevant caveat: a breadth
metric that rewards touching many areas can incentivize costly context-switching — reinforcing that
breadth should be measured as *sustained* second-stack contribution, not scatter.

---

## 4. Mentoring / knowledge-transfer signals from code review

### 4.1 Review as knowledge sharing — Rigby & Bird, "Convergent Contemporary Peer Review" (FSE 2013)

**Key quantified result.** They define a **knowledge-sharing** measure = the number of distinct files a
developer becomes aware of through reviewing others' changes. Participating in review **increases the
distinct files a developer "knows about" by 66%–150%** depending on project (Android, Chromium OS, Bing,
Office, SQL, AMD). This is the single strongest piece of prior art that **review spreads knowledge
measurably**, and it is computed purely from review participation + changed-file lists — i.e., git/PR
data.

**Reusable computation.** For reviewer `r`: `files_known_via_review(r) = ⋃ changed_files(PRs r reviewed)`.
Growth of this set over time, especially its overlap with a **new** stack, is a git-only mentoring/growth
signal.

### 4.2 Onboarding, mentoring, expertise growth via review

- Code review is repeatedly documented as an onboarding/mentoring mechanism: newcomers learn standards,
  architecture, and the "why" through review feedback (Graphite; multiple SLRs on onboarding & OSS
  mentoring). *Code Review for Newcomers: Is It Different?* (Bosu/others) studies how newcomer reviews
  differ.
- OSS mentoring at scale (48,402 "good first issues" study) shows expert involvement in newcomer issue
  resolution and links mentoring to retention — evidence the **novice-author ↔ expert-reviewer** pairing
  is real and detectable in history.
- Knowledge-transfer-in-MCR studies catalog the practices (context-sharing comments, code-snippet banks)
  that constitute mentoring inside review.

**Metric for a novice↔expert pairing (our synthesis, from this literature).** A mentoring event is
detectable when, on a PR that touches stack `S`:
- the **author** has low DOA/expertise in `S` (novice/entering — see Section 6.3), **and**
- a **reviewer** has high DOA in `S` (established expert), **and**
- the review is substantive (non-trivial comment count / back-and-forth, not a rubber-stamp).
No prior work packages exactly this triple, but each ingredient (author expertise, reviewer expertise,
review depth) is individually established and git/PR-computable.

**Caveats.** "Reviewed" ≠ "learned"; rubber-stamp approvals inflate any review-based knowledge measure
(hence the substantiveness gate). Reviewer expertise itself must be estimated with the same fragile DOA.

---

## 5. Defining "stacks"/areas objectively (mapping files → areas)

**Method.** Map each file to a canonical **area/stack** using **path + extension/language**:
- **Language detection à la GitHub Linguist:** primarily by **file extension**, with a **Bayesian
  classifier** to disambiguate shared extensions (`.h`, `.m`, `.r`), plus shebang and content
  heuristics. Linguist also flags **vendored** and **generated** files and can be overridden per repo
  via `.gitattributes` (`linguist-vendored`, `linguist-generated`, `linguist-documentation`,
  `linguist-language`). Reusing Linguist's classifier + its vendored/generated exclusion lists is the
  standard, battle-tested way to turn files into languages.
- **Directory-based modularization:** top-level or convention dirs (`/frontend`, `/ios`, `/android`,
  `/api`, `/terraform`, `/.github`, `/tests`) map to stacks; combine with language for robustness.

**Pitfalls (well-documented).**
- **Generated / vendored code** (minified JS, lockfiles, protobuf output, migrations) inflates authorship
  and language stats — must be excluded (Linguist attributes; deny-lists).
- **Monorepos** mix many stacks; a single "repo language" is meaningless — must classify per file/dir.
- **Polyglot files & config** (YAML, Dockerfiles) are ambiguous across stacks.
- **Renames/moves** break per-file history (`git log --follow` helps but is imperfect).
- **Squash-merges** collapse authorship to one commit, erasing who-did-what.

---

## 6. Gaming risks, validity caveats & ethics

### 6.1 Goodhart's law is the governing constraint
"When a measure becomes a target, it ceases to be a good measure." Documented dev-metric gaming: LOC
padding, artificial PR splitting, ticket-splitting, inflating story points. A **breadth** target is
*especially* gameable — the incentive is to make a trivial commit in each stack to "look broad."

### 6.2 Specific gaming vectors for a breadth metric
- **Scatter/tourism:** one-line commits across many areas to raise entropy/area-count.
- **Generated-file breadth:** touching generated/config files in an unfamiliar stack.
- **Rubber-stamp reviews** to inflate review-based knowledge-sharing.
- **First-authorship farming:** scaffolding empty files to grab FA=1 across stacks.

### 6.3 Mitigations (durable-line weighting & sustained-contribution thresholds)
- **Durable-line weighting:** weight contribution by lines that **survive** in later blame (à la
  code-survival / `git blame` of a later revision), not raw churn — trivial/scattered edits mostly don't
  survive.
- **Sustained-contribution thresholds:** require a **minimum durable footprint over a minimum time span**
  in a stack before it counts toward breadth (mirrors Avelino's absolute DOA floor `≥ 3.293` and
  normalized `≥ 0.75` gates). "Entering a new stack" should require crossing this bar, not a single
  commit.
- **Exclude generated/vendored files** (Section 5) before any counting.
- **Depth-gated breadth:** count an area toward breadth only where per-area DOA clears a depth bar, so
  breadth = "several areas known *reasonably well*," not "several areas touched once."
- **Whole-team framing over individual scores:** prefer per-stack **truck factor** and team
  knowledge-distribution (structural, hard to game) over per-person leaderboards.

### 6.4 Ethics of measuring individuals
- Ownership research shows concentration → fewer defects, i.e., pushing breadth has a real quality cost;
  measure and *manage* that trade-off rather than hiding it.
- Per-individual knowledge scores are **development tools, not ranking tools** — surfacing them as
  performance rankings invites gaming and harm; combine multiple signals, keep qualitative judgment in
  the loop, and (per CodeScene's own guidance) prefer **team-level** aggregation.

---

## 7. How this maps to measuring full-stack progression

### (a) Per-developer stack-breadth
- **Primary:** per-developer **entropy / Gini–Simpson over stacks** (§3.1–3.2) computed on
  **durable-line-weighted, generated-excluded** contribution, i.e. `p_i` = d's durable footprint in
  stack `i` / their total. Normalize by `log(#stacks)` for a 0–1 breadth index.
- **Depth per stack:** per-stack **DOA** (§1.1) / ownership share (§1.3) gives the "vertical bar";
  combine depth (max/second DOA) + breadth (entropy) to operationalize **T-shaped**.
- **Guardrail:** always display alongside the §3 **defect counter-signal** and §6.3 gates so breadth
  isn't rewarded as scatter.

### (b) Detecting a developer entering a NEW stack
- Define **known stacks** = stacks where d clears the sustained/DOA bar (§6.3). A **new-stack entry** =
  d's durable footprint in a previously-unknown stack crosses that bar over a time window.
- Reuse the **Expertise Browser** breadth-profile idea (§1.2) and **per-stack truck-factor** (§2): a new
  entrant *adds* to the stack's author set / raises its TF — a team-level, hard-to-game confirmation that
  entry is real, not tourism.
- Use §6.3 durable-line weighting so a burst of trivial commits doesn't register as "entered."

### (c) Detecting mentoring (expert reviews a learner's cross-stack PR)
- Implement the §4.2 **novice-author ↔ expert-reviewer** triple on each PR touching stack `S`:
  low author-DOA in `S` + high reviewer-DOA in `S` + substantive review (comment depth/iterations,
  not rubber-stamp).
- Quantify transfer with Rigby & Bird's **knowledge-sharing** measure (§4.1): growth in the learner's
  and reviewer's set of distinct files-known-in-`S`, especially newly-touched `S` files. A team whose
  cross-stack PRs are consistently expert-reviewed, with the learners' per-stack DOA subsequently rising,
  is the signature of the exact mentoring program we want to measure.

---

## Sources

**Degree-of-Authorship / Degree-of-Knowledge**
- Fritz, Ou, Murphy-Hill, Murphy — "A Degree-of-Knowledge Model to Capture Source Code Familiarity" (ICSE 2010): https://hasel.dev/publication/degree-of-knowledge-modeling-a-developers-knowledge-of-code/ · IEEE: https://ieeexplore.ieee.org/document/6062106/ · PDF/Semantic Scholar: https://www.semanticscholar.org/paper/A-degree-of-knowledge-model-to-capture-source-code-Fritz-Ou/ee81a50eae3a2d1b62ee5951aeb20f9118ae962e
- DOA formula reference: https://contributoriq.com/blog/degree-of-authorship-code-ownership-explained
- GenAI impact on authorship-based expertise models: https://arxiv.org/pdf/2507.08160

**Expertise Browser / Expertise Recommender / Bug-triage expertise**
- Mockus & Herbsleb — "Expertise Browser: A Quantitative Approach to Identifying Expertise" (ICSE 2002): https://herbsleb.org/web-pubs/pdfs/mockus-expertise-2002.pdf · http://mockus.us/papers/exbdraft.pdf · tool: https://mockus.org/ExV/index.html
- Anvik, Hiew, Murphy — "Who Should Fix This Bug?" (ICSE 2006): https://www.ifi.uzh.ch/dam/jcr:00000000-2f41-7b40-0000-00005fabb70c/murphy-icse06.pdf
- Anvik & Murphy — "Reducing the effort of bug report triage" (TOSEM 2011): https://dl.acm.org/doi/10.1145/2000791.2000794

**Ownership vs. defects**
- Bird, Nagappan, Murphy, Gall, Devanbu — "Don't Touch My Code! Examining the Effects of Ownership on Software Quality" (FSE 2011): https://www.microsoft.com/en-us/research/uploads/prod/2016/02/bird2011dtm.pdf · summary: https://neverworkintheory.org/2011/09/05/dont-touch-my-code.html
- Greiler, Herzig, Czerwonka — "Code Ownership and Software Quality (modern code review)" (ICSE 2016): https://dl.acm.org/doi/abs/10.1145/2884781.2884852
- "An Analysis of the Effect of Code Ownership on Software Quality" (MSR): https://www.microsoft.com/en-us/research/wp-content/uploads/2016/02/ownership.pdf

**Knowledge maps (industrial)**
- CodeScene — Knowledge Distribution / bus factor docs: https://codescene.com/knowledge-distribution · https://codescene.io/docs/guides/social/knowledge-distribution.html

**Truck / bus factor**
- Avelino, Passos, Hora, Valente — "A Novel Approach for Estimating Truck Factors" (ICPC 2016): https://arxiv.org/abs/1604.06766 · PDF: https://arxiv.org/pdf/1604.06766 · tool: https://github.com/aserg-ufmg/Truck-Factor
- Cosentino, Izquierdo, Cabot — "Assessing the Bus Factor of Git Repositories" (SANER 2015): https://www.researchgate.net/profile/Valerio-Cosentino/publication/272824568_Assessing_the_Bus_Factor_of_Git_Repositories
- Ferreira, Valente, Ferreira — "Algorithms for estimating truck factors: a comparative study" (SQJ 2019): https://link.springer.com/content/pdf/10.1007/s11219-019-09457-2.pdf
- Jabrayilzade et al. — "Bus Factor in Practice" (ICSE-SEIP 2022): https://arxiv.org/pdf/2202.01523
- "Who Can Maintain This Code?" (IEEE Software 2018): https://homepages.dcc.ufmg.br/~mtov/pub/2018-ieeesw.pdf
- Knowledge Islands (visualization): https://arxiv.org/pdf/2408.08733

**Breadth / focus / entropy**
- Posnett, D'Souza, Devanbu, Filkov — "Dual Ecological Measures of Focus in Software Development" (ICSE 2013): https://dl.acm.org/doi/abs/10.5555/2486788.2486848
- Entropy-based developer-contribution & fault-proneness: https://www.atlantis-press.com/journals/ijndc/25905544
- Fork-entropy / diversity (entropy-as-diversity methodology): https://arxiv.org/html/2205.09931

**Review as knowledge sharing / mentoring / onboarding**
- Rigby & Bird — "Convergent Contemporary Software Peer Review Practices" (FSE 2013): https://www.microsoft.com/en-us/research/wp-content/uploads/2016/02/rigby2013convergent.pdf
- Code reviews as a mentoring tool: https://graphite.com/guides/code-reviews-mentoring-junior-devs
- "Code Review for Newcomers: Is It Different?" (ICSME 2018): https://ieeexplore.ieee.org/document/8445531
- Onboarding SLR: https://arxiv.org/pdf/2408.15989 · OSS mentoring / good-first-issues: https://arxiv.org/pdf/2302.05058
- Knowledge-transfer practices in MCR: https://link.springer.com/chapter/10.1007/978-3-031-91485-0_5

**Mapping files → stacks / languages**
- GitHub Linguist: https://github.com/github-linguist/linguist · overrides/gitattributes: https://github.com/github-linguist/linguist/blob/main/docs/overrides.md

**Goodhart / gaming / ethics**
- Goodhart's law & gamed dev metrics: https://www.keypup.io/blog/goodharts-law-in-action-why-your-dev-metrics-are-being-gamed-and-how-to-fix-it/ · https://codepulsehq.com/guides/goodharts-law-engineering-metrics
