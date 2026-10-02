import { animate, state, style, transition, trigger } from "@angular/animations";
import { ListRange } from "@angular/cdk/collections";
import { CdkVirtualScrollViewport } from "@angular/cdk/scrolling";
import {
  AfterViewInit,
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  HostListener,
  NgZone,
  OnDestroy,
  ViewChild,
} from "@angular/core";
import { ActivatedRoute, Router } from "@angular/router";
import { NgbModal } from "@ng-bootstrap/ng-bootstrap";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { AllowIn, ShortcutInput } from "ng-keyboard-shortcuts";
import { ToastrService } from "ngx-toastr";
import {
  debounceTime,
  distinctUntilChanged,
  filter,
  fromEvent,
  map,
  Subscription,
  skip,
} from "rxjs";
import { DownloadManagerComponent } from "../download-manager/download-manager.component";
import { ErrorService } from "../error.service";
import { MemoryService } from "../memory.service";
import { BulkActionType } from "../models/bulkActionType";
import { Channel } from "../models/channel";
import { Filters } from "../models/filters";
import { FocusArea, FocusAreaPrefix } from "../models/focusArea";
import { LAST_SEEN_VERSION } from "../models/localStorage";
import { MediaType } from "../models/mediaType";
import { Node } from "../models/node";
import { NodeType } from "../models/nodeType";
import { RailItem } from "../models/railItem";
import { Settings } from "../models/settings";
import { SortType } from "../models/sortType";
import { Source } from "../models/source";
import { SourceType } from "../models/sourceType";
import { Stack } from "../models/stack";
import { ViewMode } from "../models/viewMode";
import { isInputFocused } from "../utils";
import { WhatsNewModalComponent } from "../whats-new-modal/whats-new-modal.component";
import { DownloadSidebarComponent } from "./download-sidebar/download-sidebar.component";
import { NavRailComponent } from "./nav-rail/nav-rail.component";
import { PlaylistSidebarComponent } from "./playlist-sidebar/playlist-sidebar.component";

@Component({
  selector: "app-home",
  templateUrl: "./home.component.html",
  styleUrl: "./home.component.css",
  animations: [
    trigger("fadeInOut", [
      transition(":enter", [
        style({ opacity: 0, height: 0, padding: "0", margin: "0" }),
        animate("250ms", style({ opacity: 1, height: "*", padding: "*", margin: "*" })),
      ]),
      transition(":leave", [
        style({ opacity: 1, height: "*", padding: "*", margin: "*" }),
        animate("250ms", style({ opacity: 0, height: 0, padding: "0", margin: "0" })),
      ]),
    ]),
    trigger("fade", [
      state(
        "visible",
        style({
          opacity: 1,
        }),
      ),
      state(
        "hidden",
        style({
          opacity: 0,
        }),
      ),
      transition("visible => hidden", [animate("250ms ease-out")]),
      transition("hidden => visible", [animate("250ms ease-in")]),
    ]),
  ],
  changeDetection: ChangeDetectionStrategy.Eager,
  standalone: false,
})
export class HomeComponent implements AfterViewInit, OnDestroy {
  channels: Channel[] = [];
  readonly viewModeEnum = ViewMode;
  bulkActionType = BulkActionType;
  readonly mediaTypeEnum = MediaType;
  readonly railItemEnum = RailItem;
  currentRailItem = RailItem.Channels;
  @ViewChild("search") search!: ElementRef;
  @ViewChild(NavRailComponent) navRail?: NavRailComponent;
  @ViewChild(PlaylistSidebarComponent) playlistSidebar?: PlaylistSidebarComponent;
  @ViewChild(DownloadSidebarComponent) downloadSidebar?: DownloadSidebarComponent;
  @ViewChild(DownloadManagerComponent) downloadManager?: DownloadManagerComponent;
  shortcuts: ShortcutInput[] = [];
  focus: number = 0;
  focusArea = FocusArea.Tiles;
  // Whether the keyboard cursor is currently in the Favourites page's
  // second grid (movies/series, showFavMediaSection()) rather than the
  // primary channels grid/list above it - both count as FocusArea.Tiles
  // for Tab-cycling purposes, this just tracks which of the two `focus`
  // is indexing into. Reset to false any time Tiles focus is freshly
  // (re)entered from elsewhere, so a stale value never survives leaving
  // Favourites or reloading.
  inFavMedia = false;
  // Index into preGridButtonIds() the keyboard cursor is currently on, or
  // -1 when it's indexing into a tile instead - also FocusArea.Tiles for
  // Tab-cycling purposes, and reset alongside inFavMedia for the same
  // reason. A plain index (not e.g. a single onBackButton boolean) since
  // more than one of these can be visible at once (e.g. History showing
  // both "Clear history" and, once you've drilled into a favourited
  // series from there, the back button too).
  preGridFocusIndex = -1;

  // Contextual buttons shown above the grid, in on-screen top-to-bottom
  // order - kept in one place so nav()'s pre-grid Up/Down handling and
  // focusPreGridOrTile()'s entry point never drift out of sync with
  // what's actually rendered there.
  preGridButtonIds(): string[] {
    const ids: string[] = [];
    if (this.showBackButton()) ids.push("back-button");
    if (this.currentRailItem === RailItem.History) ids.push("clear-history-button");
    return ids;
  }
  viewType = ViewMode.All;
  currentWindowSize: number = window.innerWidth;
  subscriptions: Subscription[] = [];
  filters?: Filters;
  chkLiveStream = true;
  chkMovie = true;
  chkSerie = true;
  reachedMax = false;
  readonly PAGE_SIZE = 36;
  channelsVisible = true;
  prevSearchValue: string = "";
  loading = false;
  nodeStack: Stack = new Stack();
  showScrollTop = false;

  // Not shown for a Category node - the playlist sidebar already shows
  // which category is selected and lets you pick another (or a different
  // source) to get back out, so a back button there is redundant. Series/
  // Season nesting has no sidebar equivalent to get back out with, so it
  // stays there.
  showBackButton(): boolean {
    return this.nodeStack.hasNodes() && this.nodeStack.get()?.type !== NodeType.Category;
  }

  private _viewport?: CdkVirtualScrollViewport;
  private viewportSubscriptions: Subscription[] = [];

