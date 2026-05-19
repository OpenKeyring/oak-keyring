//! Build script: read Google OAuth2 credentials from env or .env file,
//! generate obfuscated credential helpers via obfstr.

use std::env;
use std::error::Error;
use std::fs;
use std::path::Path;

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=locales/");
    println!("cargo:rerun-if-changed=.env");

    // Priority 1: OS environment variables
    let mut client_id = env::var("OAK_GOOGLE_CLIENT_ID").unwrap_or_default();
    let mut client_secret = env::var("OAK_GOOGLE_CLIENT_SECRET").unwrap_or_default();

    // Priority 2: .env file fallback
    if client_id.is_empty() || client_secret.is_empty() {
        if let Ok(env_content) = fs::read_to_string(".env") {
            for line in env_content.lines() {
                let line = line.trim();
                if line.starts_with('#') || line.is_empty() {
                    continue;
                }
                if let Some((key, value)) = line.split_once('=') {
                    let key = key.trim();
                    let value = value.trim().trim_matches('"').trim_matches('\'');
                    match key {
                        "OAK_GOOGLE_CLIENT_ID" if client_id.is_empty() => {
                            client_id = value.to_string();
                        }
                        "OAK_GOOGLE_CLIENT_SECRET" if client_secret.is_empty() => {
                            client_secret = value.to_string();
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    if client_id.is_empty() || client_secret.is_empty() {
        eprintln!();
        eprintln!("ERROR: Google OAuth2 credentials not found.");
        eprintln!("Set OAK_GOOGLE_CLIENT_ID and OAK_GOOGLE_CLIENT_SECRET environment variables");
        eprintln!("or create a .env file with these values.");
        eprintln!();
        eprintln!("See .env.example for format.");
        eprintln!();
        std::process::exit(1);
    }

    // Generate obfuscated credential accessors.
    // obfstring! requires a string literal, so we generate a source file
    // with the credentials embedded as literals and include it at compile time.
    let out_dir = env::var("OUT_DIR")?;
    let dest = Path::new(&out_dir).join("_obfuscated_credentials.rs");

    let content = format!(
        r#"/// Returns the compiled-in Google OAuth2 client id (obfuscated in binary).
pub fn google_client_id() -> String {{
    obfstr::obfstring!("{client_id}")
}}

/// Returns the compiled-in Google OAuth2 client secret (obfuscated in binary).
pub fn google_client_secret() -> String {{
    obfstr::obfstring!("{client_secret}")
}}
"#,
        client_id = escape_literal(&client_id),
        client_secret = escape_literal(&client_secret),
    );

    fs::write(&dest, content)?;

    Ok(())
}

/// Escape special characters for use inside a string literal.
fn escape_literal(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
