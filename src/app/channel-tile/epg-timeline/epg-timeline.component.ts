import {
  AfterViewInit,
  ChangeDetectionStrategy,
  ChangeDetectorRef,
  Component,
  ElementRef,
  Input,
  OnChanges,
  OnDestroy,
  SimpleChanges,
} from "@angular/core";
import { NgbModal } from "@ng-bootstrap/ng-bootstrap";
import { invoke } from "@tauri-apps/api/core";
import { debounceTime, interval, Subject, Subscription } from "rxjs";
import { EpgModalComponent } from "../../epg-modal/epg-modal.component";
import { MemoryService } from "../../memory.service";
import { Channel } from "../../models/channel";
import { EPG } from "../../models/epg";
import {
  EPG_FETCH_LOOKAHEAD_SECONDS,
  EPG_FETCH_LOOKBACK_SECONDS,
  EPG_PAN_STEP_SECONDS,
  epgTimelinePercentFor,
} from "../../models/epgTimelineWindow";

@Component({
    selector: "app-epg-timeline",
    templateUrl: "./epg-timeline.component.html",
    styleUrl: "./epg-timeline.component.css",
    changeDetection: ChangeDetectionStrategy.OnPush,
    standalone: false
})
export class EpgTimelineComponent implements AfterViewInit, OnChanges, OnDestroy {
  @Input() channel?: Channel;

  epgs: EPG[] = [];
  fetched = false;
  // Which block is under the keyboard "guide cursor" - null when the
  // guide isn't active. Kept as component state rather than moving real
  // DOM focus into a block, since virtual-scroll recycling would make
  // tracking/restoring real focus across row recycles far more fragile
  // than just letting ChannelTileComponent keep DOM focus on the tile.
  //
  // Tracked by start_timestamp (unique per programme on a given channel)
  // rather than a raw array index - moving the guide cursor off the edge
  // of the currently visible window pans the shared timeline to follow
  // it (see move()/ensureFocusedVisible()), which re-fetches epgs for the
  // new window and replaces this array entirely, so a plain index would
  // silently point at the wrong programme (or nothing) once that lands.
  private focusedStartTimestamp: number | null = null;

  get focusedIndex(): number | null {
    if (this.focusedStartTimestamp == null) return null;
    const index = this.epgs.findIndex((e) => e.start_timestamp === this.focusedStartTimestamp);
    return index == -1 ? null : index;
  }
  // The window all blocks/the now-line position against - real time plus
  // the shared pan offset, so every row moves together when you page
  // forward/backward via the timeline header.
  now = Date.now() / 1000;

  // Cached once per tick rather than reading Date.now() directly inside
  // nowPercent()/showNowLine()/isNowPlaying() - calling Date.now() fresh
  // from two separate template bindings in the same change-detection pass
  // can return different values milliseconds apart, which Angular's
  // dev-mode checkNoChanges pass flags as NG0100.
  private trueNow = Date.now() / 1000;
  private offsetSeconds = 0;
  private observer?: IntersectionObserver;
  private tickSubscription?: Subscription;
  private offsetSubscription?: Subscription;
  private recycleSubscription?: Subscription;
  // Virtual scroll can recycle this row through many channels in a single
  // fast scroll - fetching (an IPC round-trip + full block re-render) on
  // every one of those intermediate swaps was the main source of the CPU
  // spike reported while scrolling quickly, since almost all of that work
  // is thrown away the instant the row recycles again. Debouncing means
  // only the channel the row actually settles on triggers a fetch.
  private recycled$ = new Subject<void>();
  private viewInitialized = false;

  constructor(
    private el: ElementRef,
    private modal: NgbModal,
    private memory: MemoryService,
    private cdr: ChangeDetectorRef,
  ) {}

