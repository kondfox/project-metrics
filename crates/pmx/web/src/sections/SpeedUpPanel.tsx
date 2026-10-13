import type { ReactNode } from "react";
import { HelpTip } from "../components/ui/HelpTip";
import type { Comparison, Phase, Ratio, Window } from "../data/types";
import { specUrl } from "../lib/docs";
import { formatNumber } from "../lib/format";
import { useDashboard, useSpeedUps } from "../state/hooks";
import styles from "./speedup.module.css";

const x = (v: number) => `×${formatNumber(v, v >= 10 ? 0 : 1)}`;
const span = (w: Window) => `${w.from} – ${w.to}`;

/** One speed-up: the ratio big, the medians and the interval under it; faded when unclear. */
function Speed({
  label,
  r,
  unit,
  missing,
}: {
  label: string;
  r: Ratio | null;
  unit: string;
  missing: string;
}) {
  if (!r)
    return (
      <div className={styles.speed}>
        <div className={styles.label}>{label}</div>
        <div className={styles.none}>–</div>
        <div className={styles.note}>{missing}</div>
      </div>
    );
  const unclear = r.low <= 1 && r.high >= 1;
  return (
    <div className={`${styles.speed} ${unclear ? styles.unclear : ""}`} aria-label={`${label} ${x(r.ratio)}`}>
      <div className={styles.label}>{label}</div>
      <div className={styles.ratio}>
        {x(r.ratio)}
        {unclear && <span className={styles.flag}>no clear change</span>}
      </div>
      <div className={styles.note}>
        median {formatNumber(r.now, 1)} vs {formatNumber(r.before, 1)} {unit} · 90%: {x(r.low)}–{x(r.high)}
      </div>
    </div>
  );
}

/** Rework up or tests down is a guardrail giving way: shown in red. */
function Guard({ label, pts, worseWhen }: { label: string; pts: number | null; worseWhen: "up" | "down" }) {
  if (pts == null) return <span className={styles.muted}>{label} –</span>;
  const worse = worseWhen === "up" ? pts > 0.5 : pts < -0.5;
  return (
    <span className={worse ? styles.bad : styles.muted}>
      {label} {pts > 0 ? "+" : pts < 0 ? "−" : "±"}
      {formatNumber(Math.abs(pts), 1)} pts
    </span>
  );
}

function title(c: Comparison, kind: "phase" | "marker"): string {
  if (c.source === "fallback") return "Against the 12 weeks before";
  return kind === "phase" ? `Since the start of “${c.label}”` : `Since “${c.label}”`;
}

/** The phase holding `date` (phases are oldest first). */
function phaseAt(phases: Phase[], date: string): Phase | undefined {
  return [...phases].reverse().find((p) => p.from <= date);
}

