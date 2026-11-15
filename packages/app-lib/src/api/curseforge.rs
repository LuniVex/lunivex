use bytes::Bytes;
use reqwest::header::{HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

mod client;

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
		.redirect(reqwest::redirect::Policy::none())
		.connect_timeout(Duration::from_secs(15))
		.timeout(Duration::from_secs(30))
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
pub struct CfFileHash {
	pub value: String,
	pub algo: u8,
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
	#[serde(default)]
	pub hashes: Vec<CfFileHash>,
	#[serde(default)]
	pub is_server_pack: bool,
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

pub async fn search(
	query: Option<String>,
	game_version: Option<String>,
	loader: Option<String>,
	category_id: Option<u32>,
	class_id: u32,
	page: u32,
	page_size: u32,
) -> crate::Result<CfSearchResult> {
	client::Api::new()?
		.search(
			query,
			game_version,
			loader,
			category_id,
			class_id,
			page,
			page_size,
		)
		.await
}

pub async fn get_categories(class_id: u32) -> crate::Result<Vec<CfCategory>> {
	client::Api::new()?.categories(class_id).await
}

pub async fn search_filtered(
	query: Option<String>,
	game_version: Option<String>,
	loader: Option<String>,
	category_id: Option<u32>,
	class_id: u32,
	page: u32,
	page_size: u32,
	game_versions: Option<Vec<String>>,
	loaders: Option<Vec<String>>,
	sort: Option<String>,
) -> crate::Result<CfSearchResult> {
	client::Api::new()?
		.search_filtered(
			query,
			game_version,
			loader,
			category_id,
			class_id,
			page,
			page_size,
			game_versions,
			loaders,
			sort,
		)
		.await
}

pub async fn get_mod(mod_id: u32) -> crate::Result<CfMod> {
	client::Api::new()?.project(mod_id).await
}

pub async fn get_mods(mod_ids: Vec<u32>) -> crate::Result<Vec<CfMod>> {
	if mod_ids.is_empty() {
		return Ok(Vec::new());
	}
	client::Api::new()?.projects(mod_ids).await
}

pub async fn get_mod_files(
	mod_id: u32,
	game_version: Option<String>,
	loader: Option<String>,
	page: Option<u32>,
	page_size: Option<u32>,
) -> crate::Result<CfFilesResult> {
	client::Api::new()?
		.files(
			mod_id,
			game_version.as_deref(),
			loader.as_deref(),
			page.unwrap_or(0),
			page_size.unwrap_or(50),
		)
		.await
}

pub async fn get_file(mod_id: u32, file_id: u32) -> crate::Result<CfFile> {
	client::Api::new()?.file(mod_id, file_id).await
}

pub async fn get_file_changelog(
	mod_id: u32,
	file_id: u32,
) -> crate::Result<String> {
	client::Api::new()?
		.text(&format!("/v1/mods/{mod_id}/files/{file_id}/changelog"))
		.await
}

pub async fn get_mod_description(mod_id: u32) -> crate::Result<String> {
	client::Api::new()?
		.text(&format!("/v1/mods/{mod_id}/description"))
		.await
}

struct StagedFile {
	file: CfFile,
	path: PathBuf,
}

struct StagedInstall {
	_directory: tempfile::TempDir,
	files: Vec<StagedFile>,
}

fn cf_error(message: impl Into<String>) -> crate::Error {
	crate::ErrorKind::OtherError(message.into()).into()
}

async fn stage_files(
	api: &client::Api,
	files: Vec<CfFile>,
	installed_hashes: &HashSet<String>,
	loading_bar: Option<&crate::event::LoadingBarId>,
) -> crate::Result<StagedInstall> {
	let directory = tempfile::tempdir()?;
	let mut staged = Vec::new();
	let total = files.len();
	let mut names = HashSet::new();
	for (index, file) in files.into_iter().enumerate() {
		client::validate_file_name(&file.file_name)?;
		if !names.insert(file.file_name.to_lowercase()) {
			return Err(cf_error(format!(
				"CurseForge returned duplicate file name {}",
				file.file_name
			)));
		}
		if !file.hashes.iter().any(|hash| {
			hash.algo == 1
				&& installed_hashes.contains(&hash.value.to_lowercase())
		}) {
			let label = format!(
				"{} (project {}, file {})",
				file.file_name, file.mod_id, file.id
			);
			let result = async {
				let download = api.prepare(file).await?;
				let bytes = api.download(&download).await?;
				let path = directory.path().join(index.to_string());
				tokio::fs::write(&path, bytes).await?;
				Ok::<_, crate::Error>(StagedFile {
					file: download.file,
					path,
				})
			}
			.await
			.map_err(|error| {
				cf_error(format!(
					"Could not install required CurseForge file {label}: {error}"
				))
			})?;
			staged.push(result);
		}
		if let Some(bar) = loading_bar {
			let _ = crate::event::emit::emit_loading(
				bar,
				1.0,
				Some(&format!("Preparing mods ({}/{total})", index + 1)),
			);
		}
	}
	Ok(StagedInstall {
		_directory: directory,
		files: staged,
	})
}

fn project_type(
	class_id: Option<u32>,
	fallback: Option<crate::state::ProjectType>,
) -> crate::Result<Option<crate::state::ProjectType>> {
	use crate::state::ProjectType;
	match class_id {
		Some(6) => Ok(Some(ProjectType::Mod)),
		Some(12) => Ok(Some(ProjectType::ResourcePack)),
		Some(6552) => Ok(Some(ProjectType::ShaderPack)),
		Some(4471) => {
			Err(cf_error("Install CurseForge modpacks as a new instance"))
		}
		_ => Ok(fallback),
	}
}

/// Install the selected project together with its required, compatible dependencies.
pub async fn install_mod(
	profile_path: &str,
	mod_id: u32,
	_game_version: Option<String>,
	_loader: Option<String>,
	requested_type: Option<crate::state::ProjectType>,
) -> crate::Result<String> {
	let api = client::Api::new()?;
	let profile = crate::api::profile::get(profile_path)
		.await?
		.ok_or_else(|| cf_error("The target instance no longer exists"))?;
	let kind =
		project_type(api.project(mod_id).await?.class_id, requested_type)?;
	let loader = if kind == Some(crate::state::ProjectType::Mod) {
		Some(profile.loader.as_str())
	} else {
		None
	};
	if loader == Some("vanilla") {
		return Err(cf_error(
			"Choose a modded instance before installing CurseForge mods",
		));
	}
	let file = api
		.best_file(mod_id, Some(&profile.game_version), loader, &HashSet::new())
		.await?;
	install_into_profile(&api, profile_path, file, kind).await
}

/// File names and compatibility come from the API and target profile, not caller input.
pub async fn install_mod_file(
	profile_path: &str,
	mod_id: u32,
	file_id: u32,
	_file_name: &str,
	requested_type: Option<crate::state::ProjectType>,
) -> crate::Result<String> {
	let api = client::Api::new()?;
	let kind =
		project_type(api.project(mod_id).await?.class_id, requested_type)?;
	let file = api.file(mod_id, file_id).await?;
	install_into_profile(&api, profile_path, file, kind).await
}

async fn install_into_profile(
	api: &client::Api,
	profile_path: &str,
	root: CfFile,
	kind: Option<crate::state::ProjectType>,
) -> crate::Result<String> {
	let profile = crate::api::profile::get(profile_path)
		.await?
		.ok_or_else(|| cf_error("The target instance no longer exists"))?;
	let is_mod = kind == Some(crate::state::ProjectType::Mod);
	let loader = profile.loader.as_str();
	if !root.game_versions.contains(&profile.game_version)
		|| (is_mod
			&& (loader == "vanilla"
				|| !root
					.game_versions
					.iter()
					.any(|v| v.eq_ignore_ascii_case(loader))))
	{
		return Err(cf_error(
			"This CurseForge file does not support the target instance's Minecraft version and loader",
		));
	}
	let root_name = root.file_name.clone();
	client::validate_file_name(&root_name)?;
	let existing =
		crate::api::profile::get_projects(profile_path, None).await?;
	let installed_hashes = existing
		.iter()
		.filter(|entry| !entry.key().ends_with(".disabled"))
		.map(|entry| entry.hash.to_lowercase())
		.collect::<HashSet<_>>();
	let files = if is_mod {
		api.dependencies(root, &profile.game_version, loader, &installed_hashes)
			.await?
	} else {
		vec![root]
	};
	let staged = stage_files(api, files, &installed_hashes, None).await?;
	let base = crate::api::profile::get_full_path(profile_path).await?;
	for file in &staged.files {
		let folder = kind.map(|t| t.get_folder()).unwrap_or("mods");
		let target = base.join(folder).join(&file.file.file_name);
		if target.exists() {
			return Err(cf_error(format!(
				"{} already exists in this instance. Remove or update the existing file before installing another version.",
				file.file.file_name
			)));
		}
	}
	let state = crate::State::get().await?;
	for file in &staged.files {
		crate::state::Profile::add_project_bytes(
			profile_path,
			&file.file.file_name,
			Bytes::from(tokio::fs::read(&file.path).await?),
			None,
			kind,
			None,
			&state.io_semaphore,
			&state.pool,
		)
		.await?;
	}
	crate::event::emit::emit_profile(
		profile_path,
		crate::event::ProfilePayloadType::Edited,
	)
	.await?;
	Ok(format!(
		"{}/{}",
		kind.map(|t| t.get_folder()).unwrap_or("mods"),
		root_name
	))
}

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
	#[serde(default)]
	mod_loaders: Vec<CfPackLoader>,
}

#[derive(Deserialize)]
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

fn parse_loader_id(
	id: &str,
) -> crate::Result<(crate::state::ModLoader, Option<String>)> {
	for (prefix, loader) in [
		("neoforge-", crate::state::ModLoader::NeoForge),
		("forge-", crate::state::ModLoader::Forge),
		("fabric-", crate::state::ModLoader::Fabric),
		("quilt-", crate::state::ModLoader::Quilt),
	] {
		if let Some(version) = id.strip_prefix(prefix).filter(|v| !v.is_empty())
		{
			return Ok((loader, Some(version.to_string())));
		}
	}
	Err(cf_error(format!(
		"Unsupported CurseForge modpack loader: {id}"
	)))
}

fn override_relative_path(
	overrides: &str,
	name: &str,
) -> crate::Result<Option<PathBuf>> {
	fn safe(path: &str) -> bool {
		!path.is_empty()
			&& !path.contains(['\\', ':', '\0'])
			&& path.split('/').all(|part| !matches!(part, "" | "." | ".."))
	}
	let overrides = overrides.trim_end_matches('/');
	if !safe(overrides) {
		return Err(cf_error("Unsafe CurseForge overrides directory"));
	}
	let Some(relative) = name.strip_prefix(&format!("{overrides}/")) else {
		return Ok(None);
	};
	let relative = relative.trim_end_matches('/');
	if !safe(relative) {
		return Err(cf_error(format!(
			"Unsafe CurseForge override path: {name}"
		)));
	}
	Ok(Some(PathBuf::from(relative)))
}

fn copy_overrides(
	zip_bytes: &[u8],
	overrides: &str,
	target: Option<&Path>,
) -> crate::Result<()> {
	let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))
		.map_err(|e| {
			cf_error(format!("Invalid CurseForge modpack archive: {e}"))
		})?;
	for index in 0..archive.len() {
		let mut entry = archive
			.by_index(index)
			.map_err(|e| cf_error(e.to_string()))?;
		if entry.is_dir() {
			continue;
		}
		let Some(relative) = override_relative_path(overrides, entry.name())?
		else {
			continue;
		};
		if entry
			.unix_mode()
			.is_some_and(|mode| mode & 0o170000 == 0o120000)
		{
			return Err(cf_error(
				"CurseForge overrides cannot contain symbolic links",
			));
		}
		if let Some(target) = target {
			let destination = target.join(relative);
			if let Some(parent) = destination.parent() {
				std::fs::create_dir_all(parent)?;
			}
			let mut output = std::fs::File::create(destination)?;
			std::io::copy(&mut entry, &mut output)?;
		}
	}
	Ok(())
}

