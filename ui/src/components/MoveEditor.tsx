import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Move } from "../editorTypes";
import { MOVE_EFFECTS, MOVE_TYPES } from "../editorTypes";
import {
  emptyMove,
  emptyMoveFormErrors,
  hasFormErrors,
  maxId,
  validateMoveForm,
} from "../lib/validation";
import { Button } from "./ui/button";
import { Badge } from "./ui/badge";
import { Card, CardContent, CardFooter, CardHeader, CardTitle } from "./ui/card";
import { Input } from "./ui/input";
import { Label } from "./ui/label";
import { cn } from "@/lib/utils";
import { NumberField, SelectField, TextField } from "./fields";

export interface MoveApi {
  loadMoves: () => Promise<Move[]>;
  saveMoves: (moves: Move[]) => Promise<void>;
}

const defaultApi: MoveApi = {
  loadMoves: () => invoke("load_moves"),
  saveMoves: (moves) => invoke("save_moves", { moves }),
};

interface Draft {
  name: string;
  type: string;
  power: string;
  physical: string; // "Physical" | "Special"
  accuracy: string;
  effect: string;
}

function toDraft(m: Move): Draft {
  return {
    name: m.name,
    type: m.type,
    power: String(m.power),
    physical: m.physical === 1 ? "Physical" : "Special",
    accuracy: String(m.accuracy),
    effect: m.effect,
  };
}

function fromDraft(d: Draft): Move {
  return {
    id: 0, // filled by caller from the row being edited
    name: d.name.trim(),
    type: d.type,
    power: Number(d.power),
    physical: d.physical === "Physical" ? 1 : 0,
    accuracy: Number(d.accuracy),
    effect: d.effect,
  };
}

export function MoveEditor({ api = defaultApi }: { api?: MoveApi }) {
  const [moves, setMoves] = useState<Move[]>([]);
  const [selected, setSelected] = useState(-1);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [errors, setErrors] = useState(emptyMoveFormErrors());
  const [status, setStatus] = useState<string | null>(null);
  const [statusError, setStatusError] = useState(false);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    let cancelled = false;
    api
      .loadMoves()
      .then((ms) => {
        if (cancelled) return;
        setMoves(ms);
        setLoaded(true);
      })
      .catch((e) => {
        if (cancelled) return;
        setStatusError(true);
        setStatus(`load failed: ${String(e)}`);
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const populate = (idx: number) => {
    if (idx < 0 || idx >= moves.length) return;
    setSelected(idx);
    setDraft(toDraft(moves[idx]));
    setErrors(emptyMoveFormErrors());
    setStatus(null);
    setStatusError(false);
  };

  const setDraftField = <K extends keyof Draft>(key: K, value: Draft[K]) => {
    setDraft((d) => (d ? { ...d, [key]: value } : d));
  };

  const save = async () => {
    if (selected < 0 || !draft) return;
    const nextErrors = validateMoveForm(draft);
    setErrors(nextErrors);
    if (hasFormErrors(nextErrors)) {
      setStatusError(true);
      setStatus("fix validation errors before saving");
      return;
    }
    const patch = fromDraft(draft);
    const updated = moves.map((m, i) => (i === selected ? { ...patch, id: m.id } : m));
    setMoves(updated);
    try {
      await api.saveMoves(updated);
      setStatusError(false);
      setStatus(`saved ${updated.length} moves`);
    } catch (e) {
      setStatusError(true);
      setStatus(`save failed: ${String(e)}`);
    }
  };

  const addMove = async () => {
    const nextId = maxId(moves) + 1;
    const updated = [...moves, emptyMove(nextId)];
    setMoves(updated);
    try {
      await api.saveMoves(updated);
      setSelected(updated.length - 1);
      setDraft(toDraft(updated[updated.length - 1]));
      setErrors(emptyMoveFormErrors());
      setStatusError(false);
      setStatus(`added move ${nextId}`);
    } catch (e) {
      setStatusError(true);
      setStatus(`save failed: ${String(e)}`);
    }
  };

  const deleteMove = async () => {
    if (selected < 0) return;
    const updated = moves.filter((_, i) => i !== selected);
    setMoves(updated);
    try {
      await api.saveMoves(updated);
      const nextSel = Math.min(selected, updated.length - 1);
      setSelected(updated.length > 0 ? nextSel : -1);
      setDraft(updated.length > 0 ? toDraft(updated[nextSel]) : null);
      setErrors(emptyMoveFormErrors());
      setStatusError(false);
      setStatus("deleted move");
    } catch (e) {
      setStatusError(true);
      setStatus(`save failed: ${String(e)}`);
    }
  };

  if (!loaded) return <div data-testid="move-loading">loading moves…</div>;

  return (
    <div
      className="flex h-full items-start gap-4 overflow-auto p-4"
      data-testid="move-editor"
    >
      <Card className="w-60 shrink-0">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            Moves
            <Badge variant="secondary">{moves.length}</Badge>
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-1">
          {moves.map((m, i) => (
            <Button
              key={m.id}
              type="button"
              variant={i === selected ? "secondary" : "ghost"}
              className="w-full justify-start"
              onClick={() => populate(i)}
            >
              {m.name}
            </Button>
          ))}
        </CardContent>
        <CardFooter className="gap-2">
          <Button type="button" onClick={addMove}>
            + Add
          </Button>
          <Button
            type="button"
            variant="destructive"
            onClick={deleteMove}
            disabled={selected < 0}
          >
            - Delete
          </Button>
        </CardFooter>
      </Card>

      <Card className="flex-1">
        {!draft ? (
          <CardContent>
            <p className="py-4 text-sm text-muted-foreground">Select a move to edit.</p>
          </CardContent>
        ) : (
          <CardContent className="flex flex-col gap-4">
            <div className="grid gap-1.5">
              <Label>ID</Label>
              <Input type="text" value={String(moves[selected].id)} disabled readOnly />
            </div>
            <TextField
              label="Name"
              value={draft.name}
              onChange={(v) => setDraftField("name", v)}
              error={errors.name}
            />
            <div className="grid grid-cols-3 gap-2">
              <SelectField
                label="Type"
                value={draft.type}
                options={MOVE_TYPES}
                onChange={(v) => setDraftField("type", v)}
              />
              <SelectField
                label="Physical"
                value={draft.physical}
                options={["Physical", "Special"]}
                onChange={(v) => setDraftField("physical", v)}
              />
            </div>
            <div className="grid grid-cols-3 gap-2">
              <NumberField
                label="Power"
                value={draft.power}
                onChange={(v) => setDraftField("power", v)}
                error={errors.power}
              />
              <NumberField
                label="Accuracy"
                value={draft.accuracy}
                onChange={(v) => setDraftField("accuracy", v)}
                error={errors.accuracy}
              />
              <SelectField
                label="Effect"
                value={draft.effect}
                options={MOVE_EFFECTS}
                onChange={(v) => setDraftField("effect", v)}
              />
            </div>
            <div className="flex gap-2">
              <Button type="button" onClick={save}>
                Save
              </Button>
            </div>
            <div
              className={cn(
                "min-h-4 text-sm",
                statusError ? "text-destructive" : "text-emerald-600 dark:text-emerald-400"
              )}
              role="status"
            >
              {status}
            </div>
          </CardContent>
        )}
      </Card>
    </div>
  );
}