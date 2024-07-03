# quicksync-rs

When a new node joins the Spacemesh network, it must first get up to speed with the rest of its peers. This process is referred to as "syncing" and is a pre-requisite to running a full or a smeshing node. Historically, it has been difficult for smeshers to successfully sync their nodes owing to how time-consuming the syncing process is. Primarily, syncing includes downloading and independently verifying all blocks, transactions, ATXs, along with some other data, and reconstructing the current state from scratch. Naturally, this took the average smesher a lot of time. As such, in response to the growing difficulty of syncing a fresh node, we have prepared a way to speed up the syncing process. Introducing, Quicksync.

With Quicksync, instead of performing all of the syncing actions as stated above and calculating the network state from genesis, one just needs to download the current state from a trusted peer like the Spacemesh dev team or some other node. While this runs contrary to the web3 philosophy of "Don't trust, verify", we believe that this could be a choice some smeshers may be interested in given the high rate of trouble with syncing. Moreover, nothing precludes a smesher from verifying this state in the background once it is downloaded.

The state (also called an archive) that is downloaded is in the form of a state.sql file and can either be downloaded automatically using Smapp, or manually by using the `quicksync-rs` utility. 

Instructions for using `quicksync-rs` to download the state are given below.

## Windows

1. Download the latest release of the `quicksync-windows-vX.X.X.zip` file from the GitHub releases section.
2. Extract the `quicksync.exe` file from the zip file downloaded in step 1.
3. Move the `quicksync.exe` file to your `go-spacemesh` folder.
4. If you see a `state.sql` file in this folder, delete it. Otherwise, continue to step 5.
5. Open the Windows Powershell in this folder. You can do this by holding the "shift" key, right-clicking, and selecting the "Open PowerShell here" option.
6. Inside the powershell, type `.\quicksync.exe --help` and press enter. This will show you the available options.
7. We want to download the state database. Type `.\quicksync.exe download --node-data <path-to-your-node-data-folder>`. By default, the node data folder is located in the `C:\Users\{USERNAME}\AppData\Roaming\Spacemesh` directory and can be identified by the other folders and files it contains such as `bootstrap/`, `p2p/`, `genesis.json`, etc. A lot of smeshers label the node data folder as `sm_data` or just `data`. In the official Spacemesh docs and guides, this folder has been labelled as `node_data`.
8. Wait for the process to complete. The `quicksync-rs` utility will download, unzip, and verify the downloaded state.
9. Your node data folder should now have the latest `state.sql` file.

## Linux

1. Download the latest release of the `quicksync-linux-vX.X.X.zip` file from the GitHub releases section.
2. Extract the `quicksync` file from the zip file downloaded in step 1.
3. Make the `quicksync` file executable by using this CLI command: `chmod +x quicksync`. Now you have the `quicksync` executable.
4. Move the `quicksync` executable  to your `go-spacemesh` folder.
5. Delete the existing `state.sql` file from the node data directory.
6. Open a terminal in the `go-spacemesh` directory where the `quicksync` executable is, and run this command: `./quicksync download --node-data <path-to-your-node-data-folder>`. By default, the node data folder is located in `~/.config/Spacemesh`.
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

## Commands

The list of available commands for the `quicksync` utility is presented below. Note that these commands are for Linux. Simply, Change `./quicksync` to `.\quicksync.exe` For the Windows commands.

- `./quicksync download`: Downloads the latest `state.sql` file.
- `./quicksync check`: Checks if the current `state.sql` is up to date.
- `./quicksync help`: Displays all operations that `quicksync` can perform.
- `./quicksync --version`: Displays the quicksync version.
- `cargo run -- help`: Displays helpful commands for running the package. Relevant for developers.
