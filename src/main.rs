use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::error::Error;
use std::process;

mod utils;
mod checksum;
mod download;
mod sql;
mod go_spacemesh;
mod zip;

use utils::*;
use checksum::*;
use download::download_with_retries;
use sql::get_last_layer_from_db;
use go_spacemesh::get_version;
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
    let max_retries = 5;

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
            let unpacked_file_path = dir_path.join("state_downloaded.sql");
            let final_file_path = dir_path.join("state.sql");
            let backup_file_path = dir_path.join("state.sql.bak");

            if !archive_file_path.exists() {
                println!("Downloading the latest database...");
                let url = if redirect_file_path.exists() {
                    std::fs::read_to_string(&redirect_file_path)?
                } else {
                    let go_path = resolve_path(&go_spacemesh_path).unwrap();
                    let go_path_str = go_path.to_str().expect("Cannot resolve path to go-spacemesh");
                    let path = format!("{}/state.zip", &get_version(&go_path_str)?);
                    let url = build_url(&download_url, &path)?;
                    url.to_string()
                };

                if let Err(e) = download_with_retries(&url, &temp_file_path, &redirect_file_path, max_retries).await {
                    eprintln!("Failed to download a file after {} attempts: {}", max_retries, e);
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
                },
                Err(b) => {
                    let dyn_err = b.as_ref();
                    let e = dyn_err.downcast_ref::<std::io::Error>().expect("Cannot read Error message");
                    if e.raw_os_error() == Some(28) {
                        println!("Cannot unpack archive: not enough disk space");
                        std::fs::remove_file(&unpacked_file_path)?;
                        process::exit(2);
                    } else {
                        println!("Cannot unpack archive: {}", e);
                        std::fs::remove_file(&unpacked_file_path)?;
                        std::fs::remove_file(&archive_file_path)?;
                        process::exit(3);
                    }
                }
            }

            println!("Checking MD5 checksum...");
            let archive_url = String::from_utf8(
                std::fs::read(&redirect_file_path).expect("Cannot read state.url")
            )?;
            let md5_expected = download_checksum(&archive_url).await.expect("Cannot download md5");
            let md5_actual = calculate_checksum(&unpacked_file_path).expect("Cannot calculate md5");

            if md5_actual != md5_expected {
                println!("MD5 checksums are not equal. Deleting archive and unpacked state.sql");
                std::fs::remove_file(&unpacked_file_path)?;
                std::fs::remove_file(&archive_file_path)?;
                process::exit(4);
            }

            if final_file_path.exists() {
                println!("Renaming current state.sql file into state.sql.bak");
                // Rename original State.Sql (backup)
                std::fs::rename(&final_file_path, &backup_file_path).expect("Cannot rename state.sql -> state.sql.bak");
            }
            std::fs::rename(&unpacked_file_path, &final_file_path).expect("Cannot rename downloaded file into state.sql");

            std::fs::remove_file(&redirect_file_path)?;
            std::fs::remove_file(&archive_file_path)?;

            println!("Done!");
            println!("Now you can run go-spacemesh as usually.");

            Ok(())
        }
    }
}
