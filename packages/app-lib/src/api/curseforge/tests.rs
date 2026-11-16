use super::*;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{
	Arc, Mutex,
	atomic::{AtomicBool, Ordering},
};
use std::thread;

pub(super) struct Server {
	pub(super) url: String,
	pub(super) requests: Arc<Mutex<Vec<String>>>,
	stop: Arc<AtomicBool>,
	thread: Option<thread::JoinHandle<()>>,
}

impl Server {
	pub(super) fn new(
		handler: impl Fn(&str) -> (u16, Vec<u8>) + Send + 'static,
	) -> Self {
		let listener = TcpListener::bind("127.0.0.1:0").unwrap();
		let url = format!("http://{}", listener.local_addr().unwrap());
		listener.set_nonblocking(true).unwrap();
		let requests = Arc::new(Mutex::new(Vec::new()));
		let stop = Arc::new(AtomicBool::new(false));
		let seen = requests.clone();
		let stopped = stop.clone();
		let thread = thread::spawn(move || {
			while !stopped.load(Ordering::SeqCst) {
				let (mut stream, _) = match listener.accept() {
					Ok(value) => value,
					Err(error)
						if error.kind() == std::io::ErrorKind::WouldBlock =>
					{
						thread::sleep(Duration::from_millis(1));
						continue;
					}
					Err(error) => panic!("{error}"),
				};
				stream
					.set_read_timeout(Some(Duration::from_secs(3)))
					.unwrap();
				let mut request = Vec::new();
				loop {
					let mut buffer = [0; 4096];
					let count = stream.read(&mut buffer).unwrap();
					if count == 0 {
						break;
					}
					request.extend_from_slice(&buffer[..count]);
					if request.windows(4).any(|v| v == b"\r\n\r\n") {
						break;
					}
				}
				let request = String::from_utf8(request).unwrap();
				seen.lock().unwrap().push(request.clone());
				let target = request.split_whitespace().nth(1).unwrap();
				let (status, body) = handler(target);
				write!(stream, "HTTP/1.1 {status} Response\r\nContent-Length: {}\r\nConnection: close\r\nContent-Type: application/json\r\n\r\n", body.len()).unwrap();
				stream.write_all(&body).unwrap();
			}
		});
		Self {
			url,
			requests,
			stop,
			thread: Some(thread),
		}
	}

	pub(super) fn api(&self) -> client::Api {
		client::Api::for_test(self.url.clone())
	}
}

impl Drop for Server {
	fn drop(&mut self) {
		self.stop.store(true, Ordering::SeqCst);
		self.thread.take().unwrap().join().unwrap();
	}
}

pub(super) fn response(value: Value) -> (u16, Vec<u8>) {
	(200, serde_json::to_vec(&value).unwrap())
}

pub(super) fn file(mod_id: u32, id: u32) -> CfFile {
	serde_json::from_value(json!({
		"id": id, "modId": mod_id, "displayName": "Test mod", "fileName": format!("mod-{mod_id}-{id}.jar"),
		"gameVersions": ["1.20.1", "Fabric"], "releaseType": 1,
		"downloadCount": 1, "fileDate": "2026-01-01T00:00:00Z", "isAvailable": true,
	})).unwrap()
}

fn files_page(target: &str, files: &[CfFile]) -> (u16, Vec<u8>) {
	let url = url::Url::parse(&format!("http://localhost{target}")).unwrap();
	let params = url
		.query_pairs()
		.collect::<std::collections::HashMap<_, _>>();
	let index = params.get("index").unwrap().parse::<usize>().unwrap();
	let size = params.get("pageSize").unwrap().parse::<usize>().unwrap();
	assert!(size <= 50);
	let selected = files
		.iter()
		.skip(index)
		.take(size)
		.cloned()
		.collect::<Vec<_>>();
	response(
		json!({"pagination": {"index": index, "pageSize": size, "resultCount": selected.len(), "totalCount": files.len()}, "data": selected}),
	)
}

pub(super) fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
	let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
	for (name, contents) in entries {
		zip.start_file(*name, zip::write::SimpleFileOptions::default())
			.unwrap();
		zip.write_all(contents).unwrap();
	}
	zip.finish().unwrap().into_inner()
}

