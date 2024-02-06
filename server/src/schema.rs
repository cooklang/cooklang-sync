// @generated automatically by Diesel CLI.

diesel::table! {
    file_records (id) {
        id -> Integer,
        path -> Text,
        deleted -> Bool,
        chunk_ids -> Text,
        format -> Text,
    }
}
