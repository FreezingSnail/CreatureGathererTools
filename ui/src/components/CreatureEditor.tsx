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
    <div className="cgfx-editor" data-testid="creature-editor">
      <div className="cgfx-editor__list">
        <div className="cgfx-editor__list-header">Creatures</div>
        <div className="cgfx-editor__list-items">
          {creatures.map((c, i) => (
            <button
              key={c.id}
              type="button"
              className={`cgfx-editor__list-item${
                i === selected ? " cgfx-editor__list-item--selected" : ""
              }`}
              onClick={() => populate(i)}
            >
              {c.name}
            </button>
          ))}
        </div>
        <div className="cgfx-editor__list-actions">
          <button type="button" onClick={addCreature}>
            + Add
          </button>
          <button type="button" onClick={deleteCreature} disabled={selected < 0}>
            - Delete
          </button>
        </div>
      </div>

      <div className="cgfx-editor__form">
        {!draft ? (
          <div className="cgfx-editor__form-empty">Select a creature to edit.</div>
        ) : (
          <>
            <TextField
              label="Name"
              value={draft.name}
              onChange={(v) => setDraftField("name", v)}
              error={errors.name}
            />
            <div className="cgfx-field">
              <label>ID</label>
              <div className="cgfx-field__control">
                <input type="text" value={String(creatures[selected].id)} disabled readOnly />
              </div>
            </div>
            <div className="cgfx-stat-row">
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
            <div className="cgfx-stat-row">
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
            <div className="cgfx-stat-row">
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

            <div className="cgfx-movegrid">
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

            <div className="cgfx-editor__form">
              <div className="cgfx-editor__list-header">Move list</div>
              <div className="cgfx-stat-row">
                <SelectField
                  label="Add move"
                  value={addMoveName}
                  options={moveNames.length ? moveNames : [""]}
                  onChange={setAddMoveName}
                />
                <button type="button" onClick={addMoveToList} disabled={!addMoveName}>
                  Add to list
                </button>
              </div>
              {draft.moveList.length === 0 ? (
                <div className="cgfx-editor__form-empty">No moves in list.</div>
              ) : (
                <table className="cgfx-movelist-table">
                  <thead>
                    <tr>
                      <th>Name</th>
                      <th>Type</th>
                      <th>Power</th>
                      <th>Phys</th>
                      <th>Acc</th>
                      <th>Effect</th>
                      <th />
                    </tr>
                  </thead>
                  <tbody>
                    {draft.moveList.map((me, i) => (
                      <tr key={`${me.id}-${i}`}>
                        <td>{me.name}</td>
                        <td>{me.type}</td>
                        <td>{me.power}</td>
                        <td>{me.physical ? "Phys" : "Spec"}</td>
                        <td>{me.accuracy}</td>
                        <td>{me.effect}</td>
                        <td>
                          <button type="button" onClick={() => removeMoveFromList(i)}>
                            Remove
                          </button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>

            <div className="cgfx-editor__list-actions">
              <button type="button" onClick={save}>
                Save
              </button>
            </div>
            <div
              className={`cgfx-editor__status${statusError ? " cgfx-editor__status--error" : ""}`}
              role="status"
            >
              {status}
            </div>
          </>
        )}
      </div>
    </div>
  );
}

