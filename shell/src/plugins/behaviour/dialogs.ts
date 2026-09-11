/**
 * Dialog-based components: the command palette and the image lightbox.
 *
 * Both sit on a native `<dialog>`, so opening, the backdrop, Escape and the
 * focus trap are the browser's job. A plugin opens one with the markup the
 * sanitizer already allows:
 *
 *     <button commandfor="palette" command="show-modal">Search</button>
 *     <dialog id="palette" class="sb-modal sb-command" data-sb-command>…</dialog>
 *
 * There is deliberately no global Ctrl+K: the bar is a dock window that
 * usually does not hold keyboard focus, and a document-wide shortcut would
 * fire into whichever plugin happened to render a palette first.
 */
import { t } from "../../i18n/t";

import { behaviour, queryAll } from "./delegate";

let itemSequence = 0;

/** The palette's visible result items. */
function commandItems(dialog: ParentNode): HTMLElement[] {
  return queryAll(dialog, ".sb-command__item:not([hidden])");
}

/**
 * Marks one result active.
 *
 * APG combobox: DOM focus stays in the input and the active option is named
 * by aria-activedescendant, so typing continues uninterrupted.
 */
function setActiveItem(dialog: HTMLElement, item: HTMLElement | null): void {
  const input = dialog.querySelector(".sb-command__input");
  for (const previous of queryAll(dialog, ".sb-command__item.is-active")) {
    previous.classList.remove("is-active");
    previous.setAttribute("aria-selected", "false");
  }
  if (input === null) return;
  if (item === null) {
    input.removeAttribute("aria-activedescendant");
    return;
  }
  if (item.id === "") {
    itemSequence += 1;
    item.id = `sb-command-item-${String(itemSequence)}`;
  }
  item.classList.add("is-active");
  item.setAttribute("aria-selected", "true");
  input.setAttribute("aria-activedescendant", item.id);
  item.scrollIntoView({ block: "nearest" });
}

/** Hides non-matching results, empty groups and the empty state. */
function filterCommand(dialog: HTMLElement, query: string): void {
  const needle = query.trim().toLowerCase();
  let anyVisible = false;
  for (const item of queryAll(dialog, ".sb-command__item")) {
    const match = item.textContent.toLowerCase().includes(needle);
    item.hidden = !match;
    if (match) anyVisible = true;
  }
  for (const group of queryAll(dialog, ".sb-command__group")) {
    group.hidden =
      group.querySelector(".sb-command__item:not([hidden])") === null;
  }
  const empty = dialog.querySelector<HTMLElement>(".sb-command__empty");
  if (empty !== null) empty.hidden = anyVisible;
  setActiveItem(dialog, commandItems(dialog)[0] ?? null);
}

behaviour("input", ".sb-command__input", (input) => {
  const dialog = input.closest<HTMLElement>("dialog[data-sb-command]");
  if (dialog !== null && input instanceof HTMLInputElement) {
    filterCommand(dialog, input.value);
  }
});

// Activating a result always dismisses the palette — by mouse or by the
// synthetic click Enter produces below.
behaviour("click", ".sb-command__item", (item) => {
  item.closest<HTMLDialogElement>("dialog[data-sb-command]")?.close();
});

behaviour("keydown", ".sb-command__input", (input, event) => {
  if (!(event instanceof KeyboardEvent)) return;
  const dialog = input.closest<HTMLElement>("dialog[data-sb-command]");
  if (dialog === null) return;
  const items = commandItems(dialog);
  if (items.length === 0) return;
  const active = dialog.querySelector<HTMLElement>(
    ".sb-command__item.is-active",
  );
  const current = active === null ? -1 : items.indexOf(active);

  // Clamped rather than wrapping: a result list is read top to bottom, and
  // jumping from the last hit back to the first reads as a reset.
  const last = items.length - 1;
  const targets: Record<string, number | undefined> = {
    ArrowDown: current < 0 ? 0 : Math.min(current + 1, last),
    ArrowUp: current < 0 ? last : Math.max(current - 1, 0),
    Home: 0,
    End: last,
  };
  const index = targets[event.key];
  if (index !== undefined) {
    event.preventDefault();
    setActiveItem(dialog, items[index] ?? null);
    return;
  }
  if (event.key === "Enter" && current >= 0) {
    event.preventDefault();
    items[current]?.click();
  }
});

// `close` does not bubble, so the capture-phase delegation is what catches
// it: every open starts from a cleared filter.
behaviour("close", "dialog[data-sb-command]", (dialog) => {
  const input = dialog.querySelector("input.sb-command__input");
  if (input instanceof HTMLInputElement) input.value = "";
  filterCommand(dialog, "");
});

/** State of the one open lightbox: its gallery links and current index. */
let galleryLinks: HTMLAnchorElement[] = [];
let galleryIndex = 0;
let galleryOpener: HTMLElement | null = null;

