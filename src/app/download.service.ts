import { Injectable, NgZone } from "@angular/core";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Subject } from "rxjs";
import { ErrorService } from "./error.service";
import { Channel } from "./models/channel";
import { Download } from "./models/download";
import { DownloadHistoryItem } from "./models/downloadHistoryItem";
import { DownloadProgress } from "./models/downloadProgress";
import { getExtension, sanitizeFileName } from "./utils";

// A "Download Series"/"Download Season" batch - owns its own sequential
// queue (see enqueueSeries below) so cancelAll() can actually stop it
// between episodes, not just cancel whichever one is in flight. Pausing
// doesn't need a batch-level flag: pauseAll() pauses whichever episode is
// currently active, and enqueueSeries already holds the queue at that
// episode (via its own status) until it's resumed or cancelled.
export interface SeriesBatch {
  id: string;
  cancelled: boolean;
}

export type DownloadCategory = "queued" | "completed" | "cancelled";

@Injectable({
  providedIn: "root",
})
export class DownloadService {
  Downloads: Map<string, Download> = new Map();
  Batches: Map<string, SeriesBatch> = new Map();
  History: DownloadHistoryItem[] = [];
  // Shared with download-sidebar.component.ts (a sibling, not a parent/child
  // of the Downloads tab's main panel) - simplest shared place for both to
  // read/write which category is currently selected.
  SelectedCategory: DownloadCategory = "queued";

  constructor(
    private error: ErrorService,
    private ngZone: NgZone,
  ) {
    this.refreshHistory();
  }

  refreshHistory() {
    invoke("get_download_history").then((x) => {
      this.ngZone.run(() => {
        this.History = x as DownloadHistoryItem[];
      });
    });
  }

  async deleteHistoryItem(id: string) {
    try {
      await invoke("delete_download_history_item", { downloadId: id });
      this.History = this.History.filter((h) => h.id !== id);
    } catch (e) {
      this.error.handleError(e);
    }
  }

  // For a completed entry specifically - also removes the actual file from
  // disk, unlike deleteHistoryItem above (used for cancelled/failed rows,
  // whose file is either already gone or was never created).
  async deleteDownloadedFile(id: string) {
    try {
      await invoke("delete_downloaded_file", { downloadId: id });
      this.History = this.History.filter((h) => h.id !== id);
    } catch (e) {
      this.error.handleError(e);
    }
  }

  async clearCancelled() {
    try {
      await invoke("clear_cancelled_downloads");
      this.History = this.History.filter(
        (h) => h.status !== "cancelled" && h.status !== "failed" && h.status !== "paused",
      );
    } catch (e) {
      this.error.handleError(e);
    }
  }

  async addDownload(
    id: string,
    channel: Channel,
    status: Download["status"] = "downloading",
  ): Promise<Download> {
    const download: Download = {
      channel: channel,
      progress: 0,
      complete: new Subject(),
      id: id,
      progressUpdate: new Subject(),
      status: status,
      downloadedBytes: 0,
      totalBytes: 0,
    };
    download.unlisten = await listen<DownloadProgress>(`progress-${download.id}`, (event) => {
      this.ngZone.run(() => {
        download.progress = event.payload.progress;
        download.downloadedBytes = event.payload.downloaded_bytes;
        download.totalBytes = event.payload.total_bytes;
      });
      download.progressUpdate.next(download.progress);
    });
    this.Downloads.set(download.id, download);
    return download;
  }

  async abortDownload(id: string) {
    try {
      const download = this.Downloads.get(id);
      await invoke("abort_download", { downloadId: id });
      if (download) this.deleteDownload(download);
    } catch (e) {
      console.error(e);
      this.error.handleError(e);
    }
    this.refreshHistory();
  }

  async pauseDownload(id: string) {
    try {
      await invoke("pause_download", { downloadId: id });
    } catch (e) {
      console.error(e);
      this.error.handleError(e);
    }
  }

  // The download it's resuming may no longer be in Downloads (e.g. after an
  // app restart) - re-registers it from the history item's channel info if
  // needed, same as a fresh download would.
  async resumeDownload(id: string, channel: Channel) {
    let download = this.Downloads.get(id);
    if (!download) download = await this.addDownload(id, channel);
    else download.status = "downloading";
    // Optimistic - History won't actually refresh from the database until
    // this whole attempt settles (could be a while for a large file), so
    // without this the item stays visible in Cancelled/Downloaded with its
    // old status for the entire redownload, appearing twice at once.
    this.History = this.History.filter((h) => h.id !== id);
    try {
      await invoke("resume_download", { downloadId: id, channel });
      this.error.success("Download completed successfully");
      this.deleteDownload(download);
    } catch (e) {
      this.handlePauseableOutcome(download, e);
    }
    this.refreshHistory();
  }

  async pauseAll() {
    try {
      await invoke("pause_all_downloads");
    } catch (e) {
      this.error.handleError(e);
    }
  }

