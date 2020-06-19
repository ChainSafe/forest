// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::cli::Subcommand;

/// Converts a human readable string to a u64 size.
fn ram_to_int(_size: &str) -> Result<u64, String> {
    todo!()
}

/// Process CLI subcommand
pub(super) fn process(command: Subcommand) {
    match command {
        Subcommand::FetchParams { params_size } => {
            let _sector_size = ram_to_int(&params_size).unwrap();
            todo!()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ram_str_conversions() {
        assert_eq!(ram_to_int("2048").unwrap(), 2048);
        assert_eq!(ram_to_int("2048B").unwrap(), 2048);
        assert_eq!(ram_to_int("2kib").unwrap(), 2048);
        assert_eq!(ram_to_int("2KB").unwrap(), 2000);
        assert_eq!(ram_to_int("512MiB").unwrap(), 512 * 2 << 20);
        assert_eq!(ram_to_int("32Gi").unwrap(), 2 << 30);
        assert_eq!(ram_to_int("32GiB").unwrap(), 2 << 30);
    }
}
