import { describe, expect, it } from "vitest";
import readme from "../README.md?raw";
import { CLAUDE_GUIDE, CLAUDE_GUIDE_PRIVACY } from "./claudeGuide";

describe("Claude guide", () => {
  it("matches the README word for word", () => {
    CLAUDE_GUIDE.forEach((step, i) => expect(readme).toContain(`${i + 1}. ${step}`));
    expect(readme).toContain(CLAUDE_GUIDE_PRIVACY);
  });
});
