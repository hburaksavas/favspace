use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rusqlite::{params, Connection, OptionalExtension};
use scraper::{Html, Selector};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    net::{IpAddr, ToSocketAddrs},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{Manager, State};
use url::Url;

#[cfg(target_os = "windows")]
mod windows_thumbnail;

struct AppState {
    database: Mutex<Connection>,
    icon_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LibraryItem {
    id: i64,
    kind: String,
    title: String,
    location: String,
    normalized_location: String,
    description: Option<String>,
    icon_data_url: Option<String>,
    status: String,
    favorite: bool,
    created_at: i64,
    collection_ids: Vec<i64>,
    tags: Vec<ItemTag>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ItemTag {
    id: i64,
    name: String,
    color: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CollectionSummary {
    id: i64,
    name: String,
    color: String,
    icon: String,
    item_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TagSummary {
    id: i64,
    name: String,
    color: Option<String>,
    item_count: i64,
}

#[derive(Default)]
struct UrlMetadata {
    title: Option<String>,
    description: Option<String>,
    icon_path: Option<String>,
}

struct LocalCandidate {
    kind: &'static str,
    title: String,
    location: String,
    normalized: String,
    icon_path: Option<String>,
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn local_resource_status(path: &Path) -> &'static str {
    if path.exists() {
        return "available";
    }
    let raw = path.to_string_lossy();
    if raw.starts_with("\\\\") {
        return "offline";
    }
    let bytes = raw.as_bytes();
    if bytes.len() >= 3 && bytes[1] == b':' {
        let drive_root = format!("{}:\\", bytes[0] as char);
        if !Path::new(&drive_root).exists() {
            return "offline";
        }
    }
    "missing"
}

fn cache_local_shell_image(path: &Path, icon_dir: &Path) -> Option<String> {
    if !path.exists() {
        return None;
    }
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().to_lowercase().as_bytes());
    if let Ok(metadata) = fs::metadata(path) {
        hasher.update(metadata.len().to_le_bytes());
        if let Ok(modified) = metadata.modified() {
            if let Ok(duration) = modified.duration_since(UNIX_EPOCH) {
                hasher.update(duration.as_secs().to_le_bytes());
            }
        }
    }
    let target = icon_dir.join(format!("shell-{}.png", hex::encode(hasher.finalize())));
    if target.exists() {
        return Some(target.to_string_lossy().into_owned());
    }

    #[cfg(target_os = "windows")]
    {
        let png = windows_thumbnail::extract_png(path, 128).ok()?;
        fs::write(&target, png).ok()?;
        Some(target.to_string_lossy().into_owned())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = target;
        None
    }
}

fn initialize_database(connection: &Connection) -> Result<(), rusqlite::Error> {
    connection.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS items (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            kind                TEXT NOT NULL CHECK(kind IN ('url', 'file', 'folder')),
            title               TEXT NOT NULL,
            location            TEXT NOT NULL,
            normalized_location TEXT NOT NULL UNIQUE,
            description         TEXT,
            icon_path           TEXT,
            status              TEXT NOT NULL DEFAULT 'available',
            favorite            INTEGER NOT NULL DEFAULT 0,
            created_at          INTEGER NOT NULL,
            last_checked_at     INTEGER
        );

        CREATE TABLE IF NOT EXISTS collections (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT NOT NULL,
            color       TEXT,
            icon        TEXT,
            created_at  INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS collection_items (
            collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
            item_id       INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
            position      INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (collection_id, item_id)
        );

        CREATE TABLE IF NOT EXISTS tags (
            id      INTEGER PRIMARY KEY AUTOINCREMENT,
            name    TEXT NOT NULL UNIQUE,
            color   TEXT
        );

        CREATE TABLE IF NOT EXISTS item_tags (
            item_id INTEGER NOT NULL REFERENCES items(id) ON DELETE CASCADE,
            tag_id  INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
            PRIMARY KEY (item_id, tag_id)
        );
        "#,
    )?;
    Ok(())
}

fn normalize_url(raw: &str) -> Result<Url, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("URL boş olamaz.".into());
    }
    let lower = trimmed.to_ascii_lowercase();
    let candidate = if lower.starts_with("http://") || lower.starts_with("https://") {
        trimmed.to_string()
    } else if trimmed.contains("://")
        || lower.starts_with("file:")
        || lower.starts_with("javascript:")
        || lower.starts_with("data:")
        || lower.starts_with("mailto:")
    {
        return Err("Yalnızca http ve https bağlantıları destekleniyor.".into());
    } else {
        format!("https://{trimmed}")
    };
    let mut url = Url::parse(&candidate).map_err(|_| "Geçerli bir URL girin.".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Yalnızca http ve https bağlantıları destekleniyor.".into());
    }
    if url.host_str().is_none() {
        return Err("URL içinde geçerli bir alan adı bulunamadı.".into());
    }
    url.set_fragment(None);
    Ok(url)
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let octets = ip.octets();
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_unspecified()
                && !ip.is_multicast()
                && !(octets[0] == 100 && (64..=127).contains(&octets[1]))
        }
        IpAddr::V6(ip) => {
            !ip.is_loopback()
                && !ip.is_unspecified()
                && !ip.is_multicast()
                && !ip.is_unique_local()
                && !ip.is_unicast_link_local()
        }
    }
}

