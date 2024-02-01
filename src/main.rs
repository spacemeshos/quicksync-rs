use chrono::Duration;
use clap::{Parser, Subcommand};
use std::error::Error;
use std::path::PathBuf;
use std::process;
use url::Url;

mod checksum;
mod download;
mod go_spacemesh;
mod parsers;
mod sql;
mod utils;
mod zip;

use checksum::*;
use download::download_with_retries;
use go_spacemesh::get_version;
use parsers::*;
use sql::get_last_layer_from_db;
use utils::*;
use zip::unpack;

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
    node_data: PathBuf,
    /// Genesis time in ISO format
    #[clap(short = 'g', long, default_value = "2023-07-14T08:00:00Z")]
    genesis_time: chrono::DateTime<chrono::Utc>,
    /// Layer duration
    #[clap(short = 'l', long, default_value = "5m", value_parser = parse_duration)]
    layer_duration: Duration,
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
      default_value = "https://quicksync.spacemesh.network/"
    )]
    download_url: Url,
    /// Maximum retries amount for downloading (or resuming download) if something went wrong
    #[clap(short = 'r', long, default_value = "5")]
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  let cli = Cli::parse();

  match cli.command {
    Commands::Check {
      node_data,
      genesis_time,
      layer_duration,
    } => {
      let dir_path = node_data.clone();
      let db_file_path = dir_path.join("state.sql");
      let db_file_str = db_file_path.to_str().expect("Cannot compose path");
      println!("Checking database: {}", &db_file_str);
      let db_layer = i64::from(get_last_layer_from_db(db_file_str)?);
      let time_layer = calculate_latest_layer(genesis_time, layer_duration)?;
      println!("Latest layer in db: {}", db_layer);
      println!("Latest calculated layer: {}", time_layer);
      if time_layer - db_layer > 100 {
        println!("Too far behind");
      } else {
        println!("OK!");
      }
      Ok(())
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
      let archive_file_path = dir_path.join("state.zip");
      let unpacked_file_path = dir_path.join("state_downloaded.sql");
      let final_file_path = dir_path.join("state.sql");

      // Download archive if needed
      if !archive_file_path.exists() {
        println!("Downloading the latest database...");
        let url = if redirect_file_path.exists() {
          std::fs::read_to_string(&redirect_file_path)?
        } else {
          let go_path = resolve_path(&go_spacemesh_path).unwrap();
          let go_path_str = go_path
            .to_str()
            .expect("Cannot resolve path to go-spacemesh");
          let path = format!("{}/state.zip", &get_version(&go_path_str)?);
          let url = build_url(&download_url, &path)?;
          url.to_string()
        };

        if let Err(e) =
          download_with_retries(&url, &temp_file_path, &redirect_file_path, max_retries).await
        {
          eprintln!(
            "Failed to download a file after {} attempts: {}",
            max_retries, e
          );
          process::exit(1);
        }

        // Rename `state.download` -> `state.zip`
        std::fs::rename(&temp_file_path, &archive_file_path)?;
        println!("Archive downloaded!");
      }

      // Unzip
      match unpack(&archive_file_path, &unpacked_file_path).await {
        Ok(_) => {
          println!("Archive unpacked successfully");
        }
        Err(e) if e.raw_os_error() == Some(28) => {
          println!("Cannot unpack archive: not enough disk space");
          std::fs::remove_file(&unpacked_file_path)?;
          process::exit(2);
        }
        Err(e) => {
          println!("Cannot unpack archive: {}", e);
          std::fs::remove_file(&unpacked_file_path)?;
          std::fs::remove_file(&archive_file_path)?;
          process::exit(3);
        }
      }

      // Verify checksum
      println!("Verifying MD5 checksum...");
      match verify(&redirect_file_path, &unpacked_file_path).await {
        Ok(true) => {
          println!("Checksum is valid");
        }
        Ok(false) => {
          println!("MD5 checksums are not equal. Deleting archive and unpacked state.sql");
          std::fs::remove_file(&unpacked_file_path)?;
          std::fs::remove_file(&archive_file_path)?;
          process::exit(4);
        }
        Err(e) => {
          println!("Cannot verify checksum: {}", e.to_string());
          process::exit(5);
        }
      }

      if final_file_path.exists() {
        println!("Backing up current state.sql file");
        match backup_file(&final_file_path) {
          Ok(b) => {
            let backup_name = b.to_str().expect("Cannot get a path of backed up file");
            println!("File backed up to: {}", backup_name);
          }
          Err(e) => {
            println!("Cannot create a backup file: {}", e.to_string())
          }
        }
      }
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
