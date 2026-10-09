## Work-item complexity scoring (six-axis rubric)

Score every work item on six independent dimensions, 0-3 each. Read the actual
diff for each item. Never score from the commit message alone, and never from
line counts.

R1 - LAYERS TOUCHED
  How many distinct architectural layers the change crosses.
  0 = one layer
  1 = two layers
  2 = three or four layers
  3 = five or more layers
  (Name your own layers before you start, e.g. transport/DTO, validation,
  service, domain, persistence/entity, database object, UI, navigation.)

R2 - DATA MODEL
  What the change does to persistent structure.
  0 = no schema change
  1 = field or column added or altered on an existing structure
  2 = new table, entity, view, stored procedure or enum type
  3 = structural rewrite, destructive change, or a migration that
      backfills or transforms existing rows

R3 - CONTRACT
  What changes for a consumer outside the module.
  0 = nothing externally visible
  1 = field added to an existing contract
  2 = new endpoint, screen, dialog or command
  3 = breaking change to an existing contract, or a new interface to an
      external system

R4 - ALGORITHMIC CONTENT
  How much real reasoning lives in the code.
  0 = CRUD, declarative wiring, configuration
  1 = conditional logic, branching, simple derivation
  2 = non-trivial algorithm: query composition, parser, sort or ordering
      rule, three-valued / nullable-as-unknown logic, non-obvious state machine
  3 = an invariant that must hold in several places at once, concurrency
      or async coordination, or an external protocol you do not control

R5 - BLAST RADIUS
  What breaks if this is wrong.
  0 = one file
  1 = one module
  2 = several modules
  3 = application-wide behaviour: authentication or authorization, sync,
      schema, ordering or filtering applied everywhere, build and release

R6 - VERIFICATION NEED
  What it would take to be confident this is correct. Score the need, NOT
  whether tests were actually written. The gap between the two is a finding,
  so report it separately.
  0 = correctness is visible at a glance
  1 = a manual check is enough
  2 = a unit test is required
  3 = an integration, migration or end-to-end test is required

SUM the six scores (0-18) and map:

  0-2    XS    1 point
  3-5    S     2 points
  6-8    M     3 points
  9-11   L     5 points
  12-14  XL    8 points
  15-18  XXL  13 points

HARD RULES
- Diff size never raises a score. A 40-line change to an ordering rule used by
  every list query outranks a 900-line change that adds one CRUD endpoint.
- Exclude generated artifacts from every signal: lockfiles, generated
  migrations and schema dumps, snapshots, vendored code, formatting-only diffs.
- Score delivered work, not effort. Dead ends, discarded approaches and long
  debugging sessions leave no trace in the diff and are out of scope.
- Fix the rubric before scoring. Do not adjust anchors after seeing results.
- When two anchors both fit, take the LOWER one.
- Items without a ticket id must be grouped into logical work packages first.
  State how you grouped them; that grouping is the least reliable step.

OUTPUT
One row per work item:
  id | one-line description of what it does | R1 | R2 | R3 | R4 | R5 | R6 |
  sum | size | points

Then per developer and per stream:
  total points
  points per active day        (active day = a distinct day that person committed)
  points per available working day (business days in the window, minus holidays)
  points of work where R6 >= 2 but no test was written
