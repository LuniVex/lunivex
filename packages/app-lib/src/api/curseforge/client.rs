use super::*;
use serde::de::DeserializeOwned;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Duration;

const API_PAGE_SIZE: u32 = 50;
const RESULT_LIMIT: u32 = 10_000;

pub(super) struct Api {
	client: reqwest::Client,
	download_client: reqwest::Client,
	base_url: String,
}

#[derive(Deserialize)]
struct Page<T> {
	data: Vec<T>,
	pagination: CfPagination,
}

pub(super) struct Download {
	pub file: CfFile,
	url: String,
}

fn failure(message: impl Into<String>) -> crate::Error {
	crate::ErrorKind::OtherError(message.into()).into()
}

impl Api {
	#[cfg(test)]
	pub(super) fn for_test(base_url: String) -> Self {
		let mut headers = reqwest::header::HeaderMap::new();
		headers.insert(
			"x-api-key",
			reqwest::header::HeaderValue::from_static("test-api-key"),
		);
		Self {
			client: reqwest::Client::builder()
				.no_proxy()
				.default_headers(headers)
				.timeout(Duration::from_secs(3))
				.build()
				.unwrap(),
			download_client: reqwest::Client::builder()
				.no_proxy()
				.timeout(Duration::from_secs(3))
				.build()
				.unwrap(),
			base_url,
		}
	}

	pub fn new() -> crate::Result<Self> {
		Ok(Self {
			client: cf_client(cf_api_key()?)?,
			download_client: reqwest::Client::builder()
				.https_only(true)
				.user_agent(crate::launcher_user_agent())
				.connect_timeout(Duration::from_secs(15))
				.timeout(Duration::from_secs(120))
				.build()?,
			base_url: CF_API_URL.to_string(),
		})
	}

	async fn get<T: DeserializeOwned>(
		&self,
		path: &str,
		params: &[(&str, String)],
	) -> crate::Result<T> {
		Ok(self
			.client
			.get(format!("{}{path}", self.base_url))
			.query(params)
			.send()
			.await?
			.error_for_status()?
			.json()
			.await?)
	}

	async fn page<T: DeserializeOwned>(
		&self,
		path: &str,
		mut params: Vec<(&str, String)>,
		page: u32,
		page_size: u32,
	) -> crate::Result<Page<T>> {
		if !(1..=100).contains(&page_size) {
			return Err(failure(
				"CurseForge page size must be between 1 and 100",
			));
		}
		let start = page.checked_mul(page_size).filter(|i| *i < RESULT_LIMIT)
			.ok_or_else(|| failure("CurseForge exposes at most 10,000 search results. Narrow the search to see more."))?;
		let end = start.saturating_add(page_size).min(RESULT_LIMIT);
		let mut index = start;
		let mut data = Vec::new();
		let mut total_count = 0;
		while index < end {
			let size = (end - index).min(API_PAGE_SIZE);
			params.retain(|(key, _)| *key != "index" && *key != "pageSize");
			params.push(("index", index.to_string()));
			params.push(("pageSize", size.to_string()));
			let response: Page<T> = self.get(path, &params).await?;
			total_count = response.pagination.total_count.min(RESULT_LIMIT);
			let count = response.data.len() as u32;
			if response.pagination.index != index
				|| count > size
				|| response.pagination.result_count != count
			{
				return Err(failure(
					"CurseForge returned an inconsistent result page",
				));
			}
			data.extend(response.data);
			index += count;
			if count == 0 || index >= total_count {
				break;
			}
		}
		Ok(Page {
			pagination: CfPagination {
				index: start,
				page_size,
				result_count: data.len() as u32,
				total_count,
			},
			data,
		})
	}

	pub async fn search(
		&self,
		query: Option<String>,
		game_version: Option<String>,
		loader: Option<String>,
		category_id: Option<u32>,
		class_id: u32,
		page: u32,
		page_size: u32,
	) -> crate::Result<CfSearchResult> {
		self.search_filtered(
			query,
			game_version,
			loader,
			category_id,
			class_id,
			page,
			page_size,
			None,
			None,
			None,
		)
		.await
	}

