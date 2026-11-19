//! The certificates the service presents (docs/config.md, "The web page and the admin API"): its
//! own, which the app pins, to every client, and a real one from files (`[web] cert_name`,
//! `cert_file`, `key_file`) to a browser that asks for that name (SNI) alone. The files are read
//! again when they change; while they don't load, aren't for their key or have expired, the service
//! presents its own and says why.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

use rustls::crypto::CryptoProvider;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;

use crate::settings::WebDef;

/// What became of the real certificate, as `elamond check`, `setup --check` and the admin API say it.
#[derive(Clone, Debug, PartialEq)]
pub struct State {
	pub name: String,
	/// Presented to browsers that ask for `name`.
	pub ok: bool,
	/// Unix time it expires, when it could be read.
	pub until: Option<u64>,
	/// One line: "valid until 2026-02-13", or what is wrong.
	pub message: String,
}

/// What a file looked like when read: modified and length, to notice a new one.
type Stamp = Option<(SystemTime, u64)>;

/// The name, the files and their stamps a certificate was read with.
type Read = (String, PathBuf, PathBuf, Stamp, Stamp);

fn stamp(p: &Path) -> Stamp {
	let m = fs::metadata(p).ok()?;
	Some((m.modified().ok()?, m.len()))
}

/// Picks the certificate for a connection: the real one for its name, the service's own else.
#[derive(Debug)]
pub struct Resolver {
	own: RwLock<Arc<CertifiedKey>>,
	/// The real one and the name it is for, while it loads.
	web: RwLock<Option<(String, Arc<CertifiedKey>)>>,
	/// The files and their stamps as last read, to read them again only when they change.
	read: RwLock<Option<Read>>,
}

impl ResolvesServerCert for Resolver {
	fn resolve(&self, hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
		if let Some(sni) = hello.server_name()
			&& let Ok(web) = self.web.read()
			&& let Some((name, key)) = web.as_ref()
			&& sni.trim_end_matches('.').eq_ignore_ascii_case(name.trim_end_matches('.'))
		{
			return Some(key.clone());
		}
		self.own.read().ok().map(|k| k.clone())
	}
}

fn provider() -> Arc<CryptoProvider> {
	Arc::new(rustls::crypto::ring::default_provider())
}

/// The service's own certificate and key, DER.
pub fn own_key(cert: &[u8], key: &[u8]) -> Result<CertifiedKey, String> {
	let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.to_vec()));
	CertifiedKey::from_der(vec![CertificateDer::from(cert.to_vec())], key, &provider()).map_err(|e| e.to_string())
}

impl Resolver {
	pub fn new(own: CertifiedKey) -> Resolver {
		Resolver { own: RwLock::new(Arc::new(own)), web: RwLock::new(None), read: RwLock::new(None) }
	}

	/// The service's own certificate from now on (after `elamond rotate-cert`).
	pub fn set_own(&self, own: CertifiedKey) {
		if let Ok(mut k) = self.own.write() {
			*k = Arc::new(own);
		}
	}

	/// Takes `[web]` as the settings say it now: reads the real certificate's files when they are
	/// new to it or changed, and says what became of them, once for each change (`None` when nothing
	/// changed). Without a certificate in the settings, there is none.
	pub fn refresh(&self, web: &WebDef, now: u64) -> Option<Option<State>> {
		let (Some(name), Some(cert), Some(key)) = (&web.cert_name, &web.cert_file, &web.key_file) else {
			let had = self.read.write().map(|mut r| r.take().is_some()).unwrap_or(false);
			if let Ok(mut w) = self.web.write() {
				*w = None;
			}
			return had.then_some(None);
		};
		let seen = (name.clone(), cert.clone(), key.clone(), stamp(cert), stamp(key));
		if self.read.read().ok().and_then(|r| r.clone()).as_ref() == Some(&seen) {
			return None;
		}
		if let Ok(mut r) = self.read.write() {
			*r = Some(seen);
		}
		let (state, loaded) = load(name, cert, key, now);
		if let Ok(mut w) = self.web.write() {
			// A certificate that doesn't load leaves the one before, if any, else the service's own.
			if let Some(k) = loaded {
				*w = Some((name.clone(), Arc::new(k)));
			} else if w.as_ref().is_some_and(|(n, _)| n != name) {
				*w = None;
			}
		}
		Some(Some(state))
	}
}

