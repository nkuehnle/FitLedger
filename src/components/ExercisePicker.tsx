import { useEffect, useMemo, useState } from "react";
import { api, type ExerciseInfo, type Muscle } from "../api";
import { EQUIPMENT_LABELS } from "../format";
import { ExerciseEditor } from "./ExerciseEditor";
import { Modal } from "./Modal";

const norm = (s: string) => s.toLowerCase().replace(/[^a-z0-9]/g, "");

/** Search the library and pick an exercise. `similarTo` adds a "works the same muscles" section. */
export function ExercisePicker({
  title,
  onPick,
  onClose,
  similarTo,
}: {
  title: string;
  onPick: (e: ExerciseInfo) => void;
  onClose: () => void;
  similarTo?: number | null;
}) {
  const [all, setAll] = useState<ExerciseInfo[]>([]);
  const [muscles, setMuscles] = useState<Muscle[]>([]);
  const [similar, setSimilar] = useState<ExerciseInfo[]>([]);
  const [q, setQ] = useState("");
  const [equipment, setEquipment] = useState<string | null>(null);
  const [muscle, setMuscle] = useState<string>("");
  const [creating, setCreating] = useState(false);

  useEffect(() => {
    api.listExercises(false).then(setAll);
    api.muscles().then((m) => setMuscles(m.muscles));
    if (similarTo) api.similarExercises(similarTo).then(setSimilar);
  }, [similarTo]);

  const shown = useMemo(() => {
    const nq = norm(q);
    return all
      .filter((e) => !equipment || e.equipment === equipment)
      .filter((e) => !muscle || e.primary.includes(muscle) || e.secondary.includes(muscle))
      .filter((e) => !nq || norm(e.name).includes(nq) || e.aliases.some((a) => norm(a).includes(nq)))
      .slice(0, 80);
  }, [all, q, equipment, muscle]);

  if (creating) {
    return (
      <ExerciseEditor
        initial={{ name: q }}
        onClose={() => setCreating(false)}
        onSaved={async (id) => {
          const info = await api.exerciseInfo(id);
          if (info) onPick(info);
        }}
      />
    );
  }

  const row = (e: ExerciseInfo) => (
    <button key={e.id} className="pick-row" onClick={() => onPick(e)}>
      <span>
        {e.name}
        <span className="muted small">
          {" "}
          ·{" "}
          {[EQUIPMENT_LABELS[e.equipment ?? ""] ?? e.equipment, ...e.primary.map((m) => muscles.find((x) => x.id === m)?.name ?? m)]
            .filter(Boolean)
            .join(" · ")}
        </span>
      </span>
      {e.sessions > 0 && <span className="muted small">{e.sessions}×</span>}
    </button>
  );

  return (
    <Modal title={title} onClose={onClose} wide>
      <input className="search" type="search" placeholder="Search exercises…" value={q} onChange={(e) => setQ(e.target.value)} autoFocus />
      <div className="chips-row" style={{ margin: "8px 0" }}>
        <button className={!equipment ? "chip-btn active" : "chip-btn"} onClick={() => setEquipment(null)}>
          All
        </button>
        {Object.entries(EQUIPMENT_LABELS).map(([k, v]) => (
          <button
            key={k}
            className={equipment === k ? "chip-btn active" : "chip-btn"}
            onClick={() => setEquipment(equipment === k ? null : k)}
          >
            {v}
          </button>
        ))}
      </div>
      <select value={muscle} onChange={(e) => setMuscle(e.target.value)} className="select">
        <option value="">Any muscle</option>
        {muscles.map((m) => (
          <option key={m.id} value={m.id}>
            {m.name}
          </option>
        ))}
      </select>
      {similar.length > 0 && !q && !equipment && !muscle && (
        <>
          <div className="section-label">Works the same muscles</div>
          <div className="pick-list">{similar.map(row)}</div>
          <div className="section-label">All exercises</div>
        </>
      )}
      <div className="pick-list">{shown.map(row)}</div>
      <button className="button" style={{ marginTop: 12 }} onClick={() => setCreating(true)}>
        + Create custom exercise{q ? ` “${q}”` : ""}
      </button>
    </Modal>
  );
}
