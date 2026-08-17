// Inline script editor for the selected NPC/script object. Parse errors from
// the `parse_script` command are advisory: red line/col/msg display.

import type { ParseResult } from "./types";

export interface ScriptPaneProps {
  title: string;
  script: string;
  parseKey: string | null;
  parseResult: ParseResult | null;
  onChange: (text: string) => void;
}

export function ScriptPane({
  title,
  script,
  parseKey,
  parseResult,
  onChange,
}: ScriptPaneProps) {
  return (
    <div
      style={{
        borderTop: "1px solid #333",
        padding: 8,
        display: "flex",
        flexDirection: "column",
        gap: 6,
      }}
    >
      <label style={{ fontWeight: 600 }}>{title}</label>
      <textarea
        value={script}
        onChange={(e) => onChange(e.target.value)}
        rows={6}
        spellCheck={false}
        style={{
          fontFamily: "monospace",
          fontSize: 12,
          background: "#111",
          color: "#ddd",
          border: "1px solid #444",
          resize: "vertical",
        }}
        data-testid="script-textarea"
      />
      {parseKey && parseResult ? (
        parseResult.ok ? (
          <div style={{ color: "#6f6" }} data-testid="script-ok">
            script parses ok
          </div>
        ) : (
          <ul style={{ margin: 0, paddingLeft: 18, color: "#f66" }} data-testid="script-errors">
            {parseResult.errors.map((err, i) => (
              <li key={i} style={{ fontFamily: "monospace", fontSize: 12 }}>
                line {err.line}, col {err.col}: {err.msg}
              </li>
            ))}
          </ul>
        )
      ) : null}
    </div>
  );
}