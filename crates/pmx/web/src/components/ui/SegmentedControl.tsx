import styles from "./ui.module.css";

export interface SegmentOption<T extends string> {
  value: T;
  label: string;
}

export function SegmentedControl<T extends string>(props: {
  options: ReadonlyArray<SegmentOption<T>>;
  value: T;
  onChange(value: T): void;
  label: string;
}) {
  return (
    <span className={styles.seg} role="group" aria-label={props.label}>
      {props.options.map((o) => (
        <button
          key={o.value}
          type="button"
          aria-pressed={o.value === props.value}
          onClick={() => props.onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </span>
  );
}
