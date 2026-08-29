import { FormEvent, useEffect, useMemo, useState, type CSSProperties, type DragEvent } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AppWindow,
  ArrowUpRight,
  Box,
  Check,
  ChevronDown,
  File,
  Folder,
  FolderOpen,
  Globe2,
  Grid2X2,
  Heart,
  Hash,
  Layers3,
  Link2,
  LoaderCircle,
  MoreHorizontal,
  Pencil,
  Plus,
  RefreshCw,
  Save,
  Search,
  Sparkles,
  Tag as TagIcon,
  Trash2,
  X
} from "lucide-react";
import {
  addLocalPaths,
  addUrl,
  createCollection,
  deleteCollection,
  desktopAvailable,
  kindLabel,
  listCollections,
  listItems,
  listTags,
  openItem,
  removeItem,
  refreshLocalItems,
  revealItem,
  setItemCollection,
  toggleFavorite,
  updateItemMetadata
} from "./lib/api";
import type { Collection, ItemKind, LibraryItem, Tag } from "./types";

type SystemFilter = "all" | ItemKind | "favorite";
type Filter = SystemFilter | `collection:${number}`;

const filters: Array<{ id: SystemFilter; label: string; icon: typeof Globe2 }> = [
  { id: "all", label: "Tüm kaynaklar", icon: Grid2X2 },
  { id: "url", label: "Bağlantılar", icon: Globe2 },
  { id: "file", label: "Dosyalar", icon: File },
  { id: "folder", label: "Klasörler", icon: Folder },
  { id: "favorite", label: "Favoriler", icon: Heart }
];

function ItemGlyph({ item }: { item: LibraryItem }) {
  if (item.iconDataUrl) {
    return <img className={item.kind === "url" ? "favicon" : "shell-preview"} src={item.iconDataUrl} alt="" />;
  }
  if (item.kind === "folder") return <Folder size={29} strokeWidth={1.8} />;
  if (item.kind === "file") return <File size={27} strokeWidth={1.8} />;
  return <Globe2 size={27} strokeWidth={1.8} />;
}

