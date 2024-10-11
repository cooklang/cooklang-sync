# Cooklang-sync library (Work in progress)

Based on desings from Dropbox https://dropbox.tech/infrastructure/streaming-file-synchronization.

Suitable for syncing text files and not large (<10MB) binary files.

## Roadmap

- Batch Downloading
- Tests!
- Need to generalise, now it’s too specific to our case
- Improve DevEx for external developers
- Read only files
- Export metrics
- Security audit and hardening
- Streaming
- Garbage collection for client
- Support of symlinks
- Hidden files

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

