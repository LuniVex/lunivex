use bytes::Bytes;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use std::io::Read as _;

const CF_API_URL: &str = "https://api.curseforge.com";
const MINECRAFT_GAME_ID: u32 = 432;

fn cf_api_key() -> crate::Result<&'static str> {
	match option_env!("CURSEFORGE_API_KEY") {
		Some(key) if !key.is_empty() => Ok(key),
		_ => Err(crate::ErrorKind::OtherError(
			"CURSEFORGE_API_KEY is not configured. \
			Add it to packages/app-lib/.env and rebuild."
				.to_string(),
		)
		.into()),
	}
}

fn cf_client(api_key: &str) -> crate::Result<reqwest::Client> {
	let mut headers = HeaderMap::new();
	headers.insert(
		"x-api-key",
		HeaderValue::from_str(api_key).map_err(|e| {
			crate::ErrorKind::OtherError(format!(
				"Invalid CurseForge API key: {e}"
			))
		})?,
	);
	reqwest::Client::builder()
		.https_only(true)
		.default_headers(headers)
		.user_agent(crate::launcher_user_agent())
		.build()
		.map_err(|e| {
			crate::ErrorKind::OtherError(format!(
				"Failed to build CurseForge HTTP client: {e}"
			))
			.into()
		})
}

