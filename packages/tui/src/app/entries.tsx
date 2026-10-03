import { createCliRenderer } from "@opentui/core";
import { render } from "@opentui/solid";
import { createMemo } from "solid-js";

import { App } from "@app/shells.tsx";
import { loadConfig } from "@config/loaders.ts";
import { ThemeProvider } from "@theme/providers.tsx";
import { resolveTheme } from "@theme/palettes.ts";

const renderer = await createCliRenderer({ consoleMode: "disabled", exitOnCtrlC: false });
renderer.setTerminalTitle("Mitos");
const configHandle = await loadConfig(process.env.MITOS_STATE_DIR);

await render(() => {
  const theme = createMemo(() => resolveTheme(configHandle.config().theme, configHandle.customThemes()));
  return (
    <ThemeProvider theme={theme()}>
      <App configHandle={configHandle} />
    </ThemeProvider>
  );
}, renderer);
