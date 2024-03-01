use chrono::Duration;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process;
use url::Url;

mod checksum;
mod download;
mod go_spacemesh;
mod parsers;
mod reader_with_bytes;
mod reader_with_progress;
mod sql;
mod unpack;
mod utils;

use checksum::*;
use download::download_with_retries;
use go_spacemesh::get_version;
use parsers::*;
use sql::get_last_layer_from_db;
use unpack::{unpack_zip, unpack_zstd};
use utils::*;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Cli {
  #[clap(subcommand)]
  command: Commands,
}

const DEFAULT_DOWNLOAD_URL: &str = "https://quicksync.spacemesh.network/";

#[derive(Subcommand, Debug)]
enum Commands {
  /// Checks if quicksync is recommended
  Check {
    /// Path to the node-data directory
    #[clap(short = 'd', long)]
    node_data: PathBuf,
    /// Genesis time in ISO format
    #[clap(short = 't', long, default_value = "2023-07-14T08:00:00Z")]
    genesis_time: chrono::DateTime<chrono::Utc>,
    /// Layer duration
    #[clap(short = 'l', long, default_value = "5m", value_parser = parse_duration)]
    layer_duration: Duration,
    /// Path to go-spacemesh binary
    #[clap(short = 'g', long, default_value = go_spacemesh_default_path())]
    go_spacemesh_path: PathBuf,
    /// URL to download database from. Node version will be appended at the end
    #[clap(
      short = 'u',
      long,
      default_value = DEFAULT_DOWNLOAD_URL
    )]
    download_url: Url,
  },
  /// Downloads latest db from official website
  Download {
    /// Path to the node-data directory
    #[clap(short = 'd', long)]
    node_data: PathBuf,
    /// Path to go-spacemesh binary
    #[clap(short = 'g', long, default_value = go_spacemesh_default_path())]
    go_spacemesh_path: PathBuf,
    /// URL to download database from. Node version will be appended at the end
    #[clap(
      short = 'u',
      long,
      default_value = DEFAULT_DOWNLOAD_URL
    )]
    download_url: Url,
    /// Maximum retries amount for downloading (or resuming download) if something went wrong
    #[clap(short = 'r', long, default_value = "10")]
    max_retries: u32,
  },
}

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

fn backup_or_fail(file_path: &PathBuf) -> () {
  if file_path.exists() {
    println!(
      "Backing up file: {}",
      file_path.file_name().unwrap().to_str().unwrap()
    );
    match backup_file(&file_path) {
      Ok(b) => {
        let backup_name = b.to_str().expect("Cannot get a path of backed up file");
        println!("File backed up to: {}", backup_name);
      }
      Err(e) => {
        eprintln!("Cannot create a backup file: {}", e);
        process::exit(6);
      }
    }
  }
}

