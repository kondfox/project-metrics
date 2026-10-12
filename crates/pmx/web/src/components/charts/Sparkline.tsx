import styles from "./charts.module.css";

const W = 72;
const H = 20;
const PAD = 2;

/** A small trend line under a value: gaps stay gaps, the latest point is dotted. */
export function Sparkline({ values, label }: { values: Array<number | null>; label: string }) {
  const known = values.flatMap((v, i) => (v == null ? [] : [{ v, i }]));
  if (known.length < 2) return null;
  const lo = Math.min(...known.map((p) => p.v));
  const hi = Math.max(...known.map((p) => p.v));
  const x = (i: number) => PAD + (i / (values.length - 1)) * (W - 2 * PAD);
  // A flat series sits in the middle.
  const y = (v: number) => (hi === lo ? H / 2 : PAD + (1 - (v - lo) / (hi - lo)) * (H - 2 * PAD));
  const runs: string[] = [];
  let run: string[] = [];
  values.forEach((v, i) => {
    if (v == null) {
      if (run.length > 1) runs.push(run.join(" "));
      run = [];
    } else run.push(`${x(i).toFixed(1)},${y(v).toFixed(1)}`);
  });
  if (run.length > 1) runs.push(run.join(" "));
  const last = known.at(-1)!;
  return (
    <svg
      className={styles.sparkline}
      width={W}
      height={H}
      viewBox={`0 0 ${W} ${H}`}
      role="img"
      aria-label={label}
    >
      {runs.map((pts) => (
        <polyline key={pts} points={pts} />
      ))}
      <circle cx={x(last.i)} cy={y(last.v)} r={2} />
    </svg>
  );
}
