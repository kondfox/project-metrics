import { useDashboard } from "../../state/hooks";
import { Banner } from "../ui/Banner";

/** Things that change how to read the page. */
export function Notices() {
  const { project, engine } = useDashboard();
  return (
    <>
      {project.meta.dialect !== "spec" && (
        <Banner tone="warn">
          Produced in the <b>{project.meta.dialect}</b> dialect (parity harness), not by a normal run.
        </Banner>
      )}
      {!engine && (
        <Banner tone="warn">
          This pmx was built without the metrics engine (WASM): only whole weeks and months can be shown.
        </Banner>
      )}
    </>
  );
}
