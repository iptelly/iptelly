import { FormControl } from "@angular/forms";
import { mockIPC } from "@tauri-apps/api/mocks";
import { firstValueFrom } from "rxjs";
import { SourceNameExistsValidator } from "./source-name-exists-validator.directive";

describe("SourceNameExistsValidatorDirective", () => {
  let directive: SourceNameExistsValidator;
  let calls: { cmd: string; args: unknown }[];
  let exists: boolean;

  beforeEach(() => {
    directive = new SourceNameExistsValidator();
    calls = [];
    exists = false;
    mockIPC((cmd, args) => {
      calls.push({ cmd, args });
      return exists;
    });
  });

  it("should create an instance", () => {
    expect(directive).toBeTruthy();
  });

  it("skips the backend for empty or blank names", async () => {
    expect(await firstValueFrom(directive.validate(new FormControl("")))).toBeNull();
    expect(await firstValueFrom(directive.validate(new FormControl("  ")))).toBeNull();
    expect(await firstValueFrom(directive.validate(new FormControl(null)))).toBeNull();
    expect(calls).toEqual([]);
  });

  it("flags a name that already exists", async () => {
    exists = true;
    expect(await firstValueFrom(directive.validate(new FormControl("Taken")))).toEqual({
      sourceNameExists: true,
    });
  });

  it("accepts a name that does not exist", async () => {
    expect(await firstValueFrom(directive.validate(new FormControl("Free")))).toBeNull();
  });

  it("sends the trimmed name to the backend", async () => {
    await firstValueFrom(directive.validate(new FormControl("  My source  ")));
    expect(calls).toEqual([{ cmd: "source_name_exists", args: { name: "My source" } }]);
  });
});
