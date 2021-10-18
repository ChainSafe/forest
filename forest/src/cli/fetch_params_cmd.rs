// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use structopt::StructOpt;

use fil_types::SectorSize;
use paramfetch::{get_params_default, SectorSizeOpt};

#[allow(missing_docs)]
#[derive(Debug, StructOpt)]
pub struct FetchCommands {
    #[structopt(short, long, help = "Download all proof parameters")]
    all: bool,
    #[structopt(short, long, help = "Download only verification keys")]
    keys: bool,
    #[structopt(required_ifs(&[("all", "false"), ("keys", "false")]), help = "Size in bytes")]
    params_size: Option<String>,
    #[structopt(short, long, help = "Show verbose logging")]
    verbose: bool,
}

impl FetchCommands {
    pub async fn run(&self) {
        let sizes = if self.all {
            SectorSizeOpt::All
        } else if let Some(size) = &self.params_size {
            let sector_size = ram_to_int(size).unwrap();
            SectorSizeOpt::Size(sector_size)
        } else if self.keys {
            SectorSizeOpt::Keys
        } else {
            panic!("Sector size option must be chosen. Choose between --all, --keys, or <size>");
        };

        get_params_default(sizes, self.verbose).await.unwrap();
    }
}

/// Converts a human readable string to a u64 size.
fn ram_to_int(size: &str) -> Result<SectorSize, String> {
    // * there is no library to do this, but if other sector sizes are supported in future
    // this should probably be changed to parse from string to `SectorSize`
    let mut trimmed = size.trim_end_matches('B');
    trimmed = trimmed.trim_end_matches('b');

    match trimmed {
        "2048" | "2Ki" | "2ki" => Ok(SectorSize::_2KiB),
        "8388608" | "8Mi" | "8mi" => Ok(SectorSize::_8MiB),
        "536870912" | "512Mi" | "512mi" => Ok(SectorSize::_512MiB),
        "34359738368" | "32Gi" | "32gi" => Ok(SectorSize::_32GiB),
        "68719476736" | "64Gi" | "64gi" => Ok(SectorSize::_64GiB),
        _ => Err(format!(
            "Failed to parse: {}. Must be a valid sector size",
            size
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ram_str_conversions() {
        assert_eq!(ram_to_int("2048").unwrap(), SectorSize::_2KiB);
        assert_eq!(ram_to_int("2048B").unwrap(), SectorSize::_2KiB);
        assert_eq!(ram_to_int("2kib").unwrap(), SectorSize::_2KiB);
        assert_eq!(ram_to_int("8Mib").unwrap(), SectorSize::_8MiB);
        assert_eq!(ram_to_int("512MiB").unwrap(), SectorSize::_512MiB);
        assert_eq!(ram_to_int("32Gi").unwrap(), SectorSize::_32GiB);
        assert_eq!(ram_to_int("32GiB").unwrap(), SectorSize::_32GiB);
        assert_eq!(ram_to_int("64Gib").unwrap(), SectorSize::_64GiB);
    }
}
