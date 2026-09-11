// @vitest-environment happy-dom
// Tests must not touch the network — stop happy-dom from fetching resources
// referenced by the fixtures below.
// @vitest-environment-options { "settings": { "disableCSSFileLoading": true, "disableJavaScriptFileLoading": true, "navigation": { "disableChildFrameNavigation": true } } }
import { expect, test } from "vitest";

import { assetRelativePath } from "./assets";
import { sanitizeHtml } from "./sanitize";

/** Serializes the sanitized fragment for readable assertions. */
function render(html: string): string {
  const div = document.createElement("div");
  div.appendChild(sanitizeHtml(html));
  return div.innerHTML;
}

test("returns a DocumentFragment", () => {
  expect(sanitizeHtml("<p>hi</p>")).toBeInstanceOf(DocumentFragment);
});

test("keeps allowed markup with allowed attributes", () => {
  const html =
    '<div class="a" style="color:red" title="t">' +
    "<p>Hi <strong>there</strong><br></p>" +
    "<ul><li>one</li><li>two</li></ul><hr>" +
    '<button data-action="go" data-value="7">Go</button>' +
    "</div>";
  expect(render(html)).toBe(html);
});

test("keeps plain text and entities", () => {
  expect(render("a &amp; b")).toBe("a &amp; b");
});

test("removes script tags including their content", () => {
  expect(render("<div>ok<script>window.leak = 1</script></div>")).toBe(
    "<div>ok</div>",
  );
});

test("removes forbidden tags regardless of case", () => {
  expect(render("<SCRIPT>window.leak = 1</SCRIPT><p>ok</p>")).toBe("<p>ok</p>");
});

test("removes nested forbidden tags inside allowed ones", () => {
  const html =
    "<div><iframe><script>x()</script></iframe>" +
    "<object><embed></object><template><b>t</b></template><p>ok</p></div>";
  expect(render(html)).toBe("<div><p>ok</p></div>");
});

test("removes style, link and meta entirely", () => {
  const html =
    "<style>* { display: none }</style>" +
    '<link rel="icon" href="https://x"><meta charset="utf-8"><p>ok</p>';
  expect(render(html)).toBe("<p>ok</p>");
});

test("unwraps a tag that is neither allowed nor dropped", () => {
  // marquee is obsolete, not dangerous: the tag goes, the children stay.
  expect(render("<marquee><b>hi</b></marquee>")).toBe("<b>hi</b>");
  expect(render("<font size='7'>big</font>")).toBe("big");
});

test("structural tags a real layout needs now survive", () => {
  expect(render("<h5>Title</h5>")).toBe("<h5>Title</h5>");
  expect(render("<nav><b>menu</b></nav>")).toBe("<nav><b>menu</b></nav>");
  expect(render('<section><span class="s">kept</span></section>')).toBe(
    '<section><span class="s">kept</span></section>',
  );
  expect(render("<table><tr><td>cell</td></tr></table>")).toBe(
    "<table><tr><td>cell</td></tr></table>",
  );
  expect(render("<figure><figcaption>cap</figcaption></figure>")).toBe(
    "<figure><figcaption>cap</figcaption></figure>",
  );
});

test("keeps headings h1-h4 and label", () => {
  expect(render("<h1>a</h1><h2>b</h2><h3>c</h3><h4>d</h4>")).toBe(
    "<h1>a</h1><h2>b</h2><h3>c</h3><h4>d</h4>",
  );
  expect(render("<label>City</label>")).toBe("<label>City</label>");
});

test("keeps select size so multi-row controls retain native semantics", () => {
  expect(render('<select size="2"><option>One</option></select>')).toBe(
    '<select size="2"><option>One</option></select>',
  );
});

test("strips on* event handler attributes", () => {
  expect(render('<span onclick="evil()" onmouseover="evil()">t</span>')).toBe(
    "<span>t</span>",
  );
  expect(render('<button onclick="evil()" data-action="ok">x</button>')).toBe(
    '<button data-action="ok">x</button>',
  );
});

