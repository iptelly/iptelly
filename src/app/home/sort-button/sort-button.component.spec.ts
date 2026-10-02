import { ComponentFixture, TestBed } from "@angular/core/testing";
import { appTestModule } from "../../../testing/test-module";

import { SortButtonComponent } from "./sort-button.component";

describe("SortButtonComponent", () => {
  let component: SortButtonComponent;
  let fixture: ComponentFixture<SortButtonComponent>;

  beforeEach(async () => {
    await TestBed.configureTestingModule(appTestModule).compileComponents();

    fixture = TestBed.createComponent(SortButtonComponent);
    component = fixture.componentInstance;
    fixture.detectChanges();
  });

  it("should create", () => {
    expect(component).toBeTruthy();
  });
});
