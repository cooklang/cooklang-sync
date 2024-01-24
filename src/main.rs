use cooklang_sync::run;
use std::env;

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 {
        let first_arg = &args[1]; // Get the first argument
        futures::executor::block_on(async {
            if let Err(e) = run(first_arg, "./mydb.sqlite3", "token").await {
                println!("error: {:?}", e)
            }
        });
    } else {
        println!("No arguments were provided.");
    }
}