async fn install_modpack_inner(
	api: &client::Api,
	mod_id: u32,
	file_id: u32,
) -> crate::Result<String> {
	let project = api.project(mod_id).await?;
	let icon = project
		.logo
		.and_then(|logo| logo.thumbnail_url.or(logo.url));
	let download = api.prepare(api.file(mod_id, file_id).await?).await?;
	let zip_bytes = api.download(&download).await?;
	let manifest: CfPackManifest = {
		let mut archive = zip::ZipArchive::new(std::io::Cursor::new(
			&zip_bytes,
		))
		.map_err(|e| {
			cf_error(format!("Invalid CurseForge modpack archive: {e}"))
		})?;
		let mut entry = archive.by_name("manifest.json").map_err(|_| {
			cf_error("CurseForge modpack is missing manifest.json")
		})?;
		let mut json = String::new();
		entry.read_to_string(&mut json)?;
		serde_json::from_str(&json)?
	};
	copy_overrides(&zip_bytes, &manifest.overrides, None)?;
	let primary = manifest
		.minecraft
		.mod_loaders
		.iter()
		.find(|l| l.primary)
		.or_else(|| manifest.minecraft.mod_loaders.first());
	let (loader, version) = primary
		.map(|l| parse_loader_id(&l.id))
		.transpose()?
		.unwrap_or((crate::state::ModLoader::Vanilla, None));
	let bar = crate::event::emit::init_loading(
		crate::event::LoadingBarType::CfModpackInstall {
			pack_name: manifest.name.clone(),
			icon: icon.clone(),
		},
		manifest.files.iter().filter(|f| f.required).count().max(1) as f64,
		"Preparing mods",
	)
	.await
	.ok();
	let mut files = Vec::new();
	for reference in manifest.files.iter().filter(|f| f.required) {
		files.push(api.file(reference.project_id, reference.file_id).await?);
	}
	let staged = stage_files(api, files, &HashSet::new(), bar.as_ref()).await?;
	let profile_path = crate::api::profile::create::profile_create(
		manifest.name,
		manifest.minecraft.version,
		loader,
		version,
		None,
		None,
		Some(true),
	)
	.await?;
	let result = async {
		crate::api::profile::edit(&profile_path, |profile| {
			profile.install_stage =
				crate::state::ProfileInstallStage::PackInstalling;
			profile.icon_path.clone_from(&icon);
			async { Ok(()) }
		})
		.await?;
		let state = crate::State::get().await?;
		for file in &staged.files {
			let kind = file
				.file
				.file_name
				.to_lowercase()
				.ends_with(".jar")
				.then_some(crate::state::ProjectType::Mod);
			crate::state::Profile::add_project_bytes(
				&profile_path,
				&file.file.file_name,
				Bytes::from(tokio::fs::read(&file.path).await?),
				None,
				kind,
				None,
				&state.io_semaphore,
				&state.pool,
			)
			.await?;
		}
		let base = crate::api::profile::get_full_path(&profile_path).await?;
		copy_overrides(&zip_bytes, &manifest.overrides, Some(&base))?;
		crate::api::profile::edit(&profile_path, |profile| {
			profile.install_stage =
				crate::state::ProfileInstallStage::PackInstalled;
			async { Ok(()) }
		})
		.await?;
		let profile =
			crate::api::profile::get(&profile_path).await?.ok_or_else(
				|| cf_error("The new CurseForge instance disappeared"),
			)?;
		crate::launcher::install_minecraft(&profile, None, false).await?;
		crate::event::emit::emit_profile(
			&profile_path,
			crate::event::ProfilePayloadType::Edited,
		)
		.await?;
		Ok::<_, crate::Error>(profile_path.clone())
	}
	.await;
	if result.is_err() {
		if let Err(error) = crate::api::profile::remove(&profile_path).await {
			tracing::warn!(
				"Could not remove incomplete CurseForge instance {profile_path}: {error}"
			);
		}
	}
	result
}

/// Install a complete CurseForge pack; missing required files fail the installation.
pub async fn install_modpack(mod_id: u32) -> crate::Result<String> {
	let api = client::Api::new()?;
	let file = api.best_file(mod_id, None, None, &HashSet::new()).await?;
	install_modpack_inner(&api, mod_id, file.id).await
}

pub async fn install_modpack_file(
	mod_id: u32,
	file_id: u32,
) -> crate::Result<String> {
	install_modpack_inner(&client::Api::new()?, mod_id, file_id).await
}

#[cfg(test)]
mod tests;
