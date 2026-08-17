import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Creature, Move, MoveEntry } from "../editorTypes";
import { CREATURE_TYPES } from "../editorTypes";
import {
  emptyCreature,
  emptyCreatureFormErrors,
  hasFormErrors,
  maxId,
  moveEntryFromMove,
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
  loadMoves: () => Promise<Move[]>;
}

const defaultApi: CreatureApi = {
  loadCreatures: () => invoke("load_creatures"),
  saveCreatures: (creatures) => invoke("save_creatures", { creatures }),
  loadMoves: () => invoke("load_moves"),
};

interface Draft {
  name: string;
  type1: string;
  type2: string;
  evelevel: string;
  atk: string;
  deff: string;
  spcatk: string;
  spcdef: string;
  hp: string;
  spd: string;
  move1: string;
  move2: string;
  move3: string;
  move4: string;
  moveList: MoveEntry[];
}

function toDraft(c: Creature, moveNameById: Map<number, string>): Draft {
  return {
    name: c.name,
    type1: c.type1,
    type2: c.type2,
    evelevel: String(c.evelevel),
    atk: String(c.atk),
    deff: String(c.deff),
    spcatk: String(c.spcatk),
    spcdef: String(c.spcdef),
    hp: String(c.hp),
    spd: String(c.spd),
    move1: moveNameById.get(c.move1) ?? "",
    move2: moveNameById.get(c.move2) ?? "",
    move3: moveNameById.get(c.move3) ?? "",
    move4: moveNameById.get(c.move4) ?? "",
    moveList: c.moveList,
  };
}

