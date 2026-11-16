use super::*;
use std::sync::LazyLock;
use tokio::sync::Mutex;

static INSTALL_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CfMissingFile {
	pub mod_id: u32,
	pub file_id: u32,
	pub file_name: String,
	pub url: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CfPendingInstall {
	pub id: String,
	pub mod_id: u32,
	pub file_id: u32,
	pub name: String,
	pub downloaded_files: usize,
	pub total_files: usize,
	pub missing_files: Vec<CfMissingFile>,
	pub error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CfPackInstallResult {
	pub profile_path: Option<String>,
	pub pending: Option<CfPendingInstall>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CfImportResult {
	pub imported: usize,
	pub rejected: Vec<String>,
	pub pending: CfPendingInstall,
}

#[derive(Clone, Deserialize, Serialize)]
struct Entry {
	reference: CfPackFileRef,
	metadata: Option<CfFile>,
	manual: bool,
	source_url: Option<String>,
	#[serde(default)]
	bundled: bool,
}

#[derive(Deserialize, Serialize)]
struct Record {
	schema: u32,
	mod_id: u32,
	file_id: u32,
	archive: CfFile,
	manifest: CfPackManifest,
	icon: Option<String>,
	entries: Vec<Entry>,
	error: Option<String>,
	profile_path: Option<String>,
	completed: bool,
}

pub(super) struct Session {
	directory: PathBuf,
	record: Record,
}

fn lock_error() -> crate::Error {
	cf_error(
		"Another CurseForge installation operation is running. Wait for it to finish.",
	)
}

fn identifier(mod_id: u32, file_id: u32) -> String {
	format!("{mod_id}-{file_id}")
}

fn checked_directory(root: &Path, id: &str) -> crate::Result<PathBuf> {
	let Some((project, file)) = id.split_once('-') else {
		return Err(cf_error("Invalid CurseForge installation ID"));
	};
	let project = project.parse::<u32>().ok().filter(|id| *id > 0);
	let file = file.parse::<u32>().ok().filter(|id| *id > 0);
	match (project, file) {
		(Some(project), Some(file)) if identifier(project, file) == id => {
			Ok(root.join(id))
		}
		_ => Err(cf_error("Invalid CurseForge installation ID")),
	}
}

async fn root_directory() -> crate::Result<PathBuf> {
	Ok(crate::State::get()
		.await?
		.directories
		.settings_dir
		.join("curseforge-installs"))
}

async fn write_atomic(path: &Path, contents: &[u8]) -> crate::Result<()> {
	tokio::fs::create_dir_all(
		path.parent()
			.ok_or_else(|| cf_error("Invalid installation path"))?,
	)
	.await?;
	let partial = path.with_extension("part");
	tokio::fs::write(&partial, contents).await?;
	tokio::fs::rename(&partial, path).await?;
	Ok(())
}

fn read_manifest(bytes: &[u8]) -> crate::Result<CfPackManifest> {
	let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
		.map_err(|e| {
			cf_error(format!("Invalid CurseForge modpack archive: {e}"))
		})?;
	let mut json = String::new();
	archive
		.by_name("manifest.json")
		.map_err(|_| cf_error("CurseForge modpack is missing manifest.json"))?
		.read_to_string(&mut json)?;
	let manifest: CfPackManifest = serde_json::from_str(&json)?;
	copy_overrides(bytes, &manifest.overrides, None)?;
	for loader in &manifest.minecraft.mod_loaders {
		parse_loader_id(&loader.id)?;
	}
	Ok(manifest)
}

fn official_file_page(project: &CfMod, file_id: u32) -> Option<String> {
	let mut url =
		url::Url::parse(project.links.as_ref()?.website_url.as_deref()?)
			.ok()?;
	if url.scheme() != "https"
		|| !matches!(
			url.host_str(),
			Some("www.curseforge.com" | "curseforge.com")
		) || !url.username().is_empty()
		|| url.password().is_some()
	{
		return None;
	}
	url.set_path(&format!(
		"{}/files/{file_id}",
		url.path().trim_end_matches('/')
	));
	url.set_query(None);
	url.set_fragment(None);
	Some(url.into())
}

struct BundledFiles<'a> {
	archive: zip::ZipArchive<std::io::Cursor<&'a [u8]>>,
	indices: std::collections::HashMap<String, Vec<usize>>,
}

impl<'a> BundledFiles<'a> {
	fn new(bytes: &'a [u8], overrides: &str) -> crate::Result<Self> {
		let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
			.map_err(|error| cf_error(error.to_string()))?;
		let mut indices =
			std::collections::HashMap::<String, Vec<usize>>::new();
		for index in 0..archive.len() {
			let entry = archive
				.by_index(index)
				.map_err(|error| cf_error(error.to_string()))?;
			if entry.is_dir() {
				continue;
			}
			if let Some(relative) =
				override_relative_path(overrides, entry.name())?
				&& let Some(name) =
					relative.file_name().and_then(|name| name.to_str())
			{
				indices.entry(name.to_lowercase()).or_default().push(index);
			}
		}
		Ok(Self { archive, indices })
	}

	fn matching_file(
		&mut self,
		file: &CfFile,
	) -> crate::Result<Option<Vec<u8>>> {
		if !file.hashes.iter().any(|hash| hash.algo == 1) {
			return Ok(None);
		}
		let Some(indices) = self.indices.get(&file.file_name.to_lowercase())
		else {
			return Ok(None);
		};
		let maximum = file
			.file_length
			.unwrap_or(512 * 1024 * 1024)
			.min(512 * 1024 * 1024);
		for &index in indices {
			let entry = self
				.archive
				.by_index(index)
				.map_err(|error| cf_error(error.to_string()))?;
			if entry.size() > maximum
				|| file.file_length.is_some_and(|size| size != entry.size())
			{
				continue;
			}
			let mut bytes = Vec::new();
			entry.take(maximum + 1).read_to_end(&mut bytes)?;
			if bytes.len() as u64 <= maximum
				&& client::validate_download(file, &bytes).is_ok()
			{
				return Ok(Some(bytes));
			}
		}
		Ok(None)
	}
}

impl Session {
	async fn create(
		root: &Path,
		api: &client::Api,
		mod_id: u32,
		file_id: u32,
	) -> crate::Result<Self> {
		let project = api.project(mod_id).await?;
		let archive = api.file(mod_id, file_id).await?;
		let download = api.prepare(archive.clone()).await?;
		let bytes = api.download(&download).await?;
		let manifest = read_manifest(&bytes)?;
		let mut seen = HashSet::new();
		let entries = manifest
			.files
			.iter()
			.filter(|file| file.required)
			.filter(|file| seen.insert((file.project_id, file.file_id)))
			.map(|reference| Entry {
				reference: reference.clone(),
				metadata: None,
				manual: false,
				source_url: None,
				bundled: false,
			})
			.collect();
		let session = Self {
			directory: checked_directory(root, &identifier(mod_id, file_id))?,
			record: Record {
				schema: 1,
				mod_id,
				file_id,
				archive,
				manifest,
				icon: project
					.logo
					.and_then(|logo| logo.thumbnail_url.or(logo.url)),
				entries,
				error: None,
				profile_path: None,
				completed: false,
			},
		};
		write_atomic(&session.directory.join("pack.zip"), &bytes).await?;
		session.save().await?;
		Ok(session)
	}

	async fn load(root: &Path, id: &str) -> crate::Result<Self> {
		let directory = checked_directory(root, id)?;
		let record: Record = serde_json::from_slice(
			&tokio::fs::read(directory.join("state.json")).await?,
		)?;
		if record.schema != 1
			|| identifier(record.mod_id, record.file_id) != id
			|| record.archive.mod_id != record.mod_id
			|| record.archive.id != record.file_id
		{
			return Err(cf_error("Invalid saved CurseForge installation"));
		}
		for entry in &record.entries {
			if let Some(file) = &entry.metadata {
				client::validate_file_name(&file.file_name)?;
				if file.mod_id != entry.reference.project_id
					|| file.id != entry.reference.file_id
				{
					return Err(cf_error(
						"Saved CurseForge file metadata does not match the installation",
					));
				}
			}
		}
		if let Some(path) = &record.profile_path {
			client::validate_file_name(path)?;
		}
		Ok(Self { directory, record })
	}

	async fn save(&self) -> crate::Result<()> {
		write_atomic(
			&self.directory.join("state.json"),
			&serde_json::to_vec(&self.record)?,
		)
		.await
	}

	fn file_path(&self, entry: &Entry) -> PathBuf {
		self.directory.join("files").join(identifier(
			entry.reference.project_id,
			entry.reference.file_id,
		))
	}

	async fn valid_cached_file(&self, entry: &Entry) -> crate::Result<bool> {
		let Some(file) = &entry.metadata else {
			return Ok(false);
		};
		let path = self.file_path(entry);
		match tokio::fs::read(&path).await {
			Ok(bytes) if client::validate_download(file, &bytes).is_ok() => {
				Ok(true)
			}
			Ok(_) => {
				tokio::fs::remove_file(path).await?;
				Ok(false)
			}
			Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
				Ok(false)
			}
			Err(error) => Err(error.into()),
		}
	}

	async fn summary(&self) -> crate::Result<CfPendingInstall> {
		let mut downloaded_files = 0;
		let mut missing_files = Vec::new();
		for entry in &self.record.entries {
			if self.file_path(entry).is_file() {
				downloaded_files += 1;
			} else if entry.manual
				&& let Some(file) = &entry.metadata
			{
				missing_files.push(CfMissingFile {
					mod_id: file.mod_id,
					file_id: file.id,
					file_name: file.file_name.clone(),
					url: entry.source_url.clone(),
				});
			}
		}
		Ok(CfPendingInstall {
			id: identifier(self.record.mod_id, self.record.file_id),
			mod_id: self.record.mod_id,
			file_id: self.record.file_id,
			name: self.record.manifest.name.clone(),
			downloaded_files,
			total_files: self.record.entries.len(),
			missing_files,
			error: self.record.error.clone(),
		})
	}

	async fn archive_bytes(&self, api: &client::Api) -> crate::Result<Vec<u8>> {
		if let Ok(bytes) =
			tokio::fs::read(self.directory.join("pack.zip")).await
			&& client::validate_download(&self.record.archive, &bytes).is_ok()
		{
			return Ok(bytes);
		}
		let download = api.prepare(self.record.archive.clone()).await?;
		let bytes = api.download(&download).await?;
		write_atomic(&self.directory.join("pack.zip"), &bytes).await?;
		Ok(bytes.to_vec())
	}

	async fn prepare_files(
		&mut self,
		api: &client::Api,
		zip_bytes: Option<&[u8]>,
		bar: Option<&crate::event::LoadingBarId>,
	) -> crate::Result<bool> {
		let mut bundled = zip_bytes
			.map(|bytes| {
				BundledFiles::new(bytes, &self.record.manifest.overrides)
			})
			.transpose()?;
		let mut complete = true;
		let mut names = HashSet::new();
		for index in 0..self.record.entries.len() {
			let mut entry = self.record.entries[index].clone();
			let file = match entry.metadata.clone() {
				Some(file) => file,
				None => {
					api.file(
						entry.reference.project_id,
						entry.reference.file_id,
					)
					.await?
				}
			};
			client::validate_file_name(&file.file_name)?;
			if !file.is_available {
				return Err(cf_error(format!(
					"Required file {} is unavailable on CurseForge",
					file.file_name
				)));
			}
			if !names.insert(file.file_name.to_lowercase()) {
				return Err(cf_error(format!(
					"CurseForge returned duplicate file name {}",
					file.file_name
				)));
			}
			entry.metadata = Some(file.clone());
			self.record.entries[index] = entry.clone();
			entry.bundled = false;
			if let Some(bundled) = &mut bundled
				&& let Some(bytes) = bundled.matching_file(&file)?
			{
				write_atomic(&self.file_path(&entry), &bytes).await?;
				entry.bundled = true;
				entry.manual = false;
			}
			if !self.valid_cached_file(&entry).await? {
				let result = async {
					if file.download_url.as_deref().is_none_or(str::is_empty) {
						let project = api.project(file.mod_id).await?;
						entry.source_url =
							official_file_page(&project, file.id);
						if project.allow_mod_distribution == Some(false) {
							entry.manual = true;
							return Ok(None);
						}
					}
					api.download_option(file).await
				}
				.await;
				match result {
					Ok(Some(download)) => {
						let bytes = api.download(&download).await?;
						write_atomic(&self.file_path(&entry), &bytes).await?;
						entry.manual = false;
					}
					Ok(None) => {
						entry.manual = true;
						complete = false;
					}
					Err(error) => {
						self.record.entries[index] = entry;
						self.save().await?;
						return Err(error);
					}
				}
			}
			self.record.entries[index] = entry;
			self.save().await?;
			if let Some(bar) = bar {
				let _ = crate::event::emit::emit_loading(
					bar,
					1.0,
					Some("Saving modpack files"),
				);
			}
		}
		Ok(complete)
	}

	async fn import(
		&mut self,
		paths: &[String],
	) -> crate::Result<CfImportResult> {
		if paths.len() > 256 {
			return Err(cf_error(
				"Select at most 256 downloaded files at a time",
			));
		}
		let mut imported = 0;
		let mut rejected = Vec::new();
		for source in paths {
			let source = Path::new(source);
			let label =
				source.file_name().unwrap_or_default().to_string_lossy();
			let result = async {
				let metadata = tokio::fs::metadata(source).await?;
				let maximum = self.record.entries.iter().filter(|entry| entry.manual)
					.filter_map(|entry| entry.metadata.as_ref())
					.map(|file| file.file_length.unwrap_or(512 * 1024 * 1024))
					.max().unwrap_or(0);
				if !metadata.is_file() || metadata.len() > maximum {
					return Err(cf_error("The file does not match any remaining required download"));
				}
				let bytes = tokio::fs::read(source).await?;
				let hash = sha1_smol::Sha1::from(&bytes).hexdigest();
				let entry = self.record.entries.iter().find(|entry| entry.manual
					&& entry.metadata.as_ref().is_some_and(|file| file.hashes.iter()
						.any(|expected| expected.algo == 1 && expected.value.eq_ignore_ascii_case(&hash))))
					.ok_or_else(|| cf_error("The file checksum does not match a required download. Select the exact version listed."))?;
				client::validate_download(entry.metadata.as_ref().unwrap(), &bytes)?;
				write_atomic(&self.file_path(entry), &bytes).await?;
				Ok::<_, crate::Error>(())
			}.await;
			match result {
				Ok(()) => imported += 1,
				Err(error) => rejected.push(format!("{label}: {error}")),
			}
		}
		self.save().await?;
		Ok(CfImportResult {
			imported,
			rejected,
			pending: self.summary().await?,
		})
	}

	async fn finish(&mut self, bytes: &[u8]) -> crate::Result<String> {
		let state = crate::State::get().await?;
		let marker_name = ".lunivex-curseforge-install";
		let id = identifier(self.record.mod_id, self.record.file_id);
		if self.record.profile_path.is_none() {
			tokio::fs::create_dir_all(state.directories.profiles_dir()).await?;
			let mut directories =
				tokio::fs::read_dir(state.directories.profiles_dir()).await?;
			while let Some(directory) = directories.next_entry().await? {
				if tokio::fs::read(directory.path().join(marker_name))
					.await
					.ok()
					.as_deref() == Some(id.as_bytes())
				{
					let path =
						directory.file_name().to_string_lossy().to_string();
					if crate::api::profile::get(&path).await?.is_some_and(
						|profile| {
							profile.install_stage
								!= crate::state::ProfileInstallStage::Installed
						},
					) {
						self.record.profile_path = Some(path);
						break;
					}
				}
			}
		}
		let profile_path = if let Some(path) = &self.record.profile_path {
			let base = crate::api::profile::get_full_path(path).await?;
			if tokio::fs::read(base.join(marker_name)).await?.as_slice()
				!= id.as_bytes()
			{
				return Err(cf_error(
					"The saved instance does not belong to this installation. Its files were left untouched.",
				));
			}
			path.clone()
		} else {
			let primary = self
				.record
				.manifest
				.minecraft
				.mod_loaders
				.iter()
				.find(|loader| loader.primary)
				.or_else(|| self.record.manifest.minecraft.mod_loaders.first());
			let (loader, version) = primary
				.map(|loader| parse_loader_id(&loader.id))
				.transpose()?
				.unwrap_or((crate::state::ModLoader::Vanilla, None));
			let path = crate::api::profile::create::profile_create(
				self.record.manifest.name.clone(),
				self.record.manifest.minecraft.version.clone(),
				loader,
				version,
				None,
				None,
				Some(true),
			)
			.await?;
			let base = crate::api::profile::get_full_path(&path).await?;
			write_atomic(&base.join(marker_name), id.as_bytes()).await?;
			self.record.profile_path = Some(path.clone());
			self.save().await?;
			path
		};
		if crate::api::profile::get(&profile_path).await?.is_some_and(
			|profile| {
				profile.install_stage
					== crate::state::ProfileInstallStage::Installed
			},
		) {
			return self.complete(profile_path).await;
		}
		let files = self
			.record
			.entries
			.iter()
			.map(|entry| {
				Ok(StagedFile {
					file: entry.metadata.clone().ok_or_else(|| {
						cf_error("Missing saved file metadata")
					})?,
					path: self.file_path(entry),
					bundled: entry.bundled,
				})
			})
			.collect::<crate::Result<Vec<_>>>()?;
		let result = finish_modpack_profile(
			&profile_path,
			&self.record.manifest,
			bytes,
			&files,
			&self.record.icon,
		)
		.await;
		if let Err(error) = result {
			let _ = crate::api::profile::edit(&profile_path, |profile| {
				profile.install_stage =
					crate::state::ProfileInstallStage::PackInstalling;
				async { Ok(()) }
			})
			.await;
			return Err(error);
		}
		self.complete(profile_path).await
	}

	async fn complete(
		&mut self,
		profile_path: String,
	) -> crate::Result<String> {
		self.record.completed = true;
		self.record.error = None;
		self.save().await?;
		if let Err(error) =
			tokio::fs::remove_dir_all(self.directory.join("files")).await
		{
			tracing::warn!(
				"Could not clean up completed CurseForge downloads: {error}"
			);
		}
		let _ = tokio::fs::remove_file(self.directory.join("pack.zip")).await;
		Ok(profile_path)
	}

	async fn run(
		&mut self,
		api: &client::Api,
	) -> crate::Result<CfPackInstallResult> {
		self.record.error = None;
		let bar = crate::event::emit::init_loading(
			crate::event::LoadingBarType::CfModpackInstall {
				pack_name: self.record.manifest.name.clone(),
				icon: self.record.icon.clone(),
			},
			self.record.entries.len().max(1) as f64,
			"Preparing modpack files",
		)
		.await
		.ok();
		let result = async {
			let bytes = self.archive_bytes(api).await?;
			let manifest = read_manifest(&bytes)?;
			let expected = manifest
				.files
				.iter()
				.filter(|file| file.required)
				.map(|file| (file.project_id, file.file_id))
				.collect::<HashSet<_>>();
			let saved = self
				.record
				.entries
				.iter()
				.map(|entry| {
					(entry.reference.project_id, entry.reference.file_id)
				})
				.collect::<HashSet<_>>();
			if expected != saved {
				return Err(cf_error(
					"Saved installation files do not match the original modpack",
				));
			}
			self.record.manifest = manifest;
			if self.prepare_files(api, Some(&bytes), bar.as_ref()).await? {
				return Ok(Some(self.finish(&bytes).await?));
			}
			Ok::<_, crate::Error>(None)
		}
		.await;
		if let Some(bar) = &bar {
			let _ = crate::event::emit::emit_loading(
				bar,
				self.record.entries.len().max(1) as f64,
				Some("Modpack preparation finished"),
			);
		}
		match result {
			Ok(Some(profile_path)) => Ok(CfPackInstallResult {
				profile_path: Some(profile_path),
				pending: None,
			}),
			result => {
				if let Err(error) = result {
					self.record.error = Some(error.to_string());
				}
				self.save().await?;
				Ok(CfPackInstallResult {
					profile_path: None,
					pending: Some(self.summary().await?),
				})
			}
		}
	}
}

async fn sessions(root: &Path) -> crate::Result<Vec<Session>> {
	let mut entries = match tokio::fs::read_dir(root).await {
		Ok(entries) => entries,
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
			return Ok(Vec::new());
		}
		Err(error) => return Err(error.into()),
	};
	let mut sessions = Vec::new();
	while let Some(entry) = entries.next_entry().await? {
		if !entry.file_type().await?.is_dir() {
			continue;
		}
		let id = entry.file_name().to_string_lossy().to_string();
		match Session::load(root, &id).await {
			Ok(session) => sessions.push(session),
			Err(error) => tracing::warn!(
				"Could not read saved CurseForge installation {id}: {error}"
			),
		}
	}
	sessions.sort_by_key(|session| std::cmp::Reverse(session.record.file_id));
	Ok(sessions)
}

