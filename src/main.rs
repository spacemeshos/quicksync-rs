use chrono::{DateTime, Utc, Duration};
use clap::{Parser, Subcommand};
use duration_string::DurationString;
use reqwest::{Client, header};
use rusqlite::{Connection, params};
use zip::ZipArchive;
use std::env;
use std::fs::{OpenOptions, create_dir_all, File};
use std::io::{Seek, SeekFrom, Write, Read};
use std::process::Command;
use std::path::{PathBuf, Path};
use std::error::Error;
use std::time::Instant;
use futures_util::StreamExt;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Checks if quicksync is recommended
    Check {
        /// Path to the node-data directory
        #[clap(short = 'd', long)]
        node_data: String,
        /// Genesis time in ISO format
        #[clap(short = 'g', long, default_value = "2023-07-14T08:00:00Z")]
        genesis_time: String,
        /// Layer duration
        #[clap(short = 'l', long, default_value = "5m")]
        layer_duration: String,
    },
    /// Downloads latest db from official website
    Download {
        /// Path to the node-data directory
        #[clap(short = 'd', long)]
        node_data: String,
        /// Path to go-spacemesh binary
        #[clap(short = 'p', long, default_value = go_spacemesh_default_path())]
        go_spacemesh_path: String,
        /// URL to download database from. Node version will be appended at the end
        #[clap(short = 'u', long, default_value = "https://quicksync.spacemesh.network/")]
        download_url: String,
    },
}

// Функция для определения пути по умолчанию в зависимости от ОС
fn go_spacemesh_default_path() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "./go-spacemesh.exe"
    }
    #[cfg(not(target_os = "windows"))]
    {
        "./go-spacemesh"
    }
}

fn parse_iso_date(iso_date: &str) -> Result<DateTime<Utc>, chrono::ParseError> {
    iso_date.parse::<DateTime<Utc>>()
}

async fn download_file(url: &str, file_path: &str, redirect_path: &str) -> Result<(), Box<dyn Error>> {
    let path = Path::new(file_path);

    if let Some(dir) = path.parent() {
        create_dir_all(dir).expect("Cannot create directory");
    }

    let mut file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(file_path)
        .expect("Cannot create file");

    let file_size = file.metadata()?.len();

    let client = Client::new();
    let response = client.get(url)
        .header("Range", format!("bytes={}-", file_size))
        .send()
        .await?;
    let final_url = response.url().clone();

    std::fs::write(redirect_path, final_url.as_str())?;

    if response.status().is_success() {
        let total_size = response
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|ct_len| ct_len.to_str().ok())
            .and_then(|ct_len| ct_len.parse::<u64>().ok())
            .unwrap_or(0) + file_size;

        file.seek(SeekFrom::End(0))?;
        let mut stream = response.bytes_stream();
        let start = Instant::now();

        let mut downloaded: u64 = file_size;
        let mut last_reported_progress: i64 = -1;

        while let Some(item) = stream.next().await {
            let chunk = item?;
            file.write_all(&chunk)?;
            downloaded += chunk.len() as u64;

            let elapsed = start.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                downloaded as f64 / elapsed
            } else {
                0.0
            };
            let eta = if speed > 0.0 {
                (total_size as f64 - downloaded as f64) / speed
            } else {
                0.0
            };

            let progress = (downloaded as f64 / total_size as f64 * 100.0).round() as i64;
            if progress > last_reported_progress {
                println!("Downloading... {:.2}% ({:.2} MB/{:.2} MB) ETA: {:.0} sec",
                    progress,
                    downloaded as f64 / 1_024_000.0,
                    total_size as f64 / 1_024_000.0,
                    eta);
                last_reported_progress = progress;
            }
        }
    } else {
        println!("Cannot resume downloading: {:?}", response.status());
    }

    Ok(())
}

fn trim_version(version: &str) -> &str {
    version.split('+').next().unwrap_or(version)
}

fn get_go_spacemesh_version(path: &str) -> Result<String, Box<dyn Error>> {
    let output = Command::new(path)
        .arg("version")
        .output()
        .expect("Cannot run go-spacemesh version");

    let version = String::from_utf8(output.stdout)?;
    let trimmed = trim_version(version.trim()).to_string();

    Ok(trimmed)
}

fn resolve_path(relative_path: &str) -> Result<PathBuf, Box<dyn Error>> {
    let current_dir = env::current_dir()?;
    let resolved_path = current_dir.join(relative_path);
    Ok(resolved_path)
}

fn get_last_layer_from_db(db_path: &str) -> Result<i32, Box<dyn Error>> {
    let conn = Connection::open(db_path)?;

    let mut stmt = conn.prepare("SELECT * FROM layers ORDER BY id DESC LIMIT 1")?;
    let mut layer_iter = stmt.query_map(params![], |row| {
        Ok(row.get::<_, i32>(0)?)
    })?;

    if let Some(result) = layer_iter.next() {
        let last_id = result?;
        Ok(last_id)
    } else {
        Ok(0)
    }
}

