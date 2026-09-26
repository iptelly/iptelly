export class DownloadHistoryItem {
  id!: string;
  channel_id?: number;
  source_id?: number;
  name!: string;
  path!: string;
  status!: "downloading" | "paused" | "completed" | "cancelled" | "failed";
  downloaded_bytes!: number;
  total_bytes?: number;
  created_at!: number;
  updated_at!: number;
}