function ResourceCard({
  item,
  onFavorite,
  onRemove,
  onEdit,
  onDragStart,
  onDragEnd,
  dragging
}: {
  item: LibraryItem;
  onFavorite: (item: LibraryItem) => void;
  onRemove: (item: LibraryItem) => void;
  onEdit: (item: LibraryItem) => void;
  onDragStart: (event: DragEvent<HTMLElement>, item: LibraryItem) => void;
  onDragEnd: () => void;
  dragging: boolean;
}) {
  const [menuOpen, setMenuOpen] = useState(false);
  const detail = item.kind === "url"
    ? new URL(item.location).hostname.replace(/^www\./, "")
    : item.location;

  return (
    <article
      className={`resource-card kind-${item.kind} ${dragging ? "dragging-card" : ""}`}
      draggable
      onDragStart={(event) => onDragStart(event, item)}
      onDragEnd={onDragEnd}
      onDoubleClick={() => openItem(item.id)}
    >
      <div className="card-shine" />
      <div className="card-topline">
        <div className="item-glyph"><ItemGlyph item={item} /></div>
        <div className="card-actions">
          <button
            className={`icon-button favorite-button ${item.favorite ? "active" : ""}`}
            onClick={() => onFavorite(item)}
            aria-label={item.favorite ? "Favorilerden çıkar" : "Favorilere ekle"}
          >
            <Heart size={17} fill={item.favorite ? "currentColor" : "none"} />
          </button>
          <div className="menu-wrap">
            <button className="icon-button" onClick={() => setMenuOpen((value) => !value)} aria-label="Menü">
              <MoreHorizontal size={18} />
            </button>
            {menuOpen && (
              <div className="card-menu">
                <button onClick={() => openItem(item.id)}><ArrowUpRight size={15} /> Aç</button>
                <button onClick={() => { setMenuOpen(false); onEdit(item); }}><Pencil size={15} /> Düzenle</button>
                {item.kind !== "url" && (
                  <button onClick={() => revealItem(item.id)}><FolderOpen size={15} /> Explorer’da göster</button>
                )}
                <button className="danger" onClick={() => onRemove(item)}><Trash2 size={15} /> Kayıttan kaldır</button>
              </div>
            )}
          </div>
        </div>
      </div>
      <div className="card-copy">
        <span className="kind-chip">{kindLabel(item.kind)}</span>
        <h3>{item.title}</h3>
        <p title={detail}>{detail}</p>
        {item.tags.length > 0 && (
          <div className="card-tags">
            {item.tags.slice(0, 2).map((tag) => <span key={tag.id} style={{ "--tag-color": tag.color ?? "#8b5cf6" } as CSSProperties}>#{tag.name}</span>)}
            {item.tags.length > 2 && <span>+{item.tags.length - 2}</span>}
          </div>
        )}
      </div>
      <div className="card-footer">
        <span className={`status-dot status-${item.status}`} />
        <span>{{ available: "Hazır", missing: "Bulunamadı", offline: "Çevrimdışı", error: "Kontrol gerekli" }[item.status]}</span>
        <button className="open-arrow" onClick={() => openItem(item.id)} aria-label="Aç">
          <ArrowUpRight size={17} />
        </button>
      </div>
    </article>
  );
}

const collectionColors = ["#8b5cf6", "#dc6d67", "#4f8e72", "#c1802c", "#4c7fa8", "#a45d86"];
const collectionIcons = ["sparkles", "folder", "layers", "hash"];

function CollectionGlyph({ icon, size = 16 }: { icon: string; size?: number }) {
  if (icon === "folder") return <Folder size={size} />;
  if (icon === "layers") return <Layers3 size={size} />;
  if (icon === "hash") return <Hash size={size} />;
  return <Sparkles size={size} />;
}

function CollectionModal({
  onClose,
  onCreate
}: {
  onClose: () => void;
  onCreate: (name: string, color: string, icon: string) => Promise<void>;
}) {
  const [name, setName] = useState("");
  const [color, setColor] = useState(collectionColors[0]);
  const [icon, setIcon] = useState(collectionIcons[0]);
  const [saving, setSaving] = useState(false);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!name.trim() || saving) return;
    setSaving(true);
    try { await onCreate(name, color, icon); } finally { setSaving(false); }
  };

  return (
    <div className="modal-backdrop" onMouseDown={onClose}>
      <form className="collection-modal" onSubmit={submit} onMouseDown={(event) => event.stopPropagation()}>
        <div className="modal-heading">
          <div className="modal-symbol" style={{ background: `${color}20`, color }}><CollectionGlyph icon={icon} size={22} /></div>
          <div><span className="overline">Yeni bir köşe</span><h2>Koleksiyon oluştur</h2></div>
          <button type="button" className="modal-close" onClick={onClose}><X size={18} /></button>
        </div>
        <label className="form-label">Koleksiyon adı
          <input autoFocus value={name} maxLength={60} onChange={(event) => setName(event.target.value)} placeholder="Örn. Aktif projeler" />
        </label>
        <div className="option-row">
          <div><span className="form-label-text">Renk</span><div className="color-options">
            {collectionColors.map((entry) => <button type="button" key={entry} className={color === entry ? "selected" : ""} style={{ background: entry }} onClick={() => setColor(entry)} aria-label={entry} />)}
          </div></div>
          <div><span className="form-label-text">İkon</span><div className="icon-options">
            {collectionIcons.map((entry) => <button type="button" key={entry} className={icon === entry ? "selected" : ""} onClick={() => setIcon(entry)}><CollectionGlyph icon={entry} /></button>)}
          </div></div>
        </div>
        <div className="modal-footer"><button type="button" className="secondary-button" onClick={onClose}>Vazgeç</button><button className="primary-button compact" disabled={!name.trim() || saving}>{saving ? <LoaderCircle className="spin" size={17} /> : <Plus size={17} />} Oluştur</button></div>
      </form>
    </div>
  );
}

