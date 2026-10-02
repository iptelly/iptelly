import { TimeAgoPipe } from "./time-ago.pipe";

describe("TimeAgoPipe", () => {
  const now = new Date(2026, 0, 15, 12, 0, 0);
  const secondsAgo = (seconds: number) => new Date(now.getTime() - seconds * 1000);
  let pipe: TimeAgoPipe;

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(now);
    pipe = new TimeAgoPipe();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns an empty string for empty values", () => {
    expect(pipe.transform(undefined)).toBe("");
    expect(pipe.transform(null)).toBe("");
    expect(pipe.transform("")).toBe("");
  });

  it("says just now for under 29 seconds", () => {
    expect(pipe.transform(now)).toBe("Just now");
    expect(pipe.transform(secondsAgo(28))).toBe("Just now");
  });

  it("counts seconds from 29 seconds", () => {
    expect(pipe.transform(secondsAgo(29))).toBe("29 seconds ago");
    expect(pipe.transform(secondsAgo(59))).toBe("59 seconds ago");
  });

  it("uses the singular for a count of one", () => {
    expect(pipe.transform(secondsAgo(60))).toBe("1 minute ago");
    expect(pipe.transform(secondsAgo(3600))).toBe("1 hour ago");
    expect(pipe.transform(secondsAgo(86400))).toBe("1 day ago");
  });

  it("uses the largest whole unit", () => {
    expect(pipe.transform(secondsAgo(150))).toBe("2 minutes ago");
    expect(pipe.transform(secondsAgo(3 * 3600 + 59 * 60))).toBe("3 hours ago");
    expect(pipe.transform(secondsAgo(10 * 86400))).toBe("10 days ago");
  });

  it("accepts date strings and timestamps", () => {
    expect(pipe.transform(secondsAgo(120).toISOString())).toBe("2 minutes ago");
    expect(pipe.transform(secondsAgo(120).getTime())).toBe("2 minutes ago");
  });
});
