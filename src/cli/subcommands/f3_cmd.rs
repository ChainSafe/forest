// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(test)]
mod tests;

use std::{borrow::Cow, time::Duration};

use crate::{
    blocks::TipsetKey,
    lotus_json::HasLotusJson as _,
    rpc::{
        self,
        f3::{F3Instant, F3Manifest, F3PowerEntry, FinalityCertificate},
        prelude::*,
    },
    shim::fvm_shared_latest::ActorID,
};
use ahash::HashSet;
use anyhow::Context as _;
use cid::Cid;
use clap::{Subcommand, ValueEnum};
use itertools::Itertools as _;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use tera::Tera;

const MANIFEST_TEMPLATE_NAME: &str = "manifest.tpl";
const CERTIFICATE_TEMPLATE_NAME: &str = "certificate.tpl";
const PROGRESS_TEMPLATE_NAME: &str = "progress.tpl";

static TEMPLATES: Lazy<Tera> = Lazy::new(|| {
    let mut tera = Tera::default();
    tera.add_raw_template(MANIFEST_TEMPLATE_NAME, include_str!("f3_cmd/manifest.tpl"))
        .unwrap();
    tera.add_raw_template(
        CERTIFICATE_TEMPLATE_NAME,
        include_str!("f3_cmd/certificate.tpl"),
    )
    .unwrap();
    tera.add_raw_template(PROGRESS_TEMPLATE_NAME, include_str!("f3_cmd/progress.tpl"))
        .unwrap();

    #[allow(clippy::disallowed_types)]
    fn format_duration(
        value: &serde_json::Value,
        _args: &std::collections::HashMap<String, serde_json::Value>,
    ) -> tera::Result<serde_json::Value> {
        if let Some(duration_nano_secs) = value.as_u64() {
            let duration = Duration::from_lotus_json(duration_nano_secs);
            return Ok(serde_json::Value::String(
                humantime::format_duration(duration).to_string(),
            ));
        }

        Ok(value.clone())
    }
    tera.register_filter("format_duration", format_duration);

    tera
});

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
    #[command(subcommand, name = "powertable", visible_alias = "pt")]
    PowerTable(F3PowerTableCommands),
}

impl F3Commands {
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        match self {
            Self::Manifest { output } => {
                let manifest = client.call(F3GetManifest::request(())?).await?;
                match output {
                    F3OutputFormat::Text => {
                        println!("{}", render_manifest_template(&manifest)?);
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
                println!("{}", render_progress_template(&progress)?);
                let manifest = client.call(F3GetManifest::request(())?).await?;
                println!("{}", render_manifest_template(&manifest)?);
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
    /// Lists a range of F3 finality certificates.
    List {
        /// Inclusive range of `from` and `to` instances in following notation:
        /// `<from>..<to>`. Either `<from>` or `<to>` may be omitted, but not both.
        range: Option<String>,
        /// The output format.
        #[arg(long, value_enum, default_value_t = F3OutputFormat::Text)]
        output: F3OutputFormat,
        /// The maximum number of instances. A value less than 0 indicates no limit.
        #[arg(long, default_value_t = 10)]
        limit: i64,
        /// Reverses the default order of output.
        #[arg(long, default_value_t = false)]
        reverse: bool,
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
                        println!("{}", render_certificate_template(&cert)?);
                    }
                    F3OutputFormat::Json => {
                        println!("{}", serde_json::to_string_pretty(&cert)?);
                    }
                }
            }
            Self::List {
                range,
                output,
                limit,
                reverse,
            } => {
                let (from, to_opt) = if let Some(range) = range {
                    let (from_opt, to_opt) = Self::parse_range_unvalidated(&range)?;
                    (from_opt.unwrap_or_default(), to_opt)
                } else {
                    (0, None)
                };
                let to = if let Some(i) = to_opt {
                    i
                } else {
                    F3GetLatestCertificate::call(&client, ()).await?.instance
                };
                anyhow::ensure!(
                    to >= from,
                    "ERROR: invalid range: 'from' cannot exceed 'to':  {from} > {to}"
                );
                let limit = if limit < 0 {
                    usize::MAX
                } else {
                    limit as usize
                };
                let range: Box<dyn Iterator<Item = u64>> = if reverse {
                    Box::new((from..=to).take(limit))
                } else {
                    Box::new((from..=to).rev().take(limit))
                };
                for i in range {
                    let cert = F3GetCertificate::call(&client, (i,)).await?;
                    match output {
                        F3OutputFormat::Text => {
                            println!("{}", render_certificate_template(&cert)?);
                        }
                        F3OutputFormat::Json => {
                            println!("{}", serde_json::to_string_pretty(&cert)?);
                        }
                    }
                    println!();
                }
            }
        }

        Ok(())
    }

