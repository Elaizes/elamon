//! Builds the web page's files into elamond: every file under web/ (but its tests and its type
//! checker's settings), as they are. Nothing is compiled or bundled: the page is the files.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// What isn't the page: its tests and the settings of `tsc --checkJs`.
fn left_out(rel: &str) -> bool {
	rel.starts_with("test/") || rel == "jsconfig.json" || rel.ends_with(".md") || rel.split('/').any(|p| p.starts_with('.'))
}

fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, PathBuf)>) {
	let mut entries: Vec<_> = fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())).flatten().map(|e| e.path()).collect();
	entries.sort();
	for p in entries {
		let rel = p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
		if p.is_dir() {
			if !left_out(&format!("{rel}/")) {
				walk(&p, base, out);
			}
		} else if !left_out(&rel) {
			out.push((rel, p));
		}
	}
}

fn main() {
	let base = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("web");
	println!("cargo:rerun-if-changed=web");
	let mut files = Vec::new();
	walk(&base, &base, &mut files);
	for (_, p) in &files {
		println!("cargo:rerun-if-changed={}", p.display());
	}
	let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("web_files.rs");
	let mut f = fs::File::create(&out).unwrap();
	writeln!(f, "/// The page's files: their path under web/, and their bytes.").unwrap();
	writeln!(f, "pub const FILES: &[(&str, &[u8])] = &[").unwrap();
	for (rel, p) in &files {
		writeln!(f, "\t({rel:?}, include_bytes!({:?})),", p.display().to_string()).unwrap();
	}
	writeln!(f, "];").unwrap();
}
