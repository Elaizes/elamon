//! The service's identity: its id, and the certificate phones pin with the key that goes with it.
//!
//! `elamond rotate-cert` replaces the certificate and key. Before the old key is thrown away it
//! signs a statement that the new certificate follows the old one, so a paired phone that meets
//! the new certificate can check, from the one it pinned, that it is the same service's, and pin
//! it without pairing again. The last `limit::ROTATIONS_KEPT` statements are kept, for phones that
//! missed a rotation or two.
//!
//! In the data directory: id, cert.der, key.der, rotations.json (oldest first), and while a
//! rotation is being written, next-identity.json.

use std::fs;
use std::io;
use std::path::Path;
use std::time::SystemTime;

use rcgen::SigningKey;
use serde::{Deserialize, Serialize};

use crate::consts::{limit, protocol};
use crate::store::{now, random_bytes, read_json, sha256_hex, write_json, write_private};

pub const ID_FILE: &str = "id";
pub const CERT_FILE: &str = "cert.der";
pub const KEY_FILE: &str = "key.der";
pub const ROTATIONS_FILE: &str = "rotations.json";
/// The new certificate and key while a rotation is written: from here they are taken up once the
/// statement for them is saved, or dropped if it never was.
pub const NEXT_FILE: &str = "next-identity.json";

const B64: data_encoding::Encoding = data_encoding::BASE64;

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
		finish_rotation(dir)?;
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

/// cert.der's stamp, to notice a new certificate. cert.der is written last in a rotation, so a
/// new one means the key beside it is new too.
pub fn stamp(dir: &Path) -> Option<Stamp> {
	use std::os::unix::fs::MetadataExt;
	let m = fs::metadata(dir.join(CERT_FILE)).ok()?;
	Some((m.modified().ok()?, m.ino(), m.len()))
}

/// That the certificate `new_fp` follows `old_cert`, signed with `old_cert`'s key.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Rotation {
	/// The version of the signed text: `protocol::ROTATION_FORMAT`.
	pub format: String,
	/// SHA-256 of `old_cert`, lowercase hex.
	pub old_fp: String,
	/// SHA-256 of the certificate that replaced it.
	pub new_fp: String,
	/// Unix time of the rotation.
	pub t: u64,
	/// The old certificate, DER in base64: its public key checks the signature.
	pub old_cert: String,
	/// ECDSA P-256 with SHA-256 over `signed_text`, DER in base64.
	pub signature: String,
}

/// The exact bytes a rotation's signature covers.
pub fn signed_text(format: &str, old_fp: &str, new_fp: &str, t: u64) -> String {
	format!("{format}\n{old_fp}\n{new_fp}\n{t}")
}

/// The rotations kept, oldest first; none when there have been none.
pub fn rotations(dir: &Path) -> io::Result<Vec<Rotation>> {
	Ok(read_json(&dir.join(ROTATIONS_FILE))?.unwrap_or_default())
}

/// The rotations that lead to the certificate `fp`, oldest first: what the service serves while it
/// presents that one. A chain that doesn't end at it (the files were changed by hand) leads to it
/// only as far as it does.
pub fn chain_to<'a>(rotations: &'a [Rotation], fp: &str) -> &'a [Rotation] {
	match rotations.iter().rposition(|r| r.new_fp == fp) {
		Some(i) => &rotations[..=i],
		None => &[],
	}
}

#[derive(Serialize, Deserialize)]
struct Next {
	cert: String,
	key: String,
}

/// Whether `bytes` read as next-identity.json, for `elamond restore` to check before it writes one.
pub fn next_reads(bytes: &[u8]) -> bool {
	serde_json::from_slice::<Next>(bytes).is_ok_and(|n| B64.decode(n.cert.as_bytes()).is_ok() && B64.decode(n.key.as_bytes()).is_ok())
}

/// Completes a rotation that stopped midway: takes up its certificate if its statement was saved,
/// or drops it if it wasn't. Writes key.der before cert.der, so a running service that sees a new
/// cert.der finds the key that goes with it.
fn finish_rotation(dir: &Path) -> Result<(), String> {
	let path = dir.join(NEXT_FILE);
	let next = match read_json::<Next>(&path) {
		Ok(Some(n)) => n,
		Ok(None) => return Ok(()),
		Err(e) => return Err(format!("{e}\n  remove it: the rotation it was part of didn't finish")),
	};
	let decode = |s: &str| B64.decode(s.as_bytes()).map_err(|e| format!("{}: {e}", path.display()));
	let (cert, key) = (decode(&next.cert)?, decode(&next.key)?);
	let recorded = rotations(dir).ok().and_then(|r| r.last().map(|l| l.new_fp == sha256_hex(&cert))).unwrap_or(false);
	if recorded {
		saved(&dir.join(KEY_FILE), &key)?;
		saved(&dir.join(CERT_FILE), &cert)?;
	}
	match fs::remove_file(&path) {
		Err(e) if e.kind() != io::ErrorKind::NotFound => Err(format!("can't remove {}: {e}", path.display())),
		_ => Ok(()),
	}
}

