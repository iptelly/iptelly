import { ComponentFixture, TestBed } from "@angular/core/testing";
import { appTestModule } from "../../testing/test-module";

import { EditChannelModalComponent } from "./edit-channel-modal.component";

describe("EditChannelModalComponent", () => {
  let component: EditChannelModalComponent;
  let fixture: ComponentFixture<EditChannelModalComponent>;

  beforeEach(async () => {
    await TestBed.configureTestingModule(appTestModule).compileComponents();

    fixture = TestBed.createComponent(EditChannelModalComponent);
    component = fixture.componentInstance;
    fixture.detectChanges();
  });

  it("should create", () => {
    expect(component).toBeTruthy();
  });
});
