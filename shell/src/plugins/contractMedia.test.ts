// @vitest-environment happy-dom
// @vitest-environment-options { "settings": { "navigation": { "disableChildFrameNavigation": true } } }
import { expect, test } from "vitest";

import tauriConfig from "../../../crates/smabar/tauri.conf.json";
import contract from "../../../ui-kit/contract.json";
import { ASSET_SCHEME, assetRelativePath } from "./assets";
import { sanitizeHtml } from "./sanitize";

const sorted = (values: Iterable<string>): string[] => [...values].sort();

function tagSequence(root: ParentNode): string[] {
  return [...root.querySelectorAll("*")].map((el) => el.tagName.toLowerCase());
}

test("the documented sb-asset scheme is the one the sanitizer implements", () => {
  expect(contract.sanitizer.imgSrcRule).toContain(ASSET_SCHEME);
  expect(contract.media.images).toContain(ASSET_SCHEME);
  expect(contract.media.videos).toContain(ASSET_SCHEME);
  expect(contract.media.audio).toContain(ASSET_SCHEME);
  expect(contract.media.tracks).not.toContain(ASSET_SCHEME);
  expect(contract.media.tracks).toContain("data:text/vtt");
  expect(contract.media.assetScheme).toContain("app.data_dir");
  expect(assetRelativePath(`${ASSET_SCHEME}icons/sun.png`)).toBe(
    "icons/sun.png",
  );
  expect(assetRelativePath(`${ASSET_SCHEME}../other/secret`)).toBeNull();
});

test("the bundled CSP matches the documented media sources", () => {
  const csp = tauriConfig.app.security.csp;
  const documentedCspSources = (description: string): string[] => {
    const sources = new Set<string>();
    for (const source of description.match(
      /https?:\/\/|data:(?:(?:image|video|audio)\/|text\/vtt)|sb-asset:/g,
    ) ?? []) {
      if (source === "http://") sources.add("http:");
      else if (source === "https://") sources.add("https:");
      else if (source.startsWith("data:")) sources.add("data:");
      else {
        sources.add("asset:");
        sources.add("http://asset.localhost");
      }
    }
    return sorted(sources);
  };
  const externalCspSources = (directive: string): string[] =>
    sorted(
      directive
        .split(/\s+/)
        .filter((source) => source !== "'self'" && source !== "blob:"),
    );

  expect(externalCspSources(csp["img-src"])).toEqual(
    documentedCspSources(contract.media.images),
  );
  expect(externalCspSources(csp["media-src"])).toEqual(
    documentedCspSources(
      `${contract.media.videos} ${contract.media.audio} ${contract.media.tracks}`,
    ),
  );
  expect(csp["frame-src"]).toBe("http://127.0.0.1:*");
  expect(contract.media.embeds).toContain("NO Tauri capability");
  expect(contract.media.appIdentity).toBe("https://dev.smabar.desktop/");
  expect(contract.media.providerIdentity).toContain(
    "registered HTTPS app identity",
  );
  expect(contract.media.providerIdentity).toContain("arbitrary provider URLs");
});

test("every documented media example survives as loadable markup", () => {
  const resolveContractAsset = (value: string): string | null => {
    const relative = assetRelativePath(value);
    return relative === null ? null : `asset://localhost/${relative}`;
  };
  const resolveContractEmbed = (value: string, title: string): string =>
    `http://127.0.0.1:4242/embed?${new URLSearchParams({ src: value, title })}`;

  for (const [name, example] of Object.entries(contract.media.examples)) {
    const parsed = document.createElement("template");
    parsed.innerHTML = example;
    const fragment = sanitizeHtml(
      example,
      resolveContractAsset,
      () => undefined,
      resolveContractEmbed,
    );
    expect(tagSequence(fragment), name).toEqual(tagSequence(parsed.content));
    for (const media of fragment.querySelectorAll(
      "img,audio,video,source,track",
    )) {
      expect(media.getAttribute("src"), name).not.toMatch(/^sb-asset:/);
    }
    for (const frame of fragment.querySelectorAll("iframe")) {
      expect(frame.getAttribute("src"), name).toMatch(
        /^http:\/\/127\.0\.0\.1:4242\/embed\?/,
      );
    }
  }
});
