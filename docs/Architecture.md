
## Upload flow

When client has some local changes they want to upload to remote.

Trying to commit

```mermaid
sequenceDiagram
    Upload Client->>Metaserver: commit("/Breakfast/Easy Pancakes.cook", "h1,h2,h3")
    Metaserver->>Upload Client: Need blocks: “h1,h2"
```

Uploading chunks requested

```mermaid
sequenceDiagram
    Upload Client->>Chunkserver: store([h1, h2], [b1, b2])
    Chunkserver->>Upload Client: Ok
```

Successful commit

```mermaid
sequenceDiagram
    Upload Client->>Metaserver: commit("/Breakfast/Easy Pancakes.cook", "h1,h2,h3")
    Metaserver->>Upload Client: Ok, jid=123
```

Advances local journal to version jid 123.

## Download flow

When remote has changes and a client want to download them.

```mermaid
sequenceDiagram
    Download Client->>Metaserver: list(jid=122)
    Metaserver->>Download Client: [(jid=123,"/Breakfast/Easy Pancakes.cook", “h1,h2,h3")]
```

```mermaid
sequenceDiagram
    Download Client->>Chunkserver: retrieve("h1,h2")
    Chunkserver->>Download Client: [b1, b2]
```

Advances local journal to version jid 123.
