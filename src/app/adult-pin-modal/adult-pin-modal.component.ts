import { Component, ElementRef, ViewChild } from "@angular/core";
import { NgbActiveModal } from "@ng-bootstrap/ng-bootstrap";
import { invoke } from "@tauri-apps/api/core";
import { ErrorService } from "../error.service";

@Component({
  selector: "app-adult-pin-modal",
  templateUrl: "./adult-pin-modal.component.html",
  styleUrl: "./adult-pin-modal.component.css",
})
export class AdultPinModalComponent {
  @ViewChild("pinInput") pinInput!: ElementRef;
  pin: string = "";
  loading: boolean = false;
  incorrect: boolean = false;

  constructor(
    public activeModal: NgbActiveModal,
    private error: ErrorService,
  ) {}

  async unlock() {
    if (!this.pin) return;
    this.loading = true;
    this.incorrect = false;
    try {
      const correct = await invoke("verify_adult_pin", { pin: this.pin });
      if (correct) {
        this.activeModal.close(true);
      } else {
        this.incorrect = true;
        this.pin = "";
        this.pinInput?.nativeElement?.focus();
      }
    } catch (e) {
      this.error.handleError(e, "Failed to verify PIN");
    }
    this.loading = false;
  }
}
