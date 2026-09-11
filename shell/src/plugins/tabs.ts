/** Activates one tab id inside a data-tabs container (buttons + panels). */
export function activateTab(container: HTMLElement, id: string): void {
  for (const tab of container.querySelectorAll<HTMLElement>("[data-tab]")) {
    tab.classList.toggle("sb-active", tab.dataset.tab === id);
  }
  for (const panel of container.querySelectorAll<HTMLElement>(
    "[data-tab-panel]",
  )) {
    panel.hidden = panel.dataset.tabPanel !== id;
  }
}

/** Wires a data-tabs container to its local buttons and panels. */
export function enhanceTabs(root: ParentNode): void {
  for (const container of root.querySelectorAll<HTMLElement>("[data-tabs]")) {
    const tabs = container.querySelectorAll<HTMLElement>("[data-tab]");
    const active =
      [...tabs].find((tab) => tab.classList.contains("sb-active")) ?? tabs[0];
    if (active === undefined) continue;
    activateTab(container, active.dataset.tab ?? "");
    container.addEventListener("click", (event) => {
      if (!(event.target instanceof Element)) return;
      const tab = event.target.closest<HTMLElement>("[data-tab]");
      if (tab === null || !container.contains(tab)) return;
      const id = tab.dataset.tab;
      if (id !== undefined) activateTab(container, id);
    });
  }
}
