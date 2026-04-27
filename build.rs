//! Build script: read Google OAuth2 credentials from env or .env file.

fn main() {
    println!("cargo:rerun-if-changed=locales/");
    println!("cargo:rerun-if-changed=.env");

    // Priority 1: OS environment variables
    let mut client_id = std::env::var("OAK_GOOGLE_CLIENT_ID").unwrap_or_default();
    let mut client_secret = std::env::var("OAK_GOOGLE_CLIENT_SECRET").unwrap_or_default();

    // Priority 2: .env file fallback
    if client_id.is_empty() || client_secret.is_empty() {
        if let Ok(env_content) = std::fs::read_to_string(".env") {
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

    println!("cargo:rustc-env=OAK_GOOGLE_CLIENT_ID={}", client_id);
    println!("cargo:rustc-env=OAK_GOOGLE_CLIENT_SECRET={}", client_secret);
}