fn calculate_latest_layer(genesis_time: String, layer_duration: String) -> Result<i64, Box<dyn Error>> {
    let genesis = parse_iso_date(&genesis_time)?;
    let delta = Utc::now() - genesis;
    let dur = Duration::from_std(DurationString::from_string(layer_duration)?.into())?;
    Ok(delta.num_milliseconds() / dur.num_milliseconds())
}

fn unzip_state_sql(archive_path: &str, output_path: &str) -> Result<(), Box<dyn Error>> {
    let file = File::open(archive_path)?;
    let mut zip = ZipArchive::new(file)?;

    let mut state_sql = zip.by_name("state.sql")
        .expect("State.sql file not found in archive");
    let outpath = Path::new(output_path);

    if let Some(p) = outpath.parent() {
        std::fs::create_dir_all(&p)?;
    }
    let mut outfile = File::create(&outpath)?;

    let total_size = state_sql.size();
    let mut extracted_size: u64 = 0;
    let mut buffer = [0; 4096];

    let mut last_reported_progress: i64 = -1;

    while let Ok(bytes_read) = state_sql.read(&mut buffer) {
        if bytes_read == 0 {
            break;
        }
        outfile.write_all(&buffer[..bytes_read])?;
        extracted_size += bytes_read as u64;

        let progress = (extracted_size as f64 / total_size as f64 * 100.0).round() as i64;
        if last_reported_progress != progress {
            last_reported_progress = progress;
            println!("Unzipping... {}%", progress);
        }
    }
    if last_reported_progress < 100 {
        // Ensure that 100% will be printed
        println!("Unzipping... 100%");
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Check { node_data, genesis_time, layer_duration } => {
            let dir_path = PathBuf::from(node_data.clone());
            let db_file_path = dir_path.join("state.sql");
            let db_file_str = db_file_path.to_str().expect("Cannot compose path");
            println!("Checking database: {}", &db_file_str);
            let db_layer = i64::from(get_last_layer_from_db(db_file_str)?);
            let time_layer = calculate_latest_layer(genesis_time, layer_duration)?;
            println!("Latest layer in db: {}", db_layer);
            println!("Latest calculated layer: {}", time_layer);
            if db_layer - time_layer > 100 {
                println!("Too far behind");
            } else {
                println!("OK!");
            }
            Ok(())
        }
        Commands::Download { node_data, go_spacemesh_path, download_url } => {
            let dir_path = PathBuf::from(node_data);
            let temp_file_path = dir_path.join("state.download");
            let redirect_file_path = dir_path.join("state.url");
            let archive_file_path = dir_path.join("state.zip");
            let final_file_path = dir_path.join("state.sql");
            let backup_file_path = dir_path.join("state.sql.bak");

            let temp_file_str = temp_file_path.to_str().expect("Cannot compose path");
            let redirect_file_str = redirect_file_path.to_str().expect("Cannot compose path");
            let archive_file_str = archive_file_path.to_str().expect("Cannot compose path");
            let final_file_str = final_file_path.to_str().expect("Cannot compose path");
            let backup_file_str = backup_file_path.to_str().expect("Cannot compose path");

            if !archive_file_path.exists() {
                println!("Downloading the latest database...");
                let url = if redirect_file_path.exists() {
                    std::fs::read_to_string(redirect_file_str)?
                } else {
                    let go_path = resolve_path(&go_spacemesh_path).unwrap();
                    let go_path_str = go_path.to_str().expect("Cannot resolve path to go-spacemesh");
                    format!("{}{}", &download_url, get_go_spacemesh_version(&go_path_str)?)
                };

                download_file(&url, temp_file_str, redirect_file_str).await?;

                // Rename `state.download` -> `state.zip`
                std::fs::rename(temp_file_str, archive_file_str)?;
                println!("Archive downloaded!");
            } else {
                println!("Archive found...");
            }

            if final_file_path.exists() {
                println!("Renaming current state.sql file into state.sql.bak");
                // Rename original State.Sql (backup)
                std::fs::rename(final_file_str, backup_file_str)?;
            }
            
            // Unzip
            unzip_state_sql(archive_file_str, final_file_str)
                .expect("Cannot unzip archive");

            // TODO: Download the checksum and validate (e.g. http://localhost:8080/abcdef.checksum)

            std::fs::remove_file(redirect_file_str)?;
            std::fs::remove_file(archive_file_str)?;

            println!("Done!");
            println!("Now you can run go-spacemesh as usually.");

            Ok(())
        }
    }
}