/** The lightbox dialog for a gallery, created once per plugin shadow root. */
function ensureLightbox(gallery: HTMLElement): HTMLDialogElement | null {
  const root = gallery.getRootNode();
  if (!(root instanceof ShadowRoot || root instanceof Document)) return null;
  const existing = root.querySelector("dialog.sb-lightbox");
  if (existing instanceof HTMLDialogElement) return existing;

  const dialog = document.createElement("dialog");
  dialog.className = "sb-lightbox";
  dialog.setAttribute("aria-label", t("kit.lightbox"));

  const close = document.createElement("button");
  close.type = "button";
  close.className = "sb-lightbox__close";
  close.setAttribute("aria-label", t("kit.close"));
  close.autofocus = true;
  close.textContent = "×";

  const figure = document.createElement("figure");
  figure.className = "sb-lightbox__figure";
  const image = document.createElement("img");
  image.className = "sb-lightbox__img";
  const caption = document.createElement("figcaption");
  caption.className = "sb-lightbox__caption";
  figure.append(image, caption);

  const counter = document.createElement("div");
  counter.className = "sb-lightbox__counter";
  counter.setAttribute("aria-live", "polite");

  const nav = (suffix: string, label: string) => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = `sb-lightbox__nav sb-lightbox__nav--${suffix}`;
    button.setAttribute("aria-label", label);
    return button;
  };

  dialog.append(
    close,
    nav("prev", t("kit.previous")),
    nav("next", t("kit.next")),
    figure,
    counter,
  );
  dialog.addEventListener("close", () => {
    if (galleryOpener?.isConnected === true) galleryOpener.focus();
    galleryOpener = null;
  });
  // Inside the plugin's own root, so the adopted kit stylesheet reaches it.
  // The open dialog is lifted into the top layer either way.
  gallery.append(dialog);
  return dialog;
}

/** Shows one gallery image, wrapping the index at both ends. */
function showImage(dialog: HTMLDialogElement, at: number): void {
  const total = galleryLinks.length;
  if (total === 0) return;
  galleryIndex = ((at % total) + total) % total;
  const link = galleryLinks[galleryIndex];
  if (link === undefined) return;
  const thumb = link.querySelector("img");
  const figcaption = link.querySelector("figcaption");
  const text = (figcaption?.textContent ?? thumb?.alt ?? "").trim();

  const image = dialog.querySelector<HTMLImageElement>(".sb-lightbox__img");
  if (image !== null) {
    image.src = link.href;
    image.alt = thumb?.alt !== undefined && thumb.alt !== "" ? thumb.alt : text;
  }
  const caption = dialog.querySelector<HTMLElement>(".sb-lightbox__caption");
  if (caption !== null) {
    caption.textContent = text;
    caption.hidden = text === "";
  }
  const single = total < 2;
  for (const button of queryAll(dialog, ".sb-lightbox__nav")) {
    button.hidden = single;
  }
  const counter = dialog.querySelector<HTMLElement>(".sb-lightbox__counter");
  if (counter !== null) {
    counter.hidden = single;
    counter.textContent = `${String(galleryIndex + 1)}/${String(total)}`;
  }
}

behaviour("click", ".sb-lightbox__nav", (button) => {
  const dialog = button.closest<HTMLDialogElement>("dialog.sb-lightbox");
  if (dialog === null) return;
  const step = button.classList.contains("sb-lightbox__nav--prev") ? -1 : 1;
  showImage(dialog, galleryIndex + step);
});

behaviour("click", "[data-sb-lightbox] a[href]", (link, event) => {
  // Only links wrapping a thumbnail are gallery items; a plain text link
  // inside the gallery keeps behaving like a link.
  if (
    !(link instanceof HTMLAnchorElement) ||
    link.querySelector("img") === null
  ) {
    return;
  }
  const gallery = link.closest<HTMLElement>("[data-sb-lightbox]");
  if (gallery === null) return;
  event.preventDefault();
  const dialog = ensureLightbox(gallery);
  if (dialog === null) return;
  galleryLinks = queryAll(gallery, "a[href]").filter(
    (candidate): candidate is HTMLAnchorElement =>
      candidate instanceof HTMLAnchorElement &&
      candidate.querySelector("img") !== null,
  );
  galleryOpener = link;
  showImage(dialog, galleryLinks.indexOf(link));
  dialog.showModal();
});

behaviour("keydown", "dialog.sb-lightbox", (dialog, event) => {
  if (!(event instanceof KeyboardEvent) || galleryLinks.length < 2) return;
  if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
  if (!(dialog instanceof HTMLDialogElement) || !dialog.open) return;
  event.preventDefault();
  showImage(dialog, galleryIndex + (event.key === "ArrowLeft" ? -1 : 1));
});
