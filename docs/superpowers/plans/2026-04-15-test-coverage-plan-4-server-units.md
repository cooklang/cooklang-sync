# Test Coverage Plan 4: Server Unit Tests

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Pin the behavior of every server module whose logic is self-contained (auth token decoding, `ChunkId` pathing, metadata DB queries, serde shapes, notification fan-out, request guards). Each module grows an inline `#[cfg(test)] mod tests` block. No route-level Rocket client tests yet — those come in Plan 5.

**Architecture:** All tests live inline in their respective `server/src/**/*.rs` files. Module visibility on the server side is overwhelmingly `pub(crate)` (the only public symbol is `create_server()`), so external integration tests cannot reach the internals we want to cover here. Inline `#[cfg(test)] mod tests` sidesteps that and also gives each unit file its own bounded commit.

Two awkward shared-state surfaces force tests to serialise:

- `UPLOAD_DIR` is read from the process environment inside `ChunkId::file_path()` and therefore also inside `is_present()` / `non_local_chunks()`.
- `JWT_SECRET` is read from the process environment inside `auth/request.rs::secret()`.

Both are handled with `serial_test::serial` + a per-test tempdir / sentinel secret. Tests that never touch env vars stay parallel.

**Tech Stack:** Rust 2021, `rocket` (local async client for request-guard tests only), `diesel` + `diesel_migrations` (in-memory SQLite for DB tests), `jsonwebtoken` (already a prod dep, reused for signing fixtures), `tempfile`, `serial_test`, `serde_json`.

**Stacked PR context:** This is Plan 4 of 6, stacked on top of Plan 3 (`test-coverage-plan-3`). Plan 5 (server routes) and Plan 6 (end-to-end) will stack on this one.

**Scope gate (MUST follow):**

- Do **not** modify any file under `client/**` (prod or tests).
- Do **not** modify server prod logic — i.e. any code *outside* `#[cfg(test)]` blocks, and outside `[dev-dependencies]` in `server/Cargo.toml`.
- Do **not** add any file under `server/tests/**`. All new test code goes inline in `server/src/**/*.rs` via `#[cfg(test)] mod tests`.
- Cleaning up pre-existing clippy warnings inside existing `#[cfg(test)]` blocks (e.g. `assert_eq!(x, true)` → `assert!(x)`) is allowed and expected when encountered in a touched file. Flag such cleanups explicitly in the commit body.
- Every commit must pass `cargo test --workspace --all-features` and `cargo clippy --workspace --all-features --all-targets -- -D warnings`.

---

## Task 1: Server dev-deps + foundation

**Files:**
- Modify: `server/Cargo.toml`

**Why:** All later tasks depend on `tempfile` (for `TempDir`-backed UPLOAD_DIR), `serial_test` (for env-var isolation), and `serde_json` (for explicit JSON round-trips without leaning on rocket re-exports).

- [ ] **Step 1: Add dev-deps**

In `server/Cargo.toml`, extend `[dev-dependencies]` so it reads:

```toml
[dev-dependencies]
mockall = "0.13"
tokio-test = "0.4"
proptest = "1.9"
rocket = { version = "0.5", features = ["json"] }
tempfile = "3"
serial_test = "3"
serde_json = "1"
```

No other line in the file changes.

- [ ] **Step 2: Verify baseline builds**

Run: `cargo build --workspace --all-targets`
Expected: clean build, new deps fetched.

- [ ] **Step 3: Commit**

```bash
git add server/Cargo.toml Cargo.lock
git commit -m "test(server): add tempfile/serial_test/serde_json dev-deps"
```

---

## Task 2: `auth/token.rs` — `decode_token` behavior

**Files:**
- Modify: `server/src/auth/token.rs`

**Why:** `decode_token` is the single seam between `auth/request.rs` and the `jsonwebtoken` crate. We pin happy-path, expired, wrong-secret, and malformed behavior so downstream request-guard tests can assume it.

- [ ] **Step 1: Add inline test module**

Append to `server/src/auth/token.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};

    const SECRET: &[u8] = b"unit-test-secret";

    fn make_token(uid: i32, exp: usize, secret: &[u8]) -> String {
        #[derive(serde::Serialize)]
        struct TestClaims {
            uid: i32,
            exp: usize,
        }
        encode(
            &Header::new(Algorithm::HS256),
            &TestClaims { uid, exp },
            &EncodingKey::from_secret(secret),
        )
        .expect("encode test token")
    }

    const YEAR_2100: usize = 4_102_444_800;

    #[test]
    fn decode_valid_hs256_returns_uid() {
        let token = make_token(42, YEAR_2100, SECRET);
        let claims = decode_token(&token, SECRET).expect("valid token decodes");
        assert_eq!(claims.uid, 42);
    }

    #[test]
    fn decode_expired_token_returns_err() {
        // 2001-09-09 timestamp — safely in the past.
        let token = make_token(7, 1_000_000_000, SECRET);
        assert!(decode_token(&token, SECRET).is_err());
    }

    #[test]
    fn decode_with_wrong_secret_returns_err() {
        let token = make_token(1, YEAR_2100, SECRET);
        assert!(decode_token(&token, b"different-secret").is_err());
    }

    #[test]
    fn decode_malformed_token_returns_err() {
        assert!(decode_token("not.a.jwt", SECRET).is_err());
        assert!(decode_token("", SECRET).is_err());
        assert!(decode_token("only.two", SECRET).is_err());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p cooklang-sync-server --all-features auth::token::tests -- --nocapture`
