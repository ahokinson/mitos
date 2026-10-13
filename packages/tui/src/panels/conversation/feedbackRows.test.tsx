import { expect, test } from "bun:test";
import { FeedbackTone } from "@commands/feedbackLines.ts";
import { testRender } from "@opentui/solid";
import { FeedbackLineRow } from "@panels/conversation/feedbackRows.tsx";
import { resolveTheme } from "@theme/palettes.ts";
import { ThemeProvider } from "@theme/providers.tsx";

test("each feedback line gets its own row and keeps its indent", async () => {
  const setup = await testRender(
    () => (
      <ThemeProvider theme={resolveTheme({ name: "mocha" })}>
        <FeedbackLineRow
          feedback={
            {
              threadId: null,
              tone: FeedbackTone.Info,
              lines: [
                "claude    installed",
                "          /home/me/.claude/settings.json",
                "          no data received yet",
              ],
            } as never
          }
        />
      </ThemeProvider>
    ),
    { width: 50, height: 8 },
  );

  try {
    await setup.renderOnce();
    const lines = setup
      .captureCharFrame()
      .split("\n")
      .filter((line) => line.trim().length > 0);
    expect(lines).toHaveLength(3);
    expect(lines[0]).toContain("claude    installed");
    expect(lines[1]).toContain("          /home/me/.claude/settings.json");
    expect(lines[2]).toContain("          no data received yet");
  } finally {
    setup.renderer.destroy();
  }
});
