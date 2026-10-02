import { FormControl } from "@angular/forms";
import { mockIPC } from "@tauri-apps/api/mocks";
import { firstValueFrom } from "rxjs";
import { GroupNameExistsValidator } from "./group-name-exists.directive";

describe("GroupNameExistsDirective", () => {
  let directive: GroupNameExistsValidator;
  let calls: { cmd: string; args: unknown }[];
  let exists: boolean;

  beforeEach(() => {
    directive = new GroupNameExistsValidator();
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
    expect(calls).toEqual([]);
  });

  it("skips the backend when the name is unchanged", async () => {
    directive.originalName = "Sports";
    expect(await firstValueFrom(directive.validate(new FormControl(" Sports ")))).toBeNull();
    expect(calls).toEqual([]);
  });

  it("flags a name that already exists in the source", async () => {
    exists = true;
    directive.originalName = "Sports";
    expect(await firstValueFrom(directive.validate(new FormControl("News")))).toEqual({
      groupNameExists: true,
    });
  });

  it("accepts a name that does not exist", async () => {
    expect(await firstValueFrom(directive.validate(new FormControl("News")))).toBeNull();
  });

  it("sends the trimmed name and source id to the backend", async () => {
    directive.sourceId = 7;
    await firstValueFrom(directive.validate(new FormControl("  News  ")));
    expect(calls).toEqual([{ cmd: "group_exists", args: { name: "News", sourceId: 7 } }]);
  });
});
