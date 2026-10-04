import { type StyleDefinitionInput, SyntaxStyle } from "@opentui/core";

import { ColorRole, toColor } from "@theme/colors.ts";
import { registerParsers } from "@theme/parsers.ts";
import type { Theme } from "@theme/themes.ts";

registerParsers();

type Scope = { token: keyof Theme; style?: Omit<StyleDefinitionInput, "fg"> };

const SCOPES: Record<string, Scope> = {
  keyword: { token: "accent" },
  "keyword.return": { token: "accent" },
  "keyword.function": { token: "accent" },
  "keyword.operator": { token: "textMuted" },
  string: { token: "success" },
  "string.escape": { token: "warning" },
  "string.regexp": { token: "warning" },
  comment: { token: "textDim", style: { italic: true } },
  number: { token: "warning" },
  boolean: { token: "warning" },
  constant: { token: "warning" },
  "constant.builtin": { token: "warning" },
  type: { token: "warning" },
  "type.builtin": { token: "warning" },
  function: { token: "text", style: { bold: true } },
  "function.method": { token: "text", style: { bold: true } },
  "function.call": { token: "text" },
  "function.macro": { token: "accent" },
  variable: { token: "text" },
  "variable.builtin": { token: "accent" },
  "variable.parameter": { token: "text" },
  "variable.member": { token: "text" },
  property: { token: "text" },
  attribute: { token: "warning" },
  tag: { token: "err" },
  module: { token: "text" },
  label: { token: "accent" },
  operator: { token: "textMuted" },
  punctuation: { token: "textMuted" },
  "punctuation.bracket": { token: "textMuted" },
  "punctuation.delimiter": { token: "textMuted" },
  "punctuation.special": { token: "accent" },
  "markup.heading": { token: "accent", style: { bold: true } },
  "markup.strong": { token: "text", style: { bold: true } },
  "markup.italic": { token: "text", style: { italic: true } },
  "markup.strikethrough": { token: "textDim" },
  "markup.raw": { token: "warning" },
  "markup.raw.block": { token: "text" },
  "markup.list": { token: "accent" },
  "markup.quote": { token: "textMuted", style: { italic: true } },
  "markup.link": { token: "accent", style: { underline: true } },
  "markup.link.label": { token: "accent", style: { underline: true } },
  "markup.link.url": { token: "textDim" },
  conceal: { token: "textDim" },
  default: { token: "text" },
};

/** The code, diff and markdown highlight styles for a theme, drawn only from
 * its own tokens so a custom theme recolors them too. */
export function syntaxStyleFor(theme: Theme): SyntaxStyle {
  const styles: Record<string, StyleDefinitionInput> = {};
  for (const [name, scope] of Object.entries(SCOPES)) {
    const fg = toColor(
      theme[scope.token] as string | undefined,
      ColorRole.Foreground,
    );
    styles[name] = { ...scope.style, ...(fg === undefined ? {} : { fg }) };
  }
  return SyntaxStyle.fromStyles(styles);
}
