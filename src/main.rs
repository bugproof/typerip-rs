use std::env;
use std::fs::{File, create_dir_all};
use std::io::{self, Write, BufRead};
use std::io::Cursor;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use ureq;
use serde_json::Value;
use woff2_patched::decode::{convert_woff2_to_ttf, is_woff2};
use clap::{App, Arg};
use arboard::Clipboard;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = App::new("TypeRip")
        .version("1.0")
        .author("Your Name")
        .about("Downloads and converts Adobe Fonts")
        .arg(Arg::with_name("url")
            .help("The Adobe Fonts URL")
            .index(1))
        .arg(Arg::with_name("install")
            .short("i")
            .long("install")
            .help("Auto-install fonts on Windows")
            .takes_value(false))
        .arg(Arg::with_name("shell")
            .short("s")
            .long("shell")
            .help("Run in shell mode to process multiple URLs")
            .takes_value(false))
        .get_matches();

    let auto_install = matches.is_present("install");

    if matches.is_present("shell") {
        run_shell_mode(auto_install)?;
    } else {
        let url = if let Some(url) = matches.value_of("url") {
            prepend_https_to_url(url)
        } else {
            // If no URL is provided, try to get it from the clipboard
            let mut clipboard = Clipboard::new()?;
            let clipboard_content = clipboard.get_text()?;
            prepend_https_to_url(&clipboard_content)
        };

        process_url(&url, auto_install)?;
    }

    Ok(())
}

fn run_shell_mode(auto_install: bool) -> Result<(), Box<dyn std::error::Error>> {
    println!("Entering shell mode. Paste URLs and press Enter. Type 'exit' to quit.");
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("TypeRip> ");
        stdout.flush()?;

        let mut input = String::new();
        stdin.lock().read_line(&mut input)?;
        let input = input.trim();

        if input.eq_ignore_ascii_case("exit") {
            println!("Exiting shell mode.");
            break;
        }

        let url = prepend_https_to_url(input);
        match process_url(&url, auto_install) {
            Ok(_) => println!("Processed URL successfully."),
            Err(e) => println!("Error processing URL: {}", e),
        }
    }

    Ok(())
}

fn process_url(url: &str, auto_install: bool) -> Result<(), Box<dyn std::error::Error>> {
    let url_type = get_url_type(&url);

    match url_type {
        URLTypes::FontFamily => get_font_family(&url, auto_install)?,
        URLTypes::FontCollection => get_font_collection(&url, auto_install)?,
        URLTypes::Invalid => println!("Invalid URL. Please provide a valid Adobe Fonts URL."),
    }

    Ok(())
}

fn prepend_https_to_url(url: &str) -> String {
    if !url.to_lowercase().starts_with("http://") && !url.to_lowercase().starts_with("https://") {
        format!("https://{}", url)
    } else {
        url.to_string()
    }
}

enum URLTypes {
    Invalid,
    FontFamily,
    FontCollection,
}

fn get_url_type(url: &str) -> URLTypes {
    if url.contains("fonts.adobe.com/collections") {
        URLTypes::FontCollection
    } else if url.contains("fonts.adobe.com/fonts") {
        URLTypes::FontFamily
    } else {
        URLTypes::Invalid
    }
}

fn get_font_family(url: &str, auto_install: bool) -> Result<(), Box<dyn std::error::Error>> {
    let response = ureq::get(url).call()?.into_string()?;
    let json_start = response.find("{\"family\":{\"slug\":\"").ok_or("Unexpected response format")?;
    let json_data = &response[json_start..];
    let json_end = json_data.find("</script>").ok_or("Unexpected response format")?;
    let json_data = &json_data[..json_end];

    let v: Value = serde_json::from_str(json_data)?;

    let family_name = v["family"]["name"].as_str().unwrap();
    println!("Font Family: {}", family_name);
    println!("Foundry: {}", v["family"]["foundry"]["name"]);
    println!("Designers:");
    for designer in v["family"]["designers"].as_array().unwrap() {
        println!("- {}", designer["name"]);
    }
    println!("Fonts:");

    let family_dir = Path::new("fonts").join(family_name);
    create_dir_all(&family_dir)?;

    for font in v["family"]["fonts"].as_array().unwrap() {
        println!("- {} ({})", font["name"], font["variation_name"]);
    
        let font_web_id = font["family"]["web_id"].as_str().unwrap();
        let font_variation_name = font["font"]["web"]["fvd"].as_str().unwrap();

        let font_url = format!(
            "https://use.typekit.net/pf/tk/{}/{}/l?unicode=AAAAAQAAAAEAAAAB&features=ALL&v=3&ec_token=3bb2a6e53c9684ffdc9a9bf71d5b2a620e68abb153386c46ebe547292f11a96176a59ec4f0c7aacfef2663c08018dc100eedf850c284fb72392ba910777487b32ba21c08cc8c33d00bda49e7e2cc90baff01835518dde43e2e8d5ebf7b76545fc2687ab10bc2b0911a141f3cf7f04f3cac438a135f",
            font_web_id,
            font_variation_name
        );

        let font_name = font["name"].as_str().unwrap();
        download_and_convert_font(&font_url, &family_dir, font_name, auto_install)?;
    }

    Ok(())
}

