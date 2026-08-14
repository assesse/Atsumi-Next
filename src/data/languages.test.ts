import { describe, expect, it } from "vitest";
import { languagePresentation } from "./languages";

describe("Classic language assets", () => {
  it("uses PNG assets captured from the Classic runtime for its three countries", () => {
    const icons = [
      languagePresentation.korean.icon,
      languagePresentation.japanese.icon,
      languagePresentation.english.icon,
    ];

    const isPngAssetReference = (icon: string | null) =>
      Boolean(
        icon &&
          (icon.startsWith("data:image/png") ||
            /\/classic-runtime\/(?:kr|jp|us)\.png(?:\?|$)/.test(icon)),
      );

    expect(icons.every(isPngAssetReference)).toBe(true);
    expect(new Set(icons).size).toBe(3);
  });

  it("omits both the icon and fallback for Chinese like Classic", () => {
    expect(languagePresentation.chinese.icon).toBeNull();
    expect(languagePresentation.chinese.fallback).toBeNull();
  });
});
