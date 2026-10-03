# Porting the Angular UI to Slint

`iptelly-slint` is a Slint replacement for the Tauri/Angular app in `src-tauri/` and `src/`.
Both use `iptelly-core`, and both play streams by starting mpv or VLC as separate
processes. The two apps share one database, so you can run either while the port is in
progress. When this list is done, `src-tauri/` and `src/` can be removed.

Run it with:

```
cargo run -p iptelly-slint
```

In the lists below, `[x]` means done and `[ ]` means not started. Each item names the
Angular component it replaces, under `src/app/`.

## App shell

- [x] Startup sequence. `main.rs` runs the nuke check, creates or initialises the database
      and reconciles interrupted downloads. Then:
  - [x] Go to setup if there are no sources.
  - [x] `on_start_check_epg` when there is an Xtream source.
  - [x] Run refresh-on-start.
- [x] Toasts.
- [x] Error dialog (`error-modal`): the error text, a Copy button, and where the logs are.
      Opened by clicking an error toast.
- [x] Desktop notifications for EPG reminders (notify-rust). If no notification service is
      running, the reminder shows as a toast instead.
- [x] Single instance (`instance.rs`): a second launch asks the running app to show its
      window, then exits. GNOME on Wayland may only flag the window as wanting attention
      rather than switch to it.
- [x] Remember window size, position and maximized state (`window_state.rs`). Wayland
      doesn't let windows place themselves, so there only the size and maximized state
      come back.
- [ ] Tray icon (Windows and macOS).
- [x] Classic and Modern themes; UI zoom (`zoom.rs`), which multiplies the system's scale
      factor and applies as soon as it's changed.
- What's new dialog (`whats-new-modal`): not ported, by decision. Its notes were last
  updated for open-tv 1.9.1, before the fork.
- [x] Keyboard shortcuts help, opened with F1 (`shortcuts-help-modal`).

## Home (`home/`)

- [x] Nav rail: Favourites, Channels, Movies, Series, History, Downloads, Manage
      Categories, Settings.
- [x] Adult content lock button on the nav rail (`adult-pin-modal`).
- [x] Playlist sidebar: All playlists, each source, and each source's categories, which
      expand and collapse. Selecting a category drills into it.
- [x] Hide a category from its context menu.
- [x] Collapse and resize the playlist sidebar (216 to 416px). Both last until the app
      closes, as in the Angular app.
- [x] Search with a 300 ms debounce.
- [x] Keyword search toggle.
- [x] Sort menu.
- [x] Bulk actions: hide, unhide, favourite, unfavourite.
- [x] Channel grid:
  - [x] Columns fit the window width.
  - [x] Loads the next page of 36 when you scroll near the end.
  - [x] Posters and logos load in the background and are cached on disk.
  - [x] Empty-state messages.
- [x] List layout with an EPG timeline for live channels (`epg-timeline`,
      `epg-timeline-header`), panned 3 hours at a time.
- [ ] EPG keyboard guide mode (Right arrow on a row to step through its programmes).
- [x] Favourites view: favourite channels (as a list with the EPG) in the top 40%, and
      favourite movies and series in a second grid below. One search box and sort drive
      both. Down from the last channel moves into the second grid, and Up from its top row
      comes back.
- [x] Drill into a series, then a season, then a category. A back row returns you.
- [x] Clicking a tile plays it. Clicking again while it starts cancels the play. Playing
      adds the item to history.
- [x] History: a Clear history button.
- [x] Tile context menu:
  - [x] Favourite and unfavourite.
  - [x] Hide and unhide.
  - [x] Remove from history.
  - [x] Copy URL.
  - [x] Record.
  - [x] Download, Download series, Download season, and a progress bar on the tile.
  - [x] Re-stream.
  - [x] Edit, Share and Delete, for custom sources. The grid never shows category tiles,
        so a custom category's Edit, Share and Delete are on its sidebar context menu.
- [x] Keyboard navigation:
  - [x] Ctrl+F (or Ctrl+Space) for search, and again to go back to the grid.
  - [x] Arrow keys, Home and End move between tiles. Up from the top row goes to search,
        and Down from search comes back. Enter plays, and Menu or Shift+F10 opens the
        tile's menu.
  - [x] Esc clears the search, or else goes back a level.
  - [x] Tab moves between the search box, the toolbar buttons and the grid. This uses
        Slint's own focus chain, so the nav rail and the playlist sidebar aren't Tab stops
        yet.
  - [x] Ctrl+A/D/R switch to Channels, History and Favourites. Ctrl+S (the Angular
        app's categories view) has no equivalent: categories are in the sidebar.
  - [x] Ctrl+Q/W/E toggle media types.

## Other screens

- [x] Setup (`setup/`): M3U file, M3U URL, Xtream and Custom sources.
- [x] Setup: a warning for Xtream URLs whose path is `/`.
- [x] Setup: Custom import, Import backup, and the Delete everything button.
- [x] Settings (`settings/`): every setting, the adult PIN, export and import of app data,
      Delete everything (`confirm-delete-modal`). Settings save as you change them.
- [x] Settings: UI zoom, 50% to 300%. The Angular app allowed 10% to 1000%, where the
      window becomes unusable.
- [x] Source tiles (`settings/source-tile/`): enable, edit, refresh, EPG, favourites
      backup and restore, delete, and Share for custom sources.
- [x] Source tiles: Add channel, Add category and Import for custom sources.
- [x] Manage Categories (`manage-categories/`), including Shift+click range selection.
- [x] Downloads (`download-manager/`, `home/download-sidebar/`), including queued downloads
      of whole series and seasons.
- [x] EPG dialog (`epg-modal/`): catch-up playback and download, and reminders.
- [x] Re-stream dialog (`restream-modal/`).
- [x] Custom channel and category dialogs (`edit-channel-modal`, `edit-group-modal`,
      `delete-group-modal`, `import-modal`). The category typeahead is a drop-down of the
      source's categories.

## Angular bugs not to copy

- `edit-channel-modal` sends `channel_exists` its name and URL arguments swapped.
- Deleting a custom channel, or an empty custom category, has no confirmation. The Slint
  app asks first.
- In Settings, the re-stream port and zoom are only saved when you leave the page.
- Deleting a source has no confirmation. The Slint app asks first.
- The EPG reminder bell does nothing when the tray icon is off, which is always the case on
  Linux. The Slint app always allows reminders.
