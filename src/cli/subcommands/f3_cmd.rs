// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    blocks::TipsetKey,
    rpc::{
        self,
        f3::{F3Instant, F3Manifest, F3PowerEntry, FinalityCertificate},
        prelude::*,
    },
};
use anyhow::Context as _;
use cid::Cid;
use clap::{Subcommand, ValueEnum};
use sailfish::TemplateSimple;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};

/// Output format
#[derive(ValueEnum, Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum F3OutputFormat {
    /// Text
    #[default]
    Text,
    /// JSON
    Json,
}

/// Manages Filecoin Fast Finality (F3) interactions
#[derive(Debug, Subcommand)]
pub enum F3Commands {
    /// Gets the current manifest used by F3
    Manifest {
        /// The output format.
        #[arg(long, value_enum, default_value_t = F3OutputFormat::Text)]
        output: F3OutputFormat,
    },
    /// Checks the F3 status.
    Status,
    /// Manages interactions with F3 finality certificates.
    #[command(subcommand, visible_alias = "c")]
    Certs(F3CertsCommands),
    /// Gets F3 power table at a specific instance ID or latest instance if none is specified.
    #[command(subcommand, visible_alias = "pt")]
    PowerTable(F3PowerTableCommands),
}

impl F3Commands {
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        match self {
            Self::Manifest { output } => {
                let manifest = client.call(F3GetManifest::request(())?).await?;
                match output {
                    F3OutputFormat::Text => {
                        let template = ManifestTemplate::new(manifest);
                        println!("{}", template.render_once()?);
                    }
                    F3OutputFormat::Json => {
                        println!("{}", serde_json::to_string_pretty(&manifest)?);
                    }
                }
                Ok(())
            }
            Self::Status => {
                let is_running = client.call(F3IsRunning::request(())?).await?;
                println!("Running: {is_running}");
                let progress = client.call(F3GetProgress::request(())?).await?;
                let progress_template = ProgressTemplate::new(progress);
                println!("{}", progress_template.render_once()?);
                let manifest = client.call(F3GetManifest::request(())?).await?;
                let manifest_template = ManifestTemplate::new(manifest);
                println!("{}", manifest_template.render_once()?);
                Ok(())
            }
            Self::Certs(cmd) => cmd.run(client).await,
            Self::PowerTable(cmd) => cmd.run(client).await,
        }
    }
}

/// Manages interactions with F3 finality certificates.
#[derive(Debug, Subcommand)]
pub enum F3CertsCommands {
    /// Gets an F3 finality certificate to a given instance ID, or the latest certificate if no instance is specified.
    Get {
        instance: Option<u64>,
        /// The output format.
        #[arg(long, value_enum, default_value_t = F3OutputFormat::Text)]
        output: F3OutputFormat,
    },
}

impl F3CertsCommands {
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        match self {
            Self::Get { instance, output } => {
                let cert = if let Some(instance) = instance {
                    client.call(F3GetCertificate::request((instance,))?).await?
                } else {
                    client.call(F3GetLatestCertificate::request(())?).await?
                };
                match output {
                    F3OutputFormat::Text => {
                        let template = FinalityCertificateTemplate::new(cert);
                        println!("{}", template.render_once()?);
                    }
                    F3OutputFormat::Json => {
                        println!("{}", serde_json::to_string_pretty(&cert)?);
                    }
                }
                Ok(())
            }
        }
    }
}

/// Gets F3 power table at a specific instance ID or latest instance if none is specified.
#[derive(Debug, Subcommand)]
pub enum F3PowerTableCommands {
    #[command(visible_alias = "g")]
    Get {
        /// instance ID. (default: latest)
        instance: Option<u64>,
        /// Whether to get the power table from EC. (default: false)
        #[arg(long, default_value_t = false)]
        ec: bool,
    },
}

