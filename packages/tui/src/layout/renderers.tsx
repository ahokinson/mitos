import type { JSX } from "solid-js"

import { renderTile, type TileNode } from "@layout/tiles.tsx"
import { isTemplateWidget, type LayoutConfig, type LayoutNode } from "@layout/trees.ts"
import { WIDGET_REGISTRY } from "@layout/registries.ts"
import { TemplateWidget } from "@widgets/custom/customWidgets.tsx"

function toTileNode(node: LayoutNode): TileNode<string> {
  return "widget" in node
    ? { leaf: node.widget, size: node.size }
    : { direction: node.direction, size: node.size, children: node.children.map(toTileNode) }
}

export function LayoutRenderer(props: { config: LayoutConfig }): JSX.Element {
  return renderTile(
    toTileNode(props.config.tree),
    (widgetId, style) => {
      const definition = props.config.widgets[widgetId]
      if (!definition) return <box {...style} />
      if (isTemplateWidget(definition))
        return <TemplateWidget paneStyle={style} template={definition.template} align={definition.align} />
      const Widget = WIDGET_REGISTRY[definition.kind]
      return <Widget paneStyle={style} focusId={widgetId} />
    },
    { flexGrow: 1 },
  )
}
