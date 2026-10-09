import { NOT_MEASURED, type NotMeasuredId } from "../../metrics/registry";
import { Tile } from "./Tile";

export function NotMeasuredTile({ id, reason }: { id: NotMeasuredId; reason?: string }) {
  const m = NOT_MEASURED[id];
  return <Tile label={m.label} state="notMeasured" note={reason ?? `needs ${m.needs}`} />;
}
