// @generated automatically by Diesel CLI.

diesel::table! {
    file_records (id) {
        id -> Integer,
        jid -> Nullable<Integer>,
        deleted -> Bool,
        path -> Text,
        modified_at -> TimestamptzSqlite,
        size -> BigInt,
        namespace_id -> Integer,
    }
}

diesel::table! {
    sync_state (namespace_id) {
        namespace_id -> Integer,
        download_watermark -> Integer,
    }
}

diesel::allow_tables_to_appear_in_same_query!(file_records, sync_state,);