  @ViewChild("virtualViewport")
  set viewport(vp: CdkVirtualScrollViewport | undefined) {
    this._viewport = vp;
    this.viewportSubscriptions.forEach((s) => s.unsubscribe());
    this.viewportSubscriptions = [];
    if (vp) {
      // CDK's own scroll listener already runs outside Angular's zone, but
      // RxJS re-enters whatever zone was active when .subscribe() itself was
      // called - so subscribing here (a normal Angular lifecycle context)
      // would still pull every single emission back into the zone and
      // trigger a full change-detection pass each time. These streams fire
      // very frequently during scroll, so this reintroduces the exact "100%
      // CPU while scrolling" problem the rAF-throttled window listener was
      // fixing earlier, just against a different scroll source. Only re-
      // enter the zone when a bound property actually needs to change.
      this.ngZone.runOutsideAngular(() => {
        this.viewportSubscriptions.push(
          vp.renderedRangeStream.subscribe((range) => {
            if (this.shouldLoadMoreFor(range)) {
              this.ngZone.run(() => this.loadMore());
            }
          }),
        );
        this.viewportSubscriptions.push(
          vp.elementScrolled().subscribe(() => {
            const shouldShow = (vp.measureScrollOffset("top") ?? 0) > 300;
            if (shouldShow !== this.showScrollTop) {
              this.ngZone.run(() => (this.showScrollTop = shouldShow));
            }
          }),
        );
      });
    }
  }
  get viewport(): CdkVirtualScrollViewport | undefined {
    return this._viewport;
  }

  scrollToTop() {
    this.viewport?.scrollToIndex(0, "smooth");
  }

  // Favourites splits into two independently paginated/virtualized
  // sections: the primary channels/filters/viewport above stays the
  // channels-with-EPG section, and this mirrors the same pattern for the
  // movies/series section shown below it. Kept as a parallel, separate set
  // of state (rather than generalizing the primary one) so the already
  // carefully-tuned primary list/keyboard-nav logic isn't put at risk.
  favMediaChannels: Channel[] = [];
  favMediaFilters?: Filters;
  favMediaReachedMax = false;
  favMediaLoading = false;

  private _favMediaViewport?: CdkVirtualScrollViewport;
  private favMediaViewportSubscriptions: Subscription[] = [];

  @ViewChild("favMediaViewport")
  set favMediaViewport(vp: CdkVirtualScrollViewport | undefined) {
    this._favMediaViewport = vp;
    this.favMediaViewportSubscriptions.forEach((s) => s.unsubscribe());
    this.favMediaViewportSubscriptions = [];
    if (vp) {
      this.ngZone.runOutsideAngular(() => {
        this.favMediaViewportSubscriptions.push(
          vp.renderedRangeStream.subscribe((range) => {
            if (this.shouldLoadMoreForFavMedia(range)) {
              this.ngZone.run(() => this.loadMoreFavMedia());
            }
          }),
        );
      });
    }
  }
  get favMediaViewport(): CdkVirtualScrollViewport | undefined {
    return this._favMediaViewport;
  }

  constructor(
    private router: Router,
    private route: ActivatedRoute,
    public memory: MemoryService,
    public toast: ToastrService,
    private error: ErrorService,
    private modal: NgbModal,
    private ngZone: NgZone,
  ) {
    this.getSources();
  }

  getSources() {
    const get_settings = invoke("get_settings");
    const get_sources = invoke("get_sources");
    const get_has_adult_pin = invoke("has_adult_pin");
    Promise.all([get_settings, get_sources, get_has_adult_pin])
      .then((data) => {
        const settings = data[0] as Settings;
        const sources = data[1] as Source[];
        if (settings.zoom) getCurrentWebview().setZoom(Math.trunc(settings.zoom! * 100) / 10000);
        this.memory.trayEnabled = settings.enable_tray_icon ?? true;
        this.memory.AlwaysAskSave = settings.always_ask_save ?? false;
        this.memory.LightweightMode = settings.lightweight_mode ?? false;
        this.memory.AdultPinSet = data[2] as boolean;
        this.memory.Sources = new Map(sources.filter((x) => x.enabled).map((s) => [s.id!, s]));
        if (sources.length == 0) this.reset();
        else {
          getVersion().then((version) => {
            if (localStorage.getItem(LAST_SEEN_VERSION) != version) {
              this.memory.AppVersion = version;
              this.memory.ModalRef = this.modal.open(WhatsNewModalComponent, {
                backdrop: "static",
                size: "xl",
                keyboard: false,
              });
              this.memory.ModalRef.componentInstance.name = "WhatsNewModal";
            }
          });
          sources
            .filter((x) => x.source_type == SourceType.Custom)
            .map((x) => x.id!)
            .forEach((x) => this.memory.CustomSourceIds?.add(x));
          sources
            .filter((x) => x.source_type == SourceType.Xtream)
            .map((x) => x.id!)
            .forEach((x) => this.memory.XtreamSourceIds.add(x));
          if (
            this.memory.XtreamSourceIds.size > 0 &&
            !sessionStorage.getItem("epgCheckedOnStart")
          ) {
            sessionStorage.setItem("epgCheckedOnStart", "true");
            invoke("on_start_check_epg");
          }
          // default_view now stores a RailItem directly (Favourites/
          // Channels/Movies/Series/History - the Settings dropdown only
          // offers those 5) rather than the old ViewMode, which could only
          // distinguish Favorites/History and left Movies/Series
          // indistinguishable from Channels.
          const defaultRailItem = settings.default_view ?? RailItem.Channels;
          this.currentRailItem = defaultRailItem in RailItem ? defaultRailItem : RailItem.Channels;
          // Allows navigating in from another routed page (e.g. Manage
          // Categories' nav rail) directly to a specific view, instead of
          // always landing back on the configured default. Number(null) is
          // 0 (a valid RailItem!), so the param's absence must be checked
          // before converting, not after.
          const railParamRaw = this.route.snapshot.queryParamMap.get("rail");
          if (railParamRaw != null) {
            const railParam = Number(railParamRaw);
            if (!isNaN(railParam) && railParam in RailItem) {
              this.currentRailItem = railParam;
            }
          }
          const { view_type, media_types } = this.filtersForRailItem(this.currentRailItem);
          this.filters = {
            source_ids: Array.from(this.memory.Sources.keys()),
            view_type,
            media_types,
            page: 1,
            use_keywords: false,
            sort: SortType.provider,
          };
          if (settings.default_sort != undefined && settings.default_sort != SortType.provider) {
            this.memory.Sort.next([settings.default_sort, false]);
            this.filters.sort = settings.default_sort;
          }
          this.chkLiveStream = this.filters.media_types.includes(MediaType.livestream);
          this.chkMovie = this.filters.media_types.includes(MediaType.movie);
          this.chkSerie = this.filters.media_types.includes(MediaType.serie) && this.anyXtream();
          if (settings.refresh_on_start === true && !sessionStorage.getItem("refreshedOnStart")) {
            sessionStorage.setItem("refreshedOnStart", "true");
            this.refreshOnStart().then((_) => _);
          }
          this.load().then((_) => _);
          this.reloadFavMediaIfActive().then((_) => _);
        }
      })
      .catch((e) => {
        this.error.handleError(e);
        this.reset();
      });
  }

