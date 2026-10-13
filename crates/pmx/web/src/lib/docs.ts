/** Where the metrics are defined in detail: the spec in the repo's wiki (it wins over the topic pages). */
const SPEC_URL = "https://github.com/kondfox/project-metrics/blob/main/wiki/metrics-spec.md";

/**
 * Spec section → its heading's anchor on GitHub. `docs.test.ts` checks each one against the headings of
 * wiki/metrics-spec.md, so a renamed heading fails the build instead of breaking a link.
 */
export const SPEC_ANCHORS = {
  "§2": "2-activity--context-metrics-project-dashboard",
  "§3": "3-quality-fleet-headline--quality-indexmd-40",
  "§3.1": "31-rework-rate--s_rework",
  "§3.2": "32-tests-with-code--s_tests",
  "§3.3": "33-docs-with-code--s_docs",
  "§3.4": "34-duplication---s_dup",
  "§3.5": "35-trend-only-companions-not-in-the-score",
  "§4": "4-velocity-fleet-headline--velocity-indexmd",
  "§4.1": "41-speed-up-phases-and-markers-decided-2026-10-13-plan-d57",
  "§5": "5-multi-stack-developers-fleet-headline--fullstack-metricsmd",
  "§6": "6-ai-assisted-fleet-headline",
  "§7.1": "71-dependency-vulnerabilities",
  "§7.2": "72-live-secrets-gate",
  "§7.3": "73-security-score",
  "§7.4": "74-own-code-sast-project-dashboard-trend-only--never-ranked",
  "§8": "8-code-host-prmr-flow--peer-review-fleet-column--project-panels",
  "§10.2": "102-fte-provider-external-source-stub-in-v1",
} as const;

export type SpecSection = keyof typeof SPEC_ANCHORS;

/** The link to how a metric is measured; null for a section without an anchor here. */
export function specUrl(section: string): string | null {
  const anchor = (SPEC_ANCHORS as Record<string, string>)[section];
  return anchor ? `${SPEC_URL}#${anchor}` : null;
}
