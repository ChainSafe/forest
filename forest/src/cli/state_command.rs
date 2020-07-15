
use structopt::StructOpt;

macro_rules! sub_cmd{
    ($enum: ident,  $cmd_name: expr , $desc: expr => 
        $($variant: ident , $name: expr , $about: expr, )+
    ) => {
        #[derive(StructOpt)]
        #[structopt(
            name = $cmd_name,
            about = $desc
        )]
        pub enum $enum {
            $(
                #[structopt(
                    name = $name,
                    about = $about
                )]
                $variant ($variant)
            ),+
        }
    }
}


sub_cmd!(
    StateCommand, "state", "Interact with and query filecoin chain state" =>
        Power, "power", "Query network or miner power",
        // Sectors, "sectors", "Query the sector set of a miner",
        // Proving, "proving", "Query the proving set of a miner",
        // PledgeCollateral, "pledge-collateral", "Get minimum miner pledge collateral",
        // ListActors, "list-actors", "list all actors in the network",
        // ListMiners, "list-miners", "list all miners in the network",
        // GetActor, "get-actor", "Print actor information",
        // Lookup, "lookup", "Find corresponding ID address",
        // Replay, "replay", "Replay a particular message within a tipset",
        // SectorSize, "sector-size", "Look up miners sector size",
        // ReadState, "read-state", "View a json representation of an actors state",
        // ListMessages, "list-messages", "list messages on chain matching given criteria",
        // ComputeState, "compute-state", "Perform state computations",
        // Call, "call", "Invoke a method on an actor locally",
        // GetDeal, "get-deal", "View on-chain deal info", 
        // WaitMsg, "wait-msg", "Wait for a message to appear on chain",
        // SearchMsg, "search-msg", "Search to see whether a message has appeared on chain",
        // MinerInfo, "miner-info", "Retrieve miner information",
);

#[derive(StructOpt)]
pub struct Power{
    #[structopt(short, long)]
    miner_address : Option<String>
}