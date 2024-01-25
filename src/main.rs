use cooklang_sync::run;

#[tokio::main]
async fn main() -> Result<(), cooklang_sync::errors::SyncError> {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();

    if args.len() > 2 {
        let monitor_path = &args[1];
        let db_path = &args[2];
        let client_token = &args[3];
        run(monitor_path, db_path, client_token).await?;
    } else {
        panic!("No arguments were provided.");
    }
    Ok(())
}
