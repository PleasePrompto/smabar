// @vitest-environment happy-dom
import { afterEach, expect, test } from "vitest";

import { resolveEmbed, setEmbedRoot } from "./embeds";

afterEach(() => {
  setEmbedRoot("");
});

test("wraps a titled HTTPS player for the validating loopback server", () => {
  setEmbedRoot("http://127.0.0.1:4242/embed");
  const wrapped = resolveEmbed(
    "https://www.youtube-nocookie.com/embed/abc?start=5",
    "Latest video",
  );
  expect(wrapped).not.toBeNull();
  const url = new URL(wrapped ?? "http://invalid");
  const payload = url.searchParams;
  expect(`${url.origin}${url.pathname}`).toBe("http://127.0.0.1:4242/embed");
  expect(url.hash).toBe("");
  expect(payload.get("src")).toBe(
    "https://www.youtube-nocookie.com/embed/abc?start=5",
  );
  expect(payload.get("title")).toBe("Latest video");
});

test("rejects unsafe, credentialed, untitled, and unavailable embeds", () => {
  expect(resolveEmbed("https://player.vimeo.com/video/1", "Video")).toBeNull();
  setEmbedRoot("http://127.0.0.1:4242/embed");
  for (const source of [
    "http://example.com/player",
    "javascript:alert(1)",
    "data:text/html,x",
    "file:///tmp/player.html",
    "https://user:secret@example.com/player",
  ]) {
    expect(resolveEmbed(source, "Video"), source).toBeNull();
  }
  expect(resolveEmbed("https://example.com/player", "   ")).toBeNull();
  expect(
    resolveEmbed("https://example.com/player", "x".repeat(513)),
  ).toBeNull();
  expect(
    resolveEmbed(`https://example.com/${"x".repeat(8193)}`, "Video"),
  ).toBeNull();
});
