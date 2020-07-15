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
        Sectors, "sectors", "Query the sector set of a miner",
        Proving, "proving", "Query the proving set of a miner",
        PledgeCollateral, "pledge-collateral", "Get minimum miner pledge collateral",
        ListActors, "list-actors", "list all actors in the network",
        ListMiners, "list-miners", "list all miners in the network",
        GetActor, "get-actor", "Print actor information",
        Lookup, "lookup", "Find corresponding ID address",
        Replay, "replay", "Replay a particular message within a tipset",
        SectorSize, "sector-size", "Look up miners sector size",
        ReadState, "read-state", "View a json representation of an actors state",
        ListMessages, "list-messages", "list messages on chain matching given criteria",
        ComputeState, "compute-state", "Perform state computations",
        Call, "call", "Invoke a method on an actor locally",
        GetDeal, "get-deal", "View on-chain deal info", 
        WaitMsg, "wait-msg", "Wait for a message to appear on chain",
        SearchMsg, "search-msg", "Search to see whether a message has appeared on chain",
        MinerInfo, "miner-info", "Retrieve miner information",
);

#[derive(StructOpt)]
pub struct Power{
    #[structopt(short, long)]
    miner_address : Option<String>
}

#[derive(StructOpt)]
pub struct Sectors{
    #[structopt(short, long)]
    miner_address : String
}

#[derive(StructOpt)]
pub struct Proving{
    #[structopt(short, long)]
    miner_address : String
}

#[derive(StructOpt)]
pub struct PledgeCollateral{
}

#[derive(StructOpt)]
pub struct ListActors{
}

#[derive(StructOpt)]
pub struct ListMiners{
    #[structopt(short, long)]
    sort_by : Option<String>
}

#[derive(StructOpt)]
pub struct GetActor{
    #[structopt(short, long)]
    actor_address : String
}

#[derive(StructOpt)]
pub struct Lookup{
    #[structopt(short, long)]
    address : String,
    #[structopt(short, long)]
    reverse : bool
}

#[derive(StructOpt)]
pub struct Replay{
    #[structopt(short, long)]
    tipset_key : String,
    #[structopt(short, long)]
    message_cid : String
}

#[derive(StructOpt)]
pub struct SectorSize{
    #[structopt(short, long)]
    miner_address : String
}

#[derive(StructOpt)]
pub struct ReadState{
    #[structopt(short, long)]
    actor_address : String
}

#[derive(StructOpt)]
pub struct ListMessages {
    #[structopt(short, long)]
    to : Option<String>,
    #[structopt(short, long)]
    from : Option<String>,
    #[structopt(long)]
    to_height : Option<u64>,
    #[structopt(short, long)]
    cids : Option<bool>,
}

#[derive(StructOpt)]
pub struct ComputeState {
    #[structopt(short, long)]
    vm_height : Option<u64>,
    #[structopt(short, long)]
    apply_mpool_messages : Option<bool>,
    #[structopt(short, long)]
    show_trace : Option<bool>,
    #[structopt(long)]
    html : Option<bool>,
}

#[derive(StructOpt)]
pub struct Call {
    #[structopt(short, long)]
    to_address : String,   
    #[structopt(short, long)]
    from : Option<String>,
    #[structopt(short, long)]
    value : Option<u64>
}

#[derive(StructOpt)]
pub struct GetDeal {
    #[structopt(short, long)]
    get_deal : String,   
}

#[derive(StructOpt)]
pub struct WaitMsg {
    #[structopt(short, long)]
    message_cid : String,
    #[structopt(short, long)]
    timeout : Option<u64>,
}

#[derive(StructOpt)]
pub struct SearchMsg {
    #[structopt(short, long)]
    message_cid : String,
}

#[derive(StructOpt)]
pub struct MinerInfo {
    #[structopt(short, long)]
    miner_address : String,
}