// @vitest-environment happy-dom
import { expect, test } from "vitest";

import { enhanceBadges, enhanceTooltips } from "./decorators";

function container(html: string): HTMLDivElement {
  const div = document.createElement("div");
  const template = document.createElement("template");
  template.innerHTML = html;
  div.appendChild(template.content);
  return div;
}

test("plugin titles use themed tooltips without consuming frame titles", () => {
  const root = container(
    '<span id="existing-description">Existing</span>' +
      '<button id="labelled" title="Save image" aria-describedby="existing-description">Save</button>' +
      '<span id="iconOnly" title="Refresh"></span>' +
      '<span id="blank" title="   "></span>' +
      '<iframe id="player" title="Latest video"></iframe>',
  );
  enhanceTooltips(root);

  const labelled = root.querySelector("#labelled");
  expect(labelled?.hasAttribute("title")).toBe(false);
  expect(labelled?.getAttribute("data-sb-tooltip")).toBe("Save image");
  expect(labelled?.hasAttribute("aria-label")).toBe(false);
  const descriptionIds =
    labelled?.getAttribute("aria-describedby")?.split(/\s+/) ?? [];
  expect(descriptionIds).toContain("existing-description");
  const generatedId = descriptionIds.find((id) =>
    id.startsWith("sb-tooltip-description-"),
  );
  expect(generatedId).toBeDefined();
  expect(root.querySelector(`#${generatedId ?? "missing"}`)?.textContent).toBe(
    "Save image",
  );
  expect(root.querySelector("#iconOnly")?.getAttribute("aria-label")).toBe(
    "Refresh",
  );

  const blank = root.querySelector("#blank");
  expect(blank?.hasAttribute("title")).toBe(false);
  expect(blank?.hasAttribute("data-sb-tooltip")).toBe(false);

  const player = root.querySelector("#player");
  expect(player?.getAttribute("title")).toBe("Latest video");
  expect(player?.hasAttribute("data-sb-tooltip")).toBe(false);
});

test("badges render text and an empty value as a dot", () => {
  const root = container(
    '<div data-badge="3">Inbox</div><span data-badge="">Online</span>',
  );
  enhanceBadges(root);
  const badges = root.querySelectorAll(".sb-data-badge");
  expect(badges).toHaveLength(2);
  expect(badges[0]?.textContent).toBe("3");
  expect(badges[0]?.getAttribute("aria-hidden")).toBe("true");
  expect(badges[1]?.classList.contains("sb-data-badge-dot")).toBe(true);
  expect(root.querySelectorAll(".sb-badge-anchor")).toHaveLength(2);
});
