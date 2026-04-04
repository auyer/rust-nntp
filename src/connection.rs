use core::net;
use std::io;
use std::net::TcpStream;
use std::net::ToSocketAddrs;
use std::thread::sleep;
use std::time::Duration;
use std::vec::Vec;

pub(crate) fn connect_with_retry(
    addr: &str,
    max_retries: usize,
    retry_delay_ms: usize,
    timeout_secs: u64,
) -> io::Result<TcpStream> {
    let server: Vec<net::SocketAddr> = addr.to_socket_addrs()?.collect();

    if server.is_empty() {
        log::warn!("Address resolved to no socket addresses.");
        return Err(io::Error::other("address resolution failed"));
    }

    if server.len() > 1 {
        log::debug!(
            "addr resolved into multiple addresses, trying them cyclically : {:#?}",
            server
        );
    } else {
        log::debug!("addr resolved into: {:#?}", server.first());
    }

    // .cycle() creates an iterator that repeats the list of addresses indefinitely
    let mut addr_iter = server.iter().cycle();

    let mut attempts = 0;
    let mut last_error: Option<io::Error> = None;
    let timeout = Duration::from_secs(timeout_secs);

    // at least one connection should be attempted
    while attempts <= max_retries {
        let address = addr_iter
            .next()
            .expect("addresses should not be empty at this point");

        log::debug!(
            "Attempt {}/{}: Trying {}",
            attempts + 1,
            max_retries,
            address
        );

        match TcpStream::connect_timeout(address, timeout) {
            Ok(stream) => {
                // Success! Set timeouts and return the stream.
                stream.set_read_timeout(Some(timeout))?;
                stream.set_write_timeout(Some(timeout))?;
                log::info!("Successfully connected to {}", address);
                return Ok(stream);
            }
            Err(e) => {
                log::warn!("Connection attempt failed: {}", e);
                last_error = Some(e);
                attempts += 1;

                // If we still have attempts left, sleep before the next one
                if attempts < max_retries {
                    // exponential backoff
                    let delay_ms = (retry_delay_ms.pow(attempts as u32)) as u64;
                    log::warn!("Retrying in {}ms...", delay_ms);
                    sleep(Duration::from_millis(delay_ms));
                }
            }
        }
    }

    // If the loop finishes, we've exhausted all retries
    log::error!(
        "Exhausted all {} connection attempts for all addresses.",
        max_retries
    );

    // Return the last error encountered.
    match last_error {
        Some(e) => Err(e),
        None => Err(io::Error::other("Unknown error")),
    }
}