Expected: 4 tests pass.

- [ ] **Step 3: Clippy**

Run: `cargo clippy -p cooklang-sync-server --all-features --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add server/src/auth/token.rs
git commit -m "test(server): pin decode_token happy/expired/wrong-secret/malformed"
```

---

## Task 3: `auth/request.rs` — `FromRequest for User`

**Files:**
- Modify: `server/src/auth/request.rs`

**Why:** The request guard converts `Authorization: Bearer <jwt>` into a `User`. It reads `JWT_SECRET` from the environment, so tests must serialise on that env var. We cover the four paths: valid, no header, wrong prefix, bad token.

- [ ] **Step 1: Add inline test module**

Append to `server/src/auth/request.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::user::User;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use rocket::http::Status;
    use rocket::local::asynchronous::Client;
    use rocket::{get, routes};
    use serial_test::serial;

    const TEST_SECRET: &str = "request-guard-test-secret";

    fn sign(uid: i32, exp: usize) -> String {
        #[derive(serde::Serialize)]
        struct TestClaims {
            uid: i32,
            exp: usize,
        }
        encode(
            &Header::default(),
            &TestClaims { uid, exp },
            &EncodingKey::from_secret(TEST_SECRET.as_bytes()),
        )
        .expect("sign test token")
    }

    #[get("/whoami")]
    fn whoami(user: User) -> String {
        user.id.to_string()
    }

    async fn client() -> Client {
        let rocket = rocket::build().mount("/", routes![whoami]);
        Client::tracked(rocket).await.expect("rocket client")
    }

    #[rocket::async_test]
    #[serial(jwt_secret)]
    async fn valid_bearer_token_is_authorized() {
        std::env::set_var("JWT_SECRET", TEST_SECRET);
        let token = sign(77, 4_102_444_800);
        let client = client().await;
        let resp = client
            .get("/whoami")
            .header(rocket::http::Header::new(
                "Authorization",
                format!("Bearer {}", token),
            ))
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::Ok);
        assert_eq!(resp.into_string().await.unwrap(), "77");
    }

    #[rocket::async_test]
    #[serial(jwt_secret)]
    async fn missing_authorization_header_is_unauthorized() {
        std::env::set_var("JWT_SECRET", TEST_SECRET);
        let client = client().await;
        let resp = client.get("/whoami").dispatch().await;
        assert_eq!(resp.status(), Status::Unauthorized);
    }

    #[rocket::async_test]
    #[serial(jwt_secret)]
    async fn non_bearer_prefix_is_unauthorized() {
        std::env::set_var("JWT_SECRET", TEST_SECRET);
        let token = sign(1, 4_102_444_800);
        let client = client().await;
        let resp = client
            .get("/whoami")
            .header(rocket::http::Header::new(
                "Authorization",
                format!("Token {}", token),
            ))
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::Unauthorized);
    }

    #[rocket::async_test]
    #[serial(jwt_secret)]
    async fn bad_signature_is_unauthorized() {
        std::env::set_var("JWT_SECRET", TEST_SECRET);
        // Sign with a *different* secret than the server will use.
        #[derive(serde::Serialize)]
        struct TestClaims {
            uid: i32,
            exp: usize,
        }
        let bad = encode(
            &Header::default(),
            &TestClaims {
                uid: 1,
                exp: 4_102_444_800,
            },
            &EncodingKey::from_secret(b"other-secret"),
        )
        .unwrap();
        let client = client().await;
        let resp = client
            .get("/whoami")
            .header(rocket::http::Header::new(
                "Authorization",
                format!("Bearer {}", bad),
            ))
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::Unauthorized);
    }

    #[rocket::async_test]
    #[serial(jwt_secret)]
    async fn malformed_bearer_token_is_unauthorized() {
        std::env::set_var("JWT_SECRET", TEST_SECRET);
        let client = client().await;
        let resp = client
            .get("/whoami")
            .header(rocket::http::Header::new(
                "Authorization",
                "Bearer not.a.jwt",
            ))
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::Unauthorized);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p cooklang-sync-server --all-features auth::request::tests`
Expected: 5 tests pass. All are `#[serial(jwt_secret)]` so they run one at a time.

- [ ] **Step 3: Clippy**

Run: `cargo clippy -p cooklang-sync-server --all-features --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add server/src/auth/request.rs
git commit -m "test(server): pin FromRequest<User> happy + 4 unauth paths"
```

---

## Task 4: `chunk_id.rs` — `FromParam`, `file_path`, `is_present`

**Files:**
- Modify: `server/src/chunk_id.rs`