    /// Parse range without validating `to >= from`
    fn parse_range_unvalidated(range: &str) -> anyhow::Result<(Option<u64>, Option<u64>)> {
        let pattern = lazy_regex::regex!(r#"^(?P<from>\d+)?\.\.(?P<to>\d+)?$"#);
        if let Some(captures) = pattern.captures(range) {
            let from = captures
                .name("from")
                .map(|i| i.as_str().parse().expect("Infallible"));
            let to = captures
                .name("to")
                .map(|i| i.as_str().parse().expect("Infallible"));
            anyhow::ensure!(from.is_some() || to.is_some(), "invalid range `{range}`");
            Ok((from, to))
        } else {
            anyhow::bail!("invalid range `{range}`");
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum F3PowerTableCommands {
    /// Gets F3 power table at a specific instance ID or latest instance if none is specified.
    #[command(visible_alias = "g")]
    Get {
        /// instance ID. (default: latest)
        instance: Option<u64>,
        /// Whether to get the power table from EC. (default: false)
        #[arg(long, default_value_t = false)]
        ec: bool,
    },
    /// Gets the total proportion of power for a list of actors at a given instance.
    #[command(visible_alias = "gp")]
    GetProportion {
        actor_ids: Vec<u64>,
        /// instance ID. (default: latest)
        #[arg(long, required = false)]
        instance: Option<u64>,
        /// Whether to get the power table from EC. (default: false)
        #[arg(long, required = false, default_value_t = false)]
        ec: bool,
    },
}

impl F3PowerTableCommands {
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        match self {
            Self::Get { instance, ec } => {
                let (instance, power_table_cid, power_table) =
                    Self::get_power_table(&client, instance, ec).await?;
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
            Self::GetProportion {
                actor_ids,
                instance,
                ec,
            } => {
                anyhow::ensure!(
                    !actor_ids.is_empty(),
                    "at least one actor ID must be specified"
                );
                let (instance, power_table_cid, power_table) =
                    Self::get_power_table(&client, instance, ec).await?;
                let total = power_table
                    .iter()
                    .fold(num::BigInt::ZERO, |acc, entry| acc + &entry.power);
                let mut scaled_total = 0;
                let mut scaled_sum = 0;
                let mut actor_id_set = HashSet::from_iter(actor_ids);
                for entry in power_table.iter() {
                    let scaled_power = scale_power(&entry.power, &total)?;
                    scaled_total += scaled_power;
                    if actor_id_set.remove(&entry.id) {
                        scaled_sum += scaled_power;
                    }
                }

                let result = F3PowerTableGetProportionCommandResult {
                    instance,
                    from_ec: ec,
                    power_table: F3PowerTableCliMinimalJson {
                        cid: power_table_cid,
                        scaled_total,
                    },
                    scaled_sum,
                    proportion: (scaled_sum as f64) / (scaled_total as f64),
                    not_found: actor_id_set.into_iter().collect(),
                };
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        };

        Ok(())
    }

    async fn get_power_table(
        client: &rpc::Client,
        instance: Option<u64>,
        ec: bool,
    ) -> anyhow::Result<(u64, Cid, Vec<F3PowerEntry>)> {
        let instance = if let Some(instance) = instance {
            instance
        } else {
            let progress = F3GetProgress::call(client, ()).await?;
            progress.id
        };
        let (tsk, power_table_cid) =
            Self::get_power_table_tsk_by_instance(client, instance).await?;
        let power_table = if ec {
            F3GetECPowerTable::call(client, (tsk.into(),)).await?
        } else {
            F3GetF3PowerTable::call(client, (tsk.into(),)).await?
        };
        Ok((instance, power_table_cid, power_table))
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
            .clone();
        Ok((tsk, previous.supplemental_data.power_table))
    }
}

fn render_manifest_template(template: &F3Manifest) -> anyhow::Result<String> {
    let mut context = tera::Context::from_serialize(template)?;
    context.insert(
        "initial_power_table_cid",
        &if template.initial_power_table != Cid::default() {
            Cow::Owned(template.initial_power_table.to_string())
        } else {
            Cow::Borrowed("unknown")
        },
    );
    Ok(TEMPLATES
        .render(MANIFEST_TEMPLATE_NAME, &context)?
        .trim_end()
        .to_owned())
}

fn render_certificate_template(template: &FinalityCertificate) -> anyhow::Result<String> {
    const MAX_TIPSETS: usize = 10;
    const MAX_TIPSET_KEYS: usize = 2;
    let mut context = tera::Context::from_serialize(template)?;
    context.insert(
        "power_table_cid",
        &template.supplemental_data.power_table.to_string(),
    );
    context.insert(
        "power_table_delta_string",
        &template.power_table_delta_string(),
    );
    context.insert(
        "epochs",
        &format!(
            "{}-{}",
            template.chain_base().epoch,
            template.chain_head().epoch
        ),
    );
    let mut chain_lines = vec![];
    for (i, ts) in template.ec_chain.iter().take(MAX_TIPSETS).enumerate() {
        let table = if i + 1 == template.ec_chain.len() {
            "    └──"
        } else {
            "    ├──"
        };
        let mut keys = ts
            .key
            .iter()
            .take(MAX_TIPSET_KEYS)
            .map(|i| i.to_string())
            .join(", ");
        if ts.key.len() > MAX_TIPSET_KEYS {
            keys = format!("{keys}, ...");
        }
        chain_lines.push(format!(
            "{table}{} (length: {}): [{keys}]",
            ts.epoch,
            ts.key.len()
        ));
    }
    if template.ec_chain.len() > MAX_TIPSETS {
        let n_remaining = template.ec_chain.len() - MAX_TIPSETS;
        chain_lines.push(format!(
            "    └──...omitted the remaining {n_remaining} tipsets."
        ));
    }
    chain_lines.push(format!("Signed by {} miner(s).", template.signers.len()));
    context.insert("chain_lines", &chain_lines);
    Ok(TEMPLATES
        .render(CERTIFICATE_TEMPLATE_NAME, &context)?
        .trim_end()
        .to_owned())
}

fn render_progress_template(template: &F3Instant) -> anyhow::Result<String> {
    let mut context = tera::Context::from_serialize(template)?;
    context.insert("phase_string", template.phase_string());
    Ok(TEMPLATES
        .render(PROGRESS_TEMPLATE_NAME, &context)?
        .trim_end()
        .to_owned())
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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct F3PowerTableGetProportionCommandResult {
    instance: u64,
    #[serde(rename = "FromEC")]
    from_ec: bool,
    power_table: F3PowerTableCliMinimalJson,
    scaled_sum: i64,
    proportion: f64,
    not_found: Vec<ActorID>,
}

#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct F3PowerTableCliMinimalJson {
    #[serde(rename = "CID")]
    #[serde_as(as = "DisplayFromStr")]
    cid: Cid,
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
