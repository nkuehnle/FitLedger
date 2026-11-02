/** Title and the Overview / Exercises switch shared by the two Progress pages. */
export function ProgressHeader({ current }: { current: "stats" | "exercises" }) {
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
      </div>
    </div>
  );
}