fn metadata_fetch_allowed(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let normalized_host = host.trim_matches(['[', ']']).to_ascii_lowercase();
    if normalized_host == "localhost"
        || normalized_host.ends_with(".localhost")
        || normalized_host.ends_with(".local")
    {
        return false;
    }
    if let Ok(ip) = normalized_host.parse::<IpAddr>() {
        return is_public_ip(ip);
    }
    let port = url.port_or_known_default().unwrap_or(443);
    match (host, port).to_socket_addrs() {
        Ok(addresses) => addresses
            .into_iter()
            .all(|address| is_public_ip(address.ip())),
        Err(_) => false,
    }
}

async fn fetch_public_response(client: &reqwest::Client, start: Url) -> Option<reqwest::Response> {
    let mut current = start;
    for _ in 0..=5 {
        if !metadata_fetch_allowed(&current) {
            return None;
        }
        let response = client.get(current.clone()).send().await.ok()?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)?
                .to_str()
                .ok()?;
            current = current.join(location).ok()?;
            continue;
        }
        return response.error_for_status().ok();
    }
    None
}

fn icon_as_data_url(path: Option<String>) -> Option<String> {
    let path = path?;
    let bytes = fs::read(&path).ok()?;
    let mime = mime_guess::from_path(&path).first_or_octet_stream();
    Some(format!(
        "data:{};base64,{}",
        mime.essence_str(),
        BASE64.encode(bytes)
    ))
}

fn row_to_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<LibraryItem> {
    let icon_path: Option<String> = row.get(6)?;
    Ok(LibraryItem {
        id: row.get(0)?,
        kind: row.get(1)?,
        title: row.get(2)?,
        location: row.get(3)?,
        normalized_location: row.get(4)?,
        description: row.get(5)?,
        icon_data_url: icon_as_data_url(icon_path),
        status: row.get(7)?,
        favorite: row.get::<_, i64>(8)? != 0,
        created_at: row.get(9)?,
        collection_ids: Vec::new(),
        tags: Vec::new(),
    })
}

