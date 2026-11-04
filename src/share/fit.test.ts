import { describe, expect, it } from "vitest";
import { fit } from "./ShareDialog";

describe("fit", () => {
  it("never cuts an emoji in half", () => {
    // Long PR lines like "Pec Deck (Machine)   🏆 1RM  🏆 Weight …"; try every cut position.
    for (let pad = 0; pad < 4; pad++) {
      const line = "x".repeat(pad) + "Lat Pulldown (Cable)   " + "🏆 1RM  🏆 Weight  🏆 Reps  🏆 Vol  ".repeat(3);
      const out = fit(line, 30);
      expect(out.endsWith("…")).toBe(true);
      expect(() => encodeURIComponent(out)).not.toThrow();
    }
  });
});
