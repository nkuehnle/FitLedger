/** Title and the Overview / Exercises / Cardio switch shared by the Progress pages. */
export function ProgressHeader({ current }: { current: "stats" | "exercises" | "cardio" }) {
  return (
    <div className="title-row">
      <h1 className="page-title">Progress</h1>
      <div className="segmented small">
        <a href="#/stats" className={current === "stats" ? "seg-link active" : "seg-link"}>
          Overview
        </a>
        <a href="#/exercises" className={current === "exercises" ? "seg-link active" : "seg-link"}>
          Exercises
        </a>
        <a href="#/cardio" className={current === "cardio" ? "seg-link active" : "seg-link"}>
          Cardio
        </a>
      </div>
    </div>
  );
}
