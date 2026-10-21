import { describe, expect, it } from "vitest";
import { MAX_PICTURE_BYTES, fitWithin, pictureProblem } from "./images";

describe("picture files", () => {
  it("accepts the four picture types up to 5 MB", () => {
    for (const type of ["image/png", "image/jpeg", "image/webp", "image/gif"]) {
      expect(pictureProblem({ type, size: 1_000 })).toBeNull();
    }
    expect(pictureProblem({ type: "image/png", size: MAX_PICTURE_BYTES })).toBeNull();
  });

  it("says what is wrong with anything else", () => {
    expect(pictureProblem({ type: "application/pdf", size: 1_000 })).toMatch(/PNG/);
    expect(pictureProblem({ type: "image/bmp", size: 1_000 })).toMatch(/PNG/);
    expect(pictureProblem({ type: "image/png", size: MAX_PICTURE_BYTES + 1 })).toMatch(/5 MB/);
    expect(pictureProblem({ type: "image/png", size: 0 })).toMatch(/empty/);
  });

  it("scales the long side down and keeps the shape", () => {
    expect(fitWithin(4000, 3000, 1000)).toEqual({ width: 1000, height: 750 });
    expect(fitWithin(3000, 4000, 1000)).toEqual({ width: 750, height: 1000 });
    expect(fitWithin(500, 400, 1000)).toEqual({ width: 500, height: 400 });
    expect(fitWithin(10_000, 1, 1000)).toEqual({ width: 1000, height: 1 });
  });
});
