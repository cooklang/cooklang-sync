use log::trace;

fn main() -> Result<(), cooklang_sync_client::errors::SyncError> {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();

    if args.len() > 3 {
        let monitor_path = &args[1];
        let db_path = &args[2];
        let api_endpoint = &args[3];
        let client_token = &args[4];

        let namespace_id = cooklang_sync_client::extract_uid_from_jwt(client_token);
        trace!("namespace_id: {:?}", namespace_id);
        // TODO refactor that it checks if argument requesting continues or one off sync
        cooklang_sync_client::run(
            monitor_path,
            db_path,
            api_endpoint,
            client_token,
            namespace_id,
            false,
        )?;
        // cooklang_sync_client::run_upload_once(monitor_path, db_path, api_endpoint, client_token)?;
        // cooklang_sync_client::run_download_once(monitor_path, db_path, api_endpoint, client_token)?;
    } else {
        panic!("No arguments were provided.");
    }
    Ok(())
}
