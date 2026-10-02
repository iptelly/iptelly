import { TimeUntilPipe } from "./time-until.pipe";

describe("TimeUntilPipe", () => {
  // Local time, so the formatted date matches whatever timezone the tests
  // run in. January avoids DST changes in the northern hemisphere.
  const now = new Date(2026, 0, 1, 12, 0, 0);
  const inSeconds = (seconds: number) => new Date(now.getTime() + seconds * 1000);
  const HOUR = 3600;
  const DAY = 86400;
  let pipe: TimeUntilPipe;

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(now);
    pipe = new TimeUntilPipe();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns an empty string for empty values", () => {
    expect(pipe.transform(undefined)).toBe("");
    expect(pipe.transform(null)).toBe("");
  });

  it("marks past dates as expired", () => {
    expect(pipe.transform(new Date(2025, 11, 25))).toBe("Expired (December 25th 2025)");
  });

  it("says less than an hour for under an hour away", () => {
    expect(pipe.transform(inSeconds(0))).toBe("In less than an hour (January 1st 2026)");
    expect(pipe.transform(inSeconds(HOUR - 1))).toBe("In less than an hour (January 1st 2026)");
  });

  it("uses the singular for a count of one", () => {
    expect(pipe.transform(inSeconds(HOUR))).toBe("In 1 hour (January 1st 2026)");
    expect(pipe.transform(inSeconds(DAY))).toBe("In 1 day (January 2nd 2026)");
    expect(pipe.transform(inSeconds(7 * DAY))).toBe("In 1 week (January 8th 2026)");
  });

  it("uses the largest whole unit", () => {
    expect(pipe.transform(inSeconds(5 * HOUR))).toBe("In 5 hours (January 1st 2026)");
    expect(pipe.transform(inSeconds(2 * DAY))).toBe("In 2 days (January 3rd 2026)");
    expect(pipe.transform(inSeconds(20 * DAY))).toBe("In 2 weeks (January 21st 2026)");
    expect(pipe.transform(inSeconds(45 * DAY))).toBe("In 1 month (February 15th 2026)");
    expect(pipe.transform(inSeconds(90 * DAY))).toBe("In 3 months (April 1st 2026)");
    expect(pipe.transform(inSeconds(365 * DAY))).toBe("In 1 year (January 1st 2027)");
    expect(pipe.transform(inSeconds(800 * DAY))).toBe("In 2 years (March 11th 2028)");
  });

  it("uses the right ordinal suffix for each day", () => {
    const expected: Record<number, string> = {
      1: "st",
      2: "nd",
      3: "rd",
      4: "th",
      11: "th",
      12: "th",
      13: "th",
      20: "th",
      21: "st",
      22: "nd",
      23: "rd",
      24: "th",
      30: "th",
      31: "st",
    };
    for (const [day, suffix] of Object.entries(expected)) {
      expect(pipe.transform(new Date(2025, 9, +day))).toBe(
        `Expired (October ${day}${suffix} 2025)`,
      );
    }
  });
});