  ngAfterViewInit(): void {
    this.viewInitialized = true;
    this.tickSubscription = interval(60000).subscribe(() => this.updateNow());
    this.offsetSubscription = this.memory.EpgTimelineOffsetSeconds.subscribe((offset) => {
      this.offsetSeconds = offset;
      this.updateNow();
      // Only already-fetched (i.e. visible) rows need to follow a pan -
      // an off-screen row will pick up the current offset whenever its
      // own IntersectionObserver eventually fires the first fetch.
      if (this.fetched) {
        this.fetchEpg();
      }
    });
    this.observer = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        this.fetchEpg();
        this.observer?.disconnect();
        this.observer = undefined;
      }
    });
    this.observer.observe(this.el.nativeElement);
    this.recycleSubscription = this.recycled$
      .pipe(debounceTime(150))
      .subscribe(() => this.fetchEpg());
  }

  ngOnChanges(changes: SimpleChanges): void {
    // Virtual scroll recycles this component instance for a different row's
    // channel as you scroll - the initial fetch (above) is handled by the
    // IntersectionObserver, but a recycled instance never gets a fresh
    // ngAfterViewInit, so without this a recycled row keeps whatever epgs
    // fetchEpg() last put there.
    //
    // Deliberately does NOT touch epgs/fetched here - it used to clear both
    // immediately on every recycle, but during a fast scroll a row can
    // recycle through many channels before settling, and clearing on each
    // one blanked the tile repeatedly ("loads then unloads") while also
    // doing that clear-triggered render at full recycle frequency, not just
    // on settle. Left alone, the previous channel's blocks just stay up
    // until the debounced fetch below actually resolves and replaces them
    // in one clean swap - stale for a moment rather than blank.
    if (!changes["channel"] || changes["channel"].firstChange || !this.viewInitialized) return;
    this.observer?.disconnect();
    this.observer = undefined;
    this.recycled$.next();
  }

  // Only fetches a window around the current (possibly panned) position,
  // not the channel's whole retained schedule - see get_epg_schedule for
  // the full-range fetch, done once by the modal instead.
  private async fetchEpg() {
    // Captured so a slow response can't clobber the display if this row
    // recycles to yet another channel before this request resolves.
    const requestedChannel = this.channel;
    let result: EPG[];
    try {
      result = await invoke("get_epg", {
        channel: this.channel,
        startTimestamp: Math.floor(this.now - EPG_FETCH_LOOKBACK_SECONDS),
        endTimestamp: Math.floor(this.now + EPG_FETCH_LOOKAHEAD_SECONDS),
      });
    } catch {
      result = [];
    }
    if (this.channel !== requestedChannel) return;
    this.epgs = result;
    this.fetched = true;
    // Resolves from an IPC promise, not a template-bound event - needs an
    // explicit nudge under OnPush (see updateNow()).
    this.cdr.markForCheck();
  }

  private updateNow(): void {
    this.trueNow = Date.now() / 1000;
    this.now = this.trueNow + this.offsetSeconds;
    // Runs from a timer/subscription, not a template-bound event - under
    // OnPush (both this component and its ChannelTileComponent parent),
    // that needs an explicit nudge or the now-line/now-playing highlight
    // would just silently stop updating.
    this.cdr.markForCheck();
  }

  // Positions the true current time within the (possibly panned) window,
  // rather than always sitting at a fixed spot - only actually falls
  // inside the visible [0, 100] range when the window hasn't been panned
  // away from live.
  nowPercent(): number {
    return epgTimelinePercentFor(this.trueNow, this.now);
  }

  showNowLine(): boolean {
    const percent = this.nowPercent();
    return percent >= 0 && percent <= 100;
  }

  // epg.now_playing is a snapshot computed by the backend at fetch time -
  // a tile can stay mounted (and its EPG unfetched again) for a long time,
  // so relying on that flag directly leaves the highlight stuck on whatever
  // was airing back then. Recomputed against the true current time (not
  // the panned window) so "now playing" always means exactly that,
  // regardless of what part of the timeline is currently in view.
  isNowPlaying(epg: EPG): boolean {
    return epg.start_timestamp <= this.trueNow && epg.end_timestamp > this.trueNow;
  }

  // Clamped to the visible [0, 100] range rather than the raw start/end
  // percent, so a programme that started well before the window (an
  // overnight movie, a long-running placeholder block, etc.) still renders
  // - and its title stays visible - instead of sitting off-screen to the
  // left with only its unlabeled tail end inside the container.
  leftPercent(epg: EPG): number {
    return Math.max(0, epgTimelinePercentFor(epg.start_timestamp, this.now));
  }

  widthPercent(epg: EPG): number {
    const left = this.leftPercent(epg);
    const right = Math.min(100, epgTimelinePercentFor(epg.end_timestamp, this.now));
    return Math.max(0, right - left);
  }

  tooltip(epg: EPG): string {
    return `${epg.title} (${epg.start_time} - ${epg.end_time})`;
  }

  // Entering with no data yet (row not fetched, or a genuinely empty
  // schedule) is a no-op rather than focusing a nonexistent block -
  // ChannelTileComponent checks this return value to decide whether it
  // actually entered guide mode.
  enterGuide(): boolean {
    if (!this.fetched || this.epgs.length === 0) return false;
    const nowEpg = this.epgs.find((e) => this.isNowPlaying(e));
    this.focusedStartTimestamp = (nowEpg ?? this.epgs[0]).start_timestamp;
    this.cdr.markForCheck();
    return true;
  }

  exitGuide() {
    this.focusedStartTimestamp = null;
    this.cdr.markForCheck();
  }

  move(delta: number) {
    const index = this.focusedIndex;
    if (index == null || this.epgs.length === 0) return;
    const nextIndex = Math.max(0, Math.min(this.epgs.length - 1, index + delta));
    this.focusedStartTimestamp = this.epgs[nextIndex].start_timestamp;
    this.ensureFocusedVisible();
    this.cdr.markForCheck();
  }

  // The 48h-wide fetch (see EPG_FETCH_LOOKBACK/LOOKAHEAD_SECONDS) covers
  // far more than the 4h actually rendered on screen (EPG_TIMELINE_
  // DURATION_SECONDS) - without this, moving the guide cursor past
  // whatever's currently in view just silently focused an off-screen
  // block with nothing visibly changing. Pans the shared timeline (every
  // row moves together, same as the header's own pan buttons) by whole
  // steps until the focused programme is back in view.
  private ensureFocusedVisible() {
    if (this.focusedStartTimestamp == null) return;
    const epg = this.epgs[this.focusedIndex!];
    let offset = this.offsetSeconds;
    const guardLimit =
      (EPG_FETCH_LOOKBACK_SECONDS + EPG_FETCH_LOOKAHEAD_SECONDS) / EPG_PAN_STEP_SECONDS + 1;
    for (
      let i = 0;
      i < guardLimit && epgTimelinePercentFor(epg.end_timestamp, this.trueNow + offset) <= 0;
      i++
    ) {
      offset -= EPG_PAN_STEP_SECONDS;
    }
    for (
      let i = 0;
      i < guardLimit && epgTimelinePercentFor(epg.start_timestamp, this.trueNow + offset) >= 100;
      i++
    ) {
      offset += EPG_PAN_STEP_SECONDS;
    }
    if (offset != this.offsetSeconds) this.memory.EpgTimelineOffsetSeconds.next(offset);
  }

  selectFocused() {
    const index = this.focusedIndex;
    if (index == null) return;
    const epg = this.epgs[index];
    if (epg) this.onProgrammeClick(epg);
  }

  // Normal EPG display comes from bulk-fetched data, which never carries
  // catch-up info - checking it is on-demand, per programme, handled by
  // EpgModalComponent itself (so it applies whichever programme is
  // currently shown as you browse via prev/next, not just the one clicked
  // to open this modal).
  onProgrammeClick(epg: EPG, event?: MouseEvent) {
    event?.stopPropagation();
    this.memory.ModalRef = this.modal.open(EpgModalComponent, {
      backdrop: "static",
      size: "xl",
      keyboard: false,
    });
    this.memory.ModalRef.result.then((_) => (this.memory.ModalRef = undefined));
    const instance = this.memory.ModalRef.componentInstance;
    // The modal fetches the channel's whole retained schedule itself (see
    // get_epg_schedule) so its prev/next buttons can step through adjacent
    // programmes - this component's own this.epgs is only ever a narrow
    // window around the current pan position, not the full schedule.
    instance.channel = this.channel;
    instance.initialStartTimestamp = epg.start_timestamp;
    instance.name = this.channel?.name;
    instance.channelId = this.channel?.id;
    instance.sourceId = this.channel?.source_id;
  }

  ngOnDestroy(): void {
    this.observer?.disconnect();
    this.tickSubscription?.unsubscribe();
    this.offsetSubscription?.unsubscribe();
    this.recycleSubscription?.unsubscribe();
  }
}
