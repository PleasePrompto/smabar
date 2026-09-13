import { useEffect, useState } from "react";

/**
 * The sub-headings of the settings section that is currently open.
 *
 * A section is a list of `SettingGroup` blocks, and the blocks' titles ARE
 * the sub-headings — they are read back out of the rendered DOM rather than
 * declared a second time. Two reasons, and the first one decides:
 *
 * - A hand-kept list would go stale silently. The next block someone adds to
 *   a tab would simply not appear here, and nothing would fail.
 * - Some of them do not exist until runtime: the tiles section renders one
 *   block per installed plugin, and those arrive with `list_plugins`.
 *
 * The observer is what makes the second point work, and the equality check
 * below is what keeps it from looping: a `setState` per mutation would
 * re-render the panel, which mutates the panel, which fires the observer.
 */
export interface Subsection {
  /** Position among the section's blocks — the click target's index. */
  index: number;
  label: string;
  updateKey?: string;
}

/** Fewer than this and a sub-navigation is noise, not navigation. */
const MIN_ENTRIES = 2;

/** The navigable sub-headings of one rendered section. */
export function readSubsections(host: HTMLElement): Subsection[] {
  const found = blocks(host).flatMap((block, index) => {
    const label = block
      .querySelector(".settings-block-title")
      ?.textContent.trim();
    return label === undefined || label === ""
      ? []
      : [
          {
            index,
            label,
            ...(block.dataset.updateKey === undefined
              ? {}
              : { updateKey: block.dataset.updateKey }),
          },
        ];
  });
  return found.length >= MIN_ENTRIES ? found : [];
}

/**
 * `live` is false while a page (the store) replaces the group's sections:
 * nothing is read then, so the group's last sections stay listed beside
 * the page entry and the way back to any of them is one click. Sections
 * read for another group are never shown under this one.
 */
export function useSubsections(
  ref: React.RefObject<HTMLElement | null>,
  group: string,
  live: boolean,
): Subsection[] {
  const [known, setKnown] = useState<{ group: string; entries: Subsection[] }>({
    group,
    entries: [],
  });

  useEffect(() => {
    const host = ref.current;
    if (host === null || !live) return;

    let current: string | null = null;
    const read = () => {
      const found = readSubsections(host);
      const key = found
        .map(
          (entry) =>
            `${String(entry.index)}:${entry.label}:${entry.updateKey ?? ""}`,
        )
        .join("|");
      if (key === current) return;
      current = key;
      setKnown({ group, entries: found });
    };

    read();
    const observer = new MutationObserver(read);
    observer.observe(host, { childList: true, subtree: true });
    return () => {
      observer.disconnect();
    };
    // `group` swaps the whole body out, so the effect must re-run to observe
    // the new one and to reset the memo of what was last reported.
  }, [ref, group, live]);

  return known.group === group ? known.entries : [];
}

/** How long the block the navigation landed on stays lit; matches the CSS. */
const HIGHLIGHT_MS = 1400;
/** Roughly the length of the smooth scroll; the light waits for it. */
const SCROLL_MS = 300;

function fullyInView(block: HTMLElement, host: HTMLElement): boolean {
  const target = block.getBoundingClientRect();
  const frame = host.getBoundingClientRect();
  return target.top >= frame.top && target.bottom <= frame.bottom;
}

function light(block: HTMLElement): void {
  block.removeAttribute("data-highlight");
  // A layout read between removing and adding restarts the animation, so a
  // second click on the same entry lights the block up again.
  block.getBoundingClientRect();
  block.setAttribute("data-highlight", "");
  window.setTimeout(() => {
    block.removeAttribute("data-highlight");
  }, HIGHLIGHT_MS);
}

/**
 * Scrolls the section's nth block into view, opening a collapsed one, and
 * lights it up: when every block already fits on the screen, nothing moves,
 * and the click would otherwise seem to do nothing. A block that has to be
 * scrolled to lights up once it has arrived.
 */
export function scrollToGroup(
  host: HTMLElement | null,
  index: number,
): boolean {
  if (host === null) return false;
  // Re-queried instead of held: the list rebuilds itself between the moment
  // the entry was read and the moment it is clicked.
  const block = blocks(host)[index];
  if (block === undefined) return false;
  if (block instanceof HTMLDetailsElement) block.open = true;
  const cardDetails = block.querySelector<HTMLDetailsElement>(
    ".settings-plugin-details",
  );
  if (cardDetails !== null) cardDetails.open = true;
  const visible = fullyInView(block, host);
  const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  block.scrollIntoView({
    block: "start",
    behavior: reduced ? "auto" : "smooth",
  });
  if (visible || reduced) {
    light(block);
    return true;
  }
  window.setTimeout(() => {
    light(block);
  }, SCROLL_MS);
  return true;
}

function blocks(host: HTMLElement): HTMLElement[] {
  return [...host.querySelectorAll<HTMLElement>(".settings-block")];
}
