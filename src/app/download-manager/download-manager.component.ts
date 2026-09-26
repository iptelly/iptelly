import { Component, OnInit } from "@angular/core";
import { DownloadService } from "../download.service";
import { Download } from "../models/download";
import { DownloadHistoryItem } from "../models/downloadHistoryItem";
import { invoke } from "@tauri-apps/api/core";
import { Channel } from "../models/channel";
import { ErrorService } from "../error.service";
import { formatBytes } from "../utils";

@Component({
  selector: "app-download-manager",
  templateUrl: "./download-manager.component.html",
  styleUrl: "./download-manager.component.css",
})
export class DownloadManagerComponent implements OnInit {
  constructor(
    public downloadService: DownloadService,
    private error: ErrorService,
  ) {}

  ngOnInit(): void {
    this.downloadService.refreshHistory();
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