#[tokio::test]
async fn logical_pages_of_100_fetch_both_api_pages_without_skipping_results() {
	let files = (1..=225).map(|id| file(1, id)).collect::<Vec<_>>();
	let server = Server::new(move |target| files_page(target, &files));
	let result = server
		.api()
		.files(1, Some("1.20.1"), Some("fabric"), 1, 100)
		.await
		.unwrap();
	assert_eq!(
		result.data.iter().map(|f| f.id).collect::<Vec<_>>(),
		(101..=200).collect::<Vec<_>>()
	);
	assert_eq!(result.pagination.index, 100);
	assert_eq!(result.pagination.page_size, 100);
	assert_eq!(result.pagination.total_count, 225);
	let requests = server.requests.lock().unwrap();
	assert_eq!(requests.len(), 2);
	assert!(
		requests.iter().all(|r| r.contains("gameVersion=1.20.1")
			&& r.contains("modLoaderType=4"))
	);
}

#[tokio::test]
async fn last_partial_page_keeps_the_total() {
	let files = (1..=225).map(|id| file(1, id)).collect::<Vec<_>>();
	let server = Server::new(move |target| files_page(target, &files));
	let result = server.api().files(1, None, None, 2, 100).await.unwrap();
	assert_eq!(result.data.len(), 25);
	assert_eq!(result.pagination.total_count, 225);
}

#[tokio::test]
async fn search_of_100_returns_all_100_projects() {
	let server = Server::new(|target| {
		let url =
			url::Url::parse(&format!("http://localhost{target}")).unwrap();
		let params = url
			.query_pairs()
			.collect::<std::collections::HashMap<_, _>>();
		assert_eq!(params.get("searchFilter").unwrap(), "100% + fun");
		let start = params.get("index").unwrap().parse::<u32>().unwrap();
		let data = (start..start + 50).map(|id| json!({
			"id": id, "name": format!("Project {id}"), "slug": "test", "summary": "", "downloadCount": 0,
			"categories": [], "authors": [], "latestFilesIndexes": [], "dateModified": "2026-01-01", "isAvailable": true,
		})).collect::<Vec<_>>();
		response(
			json!({"data": data, "pagination": {"index": start, "pageSize": 50, "resultCount": 50, "totalCount": 150}}),
		)
	});
	let result = server
		.api()
		.search(Some("100% + fun".to_string()), None, None, None, 6, 0, 100)
		.await
		.unwrap();
	assert_eq!(result.data.len(), 100);
	assert_eq!(result.data[99].id, 99);
}

#[tokio::test]
async fn invalid_pagination_is_rejected_before_a_request() {
	let server = Server::new(|_| panic!("No request expected"));
	for (page, size) in [(0, 0), (0, 101), (u32::MAX, 50), (200, 50)] {
		assert!(server.api().files(1, None, None, page, size).await.is_err());
	}
}

#[tokio::test]
async fn best_file_finds_a_stable_release_beyond_the_first_50() {
	let mut files = (1..=52)
		.map(|id| {
			let mut f = file(1, id);
			f.release_type = 2;
			f
		})
		.collect::<Vec<_>>();
	files[50].release_type = 1;
	files[51].release_type = 1;
	files[51].is_available = false;
	let server = Server::new(move |target| files_page(target, &files));
	assert_eq!(
		server
			.api()
			.best_file(1, Some("1.20.1"), Some("fabric"), &HashSet::new())
			.await
			.unwrap()
			.id,
		51
	);
}

#[tokio::test]
async fn newest_stable_client_file_wins_over_newer_server_packs_and_betas() {
	let mut old = file(1, 1);
	old.file_date = "2024-01-01T00:00:00Z".to_string();
	let mut server_pack = file(1, 3);
	server_pack.is_server_pack = true;
	server_pack.file_date = "2026-02-01T00:00:00Z".to_string();
	let mut beta = file(1, 4);
	beta.release_type = 2;
	beta.file_date = "2026-03-01T00:00:00Z".to_string();
	let server = Server::new(move |target| {
		files_page(
			target,
			&[old.clone(), server_pack.clone(), beta.clone(), file(1, 2)],
		)
	});
	assert_eq!(
		server
			.api()
			.best_file(1, None, None, &HashSet::new())
			.await
			.unwrap()
			.id,
		2
	);
}

