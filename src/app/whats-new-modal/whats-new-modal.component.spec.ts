import { ComponentFixture, TestBed } from "@angular/core/testing";
import { appTestModule } from "../../testing/test-module";

import { WhatsNewModalComponent } from "./whats-new-modal.component";

describe("WhatsNewModalComponent", () => {
  let component: WhatsNewModalComponent;
  let fixture: ComponentFixture<WhatsNewModalComponent>;

  beforeEach(async () => {
    await TestBed.configureTestingModule(appTestModule).compileComponents();

    fixture = TestBed.createComponent(WhatsNewModalComponent);
    component = fixture.componentInstance;
    fixture.detectChanges();
  });

  it("should create", () => {
    expect(component).toBeTruthy();
  });
});
