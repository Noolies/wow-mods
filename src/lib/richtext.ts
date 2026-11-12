// Turns addon descriptions and release notes (markdown or HTML from GitHub,
// CurseForge and Tukui) into safe HTML. Everything passes through DOMPurify:
// no scripts, styles, forms or frames, links made absolute, https images only.

import DOMPurify from "dompurify";
import { marked } from "marked";
import type { TextFormat } from "./types";

let bases: { link: string | null; image: string | null } = { link: null, image: null };
let hooked = false;

function resolve(url: string, base: string | null): string | null {
  const trimmed = url.trim();
  if (!trimmed || trimmed.startsWith("#")) return null;
  try {
    return new URL(trimmed, base ?? undefined).toString();
  } catch {
    return null;
  }
}

function installHooks() {
  if (hooked) return;
  hooked = true;
  DOMPurify.addHook("afterSanitizeAttributes", (node) => {
    if (node.tagName === "A") {
      const href = resolve(node.getAttribute("href") ?? "", bases.link);
      if (href && /^https?:\/\//i.test(href)) {
        node.setAttribute("href", href);
        node.setAttribute("rel", "noopener noreferrer");
      } else {
        node.removeAttribute("href");
      }
    }
    if (node.tagName === "IMG") {
      const src = resolve(node.getAttribute("src") ?? "", bases.image);
      node.removeAttribute("srcset");
      if (src && /^https:\/\//i.test(src)) {
        node.setAttribute("src", src);
        node.setAttribute("loading", "lazy");
      } else {
        node.removeAttribute("src");
      }
    }
  });
}

function escapeText(text: string): string {
  const escaped = text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  return escaped
    .split(/\n{2,}/)
    .map((p) => `<p>${p.replace(/\n/g, "<br>")}</p>`)
    .join("");
}

export function renderRichText(
  text: string,
  format: TextFormat,
  linkBase: string | null = null,
  imageBase: string | null = null,
): string {
  installHooks();
  const html =
    format === "markdown"
      ? (marked.parse(text, { async: false, gfm: true }) as string)
      : format === "html"
        ? text
        : escapeText(text);
  bases = { link: linkBase, image: imageBase };
  try {
    return DOMPurify.sanitize(html, {
      FORBID_TAGS: ["style", "script", "form", "input", "button", "textarea", "select", "iframe", "object", "embed", "video", "audio"],
      FORBID_ATTR: ["style", "class", "id", "srcset"],
      ALLOW_DATA_ATTR: false,
    });
  } finally {
    bases = { link: null, image: null };
  }
}
