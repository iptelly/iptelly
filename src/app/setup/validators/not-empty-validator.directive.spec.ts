import { SimpleChange } from "@angular/core";
import { FormControl } from "@angular/forms";
import { NotEmptyValidatorDirective } from "./not-empty-validator.directive";

describe("NotEmptyValidatorDirective", () => {
  let directive: NotEmptyValidatorDirective;

  beforeEach(() => {
    directive = new NotEmptyValidatorDirective();
  });

  it("should create an instance", () => {
    expect(directive).toBeTruthy();
  });

  it("flags empty, blank and missing values", () => {
    expect(directive.validate(new FormControl(""))).toEqual({ empty: true });
    expect(directive.validate(new FormControl("   "))).toEqual({ empty: true });
    expect(directive.validate(new FormControl(null))).toEqual({ empty: true });
  });

  it("accepts values with content", () => {
    expect(directive.validate(new FormControl("My source"))).toBeNull();
    expect(directive.validate(new FormControl("  x  "))).toBeNull();
  });

  it("skips validation when disabled", () => {
    directive.disabled = true;
    expect(directive.validate(new FormControl(""))).toBeNull();
  });

  it("re-runs validation when disabled changes", () => {
    const onChange = vi.fn();
    directive.registerOnValidatorChange(onChange);

    directive.ngOnChanges({ disabled: new SimpleChange(false, true, false) });
    expect(onChange).toHaveBeenCalledTimes(1);
  });

  it("ignores changes to other inputs", () => {
    const onChange = vi.fn();
    directive.registerOnValidatorChange(onChange);

    directive.ngOnChanges({ other: new SimpleChange(1, 2, false) });
    expect(onChange).not.toHaveBeenCalled();
  });
});
