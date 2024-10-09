use chrono::Duration;
use clap::{Parser, Subcommand};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::process;
use std::{env, path::PathBuf};
use url::Url;

mod checksum;
mod download;
mod eta;
mod go_spacemesh;
mod parsers;
mod partial_quicksync;
mod read_error_response;
mod reader_with_bytes;
mod sql;
mod unpack;
mod user_agent;
mod utils;

use anyhow::{anyhow, Context};
use checksum::*;
use download::download_with_retries;
use go_spacemesh::get_version;
use parsers::*;
use partial_quicksync::partial_restore;
use sql::get_last_layer_from_db;
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
  /// Uses partial recovery quicksync method
  Partial {
    /// Path to the node state.sql
    #[clap(short = 's', long)]
    state_sql: PathBuf,
    /// Number of layers present in the DB that are not trusted to be fully synced.
    /// These layers will also be synced.
    #[clap(long, default_value_t = 10)]
    untrusted_layers: u32,
    /// Jump-back to recover earlier than latest layer. It will jump back one row in recovery metadata
    #[clap(short = 'j', long, default_value_t = 0)]
    jump_back: usize,
    /// URL to download parts from
    #[clap(short = 'u', long, default_value = partial_quicksync::DEFAULT_BASE_URL)]
    base_url: String,
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

fn backup_or_fail(file_path: PathBuf) {
  match file_path.try_exists() {
    Ok(true) => {
      println!(
        "Backing up file: {}",
        file_path.file_name().unwrap().to_str().unwrap()
      );
      match backup_file(&file_path) {
        Ok(b) => {
          let backup_name = b.to_string_lossy();
          println!("File backed up to: {}", backup_name);
        }
        Err(e) => {
          eprintln!("Cannot create a backup file: {}", e);
          process::exit(6);
        }
      }
    }
    Ok(false) => {
      println!(
        "Skip backup: file {} not found",
        file_path.to_string_lossy()
      );
    }
    Err(e) => {
      eprintln!("Cannot create a backup file: {}", e);
      process::exit(6);
    }
  }
}

fn resolve_path(relative_path: &Path) -> anyhow::Result<PathBuf> {
  let current_dir = env::current_dir()?;
  Ok(current_dir.join(relative_path))
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
        let db_layer = if db_file_path.try_exists().unwrap_or(false) {
          i64::from(get_last_layer_from_db(&db_file_path).or_else(|err| {
            eprintln!("{}", err);
            println!("Cannot read database, trating it as empty database");
            Ok::<i32, anyhow::Error>(0)
          })?)
        } else {
          println!("Database file is not found");
          0
        };
        println!("Latest layer in db: {}", db_layer);

        let time_layer = calculate_latest_layer(genesis_time, layer_duration)?;
        println!("Current network layer: {}", time_layer);

        let go_path = resolve_path(&go_spacemesh_path).unwrap();
        let go_version = get_version(&go_path)?;
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
      mut download_url,
      max_retries,
    } => {
      let dir_path = node_data;
      let redirect_file_path = dir_path.join("state.url");
      let archive_file_path = dir_path.join("state.zst");
      let unpacked_file_path = dir_path.join("state_downloaded.sql");
      let final_file_path = dir_path.join("state.sql");
      let wal_file_path = dir_path.join("state.sql-wal");

      // Download archive if needed
      if !archive_file_path.try_exists().unwrap_or(false) {
        println!("Downloading the latest database...");
        let url = if redirect_file_path.try_exists().unwrap_or(false) {
          std::fs::read_to_string(&redirect_file_path)?
        } else {
          let go_path = resolve_path(&go_spacemesh_path).context("checking node version")?;
          let version = get_version(&go_path)?;
          download_url
            .path_segments_mut()
            .map_err(|e| anyhow::anyhow!("parsing download url: {e:?}"))?
            .extend(&[&version, "state.zst"]);
          download_url.to_string()
        };

        let temp_file_path = dir_path.join("state.download");
        if let Some(dir) = temp_file_path.parent() {
          std::fs::create_dir_all(dir)?;
        }

        let mut file = OpenOptions::new()
          .create(true)
          .read(true)
          .append(true)
          .open(&temp_file_path)
          .with_context(|| format!("creating temp file: {}", temp_file_path.display()))?;

        if let Err(e) = download_with_retries(
          &url,
          &mut file,
          &redirect_file_path,
          max_retries,
          std::time::Duration::from_secs(5),
        ) {
          eprintln!("Failed to download a file after {max_retries} attempts: {e}",);
          file.flush()?;
          process::exit(1);
        }
        drop(file);

        // Rename `state.download` -> `state.zst`
        std::fs::rename(&temp_file_path, &archive_file_path)?;
        println!("Archive downloaded!");
      }

      if redirect_file_path.try_exists().unwrap_or(false) {
        println!("Verifying the checksum, it may take some time...");
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
      } else {
        println!("Download URL is not found: skip archive checksum verification");
      }

      match unpack::unpack(&archive_file_path, &unpacked_file_path) {
        Ok(_) => {
          println!("Archive unpacked successfully");
        }
        Err(e) => {
          if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
            // FIXME: use ErrorKind::StorageFull once it's stabilized (https://github.com/rust-lang/rust/issues/86442)
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
      if redirect_file_path.try_exists().unwrap_or(false) {
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
      } else {
        println!("Download URL is not found: skip DB checksum verification");
      }

      backup_or_fail(final_file_path.clone());
      backup_or_fail(wal_file_path);

      std::fs::rename(&unpacked_file_path, &final_file_path)
        .expect("Cannot rename downloaded file into state.sql");

      if archive_file_path.try_exists().unwrap_or(false) {
        println!("Archive file is deleted.");
        std::fs::remove_file(&archive_file_path)?;
      }
      if redirect_file_path.try_exists().unwrap_or(false) {
        println!("URL file is deleted.");
        std::fs::remove_file(&redirect_file_path)?;
      }

      println!("Done!");
      println!("Now you can run go-spacemesh as usually.");

      Ok(())
    }
    Commands::Partial {
      state_sql,
      untrusted_layers,
      jump_back,
      base_url,
    } => {
      println!("Partial quicksync is considered to be beta feature for now");
      let state_sql_path = resolve_path(&state_sql).context("resolving state.sql path")?;
      if !state_sql_path
        .try_exists()
        .context("checking if state file exists")?
      {
        return Err(anyhow!("state file not found: {:?}", state_sql_path));
      }
      let download_path = resolve_path(Path::new(".")).unwrap();
      partial_restore(
        &base_url,
        &state_sql_path,
        &download_path,
        untrusted_layers,
        jump_back,
      )
    }
  }
}