/// Replaces the certificate and key in `dir` with new ones, the old key signing the new
/// certificate's fingerprint first. Returns the statement.
pub fn rotate(dir: &Path) -> Result<Rotation, String> {
	finish_rotation(dir)?;
	let old = Identity::load(dir)?;
	let mut kept = rotations(dir).map_err(|e| {
		format!("{e}\n  phones follow a new certificate through this file; move it away to start it anew (phones that missed a rotation before pair again)")
	})?;
	let key = rcgen::KeyPair::try_from(old.key_der.as_slice()).map_err(|e| format!("can't read key.der: {e}"))?;
	if key.algorithm() != &rcgen::PKCS_ECDSA_P256_SHA256 {
		return Err("key.der isn't an ECDSA P-256 key, which rotations are signed with; remove cert.der and key.der to make one, then pair the phones again".into());
	}

	let (cert, new_key) = new_certificate()?;
	let new_fp = sha256_hex(&cert);
	let t = now();
	let text = signed_text(protocol::ROTATION_FORMAT, &old.fingerprint, &new_fp, t);
	let signature = key.sign(text.as_bytes()).map_err(|e| format!("can't sign: {e}"))?;
	let rotation = Rotation {
		format: protocol::ROTATION_FORMAT.into(),
		old_fp: old.fingerprint,
		new_fp,
		t,
		old_cert: B64.encode(&old.cert_der),
		signature: B64.encode(&signature),
	};

	// The new pair first, then the statement, then the new pair in place: stopped anywhere, the
	// next start (or rotation) finishes it or undoes it, and phones never meet a certificate the
	// statements don't lead to.
	let next = Next { cert: B64.encode(&cert), key: B64.encode(&new_key) };
	saved(&dir.join(NEXT_FILE), &serde_json::to_vec(&next).map_err(|e| e.to_string())?)?;
	kept.push(rotation.clone());
	let drop = kept.len().saturating_sub(limit::ROTATIONS_KEPT);
	kept.drain(..drop);
	write_json::<Vec<Rotation>>(&dir.join(ROTATIONS_FILE), &kept).map_err(|e| format!("can't save the rotation: {e}"))?;
	finish_rotation(dir)?;
	Ok(rotation)
}

/// What the app does with the rotations a service serves, for the tests: the fingerprint to pin
/// in place of `pinned` when the statements lead from it to `presented`, each signed with the key
/// of the certificate before it.
#[cfg(test)]
pub fn follow(chain: &[Rotation], pinned: &str, presented: &str) -> Option<String> {
	let start = chain.iter().position(|r| r.old_fp == pinned)?;
	let mut at = pinned.to_string();
	for r in &chain[start..] {
		let cert = B64.decode(r.old_cert.as_bytes()).ok()?;
		if r.format != protocol::ROTATION_FORMAT || r.old_fp != at || sha256_hex(&cert) != at {
			return None;
		}
		let der = rustls::pki_types::CertificateDer::from(cert.as_slice());
		let ee = webpki::EndEntityCert::try_from(&der).ok()?;
		let sig = B64.decode(r.signature.as_bytes()).ok()?;
		ee.verify_signature(webpki::ring::ECDSA_P256_SHA256, signed_text(&r.format, &r.old_fp, &r.new_fp, r.t).as_bytes(), &sig).ok()?;
		at = r.new_fp.clone();
		if at == presented {
			return Some(at);
		}
	}
	None
}

#[cfg(test)]
mod tests {
	use super::*;

	pub(crate) fn scratch() -> std::path::PathBuf {
		let dir = std::env::temp_dir().join(format!("elamond-identity-{}", data_encoding::HEXLOWER.encode(&random_bytes::<6>())));
		fs::create_dir_all(&dir).unwrap();
		dir
	}

	#[test]
	fn a_rotation_is_signed_by_the_old_key_which_is_then_gone() {
		let dir = scratch();
		let first = Identity::load_or_create(&dir).unwrap();
		let r = rotate(&dir).unwrap();
		let now = Identity::load_or_create(&dir).unwrap();
		assert_eq!((r.old_fp.as_str(), r.new_fp.as_str()), (first.fingerprint.as_str(), now.fingerprint.as_str()));
		assert_eq!(now.id, first.id, "the id stays");
		assert_ne!(now.key_der, first.key_der);
		assert_eq!(B64.decode(r.old_cert.as_bytes()).unwrap(), first.cert_der);
		// Nothing of the old key is left in the directory.
		for entry in fs::read_dir(&dir).unwrap() {
			let bytes = fs::read(entry.unwrap().path()).unwrap();
			assert!(!bytes.windows(first.key_der.len()).any(|w| w == first.key_der.as_slice()));
		}
		assert!(!dir.join(NEXT_FILE).exists());
		assert_eq!(follow(&rotations(&dir).unwrap(), &first.fingerprint, &now.fingerprint), Some(now.fingerprint.clone()));
		// The new pair loads, as TLS would take it.
		rustls::ServerConfig::builder_with_provider(std::sync::Arc::new(rustls::crypto::ring::default_provider()))
			.with_safe_default_protocol_versions()
			.unwrap()
			.with_no_client_auth()
			.with_single_cert(
				vec![now.cert_der.clone().into()],
				rustls::pki_types::PrivateKeyDer::Pkcs8(now.key_der.clone().into()),
			)
			.unwrap();
		fs::remove_dir_all(dir).unwrap();
	}

