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
    <div className="cgfx-editor" data-testid="move-editor">
      <div className="cgfx-editor__list">
        <div className="cgfx-editor__list-header">Moves</div>
        <div className="cgfx-editor__list-items">
          {moves.map((m, i) => (
            <button
              key={m.id}
              type="button"
              className={`cgfx-editor__list-item${
                i === selected ? " cgfx-editor__list-item--selected" : ""
              }`}
              onClick={() => populate(i)}
            >
              {m.name}
            </button>
          ))}
        </div>
        <div className="cgfx-editor__list-actions">
          <button type="button" onClick={addMove}>
            + Add
          </button>
          <button type="button" onClick={deleteMove} disabled={selected < 0}>
            - Delete
          </button>
        </div>
      </div>

      <div className="cgfx-editor__form">
        {!draft ? (
          <div className="cgfx-editor__form-empty">Select a move to edit.</div>
        ) : (
          <>
            <div className="cgfx-field">
              <label>ID</label>
              <div className="cgfx-field__control">
                <input type="text" value={String(moves[selected].id)} disabled readOnly />
              </div>
            </div>
            <TextField
              label="Name"
              value={draft.name}
              onChange={(v) => setDraftField("name", v)}
              error={errors.name}
            />
            <div className="cgfx-stat-row">
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
            <div className="cgfx-stat-row">
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