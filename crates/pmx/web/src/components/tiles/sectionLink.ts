import { createContext } from "react";

/** The section a tile links to (its `Section` id); null for a tile that links nowhere. */
export const SectionLinkContext = createContext<string | null>(null);

export const sectionElementId = (id: string) => `section-${id}`;

/** Scroll to a section. The URL hash holds the view state (cadence, range, project), so no `#anchor`. */
export function scrollToSection(id: string) {
  document.getElementById(sectionElementId(id))?.scrollIntoView?.({ behavior: "smooth", block: "start" });
}