	#[test]
	fn the_last_rotations_are_kept_and_lead_on_from_any_of_them() {
		let dir = scratch();
		let mut fps = vec![Identity::load_or_create(&dir).unwrap().fingerprint];
		for _ in 0..limit::ROTATIONS_KEPT + 2 {
			fps.push(rotate(&dir).unwrap().new_fp);
		}
		let chain = rotations(&dir).unwrap();
		assert_eq!(chain.len(), limit::ROTATIONS_KEPT);
		let current = fps.last().unwrap();
		// A phone that missed up to all the kept ones follows; one that missed more can't.
		for fp in &fps[2..fps.len() - 1] {
			assert_eq!(follow(&chain, fp, current).as_ref(), Some(current), "from {fp}");
		}
		assert_eq!(follow(&chain, &fps[1], current), None);
		// While the service still presents an older one, the chain is served up to it.
		assert_eq!(chain_to(&chain, &fps[4]).len(), 2);
		assert_eq!(follow(chain_to(&chain, &fps[4]), &fps[3], &fps[4]).as_ref(), Some(&fps[4]));
		assert!(chain_to(&chain, "unknown").is_empty());
		fs::remove_dir_all(dir).unwrap();
	}

	#[test]
	fn a_rotation_stopped_midway_is_finished_or_undone() {
		let dir = scratch();
		let first = Identity::load_or_create(&dir).unwrap();
		let (cert, key) = new_certificate().unwrap();
		let next = serde_json::to_vec(&Next { cert: B64.encode(&cert), key: B64.encode(&key) }).unwrap();
		// Stopped before the statement was saved: the new pair is dropped.
		fs::write(dir.join(NEXT_FILE), &next).unwrap();
		assert_eq!(Identity::load_or_create(&dir).unwrap().fingerprint, first.fingerprint);
		assert!(!dir.join(NEXT_FILE).exists());
		// Saved, but stopped with the old certificate in place: the new one is taken up.
		let r = Rotation {
			format: protocol::ROTATION_FORMAT.into(),
			old_fp: first.fingerprint.clone(),
			new_fp: sha256_hex(&cert),
			t: 1,
			old_cert: String::new(),
			signature: String::new(),
		};
		write_json::<Vec<Rotation>>(&dir.join(ROTATIONS_FILE), &[r]).unwrap();
		fs::write(dir.join(NEXT_FILE), &next).unwrap();
		fs::write(dir.join(KEY_FILE), &key).unwrap();
		let now = Identity::load_or_create(&dir).unwrap();
		assert_eq!((now.fingerprint, now.key_der), (sha256_hex(&cert), key));
		assert!(!dir.join(NEXT_FILE).exists());
		fs::remove_dir_all(dir).unwrap();
	}

	#[test]
	fn a_chain_that_doesnt_hold_leads_nowhere() {
		let dir = scratch();
		let first = Identity::load_or_create(&dir).unwrap().fingerprint;
		for _ in 0..3 {
			rotate(&dir).unwrap();
		}
		let good = rotations(&dir).unwrap();
		let last = good[2].new_fp.clone();
		assert_eq!(follow(&good, &first, &last).as_ref(), Some(&last));
		let mut bad = good.clone();
		bad[1].signature = good[0].signature.clone();
		assert_eq!(follow(&bad, &first, &last), None, "a signature over something else");
		let mut bad = good.clone();
		bad.remove(1);
		assert_eq!(follow(&bad, &first, &last), None, "a missing step");
		let mut bad = good.clone();
		bad[0].format = "elamon-rotate-v2".into();
		assert_eq!(follow(&bad, &first, &last), None, "a format it doesn't know");
		let mut bad = good.clone();
		bad[1].t += 1;
		assert_eq!(follow(&bad, &first, &last), None, "a changed time");
		assert_eq!(follow(&good, &first, &first), None, "not to the certificate presented");
		fs::remove_dir_all(dir).unwrap();
	}

	#[test]
	fn rotating_needs_a_certificate_and_a_readable_chain() {
		let dir = scratch();
		assert!(rotate(&dir).unwrap_err().contains("no certificate"));
		Identity::load_or_create(&dir).unwrap();
		fs::write(dir.join(ROTATIONS_FILE), "[{").unwrap();
		assert!(rotate(&dir).unwrap_err().contains("move it away"));
		// Nothing was changed.
		assert_eq!(fs::read(dir.join(ROTATIONS_FILE)).unwrap(), b"[{");
		fs::remove_dir_all(dir).unwrap();
	}
}
