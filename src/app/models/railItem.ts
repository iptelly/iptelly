// Numeric values are persisted (Settings > Default view stores one raw), so
// new entries must be appended at the end, never inserted in the middle -
// that would silently reassign an already-saved default_view to the wrong
// section on existing installs.
export enum RailItem {
  Favourites,
  Channels,
  Movies,
  Series,
  History,
  ManageCategories,
  Settings,
  Downloads,
}
