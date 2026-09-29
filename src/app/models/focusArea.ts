export enum FocusArea {
    NavRail,
    Playlist,
    Search,
    Tiles,
    // Only ever reachable on the Downloads rail item, where Playlist/
    // Search/Tiles aren't rendered at all - see HomeComponent.focusOrder().
    DownloadSidebar,
    DownloadList
}

// Only Tiles uses this generic prefix+index id lookup - every other region
// (NavRail included, since it became self-contained) is focused directly
// (component method / ViewChild) instead, so they're deliberately absent
// here rather than given a meaningless prefix.
export const FocusAreaPrefix: Partial<Record<FocusArea, string>> = {
    [FocusArea.Tiles]: "tile-"
  };
