use async_std::fs::File;
use async_std::io::BufReader;
use std::path::PathBuf;
use structopt::StructOpt;

use cid::Cid;
use db::MemoryDB;
use forest_car::load_car;
use statediff::print_state_diff;

#[derive(Debug, StructOpt)]
pub struct CarCommand {
    #[structopt(parse(from_os_str))]
    #[structopt(short, long, help = "The CAR archive file")]
    file: PathBuf,
    #[structopt(help = "The pre state root cid")]
    pre: Cid,
    #[structopt(help = "The post state root cid")]
    post: Cid,
}

impl CarCommand {
    pub async fn run(&self) {
        let file = File::open(&self.file).await.unwrap();
        let buf_reader = BufReader::new(file);
        let bs = MemoryDB::default();

        let cids = load_car(&bs, buf_reader).await.unwrap();
        println!("Roots:");
        for cid in cids {
            println!("{cid}");
        }
        print_state_diff(&bs, &self.pre, &self.post, None).unwrap();
    }
}

#[derive(Debug, StructOpt)]
pub struct ChainCommand {
    #[structopt(help = "The pre cid object")]
    pre: Cid,
    #[structopt(help = "The post cid object")]
    post: Option<Cid>,
}

impl ChainCommand {
    pub async fn run(&self) {
        todo!()
    }
}

/// statediff binary subcommands available.
#[derive(StructOpt, Debug)]
#[structopt(setting = structopt::clap::AppSettings::VersionlessSubcommands)]
#[structopt(about = "statediff subcommands")]
enum Subcommand {
    #[structopt(name = "car", about = "Examine the state delta from a CAR")]
    Car(CarCommand),
    #[structopt(name = "chain", about = "Examine the state delta of an API object")]
    Chain(ChainCommand),
}

/// CLI structure generated when interacting with the statediff tool
#[derive(StructOpt)]
#[structopt(
    name = env!("CARGO_PKG_NAME"),
    version = option_env!("FOREST_VERSION").unwrap_or(env!("CARGO_PKG_VERSION")),
    about = env!("CARGO_PKG_DESCRIPTION"),
    author = env!("CARGO_PKG_AUTHORS")
)]
struct Cli {
    #[structopt(subcommand)]
    cmd: Subcommand,
}

#[async_std::main]
async fn main() {
    // Capture Cli inputs
    let Cli { cmd } = Cli::from_args();
    match cmd {
        Subcommand::Car(cmd) => {
            cmd.run().await;
        }
        Subcommand::Chain(cmd) => {
            cmd.run().await;
        }
    }
}
