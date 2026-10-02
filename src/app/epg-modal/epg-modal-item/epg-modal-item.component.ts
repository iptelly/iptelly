import { Component, Input, NgZone, OnDestroy } from "@angular/core";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { Subscription, take } from "rxjs";
import { DownloadService } from "../../download.service";
import { ErrorService } from "../../error.service";
import { MemoryService } from "../../memory.service";
import { Channel } from "../../models/channel";
import { Download } from "../../models/download";
import { EPG } from "../../models/epg";
import { EPGNotify } from "../../models/epgNotify";
import { MediaType } from "../../models/mediaType";
import { getDateFormatted, getExtension, sanitizeFileName } from "../../utils";

@Component({
  selector: "app-epg-modal-item",
  templateUrl: "./epg-modal-item.component.html",
  styleUrl: "./epg-modal-item.component.css",
})
export class EpgModalItemComponent implements OnDestroy {
  constructor(
    public memory: MemoryService,
    private error: ErrorService,
    private download: DownloadService,
    private ngZone: NgZone,
  ) {}
  @Input()
  epg?: EPG;
  @Input()
  name?: string;
  @Input()
  sourceId?: number;
  @Input()
  channelId?: number;
  playing: boolean = false;
  progress: number = 0;
  subscriptions: Subscription[] = [];

  ngAfterViewInit(): void {
    const download = this.download.Downloads.get(this.getDownloadId());
    if (download) {
      this.downloadSubscribe(download);
    }
  }

  notificationOn(): boolean {
    return this.memory.Watched_epgs.has(this.epg!.epg_id);
  }

  async toggleNotification() {
    if (this.memory.LoadingNotification || !this.memory.trayEnabled) return;
    this.memory.LoadingNotification = true;
    if (!this.notificationOn()) {
      try {
        await invoke("add_epg", { epg: this.epg_to_epgNotify(this.epg!) });
        this.error.success("Added notification successfully");
      } catch (e) {
        this.error.handleError(e);
      }
    } else {
      try {
        await invoke("remove_epg", { epgId: this.epg?.epg_id });
        this.error.success("Removed notification successfully");
      } catch (e) {
        this.error.handleError(e);
      }
    }
    await this.memory.get_epg_ids();
    this.memory.LoadingNotification = false;
  }

  epg_to_epgNotify(epg: EPG): EPGNotify {
    return {
      channel_name: this.name!,
      epg_id: epg.epg_id,
      start_timestamp: epg.start_timestamp,
      title: epg.title,
    };
  }

  async timeshift() {
    if (this.playing) return;
    this.playing = true;
    const channel: Channel = {
      id: -1,
      url: this.epg?.timeshift_url,
      name: this.epg?.title,
      media_type: MediaType.movie,
      favorite: false,
      source_id: this.sourceId,
      is_adult: false,
    };
    try {
      await invoke("play", {
        channel: channel,
        record: false,
      });
    } catch (e) {
      console.error(e);
      this.error.handleError(e);
    } finally {
      this.playing = false;
    }
  }

  downloading() {
    return this.download.Downloads.has(this.getDownloadId());
  }

  getDownloadId() {
    return `${this.channelId}-${this.epg?.epg_id}`;
  }

  async downloadTimeshift() {
    let file;
    if (this.memory.IsContainer || this.memory.AlwaysAskSave) {
      file = await save({
        canCreateDirectories: true,
        title: "Select where to save catchback",
        defaultPath: `${sanitizeFileName(this.epg?.title!)}_${getDateFormatted()}.${getExtension(this.epg?.timeshift_url!)}`,
      });
      if (!file) {
        return;
      }
    }
    if (this.downloading()) return;
    const channel: Channel = {
      id: this.channelId,
      url: this.epg?.timeshift_url,
      name: this.epg?.title,
      media_type: MediaType.movie,
      favorite: false,
      source_id: this.sourceId,
      is_adult: false,
    };
    const download = await this.download.addDownload(this.getDownloadId(), channel);
    this.downloadSubscribe(download);
    await this.download.download(download.id, file);
  }

  downloadSubscribe(download: Download) {
    const progressUpdate = download.progressUpdate.subscribe((progress) => {
      this.ngZone.run(() => {
        this.progress = Math.trunc(progress);
      });
    });
    this.subscriptions.push(progressUpdate);
    this.subscriptions.push(
      download.complete.pipe(take(1)).subscribe((_) => {
        progressUpdate.unsubscribe();
        this.progress = 0;
      }),
    );
  }

  async cancelTimeshiftDownload() {
    await this.download.abortDownload(this.getDownloadId());
  }

  ngOnDestroy() {
    this.subscriptions.forEach((x) => x.unsubscribe());
  }
}
