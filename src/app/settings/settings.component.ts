import { Component, ElementRef, HostListener, ViewChild } from "@angular/core";
import { debounceTime, distinctUntilChanged, fromEvent, map, Subscription } from "rxjs";
import { Settings } from "../models/settings";
import { invoke } from "@tauri-apps/api/core";
import { Router } from "@angular/router";
import { open } from "@tauri-apps/plugin-dialog";
import { Source } from "../models/source";
import { MemoryService } from "../memory.service";
import { NgbModal } from "@ng-bootstrap/ng-bootstrap";
import { ConfirmDeleteModalComponent } from "../confirm-delete-modal/confirm-delete-modal.component";
import { SORT_TYPES, SortType, getSortTypeText } from "../models/sortType";
import { RailItem } from "../models/railItem";
import { ThemeService } from "../theme.service";
import { NetworkInterface } from "../models/networkInterface";
import { ErrorService } from "../error.service";
import { ToastrService } from "ngx-toastr";
import { AdultPinModalComponent } from "../adult-pin-modal/adult-pin-modal.component";
import { NavRailComponent } from "../home/nav-rail/nav-rail.component";

@Component({
  selector: "app-settings",
  templateUrl: "./settings.component.html",
  styleUrl: "./settings.component.css",
})
export class SettingsComponent {
  readonly railItemEnum = RailItem;
  subscriptions: Subscription[] = [];
  settings: Settings = {
    use_stream_caching: true,
    default_view: RailItem.Channels,
    volume: 100,
    restream_port: 3000,
    enable_tray_icon: true,
    zoom: 100,
    default_sort: SortType.provider,
    enable_hwdec: true,
    always_ask_save: false,
    enable_gpu: false,
  };
  sources: Source[] = [];
  expiries: Record<number, number> = {};
  timezones: Record<number, string> = {};
  networkInterfaces: NetworkInterface[] = [];
  sortTypes = SORT_TYPES;
  @ViewChild("mpvParams") mpvParams!: ElementRef;
  @ViewChild("vlcParams") vlcParams!: ElementRef;
  @ViewChild(NavRailComponent) navRail?: NavRailComponent;

  newAdultPin: string = "";
  confirmAdultPin: string = "";
  adultPinError: string = "";

  constructor(
    private router: Router,
    public memory: MemoryService,
    private nav: Router,
    private modal: NgbModal,
    private theme: ThemeService,
    private error: ErrorService,
    private toastr: ToastrService,
    private el: ElementRef,
  ) { }

  // Applies immediately (no reload needed) in addition to persisting -
  // AppComponent only reads this once at startup.
  onThemeChange() {
    this.theme.applyTheme(this.settings.theme);
    this.updateSettings();
  }

  // Applies immediately (no reload needed) in addition to persisting -
  // channel-tile/home only read memory.LightweightMode, populated once at
  // startup, so changing it from here must also update memory directly.
  onLightweightModeChange() {
    this.memory.LightweightMode = this.settings.lightweight_mode ?? false;
    this.updateSettings();
  }

  _getSortTypeText(sortType: SortType) {
    return getSortTypeText(sortType);
  }

  isInputFocused(): boolean {
    const activeElement = document.activeElement;
    return (
      activeElement instanceof HTMLInputElement ||
      activeElement instanceof HTMLTextAreaElement ||
      activeElement instanceof HTMLSelectElement
    );
  }

  @HostListener("document:keydown", ["$event"])
  onKeyDown(event: KeyboardEvent) {
    if (
      event.key == "Escape" ||
      event.key == "BrowserBack" ||
      (event.key == "Backspace" && !this.isInputFocused())
    ) {
      if (this.memory.ModalRef) {
        this.memory.ModalRef.close("close");
      } else {
        this.goBack();
      }
      event.preventDefault();
      return;
    }
    // Bail before touching the DOM at all for every key that isn't one of
    // these - otherwise this handler was re-scanning the whole page on
    // every keystroke, including normal typing into any of this page's many
    // text fields.
    if (event.key != "Tab" && event.key != "ArrowUp" && event.key != "ArrowDown") return;
    if (this.memory.ModalRef) return; // let an open modal handle its own keyboard input
    const inNavRail = document.activeElement?.closest(".nav-rail") != null;
    // Tab still crosses the nav rail <-> page content boundary (mirroring
    // ManageCategoriesComponent/HomeComponent's Tab-between-regions
    // convention) - Up/Down (below) is what moves *within* the content
    // region, which on this page is everything from "Recording path" down
    // through every source tile's fields and buttons.
    if (event.key == "Tab") {
      const rows = this.focusableRows();
      if (inNavRail && !event.shiftKey) {
        event.preventDefault();
        rows[0]?.focus();
      } else if (!inNavRail && event.shiftKey && document.activeElement == rows[0]) {
        event.preventDefault();
        this.navRail?.focusFirstRow();
      }
      return;
    }
    if (inNavRail) return; // NavRail owns its own Up/Down
    event.preventDefault();
    const rows = this.focusableRows();
    const current = rows.indexOf(document.activeElement as HTMLElement);
    const next = Math.max(0, Math.min(rows.length - 1, current + (event.key == "ArrowDown" ? 1 : -1)));
    rows[next]?.focus();
  }

