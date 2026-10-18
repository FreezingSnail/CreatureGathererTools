import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Creature } from "../editorTypes";
import { CREATURE_TYPES, DUEL_TYPES } from "../editorTypes";
import {
  emptyCreature,
  emptyCreatureFormErrors,
  hasFormErrors,
  maxId,
  validateCreatureForm,
} from "../lib/validation";
import { Button } from "./ui/button";
import { Badge } from "./ui/badge";
import { Card, CardContent, CardFooter, CardHeader, CardTitle } from "./ui/card";
import { Input } from "./ui/input";
import { Label } from "./ui/label";
import { cn } from "@/lib/utils";
import { NumberField, SelectField, TextField } from "./fields";

export interface CreatureApi {
  loadCreatures: () => Promise<Creature[]>;
  saveCreatures: (creatures: Creature[]) => Promise<void>;
}

const defaultApi: CreatureApi = {
  loadCreatures: () => invoke("load_creatures"),
  saveCreatures: (creatures) => invoke("save_creatures", { creatures }),
};

interface Draft {
  name: string;
  type1: string;
  type2: string;
  evelevel: string;
  hp: string;
  moves: string[];
}

function toDraft(c: Creature): Draft {
  return {
    name: c.name,
    type1: c.type1,
    type2: c.type2,
    evelevel: String(c.evelevel),
    hp: String(c.hp),
    moves: [...c.moves],
  };
}

export function CreatureEditor({ api = defaultApi }: { api?: CreatureApi }) {
  const [creatures, setCreatures] = useState<Creature[]>([]);
  const [selected, setSelected] = useState(-1);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [errors, setErrors] = useState(emptyCreatureFormErrors());
  const [status, setStatus] = useState<string | null>(null);
  const [statusError, setStatusError] = useState(false);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    let cancelled = false;
    api
      .loadCreatures()
      .then((cs) => {
        if (cancelled) return;
        setCreatures(cs);
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
    if (idx < 0 || idx >= creatures.length) return;
    setSelected(idx);
    setDraft(toDraft(creatures[idx]));
    setErrors(emptyCreatureFormErrors());
    setStatus(null);
    setStatusError(false);
  };

  const setDraftField = <K extends keyof Draft>(key: K, value: Draft[K]) => {
    setDraft((d) => (d ? { ...d, [key]: value } : d));
  };

  const save = async () => {
    if (selected < 0 || !draft) return;
    const nextErrors = validateCreatureForm(draft);
    setErrors(nextErrors);
    if (hasFormErrors(nextErrors)) {
      setStatusError(true);
      setStatus("fix validation errors before saving");
      return;
    }
    const next: Creature = {
      name: draft.name.trim(),
      id: creatures[selected].id,
      type1: draft.type1,
      type2: draft.type2,
      evelevel: Number(draft.evelevel),
      hp: Number(draft.hp),
      moves: draft.moves,
    };
    const updated = creatures.map((c, i) => (i === selected ? next : c));
    setCreatures(updated);
    try {
      await api.saveCreatures(updated);
      setStatusError(false);
      setStatus(`saved ${updated.length} creatures`);
    } catch (e) {
      setStatusError(true);
      setStatus(`save failed: ${String(e)}`);
    }
  };

  const addCreature = async () => {
    const nextId = maxId(creatures) + 1;
    const updated = [...creatures, emptyCreature(nextId)];
    setCreatures(updated);
    try {
      await api.saveCreatures(updated);
      setSelected(updated.length - 1);
      setDraft(toDraft(updated[updated.length - 1]));
      setErrors(emptyCreatureFormErrors());
      setStatusError(false);
      setStatus(`added creature ${nextId}`);
    } catch (e) {
      setStatusError(true);
      setStatus(`save failed: ${String(e)}`);
    }
  };

  const deleteCreature = async () => {
    if (selected < 0) return;
    const updated = creatures.filter((_, i) => i !== selected);
    setCreatures(updated);
    try {
      await api.saveCreatures(updated);
      const nextSel = Math.min(selected, updated.length - 1);
      setSelected(updated.length > 0 ? nextSel : -1);
      setDraft(updated.length > 0 ? toDraft(updated[nextSel]) : null);
      setErrors(emptyCreatureFormErrors());
      setStatusError(false);
      setStatus("deleted creature");
    } catch (e) {
      setStatusError(true);
      setStatus(`save failed: ${String(e)}`);
    }
  };

  const toggleMove = (type: string, on: boolean) => {
    if (!draft) return;
    const set = new Set(draft.moves);
    if (on) set.add(type);
    else set.delete(type);
    setDraftField("moves", DUEL_TYPES.filter((t) => set.has(t)));
  };

  if (!loaded) return <div data-testid="creature-loading">loading creatures…</div>;

  return (
    <div
      className="flex h-full items-start gap-4 overflow-auto p-4"
      data-testid="creature-editor"
    >
      <Card className="w-60 shrink-0">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            Creatures
            <Badge variant="secondary">{creatures.length}</Badge>
          </CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-1">
          {creatures.map((c, i) => (
            <Button
              key={c.id}
              type="button"
              variant={i === selected ? "secondary" : "ghost"}
              className="w-full justify-start"
              onClick={() => populate(i)}
            >
              {c.name}
            </Button>
          ))}
        </CardContent>
        <CardFooter className="gap-2">
          <Button type="button" onClick={addCreature}>
            + Add
          </Button>
          <Button
            type="button"
            variant="destructive"
            onClick={deleteCreature}
            disabled={selected < 0}
          >
            - Delete
          </Button>
        </CardFooter>
      </Card>

      <Card className="flex-1">
        {!draft ? (
          <CardContent>
            <p className="py-4 text-sm text-muted-foreground">Select a creature to edit.</p>
          </CardContent>
        ) : (
          <CardContent className="flex flex-col gap-4">
            <div className="grid gap-1.5">
              <Label>ID</Label>
              <Input type="text" value={String(creatures[selected].id)} disabled readOnly />
            </div>
            <TextField
              label="Name"
              value={draft.name}
              onChange={(v) => setDraftField("name", v)}
              error={errors.name}
            />
            <div className="grid grid-cols-3 gap-2">
              <SelectField
                label="Type 1"
                value={draft.type1}
                options={CREATURE_TYPES}
                onChange={(v) => setDraftField("type1", v)}
              />
              <SelectField
                label="Type 2"
                value={draft.type2}
                options={CREATURE_TYPES}
                onChange={(v) => setDraftField("type2", v)}
              />
              <NumberField
                label="Evo level"
                value={draft.evelevel}
                onChange={(v) => setDraftField("evelevel", v)}
                error={errors.evelevel}
              />
            </div>
            <div className="grid grid-cols-3 gap-2">
              <NumberField
                label="HP"
                value={draft.hp}
                onChange={(v) => setDraftField("hp", v)}
                error={errors.hp}
              />
            </div>

            <div className="border-t pt-3">
              <CardTitle className="mb-3">Attack types</CardTitle>
              <div className="flex flex-wrap gap-4">
                {DUEL_TYPES.map((t) => {
                  const id = `creature-move-${t}`;
                  return (
                    <div key={t} className="flex items-center gap-2">
                      <input
                        id={id}
                        type="checkbox"
                        className="h-4 w-4"
                        checked={draft.moves.includes(t)}
                        onChange={(e) => toggleMove(t, e.target.checked)}
                      />
                      <Label htmlFor={id} className="cursor-pointer capitalize">
                        {t}
                      </Label>
                    </div>
                  );
                })}
              </div>
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