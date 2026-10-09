import { formatDate, isValidDate, parseDate } from "../../lib/dates";
import { PRESETS, type PresetId } from "../../lib/ranges";
import { useDashboard } from "../../state/hooks";
import { SegmentedControl } from "../ui/SegmentedControl";
import styles from "./layout.module.css";

const CADENCES = [
  { value: "week", label: "Weekly" },
  { value: "month", label: "Monthly" },
] as const;

/** Cadence and time range. Presets and dates set the range the tiles summarize. */
export function Controls() {
  const { cadence, setCadence, preset, setPreset, range, setRange, rangeContext, engine } = useDashboard();
  const min = formatDate(rangeContext.start);
  const max = formatDate(rangeContext.asOf);
  const onDate = (which: "start" | "end", value: string) => {
    const d = parseDate(value);
    if (!isValidDate(d)) return;
    const next = { ...range, [which]: d };
    if (next.start <= next.end) setRange(next);
  };
  return (
    <div className={styles.controls}>
      <SegmentedControl label="Cadence" options={CADENCES} value={cadence} onChange={setCadence} />
      <select aria-label="Range" value={preset} onChange={(e) => setPreset(e.target.value as PresetId)}>
        {PRESETS.map((p) => (
          <option key={p.id} value={p.id} disabled={p.id === "custom" && !engine}>
            {p.label}
          </option>
        ))}
      </select>
      <input
        type="date"
        aria-label="From"
        value={formatDate(range.start)}
        min={min}
        max={max}
        disabled={!engine}
        onChange={(e) => onDate("start", e.target.value)}
      />
      <input
        type="date"
        aria-label="To"
        value={formatDate(range.end)}
        min={min}
        max={max}
        disabled={!engine}
        onChange={(e) => onDate("end", e.target.value)}
      />
      <span className={styles.legend}>
        tiles: {formatDate(range.start)} – {formatDate(range.end)} · charts:{" "}
        {cadence === "week" ? "weeks" : "months"} up to the range end · * in progress · hollow = low n
      </span>
    </div>
  );
}