**Why:** `ChunkId` is load-bearing: it validates URL params, bucketises chunk storage under `UPLOAD_DIR`, and gates every `non_local_chunks` decision. Tests must pin:
- `FromParam` accepts only ASCII alphanumerics.
- `file_path()` uses `UPLOAD_DIR` env var (falling back to `./upload`) with two-char bucketing (or `null/` for ids shorter than 2 chars).
- `is_present()` short-circuits to `true` for the empty id (`EMPTY_CHUNK_ID`), otherwise checks file existence.

- [ ] **Step 1: Add inline test module**

Append to `server/src/chunk_id.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // -- FromParam ----------------------------------------------------------

    #[test]
    fn from_param_accepts_ascii_alphanumeric() {
        for ok in ["a", "abc", "ABC123", "0", "9z"] {
            assert!(
                ChunkId::from_param(ok).is_ok(),
                "{ok} should parse as a ChunkId"
            );
        }
    }

    #[test]
    fn from_param_rejects_non_alphanumeric() {
        for bad in ["", "a-b", "a.b", "a/b", "a b", "a_b", "abc!"] {
            assert!(
                ChunkId::from_param(bad).is_err(),
                "{bad:?} should be rejected"
            );
        }
    }

    // -- file_path ----------------------------------------------------------
    //
    // All file_path tests run serial because they mutate UPLOAD_DIR.

    fn set_upload_dir(dir: &TempDir) {
        std::env::set_var("UPLOAD_DIR", dir.path());
    }

    #[test]
    #[serial(upload_dir)]
    fn file_path_buckets_by_first_two_chars() {
        let td = TempDir::new().unwrap();
        set_upload_dir(&td);
        let cid = ChunkId::from("abcdef");
        let expected: PathBuf = td.path().join("a").join("b").join("abcdef");
        assert_eq!(cid.file_path(), expected);
    }

    #[test]
    #[serial(upload_dir)]
    fn file_path_uses_null_bucket_for_single_char_id() {
        let td = TempDir::new().unwrap();
        set_upload_dir(&td);
        let cid = ChunkId::from("a");
        let expected: PathBuf = td.path().join("null").join("a");
        assert_eq!(cid.file_path(), expected);
    }

    #[test]
    #[serial(upload_dir)]
    fn file_path_uses_null_bucket_for_empty_id() {
        let td = TempDir::new().unwrap();
        set_upload_dir(&td);
        let cid = ChunkId::from("");
        let expected: PathBuf = td.path().join("null").join("");
        assert_eq!(cid.file_path(), expected);
    }

    #[test]
    #[serial(upload_dir)]
    fn file_path_defaults_to_relative_upload_when_env_unset() {
        std::env::remove_var("UPLOAD_DIR");
        let cid = ChunkId::from("abcdef");
        assert_eq!(
            cid.file_path(),
            PathBuf::from("./upload").join("a").join("b").join("abcdef")
        );
    }

    // -- is_present --------------------------------------------------------

    #[test]
    #[serial(upload_dir)]
    fn is_present_empty_id_is_always_true() {
        // No UPLOAD_DIR setup required — empty id short-circuits before the
        // filesystem lookup.
        std::env::remove_var("UPLOAD_DIR");
        let cid = ChunkId::from("");
        assert!(cid.is_present());
    }

    #[test]
    #[serial(upload_dir)]
    fn is_present_false_when_file_missing() {
        let td = TempDir::new().unwrap();
        set_upload_dir(&td);
        let cid = ChunkId::from("abcdef");
        assert!(!cid.is_present());
    }

    #[test]
    #[serial(upload_dir)]
    fn is_present_true_when_file_exists_on_disk() {
        let td = TempDir::new().unwrap();
        set_upload_dir(&td);
        let cid = ChunkId::from("abcdef");
        let path = cid.file_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"payload").unwrap();
        assert!(cid.is_present());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p cooklang-sync-server --all-features chunk_id::tests`
Expected: 9 tests pass. Serial-scoped by `upload_dir`.

- [ ] **Step 3: Clippy**

Run: `cargo clippy -p cooklang-sync-server --all-features --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add server/src/chunk_id.rs
git commit -m "test(server): pin ChunkId FromParam, file_path bucketing, is_present"
```

---

## Task 5: `metadata/models.rs` — clippy cleanup + serde round-trip

**Files:**
- Modify: `server/src/metadata/models.rs`

