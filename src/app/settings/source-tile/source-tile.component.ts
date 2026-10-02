import { Component, ElementRef, Input } from "@angular/core";
import { NgbModal } from "@ng-bootstrap/ng-bootstrap";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { EditChannelModalComponent } from "../../edit-channel-modal/edit-channel-modal.component";
import { EditGroupModalComponent } from "../../edit-group-modal/edit-group-modal.component";
import { ImportModalComponent } from "../../import-modal/import-modal.component";
import { MemoryService } from "../../memory.service";
import { CHANNEL_EXTENSION, FAVS_BACKUP, PLAYLIST_EXTENSION } from "../../models/extensions";
import { Source } from "../../models/source";
import { SourceType } from "../../models/sourceType";
import { sanitizeFileName } from "../../utils";

@Component({
  selector: "app-source-tile",
  templateUrl: "./source-tile.component.html",
  styleUrl: "./source-tile.component.css",
})
export class SourceTileComponent {
  @Input("source")
  source?: Source;
  @Input("expiry")
  expiry?: number;
  @Input("timezone")
  timezone?: string;
  showUsername = false;
  showPassword = false;
  loading = false;
  sourceTypeEnum = SourceType;
  editing = false;
  editableSource: Source = {};
  defaultUserAgent = "IPTelly";

  constructor(
    public memory: MemoryService,
    private modal: NgbModal,
    private el: ElementRef,
  ) {}

  get_source_type_name() {
    if (!this.source) return null;
    return SourceType[this.source.source_type!];
  }

  async refresh() {
    if (this.source?.source_type == SourceType.Xtream) this.memory.SeriesRefreshed.clear();
    const failed = await this.memory.tryIPC(
      "Successfully updated source",
      "Failed to refresh source",
      () => invoke("refresh_source", { source: this.source }),
    );
    // refresh_source only returns success/failure, not the updated row, so
    // the "Refreshed:" timestamp bound to this.source would otherwise stay
    // stale (still whatever get_sources() returned on page load) until a
    // full reload re-fetches sources from the DB.
    if (!failed && this.source) this.source.last_updated = Math.floor(Date.now() / 1000);
  }

  async refreshEpg() {
    const command = this.source?.epg_url ? "refresh_epg_only" : "refresh_xtream_epg_only";
    await this.memory.tryIPC("Successfully refreshed EPG", "Failed to refresh EPG", () =>
      invoke(command, { source: this.source }),
    );
  }

  // Only shown when a custom epg_url is set, since in that case refreshEpg()
  // above refreshes the custom guide instead - this is the one way to force
  // a re-download of the provider's own built-in xmltv.php without having
  // to temporarily clear the custom URL. refresh_xtream_epg_only already
  // wipes this source's stored EPG before re-inserting (see
  // refresh_epg_from_url), so there's no separate wipe step needed here.
  async refreshProviderEpg() {
    await this.memory.tryIPC(
      "Successfully refreshed provider EPG",
      "Failed to refresh provider EPG",
      () => invoke("refresh_xtream_epg_only", { source: this.source }),
    );
  }

  async pruneEpg() {
    await this.memory.tryIPC(
      "Successfully cleared old EPG data",
      "Failed to clear old EPG data",
      () => invoke("prune_old_epg", { source: this.source }),
    );
  }

  async delete() {
    await this.memory.tryIPC("Successfully deleted source", "Failed to delete source", () =>
      invoke("delete_source", { id: this.source?.id }),
    );
    this.memory.RefreshSources.next(true);
  }

  async toggleEnabled() {
    await this.memory.tryIPC("Successfully toggled source", "Failed to toggle source", () =>
      invoke("toggle_source", { value: !this.source?.enabled, sourceId: this.source?.id }),
    );
    this.memory.RefreshSources.next(true);
    // The enabled/disabled variant is swapped via *ngIf, so the button just
    // clicked/activated is always a new DOM node - re-focus its replacement
    // so keyboard users land back where they were instead of losing focus.
    setTimeout(() => {
      const btn: HTMLButtonElement | null = this.el.nativeElement.querySelector(".check-btn");
      btn?.focus();
    });
  }

  async addCustomChannel() {
    this.memory.ModalRef = this.modal.open(EditChannelModalComponent, {
      backdrop: "static",
      size: "xl",
      keyboard: false,
    });
    this.memory.ModalRef.result.then((_) => (this.memory.ModalRef = undefined));
    this.memory.ModalRef.componentInstance.name = "EditCustomChannelModal";
    this.memory.ModalRef.componentInstance.channel.data.source_id = this.source?.id;
  }

