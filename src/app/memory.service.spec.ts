import { TestBed } from "@angular/core/testing";
import { mockIPC } from "@tauri-apps/api/mocks";
import { ToastrService } from "ngx-toastr";
import { appTestModule } from "../testing/test-module";

import { ErrorService } from "./error.service";
import { MemoryService } from "./memory.service";
import { LAST_SEEN_VERSION } from "./models/localStorage";

describe("MemoryService", () => {
  let service: MemoryService;

  beforeEach(() => {
    TestBed.configureTestingModule(appTestModule);
    service = TestBed.inject(MemoryService);
  });

  it("should be created", () => {
    expect(service).toBeTruthy();
  });

  describe("tryIPC", () => {
    let success: ReturnType<typeof vi.spyOn>;
    let handleError: ReturnType<typeof vi.spyOn>;

    beforeEach(() => {
      success = vi.spyOn(TestBed.inject(ToastrService), "success").mockReturnValue(null!);
      handleError = vi
        .spyOn(TestBed.inject(ErrorService), "handleError")
        .mockImplementation(() => {});
    });

    it("shows the success message and returns false when the action succeeds", async () => {
      const failed = await service.tryIPC("Saved", "Could not save", async () => {});
      expect(failed).toBe(false);
      expect(success).toHaveBeenCalledWith("Saved");
      expect(handleError).not.toHaveBeenCalled();
    });

    it("reports the error and returns true when the action fails", async () => {
      const failed = await service.tryIPC("Saved", "Could not save", async () => {
        throw "boom";
      });
      expect(failed).toBe(true);
      expect(handleError).toHaveBeenCalledWith("boom", "Could not save");
      expect(success).not.toHaveBeenCalled();
    });

    it("sets Loading while the action runs", async () => {
      let loadingDuringAction: boolean | undefined;
      await service.tryIPC("ok", "err", async () => {
        loadingDuringAction = service.Loading;
      });
      expect(loadingDuringAction).toBe(true);
      expect(service.Loading).toBe(false);
    });

    it("clears Loading after a failure too", async () => {
      await service.tryIPC("ok", "err", () => Promise.reject("boom"));
      expect(service.Loading).toBe(false);
    });
  });

  it("get_epg_ids loads the watched EPG ids from the backend", async () => {
    mockIPC((cmd) => (cmd === "get_epg_ids" ? ["a", "b", "a"] : null));
    await service.get_epg_ids();
    expect([...service.Watched_epgs]).toEqual(["a", "b"]);
  });

  describe("downloading channels", () => {
    it("tracks a channel from add to remove", () => {
      expect(service.downloadExists(1)).toBe(false);

      service.addDownloadingChannel(1);
      expect(service.downloadExists(1)).toBe(true);
      expect(service.getDownload(1)?.[0]).toBe(0);

      service.removeDownloadingChannel(1);
      expect(service.downloadExists(1)).toBe(false);
      expect(service.getDownload(1)).toBeUndefined();
    });

    it("stores the last progress for a channel", () => {
      service.addDownloadingChannel(1);
      service.setLastDownloadProgress(1, 42);
      expect(service.getDownload(1)?.[0]).toBe(42);
    });

    it("notifies subscribers when a download finishes", () => {
      service.addDownloadingChannel(1);
      const finished = vi.fn();
      service.getDownload(1)![1].subscribe(finished);

      service.notifyDownloadFinished(1);
      expect(finished).toHaveBeenCalledWith(true);
    });

    it("ignores finish notifications for unknown channels", () => {
      expect(() => service.notifyDownloadFinished(99)).not.toThrow();
    });
  });

  describe("updateVersion", () => {
    afterEach(() => {
      localStorage.removeItem(LAST_SEEN_VERSION);
    });

    it("stores the app version as the last seen version", () => {
      service.AppVersion = "1.2.3";
      service.updateVersion();
      expect(localStorage.getItem(LAST_SEEN_VERSION)).toBe("1.2.3");
    });

    it("does nothing when the app version is not known yet", () => {
      service.AppVersion = undefined;
      service.updateVersion();
      expect(localStorage.getItem(LAST_SEEN_VERSION)).toBeNull();
    });
  });
});
