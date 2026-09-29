import { MediaType } from "./mediaType";

export class Channel {
  id?: number;
  name?: string;
  group_id?: number;
  image?: string;
  url?: string;
  media_type?: MediaType;
  source_id?: number;
  favorite?: boolean;
  stream_id?: number;
  tv_archive?: boolean;
  hidden?: boolean;
  tvg_id?: string;
  series_id?: number;
  season_id?: number;
  episode_num?: number;
  // Not optional on the Rust side (Channel.is_adult: bool, no #[serde(default)])
  // - a frontend-constructed literal missing this fails the whole "play"/
  // "download" IPC call at the Tauri deserialization boundary, not with a
  // TypeScript error. Kept non-optional here so the compiler catches any
  // new/existing literal missing it, instead of it only surfacing at
  // runtime the first time that code path actually gets exercised.
  is_adult: boolean = false;
}
