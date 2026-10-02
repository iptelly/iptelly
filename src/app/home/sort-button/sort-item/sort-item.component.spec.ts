import { ComponentFixture, TestBed } from "@angular/core/testing";
import { appTestModule } from "../../../../testing/test-module";

import { SortItemComponent } from "./sort-item.component";

describe("SortItemComponent", () => {
  let component: SortItemComponent;
  let fixture: ComponentFixture<SortItemComponent>;

  beforeEach(async () => {
    await TestBed.configureTestingModule(appTestModule).compileComponents();

    fixture = TestBed.createComponent(SortItemComponent);
    component = fixture.componentInstance;
    fixture.detectChanges();
  });

  it("should create", () => {
    expect(component).toBeTruthy();
  });
});
