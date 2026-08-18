// Inline script editor for the selected NPC/script object. Parse errors from
// the `parse_script` command are advisory: red line/col/msg display.

import type { ParseResult } from "./types";
import { Textarea } from "@/components/ui/textarea";

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
      className="border-t border-neutral-800"
      style={{
        padding: 8,
        display: "flex",
        flexDirection: "column",
        gap: 6,
      }}
    >
      <label style={{ fontWeight: 600 }}>{title}</label>
      <Textarea
        value={script}
        onChange={(e) => onChange(e.target.value)}
        rows={6}
        spellCheck={false}
        className="h-28 [field-sizing:fixed] resize-y font-mono text-xs"
        data-testid="script-textarea"
      />
      <div
        className="min-h-[1.5rem] text-xs leading-5"
        data-testid="script-status"
      >
        {parseKey && parseResult ? (
          parseResult.ok ? (
            <div className="text-green-400" data-testid="script-ok">
              script parses ok
            </div>
          ) : (
            <ul
              className="text-red-400 max-h-24 overflow-auto"
              style={{ margin: 0, paddingLeft: 18 }}
              data-testid="script-errors"
            >
              {parseResult.errors.map((err, i) => (
                <li key={i} style={{ fontFamily: "monospace", fontSize: 12 }}>
                  line {err.line}, col {err.col}: {err.msg}
                </li>
              ))}
            </ul>
          )
        ) : null}
      </div>
    </div>
  );
}