import type { ProjectFile } from "../data/types";
import { isMeasured, type MetricId } from "../metrics/registry";

/** Which input a headline needs (the `fed` keys of a pooled fleet row). */
export type Input = "quality" | "velocity" | "security" | "ai" | "peer_review";

export interface Headline {
  id: MetricId;
  label: string;
  input?: Input;
  /** A 0–100 score with an A–E band. */
  score?: boolean;
  /** Point-in-time (a snapshot), marked `~`. */
  snapshot?: boolean;
  /** Context, not a headline (fleet-vision.md). */
  context?: boolean;
  /** A rise is good news (its change arrow is green, a fall red); otherwise the arrow stays neutral. */
  higherIsBetter?: boolean;
  /** What the fleet row does with it, for the column's tooltip. */
  pooling: string;
}

/** The fleet's columns (fleet-vision.md, metrics-overview.md "Fleet headlines"). */
export const HEADLINES: readonly Headline[] = [
  {
    id: "velocity",
    label: "Velocity",
    input: "velocity",
    pooling: "Σ points ÷ Σ FTE-days, only across projects scored the same way",
  },
  {
    id: "quality",
    higherIsBetter: true,
    label: "Quality",
    input: "quality",
    score: true,
    pooling: "pooled numerators and denominators",
  },
  {
    id: "security_score",
    higherIsBetter: true,
    label: "Security",
    input: "security",
    score: true,
    snapshot: true,
    pooling: "the worst project (a gate, not an average)",
  },
  {
    id: "multi_stack_pct",
    higherIsBetter: true,
    label: "Multi-stack devs",
    pooling: "pooled; people are counted per project",
  },
  {
    id: "ai_assist_commit_pct",
    label: "AI-assisted",
    input: "ai",
    pooling: "pooled over projects that attribute AI",
  },
  {
    id: "review_coverage_pct",
    higherIsBetter: true,
    label: "Peer review",
    input: "peer_review",
    pooling: "pooled over projects with code-host data",
  },
  {
    id: "active_devs",
    label: "Active devs",
    context: true,
    pooling: "summed; people are counted per project",
  },
];

/** Did this project's run measure the headline's input? */
export function projectMeasures(p: ProjectFile, input: Input | undefined): boolean {
  switch (input) {
    case undefined:
    case "quality":
      return true;
    case "velocity":
      return isMeasured(p.meta.not_measured, "velocity");
    case "security":
      return isMeasured(p.meta.not_measured, "dependency_vulnerabilities");
    case "ai":
      return p.project.ai_attribution;
    case "peer_review":
      return isMeasured(p.meta.not_measured, "peer_review");
  }
}
