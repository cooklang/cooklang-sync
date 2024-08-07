# Cooklang-sync library (Work in progress)

Based on desings from Dropbox https://dropbox.tech/infrastructure/streaming-file-synchronization.

Suitable for syncing text files and not large (<10MB) binary files.

## Roadmap

- Not even alpha
- Batch Downloading
- Tests!
- Need to generalise, now it’s too specific to our case
- Used in Cooklang Android app
- Improve DevEx for external developers
- Read only files
- Export metrics
- Security audit and hardening
- Streaming
- PG as DB for server
- Garbage collection for client
- Namespaces for client

## Running

Start server:

    cargo run


Start clients (specify directory to sync, db location, server address and JWT):

    cargo run --bin client ../tmp ./db/client.sqlite3 http://localhost:8000 eyXX.XXX.XXX

Test JWT can be generated on https://jwt.io using default values and specifying `your-256-bit-secret` to be `secret` and payload in this way (exp is timestamp in seconds for token expiration and uid is user ID):

    {
      "uid": 100,
      "exp": 1720260223
    }

