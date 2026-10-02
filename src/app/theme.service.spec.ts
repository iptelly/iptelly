import { TestBed } from "@angular/core/testing";

import { ThemeService } from "./theme.service";

describe("ThemeService", () => {
  let service: ThemeService;

  beforeEach(() => {
    service = TestBed.inject(ThemeService);
  });

  afterEach(() => {
    document.documentElement.removeAttribute("data-theme");
  });

  const theme = () => document.documentElement.getAttribute("data-theme");

  it("applies the classic theme", () => {
    service.applyTheme("classic");
    expect(theme()).toBe("classic");
  });

  it("applies the modern theme", () => {
    service.applyTheme("modern");
    expect(theme()).toBe("modern");
  });

  it("falls back to modern for unset or unknown themes", () => {
    service.applyTheme("classic");
    service.applyTheme(undefined);
    expect(theme()).toBe("modern");

    service.applyTheme("neon");
    expect(theme()).toBe("modern");
  });
});
