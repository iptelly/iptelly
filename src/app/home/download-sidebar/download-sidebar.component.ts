import { Component, ElementRef, HostListener } from "@angular/core";
import { DownloadCategory, DownloadService } from "../../download.service";

@Component({
  selector: "app-download-sidebar",
  templateUrl: "./download-sidebar.component.html",
  styleUrl: "./download-sidebar.component.css",
})
export class DownloadSidebarComponent {
  readonly categories: { value: DownloadCategory; label: string }[] = [
    { value: "queued", label: "Queued" },
    { value: "completed", label: "Downloaded" },
    { value: "cancelled", label: "Cancelled" },
  ];

  constructor(
    public downloadService: DownloadService,
    private el: ElementRef,
  ) {}

  select(category: DownloadCategory) {
    this.downloadService.SelectedCategory = category;
  }

  // Self-contained keyboard navigation, mirroring PlaylistSidebarComponent -
  // HomeComponent only needs to know how to enter (focusFirstRow) this
  // region; movement within it is handled entirely here.
  @HostListener("keydown", ["$event"])
  onKeyDown(event: KeyboardEvent) {
    if (event.key == "Enter") {
      (document.activeElement as HTMLElement)?.click();
      return;
    }
    if (event.key != "ArrowUp" && event.key != "ArrowDown") return;
    event.preventDefault();
    const rows = this.focusableRows();
    const current = rows.indexOf(document.activeElement as HTMLElement);
    const next = Math.max(0, Math.min(rows.length - 1, current + (event.key == "ArrowDown" ? 1 : -1)));
    rows[next]?.focus();
  }

  private focusableRows(): HTMLElement[] {
    return Array.from(this.el.nativeElement.querySelectorAll('[tabindex="0"]'));
  }

  focusFirstRow() {
    this.focusableRows()[0]?.focus();
  }

  count(category: DownloadCategory): number {
    if (category === "queued") return this.downloadService.Downloads.size;
    return this.downloadService.History.filter((h) =>
      category === "completed"
        ? h.status === "completed"
        : h.status === "cancelled" || h.status === "failed" || h.status === "paused",
    ).length;
  }
}