fn get_font_collection(url: &str, auto_install: bool) -> Result<(), Box<dyn std::error::Error>> {
    let response = ureq::get(url).call()?.into_string()?;
    let json_start = response.find("{\"fontpack\":{\"all_valid_slugs\":").ok_or("Unexpected response format")?;
    let json_data = &response[json_start..];
    let json_end = json_data.find("</script>").ok_or("Unexpected response format")?;
    let json_data = &json_data[..json_end];

    let v: Value = serde_json::from_str(json_data)?;

    let collection_name = v["fontpack"]["name"].as_str().unwrap();
    println!("Font Collection: {}", collection_name);
    println!("Curator: {}", v["fontpack"]["contributor_credit"]);
    println!("Fonts:");

    let collection_dir = Path::new("fonts").join(collection_name);
    create_dir_all(&collection_dir)?;

    for font in v["fontpack"]["font_variations"].as_array().unwrap() {
        println!("- {} ({})", font["full_display_name"], font["variation_name"]);
        let font_url = format!(
            "https://use.typekit.net/pf/tk/{}/{}/l?unicode=AAAAAQAAAAEAAAAB&features=ALL&v=3&ec_token=3bb2a6e53c9684ffdc9a9bf71d5b2a620e68abb153386c46ebe547292f11a96176a59ec4f0c7aacfef2663c08018dc100eedf850c284fb72392ba910777487b32ba21c08cc8c33d00bda49e7e2cc90baff01835518dde43e2e8d5ebf7b76545fc2687ab10bc2b0911a141f3cf7f04f3cac438a135f",
            font["opaque_id"],
            font["fvd"]
        );
        let font_name = font["full_display_name"].as_str().unwrap();
        download_and_convert_font(&font_url, &collection_dir, font_name, auto_install)?;
    }

    Ok(())
}

fn download_and_convert_font(url: &str, dir: &Path, name: &str, auto_install: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut response = ureq::get(url).call()?.into_reader();
    let mut woff2_data = Vec::new();
    response.read_to_end(&mut woff2_data)?;

    let woff2_filename = dir.join(format!("{}.woff2", name));
    let mut woff2_file = File::create(&woff2_filename)?;
    woff2_file.write_all(&woff2_data)?;
    println!("Downloaded: {}", woff2_filename.display());

    if is_woff2(&woff2_data) {
        let ttf_filename = dir.join(format!("{}.ttf", name));
        let mut cursor = Cursor::new(woff2_data);
        let ttf_data = convert_woff2_to_ttf(&mut cursor)?;
        let mut ttf_file = File::create(&ttf_filename)?;
        ttf_file.write_all(&ttf_data)?;
        println!("Converted to TTF: {}", ttf_filename.display());

        if auto_install && cfg!(target_os = "windows") {
            install_font_windows(&ttf_filename)?;
        }
    } else {
        println!("The downloaded file is not a valid WOFF2 font.");
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn get_fontregister_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let current_dir = env::current_dir()?;
    let fontregister_path = current_dir.join("fontregister.exe");

    if fontregister_path.exists() {
        Ok(fontregister_path)
    } else {
        Err("fontregister.exe not found in the current directory. Download from https://github.com/Nucs/FontRegister/releases/download/2.0/FontRegister.2.0.0-net48-x64.rar".into())
    }
}

#[cfg(target_os = "windows")]
fn install_font_windows(font_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let fontregister_path = get_fontregister_path()?;

    let output = Command::new(fontregister_path)
        .arg("install")
        .arg(font_path)
        .output()?;

    if output.status.success() {
        println!("Font installed successfully: {}", font_path.display());
        Ok(())
    } else {
        let error_message = String::from_utf8_lossy(&output.stderr);
        Err(format!("Failed to install font: {}", error_message).into())
    }
}

#[cfg(not(target_os = "windows"))]
fn install_font_windows(_font_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!("Auto-install is only supported on Windows.");
    Ok(())
}