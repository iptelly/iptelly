import { ChangeDetectionStrategy, Component } from "@angular/core";
import { NgbActiveModal } from "@ng-bootstrap/ng-bootstrap";
import { KeyboardShortcut } from "../models/keyboardShortcut";

@Component({
  selector: "app-shortcuts-help-modal",
  templateUrl: "./shortcuts-help-modal.component.html",
  changeDetection: ChangeDetectionStrategy.Eager,
  standalone: false,
})
export class ShortcutsHelpModalComponent {
  constructor(public activeModal: NgbActiveModal) {}
  name = "ShortcutsHelpModal";
  groups: { label: string; shortcuts: KeyboardShortcut[] }[] = [];

  set shortcuts(shortcuts: KeyboardShortcut[]) {
    this.groups = [];
    for (const shortcut of shortcuts) {
      let group = this.groups.find((g) => g.label === shortcut.label);
      if (!group) {
        group = { label: shortcut.label, shortcuts: [] };
        this.groups.push(group);
      }
      group.shortcuts.push(shortcut);
    }
  }
}