**Why:** The existing `#[cfg(test)] mod tests` has three `assert_eq!(x, true/false)` bool-assert-comparison lints and is missing serde round-trip coverage. The client relies on the JSON shape of `FileRecord`, so we pin it here (`ResponseFileRecord` in Plan 3's `remote_tests.rs` must line up with these field names).

- [ ] **Step 1: Fix the three clippy warnings**

In the existing test module, replace every `assert_eq!(x, false)` with `assert!(!x)` and `assert_eq!(x, true)` with `assert!(x)`. There are exactly three: two `deleted == false` and one `deleted == true`.

After edits the relevant asserts should read:

```rust
assert!(!record.deleted);  // test_new_file_record_construction
assert!(record.deleted);   // test_new_file_record_with_deleted_flag
assert!(!record.deleted);  // test_file_record_construction
```

- [ ] **Step 2: Append serde round-trip tests**

Inside the same `mod tests` block, below the existing tests, append:

```rust
    #[test]
    fn file_record_serde_round_trip_matches_field_layout() {
        let record = FileRecord {
            id: 42,
            user_id: 7,
            chunk_ids: "abc,def".to_string(),
            deleted: false,
            path: "recipes/dinner.cook".to_string(),
        };
        let json = serde_json::to_string(&record).expect("serialize");
        // Client `ResponseFileRecord` (see client/tests/remote_tests.rs) expects
        // exactly this shape — keep the assertion literal so breaking the
        // contract fails here rather than in client integration tests.
        assert_eq!(
            json,
            r#"{"id":42,"user_id":7,"chunk_ids":"abc,def","deleted":false,"path":"recipes/dinner.cook"}"#
        );

        let back: FileRecord = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.id, 42);
        assert_eq!(back.user_id, 7);
        assert_eq!(back.chunk_ids, "abc,def");
        assert!(!back.deleted);
        assert_eq!(back.path, "recipes/dinner.cook");
    }

    #[test]
    fn new_file_record_deserializes_tombstone_payload() {
        // Matches what the server accepts on POST /metadata/commit for a
        // deletion: empty chunk_ids, deleted=true.
        let json = r#"{"user_id":3,"chunk_ids":"","deleted":true,"path":"gone.cook"}"#;
        let back: NewFileRecord = serde_json::from_str(json).expect("deserialize");
        assert_eq!(back.user_id, 3);
        assert_eq!(back.chunk_ids, "");
        assert!(back.deleted);
        assert_eq!(back.path, "gone.cook");
    }
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p cooklang-sync-server --all-features metadata::models::tests`
Expected: 5 tests pass (3 pre-existing + 2 new).

- [ ] **Step 4: Clippy**

Run: `cargo clippy -p cooklang-sync-server --all-features --all-targets -- -D warnings`
Expected: clean; the three bool-assert-comparison warnings are gone.

- [ ] **Step 5: Commit**

```bash
git add server/src/metadata/models.rs
git commit -m "test(server): clippy cleanup + serde round-trip for FileRecord/NewFileRecord"
```

---

## Task 6: `metadata/notification.rs` — Client Eq/Hash + `notify` fan-out

**Files:**
- Modify: `server/src/metadata/notification.rs`

**Why:** The poll route relies on two subtle behaviors: `Client` deduplicates in the `HashSet` by `uuid` only (so reconnecting with the same uuid reuses the existing `Notify`), and `ActiveClients::notify` excludes the caller's uuid so a client never wakes itself on its own commit. Both are easy to regress.

- [ ] **Step 1: Add inline test module**

Append to `server/src/metadata/notification.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn clients_with_same_uuid_are_equal_and_hash_identically() {
        let a = Client::new("same".into());
        let b = Client::new("same".into());
        assert_eq!(a, b);

        let mut set = HashSet::new();
        set.insert(a);
        // Second insert is a no-op: HashSet already contains a Client with
        // this uuid.
        assert!(!set.insert(b));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn clients_with_different_uuids_are_not_equal() {
        let a = Client::new("one".into());
        let b = Client::new("two".into());
        assert_ne!(a, b);
    }

    #[tokio::test]
    async fn notify_wakes_other_clients_and_excludes_caller() {
        let mut active = ActiveClients {
            clients: HashSet::new(),
        };
        let caller = Client::new("caller".into());
        let listener = Client::new("listener".into());
        let caller_notif = Arc::clone(&caller.notification);
        let listener_notif = Arc::clone(&listener.notification);
        active.clients.insert(caller);
        active.clients.insert(listener);

        active.notify("caller".into());

        // Listener wakes up (wait a tiny bit for Notify propagation).
        tokio::time::timeout(Duration::from_millis(250), listener_notif.notified())
            .await
            .expect("listener should have been notified");

        // Caller does NOT wake. We can't "prove a negative" indefinitely, so
        // we give it the same 100ms budget and assert it times out.
        let caller_ready = tokio::time::timeout(
            Duration::from_millis(100),
            caller_notif.notified(),
        )
        .await;
        assert!(
            caller_ready.is_err(),
            "caller should NOT be notified about its own commit"
        );
    }

    #[tokio::test]
    async fn notify_on_empty_active_clients_is_noop() {
        let active = ActiveClients {
            clients: HashSet::new(),
        };
        // Just assert it doesn't panic or hang.
        active.notify("anyone".into());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p cooklang-sync-server --all-features metadata::notification::tests`
Expected: 4 tests pass.

- [ ] **Step 3: Clippy**

Run: `cargo clippy -p cooklang-sync-server --all-features --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add server/src/metadata/notification.rs
git commit -m "test(server): pin Client uuid dedup + ActiveClients::notify caller exclusion"
```

---

## Task 7: `metadata/request.rs` — `non_local_chunks` + `from_payload_and_user_id`

**Files:**
- Modify: `server/src/metadata/request.rs`

**Why:** `non_local_chunks` is the gate that decides whether a commit becomes `Success` or `NeedChunks`. It uses `ChunkId::is_present()` under the hood, which means it also reads `UPLOAD_DIR` — so tests must serialise on that env var. `from_payload_and_user_id` is the mapping that the commit route relies on for tombstones (`chunk_ids=""`, `deleted=true`).

- [ ] **Step 1: Add inline test module**

Append to `server/src/metadata/request.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    fn set_upload_dir(dir: &TempDir) {
        std::env::set_var("UPLOAD_DIR", dir.path());
    }

    fn write_chunk(td: &TempDir, id: &str) {
        let cid = ChunkId::from(id);
        let path = cid.file_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"x").unwrap();
    }

    #[test]
    #[serial(upload_dir)]
    fn non_local_chunks_returns_all_when_none_on_disk() {
        let td = TempDir::new().unwrap();
        set_upload_dir(&td);
        let payload = CommitPayload {
            path: "a.cook",
            deleted: false,
            chunk_ids: "aa,bb,cc",
        };
        let missing: Vec<String> = payload
            .non_local_chunks()
            .iter()
            .map(|c| c.id().to_string())
            .collect();
        assert_eq!(missing, vec!["aa", "bb", "cc"]);
    }

    #[test]
    #[serial(upload_dir)]
    fn non_local_chunks_filters_out_chunks_already_on_disk() {
        let td = TempDir::new().unwrap();
        set_upload_dir(&td);
        write_chunk(&td, "bb");
        let payload = CommitPayload {
            path: "a.cook",
            deleted: false,
            chunk_ids: "aa,bb,cc",
        };
        let missing: Vec<String> = payload
            .non_local_chunks()
            .iter()
            .map(|c| c.id().to_string())
            .collect();
        assert_eq!(missing, vec!["aa", "cc"]);
    }

    #[test]
    #[serial(upload_dir)]
    fn non_local_chunks_treats_empty_id_as_already_present() {
        // An empty id (produced by a tombstone commit's `chunk_ids=""`)
        // short-circuits to is_present==true inside ChunkId.
        let td = TempDir::new().unwrap();
        set_upload_dir(&td);
        let payload = CommitPayload {
            path: "a.cook",
            deleted: true,
            chunk_ids: "",
        };
        assert!(payload.non_local_chunks().is_empty());
    }

    // --- from_payload_and_user_id -----------------------------------------
    //
    // `Form<T>` has no public constructor in Rocket 0.5, so we drive the real
    // form-parse pipeline via a local Rocket route. The route echoes the
    // fields of the resulting `NewFileRecord` as URL-encoded form data so the
    // test can assert on them without needing serde_json here.

    use rocket::http::ContentType;
    use rocket::local::asynchronous::Client;
    use rocket::{post, routes};

    #[post("/build", data = "<payload>")]
    fn build_route(payload: rocket::form::Form<CommitPayload<'_>>) -> String {
        let nr = NewFileRecord::from_payload_and_user_id(payload, 99);
        format!(
            "user_id={}&path={}&deleted={}&chunk_ids={}",
            nr.user_id, nr.path, nr.deleted, nr.chunk_ids
        )
    }

    async fn build_client() -> Client {
        Client::tracked(rocket::build().mount("/", routes![build_route]))
            .await
            .expect("rocket client")
    }

    #[rocket::async_test]
    async fn from_payload_maps_all_fields_verbatim() {
        let client = build_client().await;
        let resp = client
            .post("/build")
            .header(ContentType::Form)
            .body("path=recipes%2Fa.cook&deleted=false&chunk_ids=abc%2Cdef")
            .dispatch()
            .await;
        assert_eq!(
            resp.into_string().await.unwrap(),
            "user_id=99&path=recipes/a.cook&deleted=false&chunk_ids=abc,def"
        );
    }

    #[rocket::async_test]
    async fn from_payload_handles_tombstone_shape() {
        let client = build_client().await;
        let resp = client
            .post("/build")
            .header(ContentType::Form)
            .body("path=deleted.cook&deleted=true&chunk_ids=")
            .dispatch()
            .await;
        assert_eq!(
            resp.into_string().await.unwrap(),
            "user_id=99&path=deleted.cook&deleted=true&chunk_ids="
        );
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p cooklang-sync-server --all-features metadata::request::tests`
Expected: 5 tests pass.

- [ ] **Step 3: Clippy**

Run: `cargo clippy -p cooklang-sync-server --all-features --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add server/src/metadata/request.rs
git commit -m "test(server): pin non_local_chunks filter + from_payload_and_user_id mapping"
```

---

## Task 8: `metadata/response.rs` — `CommitResultStatus` serde shape

**Files:**
- Modify: `server/src/metadata/response.rs`

**Why:** The client's `CommitResultStatus` is deserialised from this exact wire format (see `client/tests/remote_tests.rs`: `{"Success": 42}` and `{"NeedChunks": "abc,def"}`). Serde's default enum representation is externally-tagged; these tests pin it against accidental additions of `#[serde(tag = "...")]` or renames.

- [ ] **Step 1: Add inline test module**

Append to `server/src/metadata/response.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_serialises_as_externally_tagged_object() {
        let status = CommitResultStatus::Success(42);
        assert_eq!(
            serde_json::to_string(&status).expect("serialize"),
            r#"{"Success":42}"#
        );
    }

    #[test]
    fn need_chunks_serialises_as_externally_tagged_object() {
        let status = CommitResultStatus::NeedChunks("abc,def".to_string());
        assert_eq!(
            serde_json::to_string(&status).expect("serialize"),
            r#"{"NeedChunks":"abc,def"}"#
        );
    }

    #[test]
    fn success_round_trips() {
        let json = r#"{"Success":7}"#;
        let status: CommitResultStatus = serde_json::from_str(json).expect("deserialize");
        match status {
            CommitResultStatus::Success(id) => assert_eq!(id, 7),
            other => panic!("expected Success, got {:?}", other),
        }
    }

    #[test]
    fn need_chunks_round_trips() {
        let json = r#"{"NeedChunks":"a,b,c"}"#;
        let status: CommitResultStatus = serde_json::from_str(json).expect("deserialize");
        match status {
            CommitResultStatus::NeedChunks(s) => assert_eq!(s, "a,b,c"),
            other => panic!("expected NeedChunks, got {:?}", other),
        }
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p cooklang-sync-server --all-features metadata::response::tests`
Expected: 4 tests pass.

- [ ] **Step 3: Clippy**

Run: `cargo clippy -p cooklang-sync-server --all-features --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add server/src/metadata/response.rs
git commit -m "test(server): pin CommitResultStatus externally-tagged serde shape"
```

---

## Task 9: `chunks/request.rs` — `RawContentType` FromRequest

**Files:**
- Modify: `server/src/chunks/request.rs`

**Why:** `RawContentType` is the shim that feeds `multer::parse_boundary` inside `upload_chunks`. It reads `Content-Type` directly (bypassing Rocket's own content-type parsing) and defaults to `""` when absent. We pin both branches.

- [ ] **Step 1: Add inline test module**

Append to `server/src/chunks/request.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rocket::http::{ContentType, Status};
    use rocket::local::asynchronous::Client;
    use rocket::{get, routes};

    #[get("/ct-echo")]
    fn echo(ct: RawContentType<'_>) -> String {
        ct.0.to_string()
    }

    async fn client() -> Client {
        let rocket = rocket::build().mount("/", routes![echo]);
        Client::tracked(rocket).await.expect("rocket client")
    }

    #[rocket::async_test]
    async fn raw_content_type_reflects_request_header() {
        let client = client().await;
        let resp = client
            .get("/ct-echo")
            .header(ContentType::new("multipart", "form-data"))
            .dispatch()
            .await;
        assert_eq!(resp.status(), Status::Ok);
        let body = resp.into_string().await.unwrap();
        assert!(
            body.starts_with("multipart/form-data"),
            "got {body:?}"
        );
    }

    #[rocket::async_test]
    async fn raw_content_type_defaults_to_empty_when_absent() {
        let client = client().await;
        let resp = client.get("/ct-echo").dispatch().await;
        assert_eq!(resp.status(), Status::Ok);
        assert_eq!(resp.into_string().await.unwrap(), "");
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p cooklang-sync-server --all-features chunks::request::tests`
Expected: 2 tests pass.

- [ ] **Step 3: Clippy**

Run: `cargo clippy -p cooklang-sync-server --all-features --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add server/src/chunks/request.rs
git commit -m "test(server): pin RawContentType echo + default-empty behavior"
```

---

## Task 10: `metadata/db.rs` — insert / latest_for_path / has_files / list

**Files:**
- Modify: `server/src/metadata/db.rs`

**Why:** These four functions are the metadata engine. Each one has subtle scoping behavior (by `user_id`, by deletion status, by "max id per path" windowing) that regresses easily under schema churn. Tests use an in-memory SQLite connection with the real `sqlite` migrations embedded.

**Note:** Only runs under `--features database_sqlite` (the default). Under `--features database_postgres` the test module is silently empty because `DbConnection` resolves to `PgConnection`, which has no easy in-process equivalent. We gate the whole test module on the sqlite feature so `cargo test -p cooklang-sync-server --features database_postgres` still builds.

- [ ] **Step 1: Add inline test module**

Append to `server/src/metadata/db.rs`:

```rust
#[cfg(all(test, feature = "database_sqlite"))]
mod tests {
    use super::*;
    use diesel::connection::SimpleConnection;
    use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

    const MIGRATIONS: EmbeddedMigrations = embed_migrations!("src/metadata/migrations/sqlite");

    /// Build an in-memory SQLite connection with schema migrations applied.
    fn fresh_conn() -> diesel::SqliteConnection {
        use diesel::Connection;
        let mut conn = diesel::SqliteConnection::establish(":memory:")
            .expect("open in-memory sqlite");
        // Run under a single transaction for speed; diesel migrations handle
        // their own transactions so this is a no-op but harmless.
        conn.batch_execute("PRAGMA foreign_keys = ON;").unwrap();
        conn.run_pending_migrations(MIGRATIONS)
            .expect("run migrations");
        conn
    }

    fn insert(
        conn: &mut diesel::SqliteConnection,
        user_id: i32,
        path: &str,
        chunk_ids: &str,
        deleted: bool,
    ) -> i32 {
        insert_new_record(
            conn,
            NewFileRecord {
                user_id,
                chunk_ids: chunk_ids.to_string(),
                deleted,
                path: path.to_string(),
            },
        )
        .expect("insert")
    }

    // -- insert_new_record -------------------------------------------------

    #[test]
    fn insert_returns_monotonically_increasing_ids() {
        let mut conn = fresh_conn();
        let id1 = insert(&mut conn, 1, "a.cook", "aa", false);
        let id2 = insert(&mut conn, 1, "b.cook", "bb", false);
        assert!(id2 > id1, "ids should increase: {id1} -> {id2}");
    }

    // -- latest_for_path ----------------------------------------------------

    #[test]
    fn latest_for_path_returns_none_for_unknown() {
        let mut conn = fresh_conn();
        let got = latest_for_path(&mut conn, 1, "missing.cook").unwrap();
        assert!(got.is_none());
    }

    #[test]
    fn latest_for_path_returns_most_recent_by_id() {
        let mut conn = fresh_conn();
        let _v1 = insert(&mut conn, 1, "a.cook", "aa", false);
        let _v2 = insert(&mut conn, 1, "a.cook", "bb", false);
        let v3 = insert(&mut conn, 1, "a.cook", "cc", false);
        let got = latest_for_path(&mut conn, 1, "a.cook").unwrap().expect("some");
        assert_eq!(got.id, v3);
        assert_eq!(got.chunk_ids, "cc");
    }

    #[test]
    fn latest_for_path_scopes_by_user_id() {
        let mut conn = fresh_conn();
        let _user_a = insert(&mut conn, 1, "shared.cook", "aa", false);
        let user_b = insert(&mut conn, 2, "shared.cook", "bb", false);
        let got_b = latest_for_path(&mut conn, 2, "shared.cook")
            .unwrap()
            .expect("some");
        assert_eq!(got_b.id, user_b);
        assert_eq!(got_b.chunk_ids, "bb");
    }

    // -- has_files ----------------------------------------------------------

    #[test]
    fn has_files_false_on_empty_db() {
        let mut conn = fresh_conn();
        assert!(!has_files(&mut conn, 1).unwrap());
    }

    #[test]
    fn has_files_true_when_user_has_live_record() {
        let mut conn = fresh_conn();
        insert(&mut conn, 1, "a.cook", "aa", false);
        assert!(has_files(&mut conn, 1).unwrap());
    }

    #[test]
    fn has_files_false_when_latest_record_for_every_path_is_tombstone() {
        let mut conn = fresh_conn();
        insert(&mut conn, 1, "a.cook", "aa", false);
        insert(&mut conn, 1, "a.cook", "", true); // tombstone for a.cook
        assert!(!has_files(&mut conn, 1).unwrap());
    }

    #[test]
    fn has_files_ignores_other_users_records() {
        let mut conn = fresh_conn();
        insert(&mut conn, 1, "a.cook", "aa", false);
        assert!(!has_files(&mut conn, 2).unwrap());
    }

    // -- list ---------------------------------------------------------------

    #[test]
    fn list_returns_empty_when_no_records_exist() {
        let mut conn = fresh_conn();
        assert!(list(&mut conn, 1, 0).unwrap().is_empty());
    }

    #[test]
    fn list_returns_only_latest_per_path() {
        let mut conn = fresh_conn();
        let _v1 = insert(&mut conn, 1, "a.cook", "aa", false);
        let _v2 = insert(&mut conn, 1, "a.cook", "bb", false);
        let v3_a = insert(&mut conn, 1, "a.cook", "cc", false);
        let v1_b = insert(&mut conn, 1, "b.cook", "dd", false);

        let rows = list(&mut conn, 1, 0).unwrap();
        let mut ids: Vec<i32> = rows.iter().map(|r| r.id).collect();
        ids.sort();
        assert_eq!(ids, vec![v1_b, v3_a]);
    }

    #[test]
    fn list_filters_by_jid_strictly_greater_than() {
        let mut conn = fresh_conn();
        let _v1 = insert(&mut conn, 1, "a.cook", "aa", false);
        let v2 = insert(&mut conn, 1, "b.cook", "bb", false);

        // jid = v2 should return nothing (id > jid is strict).
        let rows = list(&mut conn, 1, v2).unwrap();
        assert!(rows.is_empty(), "got {:?}", rows);
    }

    #[test]
    fn list_scopes_by_user_id() {
        let mut conn = fresh_conn();
        let _user_a = insert(&mut conn, 1, "shared.cook", "aa", false);
        let _user_b = insert(&mut conn, 2, "shared.cook", "bb", false);

        let rows_a = list(&mut conn, 1, 0).unwrap();
        assert_eq!(rows_a.len(), 1);
        assert_eq!(rows_a[0].user_id, 1);
        assert_eq!(rows_a[0].chunk_ids, "aa");

        let rows_b = list(&mut conn, 2, 0).unwrap();
        assert_eq!(rows_b.len(), 1);
        assert_eq!(rows_b[0].user_id, 2);
    }

    #[test]
    fn list_includes_tombstones_in_latest_window() {
        // A tombstone is the latest record for that path — `list` must
        // still surface it, so downstream clients delete their local copy.
        let mut conn = fresh_conn();
        insert(&mut conn, 1, "a.cook", "aa", false);
        let ts_id = insert(&mut conn, 1, "a.cook", "", true);
        let rows = list(&mut conn, 1, 0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, ts_id);
        assert!(rows[0].deleted);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p cooklang-sync-server --all-features metadata::db::tests`
Expected: 13 tests pass.

- [ ] **Step 3: Postgres-feature build sanity**

Run: `cargo build -p cooklang-sync-server --no-default-features --features database_postgres`
Expected: builds. The db-test module is gated off and absent.

> If the postgres build fails for unrelated reasons (e.g. missing libpq), skip this step — the `#[cfg(all(test, feature = "database_sqlite"))]` gate still guarantees the Postgres feature doesn't try to compile these tests.

- [ ] **Step 4: Clippy**

Run: `cargo clippy -p cooklang-sync-server --all-features --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add server/src/metadata/db.rs
git commit -m "test(server): pin db::insert/latest_for_path/has_files/list semantics"
```

---

## Task 11: Final suite + clippy + prod-diff gate

**Files:** (no code edits — verification only)

- [ ] **Step 1: Full workspace test run**

Run: `cargo test --workspace --all-features -- --nocapture`
Expected: every test passes. Serial-marked tests should not race.

- [ ] **Step 2: Clippy gate**

Run: `cargo clippy --workspace --all-features --all-targets -- -D warnings`
Expected: clean. No new warnings anywhere.

- [ ] **Step 3: Prod-diff gate**

Run: `git diff main..HEAD -- 'server/src/**/*.rs'`

Review the diff by eye. Every hunk in `server/src/` must be additions of (or, for Task 5, edits inside) a `#[cfg(test)] mod tests` block. If any production code — anything outside a `#[cfg(test)]` block, anything above the `#[cfg(test)]` boundary — changed, back it out. The scope gate forbids prod-side edits.

> **Allowed exception:** cleanups inside existing `#[cfg(test)] mod tests` blocks (e.g. the three `assert_eq!(x, true/false)` fixes in `metadata/models.rs` from Task 5) are test-code changes, not prod.

- [ ] **Step 4: File-list sanity**

Run: `git diff main..HEAD --name-only`
Expected list (exact, in any order):
```
Cargo.lock
docs/superpowers/plans/2026-04-15-test-coverage-plan-4-server-units.md
server/Cargo.toml
server/src/auth/request.rs
server/src/auth/token.rs
server/src/chunk_id.rs
server/src/chunks/request.rs
server/src/metadata/db.rs
server/src/metadata/models.rs
server/src/metadata/notification.rs
server/src/metadata/request.rs
server/src/metadata/response.rs
```

No `client/**` paths. No `server/tests/**` paths. No server prod files outside the list above.

- [ ] **Step 5: Coverage sanity (optional, informational)**

If `cargo-llvm-cov` is available, run: `cargo llvm-cov -p cooklang-sync-server --all-features --summary-only`
Note the server crate's new line-coverage baseline in the PR description. Not a gate, just a data point to compare against Plan 5's route tests later.

---

## PR template

Title: `test(server): unit tests for auth/chunk_id/metadata modules (Plan 4 of 6)`

Body:

```
Plan 4 of 6. Stacked on #17 (Plan 3).

Adds inline `#[cfg(test)] mod tests` coverage for every server module whose logic
is self-contained:

- auth/token.rs         — decode_token happy / expired / wrong secret / malformed
- auth/request.rs       — FromRequest<User> valid + 4 unauth paths (Rocket local client)
- chunk_id.rs           — FromParam validation, file_path bucketing (incl. null bucket),
                          is_present empty-id short-circuit + fs existence
- metadata/db.rs        — insert / latest_for_path / has_files / list semantics on an
                          in-memory SQLite with the real sqlite migrations embedded
- metadata/models.rs    — serde round-trip pinning the wire shape of FileRecord /
                          NewFileRecord that the client's `ResponseFileRecord` relies on
                          (+ three bool-assert-comparison clippy fixes in pre-existing tests)
- metadata/notification.rs — Client uuid dedup in the HashSet, ActiveClients::notify
                             wakes other clients but excludes the caller
- metadata/request.rs   — non_local_chunks filter against on-disk state,
                          from_payload_and_user_id mapping incl. tombstone shape
- metadata/response.rs  — CommitResultStatus externally-tagged serde ({"Success":N},
                          {"NeedChunks":"a,b"}) matching the client's decoder
- chunks/request.rs     — RawContentType echoes request header + defaults to "" when
                          the header is absent

All internal server state is `pub(crate)`, so tests live inline in their modules rather
than in `server/tests/`. Two shared-state surfaces (`UPLOAD_DIR`, `JWT_SECRET`) are
serialised via `serial_test`.

Dev-deps added: `tempfile`, `serial_test`, `serde_json`. No prod dep versions change.
No `server/src/**` prod code changes. No `client/**` changes.

Route-level tests (rocket local client hitting /metadata, /chunks) come in Plan 5.
End-to-end tests come in Plan 6.
```

Base branch: `test-coverage-plan-3`
Head branch: `test-coverage-plan-4`
