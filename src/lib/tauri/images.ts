import { invoke } from "./invoke";
import type { ImageItem, Job, Page } from "./jobs";

export interface ImageDetail {
  image: ImageItem;
  job: Job;
}

export interface ImageListOptions {
  query?: string;
  favoritesOnly?: boolean;
  cursor?: string;
  limit?: number;
}

export const imageList = (opts: ImageListOptions = {}) => invoke<Page<ImageItem>>("image_list", { ...opts });

export const imageGet = (id: string) => invoke<ImageDetail>("image_get", { id });

export const imageSetFavorite = (id: string, value: boolean) =>
  invoke<void>("image_set_favorite", { id, value });

/** `deleteFiles` が true なら、ファイルは OS のゴミ箱へ移動される。 */
export const imageDelete = (ids: string[], deleteFiles: boolean) =>
  invoke<void>("image_delete", { ids, deleteFiles });

export const imageExport = (id: string, destPath: string) => invoke<void>("image_export", { id, destPath });

export const imageReveal = (id: string) => invoke<void>("image_reveal", { id });
