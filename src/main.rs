use std::env;
use std::fs::File;
use std::io::Write;
use std::io::Cursor;
use ureq;
use serde_json::Value;
use woff2_patched::decode::{convert_woff2_to_ttf, is_woff2};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: typerip [url]");
        return Ok(());
    }

    let url = prepend_https_to_url(&args[1]);
    let url_type = get_url_type(&url);

    match url_type {
        URLTypes::FontFamily => get_font_family(&url)?,
        URLTypes::FontCollection => get_font_collection(&url)?,
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

fn get_font_family(url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let response = ureq::get(url).call()?.into_string()?;
    let json_start = response.find("{\"family\":{\"slug\":\"").ok_or("Unexpected response format")?;
    let json_data = &response[json_start..];
    let json_end = json_data.find("</script>").ok_or("Unexpected response format")?;
    let json_data = &json_data[..json_end];

    let v: Value = serde_json::from_str(json_data)?;

    println!("Font Family: {}", v["family"]["name"]);
    println!("Foundry: {}", v["family"]["foundry"]["name"]);
    println!("Designers:");
    for designer in v["family"]["designers"].as_array().unwrap() {
        println!("- {}", designer["name"]);
    }
    println!("Fonts:");
    for font in v["family"]["fonts"].as_array().unwrap() {
        println!("- {} ({})", font["name"], font["variation_name"]);
    
        // Extract web ID and variation name as strings
        let font_web_id = font["family"]["web_id"].as_str().unwrap();  // Extract web_id
        let font_variation_name = font["font"]["web"]["fvd"].as_str().unwrap();  // Extract fvd

        // Construct the URL similar to the JavaScript logic
        let font_url = format!(
            "https://use.typekit.net/pf/tk/{}/{}/l?unicode=AAAAAQAAAAEAAAAB&features=ALL&v=3&ec_token=3bb2a6e53c9684ffdc9a9bf71d5b2a620e68abb153386c46ebe547292f11a96176a59ec4f0c7aacfef2663c08018dc100eedf850c284fb72392ba910777487b32ba21c08cc8c33d00bda49e7e2cc90baff01835518dde43e2e8d5ebf7b76545fc2687ab10bc2b0911a141f3cf7f04f3cac438a135f",
            font_web_id,  // Use the correct web ID
            font_variation_name  // Use the correct variation name (fvd)
        );

        download_and_convert_font(&font_url, &font["name"].as_str().unwrap())?;
    }

    Ok(())
}

fn get_font_collection(url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let response = ureq::get(url).call()?.into_string()?;
    let json_start = response.find("{\"fontpack\":{\"all_valid_slugs\":").ok_or("Unexpected response format")?;
    let json_data = &response[json_start..];
    let json_end = json_data.find("</script>").ok_or("Unexpected response format")?;
    let json_data = &json_data[..json_end];

    let v: Value = serde_json::from_str(json_data)?;

    println!("Font Collection: {}", v["fontpack"]["name"]);
    println!("Curator: {}", v["fontpack"]["contributor_credit"]);
    println!("Fonts:");
    for font in v["fontpack"]["font_variations"].as_array().unwrap() {
        println!("- {} ({})", font["full_display_name"], font["variation_name"]);
        let font_url = format!(
            "https://use.typekit.net/pf/tk/{}/{}/l?unicode=AAAAAQAAAAEAAAAB&features=ALL&v=3&ec_token=3bb2a6e53c9684ffdc9a9bf71d5b2a620e68abb153386c46ebe547292f11a96176a59ec4f0c7aacfef2663c08018dc100eedf850c284fb72392ba910777487b32ba21c08cc8c33d00bda49e7e2cc90baff01835518dde43e2e8d5ebf7b76545fc2687ab10bc2b0911a141f3cf7f04f3cac438a135f",
            font["opaque_id"],
            font["fvd"]
        );
        download_and_convert_font(&font_url, &font["full_display_name"].as_str().unwrap())?;
    }

    Ok(())
}

fn download_and_convert_font(url: &str, name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut response = ureq::get(url).call()?.into_reader();
    let mut woff2_data = Vec::new();
    response.read_to_end(&mut woff2_data)?;

    let woff2_filename = format!("fonts/{}.woff2", name);
    let mut woff2_file = File::create(&woff2_filename)?;
    woff2_file.write_all(&woff2_data)?;
    println!("Downloaded: {}", woff2_filename);

    // Convert WOFF2 to TTF if it is a valid WOFF2 file
    if is_woff2(&woff2_data) {
        let ttf_filename = format!("fonts/{}.ttf", name);
        let mut cursor = Cursor::new(woff2_data);  // Wrap woff2_data in Cursor
        let ttf_data = convert_woff2_to_ttf(&mut cursor)?;  // Pass the cursor to convert_woff2_to_ttf
        let mut ttf_file = File::create(&ttf_filename)?;
        ttf_file.write_all(&ttf_data)?;
        println!("Converted to TTF: {}", ttf_filename);
    } else {
        println!("The downloaded file is not a valid WOFF2 font.");
    }

    Ok(())
}

