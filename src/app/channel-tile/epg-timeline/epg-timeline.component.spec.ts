import { ComponentFixture, TestBed } from "@angular/core/testing";
import { appTestModule } from "../../../testing/test-module";

import { EpgTimelineComponent } from "./epg-timeline.component";

describe("EpgTimelineComponent", () => {
  let component: EpgTimelineComponent;
  let fixture: ComponentFixture<EpgTimelineComponent>;

  beforeEach(async () => {
    await TestBed.configureTestingModule(appTestModule).compileComponents();

    fixture = TestBed.createComponent(EpgTimelineComponent);
    component = fixture.componentInstance;
    fixture.detectChanges();
  });

  it("should create", () => {
    expect(component).toBeTruthy();
  });
});