  async refreshOnStart() {
    this.toast.info("Refreshing all sources... (refresh on start enabled)");
    await this.memory.tryIPC(
      "Successfully refreshed all sources (refresh on start enabled)",
      "Failed to refresh all sources (refresh on start enabled)",
      async () => {
        await invoke("refresh_all");
      },
    );
  }

  async clearHistory() {
    await this.memory.tryIPC(
      "History cleared successfully",
      "Failed to clear history",
      async () => {
        await invoke("clear_history");
      },
    );
    await this.load();
  }

  reset() {
    this.router.navigateByUrl("setup");
  }

  async addEvents() {
    this.subscriptions.push(
      this.memory.HideChannels.subscribe((val) => {
        this.channelsVisible = val;
      }),
    );
    this.subscriptions.push(
      this.memory.SetFocus.subscribe((focus) => {
        this.focus = focus;
      }),
    );
    this.subscriptions.push(
      this.memory.SetNode.subscribe(async (dto) => {
        this.nodeStack.add(
          new Node(
            dto.id,
            dto.name,
            dto.type,
            this.filters?.query,
            this.filters?.view_type,
            this.filters?.media_types,
          ),
        );
        if (dto.type == NodeType.Category) {
          this.filters!.group_id = dto.id;
          // Entering a category from the sidebar can happen while already
          // nested in a series/season (e.g. category -> a show -> the same
          // or a different category) - without clearing these, the stale
          // series_id sticks around, and search() gives series_id priority
          // over group_id, so every subsequent category click kept
          // re-showing that same series' season list instead of the
          // newly-clicked category.
          this.filters!.series_id = undefined;
          this.filters!.season = undefined;
          // The sidebar lists categories across every source, not just
          // whatever's currently selected in the main view - without this
          // (mirroring the Series branch below), group_id could point at a
          // category belonging to a source that source_ids had already
          // filtered out, ANDing to zero rows regardless of media_types.
          this.filters!.source_ids = [dto.sourceId!];
          // The sidebar's own category list is always scoped to one media
          // type (see playlist-sidebar.component.ts's selectCategory()) -
          // without this, whatever media_types was left over from earlier
          // browsing gets ANDed with this category's channels on the
          // backend (see sql.rs's search()), which can silently return zero
          // rows for a category that genuinely has content, just not of
          // that stale type.
          if (dto.mediaType !== undefined) this.filters!.media_types = [dto.mediaType];
        } else if (dto.type == NodeType.Series) {
          this.filters!.series_id = dto.id;
          this.filters!.source_ids = [dto.sourceId!];
          // Without this, clicking into a series from Favourites' channel
          // list (media_types still [livestream] from there) left
          // isChannelListView() true for the season/episode grid too,
          // rendering seasons/episodes as if they were livestream channel
          // rows - list layout, EPG timeline header and all. The backend
          // query itself already ignores media_types once series_id is set
          // (see sql.rs's search()), so this is purely about picking the
          // right frontend layout.
          this.filters!.media_types = [MediaType.serie];
        } else if (dto.type == NodeType.Season) this.filters!.season = dto.id;

        if (this.filters!.view_type == ViewMode.Hidden) {
          this.filters!.view_type = ViewMode.Categories;
        }

        this.clearSearch();
        await this.load();
        if (this.focusArea == FocusArea.Tiles) this.selectFirstChannelDelayed(100);
      }),
    );
    this.subscriptions.push(
      this.memory.Refresh.subscribe((scroll) => {
        this.load();
        if (scroll) this.viewport?.scrollToIndex(0);
        if (this.currentRailItem == RailItem.Favourites) {
          this.loadFavMedia();
          if (scroll) this.favMediaViewport?.scrollToIndex(0);
        }
      }),
    );
    this.subscriptions.push(
      this.memory.Sort.pipe(skip(1)).subscribe(async ([sort, load]) => {
        if (!this.filters || !load) return;
        this.filters!.sort = sort;
        await this.load();
        await this.reloadFavMediaIfActive();
      }),
    );
  }

  clearSearch() {
    this.search.nativeElement.value = "";
    this.prevSearchValue = "";
    this.filters!.query = "";
  }

  async loadMore() {
    this.load(true);
  }

  // Bumped at the start of every load() and captured per-call, so a call
  // superseded by a newer one (e.g. loadMore() still in flight when the
  // user switches playlist/category before it resolves) can tell it's
  // stale once its response finally arrives and discard it instead of
  // overwriting the newer, correct data or leaving `loading` stuck true -
  // the two of which combined explained a blank grid that only a full
  // reload could recover from.
  private loadToken = 0;

  async load(more = false) {
    const token = ++this.loadToken;
    this.loading = true;
    try {
      if (more) {
        this.filters!.page++;
      } else {
        this.filters!.page = 1;
      }
      const channels: Channel[] = await invoke("search", { filters: this.filters });
      if (token !== this.loadToken) return;
      if (!more) {
        this.channels = channels;
        this.channelsVisible = true;
        // prevent flicker of hiding opacity
        this.viewType = this.filters!.view_type;
        // Switching playlist/category (onSourceSelect, SetNode) never used
        // to reset scroll - if you'd scrolled deep into a long list and then
        // switched to a shorter one, the viewport stayed scrolled past the
        // new list's actual content height, so CDK rendered nothing at all
        // (every row computed as "above" the current scroll offset) until a
        // full reload reset scroll back to 0.
        this.viewport?.scrollToIndex(0);
      } else {
        this.channels = this.channels.concat(channels);
      }
      this.reachedMax = channels.length < this.PAGE_SIZE;
    } catch (e) {
      if (token !== this.loadToken) return;
      this.error.handleError(e);
    }
    if (token !== this.loadToken) return;
    this.loading = false;
    // renderedRangeStream only re-emits when the *visible* range changes -
    // if the user isn't actively scrolling, appending a page here doesn't
    // move that range, so the stream can stay silent even though the newly
    // rendered rows still don't fill the viewport + buffer. Explicitly
    // re-check after every load instead of relying only on the next scroll-
    // driven emission; deferred a tick so CDK has recomputed the range
    // against the new (larger) data array first.
    setTimeout(() => this.checkAutoLoadMore(), 0);
  }

