//! The service's identity: its id, and the certificate phones pin with the key that goes with it.
//!
//! In the data directory: id, cert.der and key.der. `elamond rotate-cert` replaces the certificate
//! and key and keeps the id; every phone then sees the certificate changed and is paired again.
//! rotations.json and next-identity.json, which elamond 0.6.1 kept for signed rotations, aren't read.

use std::fs;
use std::io;
use std::path::Path;
use std::time::SystemTime;

use crate::consts::protocol;
use crate::store::{random_bytes, sha256_hex, write_private};

pub const ID_FILE: &str = "id";
pub const CERT_FILE: &str = "cert.der";
pub const KEY_FILE: &str = "key.der";

pub struct Identity {
	pub id: String,
	pub cert_der: Vec<u8>,
	pub key_der: Vec<u8>,
	/// SHA-256 of the certificate, the thing the app pins.
	pub fingerprint: String,
}

/// A file's contents, or `None` when it doesn't exist.
fn read_if_there(path: &Path) -> Result<Option<Vec<u8>>, String> {
	match fs::read(path) {
		Ok(b) => Ok(Some(b)),
		Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
		Err(e) => Err(format!("can't read {}: {e}", path.display())),
	}
}

fn saved(path: &Path, data: &[u8]) -> Result<(), String> {
	write_private(path, data).map_err(|e| format!("can't write {}: {e}", path.display()))
}

/// A new key and a certificate for it. The name inside doesn't matter: the app trusts this exact
/// certificate, not a name.
fn new_certificate() -> Result<(Vec<u8>, Vec<u8>), String> {
	let ck = rcgen::generate_simple_self_signed(vec!["elamon".to_string()]).map_err(|e| format!("can't make a certificate: {e}"))?;
	Ok((ck.cert.der().to_vec(), ck.signing_key.serialize_der()))
}

impl Identity {
	/// Loads the service's identity, creating it on first run. Paired phones trust exactly this
	/// identity, so it is only made anew when its files don't exist: one that can't be read
	/// stops the service with the reason instead of silently unpairing every phone.
	pub fn load_or_create(dir: &Path) -> Result<Identity, String> {
		let id_path = dir.join(ID_FILE);
		let id = match read_if_there(&id_path)? {
			Some(b) => {
				let id = String::from_utf8_lossy(&b).trim().to_string();
				if id.is_empty() {
					return Err(format!("{} is empty; remove it to make a new id, then pair the phones again", id_path.display()));
				}
				id
			}
			None => {
				let id = data_encoding::HEXLOWER.encode(&random_bytes::<{ protocol::SERVICE_ID_BYTES }>());
				saved(&id_path, id.as_bytes())?;
				id
			}
		};
		if read_if_there(&dir.join(CERT_FILE))?.is_none() && read_if_there(&dir.join(KEY_FILE))?.is_none() {
			let (cert, key) = new_certificate()?;
			saved(&dir.join(KEY_FILE), &key)?;
			saved(&dir.join(CERT_FILE), &cert)?;
		}
		let (cert_der, key_der) = Self::pair(dir)?;
		let fingerprint = sha256_hex(&cert_der);
		Ok(Identity { id, cert_der, key_der, fingerprint })
	}

	/// The certificate and key as they are on disk, never made anew: what a running service takes
	/// up after `elamond rotate-cert`, and what that command starts from.
	pub fn load(dir: &Path) -> Result<Identity, String> {
		let id = read_if_there(&dir.join(ID_FILE))?.map(|b| String::from_utf8_lossy(&b).trim().to_string()).unwrap_or_default();
		let (cert_der, key_der) = Self::pair(dir)?;
		let fingerprint = sha256_hex(&cert_der);
		Ok(Identity { id, cert_der, key_der, fingerprint })
	}

	fn pair(dir: &Path) -> Result<(Vec<u8>, Vec<u8>), String> {
		match (read_if_there(&dir.join(CERT_FILE))?, read_if_there(&dir.join(KEY_FILE))?) {
			(Some(c), Some(k)) if !c.is_empty() && !k.is_empty() => Ok((c, k)),
			(None, None) => Err(format!("there is no certificate in {} yet; the service makes one when it first starts", dir.display())),
			_ => Err(format!(
				"the TLS identity in {} is incomplete: cert.der and key.der go together; remove both to make a new one, then pair the phones again",
				dir.display()
			)),
		}
	}
}

/// What cert.der looks like on disk: modified, inode, length.
pub type Stamp = (SystemTime, u64, u64);

/// cert.der's stamp, to notice a new certificate. `renew` writes cert.der last, so a new one means
/// the key beside it is new too.
pub fn stamp(dir: &Path) -> Option<Stamp> {
	use std::os::unix::fs::MetadataExt;
	let m = fs::metadata(dir.join(CERT_FILE)).ok()?;
	Some((m.modified().ok()?, m.ino(), m.len()))
}

/// Replaces the certificate and key in `dir` with new ones and keeps the id: `elamond rotate-cert`.
/// The old key is deleted; phones see the certificate changed and pair again. The key goes first, so
/// a running service that sees a new cert.der finds its key beside it. Returns the old fingerprint
/// and the new one.
pub fn renew(dir: &Path) -> Result<(String, String), String> {
	let old = Identity::load(dir)?;
	let (cert, key) = new_certificate()?;
	saved(&dir.join(KEY_FILE), &key)?;
	saved(&dir.join(CERT_FILE), &cert)?;
	Ok((old.fingerprint, sha256_hex(&cert)))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_new_certificate_keeps_the_id_and_the_old_key_is_gone() {
		let dir = std::env::temp_dir().join(format!("elamond-identity-{}", data_encoding::HEXLOWER.encode(&random_bytes::<6>())));
		fs::create_dir_all(&dir).unwrap();
		assert!(renew(&dir).unwrap_err().contains("no certificate"));
		let first = Identity::load_or_create(&dir).unwrap();
		// Left by elamond 0.6.1: not read, whatever is in them.
		fs::write(dir.join("next-identity.json"), "{").unwrap();
		fs::write(dir.join("rotations.json"), "[{").unwrap();
		let (old_fp, new_fp) = renew(&dir).unwrap();
		let now = Identity::load_or_create(&dir).unwrap();
		assert_eq!((old_fp.as_str(), new_fp.as_str()), (first.fingerprint.as_str(), now.fingerprint.as_str()));
		assert_ne!(old_fp, new_fp);
		assert_eq!(now.id, first.id, "the id stays");
		// Nothing of the old key is left in the directory.
		for entry in fs::read_dir(&dir).unwrap() {
			let bytes = fs::read(entry.unwrap().path()).unwrap();
			assert!(!bytes.windows(first.key_der.len()).any(|w| w == first.key_der.as_slice()));
		}
		// The new pair loads, as TLS would take it.
		crate::tls_with(&now.cert_der, &now.key_der).unwrap();
		fs::remove_dir_all(dir).unwrap();
	}
}
