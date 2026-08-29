import { invoke, isTauri } from "@tauri-apps/api/core";
import type { Collection, ItemKind, LibraryItem, Tag } from "../types";

const STORAGE_KEY = "favspace.browser-preview.items";
const COLLECTIONS_KEY = "favspace.browser-preview.collections";

function previewItems(): LibraryItem[] {
  try {
    const parsed = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "[]") as LibraryItem[];
    return parsed.map((item) => ({ ...item, collectionIds: item.collectionIds ?? [], tags: item.tags ?? [] }));
  } catch {
    return [];
  }
}

function savePreview(items: LibraryItem[]) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(items));
}

function normalizeUrl(raw: string) {
  const candidate = /^https?:\/\//i.test(raw) ? raw : `https://${raw}`;
  return new URL(candidate).toString();
}

export const desktopAvailable = isTauri();

export async function listItems(): Promise<LibraryItem[]> {
  return desktopAvailable ? invoke("list_items") : previewItems();
}

export async function addUrl(rawUrl: string): Promise<LibraryItem> {
  if (desktopAvailable) return invoke("add_url", { rawUrl });

  const location = normalizeUrl(rawUrl);
  const current = previewItems();
  const existing = current.find((item) => item.normalizedLocation === location);
  if (existing) return existing;

  const url = new URL(location);
  const item: LibraryItem = {
    id: Date.now(),
    kind: "url",
    title: url.hostname.replace(/^www\./, ""),
    location,
    normalizedLocation: location,
    description: "Tarayıcı önizlemesinde eklendi",
    status: "available",
    favorite: false,
    createdAt: Math.floor(Date.now() / 1000),
    collectionIds: [],
    tags: []
  };
  savePreview([item, ...current]);
  return item;
}

export async function addLocalPaths(paths: string[]): Promise<LibraryItem[]> {
  if (!desktopAvailable) return [];
  return invoke("add_local_paths", { paths });
}

export async function toggleFavorite(id: number): Promise<boolean> {
  if (desktopAvailable) return invoke("toggle_favorite", { id });
  const items = previewItems();
  const item = items.find((candidate) => candidate.id === id);
  if (!item) return false;
  item.favorite = !item.favorite;
  savePreview(items);
  return item.favorite;
}

export async function removeItem(id: number): Promise<void> {
  if (desktopAvailable) return invoke("remove_item", { id });
  savePreview(previewItems().filter((item) => item.id !== id));
}

export async function openItem(id: number): Promise<void> {
  if (desktopAvailable) return invoke("open_item", { id });
  const item = previewItems().find((candidate) => candidate.id === id);
  if (item?.kind === "url") window.open(item.location, "_blank", "noopener,noreferrer");
}

export async function revealItem(id: number): Promise<void> {
  if (desktopAvailable) return invoke("reveal_item", { id });
}

export async function refreshLocalItems(): Promise<LibraryItem[]> {
  if (!desktopAvailable) return previewItems().filter((item) => item.kind !== "url");
  return invoke("refresh_local_items");
}

export async function setItemCollection(itemId: number, collectionId: number, assigned: boolean): Promise<LibraryItem> {
  if (desktopAvailable) return invoke("set_item_collection", { itemId, collectionId, assigned });
  const items = previewItems();
  const index = items.findIndex((item) => item.id === itemId);
  if (index < 0) throw new Error("Kaynak bulunamadı.");
  const current = items[index].collectionIds;
  items[index] = {
    ...items[index],
    collectionIds: assigned
      ? [...new Set([...current, collectionId])]
      : current.filter((id) => id !== collectionId)
  };
  savePreview(items);
  return items[index];
}

function previewCollections(): Collection[] {
  try {
    const collections = JSON.parse(localStorage.getItem(COLLECTIONS_KEY) ?? "[]") as Collection[];
    const items = previewItems();
    return collections.map((collection) => ({
      ...collection,
      itemCount: items.filter((item) => item.collectionIds.includes(collection.id)).length
    }));
  } catch {
    return [];
  }
}

export async function listCollections(): Promise<Collection[]> {
  return desktopAvailable ? invoke("list_collections") : previewCollections();
}

export async function createCollection(name: string, color: string, icon: string): Promise<Collection> {
  if (desktopAvailable) return invoke("create_collection", { name, color, icon });
  const collections = previewCollections();
  const collection: Collection = { id: Date.now(), name: name.trim(), color, icon, itemCount: 0 };
  localStorage.setItem(COLLECTIONS_KEY, JSON.stringify([...collections, collection]));
  return collection;
}

export async function deleteCollection(id: number): Promise<void> {
  if (desktopAvailable) return invoke("delete_collection", { id });
  localStorage.setItem(COLLECTIONS_KEY, JSON.stringify(previewCollections().filter((collection) => collection.id !== id)));
  savePreview(previewItems().map((item) => ({
    ...item,
    collectionIds: item.collectionIds.filter((collectionId) => collectionId !== id)
  })));
}

export async function listTags(): Promise<Tag[]> {
  if (desktopAvailable) return invoke("list_tags");
  const counts = new Map<string, Tag>();
  for (const item of previewItems()) {
    for (const tag of item.tags) {
      const current = counts.get(tag.name.toLocaleLowerCase("tr"));
      if (current) current.itemCount += 1;
      else counts.set(tag.name.toLocaleLowerCase("tr"), { ...tag, itemCount: 1 });
    }
  }
  return [...counts.values()].sort((a, b) => a.name.localeCompare(b.name, "tr"));
}

export interface ItemMetadataUpdate {
  title: string;
  description: string;
  collectionIds: number[];
  tagNames: string[];
}

export async function updateItemMetadata(id: number, update: ItemMetadataUpdate): Promise<LibraryItem> {
  if (desktopAvailable) return invoke("update_item_metadata", { id, ...update });
  const items = previewItems();
  const index = items.findIndex((item) => item.id === id);
  if (index < 0) throw new Error("Kaynak bulunamadı.");
  const previousTags = new Map(items.flatMap((item) => item.tags).map((tag) => [tag.name.toLocaleLowerCase("tr"), tag]));
  items[index] = {
    ...items[index],
    title: update.title.trim(),
    description: update.description.trim() || null,
    collectionIds: update.collectionIds,
    tags: update.tagNames.map((name, tagIndex) => previousTags.get(name.toLocaleLowerCase("tr")) ?? ({
      id: Date.now() + tagIndex,
      name,
      color: null
    }))
  };
  savePreview(items);
  return items[index];
}

export function kindLabel(kind: ItemKind) {
  return { url: "Bağlantı", file: "Dosya", folder: "Klasör" }[kind];
}
