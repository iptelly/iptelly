import { ComponentFixture, TestBed } from "@angular/core/testing";
import { appTestModule } from "../../testing/test-module";

import { SetupComponent } from "./setup.component";

describe("SetupComponent", () => {
  let component: SetupComponent;
  let fixture: ComponentFixture<SetupComponent>;

  beforeEach(async () => {
    await TestBed.configureTestingModule(appTestModule).compileComponents();

    fixture = TestBed.createComponent(SetupComponent);
    component = fixture.componentInstance;
    fixture.detectChanges();
  });

  it("should create", () => {
    expect(component).toBeTruthy();
  });
});
