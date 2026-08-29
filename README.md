# Favspace

Favspace is a playful, local-first Windows library for URLs, files, and folders. It stores references to local resources without moving or deleting the physical files.

## First vertical slice

- Add URLs and automatically cache page titles, descriptions, and favicons
- Add files and folders through native Windows pickers
- Drop local files and folders directly onto the application
- Show Windows Shell thumbnails or native file-type icons for local resources
- Refresh local resources and mark missing or disconnected-drive items
- Search and filter URLs, files, folders, and favorites together
- Create color-coded virtual collections and filter the library by collection
- Put one resource in multiple collections without moving the underlying file
- Drag a resource card directly onto a sidebar collection
- Edit resource titles and notes, and create reusable virtual tags
- Open resources and reveal local resources in Explorer
- Persist the library in a local SQLite database
- Keep favicon files in the application data directory
- Block non-HTTP URL schemes and skip metadata requests to local/private network targets

Collections, tags, and resource relationships are persisted in SQLite transactions.

## How to use

1. Start Favspace and paste a web address into the **Bir URL yapıştır** field, then select **Yakala**. Favspace normalizes the address and fetches its title, description, and favicon when the site allows it.
2. Select **Dosya ekle** or **Klasör ekle** to choose local resources. You can also drag files and folders directly onto the window. Favspace records only their paths; it does not move or copy them.
3. Double-click a resource card to open it. The three-dot menu can also open it, edit its title and notes, reveal a local resource in Explorer, or remove only the Favspace record.
4. Use the heart button for favorites. Create a virtual collection with the **+** button beside the collections heading, then drag cards onto that collection. A resource may belong to multiple collections.
5. Open **Düzenle** from a card menu to assign collections and reusable tags. Use the search box and sidebar filters to find resources by title, location, notes, or tags.
6. Use the refresh button in the top bar to check whether local files and folders are still available.

Favspace is local-first: library data stays in a per-user SQLite database, and removing a record never deletes the referenced file or folder.

## Run from source

Clone the repository, install the dependencies, and start the Windows desktop app:

```powershell
git clone https://github.com/hburaksavas/favspace.git
cd favspace
npm install
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
npm run tauri dev
```

Required system components are listed below.

## Development

Requirements:

- Windows 10 or Windows 11
- Node.js
- Rust with the MSVC toolchain
- Microsoft C++ Build Tools with **Desktop development with C++**
- Microsoft Edge WebView2

Install and run:

```powershell
npm install
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
npm run tauri dev
```

Frontend-only browser preview:

```powershell
npm run dev
```

The browser preview uses local storage and cannot select local files. Native file and folder workflows require the Tauri application.

## Verification

```powershell
npm run build
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
cargo test --manifest-path src-tauri\Cargo.toml
npm run tauri -- build --debug --no-bundle
```

The debug executable is generated at `src-tauri\target\debug\favspace.exe`.

## Personal portable build

Favspace can be built as a single portable release executable and signed with a personal code-signing certificate. Create the certificate once:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\New-PersonalSigningCertificate.ps1
```

Then create and verify the signed portable executable:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\Build-Portable.ps1
```

The result is written to `artifacts\Favspace-Portable.exe`. The private key is stored as a DPAPI-protected file that only the current Windows user can decrypt; `artifacts\Favspace-Personal-Code-Signing.cer` contains only the public certificate. This is a cryptographic signature, but it has no public CA or Microsoft SmartScreen reputation. To show it as trusted, install the public certificate for the current user under **Trusted Root Certification Authorities** and **Trusted Publishers**. Another Windows account or computer must explicitly trust the same public certificate.

On the personal Windows account that will run Favspace, the one-time trust step can be performed from a normal interactive PowerShell window. Windows may ask for confirmation before trusting the self-signed root:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\Trust-PersonalSigningCertificate.ps1
```

The portable artifact is a single executable, while its database and cached icons remain in the current user's application-data directory.

## Local data

Favspace creates `favspace.db` and an `icons` directory under the per-user application data directory resolved by Tauri. Removing an item from Favspace only deletes its library record; the referenced file or folder is never deleted.