test("strips attributes outside the allowlist", () => {
  // name= is a form-control attribute and has no meaning on a div.
  expect(
    render('<div name="n" contenteditable="true" draggable="true">t</div>'),
  ).toBe("<div>t</div>");
});

test("id and for survive, because a shadow root scopes them", () => {
  // Without these no plugin form can be labelled and no tab list can be
  // announced — and an id cannot reach anything outside its own shadow tree.
  expect(render('<label for="city">City</label><input id="city">')).toBe(
    '<label for="city">City</label><input id="city">',
  );
  expect(
    render('<div id="p" role="tabpanel" aria-labelledby="t">x</div>'),
  ).toBe('<div id="p" role="tabpanel" aria-labelledby="t">x</div>');
});

test("keeps role and aria attributes", () => {
  expect(
    render('<div role="img" aria-label="CPU load" aria-hidden="false">t</div>'),
  ).toBe('<div role="img" aria-label="CPU load" aria-hidden="false">t</div>');
});

test("keeps any data-* attribute matching the kebab-case pattern", () => {
  expect(render('<span data-lucide="cpu"></span>')).toBe(
    '<span data-lucide="cpu"></span>',
  );
  expect(
    render('<div data-chart="donut" data-value="62" data-max="100">x</div>'),
  ).toBe('<div data-chart="donut" data-value="62" data-max="100">x</div>');
  // Digits are inside the pattern; uppercase never arrives, because the HTML
  // parser lowercases attribute names.
  expect(render('<span data-col2="a">t</span>')).toBe(
    '<span data-col2="a">t</span>',
  );
});

test("keeps javascript:-free values but drops javascript: in data attributes", () => {
  expect(render('<span data-value="javascript:alert(1)">x</span>')).toBe(
    "<span>x</span>",
  );
});

test("drops attributes carrying javascript: URLs", () => {
  expect(render('<span title="javascript:alert(1)">x</span>')).toBe(
    "<span>x</span>",
  );
  // Whitespace/control chars inside the scheme must not bypass the check.
  expect(render('<span title="Java\tscri\npt:alert(1)">x</span>')).toBe(
    "<span>x</span>",
  );
});

test("keeps img only with http(s): or data:image/ sources", () => {
  expect(render('<img src="https://x/y.png" alt="a">')).toBe(
    '<img src="https://x/y.png" alt="a">',
  );
  expect(render('<img src="http://x/y.png" alt="a">')).toBe(
    '<img src="http://x/y.png" alt="a">',
  );
  expect(render('<img src="data:image/png;base64,AAAA">')).toBe(
    '<img src="data:image/png;base64,AAAA">',
  );
  expect(render('<img src="file:///etc/passwd">')).toBe("<img>");
  expect(render('<img src="javascript:alert(1)">')).toBe("<img>");
  expect(render('<img src="data:text/html,<script>x</script>">')).toBe("<img>");
});

test("img event handlers die even next to a valid src", () => {
  expect(
    render('<img src="https://x/y.png" onerror="evil()" onload="evil()">'),
  ).toBe('<img src="https://x/y.png">');
});

test("ignores src on non-media tags", () => {
  expect(render('<span src="https://x">t</span>')).toBe("<span>t</span>");
});

test("keeps links only with http(s) hrefs", () => {
  expect(render('<a href="https://x/page">go</a>')).toBe(
    '<a href="https://x/page">go</a>',
  );
  expect(render('<a href="http://x">go</a>')).toBe('<a href="http://x">go</a>');
  for (const href of [
    "javascript:alert(1)",
    "Java\tscript:alert(1)",
    "data:text/html,<script>x</script>",
    "file:///etc/passwd",
    "ftp://x",
    "/relative",
  ]) {
    expect(render(`<a href="${href}">go</a>`), href).toBe("<a>go</a>");
  }
  // href stays off other tags.
  expect(render('<span href="https://x">t</span>')).toBe("<span>t</span>");
});

test("removes comments", () => {
  expect(render("<!-- secret --><p>ok</p>")).toBe("<p>ok</p>");
});