  private focusableRows(): HTMLElement[] {
    const root: HTMLElement | null = this.el.nativeElement.querySelector(".page-wrapper");
    if (!root) return [];
    const rows: HTMLElement[] = Array.from(
      root.querySelectorAll('select, input, textarea, button, [tabindex="0"]'),
    );
    return rows.filter((el) => !(el as HTMLButtonElement).disabled);
  }

  ngOnInit(): void {
    this.getSettings();
    this.getSources();
    this.getNetworkInterfaces();
    this.getHasAdultPin();
    if (this.memory.XtreamSourceIds.size > 0) {
      this.getExpiries();
      this.getTimezones();
    }
  }

  // memory.AdultPinSet is otherwise only ever populated by HomeComponent's
  // own startup fetch - fine normally, since Settings is only ever reached
  // by clicking through from Home, but a dev-mode full page reload (or any
  // future direct navigation to this route) lands here with it still at its
  // default `false`, incorrectly showing the "set a new PIN" form even when
  // one is already set.
  getHasAdultPin() {
    invoke("has_adult_pin").then((x) => {
      this.memory.AdultPinSet = x as boolean;
    });
  }

  getNetworkInterfaces() {
    invoke("get_network_interfaces").then((x) => {
      this.networkInterfaces = x as NetworkInterface[];
    });
  }

  isSelectedInterfaceMissing(): boolean {
    if (this.settings.network_interface == undefined) return false;
    return !this.networkInterfaces.some((i) => i.ip === this.settings.network_interface);
  }

  getSettings() {
    invoke("get_settings").then((x) => {
      this.settings = x as Settings;
      if (this.settings.use_stream_caching == undefined) this.settings.use_stream_caching = true;
      if (this.settings.default_view == undefined) this.settings.default_view = RailItem.Channels;
      if (this.settings.volume == undefined) this.settings.volume = 100;
      if (this.settings.restream_port == undefined) this.settings.restream_port = 3000;
      if (this.settings.enable_tray_icon == undefined) this.settings.enable_tray_icon = true;
      if (this.settings.zoom == undefined) this.settings.zoom = 100;
      if (this.settings.default_sort == undefined) this.settings.default_sort = SortType.provider;
      if (this.settings.enable_hwdec == undefined) this.settings.enable_hwdec = true;
      if (this.settings.always_ask_save == undefined) this.settings.always_ask_save = false;
      if (this.settings.enable_gpu == undefined) this.settings.enable_gpu = false;
      if (this.settings.theme == undefined) this.settings.theme = "modern";
      if (this.settings.player == undefined) this.settings.player = "mpv";
      if (this.settings.lightweight_mode == undefined) this.settings.lightweight_mode = false;
    });
  }

  getSources() {
    invoke("get_sources").then((x) => {
      this.sources = x as Source[];
      if (this.sources.length == 0) {
        this.memory.AddingAdditionalSource = false;
        this.nav.navigateByUrl("setup");
      }
    });
  }

  // Without this, every refresh (e.g. toggling a source's enabled state)
  // replaces `sources` with all-new object references, and NgFor's default
  // identity-based diffing then destroys and recreates every
  // <app-source-tile> - silently dropping keyboard focus back to <body>
  // even though the user never left the page.
  trackSourceById(_index: number, source: Source): number | undefined {
    return source.id;
  }

  getExpiries() {
    invoke("get_all_expiries").then(expiries => {
      this.expiries = expiries as Record<number, number>;
    });
  }

  getTimezones() {
    invoke("get_all_timezones").then(timezones => {
      this.timezones = timezones as Record<number, string>;
    });
  }

