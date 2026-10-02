import {
  formatBytes,
  getDateFormatted,
  getExtension,
  isInputFocused,
  sanitizeFileName,
} from "./utils";

describe("utils", () => {
  describe("isInputFocused", () => {
    afterEach(() => {
      document.body.innerHTML = "";
    });

    for (const tag of ["input", "select", "button", "textarea"]) {
      it(`is true when a ${tag} has focus`, () => {
        const el = document.createElement(tag);
        document.body.appendChild(el);
        el.focus();
        expect(isInputFocused()).toBe(true);
      });
    }

    it("is false when nothing has focus", () => {
      (document.activeElement as HTMLElement | null)?.blur();
      expect(isInputFocused()).toBeFalsy();
    });

    it("is false when a non-input element has focus", () => {
      const div = document.createElement("div");
      div.tabIndex = 0;
      document.body.appendChild(div);
      div.focus();
      expect(isInputFocused()).toBeFalsy();
    });
  });

  describe("sanitizeFileName", () => {
    it("replaces characters that are invalid in file names", () => {
      expect(sanitizeFileName('a/b\\c:d*e?f"g<h>i|j')).toBe("a_b_c_d_e_f_g_h_i_j");
    });

    it("strips control characters", () => {
      expect(sanitizeFileName("a\x00b\x1Fc\x7F")).toBe("abc");
    });

    it("strips leading and trailing dots", () => {
      expect(sanitizeFileName("..hidden..")).toBe("hidden");
    });

    it("trims surrounding whitespace", () => {
      expect(sanitizeFileName("  My Channel  ")).toBe("My Channel");
    });

    it("keeps inner dots and spaces", () => {
      expect(sanitizeFileName("Show S01.E02 final")).toBe("Show S01.E02 final");
    });

    it("falls back to untitled when nothing is left", () => {
      expect(sanitizeFileName("")).toBe("untitled");
      expect(sanitizeFileName("...")).toBe("untitled");
      expect(sanitizeFileName("   ")).toBe("untitled");
    });
  });

  describe("getDateFormatted", () => {
    afterEach(() => {
      vi.useRealTimers();
    });

    it("formats the current UTC time with dashes and no milliseconds", () => {
      vi.useFakeTimers();
      vi.setSystemTime(new Date(Date.UTC(2026, 0, 2, 3, 4, 5, 678)));
      expect(getDateFormatted()).toBe("2026-01-02-03-04-05");
    });
  });

  describe("getExtension", () => {
    it("returns the extension after the last dot", () => {
      expect(getExtension("http://example.com:8080/movie/u/p/123.mkv")).toBe("mkv");
      expect(getExtension("http://example.com/live/u/p/1.ts")).toBe("ts");
    });

    it("defaults to mp4 when there is no dot", () => {
      expect(getExtension("stream")).toBe("mp4");
    });

    it("defaults to mp4 when the path has no extension", () => {
      expect(getExtension("http://example.com/stream")).toBe("mp4");
      expect(getExtension("http://example.com:8080/live/u/p/123")).toBe("mp4");
      expect(getExtension("http://example.com")).toBe("mp4");
      expect(getExtension("http://example.com/")).toBe("mp4");
      expect(getExtension("http://192.168.1.10/movies.dir/stream")).toBe("mp4");
    });

    it("defaults to mp4 for php endpoints", () => {
      expect(getExtension("http://example.com/get.php?username=a&password=b")).toBe("mp4");
      expect(getExtension("http://example.com/stream.php")).toBe("mp4");
    });

    it("ignores the query string and fragment", () => {
      expect(getExtension("http://example.com/movie.mkv?token=a.b")).toBe("mkv");
      expect(getExtension("http://example.com/movie.mp4#t=1.5")).toBe("mp4");
      expect(getExtension("http://example.com/play?file=movie.avi")).toBe("mp4");
    });
  });

  describe("formatBytes", () => {
    it("returns 0 B for missing, zero or negative sizes", () => {
      expect(formatBytes()).toBe("0 B");
      expect(formatBytes(0)).toBe("0 B");
      expect(formatBytes(-5)).toBe("0 B");
    });

    it("shows whole bytes without decimals", () => {
      expect(formatBytes(1)).toBe("1 B");
      expect(formatBytes(1023)).toBe("1023 B");
    });

    it("scales to larger units with one decimal place", () => {
      expect(formatBytes(1024)).toBe("1.0 KB");
      expect(formatBytes(1536)).toBe("1.5 KB");
      expect(formatBytes(5 * 1024 ** 2)).toBe("5.0 MB");
      expect(formatBytes(2.5 * 1024 ** 3)).toBe("2.5 GB");
      expect(formatBytes(1024 ** 4)).toBe("1.0 TB");
    });

    it("caps at TB", () => {
      expect(formatBytes(3 * 1024 ** 5)).toBe("3072.0 TB");
    });
  });
});
