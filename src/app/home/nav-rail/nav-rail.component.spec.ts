import { ComponentFixture, TestBed } from "@angular/core/testing";
import { appTestModule } from "../../../testing/test-module";

import { NavRailComponent } from "./nav-rail.component";

describe("NavRailComponent", () => {
  let component: NavRailComponent;
  let fixture: ComponentFixture<NavRailComponent>;

  beforeEach(async () => {
    await TestBed.configureTestingModule(appTestModule).compileComponents();

    fixture = TestBed.createComponent(NavRailComponent);
    component = fixture.componentInstance;
    fixture.detectChanges();
  });

  it("should create", () => {
    expect(component).toBeTruthy();
  });
});
