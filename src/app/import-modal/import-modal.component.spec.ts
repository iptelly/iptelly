import { ComponentFixture, TestBed } from "@angular/core/testing";
import { appTestModule } from "../../testing/test-module";

import { ImportModalComponent } from "./import-modal.component";

describe("ImportModalComponent", () => {
  let component: ImportModalComponent;
  let fixture: ComponentFixture<ImportModalComponent>;

  beforeEach(async () => {
    await TestBed.configureTestingModule(appTestModule).compileComponents();

    fixture = TestBed.createComponent(ImportModalComponent);
    component = fixture.componentInstance;
    fixture.detectChanges();
  });

  it("should create", () => {
    expect(component).toBeTruthy();
  });
});