function Card({ c, kind }: { c: Comparison; kind: "phase" | "marker" }) {
  const { phases } = useDashboard().project.timeline;
  const notes: ReactNode[] = [];
  if (c.source === "fallback")
    notes.push(
      <>
        No phases set: add <code>[[phase]]</code> to pmx.toml to compare with the start of the current phase.
      </>,
    );
  if (c.source === "detected")
    notes.push(
      <>
        Detected from the first agent instructions file ({c.date}): confirm it with <code>[[marker]]</code>{" "}
        <code>kind = &quot;ai&quot;</code> in pmx.toml.
      </>,
    );
  let body: ReactNode;
  if (c.status === "too_early") {
    body = (
      <p className={styles.early}>
        Too early: {c.weeks} complete week{c.weeks === 1 ? "" : "s"} since {c.since}; it needs 4 on each side.
      </p>
    );
  } else if (c.before_idle) {
    // No baseline: say why, and which phase the comparison really straddles, instead of empty ratios.
    const now = phaseAt(phases, c.now.to);
    const then = phaseAt(phases, c.before.from);
    body = (
      <>
        <p className={styles.windows}>
          last {c.now.weeks} weeks ({span(c.now)}) vs {c.before.weeks} weeks ({span(c.before)})
        </p>
        <p className={styles.idle}>
          No work in the {c.before.weeks} weeks before (a median of 0 lines a week): nothing to compare
          against.
          {now && then && now !== then && (
            <>
              {" "}
              {kind === "marker" ? "This marker" : "The window"} falls in “{now.name}” (from {now.from}); the
              weeks before it are mostly “{then.name}”.
            </>
          )}
        </p>
      </>
    );
  } else {
    if (c.crosses_phase) {
      const before = phaseAt(phases, c.before.from)?.name;
      const now = phaseAt(phases, c.now.to)?.name;
      notes.push(
        before && now
          ? `Different phases: “${before}” before, “${now}” now. Read with care.`
          : "The two windows lie in different phases: read with care.",
      );
    }
    if (c.spans_markers.length)
      notes.push(`Between the windows: ${c.spans_markers.map((m) => `“${m}”`).join(", ")}. Read with care.`);
    if (c.approx) notes.push("~ per active engineer-day: FTE is unknown.");
    body = (
      <>
        <p className={styles.windows}>
          last {c.now.weeks} weeks ({span(c.now)}) vs {c.before.weeks} weeks ({span(c.before)})
        </p>
        <div className={styles.speeds}>
          <Speed
            label="Velocity"
            r={c.velocity}
            unit="points / FTE-day"
            missing="nothing delivered, or no scored weeks, before"
          />
          <Speed label="Volume" r={c.volume} unit="lines / FTE-day" missing="no lines added before" />
        </div>
        <p className={styles.guards}>
          Quality held? <Guard label="Rework" pts={c.rework_pts} worseWhen="up" /> ·{" "}
          <Guard label="Tests with code" pts={c.tests_pts} worseWhen="down" />
        </p>
      </>
    );
  }
  return (
    <figure className={styles.card} aria-label={title(c, kind)}>
      <h3>{title(c, kind)}</h3>
      {body}
      {notes.length > 0 && (
        <ul className={styles.notes}>
          {notes.map((n, i) => (
            <li key={i}>{n}</li>
          ))}
        </ul>
      )}
    </figure>
  );
}

/** "Is the team faster?": against the start of the phase and each AI marker (spec §4.1). */
export function SpeedUpPanel() {
  const s = useSpeedUps();
  const { project } = useDashboard();
  const doc = specUrl("§4.1");
  return (
    <div className={styles.panel}>
      <h3 className={styles.heading}>
        Speed-up
        <HelpTip>
          Medians of weekly values: the last weeks of the range against the first weeks of the current phase,
          and against the weeks before each AI marker. Velocity is points per FTE-day; volume is source lines
          added per FTE-day. A big volume gain with a small Velocity gain is more lines, not harder work.
          Faded: the 90% interval holds ×1.0, so no clear change. Read it beside rework and tests.
        </HelpTip>
        {doc && (
          <a className={styles.doc} href={doc} target="_blank" rel="noopener noreferrer" data-noprint>
            how it&apos;s measured ↗
          </a>
        )}
      </h3>
      {!s ? (
        <p className={styles.early}>This range needs the metrics engine.</p>
      ) : (
        <div className={styles.cards}>
          <Card c={s.phase} kind="phase" />
          {s.markers.map((m) => (
            <Card key={`${m.label}${m.date}`} c={m} kind="marker" />
          ))}
          {s.markers.length === 0 && (
            <figure className={styles.card} aria-label="No AI marker">
              <h3>Since the AI tooling</h3>
              <p className={styles.early}>
                No AI marker{project.timeline.ai_adoption.first ? " before this range" : ""}: add{" "}
                <code>[[marker]]</code> with <code>kind = &quot;ai&quot;</code> to pmx.toml (for example the
                day CLAUDE.md or AGENTS.md was added) to compare before and after.
              </p>
            </figure>
          )}
        </div>
      )}
    </div>
  );
}
