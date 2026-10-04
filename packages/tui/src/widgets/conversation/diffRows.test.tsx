import { expect, test } from "bun:test";
import { testRender } from "@opentui/solid";

import { diffTexts } from "@session/diffs.ts";
import { resolveTheme } from "@theme/palettes.ts";
import { ThemeProvider } from "@theme/providers.tsx";
import { DiffRows } from "@widgets/conversation/diffRows.tsx";

async function fgsOf(path: string, before: string, after: string): Promise<Set<string>> {
  const diff = diffTexts(path, before, after);
  if (!diff) throw new Error("no diff");
  const setup = await testRender(
    () => (
      <ThemeProvider theme={resolveTheme({ name: "mocha" })}>
        <DiffRows diffs={[diff]} />
      </ThemeProvider>
    ),
    { width: 80, height: 12 },
  );
  try {
    for (let i = 0; i < 30; i++) {
      await Bun.sleep(25);
      await setup.renderOnce();
    }
    const fgs = new Set<string>();
    for (const line of setup.captureSpans().lines)
      for (const span of line.spans)
        if (span.text.trim()) fgs.add(span.fg.toString());
    return fgs;
  } finally {
    setup.renderer.destroy();
  }
}

const SAMPLES: Record<string, [string, string]> = {
  "a.ts": ["const a = 1;\n", 'const a = "hello"; // note\n'],
  "a.rs": ["fn main() {}\n", 'fn main() { let s = "hi"; } // note\n'],
  "a.sh": ["echo a\n", 'if [ -f "$x" ]; then echo hi; fi # note\n'],
  "a.json": ['{"a": 1}\n', '{"a": "hello", "b": true}\n'],
  "a.toml": ["a = 1\n", '[table]\na = "hello" # note\n'],
};

for (const [path, [before, after]] of Object.entries(SAMPLES)) {
  test(`a ${path} diff is syntax highlighted`, async () => {
    const highlighted = await fgsOf(path, before, after);
    const plain = await fgsOf("a.txt", before, after);
    expect(highlighted.size).toBeGreaterThan(plain.size);
  });
}