test("keeps allowlisted input types and strips unknown ones", () => {
  // name= is now kept (native form semantics); the event handler is not.
  const html =
    '<input data-field="nummer" placeholder="Sendungsnummer" ' +
    'type="password" onfocus="window.leak=1" name="n" value="x">';
  expect(render(html)).toBe(
    '<input data-field="nummer" placeholder="Sendungsnummer" ' +
      'type="password" name="n" value="x">',
  );
  expect(render('<input type="checkbox" checked disabled>')).toBe(
    '<input type="checkbox" checked="" disabled="">',
  );
  expect(render('<input type="number" value="7">')).toBe(
    '<input type="number" value="7">',
  );
  // Still out: a tile uploads nothing and never submits natively.
  for (const type of ["file", "submit", "image", "button", "reset"]) {
    expect(render(`<input type="${type}">`)).toBe("<input>");
  }
});

test("input-only attributes stay off other tags", () => {
  expect(render('<div value="x" checked disabled>t</div>')).toBe(
    "<div>t</div>",
  );
});

test("a form is kept but can never navigate", () => {
  expect(
    render(
      '<form action="https://evil.example" method="post" target="_blank">' +
        '<input data-field="a"></form>',
    ),
  ).toBe('<form><input data-field="a"></form>');
});

test("native interaction survives so a plugin needs no JavaScript", () => {
  expect(
    render('<button commandfor="d" command="show-modal">open</button>'),
  ).toBe('<button commandfor="d" command="show-modal">open</button>');
  expect(render('<dialog id="d"><p>hi</p></dialog>')).toBe(
    '<dialog id="d"><p>hi</p></dialog>',
  );
  expect(render('<div popover id="m">menu</div>')).toBe(
    '<div popover="" id="m">menu</div>',
  );
  expect(render('<button popovertarget="m">open</button>')).toBe(
    '<button popovertarget="m">open</button>',
  );
  expect(
    render("<details open><summary>More</summary><p>body</p></details>"),
  ).toBe('<details open=""><summary>More</summary><p>body</p></details>');
});

test("a command nothing listens for is refused instead of silently dead", () => {
  expect(render('<button commandfor="d" command="--mine">x</button>')).toBe(
    '<button commandfor="d">x</button>',
  );
});

test("inline svg is allowed for shapes but never for its escape hatches", () => {
  expect(
    render(
      '<svg viewBox="0 0 10 10"><circle cx="5" cy="5" r="4"></circle></svg>',
    ),
  ).toBe(
    '<svg viewBox="0 0 10 10"><circle cx="5" cy="5" r="4"></circle></svg>',
  );
  // The ways out of an SVG subtree.
  expect(render('<svg><use href="https://evil/x#i"></use></svg>')).toBe(
    "<svg></svg>",
  );
  expect(
    render("<svg><foreignObject><iframe></iframe></foreignObject></svg>"),
  ).toBe("<svg></svg>");
  expect(render("<svg><script>x()</script></svg>")).toBe("<svg></svg>");
  expect(render('<svg onload="x()"><path d="M0 0"></path></svg>')).toBe(
    '<svg><path d="M0 0"></path></svg>',
  );
});

/** Stand-in for the real resolver: same path check, visible output. */
const resolveAsset = (value: string): string | null => {
  const relative = assetRelativePath(value);
  return relative === null ? null : `resolved://${relative}`;
};

function assetSrc(html: string): string | null {
  return resolvedAssets(html).querySelector("img")?.getAttribute("src") ?? null;
}

function resolvedAssets(html: string): HTMLDivElement {
  const div = document.createElement("div");
  div.appendChild(sanitizeHtml(html, resolveAsset));
  return div;
}

test("sb-asset resolves a relative path inside the plugin's own directory", () => {
  expect(assetSrc('<img src="sb-asset:logo.png">')).toBe("resolved://logo.png");
  expect(assetSrc('<img src="sb-asset:icons/sun.svg">')).toBe(
    "resolved://icons/sun.svg",
  );
});

