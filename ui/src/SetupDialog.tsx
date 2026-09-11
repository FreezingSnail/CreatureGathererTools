// Startup project chooser: open an existing manifest or create a new
// creature-tools.json in a selected project directory.

import { useState, type FormEvent } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { Config } from "./types";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

export interface SetupFields {
  creatures_json: string;
  moves_json: string;
  map_json: string;
  map_tileset_png: string;
  npc_tileset_png: string;
  output_dir: string;
  fx_layout: string;
}

const FIELDS: { key: keyof SetupFields; label: string }[] = [
  { key: "creatures_json", label: "Creatures JSON" },
  { key: "moves_json", label: "Moves JSON" },
  { key: "map_json", label: "Map JSON" },
  { key: "map_tileset_png", label: "Map Tileset PNG" },
  { key: "npc_tileset_png", label: "NPC Tileset PNG" },
  { key: "output_dir", label: "Output Dir" },
  { key: "fx_layout", label: "FX Layout (optional)" },
];

const emptyValues = (): SetupFields => ({
  creatures_json: "",
  moves_json: "",
  map_json: "",
  map_tileset_png: "",
  npc_tileset_png: "",
  output_dir: "",
  fx_layout: "",
});

interface SetupDialogProps {
  /** Prior config fields to prefill (e.g. after a failed reload). */
  initial?: Config;
  error?: string;
  onSubmit: (config: Config, projectDir: string) => void;
  onOpenProject: (projectPath: string) => void;
}

export function SetupDialog({ initial, error, onSubmit, onOpenProject }: SetupDialogProps) {
  const [projectDir, setProjectDir] = useState("");
  const [values, setValues] = useState<SetupFields>(() => {
    if (!initial) return emptyValues();
    return {
      creatures_json: initial.creatures_json,
      moves_json: initial.moves_json,
      map_json: initial.map_json,
      map_tileset_png: initial.map_tileset_png,
      npc_tileset_png: initial.npc_tileset_png,
      output_dir: initial.output_dir,
      fx_layout: initial.fx_layout ?? "",
    };
  });

  const setField = (key: keyof SetupFields, value: string) =>
    setValues((v) => ({ ...v, [key]: value }));

  const chooseProjectDir = async () => {
    const selected = await open({ directory: true, multiple: false, title: "Choose project folder" });
    if (typeof selected === "string") setProjectDir(selected);
  };

  const chooseProjectManifest = async () => {
    const selected = await open({
      multiple: false,
      title: "Open project manifest",
      filters: [{ name: "Project manifest", extensions: ["json"] }],
    });
    if (typeof selected === "string") onOpenProject(selected);
  };

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
      fx_layout: values.fx_layout || undefined,
    }, projectDir);
  };

  return (
    <main className="flex min-h-screen items-center justify-center p-6" data-testid="setup-dialog">
      <Card className="w-full max-w-md">
        <CardHeader>
          <CardTitle>CreatureGathererFX — New project</CardTitle>
          <CardDescription>Choose a folder. Its creature-tools.json is this project’s config.</CardDescription>
        </CardHeader>
        <CardContent>
          <form className="grid gap-4" onSubmit={submit}>
            <Button type="button" variant="outline" onClick={() => void chooseProjectManifest()}>
              Open existing project
            </Button>
            <p className="text-center text-sm text-muted-foreground">or create a new project</p>
            <div className="grid gap-2">
              <Label htmlFor="setup-project-dir">Project folder</Label>
              <div className="flex gap-2">
                <Input id="setup-project-dir" value={projectDir} onChange={(e) => setProjectDir(e.target.value)} />
                <Button type="button" variant="outline" onClick={() => void chooseProjectDir()}>Choose folder</Button>
              </div>
            </div>
            {FIELDS.map((f) => (
              <div className="grid gap-2" key={f.key}>
                <Label htmlFor={`setup-${f.key}`}>{f.label}</Label>
                <Input
                  id={`setup-${f.key}`}
                  type="text"
                  value={values[f.key]}
                  onChange={(e) => setField(f.key, e.target.value)}
                />
              </div>
            ))}
            <Button type="submit" className="mt-2" disabled={!projectDir.trim()}>
              Create project
            </Button>
            {error && (
              <Alert variant="destructive" data-testid="setup-error">
                <AlertTitle>Setup failed</AlertTitle>
                <AlertDescription>{error}</AlertDescription>
              </Alert>
            )}
          </form>
        </CardContent>
      </Card>
    </main>
  );
}
