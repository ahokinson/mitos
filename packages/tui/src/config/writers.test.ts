import { expect, test } from "bun:test";

import type { MitosConfig } from "@config/loaders.ts";
import { serializeConfig } from "@config/writers.ts";
import { KnownHarness } from "@harness/harnesses.ts";
import { LayoutDirection, WidgetKind } from "@layout/trees.ts";

test("serializes a named layout and session defaults", () => {
  const config: MitosConfig = {
    session: { defaultHarness: KnownHarness.Codex },
    handoff: { maxInlineBytes: 65_536 },
    widgets: {
      conversation: { kind: WidgetKind.Conversation, focusKey: "ctrl+1" },
      stats: { templatePath: "widgets/stats.hbs", template: "{{harness}}" },
    },
    layout: {
      widgets: {
        conversation: { kind: WidgetKind.Conversation, focusKey: "ctrl+1" },
        stats: { templatePath: "widgets/stats.hbs", template: "{{harness}}" },
      },
      tree: {
        direction: LayoutDirection.Row,
        children: [{ widget: "conversation" }, { widget: "stats", size: 30 }],
      },
    },
  };
  const toml = serializeConfig(config);
  expect(toml).toContain('[session]\ndefault_harness = "codex"');
  expect(toml).toContain("max_inline_bytes = 65536");
  expect(toml).toContain('[widgets.stats]\ntemplate = "widgets/stats.hbs"');
  expect(toml).not.toContain('kind = "custom"');
  expect(toml).toContain(
    'children = [{ widget = "conversation" }, { widget = "stats", size = 30 }]',
  );
});
