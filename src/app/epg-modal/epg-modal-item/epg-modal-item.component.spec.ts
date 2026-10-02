import { ComponentFixture, TestBed } from "@angular/core/testing";
import { appTestModule } from "../../../testing/test-module";

import { EPG } from "../../models/epg";
import { EpgModalItemComponent } from "./epg-modal-item.component";

describe("EpgModalItemComponent", () => {
  let component: EpgModalItemComponent;
  let fixture: ComponentFixture<EpgModalItemComponent>;

  beforeEach(async () => {
    await TestBed.configureTestingModule(appTestModule).compileComponents();

    fixture = TestBed.createComponent(EpgModalItemComponent);
    component = fixture.componentInstance;
    const start = Math.floor(Date.now() / 1000);
    fixture.componentRef.setInput("epg", {
      epg_id: "1",
      title: "Test programme",
      description: "",
      start_time: "",
      start_timestamp: start,
      end_time: "",
      end_timestamp: start + 3600,
      has_archive: false,
      now_playing: true,
    } satisfies EPG);
    fixture.detectChanges();
  });

  it("should create", () => {
    expect(component).toBeTruthy();
  });
});