	pub async fn search_filtered(
		&self,
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
		let sort_field = match sort.as_deref() {
			Some("downloads") => "6",
			Some("updated") => "3",
			Some("newest") => "11",
			_ => "2",
		};
		let mut params = vec![
			("gameId", MINECRAFT_GAME_ID.to_string()),
			("classId", class_id.to_string()),
			("sortField", sort_field.to_string()),
			("sortOrder", "desc".to_string()),
		];
		if let Some(query) = query.filter(|q| !q.is_empty()) {
			params.push(("searchFilter", query));
		}
		if let Some(category) = category_id {
			params.push(("categoryId", category.to_string()));
		}
		add_compatibility(
			&mut params,
			game_version.as_deref(),
			loader.as_deref(),
		);
		if let Some(versions) = game_versions.filter(|v| !v.is_empty()) {
			if versions.len() > 4 {
				return Err(failure(
					"CurseForge supports up to four game-version filters at once",
				));
			}
			params.push(("gameVersions", serde_json::to_string(&versions)?));
		}
		if let Some(loaders) = loaders.filter(|v| !v.is_empty()) {
			if loaders.len() > 5 {
				return Err(failure(
					"CurseForge supports up to five loader filters at once",
				));
			}
			let ids = loaders
				.iter()
				.map(|loader| {
					loader_type_id(loader).ok_or_else(|| {
						failure(format!(
							"Unsupported CurseForge loader filter: {loader}"
						))
					})
				})
				.collect::<crate::Result<Vec<_>>>()?;
			params.push(("modLoaderTypes", serde_json::to_string(&ids)?));
		}
		let result = self
			.page("/v1/mods/search", params, page, page_size)
			.await?;
		Ok(CfSearchResult {
			data: result.data,
			pagination: result.pagination,
		})
	}

	pub async fn categories(
		&self,
		class_id: u32,
	) -> crate::Result<Vec<CfCategory>> {
		let result: CfApiResponse<Vec<CfCategory>> = self
			.get(
				"/v1/categories",
				&[
					("gameId", MINECRAFT_GAME_ID.to_string()),
					("classId", class_id.to_string()),
				],
			)
			.await?;
		Ok(result
			.data
			.into_iter()
			.filter(|c| !c.is_class.unwrap_or(false))
			.collect())
	}

	pub async fn project(&self, mod_id: u32) -> crate::Result<CfMod> {
		let result: CfApiResponse<CfMod> =
			self.get(&format!("/v1/mods/{mod_id}"), &[]).await?;
		Ok(result.data)
	}

	pub async fn projects(
		&self,
		mod_ids: Vec<u32>,
	) -> crate::Result<Vec<CfMod>> {
		if mod_ids.is_empty() {
			return Ok(Vec::new());
		}
		let result: CfApiResponse<Vec<CfMod>> = self
			.client
			.post(format!("{}/v1/mods", self.base_url))
			.json(&serde_json::json!({ "modIds": mod_ids }))
			.send()
			.await?
			.error_for_status()?
			.json()
			.await?;
		Ok(result.data)
	}

	pub async fn files(
		&self,
		mod_id: u32,
		game_version: Option<&str>,
		loader: Option<&str>,
		page: u32,
		page_size: u32,
	) -> crate::Result<CfFilesResult> {
		let mut params = Vec::new();
		add_compatibility(&mut params, game_version, loader);
		let result = self
			.page(&format!("/v1/mods/{mod_id}/files"), params, page, page_size)
			.await?;
		Ok(CfFilesResult {
			data: result.data,
			pagination: result.pagination,
		})
	}

	pub async fn file(
		&self,
		mod_id: u32,
		file_id: u32,
	) -> crate::Result<CfFile> {
		let result: CfApiResponse<CfFile> = self
			.get(&format!("/v1/mods/{mod_id}/files/{file_id}"), &[])
			.await?;
		if result.data.id != file_id || result.data.mod_id != mod_id {
			return Err(failure(
				"CurseForge returned mismatched file metadata",
			));
		}
		Ok(result.data)
	}

	pub async fn text(&self, path: &str) -> crate::Result<String> {
		let result: CfApiResponse<String> = self.get(path, &[]).await?;
		Ok(result.data)
	}

	pub async fn best_file(
		&self,
		mod_id: u32,
		game_version: Option<&str>,
		loader: Option<&str>,
		installed_hashes: &HashSet<String>,
	) -> crate::Result<CfFile> {
		let mut best: Option<CfFile> = None;
		for page in 0..(RESULT_LIMIT / API_PAGE_SIZE) {
			let response = self
				.files(mod_id, game_version, loader, page, API_PAGE_SIZE)
				.await?;
			for file in response.data {
				if !file.is_available || file.is_server_pack {
					continue;
				}
				if file.hashes.iter().any(|h| {
					h.algo == 1
						&& installed_hashes.contains(&h.value.to_lowercase())
				}) {
					return Ok(file);
				}
				if best.as_ref().is_none_or(|b| {
					(
						file.release_type,
						std::cmp::Reverse(&file.file_date),
						std::cmp::Reverse(file.id),
					) < (
						b.release_type,
						std::cmp::Reverse(&b.file_date),
						std::cmp::Reverse(b.id),
					)
				}) {
					best = Some(file);
				}
			}
			if response.pagination.result_count == 0
				|| response.pagination.index + response.pagination.result_count
					>= response.pagination.total_count
			{
				break;
			}
		}
		best.ok_or_else(|| failure(format!("No compatible CurseForge file found for project {mod_id}, Minecraft {} and loader {}", game_version.unwrap_or("any"), loader.unwrap_or("any"))))
	}