fn hydrate_item_relations(connection: &Connection, item: &mut LibraryItem) -> Result<(), String> {
    let mut collection_statement = connection
        .prepare("SELECT collection_id FROM collection_items WHERE item_id = ?1 ORDER BY position, collection_id")
        .map_err(|error| error.to_string())?;
    item.collection_ids = collection_statement
        .query_map([item.id], |row| row.get(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    let mut tag_statement = connection
        .prepare("SELECT tags.id, tags.name, tags.color FROM tags INNER JOIN item_tags ON item_tags.tag_id = tags.id WHERE item_tags.item_id = ?1 ORDER BY tags.name COLLATE NOCASE")
        .map_err(|error| error.to_string())?;
    item.tags = tag_statement
        .query_map([item.id], |row| {
            Ok(ItemTag {
                id: row.get(0)?,
                name: row.get(1)?,
                color: row.get(2)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn select_item(connection: &Connection, id: i64) -> Result<LibraryItem, String> {
    let mut item = connection
        .query_row(
            "SELECT id, kind, title, location, normalized_location, description, icon_path, status, favorite, created_at FROM items WHERE id = ?1",
            [id],
            row_to_item,
        )
        .map_err(|error| error.to_string())?;
    hydrate_item_relations(connection, &mut item)?;
    Ok(item)
}

#[tauri::command]
fn list_items(state: State<'_, AppState>) -> Result<Vec<LibraryItem>, String> {
    let connection = state
        .database
        .lock()
        .map_err(|_| "Veritabanı kilidi alınamadı.".to_string())?;
    let mut items = {
        let mut statement = connection
            .prepare("SELECT id, kind, title, location, normalized_location, description, icon_path, status, favorite, created_at FROM items ORDER BY created_at DESC, id DESC")
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([], row_to_item)
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
    };
    for item in &mut items {
        hydrate_item_relations(&connection, item)?;
    }
    Ok(items)
}

#[tauri::command]
fn list_collections(state: State<'_, AppState>) -> Result<Vec<CollectionSummary>, String> {
    let connection = state
        .database
        .lock()
        .map_err(|_| "Veritabanı kilidi alınamadı.".to_string())?;
    let mut statement = connection
        .prepare(
            "SELECT collections.id, collections.name, COALESCE(collections.color, '#8b5cf6'), COALESCE(collections.icon, 'sparkles'), COUNT(collection_items.item_id) FROM collections LEFT JOIN collection_items ON collection_items.collection_id = collections.id GROUP BY collections.id ORDER BY collections.created_at, collections.name COLLATE NOCASE",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(CollectionSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                color: row.get(2)?,
                icon: row.get(3)?,
                item_count: row.get(4)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn create_collection(
    name: String,
    color: String,
    icon: String,
    state: State<'_, AppState>,
) -> Result<CollectionSummary, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 60 {
        return Err("Koleksiyon adı 1–60 karakter olmalı.".into());
    }
    let color = if color.starts_with('#') && color.len() == 7 {
        color
    } else {
        "#8b5cf6".into()
    };
    let icon = if icon.trim().is_empty() {
        "sparkles".into()
    } else {
        icon
    };
    let connection = state
        .database
        .lock()
        .map_err(|_| "Veritabanı kilidi alınamadı.".to_string())?;
    connection
        .execute(
            "INSERT INTO collections (name, color, icon, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![name, color, icon, now_unix()],
        )
        .map_err(|error| error.to_string())?;
    Ok(CollectionSummary {
        id: connection.last_insert_rowid(),
        name: name.to_string(),
        color,
        icon,
        item_count: 0,
    })
}

#[tauri::command]
fn delete_collection(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let connection = state
        .database
        .lock()
        .map_err(|_| "Veritabanı kilidi alınamadı.".to_string())?;
    connection
        .execute("DELETE FROM collections WHERE id = ?1", [id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn list_tags(state: State<'_, AppState>) -> Result<Vec<TagSummary>, String> {
    let connection = state
        .database
        .lock()
        .map_err(|_| "Veritabanı kilidi alınamadı.".to_string())?;
    let mut statement = connection
        .prepare("SELECT tags.id, tags.name, tags.color, COUNT(item_tags.item_id) FROM tags LEFT JOIN item_tags ON item_tags.tag_id = tags.id GROUP BY tags.id ORDER BY tags.name COLLATE NOCASE")
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(TagSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                color: row.get(2)?,
                item_count: row.get(3)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn tag_color(name: &str) -> &'static str {
    const COLORS: [&str; 6] = [
        "#8b5cf6", "#dc6d67", "#4f8e72", "#c1802c", "#4c7fa8", "#a45d86",
    ];
    let hash = name.bytes().fold(0usize, |value, byte| {
        value.wrapping_mul(31).wrapping_add(byte as usize)
    });
    COLORS[hash % COLORS.len()]
}

#[tauri::command]
fn update_item_metadata(
    id: i64,
    title: String,
    description: String,
    collection_ids: Vec<i64>,
    tag_names: Vec<String>,
    state: State<'_, AppState>,
) -> Result<LibraryItem, String> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > 180 {
        return Err("Başlık 1–180 karakter olmalı.".into());
    }
    let description = description.trim();
    if description.chars().count() > 1_000 {
        return Err("Not alanı en fazla 1000 karakter olabilir.".into());
    }
    let mut connection = state
        .database
        .lock()
        .map_err(|_| "Veritabanı kilidi alınamadı.".to_string())?;
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "UPDATE items SET title = ?1, description = NULLIF(?2, '') WHERE id = ?3",
            params![title, description, id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute("DELETE FROM collection_items WHERE item_id = ?1", [id])
        .map_err(|error| error.to_string())?;
    for (position, collection_id) in collection_ids.into_iter().take(30).enumerate() {
        transaction
            .execute(
                "INSERT OR IGNORE INTO collection_items (collection_id, item_id, position) VALUES (?1, ?2, ?3)",
                params![collection_id, id, position as i64],
            )
            .map_err(|error| error.to_string())?;
    }
    transaction
        .execute("DELETE FROM item_tags WHERE item_id = ?1", [id])
        .map_err(|error| error.to_string())?;
    for raw_name in tag_names.into_iter().take(20) {
        let name = raw_name.trim();
        if name.is_empty() || name.chars().count() > 40 {
            continue;
        }
        let existing = transaction
            .query_row(
                "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
                [name],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let tag_id = if let Some(tag_id) = existing {
            tag_id
        } else {
            transaction
                .execute(
                    "INSERT INTO tags (name, color) VALUES (?1, ?2)",
                    params![name, tag_color(name)],
                )
                .map_err(|error| error.to_string())?;
            transaction.last_insert_rowid()
        };
        transaction
            .execute(
                "INSERT OR IGNORE INTO item_tags (item_id, tag_id) VALUES (?1, ?2)",
                params![id, tag_id],
            )
            .map_err(|error| error.to_string())?;
    }
    transaction.commit().map_err(|error| error.to_string())?;
    select_item(&connection, id)
}

async fn fetch_url_metadata(url: &Url, icon_dir: &Path) -> UrlMetadata {
    if !metadata_fetch_allowed(url) {
        return UrlMetadata::default();
    }
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(9))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("Favspace/0.1 (+local resource library)")
        .build()
    {
        Ok(client) => client,
        Err(_) => return UrlMetadata::default(),
    };

    let response = match fetch_public_response(&client, url.clone()).await {
        Some(response) => response,
        None => return UrlMetadata::default(),
    };
    if response.content_length().unwrap_or(0) > 1_500_000 {
        return UrlMetadata::default();
    }
    let html = match response.bytes().await {
        Ok(bytes) if bytes.len() <= 1_500_000 => String::from_utf8_lossy(&bytes).into_owned(),
        _ => return UrlMetadata::default(),
    };

    // `scraper::Html` is intentionally kept inside this synchronous block. It is
    // not Send, so no parsed DOM value may live across the favicon network awaits.
    let (title, description, mut icon_urls) = {
        let document = Html::parse_document(&html);
        let title_selector = Selector::parse("title").expect("valid title selector");
        let meta_selector =
            Selector::parse("meta[name='description'], meta[property='og:description']")
                .expect("valid meta selector");
        let icon_selector = Selector::parse(
            "link[rel~='icon'], link[rel='shortcut icon'], link[rel='apple-touch-icon']",
        )
        .expect("valid icon selector");

        let title = document
            .select(&title_selector)
            .next()
            .map(|node| node.text().collect::<String>().trim().to_string())
            .filter(|value| !value.is_empty())
            .map(|value| value.chars().take(180).collect());
        let description = document
            .select(&meta_selector)
            .find_map(|node| node.value().attr("content"))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.chars().take(320).collect());
        let icon_urls: Vec<Url> = document
            .select(&icon_selector)
            .filter_map(|node| node.value().attr("href"))
            .filter_map(|href| url.join(href).ok())
            .filter(|candidate| matches!(candidate.scheme(), "http" | "https"))
            .collect();
        (title, description, icon_urls)
    };
    if let Ok(fallback) = url.join("/favicon.ico") {
        icon_urls.push(fallback);
    }

    let mut icon_path = None;
    for icon_url in icon_urls.into_iter().take(6) {
        if !metadata_fetch_allowed(&icon_url) {
            continue;
        }
        let response = match fetch_public_response(&client, icon_url.clone()).await {
            Some(response) => response,
            None => continue,
        };
        if response.content_length().unwrap_or(0) > 600_000 {
            continue;
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|header| header.to_str().ok())
            .unwrap_or("application/octet-stream")
            .split(';')
            .next()
            .unwrap_or("application/octet-stream")
            .to_string();
        let bytes = match response.bytes().await {
            Ok(bytes) if !bytes.is_empty() && bytes.len() <= 600_000 => bytes,
            _ => continue,
        };
        let extension = match content_type.as_str() {
            "image/png" => "png",
            "image/jpeg" => "jpg",
            "image/svg+xml" => "svg",
            "image/webp" => "webp",
            "image/gif" => "gif",
            "image/x-icon" | "image/vnd.microsoft.icon" => "ico",
            _ if icon_url.path().ends_with(".png") => "png",
            _ if icon_url.path().ends_with(".svg") => "svg",
            _ => "ico",
        };
        let hash = hex::encode(Sha256::digest(icon_url.as_str().as_bytes()));
        let target = icon_dir.join(format!("{hash}.{extension}"));
        if fs::write(&target, &bytes).is_ok() {
            icon_path = Some(target.to_string_lossy().into_owned());
            break;
        }
    }

    UrlMetadata {
        title,
        description,
        icon_path,
    }
}

#[tauri::command]
async fn add_url(raw_url: String, state: State<'_, AppState>) -> Result<LibraryItem, String> {
    let url = normalize_url(&raw_url)?;
    let normalized = url.to_string();

    {
        let connection = state
            .database
            .lock()
            .map_err(|_| "Veritabanı kilidi alınamadı.".to_string())?;
        let existing = connection
            .query_row(
                "SELECT id FROM items WHERE normalized_location = ?1",
                [&normalized],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if let Some(id) = existing {
            return select_item(&connection, id);
        }
    }

    let metadata = fetch_url_metadata(&url, &state.icon_dir).await;
    let fallback_title = url
        .host_str()
        .unwrap_or("Bağlantı")
        .trim_start_matches("www.")
        .to_string();
    let title = metadata.title.unwrap_or(fallback_title);
    let connection = state
        .database
        .lock()
        .map_err(|_| "Veritabanı kilidi alınamadı.".to_string())?;
    connection
        .execute(
            "INSERT OR IGNORE INTO items (kind, title, location, normalized_location, description, icon_path, status, favorite, created_at, last_checked_at) VALUES ('url', ?1, ?2, ?3, ?4, ?5, 'available', 0, ?6, ?6)",
            params![title, normalized, normalized, metadata.description, metadata.icon_path, now_unix()],
        )
        .map_err(|error| error.to_string())?;
    let id = connection
        .query_row(
            "SELECT id FROM items WHERE normalized_location = ?1",
            [&normalized],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    select_item(&connection, id)
}

#[tauri::command]
async fn add_local_paths(
    paths: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Vec<LibraryItem>, String> {
    let icon_dir = state.icon_dir.clone();
    let candidates = tauri::async_runtime::spawn_blocking(move || {
        paths
            .into_iter()
            .take(200)
            .filter_map(|raw_path| {
                let path = PathBuf::from(&raw_path);
                if !path.exists() {
                    return None;
                }
                let canonical = fs::canonicalize(&path).unwrap_or(path);
                let location = canonical.to_string_lossy().into_owned();
                let normalized = location.to_lowercase();
                let kind = if canonical.is_dir() { "folder" } else { "file" };
                let title = canonical
                    .file_name()
                    .and_then(|name| name.to_str())
                    .filter(|name| !name.is_empty())
                    .unwrap_or(&location)
                    .to_string();
                let icon_path = cache_local_shell_image(&canonical, &icon_dir);
                Some(LocalCandidate {
                    kind,
                    title,
                    location,
                    normalized,
                    icon_path,
                })
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|error| format!("Yerel kaynaklar hazırlanamadı: {error}"))?;

    let connection = state
        .database
        .lock()
        .map_err(|_| "Veritabanı kilidi alınamadı.".to_string())?;
    let mut added = Vec::new();
    for candidate in candidates {
        connection
            .execute(
                "INSERT INTO items (kind, title, location, normalized_location, icon_path, status, favorite, created_at, last_checked_at) VALUES (?1, ?2, ?3, ?4, ?5, 'available', 0, ?6, ?6) ON CONFLICT(normalized_location) DO UPDATE SET icon_path = COALESCE(excluded.icon_path, items.icon_path), status = 'available', last_checked_at = excluded.last_checked_at",
                params![candidate.kind, candidate.title, candidate.location, candidate.normalized, candidate.icon_path, now_unix()],
            )
            .map_err(|error| error.to_string())?;
        let id = connection
            .query_row(
                "SELECT id FROM items WHERE normalized_location = ?1",
                [&candidate.normalized],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        added.push(select_item(&connection, id)?);
    }
    Ok(added)
}

#[tauri::command]
async fn refresh_local_items(state: State<'_, AppState>) -> Result<Vec<LibraryItem>, String> {
    let local_items = {
        let connection = state
            .database
            .lock()
            .map_err(|_| "Veritabanı kilidi alınamadı.".to_string())?;
        let mut statement = connection
            .prepare("SELECT id, location, icon_path FROM items WHERE kind IN ('file', 'folder')")
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
    };
    let icon_dir = state.icon_dir.clone();
    let refreshed = tauri::async_runtime::spawn_blocking(move || {
        local_items
            .into_iter()
            .map(|(id, location, previous_icon)| {
                let path = PathBuf::from(&location);
                let status = local_resource_status(&path).to_string();
                let icon_path = if status == "available" {
                    cache_local_shell_image(&path, &icon_dir).or(previous_icon)
                } else {
                    previous_icon
                };
                (id, status, icon_path)
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|error| format!("Kaynak durumları yenilenemedi: {error}"))?;

    let connection = state
        .database
        .lock()
        .map_err(|_| "Veritabanı kilidi alınamadı.".to_string())?;
    let mut updated_items = Vec::new();
    for (id, status, icon_path) in refreshed {
        connection
            .execute(
                "UPDATE items SET status = ?1, icon_path = ?2, last_checked_at = ?3 WHERE id = ?4",
                params![status, icon_path, now_unix(), id],
            )
            .map_err(|error| error.to_string())?;
        updated_items.push(select_item(&connection, id)?);
    }
    Ok(updated_items)
}

#[tauri::command]
fn set_item_collection(
    item_id: i64,
    collection_id: i64,
    assigned: bool,
    state: State<'_, AppState>,
) -> Result<LibraryItem, String> {
    let connection = state
        .database
        .lock()
        .map_err(|_| "Veritabanı kilidi alınamadı.".to_string())?;
    if assigned {
        connection
            .execute(
                "INSERT OR IGNORE INTO collection_items (collection_id, item_id, position) VALUES (?1, ?2, COALESCE((SELECT MAX(position) + 1 FROM collection_items WHERE collection_id = ?1), 0))",
                params![collection_id, item_id],
            )
            .map_err(|error| error.to_string())?;
    } else {
        connection
            .execute(
                "DELETE FROM collection_items WHERE collection_id = ?1 AND item_id = ?2",
                params![collection_id, item_id],
            )
            .map_err(|error| error.to_string())?;
    }
    select_item(&connection, item_id)
}

#[tauri::command]
fn toggle_favorite(id: i64, state: State<'_, AppState>) -> Result<bool, String> {
    let connection = state
        .database
        .lock()
        .map_err(|_| "Veritabanı kilidi alınamadı.".to_string())?;
    connection
        .execute(
            "UPDATE items SET favorite = CASE favorite WHEN 0 THEN 1 ELSE 0 END WHERE id = ?1",
            [id],
        )
        .map_err(|error| error.to_string())?;
    connection
        .query_row("SELECT favorite FROM items WHERE id = ?1", [id], |row| {
            Ok(row.get::<_, i64>(0)? != 0)
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn remove_item(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let connection = state
        .database
        .lock()
        .map_err(|_| "Veritabanı kilidi alınamadı.".to_string())?;
    connection
        .execute("DELETE FROM items WHERE id = ?1", [id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
fn open_item(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let connection = state
        .database
        .lock()
        .map_err(|_| "Veritabanı kilidi alınamadı.".to_string())?;
    let location: String = connection
        .query_row("SELECT location FROM items WHERE id = ?1", [id], |row| {
            row.get(0)
        })
        .map_err(|error| error.to_string())?;
    open::that(location).map_err(|error| format!("Kaynak açılamadı: {error}"))
}

#[tauri::command]
fn reveal_item(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    let connection = state
        .database
        .lock()
        .map_err(|_| "Veritabanı kilidi alınamadı.".to_string())?;
    let (kind, location): (String, String) = connection
        .query_row(
            "SELECT kind, location FROM items WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| error.to_string())?;
    let target = if kind == "file" {
        PathBuf::from(location)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default()
    } else {
        PathBuf::from(location)
    };
    open::that(target).map_err(|error| format!("Explorer açılamadı: {error}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data = app.path().app_data_dir()?;
            let icon_dir = app_data.join("icons");
            fs::create_dir_all(&icon_dir)?;
            let connection = Connection::open(app_data.join("favspace.db"))?;
            initialize_database(&connection)?;
            app.manage(AppState {
                database: Mutex::new(connection),
                icon_dir,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_items,
            list_collections,
            create_collection,
            delete_collection,
            list_tags,
            update_item_metadata,
            add_url,
            add_local_paths,
            refresh_local_items,
            set_item_collection,
            toggle_favorite,
            remove_item,
            open_item,
            reveal_item
        ])
        .run(tauri::generate_context!())
        .expect("Favspace başlatılamadı");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_url_and_removes_fragment() {
        let url = normalize_url("example.com/path#section").unwrap();
        assert_eq!(url.as_str(), "https://example.com/path");
    }

    #[test]
    fn rejects_non_web_schemes() {
        assert!(normalize_url("file:///C:/secret.txt").is_err());
    }

    #[test]
    fn blocks_local_metadata_targets() {
        assert!(!metadata_fetch_allowed(
            &Url::parse("http://127.0.0.1/admin").unwrap()
        ));
        assert!(!metadata_fetch_allowed(
            &Url::parse("http://localhost:3000").unwrap()
        ));
    }

    #[test]
    fn creates_database_schema() {
        let connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'items'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn hydrates_virtual_collections_and_tags() {
        let connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO items (kind, title, location, normalized_location, status, favorite, created_at) VALUES ('folder', 'Design', 'C:\\Design', 'c:\\design', 'available', 0, 1)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO collections (name, color, icon, created_at) VALUES ('Projects', '#8b5cf6', 'layers', 1)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO tags (name, color) VALUES ('active', '#4f8e72')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO collection_items (collection_id, item_id, position) VALUES (1, 1, 0)",
                [],
            )
            .unwrap();
        connection
            .execute("INSERT INTO item_tags (item_id, tag_id) VALUES (1, 1)", [])
            .unwrap();

        let item = select_item(&connection, 1).unwrap();
        assert_eq!(item.collection_ids, vec![1]);
        assert_eq!(item.tags.len(), 1);
        assert_eq!(item.tags[0].name, "active");
        assert_eq!(item.location, "C:\\Design");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn extracts_windows_shell_file_image() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let png = windows_thumbnail::extract_png(&manifest, 64).unwrap();
        assert!(png.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]));
    }
}