  private checkAutoLoadMore() {
    const vp = this.viewport;
    if (!vp) return;
    const range = vp.getRenderedRange();
    if (this.shouldLoadMoreFor(range)) {
      this.loadMore();
    }
  }

  // Rows/channels are virtualized (see home.component.html's
  // cdk-virtual-scroll-viewport), so "near the end of scroll" is measured
  // against the rendered range CDK reports, not window/document scroll
  // position - this is what replaces the old Load More button. Kept as a
  // pure predicate (no side effects) so the caller can decide whether
  // acting on it needs to re-enter Angular's zone.
  private shouldLoadMoreFor(range: ListRange): boolean {
    // The viewport renders (and CDK emits an initial range) before
    // getSources() has populated filters/channels - without this guard,
    // that empty initial render looks like "range.end >= totalRows" and
    // fires loadMore() with filters still undefined, throwing inside load()
    // before it resets `loading`, which then stays stuck true forever and
    // silently blocks every future auto-load for the rest of the session.
    if (!this.filters || this.reachedMax === true || this.loading === true) return false;
    const totalRows = this.isChannelListView() ? this.channels.length : this.channelRows().length;
    return range.end >= totalRows - 5;
  }

  async loadMoreFavMedia() {
    this.loadFavMedia(true);
  }

  // Same stale-response race as load()/loadToken above, mirrored here.
  private favMediaLoadToken = 0;

  async loadFavMedia(more = false) {
    if (!this.favMediaFilters) return;
    const token = ++this.favMediaLoadToken;
    this.favMediaLoading = true;
    try {
      if (more) {
        this.favMediaFilters.page++;
      } else {
        this.favMediaFilters.page = 1;
      }
      const channels: Channel[] = await invoke("search", { filters: this.favMediaFilters });
      if (token !== this.favMediaLoadToken) return;
      if (!more) {
        this.favMediaChannels = channels;
        // Same stale scroll-offset issue as load() above.
        this.favMediaViewport?.scrollToIndex(0);
      } else {
        this.favMediaChannels = this.favMediaChannels.concat(channels);
      }
      this.favMediaReachedMax = channels.length < this.PAGE_SIZE;
    } catch (e) {
      if (token !== this.favMediaLoadToken) return;
      this.error.handleError(e);
    }
    if (token !== this.favMediaLoadToken) return;
    this.favMediaLoading = false;
    setTimeout(() => this.checkAutoLoadMoreFavMedia(), 0);
  }

  private checkAutoLoadMoreFavMedia() {
    const vp = this.favMediaViewport;
    if (!vp) return;
    const range = vp.getRenderedRange();
    if (this.shouldLoadMoreForFavMedia(range)) {
      this.loadMoreFavMedia();
    }
  }

  private shouldLoadMoreForFavMedia(range: ListRange): boolean {
    if (!this.favMediaFilters || this.favMediaReachedMax === true || this.favMediaLoading === true)
      return false;
    const totalRows = this.favMediaChannelRows().length;
    return range.end >= totalRows - 5;
  }

  // Builds/keeps favMediaFilters in sync with the shared bits of the
  // primary filters (search query, sort, keyword mode, source_ids) - one
  // search box and sort control drive both sections at once.
  private syncFavMediaFilters() {
    if (!this.filters) return;
    if (!this.favMediaFilters) {
      this.favMediaFilters = {
        source_ids: this.filters.source_ids,
        view_type: ViewMode.Favorites,
        media_types: [MediaType.movie, MediaType.serie],
        page: 1,
        use_keywords: this.filters.use_keywords,
        sort: this.filters.sort,
        query: this.filters.query,
      };
    } else {
      this.favMediaFilters.source_ids = this.filters.source_ids;
      this.favMediaFilters.use_keywords = this.filters.use_keywords;
      this.favMediaFilters.sort = this.filters.sort;
      this.favMediaFilters.query = this.filters.query;
    }
  }

  private async reloadFavMediaIfActive() {
    if (this.currentRailItem !== RailItem.Favourites) return;
    this.syncFavMediaFilters();
    await this.loadFavMedia();
  }

