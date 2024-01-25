use cooklang_sync::run;
use std::env;

#[tokio::main]
async fn main() -> Result<(), cooklang_sync::errors::MyError> {
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        let first_arg = &args[1];
        run(first_arg, "./mydb.sqlite3", "token").await?;
    } else {
        println!("No arguments were provided.");
    }
    Ok(())
}
