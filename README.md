# nntp

[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)

A NNTP (Network News Transfer Protocol) client library for Rust, implementing
[RFC 3977](https://tools.ietf.org/html/rfc3977) and
[RFC 4643](https://tools.ietf.org/html/rfc4643) (authentication extension).

## Features

- Connect to NNTP servers with automatic retry and exponential backoff
- Retrieve articles by number or message ID
- Fetch article headers, body, or full content
- List and select newsgroups
- Post messages to newsgroups
- USER/PASS authentication with automatic re-authentication on reconnect
- UTF-8 and WINDOWS-1252 encoding support

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
nntp = "0.1.0"
```

### Example

```rust
use nntp::NNTPStream;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to an NNTP server
    let mut client = NNTPStream::connect("nntp.example.com:119")?;

    // Optional: authenticate
    // client.user_password_authenticate("user", "password")?;

    // List available newsgroups
    let groups = client.list()?;
    for group in &groups {
        println!("{} ({} articles)", group.name, group.number);
    }

    // Select a newsgroup
    let group = client.group("comp.test")?;
    println!("Selected: {}", group);

    // Fetch an article
    match client.article_by_number(1) {
        Ok(article) => {
            // Print headers
            for (key, value) in &article.headers {
                println!("{}: {}", key, value);
            }
            // Print body
            for line in &article.body {
                print!("{}", line);
            }
        }
        Err(e) => eprintln!("Failed to fetch article: {}", e),
    }

    // Disconnect
    let _ = client.quit();
    Ok(())
}
```

## Documentation

- [API Documentation](https://docs.rs/nntp)
- [RFC 3977 - Network News Transfer Protocol](https://tools.ietf.org/html/rfc3977)
- [RFC 4643 - NNTP Authentication Extension](https://tools.ietf.org/html/rfc4643)

## Minimum Supported Rust Version

This crate uses Rust 2024 edition.

## License

Licensed under the GNU General Public License v3.0 — see [LICENSE](LICENSE) for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
