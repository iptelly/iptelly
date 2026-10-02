import { TestBed } from "@angular/core/testing";
import { NgbModal, NgbModalRef } from "@ng-bootstrap/ng-bootstrap";
import { ActiveToast, ToastrService } from "ngx-toastr";
import { Subject } from "rxjs";
import { appTestModule } from "../testing/test-module";
import { ErrorService } from "./error.service";
import { ErrorModalComponent } from "./error-modal/error-modal.component";

describe("ErrorService", () => {
  let service: ErrorService;
  let toastr: ToastrService;

  beforeEach(() => {
    TestBed.configureTestingModule(appTestModule);
    service = TestBed.inject(ErrorService);
    toastr = TestBed.inject(ToastrService);
  });

  it("should be created", () => {
    expect(service).toBeTruthy();
  });

  describe("handleError", () => {
    let onTap: Subject<void>;
    let toastError: ReturnType<typeof vi.spyOn>;
    let modalOpen: ReturnType<typeof vi.spyOn>;
    let componentInstance: { error?: string };

    beforeEach(() => {
      vi.spyOn(console, "error").mockImplementation(() => {});
      onTap = new Subject();
      toastError = vi
        .spyOn(toastr, "error")
        .mockReturnValue({ onTap: onTap.asObservable() } as ActiveToast<unknown>);
      componentInstance = {};
      modalOpen = vi
        .spyOn(TestBed.inject(NgbModal), "open")
        .mockReturnValue({ componentInstance } as NgbModalRef);
    });

    it("shows a toast with the given message", () => {
      service.handleError("boom", "Could not save");
      expect(toastError).toHaveBeenCalledWith("Could not save. Click here for more info");
    });

    it("shows a generic toast when no message is given", () => {
      service.handleError("boom");
      expect(toastError).toHaveBeenCalledWith("An error occured. Click here for more info");
    });

    it("logs the error to the console", () => {
      service.handleError("boom");
      expect(console.error).toHaveBeenCalledWith("boom");
    });

    it("does not open the details modal until the toast is tapped", () => {
      service.handleError("boom");
      expect(modalOpen).not.toHaveBeenCalled();
    });

    it("opens the details modal with the error when the toast is tapped", () => {
      service.handleError("boom");
      onTap.next();

      expect(modalOpen).toHaveBeenCalledWith(ErrorModalComponent, {
        backdrop: "static",
        size: "xl",
      });
      expect(componentInstance.error).toBe("boom");
    });

    it("opens the modal only once if the toast is tapped repeatedly", () => {
      service.handleError("boom");
      onTap.next();
      onTap.next();
      expect(modalOpen).toHaveBeenCalledTimes(1);
    });
  });

  it("info shows an info toast", () => {
    const info = vi.spyOn(toastr, "info").mockReturnValue(null!);
    service.info("Heads up");
    expect(info).toHaveBeenCalledWith("Heads up");
  });

  it("success shows a success toast", () => {
    const success = vi.spyOn(toastr, "success").mockReturnValue(null!);
    service.success("Done");
    expect(success).toHaveBeenCalledWith("Done");
  });
});
