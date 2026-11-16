//! A simulated upsd for maestro/run.sh: upst-core's demo server on a free loopback port. Prints its
//! address (`127.0.0.1:PORT`) on one line, then runs until it is killed.
use std::io::Write;

fn main() {
	let server = upst_core::demo::Server::spawn().expect("starting the simulated upsd");
	let mut out = std::io::stdout();
	writeln!(out, "{}", server.addr()).and_then(|()| out.flush()).expect("printing the address");
	loop {
		std::thread::park();
	}
}
