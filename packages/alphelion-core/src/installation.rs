//! Required-file plans and readiness rules, independent of providers and storage.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileState {
	Pending,
	ManualDownload,
	/// Supplied by an adapter after checking the current file's integrity.
	Verified,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallPlan<Key> {
	files: Vec<Key>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallProgress<Key> {
	files: Vec<(Key, FileState)>,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum InstallError {
	#[error("Saved progress contains a duplicate file")]
	DuplicateSavedFile,
	#[error("Saved files do not match the required installation plan")]
	PlanMismatch,
	#[error("Required files have not all been verified")]
	FilesNotVerified,
}

impl<Key: Clone + Eq + Hash> InstallPlan<Key> {
	/// Repeated manifest references identify one file, in first-seen order.
	pub fn new(required: impl IntoIterator<Item = Key>) -> Self {
		let mut seen = HashSet::new();
		Self {
			files: required
				.into_iter()
				.filter(|key| seen.insert(key.clone()))
				.collect(),
		}
	}

	pub fn files(&self) -> &[Key] {
		&self.files
	}

	/// Adapters must recheck cached bytes before reporting `Verified`. A saved
	/// completion flag or the existence of a path is not verification evidence.
	pub fn reconcile(
		&self,
		current: impl IntoIterator<Item = (Key, FileState)>,
	) -> Result<InstallProgress<Key>, InstallError> {
		let mut states = HashMap::new();
		for (key, state) in current {
			if states.insert(key, state).is_some() {
				return Err(InstallError::DuplicateSavedFile);
			}
		}
		if states.len() != self.files.len() {
			return Err(InstallError::PlanMismatch);
		}
		let files = self
			.files
			.iter()
			.map(|key| {
				states
					.get(key)
					.map(|state| (key.clone(), *state))
					.ok_or(InstallError::PlanMismatch)
			})
			.collect::<Result<_, _>>()?;
		Ok(InstallProgress { files })
	}
}

impl<Key> InstallProgress<Key> {
	pub fn total_files(&self) -> usize {
		self.files.len()
	}

	pub fn verified_files(&self) -> usize {
		self.files
			.iter()
			.filter(|(_, state)| *state == FileState::Verified)
			.count()
	}

	pub fn manual_files(&self) -> impl Iterator<Item = &Key> {
		self.files.iter().filter_map(|(key, state)| {
			(*state == FileState::ManualDownload).then_some(key)
		})
	}

	/// Readiness permits applying the staged files; it does not mean the
	/// instance has been written or Minecraft has been installed successfully.
	pub fn is_ready(&self) -> bool {
		self.files
			.iter()
			.all(|(_, state)| *state == FileState::Verified)
	}

	pub fn ensure_ready(&self) -> Result<(), InstallError> {
		if self.is_ready() {
			Ok(())
		} else {
			Err(InstallError::FilesNotVerified)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn repeated_manifest_files_are_planned_once_in_the_original_order() {
		let plan = InstallPlan::new(["second", "first", "second"]);
		assert_eq!(plan.files(), &["second", "first"]);
	}

	#[test]
	fn saved_progress_can_arrive_in_a_different_order() {
		let progress = InstallPlan::new(["first", "second"])
			.reconcile([
				("second", FileState::ManualDownload),
				("first", FileState::Verified),
			])
			.unwrap();
		assert_eq!(progress.verified_files(), 1);
		assert_eq!(progress.total_files(), 2);
		assert_eq!(
			progress.manual_files().copied().collect::<Vec<_>>(),
			["second"]
		);
		assert_eq!(
			progress.ensure_ready(),
			Err(InstallError::FilesNotVerified)
		);
	}

	#[test]
	fn missing_and_unexpected_saved_files_cannot_satisfy_a_plan() {
		let plan = InstallPlan::new([1, 2]);
		for keys in [vec![1], vec![1, 3], vec![1, 2, 3]] {
			let result = plan.reconcile(
				keys.into_iter().map(|key| (key, FileState::Verified)),
			);
			assert_eq!(result, Err(InstallError::PlanMismatch));
		}
	}

	#[test]
	fn duplicate_saved_files_do_not_count_as_two_required_files() {
		let result = InstallPlan::new([1, 2])
			.reconcile([(1, FileState::Verified), (1, FileState::Verified)]);
		assert_eq!(result, Err(InstallError::DuplicateSavedFile));
	}

	#[test]
	fn pending_files_block_readiness_without_becoming_manual_downloads() {
		let progress = InstallPlan::new([1, 2])
			.reconcile([(1, FileState::Verified), (2, FileState::Pending)])
			.unwrap();
		assert_eq!(progress.verified_files(), 1);
		assert_eq!(progress.manual_files().count(), 0);
		assert!(!progress.is_ready());
	}

	#[test]
	fn every_required_file_must_be_verified_before_applying_the_install() {
		let progress = InstallPlan::new([1, 2])
			.reconcile([(1, FileState::Verified), (2, FileState::Verified)])
			.unwrap();
		assert_eq!(progress.ensure_ready(), Ok(()));
	}

	#[test]
	fn changed_cache_evidence_revokes_readiness_after_a_restart() {
		let plan = InstallPlan::new([("provider", "file")]);
		assert!(
			plan.reconcile([(("provider", "file"), FileState::Verified)])
				.unwrap()
				.is_ready()
		);
		let restored = plan
			.reconcile([(("provider", "file"), FileState::Pending)])
			.unwrap();
		assert_eq!(restored.verified_files(), 0);
		assert_eq!(
			restored.ensure_ready(),
			Err(InstallError::FilesNotVerified)
		);
	}

	#[test]
	fn an_overrides_only_pack_needs_no_external_files() {
		let progress = InstallPlan::<String>::new([]).reconcile([]).unwrap();
		assert_eq!(progress.total_files(), 0);
		assert_eq!(progress.ensure_ready(), Ok(()));
	}
}
