# quicksync-rs

When a new node joins the Spacemesh network, it must first get up to speed with the rest of its peers. This process is referred to as "syncing" and is a pre-requisite to running a full or a smeshing node. Historically, it has been difficult for smeshers to successfully sync their nodes owing to how time-consuming the syncing process is. Primarily, syncing includes downloading and independently verifying all blocks, transactions, ATXs, along with some other data, and reconstructing the current state from scratch. Naturally, this took the average smesher a lot of time. As such, in response to the growing difficulty of syncing a fresh node, we have prepared a way to speed up the syncing process. Introducing, Quicksync.

With Quicksync, instead of performing all of the syncing actions as stated above and calculating the network state from genesis, one just needs to download the current state from a trusted peer like the Spacemesh dev team or some other node. While this runs contrary to the web3 philosophy of "Don't trust, verify", we believe that this could be a choice some smeshers may be interested in given the high rate of trouble with syncing. Moreover, nothing precludes a smesher from verifying this state in the background once it is downloaded.

The state (also called an archive) that is downloaded is in the form of a state.sql file and can either be downloaded automatically using Smapp, or manually by using the `quicksync-rs` utility.

Instructions for using `quicksync-rs` to download the latest state are given below. Note that if you use the latest version of Smapp, it will automatically offer to use quicksync to fetch the latest state.

## Windows

1. Download the latest release of `quicksync-windows-vX.X.X.zip` from the GitHub releases section.
2. Extract `quicksync.exe` from the zip file downloaded in step 1.
3. Move `quicksync.exe` to your `spacemesh` folder. By default, this folder is located at: `C:\Users\{USERNAME}\spacemesh`.
4. If you see a `state.sql` file in your node data folder (located inside the `spacemesh` directory and named `node-data` by default), delete it. Otherwise, continue to step 5.
5. Open a Windows Powershell terminal in the `spacemesh` directory where the `quicksync.exe` file is. You can do this by holding the "shift" key, right-clicking, and selecting the "Open Powershell here" option.
6. Inside the Powershell, type `.\quicksync.exe --help` and press enter. This will show you the available options.
7. We want to download the state database. Type `.\quicksync.exe download --node-data .\node-data`. Here, `.\node-data` is the path to the node data folder.
8. Wait for the process to complete. The `quicksync-rs` utility will download, unzip, and verify the downloaded state.
9. Your node data folder should now have the latest `state.sql` file.

## Linux

1. Download the latest release of `quicksync-linux-vX.X.X.zip` from the GitHub releases section.
2. Extract the `quicksync` file from the zip file downloaded in step 1.
3. Make the `quicksync` file executable by using this CLI command: `chmod +x quicksync`. Now you have the `quicksync` executable.
4. Move the `quicksync` executable to the `spacemesh` directory ( located at `~/spacemesh` by default).
5. If you see a `state.sql` file in your node data folder (located inside the `spacemesh` directory and named `node-data` by default), delete it. Otherwise, continue to step 6.
6. Open a terminal in the `spacemesh` directory where the `quicksync` executable is, and run this command: `./quicksync download --node-data ./node-data`. Here, `./node-data` is the path to the node data folder.
7. Wait for the process to complete. The `quicksync-rs` utility will download, unzip, and verify the downloaded state.
8. Your node data folder should now have the latest `state.sql` file.

## MacOS

1. Download the latest release of `quicksync-macos-vX.X.X.zip` (or `quicksync-macos-arm64-vX.X.X.zip` if you have an M-series Mac) from the GitHub releases section.
2. Extract the `quicksync` file from the zip file downloaded in step 1.
3. Make the `quicksync` file executable by using this CLI command: `chmod +x quicksync`. Now you have the `quicksync` executable.
4. Move the `quicksync` executable to the `spacemesh` directory. (located at `~/spacemesh` by default).
5. If you see a `state.sql` file in your node data folder (located inside the `spacemesh` directory and named `node-data` by default), delete it. Otherwise, continue to step 6.
6. Open a terminal in the `spacemesh` directory where the `quicksync` executable is, and run this command: `./quicksync download --node-data ./node-data`. Here, `./node-data` is the path to the node data folder.
7. Wait for the process to complete. The `quicksync-rs` utility will download, unzip, and verify the downloaded state.
8. Your node data folder should now have the latest `state.sql` file.

## Exit Codes

Listed below are the exit codes and what they mean:

- `0` - All good.
- `1` - Failed to download archive within max retries (any reason).
- `2` - Cannot unpack archive: not enough disk space.
- `3` - Cannot unpack archive: any other reason.
- `4` - Invalid checksum of downloaded `state.sql`.
- `5` - Cannot verify checksum for some reason.
- `6` - Cannot create a backup file.
- `7` - Invalid checksum of archive.
- `8` - Cannot validate archive checksum.


# Incremental quicksync

It is also possible to download and apply delta-based quicksync. Assuming that the `state.sql` is already present, it's worth considering applying only deltas on top of that.
Please note that syncing large portions will be faster with full quicksync, but if you are already synced and just need to catch up with the latest state, incrementa quicksync is the way to go.

Incremental quicksync works by checking the latest verified layer in the database and then downloading small files (usually about 50MB but up to 200MB) and applying them on top of the existing `state.sql`. Each batch can be interrupted.

Restoring the same batch twice is considered a no-op and will not affect the database.

## Commands

The list of available commands for the `quicksync` utility is presented below. Note that these commands are for Linux. Simply, Change `./quicksync` to `.\quicksync.exe` For the Windows commands.

- `./quicksync download`: Downloads the latest `state.sql` file.
- `./quicksync check`: Checks if the current `state.sql` is up to date.
- `./quicksync help`: Displays all operations that `quicksync` can perform.
- `./quicksync incremental`: Allows to work with delta based quicksync.
- `./quicksync --version`: Displays the quicksync version.
- `cargo run -- help`: Displays helpful commands for running the package. Relevant for developers.
