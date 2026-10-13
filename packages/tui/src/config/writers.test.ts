import { expect, test } from "bun:test";

import type { MitosConfig } from "@config/loaders.ts";
import { serializeConfig } from "@config/writers.ts";
import { KnownHarness } from "@harness/harnesses.ts";
import { KeyAction } from "@input/keybindings.ts";
import { LayoutDirection, PanelKind, TextAlign } from "@layout/configs.ts";

test("serializes a named layout and session defaults", () => {
  const config: MitosConfig = {
    session: { defaultHarness: KnownHarness.Codex },
    handoff: { maxInlineBytes: 65_536 },
    panels: {
      conversation: { kind: PanelKind.Conversation, focusKey: "ctrl+1" },
      stats: { templatePath: "panels/stats.hbs", template: "{{harness}}" },
    },
    layout: {
      panels: {
        conversation: { kind: PanelKind.Conversation, focusKey: "ctrl+1" },
        stats: { templatePath: "panels/stats.hbs", template: "{{harness}}" },
      },
      tree: {
        direction: LayoutDirection.Row,
        children: [{ panel: "conversation" }, { panel: "stats", size: 30 }],
      },
    },
  };
  const toml = serializeConfig(config);
  expect(toml).toContain('[session]\ndefault_harness = "codex"');
  expect(toml).toContain("max_inline_bytes = 65536");
  expect(toml).toContain('[panels.stats]\ntemplate = "panels/stats.hbs"');
  expect(toml).not.toContain('kind = "custom"');
  expect(toml).toContain(
    'children = [{ panel = "conversation" }, { panel = "stats", size = 30 }]',
  );
});

test("serializes theme name, overrides, aligned templates and keybindings", () => {
  const toml = serializeConfig({
    theme: {
      name: "mocha",
      overrides: { accent: "#ffffff", text: undefined },
    },
    panels: {
      stats: {
        templatePath: "stats.hbs",
        template: "x",
        align: TextAlign.Right,
      },
    },
    layout: {
      panels: {},
      tree: {
        direction: LayoutDirection.Column,
        size: "50%",
        children: [{ panel: "stats", size: "20%" }],
      },
    },
    keybindings: { [KeyAction.Quit]: ["ctrl+q", "q"] },
  });
  expect(toml).toContain('[theme]\nname = "mocha"');
  expect(toml).toContain('[theme.overrides]\naccent = "#ffffff"');
  expect(toml).not.toContain("text =");
  expect(toml).toContain('align = "right"');
  expect(toml).toContain('size = "50%"');
  expect(toml).toContain('{ panel = "stats", size = "20%" }');
  expect(toml).toContain('[keybindings]\nquit = ["ctrl+q", "q"]');
});

test("serializes conversation panel focus keys and an empty config", () => {
  expect(
    serializeConfig({
      panels: {
        chat: { kind: PanelKind.Conversation, focusKey: "ctrl+1" },
      },
    }),
  ).toBe('[panels.chat]\nkind = "conversation"\nfocus_key = "ctrl+1"\n');
  expect(serializeConfig({})).toBe("");
});
