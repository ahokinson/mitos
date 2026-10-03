import { expect, test } from "bun:test";
import { Chrome, type Theme } from "@theme/themes.ts";
import { parseStyledLine } from "@widgets/custom/templateColors.ts";
import Handlebars from "handlebars";

const theme: Theme = {
  chrome: Chrome.Framed,
  text: "#cdd6f4",
  textDim: "#6c7086",
  accent: "#89b4fa",
  success: "#a6e3a1",
  warning: "#f9e2af",
  err: "#f38ba8",
};

function renderLine(template: string, context: Record<string, unknown> = {}) {
  const compiled = Handlebars.compile(template);
  return parseStyledLine(compiled({ ...context, theme }));
}

test("a line with no color/gradient markup is a single unstyled run", () => {
  expect(parseStyledLine("plain text")).toEqual([{ text: "plain text" }]);
});

test("an empty line has no runs at all", () => {
  expect(parseStyledLine("")).toEqual([]);
});

test("{{#color}} wraps its content in the resolved theme hex", () => {
  expect(renderLine('{{#color "accent"}}hello{{/color}}')).toEqual([
    { text: "hello", fg: "#89b4fa" },
  ]);
});

test("{{#color}} accepts a literal hex in place of a theme token", () => {
  expect(renderLine('{{#color "#ff00ff"}}x{{/color}}')).toEqual([
    { text: "x", fg: "#ff00ff" },
  ]);
});

test("an unknown theme token falls back to the theme's text color", () => {
  expect(renderLine('{{#color "bogus"}}x{{/color}}')).toEqual([
    { text: "x", fg: "#cdd6f4" },
  ]);
});

test("plain text around a styled block keeps its own run, unstyled", () => {
  const runs = renderLine('before {{#color "accent"}}mid{{/color}} after');
  expect(runs).toEqual([
    { text: "before " },
    { text: "mid", fg: "#89b4fa" },
    { text: " after" },
  ]);
});

test("metrics interpolate normally inside a color block", () => {
  const runs = renderLine('{{#color "accent"}}{{name}}{{/color}}', {
    name: "codex",
  });
  expect(runs).toEqual([{ text: "codex", fg: "#89b4fa" }]);
});

test("{{#gradient}} interpolates per character between its stops, exact at both ends", () => {
  const runs = renderLine('{{#gradient "accent" "err"}}world{{/gradient}}');
  expect(runs).toHaveLength(5);
  expect(runs[0]).toEqual({ text: "w", fg: "#89b4fa" });
  expect(runs[4]).toEqual({ text: "d", fg: "#f38ba8" });
  expect(runs.map((r) => r.text).join("")).toBe("world");
});

test("{{#gradient}} with three stops passes through the middle one at the midpoint", () => {
  const runs = renderLine(
    '{{#gradient "accent" "#ffffff" "err"}}aaaaa{{/gradient}}',
  );
  expect(runs).toHaveLength(5);
  expect(runs[0]?.fg).toBe("#89b4fa");
  expect(runs[2]?.fg).toBe("#ffffff");
  expect(runs[4]?.fg).toBe("#f38ba8");
});

test("{{#gradient}} with one stop behaves like a solid color", () => {
  expect(renderLine('{{#gradient "accent"}}hi{{/gradient}}')).toEqual([
    { text: "hi", fg: "#89b4fa" },
  ]);
});

test("nesting is flattened: the outer style wins over any inner markup", () => {
  const runs = renderLine(
    '{{#color "accent"}}a{{#color "err"}}b{{/color}}c{{/color}}',
  );
  expect(runs).toEqual([{ text: "abc", fg: "#89b4fa" }]);
});

test("{{bar}} splits into a filled run and a dim empty run, sized to width", () => {
  const runs = renderLine("{{bar usage 10}}", { usage: "50%" });
  expect(runs).toEqual([
    { text: "█████", fg: "#f9e2af" },
    { text: "░░░░░", fg: "#6c7086" },
  ]);
});

test("{{bar}} colors by severity: under 50 success, under 80 warning, else err", () => {
  expect(renderLine("{{bar usage 10}}", { usage: "10%" })[0]?.fg).toBe(
    "#a6e3a1",
  );
  expect(renderLine("{{bar usage 10}}", { usage: "60%" })[0]?.fg).toBe(
    "#f9e2af",
  );
  expect(renderLine("{{bar usage 10}}", { usage: "95%" })[0]?.fg).toBe(
    "#f38ba8",
  );
});