function ItemEditor({
  item,
  collections,
  availableTags,
  onClose,
  onSave
}: {
  item: LibraryItem;
  collections: Collection[];
  availableTags: Tag[];
  onClose: () => void;
  onSave: (item: LibraryItem, title: string, description: string, collectionIds: number[], tagNames: string[]) => Promise<void>;
}) {
  const [title, setTitle] = useState(item.title);
  const [description, setDescription] = useState(item.description ?? "");
  const [collectionIds, setCollectionIds] = useState(item.collectionIds);
  const [tagNames, setTagNames] = useState(item.tags.map((tag) => tag.name));
  const [tagInput, setTagInput] = useState("");
  const [saving, setSaving] = useState(false);

  const addTag = (raw: string) => {
    const value = raw.trim().replace(/^#/, "");
    if (!value || tagNames.some((name) => name.toLocaleLowerCase("tr") === value.toLocaleLowerCase("tr"))) return;
    setTagNames((current) => [...current, value].slice(0, 20));
    setTagInput("");
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!title.trim() || saving) return;
    setSaving(true);
    try { await onSave(item, title, description, collectionIds, tagNames); } finally { setSaving(false); }
  };

  return (
    <div className="drawer-backdrop" onMouseDown={onClose}>
      <form className="item-drawer" onSubmit={submit} onMouseDown={(event) => event.stopPropagation()}>
        <div className="drawer-header">
          <div className={`item-glyph kind-${item.kind}`}><ItemGlyph item={item} /></div>
          <div><span className="overline">{kindLabel(item.kind)} düzenle</span><h2>{item.title}</h2></div>
          <button type="button" className="modal-close" onClick={onClose}><X size={19} /></button>
        </div>
        <div className="drawer-scroll">
          <label className="form-label">Başlık
            <input value={title} maxLength={180} onChange={(event) => setTitle(event.target.value)} />
          </label>
          <label className="form-label">Not
            <textarea value={description} maxLength={1000} onChange={(event) => setDescription(event.target.value)} placeholder="Bu kaynak neden önemli?" />
            <small>{description.length}/1000</small>
          </label>
          <div className="editor-section">
            <div className="editor-section-title"><Layers3 size={16} /><div><strong>Koleksiyonlar</strong><span>Aynı kaynak birden fazla yerde görünebilir.</span></div></div>
            <div className="collection-checks">
              {collections.length ? collections.map((collection) => {
                const checked = collectionIds.includes(collection.id);
                return <button type="button" key={collection.id} className={checked ? "checked" : ""} onClick={() => setCollectionIds((current) => checked ? current.filter((id) => id !== collection.id) : [...current, collection.id])}>
                  <span style={{ background: `${collection.color}20`, color: collection.color }}><CollectionGlyph icon={collection.icon} /></span><em>{collection.name}</em>{checked && <Check size={16} />}
                </button>;
              }) : <p className="inline-empty">Henüz koleksiyon yok. Sidebar’daki + ile oluşturabilirsin.</p>}
            </div>
          </div>
          <div className="editor-section">
            <div className="editor-section-title"><TagIcon size={16} /><div><strong>Etiketler</strong><span>Enter ile yeni bir sanal etiket ekle.</span></div></div>
            <div className="tag-editor">
              {tagNames.map((name) => <span key={name}>#{name}<button type="button" onClick={() => setTagNames((current) => current.filter((entry) => entry !== name))}><X size={12} /></button></span>)}
              <input value={tagInput} maxLength={40} onChange={(event) => setTagInput(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter" || event.key === ",") { event.preventDefault(); addTag(tagInput); } }} onBlur={() => addTag(tagInput)} placeholder={tagNames.length ? "başka..." : "etiket yaz..."} />
            </div>
            <div className="tag-suggestions">
              {availableTags.filter((tag) => !tagNames.some((name) => name.toLocaleLowerCase("tr") === tag.name.toLocaleLowerCase("tr"))).slice(0, 8).map((tag) => <button type="button" key={tag.id} onClick={() => addTag(tag.name)}>+ #{tag.name}</button>)}
            </div>
          </div>
        </div>
        <div className="drawer-footer"><span>Fiziksel konum değişmeyecek.</span><button className="primary-button compact" disabled={!title.trim() || saving}>{saving ? <LoaderCircle className="spin" size={17} /> : <Save size={17} />} Kaydet</button></div>
      </form>
    </div>
  );
}

export default function App() {
  const [items, setItems] = useState<LibraryItem[]>([]);
  const [collections, setCollections] = useState<Collection[]>([]);
  const [tags, setTags] = useState<Tag[]>([]);
  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");
  const [url, setUrl] = useState("");
  const [loading, setLoading] = useState(true);
  const [adding, setAdding] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [draggingItemId, setDraggingItemId] = useState<number | null>(null);
  const [dropCollectionId, setDropCollectionId] = useState<number | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [collectionModalOpen, setCollectionModalOpen] = useState(false);
  const [editingItem, setEditingItem] = useState<LibraryItem | null>(null);

  const refreshResources = async (quiet = false) => {
    if (!desktopAvailable || refreshing) return;
    setRefreshing(true);
    try {
      const refreshed = await refreshLocalItems();
      const updates = new Map(refreshed.map((item) => [item.id, item]));
      setItems((current) => current.map((item) => updates.get(item.id) ?? item));
      if (!quiet) setNotice(`${refreshed.length} yerel kaynak kontrol edildi.`);
    } catch (error) {
      setNotice(String(error));
    } finally {
      setRefreshing(false);
    }
  };

  const load = async () => {
    try {
      const [nextItems, nextCollections, nextTags] = await Promise.all([listItems(), listCollections(), listTags()]);
      setItems(nextItems);
      setCollections(nextCollections);
      setTags(nextTags);
      if (desktopAvailable) window.setTimeout(() => void refreshResources(true), 0);
    } catch (error) {
      setNotice(String(error));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { void load(); }, []);

  useEffect(() => {
    if (!desktopAvailable) return;
    let unlisten: (() => void) | undefined;
    getCurrentWebviewWindow().onDragDropEvent(async (event) => {
      if (event.payload.type === "enter" || event.payload.type === "over") setDragging(true);
      if (event.payload.type === "leave") setDragging(false);
      if (event.payload.type === "drop") {
        setDragging(false);
        try {
          const added = await addLocalPaths(event.payload.paths);
          setItems((current) => [...added, ...current.filter((item) => !added.some((next) => next.id === item.id))]);
          setNotice(`${added.length} kaynak kütüphaneye eklendi.`);
        } catch (error) {
          setNotice(String(error));
        }
      }
    }).then((stop) => { unlisten = stop; });
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    if (!notice) return;
    const timeout = window.setTimeout(() => setNotice(null), 3600);
    return () => window.clearTimeout(timeout);
  }, [notice]);

  const visibleItems = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase("tr");
    return items.filter((item) => {
      const collectionId = filter.startsWith("collection:") ? Number(filter.split(":")[1]) : null;
      const inFilter = filter === "all"
        || (filter === "favorite" ? item.favorite : collectionId !== null ? item.collectionIds.includes(collectionId) : item.kind === filter);
      const inSearch = !needle || `${item.title} ${item.location} ${item.description ?? ""} ${item.tags.map((tag) => tag.name).join(" ")}`.toLocaleLowerCase("tr").includes(needle);
      return inFilter && inSearch;
    });
  }, [items, filter, query]);

  const counts = useMemo(() => ({
    all: items.length,
    url: items.filter((item) => item.kind === "url").length,
    file: items.filter((item) => item.kind === "file").length,
    folder: items.filter((item) => item.kind === "folder").length,
    favorite: items.filter((item) => item.favorite).length
  }), [items]);

  const activeLabel = filter.startsWith("collection:")
    ? collections.find((collection) => collection.id === Number(filter.split(":")[1]))?.name ?? "Koleksiyon"
    : filters.find((entry) => entry.id === filter)?.label ?? "Kütüphane";

  const submitUrl = async (event: FormEvent) => {
    event.preventDefault();
    if (!url.trim() || adding) return;
    setAdding(true);
    try {
      const item = await addUrl(url.trim());
      setItems((current) => [item, ...current.filter((candidate) => candidate.id !== item.id)]);
      setUrl("");
      setNotice("Bağlantı yakalandı ve kütüphaneye eklendi.");
    } catch (error) {
      setNotice(String(error));
    } finally {
      setAdding(false);
    }
  };

  const pickPaths = async (kind: "file" | "folder") => {
    if (!desktopAvailable) {
      setNotice("Dosya seçimi masaüstü uygulamasında kullanılabilir.");
      return;
    }
    const selected = await open({ directory: kind === "folder", multiple: true });
    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];
    try {
      const added = await addLocalPaths(paths);
      setItems((current) => [...added, ...current.filter((item) => !added.some((next) => next.id === item.id))]);
      setNotice(`${added.length} kaynak kütüphaneye eklendi.`);
    } catch (error) {
      setNotice(String(error));
    }
  };

  const favorite = async (item: LibraryItem) => {
    const value = await toggleFavorite(item.id);
    setItems((current) => current.map((candidate) => candidate.id === item.id ? { ...candidate, favorite: value } : candidate));
  };

  const remove = async (item: LibraryItem) => {
    await removeItem(item.id);
    setItems((current) => current.filter((candidate) => candidate.id !== item.id));
    setNotice("Kaynak yalnızca kütüphaneden kaldırıldı; fiziksel dosyaya dokunulmadı.");
  };

  const addCollection = async (name: string, color: string, icon: string) => {
    try {
      const collection = await createCollection(name, color, icon);
      setCollections((current) => [...current, collection]);
      setCollectionModalOpen(false);
      setFilter(`collection:${collection.id}`);
      setNotice(`“${collection.name}” koleksiyonu oluşturuldu.`);
    } catch (error) {
      setNotice(String(error));
    }
  };

  const removeCollection = async (collection: Collection) => {
    if (!window.confirm(`“${collection.name}” koleksiyonu kaldırılsın mı? Kaynaklar silinmeyecek.`)) return;
    await deleteCollection(collection.id);
    setCollections((current) => current.filter((entry) => entry.id !== collection.id));
    setItems((current) => current.map((item) => ({ ...item, collectionIds: item.collectionIds.filter((id) => id !== collection.id) })));
    if (filter === `collection:${collection.id}`) setFilter("all");
    setNotice("Koleksiyon kaldırıldı; kaynaklar kütüphanede duruyor.");
  };

  const saveItem = async (item: LibraryItem, title: string, description: string, collectionIds: number[], tagNames: string[]) => {
    try {
      const updated = await updateItemMetadata(item.id, { title, description, collectionIds, tagNames });
      setItems((current) => current.map((entry) => entry.id === updated.id ? updated : entry));
      const [nextCollections, nextTags] = await Promise.all([listCollections(), listTags()]);
      setCollections(nextCollections);
      setTags(nextTags);
      setEditingItem(null);
      setNotice("Kaynak düzeni kaydedildi.");
    } catch (error) {
      setNotice(String(error));
    }
  };

  const startCardDrag = (event: DragEvent<HTMLElement>, item: LibraryItem) => {
    event.dataTransfer.effectAllowed = "copy";
    event.dataTransfer.setData("application/x-favspace-item", String(item.id));
    event.dataTransfer.setData("text/plain", item.title);
    setDraggingItemId(item.id);
  };

  const finishCardDrag = () => {
    setDraggingItemId(null);
    setDropCollectionId(null);
  };

  const dropOnCollection = async (event: DragEvent<HTMLElement>, collection: Collection) => {
    event.preventDefault();
    const fromTransfer = Number(event.dataTransfer.getData("application/x-favspace-item"));
    const itemId = Number.isFinite(fromTransfer) && fromTransfer > 0 ? fromTransfer : draggingItemId;
    finishCardDrag();
    if (!itemId) return;
    const item = items.find((entry) => entry.id === itemId);
    if (!item || item.collectionIds.includes(collection.id)) {
      setNotice(item ? `“${item.title}” zaten bu koleksiyonda.` : "Sürüklenen kaynak bulunamadı.");
      return;
    }
    try {
      const updated = await setItemCollection(itemId, collection.id, true);
      setItems((current) => current.map((entry) => entry.id === updated.id ? updated : entry));
      setCollections(await listCollections());
      setNotice(`“${updated.title}” → ${collection.name}`);
    } catch (error) {
      setNotice(String(error));
    }
  };

  return (
    <main className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark"><Sparkles size={21} /></div>
          <div><strong>favspace</strong><span>your tiny universe</span></div>
        </div>

        <nav className="nav-list">
          <p className="eyebrow">Kütüphane</p>
          {filters.map((entry) => {
            const Icon = entry.icon;
            return (
              <button key={entry.id} className={filter === entry.id ? "active" : ""} onClick={() => setFilter(entry.id)}>
                <Icon size={18} />
                <span>{entry.label}</span>
                <em>{counts[entry.id]}</em>
              </button>
            );
          })}
        </nav>

        <div className="collection-heading">
          <p className="eyebrow">Koleksiyonlar</p>
          <button onClick={() => setCollectionModalOpen(true)} aria-label="Koleksiyon oluştur"><Plus size={15} /></button>
        </div>
        <nav className="collection-list">
          {collections.map((collection) => (
            <div
              className={`collection-nav-row ${dropCollectionId === collection.id ? "drop-target" : ""}`}
              key={collection.id}
              onDragEnter={(event) => { if (draggingItemId) { event.preventDefault(); setDropCollectionId(collection.id); } }}
              onDragOver={(event) => { if (draggingItemId) { event.preventDefault(); event.dataTransfer.dropEffect = "copy"; } }}
              onDragLeave={(event) => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDropCollectionId(null); }}
              onDrop={(event) => void dropOnCollection(event, collection)}
            >
              <button className={filter === `collection:${collection.id}` ? "active" : ""} onClick={() => setFilter(`collection:${collection.id}`)}>
                <span className="collection-glyph" style={{ background: `${collection.color}20`, color: collection.color }}><CollectionGlyph icon={collection.icon} /></span>
                <span>{collection.name}</span>
                <em>{collection.itemCount}</em>
              </button>
              <button className="collection-delete" onClick={() => removeCollection(collection)} aria-label="Koleksiyonu kaldır"><X size={12} /></button>
            </div>
          ))}
          {!collections.length && <button className="new-collection-row" onClick={() => setCollectionModalOpen(true)}><Plus size={15} /> İlk koleksiyonu oluştur</button>}
        </nav>

        <div className="sidebar-spacer" />
        <div className="drop-hint">
          <Box size={23} />
          <strong>Buraya bırak</strong>
          <span>Dosya ve klasörleri taşımadan düzenle.</span>
        </div>
        <div className="local-badge"><span /> Yalnızca bu cihazda</div>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div className="search-box">
            <Search size={19} />
            <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Kaynaklarında ara..." />
            <kbd>Ctrl K</kbd>
          </div>
          <div className="topbar-actions">
            <button className="refresh-button" onClick={() => void refreshResources()} disabled={refreshing} title="Yerel kaynakları kontrol et"><RefreshCw className={refreshing ? "spin" : ""} size={17} /></button>
            <div className="avatar">FD</div>
          </div>
        </header>

        <div className="content">
          <section className="hero">
            <div className="hero-copy">
              <span className="overline"><Sparkles size={14} /> Kişisel kaynak evrenin</span>
              <h1>Her şey yerli yerinde.<br /><i>Hiçbir şeyi taşımadan.</i></h1>
              <p>Bağlantıları, dosyaları ve klasörleri tek bir canlı kütüphanede buluştur.</p>
            </div>
            <div className="hero-orbit" aria-hidden="true">
              <span className="orbit orbit-one"><Globe2 /></span>
              <span className="orbit orbit-two"><Folder /></span>
              <span className="orbit orbit-three"><File /></span>
              <div className="orbit-core"><AppWindow /></div>
            </div>
          </section>

          <section className="capture-panel">
            <form onSubmit={submitUrl}>
              <div className="url-field">
                <Link2 size={20} />
                <input value={url} onChange={(event) => setUrl(event.target.value)} placeholder="Bir URL yapıştır: example.com/inspiration" />
                {url && <button type="button" className="clear-button" onClick={() => setUrl("")}><X size={16} /></button>}
              </div>
              <button className="primary-button" disabled={!url.trim() || adding}>
                {adding ? <LoaderCircle className="spin" size={18} /> : <Plus size={18} />}
                Yakala
              </button>
            </form>
            <div className="capture-divider"><span>veya yerelden</span></div>
            <div className="local-actions">
              <button onClick={() => pickPaths("file")}><File size={18} /> Dosya ekle</button>
              <button onClick={() => pickPaths("folder")}><Folder size={18} /> Klasör ekle</button>
              <span>sürükleyip bırakabilirsin</span>
            </div>
          </section>

          <section className="library-section">
            <div className="section-heading">
              <div>
                <span className="overline">Koleksiyon</span>
                <h2>{activeLabel}</h2>
              </div>
              <button className="sort-button">En yeni <ChevronDown size={15} /></button>
            </div>

            {loading ? (
              <div className="loading-state"><LoaderCircle className="spin" /> Kütüphane hazırlanıyor...</div>
            ) : visibleItems.length ? (
              <div className="resource-grid">
                {visibleItems.map((item) => (
                  <ResourceCard
                    key={item.id}
                    item={item}
                    onFavorite={favorite}
                    onRemove={remove}
                    onEdit={setEditingItem}
                    onDragStart={startCardDrag}
                    onDragEnd={finishCardDrag}
                    dragging={draggingItemId === item.id}
                  />
                ))}
                <button className="add-card" onClick={() => document.querySelector<HTMLInputElement>(".url-field input")?.focus()}>
                  <span><Plus size={23} /></span>
                  <strong>Yeni kaynak</strong>
                  <small>URL, dosya veya klasör</small>
                </button>
              </div>
            ) : (
              <div className="empty-state">
                <div className="empty-icon"><Sparkles size={29} /></div>
                <h3>{query ? "Aradığın kaynak burada değil" : "Bu köşe henüz boş"}</h3>
                <p>{query ? "Başka bir kelime dene veya filtreyi değiştir." : "İlk bağlantını yakala ya da bir klasörü buraya bırak."}</p>
                {!query && <button onClick={() => document.querySelector<HTMLInputElement>(".url-field input")?.focus()}><Plus size={17} /> İlk kaynağı ekle</button>}
              </div>
            )}
          </section>
        </div>
      </section>

      {dragging && (
        <div className="drop-overlay">
          <div><FolderOpen size={44} /><h2>Tamam, bırak gitsin!</h2><p>Kaynaklarını taşımadan kütüphanene ekleyeceğiz.</p></div>
        </div>
      )}

      {collectionModalOpen && <CollectionModal onClose={() => setCollectionModalOpen(false)} onCreate={addCollection} />}
      {editingItem && <ItemEditor item={editingItem} collections={collections} availableTags={tags} onClose={() => setEditingItem(null)} onSave={saveItem} />}

      {notice && <div className="toast"><Check size={17} /><span>{notice}</span><button onClick={() => setNotice(null)}><X size={15} /></button></div>}
    </main>
  );
}
