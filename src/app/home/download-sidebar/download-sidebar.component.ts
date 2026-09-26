import { Component } from "@angular/core";
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

  constructor(public downloadService: DownloadService) {}

  select(category: DownloadCategory) {
    this.downloadService.SelectedCategory = category;
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