pub async fn pending_installs() -> crate::Result<Vec<CfPendingInstall>> {
	let _guard = INSTALL_LOCK.try_lock().map_err(|_| lock_error())?;
	let mut result = Vec::new();
	for session in sessions(&root_directory().await?).await? {
		if !session.record.completed {
			result.push(session.summary().await?);
		}
	}
	Ok(result)
}

pub async fn start_install(
	mod_id: u32,
	file_id: Option<u32>,
) -> crate::Result<CfPackInstallResult> {
	let _guard = INSTALL_LOCK.try_lock().map_err(|_| lock_error())?;
	let root = root_directory().await?;
	let api = client::Api::new()?;
	let mut matching = sessions(&root).await?.into_iter().find(|session| {
		session.record.mod_id == mod_id
			&& file_id.is_none_or(|id| session.record.file_id == id)
	});
	if let Some(session) = &matching
		&& session.record.completed
	{
		if let Some(path) = &session.record.profile_path
			&& crate::api::profile::get(path).await?.is_some()
		{
			return Ok(CfPackInstallResult {
				profile_path: Some(path.clone()),
				pending: None,
			});
		}
		tokio::fs::remove_dir_all(&session.directory).await?;
		matching = None;
	}
	let mut session = match matching {
		Some(session) => session,
		None => {
			let file_id = match file_id {
				Some(id) => id,
				None => {
					api.best_file(mod_id, None, None, &HashSet::new()).await?.id
				}
			};
			Session::create(&root, &api, mod_id, file_id).await?
		}
	};
	session.run(&api).await
}

