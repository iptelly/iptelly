import { Component, EventEmitter, Input, Output } from "@angular/core";
import { invoke } from "@tauri-apps/api/core";
import { NgbModal } from "@ng-bootstrap/ng-bootstrap";
import { RailItem } from "../../models/railItem";
import { MemoryService } from "../../memory.service";
import { ErrorService } from "../../error.service";
import { AdultPinModalComponent } from "../../adult-pin-modal/adult-pin-modal.component";

@Component({
  selector: "app-nav-rail",
  templateUrl: "./nav-rail.component.html",
  styleUrl: "./nav-rail.component.css",
})
export class NavRailComponent {
  readonly railItemEnum = RailItem;
  @Input() active: RailItem = RailItem.Channels;
  @Output() select = new EventEmitter<RailItem>();

  items = [
    { item: RailItem.Favourites, label: "Favourites" },
    { item: RailItem.Channels, label: "Channels" },
    { item: RailItem.Movies, label: "Movies" },
    { item: RailItem.Series, label: "Series" },
    { item: RailItem.History, label: "History" },
    { item: RailItem.Downloads, label: "Downloads" },
    { item: RailItem.ManageCategories, label: "Manage Categories" },
  ];

  constructor(
    public memory: MemoryService,
    private modal: NgbModal,
    private error: ErrorService,
  ) {}

  click(item: RailItem) {
    this.select.emit(item);
  }

  // Re-locking never needs the PIN (only unlocking does) - clicking an
  // already-unlocked icon just immediately hides adult content again for
  // the rest of this session, mirroring apps' usual "lock" affordance.
  async toggleAdultLock(event: Event) {
    event.stopPropagation();
    if (this.memory.AdultContentUnlocked) {
      try {
        await invoke("lock_adult_content");
        this.memory.AdultContentUnlocked = false;
        this.memory.Refresh.next(false);
      } catch (e) {
        this.error.handleError(e, "Failed to lock adult content");
      }
      return;
    }
    const modalRef = this.modal.open(AdultPinModalComponent, {
      backdrop: "static",
    });
    const unlocked = await modalRef.result.catch(() => false);
    if (unlocked) {
      this.memory.AdultContentUnlocked = true;
      this.memory.Refresh.next(false);
    }
  }
}