test("sb-asset resolves local audio, video, sources, and posters", () => {
  const media = resolvedAssets(
    '<video src="sb-asset:media/clip.mp4" poster="sb-asset:media/poster.jpg">' +
      '<source src="sb-asset:media/clip.webm"><track src="sb-asset:media/en.vtt" kind="captions"></video>' +
      '<audio src="sb-asset:media/song.ogg"><source src="sb-asset:media/song.mp3"></audio>',
  );
  expect(media.querySelector("video")?.getAttribute("src")).toBe(
    "resolved://media/clip.mp4",
  );
  expect(media.querySelector("video")?.getAttribute("poster")).toBe(
    "resolved://media/poster.jpg",
  );
  expect(media.querySelector("source")?.getAttribute("src")).toBe(
    "resolved://media/clip.webm",
  );
  expect(media.querySelector("track")?.hasAttribute("src")).toBe(false);
  expect(media.querySelector("audio")?.getAttribute("src")).toBe(
    "resolved://media/song.ogg",
  );
  expect(media.querySelectorAll("source")[1]?.getAttribute("src")).toBe(
    "resolved://media/song.mp3",
  );
});

test("sb-asset refuses escaping video, source, and poster paths", () => {
  const media = resolvedAssets(
    '<video src="sb-asset:../clip.mp4" poster="sb-asset:../poster.jpg">' +
      '<source src="sb-asset:../clip.webm"></video>',
  );
  expect(media.querySelector("video")?.hasAttribute("src")).toBe(false);
  expect(media.querySelector("video")?.hasAttribute("poster")).toBe(false);
  expect(media.querySelector("source")?.hasAttribute("src")).toBe(false);
});

test("sb-asset refuses every way out of that directory", () => {
  const escapes = [
    "sb-asset:../../etc/passwd",
    "sb-asset:/etc/passwd",
    "sb-asset:\\\\server\\share",
    "sb-asset:C:/Windows/win.ini",
    "sb-asset:icons/../../../secret",
    "sb-asset:",
    "sb-asset:   ",
    "sb-asset:a//b",
    "sb-asset:./x.png",
    "sb-asset:x.png?../y",
  ];
  for (const value of escapes) {
    expect(assetSrc(`<img src="${value}">`)).toBeNull();
  }
});

test("sb-asset is inert without a resolver", () => {
  const div = document.createElement("div");
  div.appendChild(sanitizeHtml('<img src="sb-asset:logo.png">'));
  expect(div.querySelector("img")?.hasAttribute("src")).toBe(false);
});

test("a resolver leaves http and data sources alone", () => {
  expect(assetSrc('<img src="https://example.com/a.png">')).toBe(
    "https://example.com/a.png",
  );
  expect(assetSrc('<img src="data:image/png;base64,AA">')).toBe(
    "data:image/png;base64,AA",
  );
});

test("an svg stays labelled for screen readers", () => {
  // Found by the capability probe: refusing aria inside <svg> would make
  // every plugin-drawn chart unannounceable.
  expect(
    render('<svg aria-label="CPU" role="img"><path d="M0 0"></path></svg>'),
  ).toBe('<svg aria-label="CPU" role="img"><path d="M0 0"></path></svg>');
});

test("a button keeps its type, so writing one is not reported as a mistake", () => {
  expect(render('<button type="button">x</button>')).toBe(
    '<button type="button">x</button>',
  );
  expect(render('<button type="submit">x</button>')).toBe(
    '<button type="submit">x</button>',
  );
  expect(render('<button type="image">x</button>')).toBe("<button>x</button>");
});

test("details can form an exclusive accordion", () => {
  expect(render('<details name="g"><summary>a</summary></details>')).toBe(
    '<details name="g"><summary>a</summary></details>',
  );
});

test("the aria states a real component needs survive", () => {
  const html =
    '<button aria-pressed="true" aria-haspopup="menu" aria-busy="false">x</button>';
  expect(render(html)).toBe(html);
  expect(render('<time datetime="2026-08-23">today</time>')).toBe(
    '<time datetime="2026-08-23">today</time>',
  );
});
