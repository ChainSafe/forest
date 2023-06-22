fn main() -> anyhow::Result<()> {
    std::env::set_var("FOREST_KEYSTORE_PHRASE", "");

    // setup:
    // FOREST_KEYSTORE_PHRASE="" \
    //      cargo run --bin forest -- \
    //      --chain=calibnet \
    //      --no-gc \
    //      --import-snapshot=/home/aatif/chainsafe/snapshots/filecoin_full_calibnet_2023-04-07_450000.car \
    //      --halt-after-import

    forest_filecoin::forestd_main([
        "forest",
        "--encrypt-keystore=false",
        "--chain=calibnet",
        "--no-gc",
        "--import-snapshot=/home/aatif/chainsafe/snapshots/filecoin_full_calibnet_2023-04-07_450000.car",
        "--skip-load=true",
        "--height=-500",
        "--halt-after-import",
    ])
}
