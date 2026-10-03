import { isObject, parsedRecord } from "@json/records.ts";

const TITLE_LIMIT = 160;

export function textValue(value: unknown): string | null {
  if (typeof value === "string") {
    const parsed = parsedRecord(value);
    return parsed ? textValue(parsed) : value.trim() || null;
  }
  if (Array.isArray(value)) {
    const text = value.map(textValue).filter(Boolean).join("\n");
    return text || null;
  }
  if (!isObject(value)) return null;
  return (
    textValue(
      value.text ?? value.content ?? value.output_text ?? value.input_text,
    ) ?? textValue(value.parts)
  );
}

export function clip(text: string): string {
  const flat = text.replace(/\s+/g, " ").trim();
  return flat.length > TITLE_LIMIT
    ? `${flat.slice(0, TITLE_LIMIT - 1)}…`
    : flat;
}
