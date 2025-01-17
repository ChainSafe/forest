use async_std::io::{stdin, stdout};
use rs_car_ipfs::single_file::read_single_file_buffer;

#[async_std::main]
async fn main() {
    let mut stdin = stdin();
    let mut stdout = stdout();

    if let Err(err) = read_single_file_buffer(&mut stdin, &mut stdout, None, None).await {
        eprintln!("Error: {}", err);
        std::process::exit(1);
    }
}
