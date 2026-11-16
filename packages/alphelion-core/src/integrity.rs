//! Checks downloaded content against expectations supplied by a provider.

use sha2::Digest;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HashAlgorithm {
	Sha1,
	Sha512,
}

impl HashAlgorithm {
	fn hex_length(self) -> usize {
		match self {
			Self::Sha1 => 40,
			Self::Sha512 => 128,
		}
	}

	pub fn digest(self, bytes: &[u8]) -> String {
		match self {
			Self::Sha1 => sha1_smol::Sha1::from(bytes).hexdigest(),
			Self::Sha512 => format!("{:x}", sha2::Sha512::digest(bytes)),
		}
	}
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Checksum {
	algorithm: HashAlgorithm,
	value: String,
}

impl Checksum {
	pub fn new(
		algorithm: HashAlgorithm,
		value: impl Into<String>,
	) -> Result<Self, IntegrityError> {
		let value = value.into();
		if value.len() != algorithm.hex_length()
			|| !value.bytes().all(|byte| byte.is_ascii_hexdigit())
		{
			return Err(IntegrityError::InvalidChecksum { algorithm });
		}
		Ok(Self {
			algorithm,
			value: value.to_ascii_lowercase(),
		})
	}
}

#[derive(Clone, Debug)]
pub struct IntegritySpec {
	pub expected_size: Option<u64>,
	pub checksums: Vec<Checksum>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Verification {
	pub byte_length: u64,
	pub size_checked: bool,
	pub hashes_checked: usize,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum IntegrityError {
	#[error("File size does not match: expected {expected}, received {actual}")]
	SizeMismatch { expected: u64, actual: u64 },
	#[error("Invalid {algorithm:?} checksum in file metadata")]
	InvalidChecksum { algorithm: HashAlgorithm },
	#[error("{algorithm:?} checksum does not match the expected file")]
	ChecksumMismatch {
		algorithm: HashAlgorithm,
		expected: String,
		actual: String,
	},
}

impl IntegritySpec {
	/// Checks every supplied expectation. The result records what was checked;
	/// absent hashes must not be presented as checksum verification.
	pub fn verify(&self, bytes: &[u8]) -> Result<Verification, IntegrityError> {
		let byte_length = bytes.len() as u64;
		if let Some(expected) = self.expected_size
			&& expected != byte_length
		{
			return Err(IntegrityError::SizeMismatch {
				expected,
				actual: byte_length,
			});
		}
		for checksum in &self.checksums {
			let actual = checksum.algorithm.digest(bytes);
			if actual != checksum.value {
				return Err(IntegrityError::ChecksumMismatch {
					algorithm: checksum.algorithm,
					expected: checksum.value.clone(),
					actual,
				});
			}
		}
		Ok(Verification {
			byte_length,
			size_checked: self.expected_size.is_some(),
			hashes_checked: self.checksums.len(),
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	const SHA1: &str = "a9993e364706816aba3e25717850c26c9cd0d89d";
	const SHA512: &str = concat!(
		"ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2",
		"192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
	);

	fn spec(checksums: Vec<Checksum>) -> IntegritySpec {
		IntegritySpec {
			expected_size: Some(3),
			checksums,
		}
	}

	#[test]
	fn known_sha1_and_sha512_checksums_match_the_file() {
		let result = spec(vec![
			Checksum::new(HashAlgorithm::Sha1, SHA1).unwrap(),
			Checksum::new(HashAlgorithm::Sha512, SHA512).unwrap(),
		])
		.verify(b"abc")
		.unwrap();
		assert_eq!(result.hashes_checked, 2);
	}

	#[test]
	fn corruption_does_not_pass_just_because_the_size_matches() {
		let mut corrupted = *b"abc";
		corrupted[2] ^= 1;
		let result =
			spec(vec![Checksum::new(HashAlgorithm::Sha1, SHA1).unwrap()])
				.verify(&corrupted);
		assert!(matches!(
			result,
			Err(IntegrityError::ChecksumMismatch { .. })
		));
	}

	#[test]
	fn partial_download_reports_the_size_error_first() {
		let result =
			spec(vec![Checksum::new(HashAlgorithm::Sha1, SHA1).unwrap()])
				.verify(b"ab");
		assert_eq!(
			result,
			Err(IntegrityError::SizeMismatch {
				expected: 3,
				actual: 2
			})
		);
	}

	#[test]
	fn uppercase_provider_checksums_are_accepted() {
		assert!(
			spec(vec![
				Checksum::new(HashAlgorithm::Sha1, SHA1.to_ascii_uppercase())
					.unwrap()
			])
			.verify(b"abc")
			.is_ok()
		);
	}

	#[test]
	fn malformed_provider_checksums_are_rejected() {
		for value in [String::new(), "f".repeat(39), "z".repeat(40)] {
			assert!(matches!(
				Checksum::new(HashAlgorithm::Sha1, value),
				Err(IntegrityError::InvalidChecksum { .. })
			));
		}
		assert!(Checksum::new(HashAlgorithm::Sha512, SHA1).is_err());
	}

	#[test]
	fn every_supplied_checksum_must_match() {
		let result = spec(vec![
			Checksum::new(HashAlgorithm::Sha1, SHA1).unwrap(),
			Checksum::new(HashAlgorithm::Sha512, "f".repeat(128)).unwrap(),
		])
		.verify(b"abc");
		assert!(matches!(
			result,
			Err(IntegrityError::ChecksumMismatch {
				algorithm: HashAlgorithm::Sha512,
				..
			})
		));
	}

	#[test]
	fn size_only_validation_is_not_reported_as_checksum_verification() {
		let result = spec(Vec::new()).verify(b"abc").unwrap();
		assert!(result.size_checked);
		assert_eq!(result.hashes_checked, 0);
	}

	#[test]
	fn missing_expectations_are_reported_explicitly() {
		let result = IntegritySpec {
			expected_size: None,
			checksums: Vec::new(),
		}
		.verify(b"abc")
		.unwrap();
		assert!(!result.size_checked);
		assert_eq!(result.hashes_checked, 0);
	}
}