fn main() -> anyhow::Result<()> {
  let cli = Cli::parse();

  match cli.command {
    Commands::Check {
      node_data,
      genesis_time,
      layer_duration,
      go_spacemesh_path,
      download_url,
    } => {
      let result = {
        let dir_path = node_data.clone();
        let db_file_path = dir_path.join("state.sql");
        let db_file_str = db_file_path.to_str().expect("Cannot compose path");
        println!("Checking database: {}", db_file_str);
        let db_layer = if db_file_path.exists() {
          i64::from(get_last_layer_from_db(&db_file_path).or_else(|err| {
            eprintln!("{}", err);
            println!("Cannot read database, trating it as empty database");
            return Ok::<i32, anyhow::Error>(0);
          })?)
        } else {
          println!("Database file is not found");
          0
        };
        println!("Latest layer in db: {}", db_layer);

        let time_layer = calculate_latest_layer(genesis_time, layer_duration)?;
        println!("Current network layer: {}", time_layer);

        let go_path = resolve_path(&go_spacemesh_path).unwrap();
        let go_path_str = go_path
          .to_str()
          .expect("Cannot resolve path to go-spacemesh");
        let go_version = get_version(&go_path_str)?;
        let quicksync_layer = fetch_latest_available_layer(&download_url, &go_version)?;
        println!("Latest layer in cloud: {}", quicksync_layer);
        Ok(())
      };
      if result.is_err() {
        process::exit(1);
      }
      result
    }
    Commands::Download {
      node_data,
      go_spacemesh_path,
      download_url,
      max_retries,
    } => {
      let dir_path = node_data;
      let temp_file_path = dir_path.join("state.download");
      let redirect_file_path = dir_path.join("state.url");
      let archive_zip_file_path = dir_path.join("state.zip");
      let archive_zstd_file_path = dir_path.join("state.zst");
      let unpacked_file_path = dir_path.join("state_downloaded.sql");
      let final_file_path = dir_path.join("state.sql");
      let wal_file_path = dir_path.join("state.sql-wal");

      // Download archive if needed
      let archive_file_path = if !archive_zip_file_path.exists() && !archive_zstd_file_path.exists()
      {
        println!("Downloading the latest database...");
        let url = if redirect_file_path.exists() {
          std::fs::read_to_string(&redirect_file_path)?
        } else {
          let go_path = resolve_path(&go_spacemesh_path).unwrap();
          let go_path_str = go_path
            .to_str()
            .expect("Cannot resolve path to go-spacemesh");
          let path = format!("{}/state.zst", &get_version(go_path_str)?);
          let url = build_url(&download_url, &path);
          url.to_string()
        };

        if let Err(e) =
          download_with_retries(&url, &temp_file_path, &redirect_file_path, max_retries)
        {
          eprintln!(
            "Failed to download a file after {} attempts: {}",
            max_retries, e
          );
          process::exit(1);
        }

        let archive_file_path = if url.ends_with(".zip") {
          archive_zip_file_path
        } else {
          archive_zstd_file_path
        };

        // Rename `state.download` -> `state.zst`
        std::fs::rename(&temp_file_path, &archive_file_path)?;
        println!("Archive downloaded!");
        archive_file_path
      } else if archive_zip_file_path.exists() {
        archive_zip_file_path
      } else {
        archive_zstd_file_path
      };

      let archive_url = std::fs::read_to_string(&redirect_file_path)?;
      let unpack = if archive_url.ends_with(".zip") {
        unpack_zip
      } else {
        unpack_zstd
      };

      // Verify downloaded archive
      match verify_archive(&redirect_file_path, &archive_file_path) {
        Ok(true) => {
          println!("Archive checksm validated");
        }
        Ok(false) => {
          eprintln!("Archive checksum is invalid. Deleting archive");
          std::fs::remove_file(&archive_file_path)?;
          process::exit(7);
        }
        Err(e) => {
          eprintln!("Cannot validate archive checksum: {}", e);
          process::exit(8);
        }
      }

      // Unzip
      match unpack(&archive_file_path, &unpacked_file_path) {
        Ok(_) => {
          println!("Archive unpacked successfully");
        }
        Err(e) => {
          if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
            if io_err.raw_os_error() == Some(28) {
              eprintln!("Cannot unpack archive: not enough disk space");
              std::fs::remove_file(&unpacked_file_path)?;
              process::exit(2);
            }
          }
          eprintln!("Cannot unpack archive: {}", e);
          std::fs::remove_file(&unpacked_file_path)?;
          process::exit(3);
        }
      }

      // Verify checksum
      println!("Verifying MD5 checksum...");
      match verify_db(&redirect_file_path, &unpacked_file_path) {
        Ok(true) => {
          println!("Checksum is valid");
        }
        Ok(false) => {
          eprintln!("MD5 checksums are not equal. Deleting archive and unpacked state.sql");
          std::fs::remove_file(&unpacked_file_path)?;
          std::fs::remove_file(&archive_file_path)?;
          std::fs::remove_file(&redirect_file_path)?;
          process::exit(4);
        }
        Err(e) => {
          eprintln!("Cannot verify checksum: {}", e);
          process::exit(5);
        }
      }

      backup_or_fail(&final_file_path);
      backup_or_fail(&wal_file_path);

      std::fs::rename(&unpacked_file_path, &final_file_path)
        .expect("Cannot rename downloaded file into state.sql");

      std::fs::remove_file(&redirect_file_path)?;
      std::fs::remove_file(&archive_file_path)?;

      println!("Done!");
      println!("Now you can run go-spacemesh as usually.");

      Ok(())
    }
  }
}