  async cancelAll() {
    try {
      await invoke("cancel_all_downloads");
    } catch (e) {
      this.error.handleError(e);
    }
    for (const batch of this.Batches.values()) batch.cancelled = true;
  }

  // Single-file download (movie/episode, and each episode of a series/
  // season batch via enqueueSeries below). Stays in Downloads with
  // status 'paused' rather than being removed when paused, so it can be
  // resumed later - only removed from the active map on completion,
  // cancellation, or failure.
  async download(id: string, path?: string) {
    const download = this.Downloads.get(id)!;
    try {
      await invoke("download", {
        downloadId: download.id,
        channel: download.channel,
        path: path,
      });
      this.error.success("Download completed successfully");
      this.deleteDownload(download);
    } catch (e) {
      this.handlePauseableOutcome(download, e);
    }
    this.refreshHistory();
  }

  private handlePauseableOutcome(download: Download, e: unknown) {
    if (e == "download paused") {
      download.status = "paused";
      this.error.info("Download paused");
      return;
    }
    if (e == "download aborted") this.error.info("Download cancelled");
    else this.error.handleError(e);
    this.deleteDownload(download);
  }

  deleteDownload(download: Download) {
    download.complete.next(true);
    try {
      download.unlisten!();
    } catch (e) {
      console.error(e);
    }
    this.Downloads.delete(download.id);
  }

  // Registers every episode up front (status 'queued') rather than one at a
  // time as the loop reaches it, so the Downloads tab can show the whole
  // batch - and its individual progress bars - immediately, not just
  // whichever episode happens to be downloading right now.
  async enqueueSeries(
    baseFolder: string,
    showName: string,
    episodes: { channel: Channel; seasonName: string }[],
  ): Promise<void> {
    const batch: SeriesBatch = { id: crypto.randomUUID(), cancelled: false };
    this.Batches.set(batch.id, batch);
    const showFolder = sanitizeFileName(showName);
    const allEntries = episodes.map((episode) => {
      const seasonFolder = sanitizeFileName(episode.seasonName);
      const fileName = `${sanitizeFileName(episode.channel.name!)}.${getExtension(episode.channel.url!)}`;
      return {
        id: episode.channel.id!.toString(),
        channel: episode.channel,
        path: `${baseFolder}/${showFolder}/${seasonFolder}/${fileName}`,
      };
    });
    // Checked (and skipped) before anything is queued - queuing would
    // upsert a fresh 'queued' row over an already-'completed' one, which
    // is exactly the state this check is trying to leave alone.
    const entries = [];
    let skipped = 0;
    for (const entry of allEntries) {
      let alreadyDownloaded = false;
      try {
        alreadyDownloaded = await invoke<boolean>("is_already_downloaded", {
          channel: entry.channel,
          path: entry.path,
        });
      } catch (e) {
        console.error(e);
      }
      if (alreadyDownloaded) skipped++;
      else entries.push(entry);
    }
    if (skipped > 0) {
      this.error.info(`Skipped ${skipped} episode${skipped === 1 ? "" : "s"} already downloaded`);
    }
    for (const entry of entries) {
      await this.addDownload(entry.id, entry.channel, "queued");
      // Persisted immediately (not just held in memory) so an episode
      // cancelled while still queued - the common case when cancelling a
      // large batch partway through - still shows up in History as
      // cancelled instead of just disappearing with no trace.
      try {
        await invoke("queue_download", {
          downloadId: entry.id,
          channel: entry.channel,
          path: entry.path,
        });
      } catch (e) {
        console.error(e);
      }
    }
    try {
      for (const entry of entries) {
        if (!this.Downloads.has(entry.id)) continue; // cancelled individually while queued
        if (batch.cancelled) {
          // The whole batch was cancelled - mark the rest of the still-
          // queued entries cancelled too (via the real backend path, same
          // as an individual cancel), not just cleared from the active
          // list - each one already has its own persisted 'queued' row
          // from the loop above, which needs updating to 'cancelled' or it
          // never shows up in History at all, just vanishes.
          await this.abortDownload(entry.id);
          continue;
        }
        const download = this.Downloads.get(entry.id)!;
        download.status = "downloading";
        await this.download(entry.id, entry.path);
        // If paused mid-transfer (individually, or via pauseAll()), the
        // queue holds here rather than skipping ahead to the next episode -
        // it resumes on its own once the user resumes this one from the
        // Downloads tab, or stops if it's cancelled while paused instead.
        // Cast: TS narrows download.status as still "downloading" here since
        // nothing it can see reassigns it between the two lines above - it
        // doesn't account for the mutation happening inside the awaited
        // this.download() call via the same shared Download object.
        while ((download.status as Download["status"]) === "paused") {
          if (!this.Downloads.has(entry.id)) break; // cancelled while paused
          await new Promise((resolve) => setTimeout(resolve, 300));
        }
      }
    } finally {
      this.Batches.delete(batch.id);
    }
  }
}
