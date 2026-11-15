use crate::api::Result;
use theseus::curseforge::{
	CfCategory, CfFile, CfFilesResult, CfMod, CfSearchResult,
};
use theseus::data::ProjectType;

pub fn init<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
	tauri::plugin::Builder::new("curseforge")
		.invoke_handler(tauri::generate_handler![
			curseforge_search,
			curseforge_get_categories,
			curseforge_get_mod,
			curseforge_get_mods,
			curseforge_get_mod_files,
			curseforge_get_file,
			curseforge_get_file_changelog,
			curseforge_install_mod,
			curseforge_install_mod_file,
			curseforge_get_mod_description,
			curseforge_install_modpack,
			curseforge_install_modpack_file,
		])
		.build()
}

#[tauri::command]
pub async fn curseforge_search(
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
) -> Result<CfSearchResult> {
	Ok(theseus::curseforge::search_filtered(
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
	.await?)
}

#[tauri::command]
pub async fn curseforge_get_categories(
	class_id: u32,
) -> Result<Vec<CfCategory>> {
	Ok(theseus::curseforge::get_categories(class_id).await?)
}

#[tauri::command]
pub async fn curseforge_get_mod(mod_id: u32) -> Result<CfMod> {
	Ok(theseus::curseforge::get_mod(mod_id).await?)
}

#[tauri::command]
pub async fn curseforge_get_mods(mod_ids: Vec<u32>) -> Result<Vec<CfMod>> {
	Ok(theseus::curseforge::get_mods(mod_ids).await?)
}

#[tauri::command]
pub async fn curseforge_get_mod_files(
	mod_id: u32,
	game_version: Option<String>,
	loader: Option<String>,
	page: Option<u32>,
	page_size: Option<u32>,
) -> Result<CfFilesResult> {
	Ok(theseus::curseforge::get_mod_files(
		mod_id,
		game_version,
		loader,
		page,
		page_size,
	)
	.await?)
}

#[tauri::command]
pub async fn curseforge_get_file(mod_id: u32, file_id: u32) -> Result<CfFile> {
	Ok(theseus::curseforge::get_file(mod_id, file_id).await?)
}

#[tauri::command]
pub async fn curseforge_get_file_changelog(
	mod_id: u32,
	file_id: u32,
) -> Result<String> {
	Ok(theseus::curseforge::get_file_changelog(mod_id, file_id).await?)
}

/// Install a CurseForge mod into an instance. Finds the best compatible file,
/// downloads it via the official API download-url endpoint, and adds it to the profile.
#[tauri::command]
pub async fn curseforge_install_mod(
	profile_path: &str,
	mod_id: u32,
	game_version: Option<String>,
	loader: Option<String>,
	project_type: Option<String>,
) -> Result<String> {
	let pt = project_type.as_deref().and_then(|s| match s {
		"mod" => Some(ProjectType::Mod),
		"resourcepack" => Some(ProjectType::ResourcePack),
		"shader" => Some(ProjectType::ShaderPack),
		_ => None, // modpack and unknown types: let add_project_bytes infer from contents
	});

	Ok(theseus::curseforge::install_mod(
		profile_path,
		mod_id,
		game_version,
		loader,
		pt,
	)
	.await?)
}

#[tauri::command]
pub async fn curseforge_get_mod_description(mod_id: u32) -> Result<String> {
	Ok(theseus::curseforge::get_mod_description(mod_id).await?)
}

/// Download a CurseForge modpack and install it as a new instance.
/// Returns the path of the newly created profile.
#[tauri::command]
pub async fn curseforge_install_modpack(mod_id: u32) -> Result<String> {
	Ok(theseus::curseforge::install_modpack(mod_id).await?)
}

/// Install a specific CurseForge mod file (by file ID) into an existing instance.
#[tauri::command]
pub async fn curseforge_install_mod_file(
	profile_path: &str,
	mod_id: u32,
	file_id: u32,
	file_name: &str,
	project_type: Option<String>,
) -> Result<String> {
	let pt = project_type.as_deref().and_then(|s| match s {
		"mod" => Some(ProjectType::Mod),
		"resourcepack" => Some(ProjectType::ResourcePack),
		"shader" => Some(ProjectType::ShaderPack),
		_ => None,
	});
	Ok(theseus::curseforge::install_mod_file(
		profile_path,
		mod_id,
		file_id,
		file_name,
		pt,
	)
	.await?)
}

/// Install a specific CurseForge modpack file (by file ID) as a new instance.
/// Returns the path of the newly created profile.
#[tauri::command]
pub async fn curseforge_install_modpack_file(
	mod_id: u32,
	file_id: u32,
) -> Result<String> {
	Ok(theseus::curseforge::install_modpack_file(mod_id, file_id).await?)
}
