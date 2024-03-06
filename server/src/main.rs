#[rocket::launch]
fn rocket() -> rocket::Rocket<rocket::Build> {
    cooklang_sync_server::create_server()
}
