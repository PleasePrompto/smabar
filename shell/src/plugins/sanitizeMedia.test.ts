// @vitest-environment happy-dom
import type { IFetchInterceptor, Window as HappyDomWindow } from "happy-dom";
import { expect, test } from "vitest";

import {
  EMBED_ALLOW,
  EMBED_REFERRER_POLICY,
  EMBED_SANDBOX,
  sanitizeHtml,
} from "./sanitize";

function render(html: string): string {
  const div = document.createElement("div");
  div.appendChild(sanitizeHtml(html));
  return div.innerHTML;
}

test("keeps video and source with http(s)/data:video sources and safe attrs", () => {
  expect(render('<video src="https://x/clip.mp4" controls></video>')).toBe(
    '<video src="https://x/clip.mp4" controls=""></video>',
  );
  expect(render('<video src="data:video/mp4;base64,AAAA"></video>')).toBe(
    '<video src="data:video/mp4;base64,AAAA"></video>',
  );
  expect(
    render(
      '<video controls loop playsinline preload="metadata" poster="https://x/p.jpg">' +
        '<source src="http://x/clip.webm"></video>',
    ),
  ).toBe(
    '<video controls="" loop="" playsinline="" preload="metadata" poster="https://x/p.jpg">' +
      '<source src="http://x/clip.webm"></video>',
  );
  expect(render('<video src="javascript:alert(1)"></video>')).toBe(
    "<video></video>",
  );
  expect(render('<video src="data:text/html,x" onplay="evil()"></video>')).toBe(
    "<video></video>",
  );
  expect(render('<video poster="javascript:alert(1)"></video>')).toBe(
    "<video></video>",
  );
  expect(render('<source src="data:text/html,x">')).toBe("<source>");
  expect(render('<div controls poster="https://x/p.jpg">t</div>')).toBe(
    "<div>t</div>",
  );
});

test("keeps the existing picture container around a sanitized image", () => {
  expect(
    render(
      '<picture><img src="https://example.com/photo.jpg" alt="Photo"></picture>',
    ),
  ).toBe(
    '<picture><img src="https://example.com/photo.jpg" alt="Photo"></picture>',
  );
});

test("autoplay is muted and moving video remains pausable", () => {
  expect(render('<video src="https://x/a.mp4" autoplay></video>')).toBe(
    '<video src="https://x/a.mp4" autoplay="" muted="" controls=""></video>',
  );
  expect(render('<video src="https://x/a.mp4" autoplay muted></video>')).toBe(
    '<video src="https://x/a.mp4" autoplay="" muted="" controls=""></video>',
  );
  expect(render('<audio src="https://x/a.mp3" autoplay></audio>')).toBe(
    '<audio src="https://x/a.mp3" autoplay="" muted=""></audio>',
  );
});

test("reduced motion starts autoplay video paused", () => {
  const original = Object.getOwnPropertyDescriptor(window, "matchMedia");
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: () => ({ matches: true }),
  });
  try {
    expect(render('<video src="https://x/a.mp4" autoplay loop></video>')).toBe(
      '<video src="https://x/a.mp4" loop="" muted="" controls=""></video>',
    );
  } finally {
    if (original === undefined) {
      Reflect.deleteProperty(window, "matchMedia");
    } else {
      Object.defineProperty(window, "matchMedia", original);
    }
  }
});

test("keeps playable audio sources and WebVTT tracks", () => {
  const html =
    '<audio controls preload="metadata"><source src="https://x/audio.ogg" type="audio/ogg"></audio>' +
    '<video controls crossorigin><track src="data:text/vtt;charset=utf-8,WEBVTT" kind="captions" srclang="en" label="English" default></video>';
  expect(render(html)).toBe(
    '<audio controls="" preload="metadata"><source src="https://x/audio.ogg" type="audio/ogg"></audio>' +
      '<video controls="" crossorigin="anonymous"><track src="data:text/vtt;charset=utf-8,WEBVTT" kind="captions" srclang="en" label="English" default=""></video>',
  );
  expect(render('<audio crossorigin="anonymous"></audio>')).toBe(
    '<audio crossorigin="anonymous"></audio>',
  );
  expect(render('<video crossorigin="use-credentials"></video>')).toBe(
    "<video></video>",
  );
  expect(render('<track src="data:text/html,x" kind="invalid">')).toBe(
    "<track>",
  );
  expect(render('<track src="data:text/vttx,x" kind="captions">')).toBe(
    '<track kind="captions">',
  );
  expect(render('<track src="sb-asset:captions.vtt" kind="captions">')).toBe(
    '<track kind="captions">',
  );
});