#[tokio::test]
async fn selected_search_filters_and_sort_use_official_api_parameters() {
	let server = Server::new(|target| {
		let url =
			url::Url::parse(&format!("http://localhost{target}")).unwrap();
		let params = url
			.query_pairs()
			.collect::<std::collections::HashMap<_, _>>();
		assert_eq!(
			serde_json::from_str::<Value>(params.get("gameVersions").unwrap())
				.unwrap(),
			json!(["1.20.1", "1.21.1"])
		);
		assert_eq!(
			serde_json::from_str::<Value>(
				params.get("modLoaderTypes").unwrap()
			)
			.unwrap(),
			json!([4, 1])
		);
		assert_eq!(params.get("sortField").unwrap(), "6");
		response(
			json!({"data": [], "pagination": {"index": 0, "pageSize": 20, "resultCount": 0, "totalCount": 0}}),
		)
	});
	server
		.api()
		.search_filtered(
			None,
			None,
			None,
			None,
			6,
			0,
			20,
			Some(vec!["1.20.1".to_string(), "1.21.1".to_string()]),
			Some(vec!["fabric".to_string(), "forge".to_string()]),
			Some("downloads".to_string()),
		)
		.await
		.unwrap();
}

#[tokio::test]
async fn an_installed_file_is_not_downloaded_or_staged_again() {
	let server = Server::new(|_| panic!("The existing file should be reused"));
	let mut existing = file(1, 1);
	existing.hashes = vec![CfFileHash {
		algo: 1,
		value: "existing-hash".to_string(),
	}];
	let staged = stage_files(
		&server.api(),
		vec![existing],
		&HashSet::from(["existing-hash".to_string()]),
		None,
	)
	.await
	.unwrap();
	assert!(staged.files.is_empty());
}

#[tokio::test]
async fn existing_compatible_dependency_is_reused_instead_of_upgraded() {
	let mut old = file(2, 1);
	old.hashes = vec![CfFileHash {
		algo: 1,
		value: "existing-hash".to_string(),
	}];
	let newer = file(2, 2);
	let server = Server::new(move |target| {
		files_page(target, &[newer.clone(), old.clone()])
	});
	let result = server
		.api()
		.best_file(
			2,
			Some("1.20.1"),
			Some("fabric"),
			&HashSet::from(["existing-hash".to_string()]),
		)
		.await
		.unwrap();
	assert_eq!(result.id, 1);
}

#[tokio::test]
async fn dependencies_follow_required_edges_and_terminate_cycles() {
	let mut root = file(1, 1);
	root.dependencies = Some(vec![
		CfFileDependency {
			mod_id: 2,
			relation_type: 3,
		},
		CfFileDependency {
			mod_id: 9,
			relation_type: 2,
		},
		CfFileDependency {
			mod_id: 10,
			relation_type: 1,
		},
	]);
	let server = Server::new(|target| {
		assert!(target.starts_with("/v1/mods/2/files?"));
		let mut dep = file(2, 2);
		dep.dependencies = Some(vec![CfFileDependency {
			mod_id: 1,
			relation_type: 3,
		}]);
		files_page(target, &[dep])
	});
	let plan = server
		.api()
		.dependencies(root, "1.20.1", "fabric", &HashSet::new())
		.await
		.unwrap();
	assert_eq!(
		plan.iter().map(|f| f.mod_id).collect::<Vec<_>>(),
		vec![2, 1]
	);
}

#[tokio::test]
async fn dependency_request_failure_does_not_become_an_empty_plan() {
	let mut root = file(1, 1);
	root.dependencies = Some(vec![CfFileDependency {
		mod_id: 2,
		relation_type: 3,
	}]);
	let server = Server::new(|_| (503, b"temporarily unavailable".to_vec()));
	let error = server
		.api()
		.dependencies(root, "1.20.1", "fabric", &HashSet::new())
		.await
		.unwrap_err()
		.to_string();
	assert!(error.contains("503"));
}

#[tokio::test]
async fn missing_download_is_reported_with_the_exact_required_file() {
	let server = Server::new(|_| response(json!({"data": null})));
	let error =
		stage_files(&server.api(), vec![file(12, 34)], &HashSet::new(), None)
			.await
			.err()
			.unwrap()
			.to_string();
	assert!(error.contains("mod-12-34.jar"));
	assert!(error.contains("project 12, file 34"));
	assert!(error.contains("manually"));
}

#[tokio::test]
async fn api_errors_are_not_misreported_as_distribution_restrictions() {
	let server = Server::new(|_| (429, b"rate limited".to_vec()));
	let error = server
		.api()
		.prepare(file(12, 34))
		.await
		.err()
		.unwrap()
		.to_string();
	assert!(error.contains("429"));
	assert!(!error.contains("author"));
}

