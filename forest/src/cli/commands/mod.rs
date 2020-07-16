use structopt::StructOpt;
use async_trait::async_trait;

pub mod sync;
pub use sync::*;

#[macro_export]
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


#[async_trait]
pub trait CLICommand{
    async fn handle(self);
}
