// First-run setup dialog: collects config file paths and creates
// creature-tools.json via save_config (Go RunWithSetup parity).

import { useState, type FormEvent } from "react";
import type { Config } from "./types";

export interface SetupFields {
  creatures_json: string;
  moves_json: string;
  map_json: string;
  map_tileset_png: string;
  npc_tileset_png: string;
  output_dir: string;
}

const FIELDS: { key: keyof SetupFields; label: string }[] = [
  { key: "creatures_json", label: "Creatures JSON" },
  { key: "moves_json", label: "Moves JSON" },
  { key: "map_json", label: "Map JSON" },
  { key: "map_tileset_png", label: "Map Tileset PNG" },
  { key: "npc_tileset_png", label: "NPC Tileset PNG" },
  { key: "output_dir", label: "Output Dir" },
];

const emptyValues = (): SetupFields => ({
  creatures_json: "",
  moves_json: "",
  map_json: "",
  map_tileset_png: "",
  npc_tileset_png: "",
  output_dir: "",
});

interface SetupDialogProps {
  /** Prior config fields to prefill (e.g. after a failed reload). */
  initial?: Config;
  error?: string;
  onSubmit: (config: Config) => void;
}

export function SetupDialog({ initial, error, onSubmit }: SetupDialogProps) {
  const [values, setValues] = useState<SetupFields>(() => {
    if (!initial) return emptyValues();
    return {
      creatures_json: initial.creatures_json,
      moves_json: initial.moves_json,
      map_json: initial.map_json,
      map_tileset_png: initial.map_tileset_png,
      npc_tileset_png: initial.npc_tileset_png,
      output_dir: initial.output_dir,
    };
  });

  const setField = (key: keyof SetupFields, value: string) =>
    setValues((v) => ({ ...v, [key]: value }));

  const submit = (e: FormEvent) => {
    e.preventDefault();
    onSubmit({
      // tileset_png stays untouched: legacy sheet, not part of the setup
      // form (Go setup never writes it either).
      tileset_png: initial?.tileset_png ?? "",
      creatures_json: values.creatures_json,
      moves_json: values.moves_json,
      map_json: values.map_json,
      map_tileset_png: values.map_tileset_png,
      npc_tileset_png: values.npc_tileset_png,
      output_dir: values.output_dir,
    });
  };

  return (
    <main className="cgfx-setup" data-testid="setup-dialog">
      <form className="cgfx-setup__form" onSubmit={submit}>
        <h1 className="cgfx-setup__title">CreatureGathererFX — Setup</h1>
        <p className="cgfx-setup__hint">Create creature-tools.json config file</p>
        {FIELDS.map((f) => (
          <div className="cgfx-field" key={f.key}>
            <label htmlFor={`setup-${f.key}`}>{f.label}</label>
            <input
              id={`setup-${f.key}`}
              type="text"
              value={values[f.key]}
              onChange={(e) => setField(f.key, e.target.value)}
            />
          </div>
        ))}
        <button type="submit" className="cgfx-setup__submit">
          Create Config
        </button>
        {error && (
          <div className="cgfx-field__error" role="alert" data-testid="setup-error">
            {error}
          </div>
        )}
      </form>
    </main>
  );
}