  async addCustomGroup() {
    this.memory.ModalRef = this.modal.open(EditGroupModalComponent, {
      backdrop: "static",
      size: "xl",
      keyboard: false,
    });
    this.memory.ModalRef.result.then((_) => (this.memory.ModalRef = undefined));
    this.memory.ModalRef.componentInstance.name = "EditCustomGroupModal";
    this.memory.ModalRef.componentInstance.group.source_id = this.source?.id;
  }

  async import() {
    this.memory.ModalRef = this.modal.open(ImportModalComponent, {
      backdrop: "static",
      size: "xl",
      keyboard: false,
    });
    this.memory.ModalRef.result.then((_) => (this.memory.ModalRef = undefined));
    this.memory.ModalRef.componentInstance.name = "ImportModalComponent";
    this.memory.ModalRef.componentInstance.source_id = this.source?.id;
  }

  async share() {
    const file = await save({
      canCreateDirectories: true,
      title: "Select where to export custom source",
      defaultPath: sanitizeFileName(this.source?.name!) + PLAYLIST_EXTENSION,
    });
    if (file) {
      await this.memory.tryIPC(
        `Successfully exported source in ${file}`,
        "Failed to export source",
        () => invoke("share_custom_source", { source: this.source, path: file }),
      );
    }
  }

  edit() {
    this.editableSource = { ...this.source };
    this.editing = true;
    // The url field only exists in the DOM once *ngIf picks up `editing` on
    // the next change detection pass, hence the setTimeout - same pattern
    // used for post-render focus elsewhere (e.g. home.component.ts).
    setTimeout(() => {
      const input: HTMLInputElement | null =
        this.el.nativeElement.querySelector('input[name="url"]');
      if (input) {
        input.focus();
        return;
      }
      // M3U sources have no typeable url once editing - just the read-only
      // path and a "Browse" button, which is the equivalent next action.
      const browseBtn: HTMLButtonElement | null =
        this.el.nativeElement.querySelector(".browse-btn");
      browseBtn?.focus();
    });
  }

  async save() {
    await this.memory.tryIPC("Successfully saved changes", "Failed to save changes", async () => {
      this.editableSource.user_agent = this.editableSource.user_agent?.trim();
      this.editableSource.stream_user_agent = this.editableSource.stream_user_agent?.trim();
      this.editableSource.epg_url = this.editableSource.epg_url?.trim();
      if (this.editableSource.user_agent == "") this.editableSource.user_agent = undefined;
      if (this.editableSource.stream_user_agent == "")
        this.editableSource.stream_user_agent = undefined;
      if (this.editableSource.epg_url == "") this.editableSource.epg_url = undefined;
      if (!this.editableSource.epg_retention_days || this.editableSource.epg_retention_days < 1)
        this.editableSource.epg_retention_days = undefined;
      await invoke("update_source", { source: this.editableSource });
      this.source = this.editableSource;
      this.editing = false;
      this.editableSource = {};
    });
  }

  async browse() {
    const file = await open({
      multiple: false,
      directory: false,
      title: "Select a new m3u file for source",
      filters: [{ name: "extension", extensions: ["m3u", "m3u8"] }],
    });
    if (file) {
      this.editableSource.url = file;
    }
  }

  cancel() {
    this.editableSource = {};
    this.editing = false;
  }

  async backupFavs() {
    const file = await save({
      canCreateDirectories: true,
      title: "Select where to save favorites",
      defaultPath: `${sanitizeFileName(this.source?.name!)}_favs${FAVS_BACKUP}`,
    });
    if (file) {
      await this.memory.tryIPC(
        "Successfully saved favorites backup",
        "Failed to save favorites backup",
        async () => {
          await invoke("backup_favs", { id: this.source?.id, path: file });
        },
      );
    }
  }

  async restoreFavs() {
    const file = await open({
      canCreateDirectories: false,
      title: "Select a favorites backup",
      directory: false,
      multiple: false,
      filters: [{ name: "extension", extensions: ["otvf"] }],
    });
    if (file) {
      await this.memory.tryIPC(
        "Successfully saved favorites backup",
        "Failed to save favorites backup",
        async () => {
          await invoke("restore_favs", { id: this.source?.id, path: file });
        },
      );
    }
  }
}
