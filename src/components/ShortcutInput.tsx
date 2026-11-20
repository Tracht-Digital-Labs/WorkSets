import { Input, type InputProps } from "@fluentui/react-components";

const modifierKeys = new Set(["Control", "Shift", "Alt", "Meta"]);

const modifierOrder = ["CTRL", "ALT", "SHIFT", "WIN"];

export function normalizeShortcut(shortcut: string) {
  const tokens = shortcut
    .split("+")
    .map((token) => token.trim())
    .filter(Boolean)
    .map((token) => token.replace(/^Key(?=[A-Z])|^Digit(?=\d)/i, ""))
    .map((token) => {
      const upper = token.toUpperCase();
      if (upper === "CONTROL") return "CTRL";
      if (upper === "META" || upper === "SUPER" || upper === "CMD") return "WIN";
      return upper;
    });
  const modifiers = modifierOrder.filter((modifier) => tokens.includes(modifier));
  const key = tokens.find((token) => !modifierOrder.includes(token));
  return [...modifiers, ...(key ? [key] : [])].join("+");
}

function keyLabel(key: string) {
  if (key === " ") return "Space";
  if (key === "Escape") return "Esc";
  if (key.length === 1) return key.toUpperCase();
  return key;
}

export function shortcutFromEvent(event: React.KeyboardEvent<HTMLInputElement>) {
  if (modifierKeys.has(event.key)) return undefined;
  if (event.key === "Backspace" || event.key === "Delete") return "";
  const keys: string[] = [];
  if (event.ctrlKey) keys.push("Ctrl");
  if (event.altKey) keys.push("Alt");
  if (event.shiftKey) keys.push("Shift");
  if (event.metaKey) keys.push("Win");
  keys.push(keyLabel(event.key));
  return keys.join("+");
}

interface ShortcutInputProps extends Omit<InputProps, "value" | "onChange"> {
  value: string;
  onChange: (shortcut: string) => void;
}

export function ShortcutInput({ value, onChange, ...props }: ShortcutInputProps) {
  return (
    <Input
      {...props}
      value={value}
      readOnly
      onKeyDown={(event) => {
        const shortcut = shortcutFromEvent(event);
        if (shortcut === undefined) return;
        event.preventDefault();
        onChange(shortcut);
      }}
    />
  );
}
