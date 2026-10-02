import { Component, ElementRef, EventEmitter, HostListener, Input, Output } from "@angular/core";
import { NgbModal } from "@ng-bootstrap/ng-bootstrap";
import { invoke } from "@tauri-apps/api/core";
import { AdultPinModalComponent } from "../../adult-pin-modal/adult-pin-modal.component";
import { ErrorService } from "../../error.service";
import { MemoryService } from "../../memory.service";
import { RailItem } from "../../models/railItem";

@Component({
    selector: "app-nav-rail",
    templateUrl: "./nav-rail.component.html",
    styleUrl: "./nav-rail.component.css",
    standalone: false
})
export class NavRailComponent {
  readonly railItemEnum = RailItem;
  @Input() active: RailItem = RailItem.Channels;
  @Output() select = new EventEmitter<RailItem>();

  // Self-contained keyboard navigation, mirroring PlaylistSidebarComponent/
  // DownloadSidebarComponent - this rail is embedded on several routed
  // pages (Home, Settings, Manage Categories) that otherwise share nothing,
  // so Up/Down has to work here rather than depending on HomeComponent's
  // focusArea machinery, which only exists on the Home route.
  @HostListener("keydown", ["$event"])
  onKeyDown(event: KeyboardEvent) {
    if (event.key != "ArrowUp" && event.key != "ArrowDown") return;
    event.preventDefault();
    const rows = this.focusableRows();
    const current = rows.indexOf(document.activeElement as HTMLElement);
    const next = Math.max(
      0,
      Math.min(rows.length - 1, current + (event.key == "ArrowDown" ? 1 : -1)),
    );
    rows[next]?.focus();
  }

  private focusableRows(): HTMLElement[] {
    return Array.from(this.el.nativeElement.querySelectorAll(".rail-item"));
  }

  focusFirstRow() {
    this.focusableRows()[0]?.focus();
  }

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
    private el: ElementRef,
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
