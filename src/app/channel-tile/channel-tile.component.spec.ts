import { ComponentFixture, TestBed } from "@angular/core/testing";
import { appTestModule } from "../../testing/test-module";

import { Channel } from "../models/channel";
import { ChannelTileComponent } from "./channel-tile.component";

describe("ChannelTileComponent", () => {
  let component: ChannelTileComponent;
  let fixture: ComponentFixture<ChannelTileComponent>;

  beforeEach(async () => {
    await TestBed.configureTestingModule(appTestModule).compileComponents();

    fixture = TestBed.createComponent(ChannelTileComponent);
    component = fixture.componentInstance;
    fixture.componentRef.setInput("channel", { id: 1, name: "Test channel" } as Channel);
    fixture.componentRef.setInput("id", 0);
    fixture.detectChanges();
  });

  it("should create", () => {
    expect(component).toBeTruthy();
  });
});
