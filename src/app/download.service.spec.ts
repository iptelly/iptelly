import { TestBed } from "@angular/core/testing";
import { appTestModule } from "../testing/test-module";

import { DownloadService } from "./download.service";

describe("DownloadService", () => {
  let service: DownloadService;

  beforeEach(() => {
    TestBed.configureTestingModule(appTestModule);
    service = TestBed.inject(DownloadService);
  });

  it("should be created", () => {
    expect(service).toBeTruthy();
  });
});
