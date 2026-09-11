/**
 * Resolves a length token (`--sb-space-s`, `--sb-bar-pad-y`, …) to CSS px in
 * the calling window. The browser does the unit work: a hidden probe takes
 * `var(name)` as its width, so rem, em, calc() and clamp() all resolve
 * against the live root font size. Reading the raw custom-property string
 * with parseFloat is the trap this replaces — "0.375rem" parsed as 0.375 and
 * rounded up to a 1px gap, which is why flyouts once sat flush on the bar.
 */
export function cssLength(name: string, fallback: number): number {
  const probe = document.createElement("div");
  probe.style.position = "absolute";
  probe.style.width = `var(${name})`;
  probe.style.visibility = "hidden";
  document.body.append(probe);
  const width = probe.getBoundingClientRect().width;
  probe.remove();
  return width > 0 ? Math.ceil(width) : fallback;
}
