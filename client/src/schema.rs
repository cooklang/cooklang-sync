// @generated automatically by Diesel CLI.

diesel::table! {
    file_records (id) {
        id -> Integer,
        jid -> Nullable<Integer>,
        deleted -> Bool,
        path -> Text,
        modified_at -> TimestamptzSqlite,
        size -> BigInt,
    }
}
