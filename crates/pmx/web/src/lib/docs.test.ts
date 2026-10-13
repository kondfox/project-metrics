import spec from "../../../../../wiki/metrics-spec.md?raw";
import { SPEC_ANCHORS, specUrl } from "./docs";
import { METRICS } from "../metrics/registry";

/** GitHub's heading anchor: link text kept, punctuation dropped, spaces → "-". */
const slug = (heading: string) =>
  heading
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .trim()
    .toLowerCase()
    .replace(/[^\p{L}\p{N}_\- ]/gu, "")
    .replace(/ /g, "-");

const anchors = new Set(
  spec
    .split("\n")
    .filter((l) => /^#{2,3} /.test(l))
    .map((l) => slug(l.replace(/^#+ /, ""))),
);

describe("spec links", () => {
  it("point at headings that exist in wiki/metrics-spec.md", () => {
    for (const [section, anchor] of Object.entries(SPEC_ANCHORS))
      expect(anchors, `${section} → #${anchor}`).toContain(anchor);
  });

  it("cover every metric's spec section", () => {
    for (const [id, def] of Object.entries(METRICS)) expect(specUrl(def.spec), id).not.toBeNull();
  });
});
