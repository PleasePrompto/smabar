export function buildSelectOptions(
  source: HTMLSelectElement,
  list: HTMLElement,
  sequence: number,
): void {
  const groups = new Map<HTMLOptGroupElement, HTMLElement>();
  Array.from(source.options).forEach((option, index) => {
    const optionGroup = option.closest("optgroup");
    if (
      option.hidden ||
      (optionGroup instanceof HTMLOptGroupElement && optionGroup.hidden)
    ) {
      return;
    }
    let container = list;
    if (optionGroup instanceof HTMLOptGroupElement) {
      let group = groups.get(optionGroup);
      if (group === undefined) {
        group = source.ownerDocument.createElement("div");
        group.className = "sb-select-group";
        group.setAttribute("role", "group");
        group.setAttribute("aria-label", optionGroup.label);
        group.setAttribute("aria-disabled", String(optionGroup.disabled));
        groups.set(optionGroup, group);
        list.append(group);
      }
      container = group;
    }
    const item = source.ownerDocument.createElement("div");
    item.className = "sb-select-option";
    item.id = `sb-select-option-${String(sequence)}-${String(index)}`;
    item.dataset.sbIndex = String(index);
    item.setAttribute("role", "option");
    item.setAttribute(
      "aria-disabled",
      String(option.disabled || optionGroup?.disabled === true),
    );
    item.textContent =
      option.label || option.text || option.textContent.trim() || option.value;
    container.append(item);
  });
}