test("reserved Tauri hosts cannot bypass the sb-asset path guard", () => {
  for (const host of [
    "asset.localhost",
    "asset.localhost.",
    "ipc.localhost",
    "tauri.localhost",
  ]) {
    expect(
      render(`<video src="http://${host}/absolute/path.mp4"></video>`),
    ).toBe("<video></video>");
    expect(render(`<img src="https://${host}/absolute/path.png">`)).toBe(
      "<img>",
    );
    expect(
      render(
        `<div data-sb-lightbox><a href="http://${host}/absolute/path.png"><img src="https://example.com/thumb.png"></a></div>`,
      ),
    ).toBe(
      '<div data-sb-lightbox=""><a><img src="https://example.com/thumb.png"></a></div>',
    );
  }
});

test("refused frames do not fetch their original URL while parsing", async () => {
  const happyWindow = window as unknown as HappyDomWindow;
  const previous = happyWindow.happyDOM.settings.fetch.interceptor;
  const requested: string[] = [];
  const interceptor: IFetchInterceptor = {
    beforeAsyncRequest: ({ request, window: requestWindow }) => {
      requested.push(request.url);
      return Promise.resolve(new requestWindow.Response(""));
    },
  };
  happyWindow.happyDOM.settings.fetch.interceptor = interceptor;
  try {
    sanitizeHtml(
      '<iframe src="https://provider.example/player" title="Video"></iframe>',
    );
    await happyWindow.happyDOM.waitUntilComplete();
    expect(requested).toEqual([]);
  } finally {
    happyWindow.happyDOM.settings.fetch.interceptor = previous;
  }
});

const resolvePlayer = (value: string, title: string): string | null => {
  const url = new URL(value);
  return url.protocol === "https:" && !url.username && !url.password
    ? `http://127.0.0.1:4242/embed?${new URLSearchParams({ src: url.href, title })}`
    : null;
};

test("rewrites HTTPS players through the fixed sandbox", () => {
  const drops: string[] = [];
  const fragment = sanitizeHtml(
    '<iframe class="sb-media" style="min-height:200px" width="320" height="200" ' +
      'src="https://www.youtube-nocookie.com/embed/abc" title="Latest video" ' +
      'allow="camera; microphone" sandbox="allow-top-navigation" srcdoc="evil"></iframe>',
    () => null,
    (drop) => drops.push(drop.what),
    resolvePlayer,
  );
  const frame = fragment.querySelector("iframe");
  expect(frame?.getAttribute("src")).toContain("http://127.0.0.1:4242/embed?");
  expect(frame?.getAttribute("title")).toBe("Latest video");
  expect(frame?.getAttribute("allow")).toBe(EMBED_ALLOW);
  expect(frame?.getAttribute("sandbox")).toBe(EMBED_SANDBOX);
  expect(frame?.getAttribute("referrerpolicy")).toBe(EMBED_REFERRER_POLICY);
  expect(frame?.getAttribute("loading")).toBe("lazy");
  expect(frame?.hasAttribute("allowfullscreen")).toBe(true);
  expect(frame?.hasAttribute("srcdoc")).toBe(false);
  expect(frame?.className).toBe("sb-media");
  expect(frame?.getAttribute("width")).toBe("320");
  expect(frame?.getAttribute("height")).toBe("200");
  expect(drops).toEqual(["iframe[allow]", "iframe[sandbox]", "iframe[srcdoc]"]);
});

test("refuses document URLs that cannot be isolated", () => {
  for (const source of [
    "http://example.com/player",
    "javascript:alert(1)",
    "data:text/html,x",
    "file:///tmp/player.html",
    "sb-asset:player.html",
    "https://user:secret@example.com/player",
  ]) {
    const fragment = sanitizeHtml(
      `<iframe src="${source}" title="Video"></iframe>`,
      () => null,
      () => undefined,
      resolvePlayer,
    );
    expect(fragment.querySelector("iframe"), source).toBeNull();
  }
  expect(
    sanitizeHtml(
      '<iframe src="https://example.com/player"></iframe>',
      () => null,
      () => undefined,
      resolvePlayer,
    ).querySelector("iframe"),
  ).toBeNull();
});

test("srcdoc and data:text/html vectors never survive", () => {
  expect(render('<iframe srcdoc="<script>evil()</script>"></iframe>')).toBe("");
  expect(render('<div srcdoc="<script>evil()</script>">t</div>')).toBe(
    "<div>t</div>",
  );
  expect(render('<embed src="data:text/html,<script>x</script>">')).toBe("");
});
