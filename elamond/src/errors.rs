//! Every `error` the API answers with and its HTTP status: the one list the code uses, which
//! docs/protocol.md repeats and elamond/fixtures/errors.json holds for the app's tests.

use axum::http::StatusCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCode {
	/// A parameter of an action is wrong, missing or unknown; `message` names it.
	BadParam,
	/// A table asked for an order that isn't among its `sorts`.
	BadSort,
	UnknownSignal,
	/// No token, or one that no paired phone has.
	Unauthorized,
	/// Pairing without a code pending.
	NoPairing,
	/// Pairing with a wrong code.
	WrongCode,
	/// The person at `elamond pair` turned the phone away, didn't answer in time, or stopped it.
	PairDeclined,
	/// Pairing or `rekey` while the most phones there may be are paired.
	TooManyPhones,
	/// `rekey` from an entry that came by `rekey` itself, or that asked once already.
	RekeyRefused,
	/// The request changes the computer and `control` is off.
	ControlDisabled,
	/// polkit or the kernel refused, or the process is one the service won't signal.
	NotPermitted,
	/// The phone's role doesn't allow it; see `access::Role`.
	NotAllowed,
	/// No such path.
	NotFound,
	NoSuchProcess,
	/// No such action on this computer.
	NoSuchAction,
	/// No such dashboard in the client's `ui`.
	NoSuchDashboard,
	/// No open stream of this client with that id.
	NoSuchStream,
	/// The player has no cover in a local file, or no such player.
	NoArt,
	/// A signal named the process's start time, and the process with that pid started at another.
	ProcessChanged,
	/// Too many acting requests from this phone, or too many wrong pairing codes from this address.
	TooManyRequests,
	/// A state file couldn't be written.
	Storage,
	/// What was asked failed; `message` says why.
	Failed,
}

impl ErrorCode {
	/// Every code, in the order of docs/protocol.md, for elamond/fixtures/errors.json.
	#[cfg(test)]
	pub const ALL: [ErrorCode; 22] = [
		ErrorCode::BadParam,
		ErrorCode::BadSort,
		ErrorCode::UnknownSignal,
		ErrorCode::Unauthorized,
		ErrorCode::NoPairing,
		ErrorCode::WrongCode,
		ErrorCode::PairDeclined,
		ErrorCode::TooManyPhones,
		ErrorCode::RekeyRefused,
		ErrorCode::ControlDisabled,
		ErrorCode::NotPermitted,
		ErrorCode::NotAllowed,
		ErrorCode::NotFound,
		ErrorCode::NoSuchProcess,
		ErrorCode::NoSuchAction,
		ErrorCode::NoSuchDashboard,
		ErrorCode::NoSuchStream,
		ErrorCode::NoArt,
		ErrorCode::ProcessChanged,
		ErrorCode::TooManyRequests,
		ErrorCode::Storage,
		ErrorCode::Failed,
	];

	fn parts(self) -> (&'static str, StatusCode) {
		match self {
			ErrorCode::BadParam => ("bad_param", StatusCode::BAD_REQUEST),
			ErrorCode::BadSort => ("bad_sort", StatusCode::BAD_REQUEST),
			ErrorCode::UnknownSignal => ("unknown_signal", StatusCode::BAD_REQUEST),
			ErrorCode::Unauthorized => ("unauthorized", StatusCode::UNAUTHORIZED),
			ErrorCode::NoPairing => ("no_pairing", StatusCode::FORBIDDEN),
			ErrorCode::WrongCode => ("wrong_code", StatusCode::FORBIDDEN),
			ErrorCode::PairDeclined => ("pair_declined", StatusCode::FORBIDDEN),
			ErrorCode::TooManyPhones => ("too_many_phones", StatusCode::FORBIDDEN),
			ErrorCode::RekeyRefused => ("rekey_refused", StatusCode::FORBIDDEN),
			ErrorCode::ControlDisabled => ("control_disabled", StatusCode::FORBIDDEN),
			ErrorCode::NotPermitted => ("not_permitted", StatusCode::FORBIDDEN),
			ErrorCode::NotAllowed => ("not_allowed", StatusCode::FORBIDDEN),
			ErrorCode::NotFound => ("not_found", StatusCode::NOT_FOUND),
			ErrorCode::NoSuchProcess => ("no_such_process", StatusCode::NOT_FOUND),
			ErrorCode::NoSuchAction => ("no_such_action", StatusCode::NOT_FOUND),
			ErrorCode::NoSuchDashboard => ("no_such_dashboard", StatusCode::NOT_FOUND),
			ErrorCode::NoSuchStream => ("no_such_stream", StatusCode::NOT_FOUND),
			ErrorCode::NoArt => ("no_art", StatusCode::NOT_FOUND),
			ErrorCode::ProcessChanged => ("process_changed", StatusCode::CONFLICT),
			ErrorCode::TooManyRequests => ("too_many_requests", StatusCode::TOO_MANY_REQUESTS),
			ErrorCode::Storage => ("storage", StatusCode::INTERNAL_SERVER_ERROR),
			ErrorCode::Failed => ("failed", StatusCode::BAD_GATEWAY),
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
