import { ComponentFixture, TestBed } from "@angular/core/testing";
import { appTestModule } from "../../testing/test-module";

import { ConfirmDeleteModalComponent } from "./confirm-delete-modal.component";

describe("ConfirmDeleteModalComponent", () => {
  let component: ConfirmDeleteModalComponent;
  let fixture: ComponentFixture<ConfirmDeleteModalComponent>;

  beforeEach(async () => {
    await TestBed.configureTestingModule(appTestModule).compileComponents();

    fixture = TestBed.createComponent(ConfirmDeleteModalComponent);
    component = fixture.componentInstance;
    fixture.detectChanges();
  });

  it("should create", () => {
    expect(component).toBeTruthy();
  });
});
