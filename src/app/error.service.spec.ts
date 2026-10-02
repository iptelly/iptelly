import { TestBed } from "@angular/core/testing";
import { appTestModule } from "../testing/test-module";

import { ErrorService } from "./error.service";

describe("ErrorService", () => {
  let service: ErrorService;

  beforeEach(() => {
    TestBed.configureTestingModule(appTestModule);
    service = TestBed.inject(ErrorService);
  });

  it("should be created", () => {
    expect(service).toBeTruthy();
  });
});