export function CreatureEditor({ api = defaultApi }: { api?: CreatureApi }) {
  const [creatures, setCreatures] = useState<Creature[]>([]);
  const [moves, setMoves] = useState<Move[]>([]);
  const [selected, setSelected] = useState(-1);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [errors, setErrors] = useState(emptyCreatureFormErrors());
  const [addMoveName, setAddMoveName] = useState("");
  const [status, setStatus] = useState<string | null>(null);
  const [statusError, setStatusError] = useState(false);
  const [loaded, setLoaded] = useState(false);

  const moveNameById = useMemo(
    () => new Map(moves.map((m) => [m.id, m.name])),
    [moves]
  );
  const moveIdByName = useMemo(
    () => new Map(moves.map((m) => [m.name, m.id])),
    [moves]
  );
  const moveNames = useMemo(() => moves.map((m) => m.name), [moves]);

  useEffect(() => {
    let cancelled = false;
    Promise.all([api.loadCreatures(), api.loadMoves()])
      .then(([cs, ms]) => {
        if (cancelled) return;
        setCreatures(cs);
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
    if (idx < 0 || idx >= creatures.length) return;
    setSelected(idx);
    setDraft(toDraft(creatures[idx], moveNameById));
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
      atk: Number(draft.atk),
      deff: Number(draft.deff),
      spcatk: Number(draft.spcatk),
      spcdef: Number(draft.spcdef),
      hp: Number(draft.hp),
      spd: Number(draft.spd),
      move1: moveIdByName.get(draft.move1) ?? 0,
      move2: moveIdByName.get(draft.move2) ?? 0,
      move3: moveIdByName.get(draft.move3) ?? 0,
      move4: moveIdByName.get(draft.move4) ?? 0,
      moveList: draft.moveList,
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
      setDraft(toDraft(updated[updated.length - 1], moveNameById));
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
      setDraft(updated.length > 0 ? toDraft(updated[nextSel], moveNameById) : null);
      setErrors(emptyCreatureFormErrors());
      setStatusError(false);
      setStatus("deleted creature");
    } catch (e) {
      setStatusError(true);
      setStatus(`save failed: ${String(e)}`);
    }
  };

  const addMoveToList = () => {
    if (!draft) return;
    const move = moves.find((m) => m.name === addMoveName);
    if (!move) return;
    setDraftField("moveList", [...draft.moveList, moveEntryFromMove(move)]);
  };

  const removeMoveFromList = (index: number) => {
    if (!draft) return;
    setDraftField(
      "moveList",
      draft.moveList.filter((_, i) => i !== index)
    );
  };

  if (!loaded) return <div data-testid="creature-loading">loading creatures…</div>;

  return (
    <div className="flex items-start gap-4 p-4" data-testid="creature-editor">
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
                label="ATK"
                value={draft.atk}
                onChange={(v) => setDraftField("atk", v)}
                error={errors.atk}
              />
              <NumberField
                label="DEF"
                value={draft.deff}
                onChange={(v) => setDraftField("deff", v)}
                error={errors.deff}
              />
              <NumberField
                label="SPC ATK"
                value={draft.spcatk}
                onChange={(v) => setDraftField("spcatk", v)}
                error={errors.spcatk}
              />
            </div>
            <div className="grid grid-cols-3 gap-2">
              <NumberField
                label="SPC DEF"
                value={draft.spcdef}
                onChange={(v) => setDraftField("spcdef", v)}
                error={errors.spcdef}
              />
              <NumberField
                label="HP"
                value={draft.hp}
                onChange={(v) => setDraftField("hp", v)}
                error={errors.hp}
              />
              <NumberField
                label="SPD"
                value={draft.spd}
                onChange={(v) => setDraftField("spd", v)}
                error={errors.spd}
              />
            </div>

            <div className="grid grid-cols-4 gap-2 border-t pt-3">
              <SelectField
                label="Move 1"
                value={draft.move1}
                options={moveNames.length ? moveNames : [""]}
                onChange={(v) => setDraftField("move1", v)}
              />
              <SelectField
                label="Move 2"
                value={draft.move2}
                options={moveNames.length ? moveNames : [""]}
                onChange={(v) => setDraftField("move2", v)}
              />
              <SelectField
                label="Move 3"
                value={draft.move3}
                options={moveNames.length ? moveNames : [""]}
                onChange={(v) => setDraftField("move3", v)}
              />
              <SelectField
                label="Move 4"
                value={draft.move4}
                options={moveNames.length ? moveNames : [""]}
                onChange={(v) => setDraftField("move4", v)}
              />
            </div>

            <div className="border-t pt-3">
              <CardTitle className="mb-3">Move list</CardTitle>
              <div className="mb-3 grid grid-cols-3 items-end gap-2">
                <SelectField
                  label="Add move"
                  value={addMoveName}
                  options={moveNames.length ? moveNames : [""]}
                  onChange={setAddMoveName}
                />
                <Button type="button" onClick={addMoveToList} disabled={!addMoveName}>
                  Add to list
                </Button>
              </div>
              {draft.moveList.length === 0 ? (
                <p className="text-sm text-muted-foreground">No moves in list.</p>
              ) : (
                <table className="w-full border-collapse text-sm">
                  <thead>
                    <tr>
                      <th className="border px-2 py-1 text-left">Name</th>
                      <th className="border px-2 py-1 text-left">Type</th>
                      <th className="border px-2 py-1 text-left">Power</th>
                      <th className="border px-2 py-1 text-left">Phys</th>
                      <th className="border px-2 py-1 text-left">Acc</th>
                      <th className="border px-2 py-1 text-left">Effect</th>
                      <th className="border px-2 py-1 text-left" />
                    </tr>
                  </thead>
                  <tbody>
                    {draft.moveList.map((me, i) => (
                      <tr key={`${me.id}-${i}`}>
                        <td className="border px-2 py-1">{me.name}</td>
                        <td className="border px-2 py-1">{me.type}</td>
                        <td className="border px-2 py-1">{me.power}</td>
                        <td className="border px-2 py-1">{me.physical ? "Phys" : "Spec"}</td>
                        <td className="border px-2 py-1">{me.accuracy}</td>
                        <td className="border px-2 py-1">{me.effect}</td>
                        <td className="border px-2 py-1">
                          <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={() => removeMoveFromList(i)}
                          >
                            Remove
                          </Button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
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