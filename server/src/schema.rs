// @generated automatically by Diesel CLI.

diesel::table! {
    file_records (id) {
        id -> Integer,
        deleted -> Bool,
        path -> Text,
        format -> Text,
    }
}
