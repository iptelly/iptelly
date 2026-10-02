import { TestBed } from "@angular/core/testing";
import { appTestModule } from "../testing/test-module";

import { MemoryService } from "./memory.service";

describe("MemoryService", () => {
  let service: MemoryService;

  beforeEach(() => {
    TestBed.configureTestingModule(appTestModule);
    service = TestBed.inject(MemoryService);
  });

  it("should be created", () => {
    expect(service).toBeTruthy();
  });
});
