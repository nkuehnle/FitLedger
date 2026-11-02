import { useEffect, useState } from "react";
import { api, type ExerciseEdit, type Muscle, type MuscleGroup } from "../api";
import { EQUIPMENT_LABELS } from "../format";
import { Modal } from "./Modal";

const PATTERNS = [
  "horizontal_push",
  "vertical_push",
  "horizontal_pull",
  "vertical_pull",
  "squat",
  "hinge",
  "lunge",
  "isolation",
  "core",
  "carry",
  "cardio",
  "olympic",
  "other",
];
const KINDS: [string, string][] = [
  ["weight_reps", "Weight × reps"],
  ["bodyweight_reps", "Bodyweight (+ added weight) × reps"],
  ["assisted_bodyweight", "Assisted bodyweight × reps"],
  ["duration", "Time"],
  ["distance_duration", "Distance + time"],
  ["weight_distance", "Weight + distance"],
];

const BLANK: ExerciseEdit = {
  name: "",
  equipment: "barbell",
  pattern: "isolation",
  kind: "weight_reps",
  load_mode: "total",
  default_rest_s: null,
  setup_note: null,
  primary: [],
  secondary: [],
};

/** Create a custom exercise (`id` omitted) or edit any exercise, including its muscles. */
export function ExerciseEditor({
  id,
  initial,
  onClose,
  onSaved,
}: {
  id?: number;
  initial?: Partial<ExerciseEdit>;
  onClose: () => void;
  onSaved: (id: number) => void;
}) {
  const [e, setE] = useState<ExerciseEdit>({ ...BLANK, ...initial });
  const [groups, setGroups] = useState<MuscleGroup[]>([]);
  const [muscles, setMuscles] = useState<Muscle[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.muscles().then((m) => {
      setGroups(m.groups);
      setMuscles(m.muscles);
    });
    if (id)
      api.exerciseInfo(id).then((x) => {
        if (x)
          setE({
            name: x.name,
            equipment: x.equipment,
            pattern: x.pattern,
            kind: x.kind,
            load_mode: x.load_mode,
            default_rest_s: x.default_rest_s,
            setup_note: x.setup_note,
            primary: x.primary,
            secondary: x.secondary,
          });
      });
  }, [id]);

  const role = (m: string) => (e.primary.includes(m) ? "primary" : e.secondary.includes(m) ? "secondary" : "none");
  const cycle = (m: string) => {
    const r = role(m);
    const primary = e.primary.filter((x) => x !== m);
    const secondary = e.secondary.filter((x) => x !== m);
    if (r === "none") primary.push(m);
    else if (r === "primary") secondary.push(m);
    setE({ ...e, primary, secondary });
  };

  async function save() {
    setError(null);
    try {
      const saved = id ? (await api.updateExercise(id, e), id) : await api.createExercise(e);
      onSaved(saved);
      onClose();
    } catch (err) {
      setError(String(err));
    }
  }

  return (
    <Modal title={id ? "Edit exercise" : "New exercise"} onClose={onClose} wide>
      <div className="form">
        <label>
          Name
          <input value={e.name} onChange={(x) => setE({ ...e, name: x.target.value })} autoFocus={!id} />
        </label>
        <div className="form-row">
          <label>
            Equipment
            <select className="select" value={e.equipment ?? ""} onChange={(x) => setE({ ...e, equipment: x.target.value || null })}>
              {Object.entries(EQUIPMENT_LABELS).map(([k, v]) => (
                <option key={k} value={k}>
                  {v}
                </option>
              ))}
            </select>
          </label>
          <label>
            Movement
            <select className="select" value={e.pattern ?? ""} onChange={(x) => setE({ ...e, pattern: x.target.value || null })}>
              {PATTERNS.map((p) => (
                <option key={p} value={p}>
                  {p.replace(/_/g, " ")}
                </option>
              ))}
            </select>
          </label>
        </div>
        <div className="form-row">
          <label>
            Logged as
            <select className="select" value={e.kind} onChange={(x) => setE({ ...e, kind: x.target.value })}>
              {KINDS.map(([k, v]) => (
                <option key={k} value={k}>
                  {v}
                </option>
              ))}
            </select>
          </label>
          <label>
            Weight entered as
            <select
              className="select"
              value={e.load_mode}
              onChange={(x) => setE({ ...e, load_mode: x.target.value as "total" | "per_hand" })}
            >
              <option value="total">Total (both hands)</option>
              <option value="per_hand">Per hand</option>
            </select>
          </label>
          <label>
            Default rest (s)
            <input
              type="number"
              inputMode="numeric"
              value={e.default_rest_s ?? ""}
              onChange={(x) => setE({ ...e, default_rest_s: x.target.value === "" ? null : Number(x.target.value) })}
            />
          </label>
        </div>
        <label>
          Sticky note (shown every time you log it)
          <textarea
            rows={2}
            value={e.setup_note ?? ""}
            placeholder="Seat 4, pin 3, cable at shoulder height…"
            onChange={(x) => setE({ ...e, setup_note: x.target.value || null })}
          />
        </label>
        <div>
          <div className="section-label">Muscles · tap to cycle: primary (1 set) → secondary (½ set) → none</div>
          {groups.map((g) => (
            <div key={g.id} className="muscle-group-row">
              <span className="muted small">{g.name}</span>
              <div className="chips-wrap">
                {muscles
                  .filter((m) => m.group === g.id)
                  .map((m) => (
                    <button key={m.id} className={`muscle-chip ${role(m.id)}`} onClick={() => cycle(m.id)}>
                      {m.name}
                    </button>
                  ))}
              </div>
            </div>
          ))}
        </div>
        {error && <p className="error">{error}</p>}
        <div className="row">
          <button className="button primary" onClick={save} disabled={!e.name.trim()}>
            Save
          </button>
          <button className="button" onClick={onClose}>
            Cancel
          </button>
        </div>
      </div>
    </Modal>
  );
}
