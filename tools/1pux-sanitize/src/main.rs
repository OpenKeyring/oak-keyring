//! 1pux-sanitize: removes brand references from .1pux files.
//!
//! Usage: 1pux-sanitize <input.1pux> <output.1pux>

use std::io::{Read, Write};

use serde_json::Value;
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <input.1pux> <output.1pux>", args[0]);
        std::process::exit(1);
    }

    let input_path = &args[1];
    let output_path = &args[2];

    let input_file = std::fs::File::open(input_path).expect("open input");
    let mut archive =
        zip::ZipArchive::new(input_file).expect("open ZIP archive");

    // Build file rename map first (needs mutable borrow).
    let rename_map = build_file_rename_map(&mut archive);

    // Read and sanitize export.data.
    let mut export_data = String::new();
    archive
        .by_name("export.data")
        .expect("find export.data")
        .read_to_string(&mut export_data)
        .expect("read export.data");

    let mut json: Value = serde_json::from_str(&export_data).expect("parse JSON");
    sanitize_json(&mut json);
    let sanitized_json = serde_json::to_string_pretty(&json).expect("serialize JSON");

    // Write output ZIP.
    let output_file = std::fs::File::create(output_path).expect("create output");
    let mut writer = zip::ZipWriter::new(output_file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

    // Write export.data.
    writer.start_file("export.data", options).expect("start export.data");
    writer
        .write_all(sanitized_json.as_bytes())
        .expect("write export.data");

    // Copy export.attributes.
    let mut attrs = String::new();
    archive
        .by_name("export.attributes")
        .expect("find export.attributes")
        .read_to_string(&mut attrs)
        .expect("read export.attributes");
    writer
        .start_file("export.attributes", options)
        .expect("start export.attributes");
    writer.write_all(attrs.as_bytes()).expect("write export.attributes");

    // Copy files/ directory entry and all files (with renames).
    writer.add_directory("files/", options).expect("add files dir");

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).expect("get entry");
        let name = entry.name().to_string();
        if name == "export.data" || name == "export.attributes" || name == "files/" {
            continue;
        }
        let output_name = rename_map.get(&name).map(|s| s.as_str()).unwrap_or(&name);
        let mut data = Vec::new();
        entry.read_to_end(&mut data).expect("read file");
        writer
            .start_file(output_name, options)
            .expect("start file");
        writer.write_all(&data).expect("write file");
    }

    writer.finish().expect("finish ZIP");
    println!("Sanitized: {} -> {}", input_path, output_path);
}

/// Build a map from old filenames to new filenames for brand cleanup.
fn build_file_rename_map(archive: &mut zip::ZipArchive<std::fs::File>) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i).expect("get entry");
        let name = entry.name().to_string();
        if name.contains("keepassxc") {
            let new_name = name.replace("keepassxc", "logo");
            map.insert(name, new_name);
        }
    }
    map
}

/// Sanitize JSON by replacing brand references at known locations.
fn sanitize_json(json: &mut Value) {
    let accounts = json
        .get_mut("accounts")
        .and_then(|v| v.as_array_mut())
        .expect("accounts array");

    for account in accounts {
        // Account-level attrs.
        if let Some(attrs) = account.get_mut("attrs") {
            replace_str(attrs, "accountName", "Team KeePassXC", "Personal");
            replace_str(attrs, "name", "Team KeePassXC", "Personal");
            replace_str(attrs, "email", "team@keepassxc.org", "user@example.com");
        }

        let vaults = account
            .get_mut("vaults")
            .and_then(|v| v.as_array_mut())
            .expect("vaults array");

        for vault in vaults {
            let items = vault
                .get_mut("items")
                .and_then(|v| v.as_array_mut())
                .expect("items array");

            for item in items {
                sanitize_item(item);
            }
        }
    }
}

/// Sanitize a single item's brand references.
fn sanitize_item(item: &mut Value) {
    // Overview: url, subtitle.
    if let Some(overview) = item.get_mut("overview") {
        replace_str(overview, "url", "https://keepassxc.org", "https://example.com");
        replace_str(overview, "subtitle", "team@keepassxc.org", "user@example.com");
        replace_str(overview, "subtitle", "KeePass XC", "John Doe");
        replace_str(overview, "title", "KeePassXC Logo", "Logo");

        // URLs array.
        if let Some(urls) = overview.get_mut("urls").and_then(|v| v.as_array_mut()) {
            for url_entry in urls {
                replace_str(url_entry, "url", "https://keepassxc.org", "https://example.com");
                replace_str(url_entry, "url", "https://twitter.com", "https://social.example.com");
            }
        }
    }

    // Details: loginFields, sections, documentAttributes.
    if let Some(details) = item.get_mut("details") {
        // LoginFields: replace username.
        if let Some(fields) = details.get_mut("loginFields").and_then(|v| v.as_array_mut()) {
            for field in fields {
                if field
                    .get("designation")
                    .and_then(|v| v.as_str())
                    .map(|s| s == "username")
                    .unwrap_or(false)
                {
                    replace_str(field, "value", "team@keepassxc.org", "user@example.com");
                }
            }
        }

        // Sections: replace brand values.
        if let Some(sections) = details.get_mut("sections").and_then(|v| v.as_array_mut()) {
            for section in sections {
                if let Some(fields) = section.get_mut("fields").and_then(|v| v.as_array_mut()) {
                    for sf in fields {
                        sanitize_section_field(sf);
                    }
                }
            }
        }

        // Document attributes.
        if let Some(doc_attrs) = details.get_mut("documentAttributes") {
            replace_str(doc_attrs, "fileName", "keepassxc.png", "logo.png");
        }
    }
}

/// Sanitize a section field's value.
fn sanitize_section_field(sf: &mut Value) {
    let id = sf.get("id").and_then(|v| v.as_str()).unwrap_or("");

    match id {
        "cardholder" | "name_on_account" | "account_name" => {
            if let Some(val) = sf.get_mut("value") {
                replace_in_value_object(val, "string", "KeePassXC", "John Doe");
            }
        }
        "firstname" => {
            if let Some(val) = sf.get_mut("value") {
                replace_in_value_object(val, "string", "KeePass", "John");
            }
        }
        "lastname" => {
            if let Some(val) = sf.get_mut("value") {
                replace_in_value_object(val, "string", "XC", "Doe");
            }
        }
        _ => {
            // Generic: replace email and any remaining brand strings in values.
            if let Some(val) = sf.get_mut("value") {
                if val.is_object() {
                    if let Some(email) = val.get_mut("email") {
                        if email.is_object() {
                            replace_str(email, "email_address", "team@keepassxc.org", "user@example.com");
                        }
                    }
                    // Catch any remaining brand strings in value objects.
                    for key in &["string", "concealed"] {
                        if let Some(v) = val.get_mut(*key) {
                            if v.as_str() == Some("KeePassXC") {
                                *v = Value::String("John Doe".into());
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Replace a string value at a given key if it matches the old value.
fn replace_str(obj: &mut Value, key: &str, old: &str, new: &str) {
    if let Some(v) = obj.get_mut(key) {
        if v.as_str() == Some(old) {
            *v = Value::String(new.to_string());
        }
    }
}

/// Replace a string inside a value object at a given sub-key.
fn replace_in_value_object(val: &mut Value, sub_key: &str, old: &str, new: &str) {
    if let Some(map) = val.as_object_mut() {
        if let Some(v) = map.get_mut(sub_key) {
            if v.as_str() == Some(old) {
                *v = Value::String(new.to_string());
            }
        }
    }
}
