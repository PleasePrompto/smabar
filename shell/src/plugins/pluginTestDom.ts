export function container(html: string): HTMLDivElement {
  const div = document.createElement("div");
  const template = document.createElement("template");
  template.innerHTML = html;
  div.appendChild(template.content);
  return div;
}