  ngAfterViewInit(): void {
    this.subscriptions.push(
      fromEvent(this.mpvParams.nativeElement, "keyup")
        .pipe(
          map((event: any) => {
            return event.target.value;
          }),
          debounceTime(500),
          distinctUntilChanged(),
        )
        .subscribe(async () => {
          await this.updateSettings();
        }),
    );
    this.subscriptions.push(
      fromEvent(this.vlcParams.nativeElement, "keyup")
        .pipe(
          map((event: any) => {
            return event.target.value;
          }),
          debounceTime(500),
          distinctUntilChanged(),
        )
        .subscribe(async () => {
          await this.updateSettings();
        }),
    );
    this.subscriptions.push(
      this.memory.RefreshSources.subscribe((_) => {
        this.getSources();
      }),
    );
  }

  addSource() {
    this.memory.AddingAdditionalSource = true;
    this.nav.navigateByUrl("setup");
  }

  async refreshAll() {
    this.memory.SeriesRefreshed.clear();
    const failed = await this.memory.tryIPC(
      "Successfully updated all sources",
      "Failed to refresh sources",
      () => invoke("refresh_all"),
    );
    // Same staleness issue as source-tile.component.ts's refresh() - unlike
    // that one, this touches every source at once rather than one known
    // row, so a full re-fetch is simpler than guessing each timestamp.
    if (!failed) this.getSources();
  }

  async goBack() {
    await this.updateSettings();
    this.router.navigateByUrl("");
  }

  // Mirrors manage-categories.component.ts's selectRail() - same rail,
  // same pattern for a route-based (non-home) page: Settings itself is a
  // no-op, Manage Categories is its own route, everything else goes back
  // to home with a query param it reads on init to pick the right section.
  async selectRail(item: RailItem) {
    if (item === RailItem.Settings) return;
    await this.updateSettings();
    if (item === RailItem.ManageCategories) {
      this.router.navigateByUrl("manage-categories");
      return;
    }
    this.router.navigate([""], { queryParams: { rail: item } });
  }

  async updateSettings() {
    this.settings.mpv_params = this.settings.mpv_params?.trim();
    if (this.settings.mpv_params == "")
      this.settings.mpv_params = undefined;
    this.settings.vlc_params = this.settings.vlc_params?.trim();
    if (this.settings.vlc_params == "")
      this.settings.vlc_params = undefined;
    await invoke("update_settings", { settings: this.settings });
  }

  async selectFolder() {
    const folder = await open({
      multiple: false,
      directory: true,
      canCreateDirectories: true,
    });
    if (folder) {
      this.settings.recording_path = folder;
      await this.updateSettings();
    }
  }

  async nuke() {
    this.memory.ModalRef = this.modal.open(ConfirmDeleteModalComponent, {
      backdrop: "static",
      size: "xl",
      keyboard: false,
    });
    this.memory.ModalRef.result.then((_) => (this.memory.ModalRef = undefined));
    this.memory.ModalRef.componentInstance.name = "ConfirmDeleteModal";
  }

  async setAdultPin() {
    this.adultPinError = "";
    if (this.newAdultPin.length < 4) {
      this.adultPinError = "PIN must be at least 4 digits.";
      return;
    }
    if (this.newAdultPin !== this.confirmAdultPin) {
      this.adultPinError = "PINs don't match.";
      return;
    }
    try {
      await invoke("set_adult_pin", { pin: this.newAdultPin });
      this.memory.AdultPinSet = true;
      this.newAdultPin = "";
      this.confirmAdultPin = "";
      this.toastr.success("Adult content PIN set");
    } catch (e) {
      this.error.handleError(e, "Failed to set PIN");
    }
  }

  // Requires re-entering the current PIN first - otherwise anyone at this
  // page could remove the lock without ever knowing it, defeating the
  // whole point of the feature.
  async removeAdultPin() {
    const modalRef = this.modal.open(AdultPinModalComponent, {
      backdrop: "static",
    });
    const unlocked = await modalRef.result.catch(() => false);
    if (!unlocked) return;
    this.memory.AdultContentUnlocked = true;
    try {
      await invoke("set_adult_pin", { pin: null });
      this.memory.AdultPinSet = false;
      this.memory.Refresh.next(false);
      this.toastr.success("Adult content PIN removed");
    } catch (e) {
      this.error.handleError(e, "Failed to remove PIN");
    }
  }

  async clearHistory() {
    await this.memory.tryIPC(
      "History cleared successfully",
      "Failed to clear history",
      async () => {
        await invoke("clear_history");
      },
    );
  }

  async clearEpgCache() {
    await this.memory.tryIPC(
      "EPG cache cleared successfully",
      "Failed to clear EPG cache",
      async () => {
        await invoke("clear_epg_cache");
      },
    );
  }

  ngOnDestroy(): void {
    this.subscriptions.forEach((x) => x.unsubscribe());
  }
}
