import { fileURLToPath } from "node:url";

// ---------------------------------------------------------------------------
// Helper to find image file paths inside free-form text that users may paste
// or drag-drop into the terminal.  Returns the cleaned-up text (with the image
// references removed) *and* the list of absolute or relative paths that were
// found.
// ---------------------------------------------------------------------------

const IMAGE_EXT_REGEX =
  "(?:png|jpe?g|gif|bmp|webp|svg)"; // deliberately kept simple

// Pattern helpers – compiled lazily so the whole file can be tree-shaken if
// unused by a particular build target.
let MARKDOWN_LINK_RE: RegExp;
let QUOTED_PATH_RE: RegExp;
let BARE_PATH_RE: RegExp;

function compileRegexes() {
  if (MARKDOWN_LINK_RE) {
    return;
  }

  MARKDOWN_LINK_RE = /!\[[^\]]*?\]\(([^)]+)\)/g; // capture path inside ()
  QUOTED_PATH_RE = new RegExp(
    `[\'\"]([^\'\"]+?\.${IMAGE_EXT_REGEX})[\'\"]`,
    "gi",
  );
  // eslint-disable-next-line no-useless-escape
  BARE_PATH_RE = new RegExp(
    `\\b(?:\\.[\\/\\\\]|[\\/\\\\]|[A-Za-z]:[\\/\\\\])?[\\w-]+(?:[\\/\\\\][\\w-]+)*\\.${IMAGE_EXT_REGEX}\\b`,
    "gi",
  );
}

export interface ExtractResult {
  paths: Array<string>;
  text: string;
}

export function extractImagePaths(input: string): ExtractResult {
  compileRegexes();

  const paths: Array<string> = [];

  let text = input;

  const replace = (
    re: RegExp,
    mapper: (match: string, path: string) => string,
  ) => {
    text = text.replace(re, mapper);
  };

  // 1) Markdown ![alt](path)
  replace(MARKDOWN_LINK_RE, (_m, p1: string) => {
    paths.push(p1.startsWith("file://") ? fileURLToPath(p1) : p1);
    return "";
  });

  // 2) Quoted
  replace(QUOTED_PATH_RE, (_m, p1: string) => {
    paths.push(p1.startsWith("file://") ? fileURLToPath(p1) : p1);
    return "";
  });

  // 3) Bare
  replace(BARE_PATH_RE, (match: string) => {
    paths.push(match.startsWith("file://") ? fileURLToPath(match) : match);
    return "";
  });

  // Remove any leftover leading slash that was immediately followed by the
  // matched path (e.g. "/Users/foo.png → '/ '" after replacement). We only
  // strip it when it's followed by whitespace or end-of-string so normal
  // typing like "/help" is untouched.
  text = text.replace(/(^|\s)\/(?=\s|$)/g, "$1");

  return { paths, text };
}
