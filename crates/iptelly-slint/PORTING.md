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
- [ ] Error dialog (`error-modal`): the error text, a Copy button, and where the logs are.
- [ ] Desktop notifications for EPG reminders. Today these show as a toast.
- [ ] Single instance.
- [ ] Remember window size and position.
- [ ] Tray icon (Windows and macOS).
- [ ] Classic and Modern themes; UI zoom.
- [ ] What's new dialog (`whats-new-modal`).
- [ ] Keyboard shortcuts help, opened with F1 (`shortcuts-help-modal`).

## Home (`home/`)

- [x] Nav rail: Favourites, Channels, Movies, Series, History, Downloads, Manage
      Categories, Settings.
- [ ] Adult content lock button on the nav rail (`adult-pin-modal`).
- [x] Playlist sidebar: All playlists, each source, and each source's categories, which
      expand and collapse. Selecting a category drills into it.
- [ ] Hide a category from its context menu.
- [ ] Collapse and resize the playlist sidebar.
- [x] Search with a 300 ms debounce.
- [x] Keyword search toggle.
- [x] Sort menu.
- [ ] Bulk actions: hide, unhide, favourite, unfavourite.
- [x] Channel grid:
  - [x] Columns fit the window width.
  - [x] Loads the next page of 36 when you scroll near the end.
  - [x] Posters and logos load in the background and are cached on disk.
  - [x] Empty-state messages.
- [ ] List layout with an EPG timeline for live channels (`epg-timeline`,
      `epg-timeline-header`).
- [ ] Favourites view: a second section for favourite movies and series.
- [x] Drill into a series, then a season, then a category. A back row returns you.
- [x] Clicking a tile plays it. Clicking again while it starts cancels the play. Playing
      adds the item to history.
- [x] History: a Clear history button.
- [ ] Tile context menu:
  - [x] Favourite and unfavourite.
  - [x] Hide and unhide.
  - [x] Remove from history.
  - [x] Copy URL.
  - [ ] Record.
  - [ ] Download, Download series, Download season.
  - [ ] Re-stream.
  - [ ] Edit, Share and Delete, for custom sources.
- [ ] Keyboard navigation:
  - [ ] Ctrl+F for search.
  - [ ] Arrow keys move between tiles.
  - [ ] Esc goes back.
  - [ ] Tab cycles between areas.
  - [ ] Ctrl+A/S/D/R switch views.
  - [ ] Ctrl+Q/W/E toggle media types.

## Other screens

- [x] Setup (`setup/`): M3U file, M3U URL, Xtream and Custom sources.
- [ ] Setup: a warning for Xtream URLs whose path is `/`.
- [ ] Setup: Custom import, Import backup, and the Delete everything button.
- [ ] Settings (`settings/`): every setting, the adult PIN, export and import of app data,
      Delete everything (`confirm-delete-modal`).
- [ ] Source tiles (`settings/source-tile/`): edit, refresh, EPG, favourites
      backup and restore, the custom-source actions.
- [ ] Manage Categories (`manage-categories/`).
- [ ] Downloads (`download-manager/`, `home/download-sidebar/`), including queued downloads
      of whole series and seasons.
- [ ] EPG dialog (`epg-modal/`): catch-up playback and download, and reminders.
- [ ] Re-stream dialog (`restream-modal/`).
- [ ] Custom channel and category dialogs (`edit-channel-modal`, `edit-group-modal`,
      `delete-group-modal`, `import-modal`).

## Angular bugs not to copy

- `edit-channel-modal` sends `channel_exists` its name and URL arguments swapped.
- In Settings, the re-stream port and zoom are only saved when you leave the page.