#[tokio::test]
async fn cdn_downloads_never_receive_the_api_key_and_reject_http_errors() {
	let download = Server::new(|_| (404, b"not a mod".to_vec()));
	let download_url = format!("{}/download.jar", download.url);
	let api_server =
		Server::new(move |_| response(json!({"data": download_url})));
	let api = api_server.api();
	let prepared = api.prepare(file(1, 1)).await.unwrap();
	assert!(api.download(&prepared).await.is_err());
	assert!(
		api_server.requests.lock().unwrap()[0]
			.contains("x-api-key: test-api-key")
	);
	assert!(
		!download.requests.lock().unwrap()[0]
			.to_lowercase()
			.contains("x-api-key")
	);
}

#[tokio::test]
async fn a_failed_second_file_aborts_staging_instead_of_skipping_it() {
	let contents = archive(&[("fabric.mod.json", b"{}")]);
	let server = Server::new(move |_| (200, contents.clone()));
	let first_url = format!("{}/first.jar", server.url);
	let api_server = Server::new(move |target| {
		if target.contains("/files/1/") {
			response(json!({"data": first_url}))
		} else {
			response(json!({"data": null}))
		}
	});
	let error = stage_files(
		&api_server.api(),
		vec![file(1, 1), file(2, 2), file(3, 3)],
		&HashSet::new(),
		None,
	)
	.await
	.err()
	.unwrap()
	.to_string();
	assert!(error.contains("project 2, file 2"));
	assert_eq!(api_server.requests.lock().unwrap().len(), 2);
	assert_eq!(server.requests.lock().unwrap().len(), 1);
}

#[test]
fn corrupt_truncated_and_non_archive_downloads_are_rejected() {
	let bytes = archive(&[("fabric.mod.json", b"{}")]);
	let mut metadata = file(1, 1);
	metadata.file_length = Some(bytes.len() as u64);
	metadata.hashes = vec![CfFileHash {
		algo: 1,
		value: sha1_smol::Sha1::from(&bytes).hexdigest(),
	}];
	assert!(client::validate_download(&metadata, &bytes).is_ok());
	assert!(
		client::validate_download(&metadata, &bytes[..bytes.len() - 1])
			.is_err()
	);
	let mut corrupt = bytes.clone();
	corrupt[0] ^= 1;
	assert!(client::validate_download(&metadata, &corrupt).is_err());
	metadata.file_length = None;
	metadata.hashes.clear();
	assert!(
		client::validate_download(&metadata, b"<html>error</html>").is_err()
	);
}

#[test]
fn archive_overrides_preserve_nested_paths_and_copy_contents() {
	let directory = tempfile::tempdir().unwrap();
	let bytes = archive(&[
		("overrides/config/example.toml", b"enabled = true"),
		("unrelated.txt", b"skip"),
	]);
	copy_overrides(&bytes, "overrides", None).unwrap();
	copy_overrides(&bytes, "overrides", Some(directory.path())).unwrap();
	assert_eq!(
		std::fs::read(directory.path().join("config/example.toml")).unwrap(),
		b"enabled = true"
	);
	assert!(!directory.path().join("unrelated.txt").exists());
}

#[test]
fn unsafe_override_paths_are_rejected_before_profile_creation() {
	for name in [
		"overrides/../escape.jar",
		"overrides/C:/escape.jar",
		"overrides/mods\\escape.jar",
		"overrides//escape.jar",
	] {
		let bytes = archive(&[(name, b"bad")]);
		assert!(copy_overrides(&bytes, "overrides", None).is_err(), "{name}");
	}
	assert!(
		override_relative_path("../overrides", "../overrides/config.txt")
			.is_err()
	);
}

#[test]
fn unsupported_loaders_and_unsafe_file_names_fail_explicitly() {
	assert_eq!(
		parse_loader_id("neoforge-21.1.1").unwrap(),
		(
			crate::state::ModLoader::NeoForge,
			Some("21.1.1".to_string())
		)
	);
	assert!(parse_loader_id("unknown-1.0").is_err());
	assert!(parse_loader_id("forge-").is_err());
	for name in ["", "..", "../file.jar", "mods\\file.jar", "C:file.jar"] {
		assert!(client::validate_file_name(name).is_err());
	}
}