test("{{bar}} at 0% is fully empty, at 100% is fully filled", () => {
  expect(renderLine("{{bar usage 10}}", { usage: "0%" })).toEqual([
    { text: "░░░░░░░░░░", fg: "#6c7086" },
  ]);
  expect(renderLine("{{bar usage 10}}", { usage: "100%" })).toEqual([
    { text: "██████████", fg: "#f38ba8" },
  ]);
});

test("{{bar}} with no parseable number renders fully empty, dim, no crash", () => {
  expect(renderLine("{{bar usage 10}}", { usage: "" })).toEqual([
    { text: "░░░░░░░░░░", fg: "#6c7086" },
  ]);
});

test("{{bar}} accepts a bare number with no unit", () => {
  expect(renderLine("{{bar usage 10}}", { usage: "30" })[0]).toEqual({
    text: "███",
    fg: "#a6e3a1",
  });
});

const terminalTheme: Theme = {
  chrome: Chrome.None,
  textDim: "ansi:bright-black",
  accent: "ansi:blue",
  success: "ansi:green",
  warning: "ansi:yellow",
  err: "ansi:red",
};

function renderTerminal(
  template: string,
  context: Record<string, unknown> = {},
) {
  return parseStyledLine(
    Handlebars.compile(template)({ ...context, theme: terminalTheme }),
  );
}

function intentOf(fg: unknown): { intent: string; slot: number } {
  const { intent, slot } = fg as { intent: string; slot: number };
  return { intent, slot };
}

test("{{#color}} on a palette theme keeps the terminal's own palette color", () => {
  const [run] = renderTerminal('{{#color "accent"}}hi{{/color}}');
  expect(run?.text).toBe("hi");
  expect(intentOf(run?.fg)).toEqual({ intent: "indexed", slot: 4 });
});

test("{{#color}} accepts a literal ansi token and the literal default", () => {
  const [ansi] = renderLine('{{#color "ansi:5"}}x{{/color}}');
  expect(intentOf(ansi?.fg)).toEqual({ intent: "indexed", slot: 5 });
  const [fallback] = renderLine('{{#color "default"}}x{{/color}}');
  expect(intentOf(fallback?.fg).intent).toBe("default");
});

test("a palette theme's tone resolves through the theme, never to a fixed color", () => {
  const [run] = renderTerminal('{{#color "err"}}bad{{/color}}');
  expect(intentOf(run?.fg)).toEqual({ intent: "indexed", slot: 1 });
});

test("an unset token on a theme with no text color falls back to the terminal default", () => {
  const bare: Theme = { chrome: Chrome.None };
  const [run] = parseStyledLine(
    Handlebars.compile('{{#color "accent"}}x{{/color}}')({ theme: bare }),
  );
  expect(intentOf(run?.fg).intent).toBe("default");
});

test("{{bar}} on a palette theme paints severity and the empty track in palette colors", () => {
  const runs = renderTerminal("{{bar usage 10}}", { usage: "90%" });
  expect(runs.map((run) => run.text)).toEqual(["█████████", "░"]);
  expect(runs.map((run) => intentOf(run.fg))).toEqual([
    { intent: "indexed", slot: 1 },
    { intent: "indexed", slot: 8 },
  ]);
});

test("{{bar}} on a bare theme is entirely the terminal's default color", () => {
  const bare: Theme = { chrome: Chrome.None };
  const runs = parseStyledLine(
    Handlebars.compile("{{bar usage 10}}")({ usage: "50%", theme: bare }),
  );
  expect(runs.every((run) => intentOf(run.fg).intent === "default")).toBe(true);
});

test("{{#gradient}} between palette colors interpolates real channels", () => {
  const runs = renderTerminal('{{#gradient "success" "err"}}abc{{/gradient}}');
  expect(runs).toHaveLength(3);
  for (const run of runs) expect(String(run.fg)).toMatch(/^#[0-9a-f]{6}$/);
  expect(runs[0]?.fg).not.toBe(runs[2]?.fg);
});

test("{{#gradient}} with one palette stop stays a palette color", () => {
  const [run] = renderTerminal('{{#gradient "accent"}}hi{{/gradient}}');
  expect(intentOf(run?.fg)).toEqual({ intent: "indexed", slot: 4 });
});
