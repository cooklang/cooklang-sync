// @generated automatically by Diesel CLI.

diesel::table! {
    file_records (id) {
        id -> Integer,
        deleted -> Bool,
        path -> Text,
        chunk_ids -> Text,
        format -> Text,
    }
}