// ── API response types ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CfApiResponse<T> {
	data: T,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CfCategory {
	pub id: u32,
	pub name: String,
	pub slug: String,
	pub icon_url: Option<String>,
	pub is_class: Option<bool>,
	pub class_id: Option<u32>,
	pub parent_category_id: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CfAuthor {
	pub id: u32,
	pub name: String,
	pub url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CfLogo {
	pub thumbnail_url: Option<String>,
	pub url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CfLinks {
	pub website_url: Option<String>,
	pub wiki_url: Option<String>,
	pub issues_url: Option<String>,
	pub source_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CfScreenshot {
	pub id: Option<u32>,
	pub mod_id: Option<u32>,
	pub title: Option<String>,
	pub description: Option<String>,
	pub thumbnail_url: Option<String>,
	pub url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CfFileIndex {
	pub game_version: String,
	pub file_id: u32,
	pub filename: String,
	pub release_type: u8,
	pub mod_loader: Option<u8>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CfMod {
	pub id: u32,
	pub name: String,
	pub slug: String,
	pub summary: String,
	pub download_count: f64,
	pub thumbs_up_count: Option<f64>,
	pub categories: Vec<CfCategory>,
	pub class_id: Option<u32>,
	pub authors: Vec<CfAuthor>,
	pub logo: Option<CfLogo>,
	pub links: Option<CfLinks>,
	pub screenshots: Option<Vec<CfScreenshot>>,
	pub latest_files: Option<Vec<CfFile>>,
	pub latest_files_indexes: Vec<CfFileIndex>,
	pub date_created: Option<String>,
	pub date_modified: String,
	pub game_popularity_rank: Option<u32>,
	pub is_available: bool,
	pub allow_mod_distribution: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CfFileDependency {
	pub mod_id: u32,
	pub relation_type: u8,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CfFile {
	pub id: u32,
	pub mod_id: u32,
	pub display_name: String,
	pub file_name: String,
	pub game_versions: Vec<String>,
	pub release_type: u8,
	pub download_count: f64,
	pub file_date: String,
	pub file_length: Option<u64>,
	pub download_url: Option<String>,
	pub dependencies: Option<Vec<CfFileDependency>>,
	pub is_available: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CfSearchResult {
	pub data: Vec<CfMod>,
	pub pagination: CfPagination,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CfPagination {
	pub index: u32,
	pub page_size: u32,
	pub result_count: u32,
	pub total_count: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CfFilesResult {
	pub data: Vec<CfFile>,
	pub pagination: CfPagination,
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn loader_type_id(loader: &str) -> Option<u8> {
	match loader.to_lowercase().as_str() {
		"forge" => Some(1),
		"fabric" => Some(4),
		"quilt" => Some(5),
		"neoforge" => Some(6),
		_ => None,
	}
}

/// Map a frontend project-type slug to the CurseForge classId.
/// Returns `None` for types that CurseForge doesn't have (datapack, server).
pub fn class_id_for(project_type: &str) -> Option<u32> {
	match project_type {
		"mod" => Some(6),
		"modpack" => Some(4471),
		"resourcepack" => Some(12),
		"shader" => Some(6552),
		_ => None,
	}
}

// ── Public API functions ───────────────────────────────────────────────────────

pub async fn search(
	query: Option<String>,
	game_version: Option<String>,
	loader: Option<String>,
	category_id: Option<u32>,
	class_id: u32,
	page: u32,
	page_size: u32,
) -> crate::Result<CfSearchResult> {
	let client = cf_client(cf_api_key()?)?;

	// Use relevance sort when there's a search query, popularity otherwise.
	let sort_field = if query.as_deref().map_or(true, str::is_empty) {
		"2"
	} else {
		"1"
	};

	let mut params: Vec<(&str, String)> = vec![
		("gameId", MINECRAFT_GAME_ID.to_string()),
		("classId", class_id.to_string()),
		("pageSize", page_size.to_string()),
		("index", (page * page_size).to_string()),
		("sortField", sort_field.to_string()),
		("sortOrder", "desc".to_string()),
	];

	if let Some(q) = &query {
		if !q.is_empty() {
			params.push(("searchFilter", q.clone()));
		}
	}
	if let Some(gv) = &game_version {
		params.push(("gameVersion", gv.clone()));
	}
	if let Some(cat) = category_id {
		params.push(("categoryId", cat.to_string()));
	}
	if let Some(l) = &loader {
		if let Some(lt) = loader_type_id(l) {
			params.push(("modLoaderType", lt.to_string()));
		}
	}

	let resp = client
		.get(format!("{CF_API_URL}/v1/mods/search"))
		.query(&params)
		.send()
		.await?;

	if !resp.status().is_success() {
		let status = resp.status();
		let body = resp.text().await.unwrap_or_default();
		return Err(crate::ErrorKind::OtherError(format!(
			"CurseForge search failed ({status}): {body}"
		))
		.into());
	}

	Ok(resp.json::<CfSearchResult>().await?)
}

pub async fn get_categories(class_id: u32) -> crate::Result<Vec<CfCategory>> {
	let client = cf_client(cf_api_key()?)?;

	let resp = client
		.get(format!("{CF_API_URL}/v1/categories"))
		.query(&[
			("gameId", MINECRAFT_GAME_ID.to_string()),
			("classId", class_id.to_string()),
		])
		.send()
		.await?;

	let result: CfApiResponse<Vec<CfCategory>> = resp.json().await?;

	// Filter out the root class entries (isClass = true)
	Ok(result
		.data
		.into_iter()
		.filter(|c| !c.is_class.unwrap_or(false))
		.collect())
}

pub async fn get_mod(mod_id: u32) -> crate::Result<CfMod> {
	let client = cf_client(cf_api_key()?)?;

	let resp = client
		.get(format!("{CF_API_URL}/v1/mods/{mod_id}"))
		.send()
		.await?;

	let result: CfApiResponse<CfMod> = resp.json().await?;
	Ok(result.data)
}

pub async fn get_mod_files(
	mod_id: u32,
	game_version: Option<String>,
	loader: Option<String>,
	page: Option<u32>,
	page_size: Option<u32>,
) -> crate::Result<CfFilesResult> {
	let client = cf_client(cf_api_key()?)?;

	let page_size = page_size.unwrap_or(50).min(50);
	let mut params: Vec<(&str, String)> = vec![
		("pageSize", page_size.to_string()),
		("index", (page.unwrap_or(0) * page_size).to_string()),
	];
	if let Some(gv) = &game_version {
		params.push(("gameVersion", gv.clone()));
	}
	if let Some(l) = &loader {
		if let Some(lt) = loader_type_id(l) {
			params.push(("modLoaderType", lt.to_string()));
		}
	}

	let resp = client
		.get(format!("{CF_API_URL}/v1/mods/{mod_id}/files"))
		.query(&params)
		.send()
		.await?;

	Ok(resp.json::<CfFilesResult>().await?)
}

pub async fn get_file(mod_id: u32, file_id: u32) -> crate::Result<CfFile> {
	let client = cf_client(cf_api_key()?)?;

	let resp = client
		.get(format!("{CF_API_URL}/v1/mods/{mod_id}/files/{file_id}"))
		.send()
		.await?;

	let result: CfApiResponse<CfFile> = resp.json().await?;
	Ok(result.data)
}

/// Fetch the HTML changelog for a specific CurseForge file.
pub async fn get_file_changelog(
	mod_id: u32,
	file_id: u32,
) -> crate::Result<String> {
	let client = cf_client(cf_api_key()?)?;

	let resp = client
		.get(format!(
			"{CF_API_URL}/v1/mods/{mod_id}/files/{file_id}/changelog"
		))
		.send()
		.await?;
	if !resp.status().is_success() {
		return Ok(String::new());
	}

	let result: CfApiResponse<String> = resp.json().await?;
	Ok(result.data)
}

/// Fetch multiple CurseForge mods by ID in one request (used for resolving
/// file dependency names).
pub async fn get_mods(mod_ids: Vec<u32>) -> crate::Result<Vec<CfMod>> {
	if mod_ids.is_empty() {
		return Ok(Vec::new());
	}
	let client = cf_client(cf_api_key()?)?;

	let resp = client
		.post(format!("{CF_API_URL}/v1/mods"))
		.json(&serde_json::json!({ "modIds": mod_ids }))
		.send()
		.await?;

	let result: CfApiResponse<Vec<CfMod>> = resp.json().await?;
	Ok(result.data)
}

async fn file_download_url(
	client: &reqwest::Client,
	mod_id: u32,
	file_id: u32,
) -> crate::Result<String> {
	let resp = client
		.get(format!(
			"{CF_API_URL}/v1/mods/{mod_id}/files/{file_id}/download-url"
		))
		.send()
		.await?;

	if !resp.status().is_success() {
		return Err(crate::ErrorKind::OtherError(format!(
			"CurseForge file {file_id} is not available for download \
			(the author may have disabled distribution)"
		))
		.into());
	}

	let result: CfApiResponse<Option<String>> = resp.json().await?;
	result.data.ok_or_else(|| {
		crate::ErrorKind::OtherError(
			"CurseForge: mod author has disabled file distribution".to_string(),
		)
		.into()
	})
}

/// Download the best matching CurseForge file and add it to the given profile.
/// Finds the most stable release for the specified game_version + loader, fetches the
/// download URL via the official API endpoint, and writes it into the instance folder.
pub async fn install_mod(
	profile_path: &str,
	mod_id: u32,
	game_version: Option<String>,
	loader: Option<String>,
	project_type: Option<crate::state::ProjectType>,
) -> crate::Result<String> {
	let client = cf_client(cf_api_key()?)?;

	// 1. Fetch files compatible with the requested version/loader
	let mut file_params: Vec<(&str, String)> =
		vec![("pageSize", "50".to_string())];
	if let Some(gv) = &game_version {
		file_params.push(("gameVersion", gv.clone()));
	}
	if let Some(l) = &loader {
		if let Some(lt) = loader_type_id(l) {
			file_params.push(("modLoaderType", lt.to_string()));
		}
	}

	let resp = client
		.get(format!("{CF_API_URL}/v1/mods/{mod_id}/files"))
		.query(&file_params)
		.send()
		.await?;

	let files: CfApiResponse<Vec<CfFile>> = resp.json().await?;
	let files = files.data;

	// 2. Pick best file: stable (release_type 1) > beta (2) > alpha (3); most recent first
	let best = files
		.iter()
		.filter(|f| f.is_available)
		.min_by_key(|f| f.release_type)
		.ok_or_else(|| {
			crate::ErrorKind::OtherError(
				"No compatible CurseForge file found for this game version and loader"
					.to_string(),
			)
		})?;

	let file_id = best.id;
	let file_name = best.file_name.clone();

	// 3. Get download URL from the official API endpoint
	let url = file_download_url(&client, mod_id, file_id).await?;

	// 4. Download
	let download_resp = client.get(&url).send().await?;
	if !download_resp.status().is_success() {
		return Err(crate::ErrorKind::OtherError(format!(
			"Failed to download CurseForge file: HTTP {}",
			download_resp.status()
		))
		.into());
	}
	let bytes: Bytes = download_resp.bytes().await?;

	// 5. Write into the profile via the standard project-bytes path
	let state = crate::State::get().await?;
	let rel_path = crate::state::Profile::add_project_bytes(
		profile_path,
		&file_name,
		bytes,
		None,
		project_type,
		None,
		&state.io_semaphore,
		&state.pool,
	)
	.await?;

	crate::event::emit::emit_profile(
		profile_path,
		crate::event::ProfilePayloadType::Edited,
	)
	.await?;

	Ok(rel_path)
}

// ── CF modpack manifest types ──────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfPackManifest {
	minecraft: CfPackMinecraft,
	name: String,
	files: Vec<CfPackFileRef>,
	#[serde(default = "default_overrides_dir")]
	overrides: String,
}

fn default_overrides_dir() -> String {
	"overrides".to_string()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfPackMinecraft {
	version: String,
	mod_loaders: Vec<CfPackLoader>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfPackLoader {
	id: String,
	primary: bool,
}

#[derive(Deserialize)]
struct CfPackFileRef {
	#[serde(rename = "projectID")]
	project_id: u32,
	#[serde(rename = "fileID")]
	file_id: u32,
	required: bool,
}

fn parse_loader_id(id: &str) -> (crate::state::ModLoader, Option<String>) {
	// CF loader IDs look like "forge-47.0.0", "fabric-0.14.21", "neoforge-20.4.0"
	for (prefix, loader) in &[
		("neoforge-", crate::state::ModLoader::NeoForge),
		("forge-", crate::state::ModLoader::Forge),
		("fabric-", crate::state::ModLoader::Fabric),
		("quilt-", crate::state::ModLoader::Quilt),
	] {
		if let Some(ver) = id.strip_prefix(prefix) {
			return (*loader, Some(ver.to_string()));
		}
	}
	(crate::state::ModLoader::Vanilla, None)
}

// ── Description & modpack install ─────────────────────────────────────────────

/// Fetch the long HTML description for a CurseForge mod.
pub async fn get_mod_description(mod_id: u32) -> crate::Result<String> {
	let client = cf_client(cf_api_key()?)?;
	let resp = client
		.get(format!("{CF_API_URL}/v1/mods/{mod_id}/description"))
		.send()
		.await?;
	if !resp.status().is_success() {
		return Ok(String::new());
	}
	let result: CfApiResponse<String> = resp.json().await?;
	Ok(result.data)
}

/// Download and install a CurseForge mod file with a known file ID into a profile.
pub async fn install_mod_file(
	profile_path: &str,
	mod_id: u32,
	file_id: u32,
	file_name: &str,
	project_type: Option<crate::state::ProjectType>,
) -> crate::Result<String> {
	let client = cf_client(cf_api_key()?)?;
	let url = file_download_url(&client, mod_id, file_id).await?;

	let download_resp = client.get(&url).send().await?;
	if !download_resp.status().is_success() {
		return Err(crate::ErrorKind::OtherError(format!(
			"Failed to download CurseForge file: HTTP {}",
			download_resp.status()
		))
		.into());
	}
	let bytes: bytes::Bytes = download_resp.bytes().await?;

	let state = crate::State::get().await?;
	let rel_path = crate::state::Profile::add_project_bytes(
		profile_path,
		file_name,
		bytes,
		None,
		project_type,
		None,
		&state.io_semaphore,
		&state.pool,
	)
	.await?;

	crate::event::emit::emit_profile(
		profile_path,
		crate::event::ProfilePayloadType::Edited,
	)
	.await?;

	Ok(rel_path)
}

async fn fetch_mod_icon(client: &reqwest::Client, mod_id: u32) -> Option<String> {
	let resp = client
		.get(format!("{CF_API_URL}/v1/mods/{mod_id}"))
		.send()
		.await
		.ok()?;
	resp.json::<CfApiResponse<CfMod>>()
		.await
		.ok()
		.and_then(|d| d.data.logo)
		.and_then(|l| l.thumbnail_url)
}

async fn install_modpack_inner(
	client: &reqwest::Client,
	mod_id: u32,
	file_id: u32,
	icon_url: Option<String>,
) -> crate::Result<String> {
	// Get the download URL via the official endpoint
	let url = file_download_url(client, mod_id, file_id).await?;

	// 3. Download the zip
	let zip_bytes = client.get(&url).send().await?.bytes().await?;

	// 4. Parse manifest.json from the zip
	let manifest: CfPackManifest = {
		let cursor = std::io::Cursor::new(zip_bytes.as_ref());
		let mut archive = zip::ZipArchive::new(cursor).map_err(|e| {
			crate::ErrorKind::OtherError(format!("Failed to open modpack zip: {e}"))
		})?;
		let mut entry = archive.by_name("manifest.json").map_err(|_| {
			crate::ErrorKind::OtherError(
				"CurseForge modpack is missing manifest.json".to_string(),
			)
		})?;
		let mut s = String::new();
		entry.read_to_string(&mut s)?;
		serde_json::from_str(&s)?
	};

	// 5. Determine the mod loader from the manifest
	let primary_loader = manifest
		.minecraft
		.mod_loaders
		.iter()
		.find(|l| l.primary)
		.or_else(|| manifest.minecraft.mod_loaders.first());
	let (loader, loader_version) = primary_loader
		.map(|l| parse_loader_id(&l.id))
		.unwrap_or((crate::state::ModLoader::Vanilla, None));

	// 6. Create the new profile (installs the loader)
	let profile_path = crate::api::profile::create::profile_create(
		manifest.name.clone(),
		manifest.minecraft.version.clone(),
		loader,
		loader_version,
		None,
		None,
		Some(false), // DO install the loader
	)
	.await?;

	// 6b. Set the modpack icon as an HTTPS URL directly on the profile so it
	//     works both in the app UI (getDisplayIconUrl handles https:// links)
	//     and in Discord Rich Presence (large_image accepts https:// URLs).
	if let Some(ref url) = icon_url {
		let url_clone = url.clone();
		let _ = crate::api::profile::edit(&profile_path, |prof| {
			prof.icon_path = Some(url_clone.clone());
			async { Ok(()) }
		})
		.await;
	}

	// 7. Download and install each required mod file with progress reporting
	let required_files: Vec<&CfPackFileRef> =
		manifest.files.iter().filter(|f| f.required).collect();
	let total = required_files.len();
	let loading_bar = crate::event::emit::init_loading(
		crate::event::LoadingBarType::CfModpackInstall {
			pack_name: manifest.name.clone(),
			icon: icon_url.clone(),
		},
		total.max(1) as f64,
		"Downloading mods",
	)
	.await
	.ok();

	let state = crate::State::get().await?;
	for (idx, file_ref) in required_files.iter().enumerate() {
		let dl_url =
			match file_download_url(client, file_ref.project_id, file_ref.file_id).await {
				Ok(u) => u,
				Err(e) => {
					tracing::warn!(
						"Skipping CF file {}/{}: {}",
						file_ref.project_id,
						file_ref.file_id,
						e
					);
					if let Some(ref bar) = loading_bar {
						let _ = crate::event::emit::emit_loading(
							bar,
							1.0,
							Some(&format!("Downloading mods ({}/{total})", idx + 1)),
						);
					}
					continue;
				}
			};
		let mod_bytes = match client.get(&dl_url).send().await {
			Ok(r) => match r.bytes().await {
				Ok(b) => b,
				Err(e) => {
					tracing::warn!("Failed to download {}: {}", dl_url, e);
					if let Some(ref bar) = loading_bar {
						let _ = crate::event::emit::emit_loading(
							bar,
							1.0,
							Some(&format!("Downloading mods ({}/{total})", idx + 1)),
						);
					}
					continue;
				}
			},
			Err(e) => {
				tracing::warn!("Failed to request {}: {}", dl_url, e);
				if let Some(ref bar) = loading_bar {
					let _ = crate::event::emit::emit_loading(
						bar,
						1.0,
						Some(&format!("Downloading mods ({}/{total})", idx + 1)),
					);
				}
				continue;
			}
		};
		// Extract a clean file name from the URL (strip query params)
		let file_name = dl_url
			.split('?')
			.next()
			.unwrap_or(&dl_url)
			.split('/')
			.last()
			.unwrap_or("mod.jar")
			.to_string();
		if let Err(e) = crate::state::Profile::add_project_bytes(
			&profile_path,
			&file_name,
			mod_bytes,
			None,
			Some(crate::state::ProjectType::Mod),
			None,
			&state.io_semaphore,
			&state.pool,
		)
		.await
		{
			tracing::warn!("Failed to add {} to profile: {}", file_name, e);
		}
		if let Some(ref bar) = loading_bar {
			let _ = crate::event::emit::emit_loading(
				bar,
				1.0,
				Some(&format!("Downloading mods ({}/{total})", idx + 1)),
			);
		}
	}

	// 8. Copy override files from the zip into the profile
	let overrides_prefix =
		format!("{}/", manifest.overrides.trim_end_matches('/'));
	let profile_base = crate::api::profile::get_full_path(&profile_path).await?;
	{
		let cursor = std::io::Cursor::new(zip_bytes.as_ref());
		let mut archive = zip::ZipArchive::new(cursor).map_err(|e| {
			crate::ErrorKind::OtherError(format!("Failed to re-read modpack zip: {e}"))
		})?;
		for i in 0..archive.len() {
			let mut entry = match archive.by_index(i) {
				Ok(e) => e,
				Err(_) => continue,
			};
			let name = entry.name().to_string();
			if !name.starts_with(&overrides_prefix) || name.ends_with('/') {
				continue;
			}
			let rel = &name[overrides_prefix.len()..];
			if rel.is_empty() {
				continue;
			}
			let dest = profile_base.join(rel);
			if let Some(parent) = dest.parent() {
				let _ = std::fs::create_dir_all(parent);
			}
			let mut buf = Vec::new();
			if entry.read_to_end(&mut buf).is_ok() {
				let _ = std::fs::write(&dest, &buf);
			}
		}
	}

	// 9. Signal that the profile was updated
	crate::event::emit::emit_profile(
		&profile_path,
		crate::event::ProfilePayloadType::Edited,
	)
	.await?;

	Ok(profile_path)
}

/// Download a CurseForge modpack, parse its manifest, create a new instance, and
/// install all listed mod files into it.  Returns the new profile path.
pub async fn install_modpack(mod_id: u32) -> crate::Result<String> {
	let client = cf_client(cf_api_key()?)?;
	let icon_url = fetch_mod_icon(&client, mod_id).await;

	let resp = client
		.get(format!("{CF_API_URL}/v1/mods/{mod_id}/files"))
		.query(&[("pageSize", "50")])
		.send()
		.await?;
	let files: CfApiResponse<Vec<CfFile>> = resp.json().await?;
	let best = files
		.data
		.iter()
		.filter(|f| f.is_available)
		.min_by_key(|f| f.release_type)
		.ok_or_else(|| {
			crate::ErrorKind::OtherError(
				"No compatible CurseForge modpack file found".to_string(),
			)
		})?;
	let file_id = best.id;

	install_modpack_inner(&client, mod_id, file_id, icon_url).await
}

/// Install a specific CurseForge modpack file by file ID.  Returns the new profile path.
pub async fn install_modpack_file(mod_id: u32, file_id: u32) -> crate::Result<String> {
	let client = cf_client(cf_api_key()?)?;
	let icon_url = fetch_mod_icon(&client, mod_id).await;
	install_modpack_inner(&client, mod_id, file_id, icon_url).await
}
