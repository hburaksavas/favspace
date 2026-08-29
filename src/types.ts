export type ItemKind = "url" | "file" | "folder";
export type ItemStatus = "available" | "missing" | "offline" | "error";

export interface ItemTag {
  id: number;
  name: string;
  color?: string | null;
}

export interface Collection {
  id: number;
  name: string;
  color: string;
  icon: string;
  itemCount: number;
}

export interface Tag extends ItemTag {
  itemCount: number;
}

export interface LibraryItem {
  id: number;
  kind: ItemKind;
  title: string;
  location: string;
  normalizedLocation: string;
  description?: string | null;
  iconDataUrl?: string | null;
  status: ItemStatus;
  favorite: boolean;
  createdAt: number;
  collectionIds: number[];
  tags: ItemTag[];
}