  ngAfterViewInit(): void {
    this.addEvents().then((_) => _);
    this.subscriptions.push(
      fromEvent(this.search.nativeElement, "keyup")
        .pipe(
          filter((event: any) => event.key !== "Escape"),
          map((event: any) => {
            this.focus = 0;
            this.focusArea = FocusArea.Tiles;
            if (this.channelsVisible && event.target.value != this.prevSearchValue)
              this.channelsVisible = false;
            this.prevSearchValue = event.target.value;
            return event.target.value;
          }),
          debounceTime(300),
        )
        .subscribe(async (term: string) => {
          this.filters!.query = term;
          await this.load();
          await this.reloadFavMediaIfActive();
        }),
    );

    this.shortcuts.push(
      {
        key: ["ctrl + f", "ctrl + space", "cmd + f"],
        label: "Search",
        description: "Go to search",
        preventDefault: true,
        allowIn: [AllowIn.Input],
        command: (_) => this.focusSearch(),
      },
      {
        key: ["ctrl + a", "cmd + a"],
        label: "Switching modes",
        description: "Selects the all channels view",
        preventDefault: true,
        command: async (_) => await this.switchMode(this.viewModeEnum.All),
      },
      {
        key: ["ctrl + s", "cmd + s"],
        label: "Switching modes",
        description: "Selects the categories view",
        command: async (_) => await this.switchMode(this.viewModeEnum.Categories),
      },
      {
        key: ["ctrl + d", "cmd + d"],
        label: "Switching modes",
        description: "Selects the history view",
        command: async (_) => await this.switchMode(this.viewModeEnum.History),
      },
      {
        key: ["ctrl + r", "cmd + r"],
        label: "Switching modes",
        description: "Selects the favorites view",
        command: async (_) => await this.switchMode(this.viewModeEnum.Favorites),
      },
      {
        key: "ctrl + q",
        label: "Media Type Filters",
        description: "Enable/Disable livestreams",
        preventDefault: true,
        allowIn: [AllowIn.Input],
        command: async (_) => {
          this.chkLiveStream = !this.chkLiveStream;
          this.updateMediaTypes(MediaType.livestream);
        },
      },
      {
        key: "ctrl + w",
        label: "Media Type Filters",
        description: "Enable/Disable movies",
        preventDefault: true,
        allowIn: [AllowIn.Input],
        command: async (_) => {
          this.chkMovie = !this.chkMovie;
          this.updateMediaTypes(MediaType.movie);
        },
      },
      {
        key: "ctrl + e",
        label: "Media Type Filters",
        description: "Enable/Disable series",
        preventDefault: true,
        allowIn: [AllowIn.Input],
        command: async (_) => {
          this.chkSerie = !this.chkSerie;
          this.updateMediaTypes(MediaType.serie);
        },
      },
      {
        key: "left",
        label: "Navigation",
        description: "Go left",
        allowIn: [AllowIn.Input],
        command: async (_) => await this.nav("ArrowLeft"),
      },
      {
        key: "right",
        label: "Navigation",
        description: "Go right",
        allowIn: [AllowIn.Input],
        command: async (_) => await this.nav("ArrowRight"),
      },
      {
        key: "up",
        label: "Navigation",
        description: "Go up",
        allowIn: [AllowIn.Input],
        preventDefault: true,
        command: async (_) => await this.nav("ArrowUp"),
      },
      {
        key: "down",
        label: "Navigation",
        description: "Go down",
        allowIn: [AllowIn.Input],
        preventDefault: true,
        command: async (_) => await this.nav("ArrowDown"),
      },
    );
  }

  updateMediaTypes(mediaType: MediaType) {
    const index = this.filters!.media_types.indexOf(mediaType);
    if (index == -1) this.filters!.media_types.push(mediaType);
    else this.filters!.media_types.splice(index, 1);
    this.load();
  }

  // The dual-section Favourites layout only applies at its top level -
  // drilling into a favourited series' seasons/episodes falls back to the
  // normal single-list rendering used everywhere else.
  showFavMediaSection(): boolean {
    return (
      this.currentRailItem === RailItem.Favourites &&
      !this.filters?.series_id &&
      !this.filters?.group_id
    );
  }

  // Shown in place of the (otherwise silently empty) grid/list once a load
  // has actually finished with zero rows - loading/channelsVisible guards
  // keep this from flashing during the brief window before the first
  // response of a fresh load() comes back.
  showEmptyState(): boolean {
    return (
      !this.loading &&
      this.channelsVisible &&
      this.channels.length === 0 &&
      !this.showFavMediaSection()
    );
  }

  emptyStateMessage(): string {
    if (this.filters?.season) return "This season has no episodes.";
    if (this.filters?.series_id) return "No seasons found for this series.";
    if (this.filters?.query) return "No results found.";
    return "Nothing to show here.";
  }

  filtersVisible() {
    return (
      !this.filters?.series_id &&
      !this.sidebarVisible() &&
      this.currentRailItem !== RailItem.Favourites &&
      this.currentRailItem !== RailItem.History
    );
  }

  async switchMode(viewMode: ViewMode) {
    if (viewMode == this.filters?.view_type) return;
    this.filters!.series_id = undefined;
    this.filters!.group_id = undefined;
    this.filters!.view_type = viewMode;
    this.filters!.season = undefined;
    this.clearSearch();
    this.nodeStack.clear();
    await this.load();
  }

  filtersForRailItem(item: RailItem): { view_type: ViewMode; media_types: MediaType[] } {
    switch (item) {
      case RailItem.Favourites:
        // Favourites splits into two sections (see favMedia* below) - this
        // is just the primary (channels) section's filters now.
        return {
          view_type: ViewMode.Favorites,
          media_types: [MediaType.livestream],
        };
      case RailItem.Movies:
        return { view_type: ViewMode.All, media_types: [MediaType.movie] };
      case RailItem.Series:
        return { view_type: ViewMode.All, media_types: [MediaType.serie] };
      case RailItem.History:
        return {
          view_type: ViewMode.History,
          media_types: [MediaType.livestream, MediaType.movie, MediaType.serie],
        };
      default:
        return { view_type: ViewMode.All, media_types: [MediaType.livestream] };
    }
  }

  async selectRail(item: RailItem) {
    if (item == RailItem.Settings) {
      this.openSettings();
      return;
    }
    if (item == RailItem.ManageCategories) {
      this.router.navigateByUrl("manage-categories");
      return;
    }
    if (item == RailItem.Downloads) {
      this.currentRailItem = item;
      this.clearSearch();
      this.nodeStack.clear();
      return;
    }
    this.currentRailItem = item;
    // Only meaningful on Favourites/mid-navigation respectively - stale
    // otherwise, and would misapply the wrong tilesPerRow/bounds or focus
    // a no-longer-relevant pre-grid button if left set from before.
    this.inFavMedia = false;
    this.preGridFocusIndex = -1;
    this.filters!.series_id = undefined;
    this.filters!.group_id = undefined;
    this.filters!.season = undefined;
    this.filters!.source_ids = Array.from(this.memory.Sources.keys());
    const { view_type, media_types } = this.filtersForRailItem(item);
    this.filters!.view_type = view_type;
    this.filters!.media_types = media_types;
    this.chkLiveStream = this.filters!.media_types.includes(MediaType.livestream);
    this.chkMovie = this.filters!.media_types.includes(MediaType.movie);
    this.chkSerie = this.filters!.media_types.includes(MediaType.serie);
    this.clearSearch();
    this.nodeStack.clear();
    await this.load();
    await this.reloadFavMediaIfActive();
    // Leaving/entering Favourites toggles the primary viewport's CSS class
    // (fav-channels-viewport caps it to 40vh so the movies/series section
    // below has room) - the viewport element itself often persists across
    // this navigation, but CDK only measures its size on scroll/init, not on
    // arbitrary CSS/flex changes, so without an explicit recheck here it
    // keeps rendering as if still capped at the old (smaller) height even
    // after the container has actually grown back to full size.
    setTimeout(() => this.viewport?.checkViewportSize(), 0);
  }

