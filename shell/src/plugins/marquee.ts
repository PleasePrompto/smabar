const SPEED_PX_PER_SECOND = 30;
const MIN_DURATION_SECONDS = 3;

/** The closed details body that currently hides an element, if any. */
function hidingDetails(element: HTMLElement): HTMLDetailsElement | null {
  let ancestor = element.parentElement;
  while (ancestor !== null) {
    if (ancestor instanceof HTMLDetailsElement && !ancestor.open) {
      const summary = Array.from(ancestor.children).find(
        (child) => child.tagName === "SUMMARY",
      );
      if (!summary?.contains(element)) return ancestor;
    }
    ancestor = ancestor.parentElement;
  }
  return null;
}

/** Updates one already-wrapped marquee from its current layout metrics. */
export function measureMarquee(
  element: HTMLElement,
  content: HTMLElement,
): void {
  const overflow = content.scrollWidth - element.clientWidth;
  element.toggleAttribute("data-marquee-overflow", overflow > 1);
  if (overflow > 1) {
    element.style.setProperty("--marquee-distance", `${String(overflow)}px`);
    element.style.setProperty(
      "--marquee-duration",
      `${String(Math.max(MIN_DURATION_SECONDS, overflow / SPEED_PX_PER_SECOND))}s`,
    );
  } else {
    element.style.removeProperty("--marquee-distance");
    element.style.removeProperty("--marquee-duration");
  }
}

/**
 * Enables marquees when they become visible. Closed details bodies wait for
 * their native toggle event so hidden plugin content never forces layout.
 */
export function enhanceMarquees(root: ParentNode): () => void {
  const cleanups: (() => void)[] = [];
  const enhance = (element: HTMLElement): void => {
    const details = hidingDetails(element);
    if (details !== null) {
      const onToggle = () => {
        if (!details.open) return;
        details.removeEventListener("toggle", onToggle);
        enhance(element);
      };
      details.addEventListener("toggle", onToggle);
      cleanups.push(() => {
        details.removeEventListener("toggle", onToggle);
      });
      return;
    }

    const content = element.ownerDocument.createElement("span");
    content.className = "sb-marquee-content";
    content.append(...element.childNodes);
    element.appendChild(content);
    element.classList.add("sb-marquee");

    const update = () => {
      measureMarquee(element, content);
    };
    update();
    if (typeof ResizeObserver === "function") {
      const observer = new ResizeObserver(update);
      observer.observe(element);
      observer.observe(content);
      cleanups.push(() => {
        observer.disconnect();
      });
    }
  };

  for (const element of root.querySelectorAll<HTMLElement>("[data-marquee]")) {
    enhance(element);
  }
  return () => {
    cleanups.forEach((cleanup) => {
      cleanup();
    });
  };
}