/// The real certificate from its files, and what to say of it.
pub fn load(name: &str, cert: &Path, key: &Path, now: u64) -> (State, Option<CertifiedKey>) {
	let bad = |message: String| (State { name: name.to_string(), ok: false, until: None, message }, None);
	let chain: Vec<CertificateDer<'static>> = match fs::read(cert) {
		Ok(b) => CertificateDer::pem_slice_iter(&b).filter_map(Result::ok).collect(),
		Err(e) => return bad(format!("{} can't be read: {e}", cert.display())),
	};
	let Some(first) = chain.first() else { return bad(format!("{} holds no certificate (PEM)", cert.display())) };
	let until = not_after(first);
	let key_der = match fs::read(key) {
		Ok(b) => match PrivateKeyDer::from_pem_slice(&b) {
			Ok(k) => k,
			Err(_) => return bad(format!("{} holds no private key (PEM)", key.display())),
		},
		Err(e) => return bad(format!("{} can't be read: {e}", key.display())),
	};
	let certified = match CertifiedKey::from_der(chain, key_der, &provider()) {
		Ok(k) => k,
		Err(e) => return bad(format!("the key in {} isn't the certificate's ({e})", key.display())),
	};
	let date = |t: u64| crate::store::format_date(t);
	match until {
		Some(t) if t <= now => (State { name: name.to_string(), ok: false, until, message: format!("expired on {}", date(t)) }, None),
		Some(t) => (State { name: name.to_string(), ok: true, until, message: format!("valid until {}", date(t)) }, Some(certified)),
		None => (State { name: name.to_string(), ok: true, until: None, message: "loaded; its expiry can't be read".into() }, Some(certified)),
	}
}

/// What `elamond check` and `setup --check` say of the real certificate in `web`, if there is one.
pub fn check(web: &WebDef) -> Option<(bool, String)> {
	let (Some(name), Some(cert), Some(key)) = (&web.cert_name, &web.cert_file, &web.key_file) else { return None };
	let (state, _) = load(name, cert, key, crate::store::now());
	let mut line = format!("web certificate for {name}: {}", state.message);
	if state.ok {
		line.push_str(" (browsers that reach the computer by that name get it; others the service's own)");
	} else {
		line.push_str("; the service presents its own meanwhile");
	}
	let secret = crate::settings::secret_file_warning("web.key_file", key, "the certificate isn't presented until it is");
	if let Some(w) = secret.into_iter().next() {
		return Some((false, format!("{line}; {w}")));
	}
	Some((state.ok, line))
}

// ---- a certificate's expiry, read from its DER ----------------------------------------------

/// One DER element at the start of `b`: its tag, its contents, and what follows it.
fn element(b: &[u8]) -> Option<(u8, &[u8], &[u8])> {
	let (&tag, rest) = b.split_first()?;
	let (&first, rest) = rest.split_first()?;
	let (len, rest) = if first < 0x80 {
		(first as usize, rest)
	} else {
		let n = (first & 0x7f) as usize;
		if n == 0 || n > 4 || rest.len() < n {
			return None;
		}
		(rest[..n].iter().fold(0usize, |a, b| (a << 8) | *b as usize), &rest[n..])
	};
	(rest.len() >= len).then(|| (tag, &rest[..len], &rest[len..]))
}

/// `notAfter` of an X.509 certificate, as Unix time.
pub fn not_after(cert: &[u8]) -> Option<u64> {
	let (_, certificate, _) = element(cert)?;
	let (_, tbs, _) = element(certificate)?;
	let mut at = tbs;
	// [0] version, when there is one.
	if at.first() == Some(&0xa0) {
		at = element(at)?.2;
	}
	// serialNumber, signature, issuer, then validity.
	for _ in 0..3 {
		at = element(at)?.2;
	}
	let (_, validity, _) = element(at)?;
	let (_, _, rest) = element(validity)?;
	let (tag, time, _) = element(rest)?;
	der_time(tag, std::str::from_utf8(time).ok()?)
}