  sidebarVisible(): boolean {
    // Gated on filters being loaded (not just the rail selection) so the
    // sidebar doesn't mount - and try to auto-expand playlists - before
    // getSources() has actually populated memory.Sources. currentRailItem
    // defaults to Channels before that async load finishes, so without this
    // guard the sidebar would initialize against an empty source list and
    // never retry once the real data arrives.
    return (
      this.filters != undefined &&
      (this.currentRailItem == RailItem.Channels ||
        this.currentRailItem == RailItem.Movies ||
        this.currentRailItem == RailItem.Series)
    );
  }

  sidebarMediaType(): MediaType | undefined {
    switch (this.currentRailItem) {
      case RailItem.Channels:
        return MediaType.livestream;
      case RailItem.Movies:
        return MediaType.movie;
      case RailItem.Series:
        return MediaType.serie;
      default:
        return undefined;
    }
  }

  readonly navRailWidthPx = 60;
  sidebarWidthPx = 216;
  readonly downloadSidebarWidthPx = 216;

  mainContentMarginPx(): number {
    if (this.currentRailItem === RailItem.Downloads) {
      return this.navRailWidthPx + this.downloadSidebarWidthPx;
    }
    return this.navRailWidthPx + (this.sidebarVisible() ? this.sidebarWidthPx : 0);
  }

  async onSourceSelect(sourceIds: number[] | undefined) {
    this.filters!.source_ids = sourceIds ?? Array.from(this.memory.Sources.keys());
    this.filters!.group_id = undefined;
    this.nodeStack.clear();
    await this.load();
  }

  isChannelListView(): boolean {
    return (
      this.filters?.media_types?.length === 1 &&
      this.filters?.media_types[0] === MediaType.livestream
    );
  }

  tilesPerRow(): number {
    return this.isChannelListView() ? 1 : 3;
  }

  // The Favourites page's second grid (movies/series) is always a 3-column
  // grid regardless of the primary section's own list/grid mode above it
  // (matching favMediaChannelRows()'s own chunking) - unlike tilesPerRow(),
  // not affected by isChannelListView().
  favTilesPerRow(): number {
    return 3;
  }

  // Stable identity for the channel grid/list so a fresh search or re-sort
  // (which always assigns a brand-new array of brand-new Channel objects,
  // even for logically-unchanged rows - see load()) reuses existing tile
  // DOM instead of destroying and rebuilding every tile. Default identity
  // (object reference) tracking can't recognize those as "the same" channel
  // since the objects themselves are new each time.
  trackByChannel(_index: number, channel: Channel): string | number {
    return channel.id != null ? `${channel.media_type}-${channel.id}` : _index;
  }

  // itemSize for the grid viewport must be a fixed row height (CDK's
  // default virtual-scroll strategy requires uniform item size), matching
  // .channel's 4.5em height plus the row gap each layout adds below it.
  rowItemSizePx(): number {
    return this.isChannelListView() ? 80 : 88;
  }

  // Chunks the flat channel list into rows for the grid viewport (grid mode
  // virtual-scrolls rows, not individual tiles). Memoized against the
  // channels array reference - load()/loadMore() always reassign (never
  // mutate) this.channels, so reference equality is a cheap, correct cache
  // key and avoids re-chunking potentially tens of thousands of items on
  // every change-detection pass.
  private cachedRows: Channel[][] = [];
  private cachedRowsFor?: Channel[];
  private cachedRowsSize = 0;

  channelRows(): Channel[][] {
    const size = this.tilesPerRow();
    if (this.cachedRowsFor === this.channels && this.cachedRowsSize === size) {
      return this.cachedRows;
    }
    const rows: Channel[][] = [];
    for (let i = 0; i < this.channels.length; i += size) {
      rows.push(this.channels.slice(i, i + size));
    }
    this.cachedRows = rows;
    this.cachedRowsFor = this.channels;
    this.cachedRowsSize = size;
    return rows;
  }

  private favMediaCachedRows: Channel[][] = [];
  private favMediaCachedRowsFor?: Channel[];

  favMediaChannelRows(): Channel[][] {
    if (this.favMediaCachedRowsFor === this.favMediaChannels) {
      return this.favMediaCachedRows;
    }
    const rows: Channel[][] = [];
    const size = this.favTilesPerRow();
    for (let i = 0; i < this.favMediaChannels.length; i += size) {
      rows.push(this.favMediaChannels.slice(i, i + size));
    }
    this.favMediaCachedRows = rows;
    this.favMediaCachedRowsFor = this.favMediaChannels;
    return rows;
  }

  searchFocused(): boolean {
    return document.activeElement?.id == "search";
  }

  focusSearch() {
    if (this.searchFocused()) {
      this.selectFirstChannel();
      return;
    } else {
      this.focus = 0;
      this.focusArea = FocusArea.Tiles;
    }
    this.viewport?.scrollToIndex(0, "smooth");
    this.search.nativeElement.focus({
      preventScroll: true,
    });
  }

  async goBackHotkey() {
    if (this.memory.ModalRef) {
      if (
        this.memory.ModalRef.componentInstance.name != "RestreamModalComponent" ||
        !this.memory.ModalRef.componentInstance.started
      )
        this.memory.ModalRef.close("close");
      return;
    } else if (this.memory.currentContextMenu?.menuOpen) {
      this.closeContextMenu();
    } else if (this.searchFocused()) {
      this.selectFirstChannel();
    } else if (this.filters?.query) {
      if (this.filters?.query) {
        this.clearSearch();
        await this.load();
        await this.reloadFavMediaIfActive();
      }
      this.selectFirstChannelDelayed(100);
    } else if (this.nodeStack.hasNodes()) {
      await this.goBack();
      this.selectFirstChannelDelayed(100);
    } else {
      this.selectFirstChannel();
    }
  }

  selectFirstChannelDelayed(milliseconds: number) {
    setTimeout(() => this.selectFirstChannel(), milliseconds);
  }

