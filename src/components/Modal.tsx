import { useEffect, useState, type ReactNode } from "react";
import { confirm, message } from "@tauri-apps/plugin-dialog";

/** Bottom sheet on phones, centered dialog on wide windows. */
export function Modal({ title, onClose, children, wide }: { title: string; onClose: () => void; children: ReactNode; wide?: boolean }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className={wide ? "modal wide" : "modal"} role="dialog" aria-label={title} onClick={(e) => e.stopPropagation()}>
        <header className="modal-head">
          <h2>{title}</h2>
          <button className="icon-btn" onClick={onClose} aria-label="Close">
            ✕
          </button>
        </header>
        <div className="modal-body">{children}</div>
      </div>
    </div>
  );
}

/** Yes/no question as a native dialog (window.confirm doesn't work in every WebView). */
export async function ask(text: string): Promise<boolean> {
  try {
    return await confirm(text, { title: "Liftbook", kind: "warning" });
  } catch {
    return window.confirm(text);
  }
}

export async function tell(text: string): Promise<void> {
  try {
    await message(text, { title: "Liftbook" });
  } catch {
    window.alert(text);
  }
}

/** Asks for a short text value in a small dialog. */
export function TextPrompt({ title, initial, onDone }: { title: string; initial: string; onDone: (v: string | null) => void }) {
  const [v, setV] = useState(initial);
  return (
    <Modal title={title} onClose={() => onDone(null)}>
      <form
        className="form"
        onSubmit={(e) => {
          e.preventDefault();
          onDone(v.trim() || null);
        }}
      >
        <input value={v} onChange={(e) => setV(e.target.value)} autoFocus />
        <div className="row">
          <button className="button primary" type="submit" disabled={!v.trim()}>
            OK
          </button>
          <button className="button" type="button" onClick={() => onDone(null)}>
            Cancel
          </button>
        </div>
      </form>
    </Modal>
  );
}