/// UTCTime (`YYMMDDHHMMSSZ`) or GeneralizedTime (`YYYYMMDDHHMMSSZ`) as Unix time.
fn der_time(tag: u8, s: &str) -> Option<u64> {
	let s = s.strip_suffix('Z')?;
	let (year, rest) = match tag {
		0x17 if s.len() == 12 => {
			let yy: u64 = s[..2].parse().ok()?;
			(if yy >= 50 { 1900 + yy } else { 2000 + yy }, &s[2..])
		}
		0x18 if s.len() == 14 => (s[..4].parse().ok()?, &s[4..]),
		_ => return None,
	};
	let n = |i: usize| rest.get(i..i + 2).and_then(|d| d.parse::<u64>().ok());
	let (month, day, hour, minute, second) = (n(0)?, n(2)?, n(4)?, n(6)?, n(8)?);
	if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
		return None;
	}
	// Days from 1970-01-01 to the date (the civil-from-days inverse, proleptic Gregorian).
	let y = if month <= 2 { year - 1 } else { year } as i64;
	let era = y.div_euclid(400);
	let yoe = y - era * 400;
	let mp = (month as i64 + 9) % 12;
	let doy = (153 * mp + 2) / 5 + day as i64 - 1;
	let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
	let days = era * 146_097 + doe - 719_468;
	u64::try_from(days).ok().map(|d| d * 86_400 + hour * 3600 + minute * 60 + second)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn scratch() -> PathBuf {
		let d = std::env::temp_dir().join(format!("elamond-webcert-{}", data_encoding::HEXLOWER.encode(&crate::store::random_bytes::<6>())));
		fs::create_dir_all(&d).unwrap();
		d
	}

	/// A certificate for `name` and its key, as PEM files in `dir`.
	fn pem_pair(dir: &Path, name: &str) -> (PathBuf, PathBuf) {
		let ck = rcgen::generate_simple_self_signed(vec![name.to_string()]).unwrap();
		let (cert, key) = (dir.join(format!("{name}.crt")), dir.join(format!("{name}.key")));
		fs::write(&cert, ck.cert.pem()).unwrap();
		fs::write(&key, ck.signing_key.serialize_pem()).unwrap();
		use std::os::unix::fs::PermissionsExt;
		fs::set_permissions(&key, fs::Permissions::from_mode(0o600)).unwrap();
		(cert, key)
	}

	#[test]
	fn dates_of_certificates() {
		assert_eq!(der_time(0x17, "700101000000Z"), Some(0));
		assert_eq!(der_time(0x17, "260213000000Z"), Some(1_770_940_800));
		assert_eq!(der_time(0x18, "20260213000000Z"), Some(1_770_940_800));
		assert_eq!(der_time(0x18, "20000301000000Z"), Some(951_868_800));
		assert_eq!(der_time(0x17, "2602130000Z"), None);
		let ck = rcgen::generate_simple_self_signed(vec!["desk.example.net".into()]).unwrap();
		// rcgen's are good until 4096-01-01.
		assert_eq!(not_after(ck.cert.der()), Some(67_090_118_400));
	}

	#[test]
	fn the_real_one_for_its_name_alone() {
		let dir = scratch();
		let (cert, key) = pem_pair(&dir, "desk.example.net");
		let web = WebDef { cert_name: Some("desk.example.net".into()), cert_file: Some(cert.clone()), key_file: Some(key.clone()), ..WebDef::default() };
		let own = rcgen::generate_simple_self_signed(vec!["elamon".into()]).unwrap();
		let resolver = Resolver::new(own_key(own.cert.der(), &own.signing_key.serialize_der()).unwrap());
		let state = resolver.refresh(&web, 1_800_000_000).unwrap().unwrap();
		assert!(state.ok, "{state:?}");
		assert!(state.message.starts_with("valid until 4096-01-01"), "{}", state.message);
		// Nothing changed: nothing to say.
		assert_eq!(resolver.refresh(&web, 1_800_000_000), None);
		let (name, _) = resolver.web.read().unwrap().clone().unwrap();
		assert_eq!(name, "desk.example.net");
		// A key that isn't the certificate's: said, and the one before stays.
		let (_, other_key) = pem_pair(&dir, "other.example.net");
		fs::copy(&other_key, &key).unwrap();
		let state = resolver.refresh(&web, 1_800_000_000).unwrap().unwrap();
		assert!(!state.ok && state.message.contains("isn't the certificate's"), "{state:?}");
		assert!(resolver.web.read().unwrap().is_some());
		// Expired.
		let (late_cert, late_key) = pem_pair(&dir, "late.example.net");
		let (state, k) = load("late.example.net", &late_cert, &key, 1_800_000_000);
		assert!(!state.ok && k.is_none());
		let (state, k) = load("late.example.net", &late_cert, &late_key, 70_000_000_000);
		assert!(!state.ok && state.message.starts_with("expired on") && k.is_none(), "{state:?}");
		// Without a certificate in the settings, none.
		assert_eq!(resolver.refresh(&WebDef::default(), 0), Some(None));
		assert!(resolver.web.read().unwrap().is_none());
		assert_eq!(check(&WebDef::default()), None);
		fs::remove_dir_all(&dir).unwrap();
	}
}
