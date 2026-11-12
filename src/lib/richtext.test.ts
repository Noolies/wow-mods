// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { renderRichText } from "./richtext";

describe("renderRichText", () => {
  it("strips scripts, event handlers and inline styles", () => {
    const html = renderRichText(
      '<p style="color:red" onclick="steal()">Hi</p><script>alert(1)</script><img src="x.png" onerror="alert(2)"><iframe src="https://evil.example"></iframe>',
      "html",
      null,
      "https://raw.githubusercontent.com/a/b/HEAD/",
    );
    expect(html).not.toMatch(/script|onerror|onclick|style=|iframe/i);
    expect(html).toContain("<p>Hi</p>");
  });

  it("makes README links and images absolute", () => {
    const html = renderRichText(
      "See the [wiki](docs/wiki.md).\n\n![shot](img/shot.png)",
      "markdown",
      "https://github.com/a/b/blob/HEAD/",
      "https://raw.githubusercontent.com/a/b/HEAD/",
    );
    expect(html).toContain('href="https://github.com/a/b/blob/HEAD/docs/wiki.md"');
    expect(html).toContain('src="https://raw.githubusercontent.com/a/b/HEAD/img/shot.png"');
  });

  it("drops unsafe link targets and insecure images", () => {
    const html = renderRichText('<a href="javascript:alert(1)">x</a><img src="http://insecure.example/a.png">', "html");
    expect(html).not.toContain("javascript:");
    expect(html).not.toContain("http://insecure.example");
  });

  it("escapes plain text", () => {
    expect(renderRichText("<b>not bold</b>\n\nnext", "text")).toBe("<p>&lt;b&gt;not bold&lt;/b&gt;</p><p>next</p>");
  });
});
