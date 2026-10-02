import { Component, ElementRef, HostListener, OnInit } from "@angular/core";
import { invoke } from "@tauri-apps/api/core";
import { DownloadService } from "../download.service";
import { ErrorService } from "../error.service";
import { Channel } from "../models/channel";
import { Download } from "../models/download";
import { DownloadHistoryItem } from "../models/downloadHistoryItem";
import { formatBytes } from "../utils";

@Component({
    selector: "app-download-manager",
    templateUrl: "./download-manager.component.html",
    styleUrl: "./download-manager.component.css",
    standalone: false
})
export class DownloadManagerComponent implements OnInit {
  constructor(
    public downloadService: DownloadService,
    private error: ErrorService,
    private el: ElementRef,
  ) {}

  ngOnInit(): void {
    this.downloadService.refreshHistory();
  }

  // Self-contained keyboard navigation, mirroring PlaylistSidebarComponent/
  // DownloadSidebarComponent - HomeComponent only needs to know how to
  // enter (focusFirstRow) this region. Only handles Up/Down (a flat walk
  // over every focusable action in the list, bulk buttons included) -
  // deliberately does NOT handle Enter here: the row buttons are a mix of
  // real <button> elements (already natively Enter/Space-activatable) and
  // custom SVG icons (which get their own explicit keydown.enter/space
  // handlers in the template, same fix as the EPG modal's buttons) - a
  // generic "call click() on whatever's focused" handler here would
  // double-fire the real buttons' native activation, same bug as before.
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
    return Array.from(this.el.nativeElement.querySelectorAll('[tabindex="0"]'));
  }

  focusFirstRow() {
    this.focusableRows()[0]?.focus();
  }

  get selectedCategory() {
    return this.downloadService.SelectedCategory;
  }

  getDownloads(): Download[] {
    return Array.from(this.downloadService.Downloads.values());
  }

  getCompleted(): DownloadHistoryItem[] {
    return this.downloadService.History.filter((h) => h.status === "completed");
  }

  // Also catches anything reconciled to 'paused' by a backend restart mid-
  // download (see reconcile_interrupted_downloads) - not "cancelled" in the
  // literal sense, but it's not active or completed either, and this is
  // where its already-working Resume button lives.
  getCancelled(): DownloadHistoryItem[] {
    return this.downloadService.History.filter(
      (h) => h.status === "cancelled" || h.status === "failed" || h.status === "paused",
    );
  }

  hasActive(): boolean {
    return this.downloadService.Downloads.size > 0;
  }

  formatBytes(bytes?: number): string {
    return formatBytes(bytes);
  }

  async cancelDownload(downloadId: string) {
    await this.downloadService.abortDownload(downloadId);
  }

  async pauseDownload(downloadId: string) {
    await this.downloadService.pauseDownload(downloadId);
  }

  async resumeDownload(download: Download) {
    await this.downloadService.resumeDownload(download.id, download.channel);
  }

  async resumeHistoryItem(item: DownloadHistoryItem) {
    if (!item.channel_id) {
      this.error.handleError(undefined, "Can't resume - this download has no channel reference");
      return;
    }
    try {
      const channel = await invoke<Channel>("get_channel_by_id", { id: item.channel_id });
      await this.downloadService.resumeDownload(item.id, channel);
    } catch (e) {
      this.error.handleError(e);
    }
  }

  // Sequential (not parallel) - same reason series/season batches already
  // download one at a time: most sources default to max_streams=1, and
  // starting several at once would just cancel each other out.
  async restartAllCancelled() {
    for (const item of this.getCancelled()) {
      await this.resumeHistoryItem(item);
    }
  }

  async pauseAll() {
    await this.downloadService.pauseAll();
  }

  async cancelAll() {
    await this.downloadService.cancelAll();
  }

  async playHistoryItem(item: DownloadHistoryItem) {
    try {
      await invoke("play_download", { path: item.path });
    } catch (e) {
      this.error.handleError(e);
    }
  }

  async deleteHistoryItem(item: DownloadHistoryItem) {
    await this.downloadService.deleteHistoryItem(item.id);
  }

  async deleteDownloadedFile(item: DownloadHistoryItem) {
    await this.downloadService.deleteDownloadedFile(item.id);
  }

  async clearCancelled() {
    await this.downloadService.clearCancelled();
  }
}