impl F3PowerTableCommands {
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        match self {
            Self::Get { instance, ec } => {
                let instance = if let Some(instance) = instance {
                    instance
                } else {
                    let progress = F3GetProgress::call(&client, ()).await?;
                    progress.id
                };
                let (tsk, power_table_cid) =
                    Self::get_power_table_tsk_by_instance(&client, instance).await?;
                let power_table = if ec {
                    F3GetECPowerTable::call(&client, (tsk.into(),)).await?
                } else {
                    F3GetF3PowerTable::call(&client, (tsk.into(),)).await?
                };
                let total = power_table
                    .iter()
                    .fold(num::BigInt::ZERO, |acc, entry| acc + &entry.power);
                let mut scaled_total = 0;
                for entry in power_table.iter() {
                    scaled_total += scale_power(&entry.power, &total)?;
                }
                let result = F3PowerTableGetCommandResult {
                    instance,
                    from_ec: ec,
                    power_table: F3PowerTableCliJson {
                        cid: power_table_cid,
                        entries: power_table,
                        total,
                        scaled_total,
                    },
                };
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        };

        Ok(())
    }

    async fn get_power_table_tsk_by_instance(
        client: &rpc::Client,
        instance: u64,
    ) -> anyhow::Result<(TipsetKey, Cid)> {
        let manifest = F3GetManifest::call(client, ()).await?;
        if instance < manifest.initial_instance + manifest.committee_lookback {
            let epoch = manifest.bootstrap_epoch - manifest.ec.finality;
            let ts = ChainGetTipSetByHeight::call(client, (epoch, None.into())).await?;
            return Ok((ts.key().clone(), manifest.initial_power_table));
        }

        let previous = F3GetCertificate::call(client, (instance.saturating_sub(1),)).await?;
        let lookback = F3GetCertificate::call(
            client,
            (instance.saturating_sub(manifest.committee_lookback),),
        )
        .await?;
        let tsk = lookback
            .ec_chain
            .last()
            .context("lookback EC chain is empty")?
            .key
            .clone()
            .try_into()?;
        Ok((tsk, previous.supplemental_data.power_table))
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct F3PowerTableGetCommandResult {
    instance: u64,
    #[serde(rename = "FromEC")]
    from_ec: bool,
    power_table: F3PowerTableCliJson,
}

#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct F3PowerTableCliJson {
    #[serde(rename = "CID")]
    #[serde_as(as = "DisplayFromStr")]
    cid: Cid,
    entries: Vec<F3PowerEntry>,
    #[serde(with = "crate::lotus_json::stringify")]
    total: num::BigInt,
    scaled_total: i64,
}

fn scale_power(power: &num::BigInt, total: &num::BigInt) -> anyhow::Result<i64> {
    const MAX_POWER: i64 = 0xffff;
    if total < power {
        anyhow::bail!("total power {total} is less than the power of a single participant {power}");
    }
    let scacled = MAX_POWER * power / total;
    Ok(scacled.try_into()?)
}

#[derive(TemplateSimple, Debug, Clone, Serialize, Deserialize)]
#[template(path = "cli/f3/manifest.stpl")]
struct ManifestTemplate {
    manifest: F3Manifest,
    is_initial_power_table_defined: bool,
}

impl ManifestTemplate {
    fn new(manifest: F3Manifest) -> Self {
        let is_initial_power_table_defined = manifest.initial_power_table != Cid::default();
        Self {
            manifest,
            is_initial_power_table_defined,
        }
    }
}

#[derive(TemplateSimple, Debug, Clone, Serialize, Deserialize)]
#[template(path = "cli/f3/progress.stpl")]
struct ProgressTemplate {
    progress: F3Instant,
}

impl ProgressTemplate {
    fn new(progress: F3Instant) -> Self {
        Self { progress }
    }
}

#[derive(TemplateSimple, Debug, Clone, Serialize, Deserialize)]
#[template(path = "cli/f3/certificate.stpl")]
struct FinalityCertificateTemplate {
    cert: FinalityCertificate,
}

impl FinalityCertificateTemplate {
    fn new(cert: FinalityCertificate) -> Self {
        Self { cert }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_template() {
        // lotus f3 manifest --output json
        let lotus_json = serde_json::json!({
          "Pause": false,
          "ProtocolVersion": 4,
          "InitialInstance": 0,
          "BootstrapEpoch": 2081674,
          "NetworkName": "calibrationnet",
          "ExplicitPower": null,
          "IgnoreECPower": false,
          "InitialPowerTable": {
            "/": "bafy2bzaceab236vmmb3n4q4tkvua2n4dphcbzzxerxuey3mot4g3cov5j3r2c"
          },
          "CommitteeLookback": 10,
          "CatchUpAlignment": 15000000000_u64,
          "Gpbft": {
            "Delta": 6000000000_u64,
            "DeltaBackOffExponent": 2_f64,
            "MaxLookaheadRounds": 5,
            "RebroadcastBackoffBase": 6000000000_u64,
            "RebroadcastBackoffExponent": 1.3,
            "RebroadcastBackoffSpread": 0.1,
            "RebroadcastBackoffMax": 60000000000_u64
          },
          "EC": {
            "Period": 30000000000_u64,
            "Finality": 900,
            "DelayMultiplier": 2_f64,
            "BaseDecisionBackoffTable": [
              1.3,
              1.69,
              2.2,
              2.86,
              3.71,
              4.83,
              6.27,
              7.5
            ],
            "HeadLookback": 0,
            "Finalize": true
          },
          "CertificateExchange": {
            "ClientRequestTimeout": 10000000000_u64,
            "ServerRequestTimeout": 60000000000_u64,
            "MinimumPollInterval": 30000000000_u64,
            "MaximumPollInterval": 120000000000_u64
          }
        });
        let manifest: F3Manifest = serde_json::from_value(lotus_json).unwrap();
        let template = ManifestTemplate::new(manifest);
        println!("{}", template.render_once().unwrap());
    }

    #[test]
    fn test_progress_template() {
        let lotus_json = serde_json::json!({
          "ID": 1000,
          "Round": 0,
          "Phase": 0
        });
        let progress: F3Instant = serde_json::from_value(lotus_json).unwrap();
        let template = ProgressTemplate::new(progress);
        println!("{}", template.render_once().unwrap());
    }

    #[test]
    fn test_finality_certificate_template() {
        // lotus f3 c get --output json 6204
        let lotus_json = serde_json::json!({
            "GPBFTInstance": 6204,
            "ECChain": [
              {
                "Epoch": 2088927,
                "Key": "AXGg5AIg1NBjOnFimwUueRXQQzvPbHZO6vXbvqNA1gcomlVrq5MBcaDkAiCaOt71j85kjjq3SZF0NQq03tauEW3iwscIr4Qw0wna+g==",
                "PowerTable": {
                  "/": "bafy2bzaceazjn2promafvtkaquebfgc3xvhoavdbxwns4i54ilgnzch7pkgua"
                },
                "Commitments": [
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0
                ]
              },
              {
                "Epoch": 2088928,
                "Key": "AXGg5AIgFn9g3q/ATrgWiWzUYZLrtN/POrkNWFPmUShj/MDqZ5IBcaDkAiACwpEW4PvUCOIsZRaYhF6W+L1bgGd2TUFLOkATNxvuGgFxoOQCILlKPpFgMxXYFcq2HslyxzBN9ZZ6iPrPSBI2uwT4tUAvAXGg5AIgwYDZ217HUZ6nGnm6fnNd5lhep2C02mSYkkjJPf5pOig=",
                "PowerTable": {
                  "/": "bafy2bzaceazjn2promafvtkaquebfgc3xvhoavdbxwns4i54ilgnzch7pkgua"
                },
                "Commitments": [
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0,
                  0
                ]
              }
            ],
            "SupplementalData": {
              "Commitments": [
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0
              ],
              "PowerTable": {
                "/": "bafy2bzaceazjn2promafvtkaquebfgc3xvhoavdbxwns4i54ilgnzch7pkgua"
              }
            },
            "Signers": [
              0,
              3
            ],
            "Signature": "uYtvw/NWm2jKQj+d99UAG4aiPnpAMSrwAWIusv0XkjsOYYR0fyU4nUM++cAQGO47E2/J8WSDjstLgL+yMVAFC+Tgao4o9ILXIlhqhxObnNZ/Ehanajthif9SaRe1AO69",
            "PowerTableDelta": [
              {
                "ParticipantID": 3782,
                "PowerDelta": "76347338653696",
                "SigningKey": "lXSMTNEVmIdVxJV4clmW35jrlsBEfytNUGTWVih2dFlQ1k/7QQttsUGzpD5JoNaQ"
              }
            ]
        });
        let cert: FinalityCertificate = serde_json::from_value(lotus_json).unwrap();
        let template = FinalityCertificateTemplate::new(cert);
        println!("{}", template.render_once().unwrap());
    }
}