	pub async fn prepare(&self, file: CfFile) -> crate::Result<Download> {
		let label = format!(
			"{} (project {}, file {})",
			file.file_name, file.mod_id, file.id
		);
		self.download_option(file).await?.ok_or_else(|| failure(format!(
			"CurseForge does not provide a download for {label}. This required file must be obtained manually from CurseForge; automatic installation cannot complete."
		)))
	}

	pub(super) async fn download_option(
		&self,
		file: CfFile,
	) -> crate::Result<Option<Download>> {
		validate_file_name(&file.file_name)?;
		if !file.is_available {
			return Err(failure(format!(
				"CurseForge file {}/{} is unavailable",
				file.mod_id, file.id
			)));
		}
		if let Some(url) =
			file.download_url.as_ref().filter(|url| !url.is_empty())
		{
			return Ok(Some(Download {
				url: url.clone(),
				file,
			}));
		}
		let path =
			format!("/v1/mods/{}/files/{}/download-url", file.mod_id, file.id);
		let result: CfApiResponse<Option<String>> =
			self.get(&path, &[]).await?;
		Ok(result
			.data
			.filter(|url| !url.is_empty())
			.map(|url| Download { file, url }))
	}

	pub async fn download(&self, download: &Download) -> crate::Result<Bytes> {
		let bytes = self
			.download_client
			.get(&download.url)
			.send()
			.await?
			.error_for_status()?
			.bytes()
			.await?;
		validate_download(&download.file, &bytes)?;
		Ok(bytes)
	}

	pub async fn dependencies(
		&self,
		root: CfFile,
		game_version: &str,
		loader: &str,
		installed_hashes: &HashSet<String>,
	) -> crate::Result<Vec<CfFile>> {
		let mut pending = VecDeque::from([root]);
		let mut queued = HashSet::from([pending[0].mod_id]);
		let mut files = HashMap::new();
		let mut order = Vec::new();
		while let Some(file) = pending.pop_front() {
			if files.contains_key(&file.mod_id) {
				continue;
			}
			if files.len() >= 128 {
				return Err(failure(
					"CurseForge dependency graph exceeds 128 projects",
				));
			}
			order.push(file.mod_id);
			files.insert(file.mod_id, file.clone());
			for dep in file.dependencies.as_deref().unwrap_or_default() {
				if dep.relation_type == 3 && queued.insert(dep.mod_id) {
					if queued.len() > 128 {
						return Err(failure(
							"CurseForge dependency graph exceeds 128 projects",
						));
					}
					pending.push_back(
						self.best_file(
							dep.mod_id,
							Some(game_version),
							Some(loader),
							installed_hashes,
						)
						.await?,
					);
				}
			}
		}
		for file in files.values() {
			if file
				.dependencies
				.as_deref()
				.unwrap_or_default()
				.iter()
				.any(|d| d.relation_type == 5 && files.contains_key(&d.mod_id))
			{
				return Err(failure(
					"CurseForge dependency graph contains incompatible projects",
				));
			}
		}
		Ok(order
			.into_iter()
			.rev()
			.filter_map(|id| files.remove(&id))
			.collect())
	}
}

fn add_compatibility(
	params: &mut Vec<(&str, String)>,
	game_version: Option<&str>,
	loader: Option<&str>,
) {
	if let Some(version) = game_version.filter(|v| !v.is_empty()) {
		params.push(("gameVersion", version.to_string()));
	}
	if let Some(loader) = loader.and_then(loader_type_id) {
		params.push(("modLoaderType", loader.to_string()));
	}
}

pub(super) fn validate_file_name(name: &str) -> crate::Result<()> {
	if name.is_empty()
		|| matches!(name, "." | "..")
		|| name.contains(['/', '\\', ':', '\0'])
	{
		return Err(failure("CurseForge returned an unsafe file name"));
	}
	Ok(())
}

pub(super) fn validate_download(
	file: &CfFile,
	bytes: &[u8],
) -> crate::Result<()> {
	use alphelion_core::integrity::{
		Checksum, HashAlgorithm, IntegrityError, IntegritySpec,
	};
	let corrupt =
		|| failure(format!("Corrupt CurseForge download: {}", file.file_name));
	let checksums = file
		.hashes
		.iter()
		.filter(|hash| hash.algo == 1)
		.map(|hash| Checksum::new(HashAlgorithm::Sha1, hash.value.clone()))
		.collect::<Result<Vec<_>, _>>()
		.map_err(|_| corrupt())?;
	IntegritySpec {
		expected_size: file.file_length,
		checksums,
	}
	.verify(bytes)
	.map_err(|error| match error {
		IntegrityError::SizeMismatch { .. } => failure(format!(
			"Incomplete CurseForge download: {}",
			file.file_name
		)),
		_ => corrupt(),
	})?;
	zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| {
		failure(format!(
			"Invalid CurseForge archive {}: {e}",
			file.file_name
		))
	})?;
	Ok(())
}
