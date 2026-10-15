import { describe, expect, it } from "vitest";
import { defaultConfig } from "./fakeTransport";

describe("defaultConfig", () => {
  it("matches the engine defaults for JavaScript rendering (spec 014)", () => {
    const r = defaultConfig().rendering;
    expect(r.mode).toBe("raw");
    expect(r.max_render_secs).toBe(10);
    expect(r.window.preset).toBe("auto");
    expect(r.block).toEqual({ images: true, media: true, fonts: true, stylesheets: false, third_party: false });
    expect(r.concurrency).toBeNull();
    expect([r.js_content_min_percent, r.js_content_min_words, r.hidden_content_percent]).toEqual([20, 50, 50]);
  });
});