pub async fn resume_install(id: &str) -> crate::Result<CfPackInstallResult> {
	let _guard = INSTALL_LOCK.try_lock().map_err(|_| lock_error())?;
	let mut session = Session::load(&root_directory().await?, id).await?;
	if session.record.completed {
		return Ok(CfPackInstallResult {
			profile_path: session.record.profile_path,
			pending: None,
		});
	}
	session.run(&client::Api::new()?).await
}

pub async fn import_install_files(
	id: &str,
	paths: Vec<String>,
) -> crate::Result<CfImportResult> {
	let _guard = INSTALL_LOCK.try_lock().map_err(|_| lock_error())?;
	let mut session = Session::load(&root_directory().await?, id).await?;
	if session.record.completed {
		return Err(cf_error("This installation is already complete"));
	}
	session.import(&paths).await
}

pub async fn discard_install(id: &str) -> crate::Result<()> {
	let _guard = INSTALL_LOCK.try_lock().map_err(|_| lock_error())?;
	let root = root_directory().await?;
	let directory = checked_directory(&root, id)?;
	Session::load(&root, id).await?;
	tokio::fs::remove_dir_all(directory).await?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::super::tests::{Server, archive, file, response};
	use super::*;
	use serde_json::json;

	fn metadata(mod_id: u32, bytes: &[u8], url: Option<String>) -> CfFile {
		let mut file = file(mod_id, mod_id);
		file.file_length = Some(bytes.len() as u64);
		file.hashes = vec![CfFileHash {
			algo: 1,
			value: sha1_smol::Sha1::from(bytes).hexdigest(),
		}];
		file.download_url = url;
		file
	}

	async fn fixture(root: &Path, files: Vec<CfFile>) -> Session {
		let references = files
			.iter()
			.map(|file| CfPackFileRef {
				project_id: file.mod_id,
				file_id: file.id,
				required: true,
			})
			.collect::<Vec<_>>();
		let manifest = CfPackManifest {
			name: "Recovery fixture".into(),
			overrides: "overrides".into(),
			minecraft: CfPackMinecraft {
				version: "1.20.1".into(),
				mod_loaders: Vec::new(),
			},
			files: references.clone(),
		};
		let zip = archive(&[(
			"manifest.json",
			&serde_json::to_vec(&manifest).unwrap(),
		)]);
		let session = Session {
			directory: checked_directory(root, "99-99").unwrap(),
			record: Record {
				schema: 1,
				mod_id: 99,
				file_id: 99,
				archive: metadata(99, &zip, None),
				manifest,
				icon: None,
				error: None,
				profile_path: None,
				completed: false,
				entries: files
					.into_iter()
					.zip(references)
					.map(|(file, reference)| Entry {
						reference,
						metadata: Some(file),
						manual: false,
						source_url: None,
						bundled: false,
					})
					.collect(),
			},
		};
		write_atomic(&session.directory.join("pack.zip"), &zip)
			.await
			.unwrap();
		session.save().await.unwrap();
		session
	}

	fn project(mod_id: u32, allowed: bool) -> serde_json::Value {
		json!({
			"id": mod_id, "name": "Fixture project", "slug": "fixture",
			"summary": "", "downloadCount": 0, "categories": [], "authors": [],
			"latestFilesIndexes": [], "dateModified": "2026-01-01", "isAvailable": true,
			"allowModDistribution": allowed,
			"links": { "websiteUrl": "https://www.curseforge.com/minecraft/data-packs/fixture" }
		})
	}

	#[tokio::test]
	async fn verified_bundled_datapacks_keep_the_pack_location_without_download_requests()
	 {
		let bytes =
			archive(&[("pack.mcmeta", b"{}"), ("data/example.json", b"{}")]);
		let mut file = metadata(2, &bytes, None);
		file.file_name = "example.zip".into();
		let pack = archive(&[
			("overrides/", &b""[..]),
			("overrides/config/paxi/datapacks/example.zip", &bytes),
		]);
		let root = tempfile::tempdir().unwrap();
		let mut session = fixture(root.path(), vec![file]).await;
		let api = Server::new(|_| {
			panic!("Bundled files must not request a download")
		});
		assert!(
			session
				.prepare_files(&api.api(), Some(&pack), None)
				.await
				.unwrap()
		);
		assert!(session.record.entries[0].bundled);
		assert!(session.summary().await.unwrap().missing_files.is_empty());
		assert!(api.requests.lock().unwrap().is_empty());
		let target = root.path().join("instance");
		copy_overrides(&pack, "overrides", Some(&target)).unwrap();
		assert_eq!(
			std::fs::read(target.join("config/paxi/datapacks/example.zip"))
				.unwrap(),
			bytes
		);
		assert!(!target.join("datapacks/example.zip").exists());
		assert_eq!(
			pack_datapack_directory(&pack, "overrides").unwrap(),
			Some(PathBuf::from("config/paxi/datapacks"))
		);
	}

	#[tokio::test]
	async fn a_bundled_file_with_the_wrong_checksum_still_requires_the_exact_version()
	 {
		let bytes =
			archive(&[("pack.mcmeta", b"{}"), ("data/correct.json", b"{}")]);
		let wrong =
			archive(&[("pack.mcmeta", b"{}"), ("data/wrong.json", b"{}")]);
		let mut file = metadata(2, &bytes, None);
		file.file_name = "example.zip".into();
		let pack =
			archive(&[("overrides/config/paxi/datapacks/example.zip", &wrong)]);
		let root = tempfile::tempdir().unwrap();
		let mut session = fixture(root.path(), vec![file]).await;
		let api = Server::new(|_| response(json!({"data": project(2, false)})));
		assert!(
			!session
				.prepare_files(&api.api(), Some(&pack), None)
				.await
				.unwrap()
		);
		assert!(!session.record.entries[0].bundled);
		assert_eq!(session.summary().await.unwrap().missing_files.len(), 1);
		assert!(!session.file_path(&session.record.entries[0]).exists());
		assert_eq!(
			pack_datapack_directory(&archive(&[]), "overrides").unwrap(),
			None
		);
	}

	#[tokio::test]
	async fn restricted_file_preserves_other_downloads_and_resumes_after_verified_import()
	 {
		let bytes = archive(&[("fabric.mod.json", b"{}")]);
		let data = bytes.clone();
		let cdn = Server::new(move |_| (200, data.clone()));
		let api = Server::new(|target| {
			assert_eq!(target, "/v1/mods/2");
			response(json!({ "data": project(2, false) }))
		});
		let root = tempfile::tempdir().unwrap();
		let mut session = fixture(
			root.path(),
			vec![
				metadata(1, &bytes, Some(format!("{}/1", cdn.url))),
				metadata(2, &bytes, None),
				metadata(3, &bytes, Some(format!("{}/3", cdn.url))),
			],
		)
		.await;
		let outcome = session.run(&api.api()).await.unwrap();
		let pending = outcome.pending.unwrap();
		assert!(outcome.profile_path.is_none());
		assert_eq!(pending.downloaded_files, 2);
		assert_eq!(pending.missing_files.len(), 1);
		assert_eq!(pending.missing_files[0].file_id, 2);
		assert_eq!(
			pending.missing_files[0].url.as_deref(),
			Some(
				"https://www.curseforge.com/minecraft/data-packs/fixture/files/2"
			)
		);
		assert_eq!(cdn.requests.lock().unwrap().len(), 2);
		drop(session);

		let mut restored = Session::load(root.path(), "99-99").await.unwrap();
		let selected = root.path().join("browser-renamed-file (1).zip");
		tokio::fs::write(&selected, &bytes).await.unwrap();
		let imported = restored
			.import(&[selected.to_string_lossy().to_string()])
			.await
			.unwrap();
		assert_eq!(imported.imported, 1);
		assert!(imported.rejected.is_empty());
		assert!(imported.pending.missing_files.is_empty());
		assert!(
			restored
				.prepare_files(&api.api(), None, None)
				.await
				.unwrap()
		);
		assert_eq!(cdn.requests.lock().unwrap().len(), 2);
		assert_eq!(api.requests.lock().unwrap().len(), 1);
	}

	#[tokio::test]
	async fn a_same_named_wrong_version_is_rejected_without_losing_saved_files()
	{
		let bytes =
			archive(&[("pack.mcmeta", b"{}"), ("data/correct.json", b"{}")]);
		let wrong =
			archive(&[("pack.mcmeta", b"{}"), ("data/wrong.json", b"{}")]);
		let root = tempfile::tempdir().unwrap();
		let mut session =
			fixture(root.path(), vec![metadata(2, &bytes, None)]).await;
		session.record.entries[0].manual = true;
		session.save().await.unwrap();
		let path = root.path().join(
			&session.record.entries[0]
				.metadata
				.as_ref()
				.unwrap()
				.file_name,
		);
		tokio::fs::write(&path, wrong).await.unwrap();
		let result = session
			.import(&[path.to_string_lossy().to_string()])
			.await
			.unwrap();
		assert_eq!(result.imported, 0);
		assert_eq!(result.rejected.len(), 1);
		assert_eq!(result.pending.missing_files.len(), 1);
		assert!(!session.file_path(&session.record.entries[0]).exists());
	}

	#[tokio::test]
	async fn network_failure_saves_progress_and_is_not_a_manual_download_restriction()
	 {
		let bytes = archive(&[("fabric.mod.json", b"{}")]);
		let data = bytes.clone();
		let cdn = Server::new(move |target| {
			if target == "/2" {
				(503, b"unavailable".to_vec())
			} else {
				(200, data.clone())
			}
		});
		let api =
			Server::new(|_| panic!("Metadata URLs should be used directly"));
		let root = tempfile::tempdir().unwrap();
		let mut session = fixture(
			root.path(),
			vec![
				metadata(1, &bytes, Some(format!("{}/1", cdn.url))),
				metadata(2, &bytes, Some(format!("{}/2", cdn.url))),
			],
		)
		.await;
		let outcome = session.run(&api.api()).await.unwrap();
		let pending = outcome.pending.unwrap();
		assert_eq!(pending.downloaded_files, 1);
		assert!(pending.missing_files.is_empty());
		assert!(pending.error.unwrap().contains("503"));
		let restored = Session::load(root.path(), "99-99").await.unwrap();
		assert!(
			restored
				.valid_cached_file(&restored.record.entries[0])
				.await
				.unwrap()
		);
		assert_eq!(restored.summary().await.unwrap().downloaded_files, 1);
	}

	#[tokio::test]
	async fn forbidden_download_url_with_distribution_enabled_remains_an_access_error()
	 {
		let bytes = archive(&[("fabric.mod.json", b"{}")]);
		let root = tempfile::tempdir().unwrap();
		let api = Server::new(|target| {
			if target == "/v1/mods/2" {
				response(json!({"data": project(2, true)}))
			} else {
				(403, b"forbidden".to_vec())
			}
		});
		let mut session =
			fixture(root.path(), vec![metadata(2, &bytes, None)]).await;
		let pending = session.run(&api.api()).await.unwrap().pending.unwrap();
		assert!(pending.missing_files.is_empty());
		assert!(pending.error.unwrap().contains("403"));
	}

	#[tokio::test]
	async fn corrupt_saved_bytes_are_rejected_before_resume() {
		let bytes = archive(&[("fabric.mod.json", b"{}")]);
		let root = tempfile::tempdir().unwrap();
		let session =
			fixture(root.path(), vec![metadata(1, &bytes, None)]).await;
		let path = session.file_path(&session.record.entries[0]);
		write_atomic(&path, b"broken cache").await.unwrap();
		assert!(
			!session
				.valid_cached_file(&session.record.entries[0])
				.await
				.unwrap()
		);
		assert!(!path.exists());
		assert!(session.directory.join("state.json").exists());
	}

	#[test]
	fn job_ids_cannot_escape_the_install_directory() {
		for id in [
			"../1-2",
			"1-2/../../world",
			"1-2\\world",
			"1-2-extra",
			"01-2",
			"0-2",
			"1-0",
		] {
			assert!(
				checked_directory(Path::new("/installations"), id).is_err(),
				"{id}"
			);
		}
		assert_eq!(
			checked_directory(Path::new("/installations"), "12-34").unwrap(),
			Path::new("/installations/12-34")
		);
	}

	#[tokio::test]
	async fn damaged_saved_identity_does_not_authorize_a_different_installation()
	 {
		let bytes = archive(&[("fabric.mod.json", b"{}")]);
		let root = tempfile::tempdir().unwrap();
		let mut session =
			fixture(root.path(), vec![metadata(1, &bytes, None)]).await;
		session.record.mod_id = 100;
		session.save().await.unwrap();
		assert!(Session::load(root.path(), "99-99").await.is_err());
	}

	#[tokio::test]
	async fn supplied_metadata_url_never_contacts_the_download_url_endpoint() {
		let api = Server::new(|_| panic!("No API request should be needed"));
		let bytes = archive(&[("fabric.mod.json", b"{}")]);
		let data = bytes.clone();
		let cdn = Server::new(move |_| (200, data.clone()));
		let client = api.api();
		let download = client
			.prepare(metadata(1, &bytes, Some(format!("{}/file", cdn.url))))
			.await
			.unwrap();
		assert_eq!(
			client.download(&download).await.unwrap().as_ref(),
			bytes.as_slice()
		);
		assert!(api.requests.lock().unwrap().is_empty());
		assert!(
			!cdn.requests.lock().unwrap()[0]
				.to_lowercase()
				.contains("x-api-key")
		);
	}
}
