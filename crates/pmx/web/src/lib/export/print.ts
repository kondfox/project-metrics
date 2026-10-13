/**
 * PDF through the browser's print dialog ("Save as PDF"): text stays text and charts stay vectors.
 * The page prints light and at page width (`html.printing`, theme.css): the charts recolor and resize
 * first, then everything goes back.
 */
export function printPage() {
  const root = document.documentElement;
  const theme = root.dataset.theme;
  root.dataset.theme = "light";
  root.classList.add("printing");
  const restore = () => {
    root.classList.remove("printing");
    if (theme) root.dataset.theme = theme;
    else delete root.dataset.theme;
  };
  window.addEventListener("afterprint", restore, { once: true });
  // Let the charts redraw in light colors at the narrower width before the dialog snapshots them.
  setTimeout(() => window.print(), 400);
}
