// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { LegalStatus } from "../../ipc/legal";
import { useSmabar } from "../../store/bar";
import { LegalTab } from "./LegalTab";

const { callMock } = vi.hoisted(() => ({ callMock: vi.fn() }));
vi.mock("../../ipc/call", () => ({ call: callMock }));

let container: HTMLDivElement;
let root: Root;

/** A fresh profile: nothing accepted yet. */
function status(overrides: Partial<LegalStatus> = {}): LegalStatus {
  return {
    required: true,
    termsVersion: "2026-09-07",
    privacyVersion: "2026-09-07",
    acceptedAt: null,
    terms: {
      title: "Terms of use",
      updated: "2026-09-07",
      html: "<h2>1. Provider</h2><p>the terms body</p>",
    },
    privacy: {
      title: "Privacy notice",
      updated: "2026-08-01",
      html: "<h2>1. Controller</h2><p>the privacy body</p>",
    },
    license: {
      title: "PolyForm Shield 1.0.0",
      updated: null,
      html: "<h1>PolyForm Shield License 1.0.0</h1>",
    },
    ...overrides,
  };
}

const ACCEPTED = status({
  required: false,
  acceptedAt: Date.UTC(2026, 8, 7, 9, 30),
});

/**
 * Answers `legal_status` with `initial` and `legal_accept` with `accept()` —
 * a factory, so a rejection only exists once something awaits it.
 */
function serve(
  initial: LegalStatus,
  accept: () => Promise<LegalStatus> = () => Promise.resolve(ACCEPTED),
): void {
  callMock.mockImplementation((command: string) => {
    if (command === "legal_status") return Promise.resolve(initial);
    if (command === "legal_accept") return accept();
    if (command === "legal_decline") return Promise.resolve(null);
    if (command === "get_system_settings") {
      return Promise.resolve({ languages: ["en", "de"] });
    }
    if (command === "update_config") return Promise.resolve(null);
    return new Promise(() => undefined);
  });
}

beforeEach(() => {
  serve(status());
  useSmabar.setState(useSmabar.getInitialState(), true);
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  container.remove();
  callMock.mockReset();
});

async function interact(run: () => void): Promise<void> {
  await act(async () => {
    run();
    await Promise.resolve();
  });
}

async function render(): Promise<void> {
  await interact(() => {
    root.render(<LegalTab />);
  });
}

function buttons(label: string): HTMLButtonElement[] {
  return [...container.querySelectorAll<HTMLButtonElement>("button")].filter(
    (element) => element.textContent === label,
  );
}

function button(label: string): HTMLButtonElement {
  const [found, ...more] = buttons(label);
  if (found === undefined || more.length > 0) {
    throw new Error(`expected exactly one "${label}" button`);
  }
  return found;
}

function readme(): string {
  return container.querySelector(".settings-readme")?.innerHTML ?? "";
}

function pressed(label: string): string | null {
  return button(label).getAttribute("aria-pressed");
}

test("the tabs switch the shown document and mark the active one", async () => {
  await render();
  expect(container.querySelector(".settings-help")?.textContent).toBe(
    "smabar is ready once you accept the license and the terms of use. The privacy notice is for your information only.",
  );
  expect(readme()).toContain("the terms body");
  expect(pressed("Terms of use")).toBe("true");
  expect(pressed("License")).toBe("false");
  expect(container.textContent).toContain("As of September 7, 2026");

  await interact(() => {
    button("License").click();
  });
  expect(readme()).toContain("PolyForm Shield License 1.0.0");
  expect(pressed("License")).toBe("true");
  expect(pressed("Terms of use")).toBe("false");
  // The license carries no date of its own.
  expect(container.textContent).not.toContain("As of");

  await interact(() => {
    button("Privacy notice").click();
  });
  expect(readme()).toContain("the privacy body");
  expect(container.textContent).toContain("As of August 1, 2026");
});

test("accepting records the acceptance and leaves a read-only section", async () => {
  await render();
  await interact(() => {
    button("Accept license and terms").click();
  });
  expect(callMock).toHaveBeenCalledWith("legal_accept");
  expect(buttons("Accept license and terms")).toHaveLength(0);
  expect(buttons("Decline and quit")).toHaveLength(0);
  expect(container.querySelector(".settings-help")?.textContent).toMatch(
    /^Accepted on .+ \(terms of September 7, 2026\)\.$/,
  );
});

test("declining asks inline first and only then quits", async () => {
  await render();
  await interact(() => {
    button("Decline and quit").click();
  });
  const confirm = container.querySelector("[data-confirm-row]");
  expect(confirm?.textContent).toContain(
    "Decline the terms and quit smabar? You can accept them the next time smabar starts.",
  );
  expect(buttons("Accept license and terms")).toHaveLength(0);
  expect(
    callMock.mock.calls.some(([command]) => command === "legal_decline"),
  ).toBe(false);

  await interact(() => {
    button("Cancel").click();
  });
  expect(container.querySelector("[data-confirm-row]")).toBeNull();
  expect(buttons("Accept license and terms")).toHaveLength(1);

  await interact(() => {
    button("Decline and quit").click();
  });
  await interact(() => {
    button("Decline and quit").click();
  });
  expect(callMock).toHaveBeenCalledWith("legal_decline");
});

test("an accepted profile shows when it accepted and offers no buttons", async () => {
  serve(ACCEPTED);
  await render();
  expect(container.querySelector(".settings-help")?.textContent).toContain(
    "Accepted on",
  );
  expect(container.textContent).toContain("As of September 7, 2026");
  expect(buttons("Accept license and terms")).toHaveLength(0);
  expect(buttons("Decline and quit")).toHaveLength(0);
  expect(container.querySelector("[data-confirm-row]")).toBeNull();
});

test("changed terms say so above the documents", async () => {
  serve(status({ acceptedAt: Date.UTC(2026, 0, 1) }));
  await render();
  expect(container.querySelector(".settings-help")?.textContent).toBe(
    "The terms of use changed. Accept the current version to continue.",
  );
  expect(buttons("Accept license and terms")).toHaveLength(1);
});

test("a rejected acceptance is shown as an alert and keeps the gate", async () => {
  serve(status(), () =>
    Promise.reject(
      new Error(
        "cannot save the acceptance to /tmp/profile/legal.json: read-only",
      ),
    ),
  );
  await render();
  await interact(() => {
    button("Accept license and terms").click();
  });
  expect(container.querySelector('[role="alert"]')?.textContent).toBe(
    "The acceptance could not be saved: cannot save the acceptance to /tmp/profile/legal.json: read-only",
  );
  expect(button("Accept license and terms").disabled).toBe(false);
  expect(buttons("Decline and quit")).toHaveLength(1);
});

test("while gated the language can be switched right there", async () => {
  await render();
  // The choice names each language in itself, so anyone can find theirs.
  expect(pressed("English")).toBe("true");
  expect(pressed("Deutsch")).toBe("false");

  await interact(() => {
    button("Deutsch").click();
  });
  expect(callMock).toHaveBeenCalledWith("update_config", {
    path: "language",
    value: "de",
  });
});

test("an accepted profile leaves the language to the System tab", async () => {
  serve(ACCEPTED);
  await render();
  expect(buttons("English")).toHaveLength(0);
  expect(buttons("Deutsch")).toHaveLength(0);
});