  async goBack() {
    var node = this.nodeStack.pop();
    if (node.type == NodeType.Category) {
      this.filters!.group_id = undefined;
      this.filters!.source_ids = Array.from(this.memory.Sources.keys());
    } else if (node.type == NodeType.Series) {
      this.filters!.series_id = undefined;
      this.filters!.source_ids = Array.from(this.memory.Sources.keys());
    } else if (node.type == NodeType.Season) {
      this.filters!.season = undefined;
    }
    if (node.query) {
      this.search.nativeElement.value = node.query;
      this.filters!.query = node.query;
    }
    if (node.fromViewType && this.filters!.view_type !== node.fromViewType) {
      this.filters!.view_type = node.fromViewType;
    }
    if (node.fromMediaTypes) {
      this.filters!.media_types = node.fromMediaTypes;
    }
    await this.load();
  }

  openSettings() {
    this.router.navigateByUrl("settings");
  }

  // Tab/Shift+Tab (cycleFocusArea) is the only thing that moves *between*
  // regions now - arrow keys here only ever move *within* whichever region
  // currently has focus, clamped at its bounds (no more falling through to
  // the next/previous region on overflow).
  async nav(key: string) {
    if (this.searchFocused()) {
      // Down from the search box drops straight into the channel grid,
      // mirroring how a combobox's results list works - Tab still also
      // gets you there, this is just the faster/more expected path. Lands
      // on the first pre-grid button first when any are shown, same as
      // entering Tiles any other way (see focusPreGridOrTile()).
      if (key == "ArrowDown") {
        this.focusArea = FocusArea.Tiles;
        await this.focusPreGridOrTile(0);
      }
      return;
    }
    if (
      this.navRailFocused() ||
      this.playlistFocused() ||
      this.epgGuideFocused() ||
      this.downloadSidebarFocused() ||
      this.downloadListFocused()
    )
      return;
    const lowSize = this.currentWindowSize < 768;
    if (this.memory.currentContextMenu?.menuOpen || this.memory.ModalRef) {
      return;
    }
    // Pre-grid buttons (back/clear history/...) aren't indexed like tiles -
    // handled separately here rather than folded into the tmpFocus math
    // below. Down steps to the next one (or into the grid, past the last);
    // Up steps to the previous one (or up to search, past the first).
    if (this.focusArea == FocusArea.Tiles && this.preGridFocusIndex >= 0) {
      if (key != "ArrowDown" && key != "ArrowUp") return;
      const ids = this.preGridButtonIds();
      const next = this.preGridFocusIndex + (key == "ArrowDown" ? 1 : -1);
      if (next >= ids.length) {
        this.preGridFocusIndex = -1;
        this.focus = 0;
        await this.focusTile(0);
      } else if (next < 0) {
        this.preGridFocusIndex = -1;
        this.focusArea = FocusArea.Search;
        this.focus = 0;
        this.search.nativeElement.focus();
      } else {
        this.preGridFocusIndex = next;
        document.getElementById(ids[next])?.focus();
      }
      return;
    }
    const inTiles = this.focusArea == FocusArea.Tiles;
    const gridSize = inTiles ? (this.inFavMedia ? this.favTilesPerRow() : this.tilesPerRow()) : 1;
    let tmpFocus = 0;
    switch (key) {
      case "ArrowUp":
        tmpFocus -= gridSize;
        break;
      case "ArrowDown":
        tmpFocus += gridSize;
        break;
      case "ArrowLeft":
        tmpFocus -= 1;
        break;
      case "ArrowRight":
        tmpFocus += 1;
        break;
    }
    if (lowSize && tmpFocus % 3 == 0 && inTiles) tmpFocus / 3;
    tmpFocus += this.focus;
    // Symmetric with the search->grid drop above - Up from the top row of
    // whichever section is active goes back to search (from the primary
    // section, via the last pre-grid button first if any are shown) or
    // down into the primary section (from favMedia), instead of just
    // clamping at the first tile.
    if (inTiles && key == "ArrowUp" && tmpFocus < 0) {
      if (this.inFavMedia) {
        this.inFavMedia = false;
        this.focus = Math.max(0, this.channels.length - 1);
        await this.focusTile(this.focus);
      } else if (this.preGridButtonIds().length > 0) {
        const ids = this.preGridButtonIds();
        this.preGridFocusIndex = ids.length - 1;
        setTimeout(() => document.getElementById(ids[ids.length - 1])?.focus(), 0);
      } else {
        this.focusArea = FocusArea.Search;
        this.focus = 0;
        this.search.nativeElement.focus();
      }
      return;
    }
    // Continues down into the Favourites page's second grid (movies/
    // series) instead of clamping at the primary section's last tile -
    // see showFavMediaSection()/ChannelTileComponent's idPrefix input.
    if (
      inTiles &&
      !this.inFavMedia &&
      key == "ArrowDown" &&
      tmpFocus >= this.channels.length &&
      this.showFavMediaSection() &&
      this.favMediaChannels.length > 0
    ) {
      this.inFavMedia = true;
      this.focus = 0;
      await this.focusTile(0);
      return;
    }
    tmpFocus = Math.max(0, tmpFocus);
    const activeChannels = this.inFavMedia ? this.favMediaChannels : this.channels;
    const activeReachedMax = this.inFavMedia ? this.favMediaReachedMax : this.reachedMax;
    if (inTiles && tmpFocus >= this.filters!.page * 36 && !activeReachedMax) {
      if (this.inFavMedia) await this.loadMoreFavMedia();
      else await this.loadMore();
    } else {
      if (tmpFocus >= activeChannels.length)
        tmpFocus = (activeChannels.length == 0 ? 1 : activeChannels.length) - 1;
      this.focus = tmpFocus;
      await this.focusTile(this.focus);
    }
  }

  private navRailFocused(): boolean {
    return document.activeElement?.closest(".nav-rail") != null;
  }

  private playlistFocused(): boolean {
    return document.activeElement?.closest(".playlist-sidebar") != null;
  }

  private downloadSidebarFocused(): boolean {
    return document.activeElement?.closest(".download-sidebar") != null;
  }

  private downloadListFocused(): boolean {
    return document.activeElement?.closest("app-download-manager") != null;
  }

