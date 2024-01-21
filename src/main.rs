
use cooklang_sync::run;

fn main() {
    println!("Hello");

    futures::executor::block_on(async {
        if let Err(e) = run("./tmp".to_string(), "./mydb.sqlite3".to_string(), "token".to_string()).await {
            println!("error: {:?}", e)
        }
    });

}

