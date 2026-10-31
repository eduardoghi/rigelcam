//! Prints what a recorder reports, to check a new model before reporting a problem with it.
//!
//! The password comes from `RIGELCAM_PASSWORD` so it stays out of the shell history.

use std::process::ExitCode;

use rigelcam_core::dahua::{Client, Stream};

/// Text from the recorder, which can't be allowed to send commands to the terminal.
fn printable(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [host, user] = args.as_slice() else {
        eprintln!("usage: RIGELCAM_PASSWORD=... rigelcam-cli <host> <user>");
        return ExitCode::from(2);
    };
    let Ok(password) = std::env::var("RIGELCAM_PASSWORD") else {
        eprintln!("RIGELCAM_PASSWORD is not set");
        return ExitCode::from(2);
    };
    let client = Client::new(host, user, &password);
    let result = client.device_type().and_then(|model| {
        println!("model: {}", printable(&model));
        client.channels()
    });
    match result {
        Ok(channels) => {
            for channel in channels {
                println!(
                    "{:>3}  {:<24} {}",
                    channel.number,
                    printable(&channel.name),
                    client.live_url(channel.number, Stream::Sub)
                );
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