  // DOM focus stays on the tile itself while its EPG guide is active (see
  // ChannelTileComponent.epgGuideActive) - this is how nav() knows to stay
  // out of the way while Left/Right/Up/Down are being handled there
  // instead, since a plain document.activeElement check can't tell that
  // apart from normal grid focus otherwise.
  private epgGuideFocused(): boolean {
    return document.activeElement?.getAttribute("data-epg-guide") == "true";
  }

  // Entry point shared by every way of landing on Tiles fresh (Tab, the
  // search->grid drop, clicking a category/series) - lands on the first
  // pre-grid button first when any are shown, same as arrowing up out of
  // the first tile would.
  private async focusPreGridOrTile(index: number) {
    this.inFavMedia = false;
    const ids = this.preGridButtonIds();
    if (ids.length > 0) {
      this.preGridFocusIndex = 0;
      this.focus = 0;
      setTimeout(() => document.getElementById(ids[0])?.focus(), 0);
    } else {
      this.preGridFocusIndex = -1;
      this.focus = index;
      await this.focusTile(index);
    }
  }

  // Tiles are virtualized, so the target index may not have a DOM element
  // yet (it's off-screen and recycled) - scroll it into view first and wait
  // for CDK to actually render it before trying to focus it. Targets
  // whichever of the two Favourites-page grids is currently active (see
  // inFavMedia) - the primary grid/list and the movies/series grid below
  // it have entirely separate viewports and DOM id prefixes.
  private async focusTile(index: number) {
    const vp = this.inFavMedia ? this.favMediaViewport : this.viewport;
    const rowSize = this.inFavMedia ? this.favTilesPerRow() : this.tilesPerRow();
    const idPrefix = this.inFavMedia ? "favtile-" : FocusAreaPrefix[FocusArea.Tiles];
    const rowIndex = Math.floor(index / rowSize);
    if (vp) {
      const range = vp.getRenderedRange();
      if (rowIndex < range.start || rowIndex >= range.end) {
        vp.scrollToIndex(rowIndex, "auto");
        await new Promise<void>((resolve) => {
          const sub = vp.renderedRangeStream.subscribe((r) => {
            if (rowIndex >= r.start && rowIndex < r.end) {
              sub.unsubscribe();
              resolve();
            }
          });
          setTimeout(() => {
            sub.unsubscribe();
            resolve();
          }, 300);
        });
      }
    }
    setTimeout(() => {
      document.getElementById(`${idPrefix}${this.focus}`)?.focus();
    }, 0);
  }

  anyXtream() {
    return (
      Array.from(this.memory.Sources.values()).findIndex(
        (x) => x.source_type == SourceType.Xtream,
      ) != -1
    );
  }

  // The Tab-stops, in order - Playlist is skipped when the sidebar isn't
  // currently shown. The Downloads rail item replaces the entire main
  // content area with <app-download-manager> - no search box or channel
  // grid exists there at all - so it gets its own short cycle instead of
  // filtering the normal one down to just NavRail. This is the only thing
  // that moves focus *between* regions; nav() (arrow keys) only ever moves
  // within one.
  private focusOrder(): FocusArea[] {
    if (this.currentRailItem === RailItem.Downloads) {
      return [FocusArea.NavRail, FocusArea.DownloadSidebar, FocusArea.DownloadList];
    }
    const order = [FocusArea.NavRail, FocusArea.Playlist, FocusArea.Search, FocusArea.Tiles];
    return this.sidebarVisible() ? order : order.filter((a) => a !== FocusArea.Playlist);
  }

  cycleFocusArea(forward: boolean) {
    const order = this.focusOrder();
    const from = order.indexOf(this.focusArea);
    const next =
      order[((from == -1 ? 0 : from) + (forward ? 1 : -1) + order.length) % order.length];
    this.focusArea = next;
    this.focus = 0;
    switch (next) {
      case FocusArea.NavRail:
        this.navRail?.focusFirstRow();
        break;
      case FocusArea.Playlist:
        this.playlistSidebar?.focusFirstRow();
        break;
      case FocusArea.Search:
        this.search?.nativeElement.focus();
        break;
      case FocusArea.Tiles:
        this.focusPreGridOrTile(0);
        break;
      case FocusArea.DownloadSidebar:
        this.downloadSidebar?.focusFirstRow();
        break;
      case FocusArea.DownloadList:
        this.downloadManager?.focusFirstRow();
        break;
    }
  }

  //Temporary solution because the ng-keyboard-shortcuts library doesn't seem to support ESC
  @HostListener("document:keydown", ["$event"])
  onKeyDown(event: KeyboardEvent) {
    if (
      event.key == "Escape" ||
      event.key == "BrowserBack" ||
      (event.key == "Backspace" && !isInputFocused())
    ) {
      this.goBackHotkey();
      event.preventDefault();
    }
    if (event.key == "Tab" && !this.memory.ModalRef) {
      event.preventDefault();
      this.cycleFocusArea(!event.shiftKey);
    }
    if (event.key == "Enter" && document.activeElement?.id?.startsWith("filter-"))
      (document.activeElement as any).click();
  }

  selectFirstChannel() {
    this.focusArea = FocusArea.Tiles;
    this.focus = 0;
    this.inFavMedia = false;
    this.preGridFocusIndex = -1;
    this.viewport?.scrollToIndex(0, "auto");
    setTimeout(() => {
      (document.getElementById("first")?.firstChild as HTMLElement)?.focus();
    }, 50);
  }

  closeContextMenu() {
    if (this.memory.currentContextMenu?.menuOpen) {
      this.memory.currentContextMenu?.closeMenu();
    }
  }

  ngOnDestroy() {
    this.subscriptions.forEach((x) => x.unsubscribe());
    this.viewportSubscriptions.forEach((x) => x.unsubscribe());
    this.favMediaViewportSubscriptions.forEach((x) => x.unsubscribe());
  }

  async toggleKeywords() {
    this.filters!.use_keywords = !this.filters!.use_keywords;
    await this.load();
    await this.reloadFavMediaIfActive();
  }

  async bulkAction(action: BulkActionType) {
    if (this.filters?.series_id && !this.filters?.season) {
      return;
    }
    const actionName = BulkActionType[action].toLowerCase();
    try {
      await invoke("bulk_update", { filters: this.filters, action: action });
      await this.load();
      this.toast.success(`Successfully executed bulk update: ${actionName}`);
    } catch (e) {
      this.error.handleError(e);
    }
  }
}
