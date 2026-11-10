//! Every `error` the API answers with and its HTTP status: the one list the code uses, which
//! docs/protocol.md repeats and elamond/fixtures/errors.json holds for the app's tests.

use axum::http::StatusCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
	/// No such path.
	NotFound,
	/// No token, or one that no paired phone has.
	Unauthorized,
	/// Pairing without a code pending.
	NoPairing,
	/// Pairing with a wrong code.
	WrongCode,
	/// A state file couldn't be written.
	Storage,
	/// The request changes the computer and `control` is off.
	ControlDisabled,
	/// polkit or the kernel refused, or the process is one the service won't signal.
	NotPermitted,
	/// What was asked failed; `message` says why.
	Failed,
	UnknownSignal,
	NoSuchProcess,
	NoSuchAction,
	/// An action with an empty command in the settings.
	EmptyCommand,
	/// The player has no cover in a local file, or no such player.
	NoArt,
	BadName,
	BadUrl,
	BadUnit,
	BadScope,
	NoSuchService,
	/// The service has no unit to start or stop.
	NoUnit,
}

impl ErrorCode {
	/// Every code, for elamond/fixtures/errors.json.
	#[cfg(test)]
	pub const ALL: [ErrorCode; 19] = [
		ErrorCode::NotFound,
		ErrorCode::Unauthorized,
		ErrorCode::NoPairing,
		ErrorCode::WrongCode,
		ErrorCode::Storage,
		ErrorCode::ControlDisabled,
		ErrorCode::NotPermitted,
		ErrorCode::Failed,
		ErrorCode::UnknownSignal,
		ErrorCode::NoSuchProcess,
		ErrorCode::NoSuchAction,
		ErrorCode::EmptyCommand,
		ErrorCode::NoArt,
		ErrorCode::BadName,
		ErrorCode::BadUrl,
		ErrorCode::BadUnit,
		ErrorCode::BadScope,
		ErrorCode::NoSuchService,
		ErrorCode::NoUnit,
	];

	fn parts(self) -> (&'static str, StatusCode) {
		match self {
			ErrorCode::NotFound => ("not_found", StatusCode::NOT_FOUND),
			ErrorCode::Unauthorized => ("unauthorized", StatusCode::UNAUTHORIZED),
			ErrorCode::NoPairing => ("no_pairing", StatusCode::FORBIDDEN),
			ErrorCode::WrongCode => ("wrong_code", StatusCode::FORBIDDEN),
			ErrorCode::Storage => ("storage", StatusCode::INTERNAL_SERVER_ERROR),
			ErrorCode::ControlDisabled => ("control_disabled", StatusCode::FORBIDDEN),
			ErrorCode::NotPermitted => ("not_permitted", StatusCode::FORBIDDEN),
			ErrorCode::Failed => ("failed", StatusCode::BAD_GATEWAY),
			ErrorCode::UnknownSignal => ("unknown_signal", StatusCode::BAD_REQUEST),
			ErrorCode::NoSuchProcess => ("no_such_process", StatusCode::NOT_FOUND),
			ErrorCode::NoSuchAction => ("no_such_action", StatusCode::NOT_FOUND),
			ErrorCode::EmptyCommand => ("empty_command", StatusCode::BAD_REQUEST),
			ErrorCode::NoArt => ("no_art", StatusCode::NOT_FOUND),
			ErrorCode::BadName => ("bad_name", StatusCode::BAD_REQUEST),
			ErrorCode::BadUrl => ("bad_url", StatusCode::BAD_REQUEST),
			ErrorCode::BadUnit => ("bad_unit", StatusCode::BAD_REQUEST),
			ErrorCode::BadScope => ("bad_scope", StatusCode::BAD_REQUEST),
			ErrorCode::NoSuchService => ("no_such_service", StatusCode::NOT_FOUND),
			ErrorCode::NoUnit => ("no_unit", StatusCode::BAD_REQUEST),
		}
	}

	pub fn as_str(self) -> &'static str {
		self.parts().0
	}

	pub fn status(self) -> StatusCode {
		self.parts().1
	}
}

/// What `message` says under `failed` when the service itself turned the request down, rather
/// than passing on what D-Bus or a command said.
pub mod reason {
	pub const UNKNOWN_ACTION: &str = "unknown_action";
	pub const UNKNOWN_OPERATION: &str = "unknown_operation";
	pub const UNKNOWN_PLAYER: &str = "unknown_player";
	pub const UNKNOWN_PROFILE: &str = "unknown_profile";
	pub const NO_VALUE: &str = "no_value";
	pub const NO_TRACK: &str = "no_track";
	pub const NO_SESSION: &str = "no_session";
	pub const NO_SYSTEM_BUS: &str = "no_system_bus";
	pub const NO_SESSION_BUS: &str = "no_session_bus";
	pub const NO_POWER_PROFILES: &str = "no_power_profiles";
	pub const VOLUME_FAILED: &str = "volume_failed";
	pub const MUTE_FAILED: &str = "mute_failed";
	pub const TIMED_OUT: &str = "timed_out";
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn codes_are_unique_and_snake_case() {
		let codes: Vec<&str> = ErrorCode::ALL.iter().map(|c| c.as_str()).collect();
		for (i, c) in codes.iter().enumerate() {
			assert!(c.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'), "{c}");
			assert!(!codes[..i].contains(c), "{c} twice");
		}
	}
}
